//! Local EX `<IffffIIf>` records; independent of the network `ex` feature.
use super::{
    bars, AppendOptions, ExDailyBar, ReadOptions, ReadResult, RecordKey, WireFloat, WriteReport,
};
use crate::error::Result;
use crate::TradingDate;
use std::path::Path;

pub fn read(path: &Path, options: &ReadOptions) -> Result<ReadResult<ExDailyBar>> {
    bars::read_records(path, options, |record, retain| {
        let raw = bars::python_legacy::decode(record, bars::python_legacy::Layout::Extended);
        let prices = raw.prices.map(WireFloat::from_bits);
        Ok(ExDailyBar::new(
            bars::decode_date(raw.date)?,
            prices,
            raw.amount,
            raw.volume,
            WireFloat::from_bits(raw.tail),
        )
        .with_raw(retain.then_some(raw.raw)))
    })
}

pub fn encode(bar: &ExDailyBar) -> Result<[u8; 32]> {
    use bars::python_legacy::{Layout, Slot};
    let fields = bars::python_legacy::encode_fields(Layout::Extended, |slot| {
        Ok::<_, crate::Error>(match slot {
            Slot::Date => bars::date_integer(bar.date()),
            Slot::Price(index) => bar.prices()[index].bits(),
            Slot::Amount => bar.amount(),
            Slot::Volume => bar.volume(),
            Slot::Tail => bar.settlement().bits(),
            Slot::Time => unreachable!("daily layout has no time slot"),
        })
    })?;
    bars::python_legacy::pack_bits(Layout::Extended, &fields)
        .ok_or_else(|| super::invalid("invalid extended encoder slots"))
}

pub fn last_date(path: &Path) -> Result<Option<TradingDate>> {
    bars::last_record(path)?
        .map(|bytes| bars::decode_date(bars::u32_at(&bytes, 0)))
        .transpose()
}
pub fn append(path: &Path, rows: &[ExDailyBar], options: &AppendOptions) -> Result<WriteReport> {
    bars::append_records(
        path,
        rows,
        options,
        false,
        encode,
        |row| RecordKey::Date(row.date()),
        |path| Ok(last_date(path)?.map(RecordKey::Date)),
    )
}
pub fn write(path: &Path, rows: &[ExDailyBar], options: &AppendOptions) -> Result<WriteReport> {
    append(path, rows, options)
}
