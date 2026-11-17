use crate::types::{AmountUnit, HqMarket, QuantityUnit, SecurityId, ValueProvenance};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PrecisionMetadata {
    decimals: u8,
    provenance: ValueProvenance,
}

impl PrecisionMetadata {
    pub(crate) fn new(decimals: u8, provenance: ValueProvenance) -> crate::error::Result<Self> {
        if decimals > 6 {
            return Err(crate::error::Error::new(
                crate::error::ErrorKind::MetadataUnavailable,
                "security decimal is outside the supported price scale",
            )
            .with_context("decimal", format!("{decimals}")));
        }
        Ok(Self {
            decimals,
            provenance,
        })
    }

    pub(crate) const fn decimals(self) -> u8 {
        self.decimals
    }
    pub(crate) const fn provenance(self) -> ValueProvenance {
        self.provenance
    }

    /// Opt-in audited pricing fallback only. It is not listing/existence evidence.
    pub(crate) fn audited_prefix_fallback(id: &SecurityId) -> Option<Self> {
        // Audited compatibility pricing prefixes from protocol::unit. This opt-in
        // rule makes no assertion that the id exists or is actually an ETF.
        let prefixes: &[&str] = &["15", "16", "50", "51", "52", "53", "56", "58"];
        prefixes
            .iter()
            .any(|prefix| id.number().starts_with(prefix))
            .then_some(Self {
                decimals: 3,
                provenance: ValueProvenance::PrefixFallback,
            })
    }

    /// HQ trades encode milli-yuan independently of quote metadata decimals.
    pub(crate) fn hq_trade_price(raw_milli_yuan: i64) -> crate::types::Price {
        crate::types::Price::with_value_and_provenance(
            crate::types::NumericValue::ScaledInteger {
                value: raw_milli_yuan,
                scale: 3,
            },
            ValueProvenance::WireRule,
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct UnitMetadata<T> {
    unit: T,
    provenance: ValueProvenance,
}

impl<T: Copy> UnitMetadata<T> {
    pub(crate) const fn new(unit: T, provenance: ValueProvenance) -> Self {
        Self { unit, provenance }
    }
    pub(crate) const fn unit(self) -> T {
        self.unit
    }
    pub(crate) const fn provenance(self) -> ValueProvenance {
        self.provenance
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MetadataRecord {
    id: SecurityId,
    price_precision: Option<PrecisionMetadata>,
    quantity_unit: Option<UnitMetadata<QuantityUnit>>,
    amount_unit: Option<UnitMetadata<AmountUnit>>,
}

impl MetadataRecord {
    pub(crate) fn new(
        id: SecurityId,
        price_precision: Option<PrecisionMetadata>,
        quantity_unit: Option<UnitMetadata<QuantityUnit>>,
        amount_unit: Option<UnitMetadata<AmountUnit>>,
    ) -> Self {
        Self {
            id,
            price_precision,
            quantity_unit,
            amount_unit,
        }
    }

    pub(crate) fn id(&self) -> &SecurityId {
        &self.id
    }
    pub(crate) const fn price_precision(&self) -> Option<PrecisionMetadata> {
        self.price_precision
    }
    pub(crate) const fn quantity_unit(&self) -> Option<UnitMetadata<QuantityUnit>> {
        self.quantity_unit
    }
    pub(crate) const fn amount_unit(&self) -> Option<UnitMetadata<AmountUnit>> {
        self.amount_unit
    }
}

pub(crate) fn market_matches(id: &SecurityId, market: HqMarket) -> bool {
    id.market() == market
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fallback_is_opt_in_audited_and_trade_scale_is_independent() {
        let fund = SecurityId::new(HqMarket::Shanghai, "510300").unwrap();
        let fallback = PrecisionMetadata::audited_prefix_fallback(&fund).unwrap();
        assert_eq!(fallback.provenance(), ValueProvenance::PrefixFallback);
        let ordinary = SecurityId::new(HqMarket::Shanghai, "600000").unwrap();
        assert!(PrecisionMetadata::audited_prefix_fallback(&ordinary).is_none());
        let quote = PrecisionMetadata::new(4, ValueProvenance::SecurityTable).unwrap();
        assert_eq!(quote.decimals(), 4);
        assert_eq!(
            PrecisionMetadata::hq_trade_price(125_528).value(),
            crate::types::NumericValue::ScaledInteger {
                value: 125_528,
                scale: 3
            }
        );
    }
}
