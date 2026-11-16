//! Local EX `<IffffIIf>` records; independent of the network `ex` feature.
use std::path::Path;
use crate::error::Result;
use crate::TradingDate;
use super::{bars, AppendOptions, ExDailyBar, ReadOptions, ReadResult, RecordKey, WireFloat, WriteReport};

pub fn read(path: &Path, options: &ReadOptions) -> Result<ReadResult<ExDailyBar>> {
    bars::read_records(path, options, |record, retain| {
        let prices = [4, 8, 12, 16].map(|offset| WireFloat::from_bits(bars::u32_at(record, offset)));
        Ok(ExDailyBar::new(bars::decode_date(bars::u32_at(record, 0))?, prices,
            bars::u32_at(record, 20), bars::u32_at(record, 24), WireFloat::from_bits(bars::u32_at(record, 28)))
            .with_raw(retain.then_some(*record)))
    })
}

pub fn encode(bar: &ExDailyBar) -> Result<[u8; 32]> {
    let mut bytes = [0_u8; 32];
    bars::put_u32(&mut bytes, 0, bars::date_integer(bar.date()));
    for (index, value) in bar.prices().iter().enumerate() { bars::put_u32(&mut bytes, 4 + index * 4, value.bits()); }
    // Never derive this slot from a float-to-integer conversion.
    bars::put_u32(&mut bytes, 20, bar.amount());
    bars::put_u32(&mut bytes, 24, bar.volume());
    bars::put_u32(&mut bytes, 28, bar.settlement().bits());
    Ok(bytes)
}

pub fn last_date(path: &Path) -> Result<Option<TradingDate>> {
    bars::last_record(path)?.map(|bytes| bars::decode_date(bars::u32_at(&bytes, 0))).transpose()
}
pub fn append(path: &Path, rows: &[ExDailyBar], options: &AppendOptions) -> Result<WriteReport> {
    bars::append_records(path, rows, options, false, encode, |row| RecordKey::Date(row.date()),
        |path| Ok(last_date(path)?.map(RecordKey::Date)))
}
pub fn write(path: &Path, rows: &[ExDailyBar], options: &AppendOptions) -> Result<WriteReport> { append(path, rows, options) }
