//! Source-backed clock decoding; no host clock, timezone or trading calendar.
use crate::{EventTime, Result, TradingDate};
use super::{protocol, models::BarTimeSource, query::{BarTime, ExPeriod}};

pub fn time_of_day(minutes: u16, second: u16) -> Result<EventTime> {
    if minutes >= 24 * 60 || second > 59 {
        return Err(protocol("invalid EX minute/second field")
            .with_context("raw_minutes", format!("{minutes}"))
            .with_context("raw_seconds", format!("{second}")));
    }
    EventTime::time_of_day((minutes / 60) as u8, (minutes % 60) as u8, second as u8)
        .map_err(|_| protocol("invalid EX clock")
            .with_context("raw_minutes", format!("{minutes}"))
            .with_context("raw_seconds", format!("{second}")))
}
pub fn packed_date(raw: u16) -> Result<TradingDate> {
    let year = (raw >> 11) + 2004;
    let rest = raw % 2048;
    TradingDate::new(year, (rest / 100) as u8, (rest % 100) as u8)
        .map_err(|_| protocol("invalid EX packed Gregorian date").with_context("raw_date", format!("{raw}")))
}
pub fn day_date(raw: u32) -> Result<TradingDate> {
    let (year,month,day) = crate::helpers::calculation::ymd_parts_with::<_,std::convert::Infallible>(&raw,
        |raw| Ok(raw/10000),|raw| Ok((raw%10000)/100),|raw| Ok(raw%100)).unwrap_or_else(|never| match never {});
    let year = u16::try_from(year).map_err(|_| protocol("EX year overflow"))?;
    TradingDate::new(year, month as u8, day as u8)
        .map_err(|_| protocol("invalid EX YYYYMMDD date").with_context("raw_date", format!("{raw}")))
}
pub fn bar_time(raw: [u8; 4], period: ExPeriod, alignment: BarTime) -> Result<(EventTime, EventTime, BarTimeSource)> {
    let (source, provenance) = if period.is_intraday() {
        let date = packed_date(u16::from_le_bytes([raw[0], raw[1]]))?;
        let minutes = u16::from_le_bytes([raw[2], raw[3]]);
        time_of_day(minutes, 0)?;
        (EventTime::local(date, (minutes / 60) as u8, (minutes % 60) as u8, 0)
            .map_err(|_| protocol("invalid EX bar clock"))?, BarTimeSource::PackedMinute)
    } else {
        (EventTime::local(day_date(u32::from_le_bytes(raw))?, 15, 0, 0)
            .map_err(|_| protocol("invalid EX daily bar clock"))?, BarTimeSource::SyntheticClose)
    };
    let aligned = match (alignment, period.duration_minutes()) {
        (BarTime::End, Some(delta)) => source.add_minutes(i64::from(delta)).map_err(|_| protocol("EX bar end overflow"))?,
        _ => source,
    };
    Ok((aligned, source, provenance))
}


/// EX clock value, independent of date and timezone. Construction is checked;
/// seconds are retained for transaction records.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TimeOfDay { hour: u8, minute: u8, second: u8 }
impl TimeOfDay {
    pub fn new(hour: u8, minute: u8, second: u8) -> Result<Self> {
        if hour > 23 || minute > 59 || second > 59 { return Err(super::invalid("invalid EX time of day")); }
        Ok(Self { hour, minute, second })
    }
    pub const fn hour(self) -> u8 { self.hour }
    pub const fn minute(self) -> u8 { self.minute }
    pub const fn second(self) -> u8 { self.second }
    pub fn as_event_time(self) -> Result<EventTime> { EventTime::time_of_day(self.hour,self.minute,self.second) }
}
