use super::{Amount, ResultMeta};

/// L1 trade-size classification in CNY. The buckets describe trade amounts,
/// and do not identify actual institutional orders or investment intent.
#[derive(Clone, Debug, PartialEq)]
pub struct FundFlow {
    pub(crate) buckets: [Amount; 8], pub(crate) main_net: Amount, pub(crate) total_net: Amount,
    pub(crate) classified_count: usize, pub(crate) skipped_count: usize, pub(crate) meta: ResultMeta,
}
impl FundFlow {
    pub const fn super_in(&self) -> Amount { self.buckets[0] }
    pub const fn large_in(&self) -> Amount { self.buckets[1] }
    pub const fn medium_in(&self) -> Amount { self.buckets[2] }
    pub const fn small_in(&self) -> Amount { self.buckets[3] }
    pub const fn super_out(&self) -> Amount { self.buckets[4] }
    pub const fn large_out(&self) -> Amount { self.buckets[5] }
    pub const fn medium_out(&self) -> Amount { self.buckets[6] }
    pub const fn small_out(&self) -> Amount { self.buckets[7] }
    pub const fn main_net(&self) -> Amount { self.main_net }
    pub const fn total_net(&self) -> Amount { self.total_net }
    pub const fn classified_count(&self) -> usize { self.classified_count }
    pub const fn skipped_count(&self) -> usize { self.skipped_count }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
}
