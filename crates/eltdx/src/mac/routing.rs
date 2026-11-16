//! Pure MAC-EX identity routing decisions; no transport/session behavior.
use crate::mac::models::{MacExMarketId, MacExRoute};

/// Source HK-stock markets take the EX transaction path. HK futures/options
/// (23..=26) and every other market remain on the common MAC command path.
pub fn transaction_route(market: MacExMarketId) -> MacExRoute {
    match market.get() { 27|31|48|49|71|98 => MacExRoute::ExHongKongTransactions, _ => MacExRoute::MacCommand }
}

/// The seven source MAC-EX quote fields are intentionally narrower than MAC Common.
pub fn default_quote_selection() -> crate::mac::bitmap::FieldSelection {
    crate::mac::bitmap::FieldSelection::mac_ex_default()
}

pub fn hk_price_from_wire(raw: u32) -> f64 { f64::from(raw) / 1000.0 }
