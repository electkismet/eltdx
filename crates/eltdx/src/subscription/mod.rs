//! Subscription internals are integrated in stages; no active public handle is
//! exposed until dispatcher, lease ownership and owner stop/join are connected.
mod cursor;
mod queue;

#[cfg(test)]
mod source_cases {
    use crate::types::{Amount, AmountUnit, EventTime, NumericValue, Price, Quantity, QuantityUnit,
        QuoteUpdate, ResultMeta, Snapshot};

    pub(super) fn update(code: &str, cursor: u32) -> QuoteUpdate {
        let id: crate::types::SecurityId = code.parse().unwrap();
        let price = Price::scaled_integer(1_000, 2);
        let quantity = Quantity::new(NumericValue::ScaledInteger { value: 1, scale: 0 }, QuantityUnit::Lots);
        let amount = Amount::new(NumericValue::ScaledInteger { value: 10, scale: 0 }, AmountUnit::Cny);
        let snapshot = Snapshot::new(id.clone(), 1, price, price, price, price, price,
            EventTime::unknown_raw(i64::from(cursor)), i64::from(cursor), quantity, quantity, quantity,
            quantity, amount, amount, Vec::new(), Vec::new(), ResultMeta::default());
        QuoteUpdate::new(id, cursor, 1, snapshot, 0)
    }
}
