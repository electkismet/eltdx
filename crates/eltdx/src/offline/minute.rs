//! `.lc1/.lc5` float32 and `.5` integer-price records are distinct formats.
use std::path::Path;
use crate::error::Result;
use crate::TradingDate;
use super::models::MinutePrices;
use super::{bars, invalid, malformed, AppendOptions, MinBar, MinuteKey, ReadOptions,
    ReadResult, RecordKey, TimeOfDay, WireFloat, WriteReport};

fn decode_key(record: &[u8; 32]) -> Result<MinuteKey> {
    let packed = bars::u16_at(record, 0);
    let date = TradingDate::new(packed / 2048 + 2004, ((packed % 2048) / 100) as u8,
        ((packed % 2048) % 100) as u8).map_err(|error| malformed("invalid packed Gregorian date").with_source(error))?;
    let minutes = bars::u16_at(record, 2);
    if minutes >= 24 * 60 { return Err(malformed("packed minute is outside the day")); }
    MinuteKey::new(date, TimeOfDay::new((minutes / 60) as u8, (minutes % 60) as u8, 0)?)
}

fn encode_key(key: MinuteKey, bytes: &mut [u8; 32]) -> Result<()> {
    let date = key.date();
    let year = date.year().checked_sub(2004).ok_or_else(|| invalid("packed date predates 2004"))?;
    let packed = u32::from(year) * 2048 + u32::from(date.month()) * 100 + u32::from(date.day());
    let packed = u16::try_from(packed).map_err(|_| invalid("packed date exceeds u16"))?;
    bytes[..2].copy_from_slice(&packed.to_le_bytes());
    let time = key.time();
    let minutes = u16::from(time.hour()) * 60 + u16::from(time.minute());
    bytes[2..4].copy_from_slice(&minutes.to_le_bytes());
    Ok(())
}

fn decode(record: &[u8; 32], integer: bool, retain: bool) -> Result<MinBar> {
    let values = [bars::u32_at(record, 4), bars::u32_at(record, 8), bars::u32_at(record, 12), bars::u32_at(record, 16)];
    let prices = if integer { MinutePrices::Integers(values) } else { MinutePrices::Floats(values.map(WireFloat::from_bits)) };
    Ok(MinBar::decoded(decode_key(record)?, prices, bars::u32_at(record, 24),
        WireFloat::from_bits(bars::u32_at(record, 20)), bars::u32_at(record, 28), retain.then_some(*record)))
}

pub fn read_lc(path: &Path, options: &ReadOptions) -> Result<ReadResult<MinBar>> {
    bars::read_records(path, options, |bytes, retain| decode(bytes, false, retain))
}
pub fn read_5(path: &Path, options: &ReadOptions) -> Result<ReadResult<MinBar>> {
    bars::read_records(path, options, |bytes, retain| decode(bytes, true, retain))
}

fn encode(bar: &MinBar, integer: bool) -> Result<[u8; 32]> {
    let mut bytes = [0_u8; 32];
    encode_key(bar.key(), &mut bytes)?;
    let values = match (&bar.prices, integer) {
        (MinutePrices::Floats(values), false) => values.map(WireFloat::bits),
        (MinutePrices::Integers(values), true) => *values,
        (_, true) => {
            let values = bar.prices();
            [bars::round_u32(values[0] * 100.0, "open")?, bars::round_u32(values[1] * 100.0, "high")?,
                bars::round_u32(values[2] * 100.0, "low")?, bars::round_u32(values[3] * 100.0, "close")?]
        }
        (_, false) => {
            let values = bar.prices();
            [bars::checked_f32(values[0])?, bars::checked_f32(values[1])?,
                bars::checked_f32(values[2])?, bars::checked_f32(values[3])?]
        }
    };
    for (index, value) in values.into_iter().enumerate() { bars::put_u32(&mut bytes, 4 + index * 4, value); }
    bars::put_u32(&mut bytes, 20, bar.amount().bits());
    let volume = match bar.source_volume() {
        Some(integer) => integer,
        None => bars::round_u32(bar.volume(), "volume")?,
    };
    bars::put_u32(&mut bytes, 24, volume);
    bars::put_u32(&mut bytes, 28, bar.reserved());
    Ok(bytes)
}

pub fn encode_lc(bar: &MinBar) -> Result<[u8; 32]> { encode(bar, false) }
pub fn encode_5(bar: &MinBar) -> Result<[u8; 32]> { encode(bar, true) }
pub fn last_lc_time(path: &Path) -> Result<Option<MinuteKey>> { bars::last_record(path)?.map(|bytes| decode_key(&bytes)).transpose() }
pub fn last_5_time(path: &Path) -> Result<Option<MinuteKey>> { last_lc_time(path) }

pub fn append_lc(path: &Path, rows: &[MinBar], options: &AppendOptions) -> Result<WriteReport> {
    bars::append_records(path, rows, options, false, encode_lc, |row| RecordKey::Minute(row.key()),
        |path| Ok(last_lc_time(path)?.map(RecordKey::Minute)))
}
pub fn append_5(path: &Path, rows: &[MinBar], options: &AppendOptions) -> Result<WriteReport> {
    bars::append_records(path, rows, options, false, encode_5, |row| RecordKey::Minute(row.key()),
        |path| Ok(last_5_time(path)?.map(RecordKey::Minute)))
}
/// Legacy name: append with the ordinary minute close policy.
pub fn write_lc(path: &Path, rows: &[MinBar], options: &AppendOptions) -> Result<WriteReport> { append_lc(path, rows, options) }
pub fn write_5(path: &Path, rows: &[MinBar], options: &AppendOptions) -> Result<WriteReport> { append_5(path, rows, options) }
