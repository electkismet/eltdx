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
