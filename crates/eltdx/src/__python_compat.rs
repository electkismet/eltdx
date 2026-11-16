//! Unstable bridge used only by the separately versioned Python binding.
//! No Python runtime, object or exception type crosses this boundary.

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eltdx_protocol::commands::security::SecurityListRequest;
use eltdx_protocol::unit::NormalizedCode;
use eltdx_protocol::{CommandRequest, CommandResponse};
use eltdx_runtime::engine::{Engine, PinHandle, PushConsumer};
use eltdx_runtime::execution::RawReply;
use eltdx_runtime::push::PushFrame;
use eltdx_runtime::RuntimeError;

use crate::error::{Error, ErrorKind, Result};
use crate::hq::executor::HqExecutor;
use crate::operation::{ChildPurpose, OperationContext, RecoveryLimit, WorkLimits};
use crate::services::{legacy_directory_pages, legacy_snapshot_rows};
use crate::session::SessionOwner;
use crate::types::HqMarket;
use crate::{CancellationToken, RawPolicy};

pub use crate::services::code_selection::CodeSelection as LegacyCodeSelection;
pub use crate::services::money_flow_probe::{legacy_money_flow_recover, LegacyMoneyFlowSources};
pub use crate::adjustment::factors::legacy_adjustment_factors;
pub use crate::calendar::index::{calendar_window, calendar_neighbor, legacy_weekday,
    calendar_days, calendar_contains, legacy_weekday_range, legacy_weekday_neighbor};
pub use crate::services::resource_legacy::legacy_resource_download;
pub use crate::stats::{legacy_stats_members, LegacyStatRow, LegacyStat2Row};
pub use crate::services::limits::legacy_special_scan;
pub use crate::services::corporate_legacy::{legacy_capital_collect, legacy_finance_recover,
    LegacyCapitalCollection, LegacyCapitalPage, LegacyCapitalSource,
    LegacyFinanceCollection, LegacyFinanceError, LegacyFinancePage, LegacyFinanceSource};

#[derive(Clone, Copy, Debug)]
pub enum LegacyChildPurpose { Required, Metadata, Page, MissingRecovery, EmptyProbe, Fallback, Renewal }

impl From<LegacyChildPurpose> for ChildPurpose {
    fn from(purpose: LegacyChildPurpose) -> Self {
        match purpose {
            LegacyChildPurpose::Required => Self::Required,
            LegacyChildPurpose::Metadata => Self::Metadata,
            LegacyChildPurpose::Page => Self::Page,
            LegacyChildPurpose::MissingRecovery => Self::MissingRecovery,
            LegacyChildPurpose::EmptyProbe => Self::EmptyProbe,
            LegacyChildPurpose::Fallback => Self::Fallback,
            LegacyChildPurpose::Renewal => Self::Renewal,
        }
    }
}

/// Domain recovery counts preserve their old scope and error projection.
#[derive(Clone, Copy, Debug)]
pub struct LegacyRecoveryLimit { pub name: &'static str, pub maximum: usize }

#[derive(Clone, Debug)]
pub struct LegacyExecutor {
    executor: HqExecutor,
    engine: Engine,
    request_timeout: Duration,
    cleanup_timeout: Duration,
}

impl LegacyExecutor {
    /// Creates a separate legacy generation. Ordinary clients cannot be converted.
    /// Engine construction/registration is lazy and starts no network resources here.
    /// Bindings create separate executors for legacy close domains and close each at
    /// its original stage, without sealing still-open later domains.
    pub fn new(
        engine: Engine, request_timeout: Duration, close_timeout: Duration, cleanup_timeout: Duration,
    ) -> Result<Self> {
        if request_timeout.is_zero() || close_timeout.is_zero() || cleanup_timeout.is_zero() {
            return Err(Error::new(ErrorKind::InvalidArgument, "legacy timeouts must be positive"));
        }
        let owner = Arc::new(SessionOwner::new(close_timeout, cleanup_timeout));
        let executor = HqExecutor::new(owner);
        executor.owner().register_engine_lazy(engine.clone())?;
        Ok(Self { executor, engine, request_timeout, cleanup_timeout })
    }

