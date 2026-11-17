use std::collections::HashMap;

use eltdx_protocol::context::PricePrecisionSource;
use eltdx_protocol::unit::{Market, NormalizedCode};

use crate::error::{Error, ErrorKind, Result};
use crate::metadata::{MetadataRecord, PrecisionMetadata};
use crate::types::{HqMarket, SecurityId};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ProtocolFamily {
    Hq,
    Ex,
    Mac,
    F10,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum MarketScope {
    Hq(HqMarket),
    Extension(String),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct SnapshotKey {
    family: ProtocolFamily,
    namespace: String,
    market: MarketScope,
}

impl SnapshotKey {
    pub(crate) fn hq(namespace: impl Into<String>, market: HqMarket) -> Self {
        Self {
            family: ProtocolFamily::Hq,
            namespace: namespace.into(),
            market: MarketScope::Hq(market),
        }
    }

    pub(crate) fn new(
        family: ProtocolFamily,
        namespace: impl Into<String>,
        market: MarketScope,
    ) -> Self {
        Self {
            family,
            namespace: namespace.into(),
            market,
        }
    }

    pub(crate) const fn family(&self) -> ProtocolFamily {
        self.family
    }
    pub(crate) fn namespace(&self) -> &str {
        &self.namespace
    }
    pub(crate) fn market(&self) -> &MarketScope {
        &self.market
    }
}

/// Immutable owner-scoped metadata captured once by an operation.
#[derive(Clone, Debug)]
pub(crate) struct MetadataSnapshot {
    key: SnapshotKey,
    version: u64,
    fetched_at_unix_seconds: i64,
    expires_at_unix_seconds: Option<i64>,
    entries: HashMap<SecurityId, MetadataRecord>,
    markets: Vec<HqMarket>,
}

impl MetadataSnapshot {
    pub(crate) fn empty(namespace: impl Into<String>) -> Self {
        Self::new(
            SnapshotKey::new(
                ProtocolFamily::Hq,
                namespace,
                MarketScope::Extension("operation".into()),
            ),
            0,
            0,
            None,
            HashMap::new(),
        )
    }

    pub(super) fn capture(
        namespace: &str,
        version: u64,
        snapshots: &[std::sync::Arc<Self>],
    ) -> Result<Self> {
        let mut capture = Self::empty(namespace);
        capture.version = version;
        capture.fetched_at_unix_seconds = snapshots
            .iter()
            .map(|snapshot| snapshot.fetched_at_unix_seconds)
            .min()
            .unwrap_or(0);
        capture.expires_at_unix_seconds = snapshots
            .iter()
            .filter_map(|snapshot| snapshot.expires_at_unix_seconds)
            .min();
        for snapshot in snapshots {
            if snapshot.key.family != ProtocolFamily::Hq || snapshot.key.namespace != namespace {
                return Err(Error::new(
                    ErrorKind::InvalidArgument,
                    "operation metadata belongs to a different source",
                ));
            }
            for &market in &snapshot.markets {
                if !capture.markets.contains(&market) {
                    capture.markets.push(market);
                }
            }
            for (id, record) in &snapshot.entries {
                if let Some(previous) = capture.entries.insert(id.clone(), record.clone()) {
                    if previous != *record {
                        return Err(Error::new(
                            ErrorKind::MetadataUnavailable,
                            "operation metadata has conflicting identities",
                        ));
                    }
                }
            }
        }
        Ok(capture)
    }

    pub(crate) fn new(
        key: SnapshotKey,
        version: u64,
        fetched_at_unix_seconds: i64,
        expires_at_unix_seconds: Option<i64>,
        entries: HashMap<SecurityId, MetadataRecord>,
    ) -> Self {
        let markets = match &key.market {
            MarketScope::Hq(market) => vec![*market],
            _ => Vec::new(),
        };
        Self {
            key,
            version,
            fetched_at_unix_seconds,
            expires_at_unix_seconds,
            entries,
            markets,
        }
    }

    pub(crate) const fn version(&self) -> u64 {
        self.version
    }
    pub(crate) fn key(&self) -> &SnapshotKey {
        &self.key
    }
    pub(crate) fn covers_market(&self, market: HqMarket) -> bool {
        self.markets.contains(&market)
    }
    pub(crate) const fn fetched_at_unix_seconds(&self) -> i64 {
        self.fetched_at_unix_seconds
    }
    pub(crate) const fn expires_at_unix_seconds(&self) -> Option<i64> {
        self.expires_at_unix_seconds
    }
    pub(crate) fn is_fresh_at(&self, now_unix_seconds: i64) -> bool {
        self.expires_at_unix_seconds
            .map_or(true, |expires| now_unix_seconds < expires)
    }

    pub(crate) fn record(&self, id: &SecurityId) -> Option<&MetadataRecord> {
        self.entries.get(id)
    }

    pub(crate) fn precision_for(&self, id: &SecurityId) -> Option<PrecisionMetadata> {
        self.entries.get(id)?.price_precision()
    }

    pub(crate) fn require_precision(&self, id: &SecurityId) -> Result<PrecisionMetadata> {
        self.precision_for(id).ok_or_else(|| {
            Error::new(
                ErrorKind::MetadataUnavailable,
                "price precision is unavailable",
            )
            .with_context("security", format!("{id}"))
            .with_context("metadata_version", format!("{}", self.version))
        })
    }
}

impl PricePrecisionSource for MetadataSnapshot {
    fn security_decimal(&self, code: &NormalizedCode) -> Option<u8> {
        if self.key.family != ProtocolFamily::Hq {
            return None;
        }
        let code_market = match code.market() {
            Market::Shenzhen => HqMarket::Shenzhen,
            Market::Shanghai => HqMarket::Shanghai,
            Market::Beijing => HqMarket::Beijing,
        };
        let id = SecurityId::new(code_market, code.number()).ok()?;
        self.precision_for(&id).map(PrecisionMetadata::decimals)
    }
}
