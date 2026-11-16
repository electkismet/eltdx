use super::{Amount, Ratio, ResultMeta, SecurityId, TradingDate};

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct MoneyFlowDay {
    pub(crate) date: Option<TradingDate>,
    pub(crate) date_raw: u32,
    pub(crate) total_amount: Amount,
    pub(crate) buckets: [u16; 16],
    pub(crate) raw_values: [u32; 21],
    pub(crate) main_net: Amount,
    pub(crate) main_buy_net: Amount,
    pub(crate) main_ratio: Ratio,
    pub(crate) main_buy_ratio: Ratio,
    pub(crate) main_size_net: [Amount; 4],
    pub(crate) main_buy_size_net: [Amount; 4],
}

impl MoneyFlowDay {
    pub const fn date(&self) -> Option<TradingDate> { self.date }
    pub const fn date_raw(&self) -> u32 { self.date_raw }
    pub const fn total_amount(&self) -> Amount { self.total_amount }
    pub const fn buckets(&self) -> &[u16; 16] { &self.buckets }
    pub const fn raw_values(&self) -> &[u32; 21] { &self.raw_values }
    pub const fn main_net(&self) -> Amount { self.main_net }
    pub const fn main_buy_net(&self) -> Amount { self.main_buy_net }
    pub const fn main_ratio(&self) -> Ratio { self.main_ratio }
    pub const fn main_buy_ratio(&self) -> Ratio { self.main_buy_ratio }
    pub const fn main_size_net(&self) -> &[Amount; 4] { &self.main_size_net }
    pub const fn main_buy_size_net(&self) -> &[Amount; 4] { &self.main_buy_size_net }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DailyMoneyFlow { id: SecurityId, records: Vec<MoneyFlowDay>, meta: ResultMeta }

impl DailyMoneyFlow {
    pub(crate) fn new(id: SecurityId, records: Vec<MoneyFlowDay>, meta: ResultMeta) -> Self { Self { id, records, meta } }
    pub(crate) fn with_meta(mut self, meta: ResultMeta) -> Self { self.meta = meta; self }
    pub fn id(&self) -> &SecurityId { &self.id }
    pub fn records(&self) -> &[MoneyFlowDay] { &self.records }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
}