    pub fn begin_operation(
        &self, name: &'static str, raw: RawPolicy, token: Option<CancellationToken>,
    ) -> Result<LegacyOperation> {
        let entry = Instant::now();
        let context = OperationContext::python_legacy(entry, name, Arc::clone(self.executor.owner()),
            self.request_timeout, self.cleanup_timeout, token, raw)?;
        Ok(LegacyOperation { executor: self.clone(), context })
    }

    pub fn close(&self) -> Result<()> { self.executor.close() }

    /// Select the exclusive legacy raw mode for this Engine. Python owns a
    /// separate Engine generation; it cannot share a Rust subscription's
    /// dispatcher. Every native poll/drain uses this one claim. Construction
    /// starts no worker, buffers no frames and performs no network I/O.
    pub fn claim_raw_push_reader(&self) -> std::result::Result<LegacyPushReader, RuntimeError> {
        Ok(LegacyPushReader { consumer: self.engine.claim_push_consumer()? })
    }
}

/// Unstable raw compatibility boundary. Retain RuntimeError unchanged so the
/// existing Python projection preserves PushOverflow's cumulative count,
/// message and context. Parsing and callbacks remain on the calling thread.
#[derive(Debug)]
pub struct LegacyPushReader { consumer: PushConsumer }

impl LegacyPushReader {
    pub fn poll_push(&self, timeout: Duration) -> std::result::Result<Option<PushFrame>, RuntimeError> {
        self.consumer.poll_push(timeout)
    }
    pub fn drain_pushes(&self) -> std::result::Result<Vec<PushFrame>, RuntimeError> {
        self.consumer.drain_pushes()
    }
}

#[derive(Debug)]
pub struct LegacyOperation { executor: LegacyExecutor, context: OperationContext }

#[derive(Clone, Debug)]
pub struct LegacySecurityPage {
    pub rows: Vec<eltdx_protocol::commands::security::SecurityCode>,
    pub payload: bytes::Bytes,
    pub wire: Option<bytes::Bytes>,
    pub endpoint: String,
    pub command: u16,
}

#[derive(Clone, Debug)]
pub enum LegacySnapshotState {
    Present {
        row: eltdx_protocol::commands::quotes::QuoteSnapshot,
        payload: bytes::Bytes,
        wire: Option<bytes::Bytes>,
        endpoint: String,
        command: u16,
    },
    Missing,
    Unmatched,
}

#[derive(Clone, Debug)]
pub struct LegacySnapshotItem {
    pub input_index: usize,
    pub requested: NormalizedCode,
    pub state: LegacySnapshotState,
}

#[derive(Clone, Debug)]
pub struct LegacySnapshotBatch {
    pub items: Vec<LegacySnapshotItem>,
    pub recovered: bool,
}

/// The old facade validated bar_time only after receiving the first page.
/// Store the binding's exact Python repr message without raising it early.
#[derive(Clone, Debug)]
pub enum LegacyBarTime { Start, End, DeferredInvalid(String) }

#[derive(Clone, Debug)]
pub struct LegacyBarSeries {
    pub response: CommandResponse,
    // An aggregate count is not a legal per-page request count (maximum800).
    // Keep it separate from the original validated first request metadata.
    pub request_count: usize,
}

#[derive(Clone, Debug)]
pub struct LegacyTradeRead {
    pub response: CommandResponse,
    pub request_count: usize,
    pub page_lengths: Vec<usize>,
    pub opening: Option<eltdx_protocol::commands::trades::TradeTick>,
    pub opening_only: bool,
}

impl LegacyOperation {
    /// `None` retains the legacy unlimited-page option; there is no fabricated cap.
    pub fn with_page_limit(mut self, maximum: Option<NonZeroUsize>) -> Result<Self> {
        self.context.set_work_limits(WorkLimits { pages: maximum, ..WorkLimits::default() })?;
        Ok(self)
    }

    pub fn execute_raw(
        &self, request: CommandRequest, purpose: LegacyChildPurpose, recovery: Option<LegacyRecoveryLimit>,
    ) -> Result<RawReply> {
        self.executor.executor.execute_raw(&self.executor.engine, &self.context, request, purpose.into(),
            recovery.map(|limit| RecoveryLimit { name: limit.name, maximum: limit.maximum }))
    }

