use crate::{Error, ErrorKind, Result};
use crate::types::{AdjustMode, AdjustmentFactor, KlineBar, NumericValue, Price};

pub fn adjust_price(price: Price, factor: &AdjustmentFactor, mode: AdjustMode) -> Result<Price> {
    let (scale, offset) = match mode {
        AdjustMode::None => return Ok(price),
        AdjustMode::Qfq | AdjustMode::FixedQfq => (factor.qfq_scale(), factor.qfq_offset()),
        AdjustMode::Hfq | AdjustMode::FixedHfq => (factor.hfq_scale(), factor.hfq_offset()),
    };
    let adjusted = price.as_f64() * scale + offset;
    if !adjusted.is_finite() { return Err(Error::new(ErrorKind::InvalidArgument, "adjusted price is non-finite")); }
    Ok(Price::with_value_and_provenance(NumericValue::F64(adjusted), price.provenance()))
}

/// Apply only the explicitly selected affine coefficients. Timestamp, source
/// time, amounts, volumes, unknown suffix and their units remain traceable.
pub fn adjust_bar(bar: &KlineBar, factor: &AdjustmentFactor, mode: AdjustMode) -> Result<KlineBar> {
    Ok(KlineBar::new(bar.time(), bar.source_time(), adjust_price(bar.open(), factor, mode)?,
        adjust_price(bar.close(), factor, mode)?, adjust_price(bar.high(), factor, mode)?,
        adjust_price(bar.low(), factor, mode)?, bar.last_close().map(|price| adjust_price(price, factor, mode)).transpose()?,
        bar.volume(), bar.amount(), bar.index_up(), bar.index_down(), bar.volume_word(), bar.amount_word(), bar.unknown_suffix()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Amount, AmountUnit, EventTime, Quantity, QuantityUnit, TradingDate};
    #[test]
    fn affine_bar_application_preserves_source_time_unknown_words_and_units() {
        let date = TradingDate::new(2026, 1, 2).unwrap(); let time = EventTime::trading_date(date);
        let price = Price::scaled_integer(1001, 2);
        let volume = Quantity::new(NumericValue::ScaledInteger { value: 77, scale: 0 }, QuantityUnit::Unknown);
        let amount = Amount::new(NumericValue::F32(99.5), AmountUnit::Unknown);
        let bar = KlineBar::new(time, time, price, price, price, price, Some(price), volume, amount,
            Some(5), Some(6), 0xf1234567, 0xe1234567, Some([1, 2, 3, 4]));
        let factor = AdjustmentFactor::new(date, 0.5, -0.3, 2., 0.6);
        let adjusted = adjust_bar(&bar, &factor, AdjustMode::FixedQfq).unwrap();
        assert_eq!(adjusted.open().as_f64(), 10.01 * 0.5 - 0.3);
        assert_eq!(adjusted.time(), bar.time()); assert_eq!(adjusted.source_time(), bar.source_time());
        assert_eq!(adjusted.volume(), volume); assert_eq!(adjusted.amount(), amount);
        assert_eq!(adjusted.volume_word(), bar.volume_word()); assert_eq!(adjusted.amount_word(), bar.amount_word());
        assert_eq!(adjusted.unknown_suffix(), bar.unknown_suffix()); assert_eq!(adjusted.index_up(), Some(5));
        assert_eq!(adjust_bar(&bar, &factor, AdjustMode::None).unwrap(), bar);
    }
}
