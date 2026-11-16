//! `.lc1/.lc5` float32 and `.5` integer-price records are distinct formats.
use std::path::Path;
use crate::error::Result;
use crate::TradingDate;
use super::models::MinutePrices;
use super::{bars, invalid, malformed, AppendOptions, MinBar, MinuteKey, ReadOptions,
    ReadResult, RecordKey, TimeOfDay, WireFloat, WriteReport};

fn decode_key(record: &[u8; 32]) -> Result<MinuteKey> {
    let packed = bars::u16_at(record, 0);
    let (year, month, day) = bars::python_legacy::packed_date_parts(packed);
    let date = TradingDate::new(year, month as u8,
        day as u8).map_err(|error| malformed("invalid packed Gregorian date").with_source(error))?;
    let minutes = bars::u16_at(record, 2);
    if minutes >= 24 * 60 { return Err(malformed("packed minute is outside the day")); }
    let (hour, minute) = bars::python_legacy::packed_time_parts(minutes);
    MinuteKey::new(date, TimeOfDay::new(hour as u8, minute as u8, 0)?)
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
    let layout = if integer { bars::python_legacy::Layout::MinuteInteger } else { bars::python_legacy::Layout::MinuteFloat };
    let raw = bars::python_legacy::decode(record, layout);
    let values = raw.prices;
    let prices = if integer { MinutePrices::Integers(values) } else { MinutePrices::Floats(values.map(WireFloat::from_bits)) };
    Ok(MinBar::decoded(decode_key(record)?, prices, raw.volume,
        WireFloat::from_bits(raw.amount), raw.tail, retain.then_some(raw.raw)))
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
    let volume = match bar.source_volume() {
        Some(integer) => integer,
        None => bars::round_u32(bar.volume(), "volume")?,
    };
    use bars::python_legacy::{Layout, Slot};
    let layout = if integer { Layout::MinuteInteger } else { Layout::MinuteFloat };
    let fields = bars::python_legacy::encode_fields(layout, |slot| Ok::<_, crate::Error>(match slot {
        Slot::Date => u32::from(bars::u16_at(&bytes, 0)), Slot::Time => u32::from(bars::u16_at(&bytes, 2)),
        Slot::Price(index) => values[index], Slot::Amount => bar.amount().bits(),
        Slot::Volume => volume, Slot::Tail => bar.reserved(),
    }))?;
    bars::python_legacy::pack_bits(layout, &fields).ok_or_else(|| invalid("invalid minute encoder slots"))
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