    pub fn execute_pinned_raw(
        &self, pin: &PinHandle, request: CommandRequest, purpose: LegacyChildPurpose,
        recovery: Option<LegacyRecoveryLimit>,
    ) -> Result<RawReply> {
        self.executor.executor.execute_pinned_raw(&self.executor.engine, pin, &self.context, request,
            purpose.into(), recovery.map(|limit| RecoveryLimit { name: limit.name, maximum: limit.maximum }))
    }

    /// Freezes the caller-selected legacy precision markets in their historical
    /// request order, without applying Rust's source-work cap or inference rules.
    pub fn prepare_hq_metadata(&self, namespace: &str, markets: &[HqMarket]) -> Result<()> {
        self.executor.executor.prepare_legacy_metadata(
            &self.executor.engine, &self.context, namespace, markets,
            self.executor.request_timeout, self.executor.cleanup_timeout,
        )?;
        Ok(())
    }

    /// Runs the same SDK directory paginator with Python's unlimited legacy
    /// page policy and the caller's one continuous operation context.
    pub fn security_pages(
        &self,
        namespace: &str,
        market: HqMarket,
        page_size: usize,
    ) -> Result<Vec<LegacySecurityPage>> {
        let replies = legacy_directory_pages(&self.context, market, page_size, |offset| {
            let limit = u16::try_from(page_size).map_err(|error| Error::new(ErrorKind::InvalidArgument,
                "legacy security page size is outside the protocol range").with_source(error))?;
            let request = CommandRequest::SecurityList(SecurityListRequest::new(protocol_market(market), offset, limit)
                .map_err(Error::from_protocol)?);
            let purpose = if offset == 0 { ChildPurpose::Required } else { ChildPurpose::Page };
            let raw = self.executor.executor.execute_raw(&self.executor.engine, &self.context, request, purpose, None)?;
            self.executor.executor.decode_unpriced(&self.context, raw)
        })?;
        // Publish only a complete directory. Metadata is opportunistic here;
        // quote preparation can use the Rust source loader if publication fails.
        // The Python facade still receives each page and runs its metadata sink.
        let _ = crate::services::publish_legacy_directory_metadata(
            &self.context,
            namespace,
            market,
            &replies,
        );
        let mut pages = Vec::new();
        pages.try_reserve(replies.len()).map_err(|error| Error::new(ErrorKind::LimitExceeded,
            "unable to reserve legacy security pages").with_source(error))?;
        for reply in replies {
            let CommandResponse::SecurityList(rows) = reply.response else {
                return Err(Error::new(ErrorKind::Protocol, "legacy directory response has the wrong type"));
            };
            pages.push(LegacySecurityPage { rows, payload: reply.payload, wire: reply.wire.map(|frame| frame.raw),
                endpoint: reply.source.host, command: reply.request.command_code() });
        }
        Ok(pages)
    }

    /// Uses the SDK's shared order/recovery path and retains original protocol
    /// scalars, response page payload and record provenance for the tuple ABI.
    pub fn recover_snapshots(&self, codes: &[NormalizedCode]) -> Result<LegacySnapshotBatch> {
        let batch = legacy_snapshot_rows(&self.context, codes, |part, purpose, recovery| {
            let request = CommandRequest::Snapshots(eltdx_protocol::commands::quotes::SnapshotsRequest::new(part.to_vec())
                .map_err(Error::from_protocol)?);
            let raw = self.executor.executor.execute_raw(&self.executor.engine, &self.context, request, purpose, recovery)?;
            self.executor.executor.decode_received(&self.context, raw)
        })?;
        let mut slots: Vec<Option<LegacySnapshotState>> = Vec::new();
        slots.try_reserve(codes.len()).map_err(|error| Error::new(ErrorKind::LimitExceeded,
            "unable to reserve legacy snapshot occurrence slots").with_source(error))?;
        slots.resize_with(codes.len(), || None);
        for record in batch.records {
            slots[record.input_index] = Some(LegacySnapshotState::Present {
                row: record.row, payload: record.payload, wire: record.wire,
                endpoint: record.endpoint, command: record.command,
            });
        }
        for (index, _) in batch.missing { slots[index] = Some(LegacySnapshotState::Missing); }
        for index in batch.unmatched { slots[index] = Some(LegacySnapshotState::Unmatched); }
        let mut items = Vec::new();
        items.try_reserve(codes.len()).map_err(|error| Error::new(ErrorKind::LimitExceeded,
            "unable to reserve legacy snapshot results").with_source(error))?;
        for (input_index, (requested, state)) in codes.iter().zip(slots).enumerate() {
            items.push(LegacySnapshotItem { input_index, requested: requested.clone(),
                state: state.ok_or_else(|| Error::new(ErrorKind::Protocol, "legacy snapshot occurrence was not classified"))? });
        }
        Ok(LegacySnapshotBatch { items, recovered: batch.recovered })
    }

