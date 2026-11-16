//! Corporate actions retain the source coefficients independently of their use
//! in affine adjustment. Finance units are classified only with field evidence.
use crate::{Error, ErrorKind, Result};
use super::{Amount, MissingPolicy, Price, Quantity, ResultMeta, SecurityId, TradingDate};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapitalQuery { batch_size: usize }
impl CapitalQuery {
    pub fn new() -> Self { Self::default() }
    pub fn with_batch_size(mut self, value: usize) -> Result<Self> { self.batch_size = value; self.validate()?; Ok(self) }
    pub const fn batch_size(&self) -> usize { self.batch_size }
    pub(crate) fn validate(&self) -> Result<()> {
        if !(1..=200).contains(&self.batch_size) { return Err(invalid("batch_size must be between 1 and 200")); }
        Ok(())
    }
}
impl Default for CapitalQuery { fn default() -> Self { Self { batch_size: 75 } } }

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FactorQuery { anchor_date: Option<TradingDate>, start_date: Option<TradingDate>, capital: CapitalQuery }
impl FactorQuery {
    pub fn new() -> Self { Self::default() }
    pub fn with_anchor_date(mut self, value: Option<TradingDate>) -> Result<Self> { self.anchor_date = value; self.validate()?; Ok(self) }
    pub fn with_start_date(mut self, value: Option<TradingDate>) -> Result<Self> { self.start_date = value; self.validate()?; Ok(self) }
    pub fn with_capital(mut self, value: CapitalQuery) -> Result<Self> { self.capital = value; self.validate()?; Ok(self) }
    pub const fn anchor_date(&self) -> Option<TradingDate> { self.anchor_date }
    pub const fn start_date(&self) -> Option<TradingDate> { self.start_date }
    pub fn capital(&self) -> &CapitalQuery { &self.capital }
    pub(crate) fn validate(&self) -> Result<()> {
        self.capital.validate()?;
        if let (Some(anchor), Some(start)) = (self.anchor_date, self.start_date) {
            if anchor < start { return Err(invalid("anchor_date must not be earlier than start_date")); }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FinanceQuery { missing: MissingPolicy }
impl FinanceQuery {
    pub fn new() -> Self { Self::default() }
    pub fn with_missing(mut self, value: MissingPolicy) -> Result<Self> { self.missing = value; Ok(self) }
    pub const fn missing(&self) -> MissingPolicy { self.missing }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CapitalEvent {
    date: Option<TradingDate>, date_raw: u32, category: u8, category_name: Option<String>,
    coefficients: [f64; 4], coefficient_bits: [[u8; 4]; 4], reserved: u8,
}
impl CapitalEvent {
    /// Construct a source event without inventing dates, units or tag semantics.
    pub fn from_wire(date_raw: u32, category: u8, coefficient_bits: [[u8; 4]; 4], reserved: u8) -> Self {
        let date = u16::try_from(date_raw / 10_000).ok().and_then(|year|
            TradingDate::new(year, ((date_raw / 100) % 100) as u8, (date_raw % 100) as u8).ok());
        Self { date, date_raw, category, category_name: None,
            coefficients: coefficient_bits.map(|bits| f64::from(f32::from_le_bytes(bits))), coefficient_bits, reserved }
    }
    pub(crate) fn with_name(mut self, name: Option<&str>) -> Self { self.category_name = name.map(str::to_owned); self }
    pub const fn date(&self) -> Option<TradingDate> { self.date }
    pub const fn date_raw(&self) -> u32 { self.date_raw }
    pub const fn category(&self) -> u8 { self.category }
    pub fn category_name(&self) -> Option<&str> { self.category_name.as_deref() }
    /// Original f32 numeric values; only tag1 has per-ten-share affine semantics.
    pub const fn coefficients(&self) -> &[f64; 4] { &self.coefficients }
    pub const fn coefficient_bits(&self) -> &[[u8; 4]; 4] { &self.coefficient_bits }
    pub const fn reserved(&self) -> u8 { self.reserved }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CapitalChangeBlock { id: SecurityId, records: Vec<CapitalEvent>, declared_blocks: u16, meta: ResultMeta }
impl CapitalChangeBlock {
    pub fn new(id: SecurityId, records: Vec<CapitalEvent>, declared_blocks: u16, meta: ResultMeta) -> Self {
        Self { id, records, declared_blocks, meta }
    }
    pub fn id(&self) -> &SecurityId { &self.id }
    pub fn records(&self) -> &[CapitalEvent] { &self.records }
    pub const fn declared_blocks(&self) -> u16 { self.declared_blocks }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdjustmentFactor { date: TradingDate, qfq_scale: f64, qfq_offset: f64, hfq_scale: f64, hfq_offset: f64 }
impl AdjustmentFactor {
    pub(crate) const fn new(date: TradingDate, qfq_scale: f64, qfq_offset: f64, hfq_scale: f64, hfq_offset: f64) -> Self {
        Self { date, qfq_scale, qfq_offset, hfq_scale, hfq_offset }
    }
    pub const fn date(&self) -> TradingDate { self.date }
    pub const fn qfq_scale(&self) -> f64 { self.qfq_scale }
    pub const fn qfq_offset(&self) -> f64 { self.qfq_offset }
    pub const fn hfq_scale(&self) -> f64 { self.hfq_scale }
    pub const fn hfq_offset(&self) -> f64 { self.hfq_offset }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AdjustmentFactors {
    id: SecurityId, anchor_date: Option<TradingDate>, start_date: Option<TradingDate>, rows: Vec<AdjustmentFactor>, meta: ResultMeta,
}
impl AdjustmentFactors {
    pub(crate) fn new(id: SecurityId, query: &FactorQuery, rows: Vec<AdjustmentFactor>, meta: ResultMeta) -> Self {
        Self { id, anchor_date: query.anchor_date(), start_date: query.start_date(), rows, meta }
    }
    pub fn id(&self) -> &SecurityId { &self.id }
    pub const fn anchor_date(&self) -> Option<TradingDate> { self.anchor_date }
    pub const fn start_date(&self) -> Option<TradingDate> { self.start_date }
    pub fn rows(&self) -> &[AdjustmentFactor] { &self.rows }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum FinanceField {
    CirculatingShares, TotalShares, StateShares, FounderLegalShares, LegalShares, BShares, HShares, Eps,
    TotalAssets, CurrentAssets, FixedAssets, IntangibleAssets, ShareholderCount, CurrentLiabilities,
    LongTermLiabilities, CapitalReserve, NetAssets, MainRevenue, MainProfit, Receivables, OperatingProfit,
    InvestmentIncome, OperatingCashFlow, TotalCashFlow, Inventory, TotalProfit, AfterTaxProfit, NetProfit,
    UndistributedProfit, NetAssetsPerShare, Reserved2,
}
impl FinanceField {
    pub const ALL: [Self; 31] = [Self::CirculatingShares, Self::TotalShares, Self::StateShares,
        Self::FounderLegalShares, Self::LegalShares, Self::BShares, Self::HShares, Self::Eps, Self::TotalAssets,
        Self::CurrentAssets, Self::FixedAssets, Self::IntangibleAssets, Self::ShareholderCount, Self::CurrentLiabilities,
        Self::LongTermLiabilities, Self::CapitalReserve, Self::NetAssets, Self::MainRevenue, Self::MainProfit,
        Self::Receivables, Self::OperatingProfit, Self::InvestmentIncome, Self::OperatingCashFlow, Self::TotalCashFlow,
        Self::Inventory, Self::TotalProfit, Self::AfterTaxProfit, Self::NetProfit, Self::UndistributedProfit,
        Self::NetAssetsPerShare, Self::Reserved2];
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum FinanceValue { Shares(Quantity), Money(Amount), PerShare(Price), Count(f64), Unclassified { value: f32, bits: [u8; 4] } }

#[derive(Clone, Debug, PartialEq)]
pub struct FinanceRecord {
    id: SecurityId, province: u16, industry: u16, updated: Option<TradingDate>, ipo: Option<TradingDate>,
    updated_raw: u32, ipo_raw: u32, values: Vec<(FinanceField, FinanceValue)>, meta: ResultMeta,
}
impl FinanceRecord {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(id: SecurityId, province: u16, industry: u16, updated_raw: u32, ipo_raw: u32,
        updated: Option<TradingDate>, ipo: Option<TradingDate>, values: Vec<(FinanceField, FinanceValue)>, meta: ResultMeta) -> Self {
        Self { id, province, industry, updated, ipo, updated_raw, ipo_raw, values, meta }
    }
    pub fn id(&self) -> &SecurityId { &self.id }
    pub const fn province(&self) -> u16 { self.province }
    pub const fn industry(&self) -> u16 { self.industry }
    pub const fn updated(&self) -> Option<TradingDate> { self.updated }
    pub const fn ipo(&self) -> Option<TradingDate> { self.ipo }
    pub const fn updated_raw(&self) -> u32 { self.updated_raw }
    pub const fn ipo_raw(&self) -> u32 { self.ipo_raw }
    pub fn values(&self) -> &[(FinanceField, FinanceValue)] { &self.values }
    pub fn value(&self, field: FinanceField) -> Option<FinanceValue> { self.values.iter().find(|(key, _)| *key == field).map(|(_, value)| *value) }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
    pub(crate) fn project(&self, fields: &[FinanceField]) -> Result<FinanceProjection> {
        let mut values = Vec::new(); values.try_reserve(fields.len()).map_err(super::super::services::allocation_error)?;
        for &field in fields { values.push((field, self.value(field).ok_or_else(|| Error::new(ErrorKind::Protocol, "decoded finance field is absent"))?)); }
        Ok(FinanceProjection { id: self.id.clone(), fields: values, updated: self.updated, ipo: self.ipo, meta: self.meta.clone() })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FinanceProjection { id: SecurityId, fields: Vec<(FinanceField, FinanceValue)>, updated: Option<TradingDate>, ipo: Option<TradingDate>, meta: ResultMeta }
impl FinanceProjection {
    pub fn id(&self) -> &SecurityId { &self.id }
    pub fn fields(&self) -> &[(FinanceField, FinanceValue)] { &self.fields }
    pub const fn updated(&self) -> Option<TradingDate> { self.updated }
    pub const fn ipo(&self) -> Option<TradingDate> { self.ipo }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
}

fn invalid(message: &str) -> Error { Error::new(ErrorKind::InvalidArgument, message).with_context("domain", "corporate") }