    /// One unpinned wire request at a time, as in the original Python facade.
    /// No directory prefetch or Rust-only collection limits are added here.
    pub fn bars(
        &self, request: CommandRequest, collect: bool,
        maximum_pages: Option<usize>, bar_time: LegacyBarTime,
    ) -> Result<LegacyBarSeries> {
        crate::services::legacy_bar_series(&self.context, request, collect, maximum_pages,
            bar_time, |request, purpose| {
                let raw = self.executor.executor.execute_raw(&self.executor.engine,
                    &self.context, request, purpose, None)?;
                self.executor.executor.decode_unpriced(&self.context, raw)
            })
    }

    /// Single minute-domain query with the original protocol DTO projection.
    /// Caller explicitly prepares legacy metadata for precision-based commands.
    pub fn minutes(&self, request: CommandRequest) -> Result<CommandResponse> {
        let (priced, _) = crate::services::minute_command(&request)?;
        crate::services::legacy_minute_reply(&self.context, request, |request, purpose| {
            let raw = self.executor.executor.execute_raw(&self.executor.engine,
                &self.context, request, purpose, None)?;
            if priced { self.executor.executor.decode_received(&self.context, raw) }
            else { self.executor.executor.decode_unpriced(&self.context, raw) }
        })
    }

    pub fn directory(&self, request: CommandRequest) -> Result<CommandResponse> {
        crate::services::legacy_directory_reply(&self.context, request, |request| {
            let raw = self.executor.executor.execute_raw(&self.executor.engine, &self.context,
                request, ChildPurpose::Required, None)?;
            self.executor.executor.decode_unpriced(&self.context, raw)
        })
    }

    pub fn trades(&self, request: CommandRequest) -> Result<CommandResponse> {
        crate::services::legacy_trade_reply(&self.context, request, |request, purpose| {
            let raw = self.executor.executor.execute_raw(&self.executor.engine, &self.context,
                request, purpose, None)?;
            // These wire prices have intrinsic units, independent of catalogs.
            self.executor.executor.decode_unpriced(&self.context, raw)
        })
    }

    pub fn auction(&self, request: CommandRequest) -> Result<CommandResponse> {
        crate::services::legacy_auction_reply(&self.context, request, |request| {
            let raw = self.executor.executor.execute_raw(&self.executor.engine, &self.context,
                request, ChildPurpose::Required, None)?;
            self.executor.executor.decode_unpriced(&self.context, raw)
        })
    }

    pub fn money_flow(&self, request: CommandRequest) -> Result<CommandResponse> {
        crate::services::legacy_money_flow_reply(&self.context, request, |request| {
            let raw = self.executor.executor.execute_raw(&self.executor.engine, &self.context,
                request, ChildPurpose::Required, None)?;
            self.executor.executor.decode_unpriced(&self.context, raw)
        })
    }

    pub fn corporate(&self, request: CommandRequest) -> Result<CommandResponse> {
        crate::services::legacy_corporate_reply(&self.context, request, |request| {
            let raw = self.executor.executor.execute_raw(&self.executor.engine, &self.context,
                request, ChildPurpose::Required, None)?;
            self.executor.executor.decode_unpriced(&self.context, raw)
        })
    }

    pub fn resource(&self, request: CommandRequest, pin: Option<&PinHandle>) -> Result<CommandResponse> {
        crate::services::legacy_resource_reply(&self.context, request, |request| {
            let raw = match pin {
                Some(pin) => self.executor.executor.execute_pinned_raw(&self.executor.engine, pin,
                    &self.context, request, ChildPurpose::Required, None)?,
                None => self.executor.executor.execute_raw(&self.executor.engine, &self.context,
                    request, ChildPurpose::Required, None)?,
            };
            self.executor.executor.decode_unpriced(&self.context, raw)
        })
    }

    pub fn special_limits(&self, request: CommandRequest) -> Result<CommandResponse> {
        crate::services::legacy_limit_reply(&self.context, request, |request| {
            let raw = self.executor.executor.execute_raw(&self.executor.engine, &self.context,
                request, ChildPurpose::Required, None)?;
            self.executor.executor.decode_unpriced(&self.context, raw)
        })
    }

    pub fn trade_read(&self, request: CommandRequest, collect: bool, opening: bool,
        maximum_pages: Option<usize>) -> Result<LegacyTradeRead> {
        crate::services::legacy_trade_read(&self.context, request, collect, opening, maximum_pages,
            |request, purpose| {
                let raw = self.executor.executor.execute_raw(&self.executor.engine, &self.context,
                    request, purpose, None)?;
                self.executor.executor.decode_unpriced(&self.context, raw)
            })
    }

    /// No metadata is implicitly loaded here. The binding either prepared the
    /// original resolver scope or deliberately uses the old global registry
    /// (pinned refresh and CodeApi.latest_suspended historically did the latter).
    pub fn quotes(&self, request: CommandRequest, pin: Option<&PinHandle>) -> Result<CommandResponse> {
        crate::services::legacy_quote_reply(&self.context, request, |request, purpose| {
            let raw = match pin {
                Some(pin) => self.executor.executor.execute_pinned_raw(&self.executor.engine, pin,
                    &self.context, request, purpose, None)?,
                None => self.executor.executor.execute_raw(&self.executor.engine, &self.context, request, purpose, None)?,
            };
            self.executor.executor.decode_received(&self.context, raw)
        })
    }

    pub fn suspended(&self, request: CommandRequest) -> Result<Vec<String>> {
        let CommandResponse::LegacyQuotes(rows) = self.quotes(request, None)? else {
            return Err(Error::new(ErrorKind::Protocol, "suspended selection requires legacy quotes"));
        };
        let mut selected = Vec::new();
        for (index, row) in rows.into_iter().enumerate() {
            if index % 256 == 0 { self.context.check_boundary(crate::error::TimeoutPhase::Aggregate)?; }
            if crate::services::suspended_status(row.trading_status_raw) {
                selected.push(format!("{}{}", row.market.as_str(), row.code));
            }
        }
        Ok(selected)
    }

    pub fn session(&self, request: CommandRequest) -> Result<CommandResponse> {
        crate::services::legacy_session_reply(&self.context, request, |request| {
            let raw = self.executor.executor.execute_raw(&self.executor.engine, &self.context,
                request, ChildPurpose::Required, None)?;
            self.executor.executor.decode_unpriced(&self.context, raw)
        })
    }

    pub fn company_info(&self, request: CommandRequest) -> Result<CommandResponse> {
        crate::services::legacy_company_reply(&self.context, request, |request| {
            let raw = self.executor.executor.execute_raw(&self.executor.engine, &self.context,
                request, ChildPurpose::Required, None)?;
            let business = raw.response.data.clone();
            Ok((self.executor.executor.decode_unpriced(&self.context, raw)?, business))
        })
    }

    pub fn remember_recoverable(&self, error: Error) { self.context.remember_recoverable(error); }
    pub fn finish(self) -> Result<()> { self.context.finish() }
}

fn protocol_market(market: HqMarket) -> eltdx_protocol::unit::Market {
    match market {
        HqMarket::Shenzhen => eltdx_protocol::unit::Market::Shenzhen,
        HqMarket::Shanghai => eltdx_protocol::unit::Market::Shanghai,
        HqMarket::Beijing => eltdx_protocol::unit::Market::Beijing,
    }
}
