use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::Arc;

use crate::error::{Error, ErrorKind, Result};

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub enum MarketDateChoice {
    #[default]
    Current,
    Date(TradingDate),
}

/// UTC epoch seconds are converted with the protocol's known Shanghai offset.
/// Implementations are deterministic injection points for tests and client policy.
pub trait Clock: Send + Sync {
    fn unix_seconds(&self) -> Result<i64>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

#[derive(Clone)]
pub(crate) struct ClockHandle(pub(crate) Arc<dyn Clock>);

impl Default for ClockHandle {
    fn default() -> Self { Self(Arc::new(SystemClock)) }
}

impl std::fmt::Debug for ClockHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ClockHandle(..)")
    }
}

impl Clock for SystemClock {
    fn unix_seconds(&self) -> Result<i64> {
        let now = SystemTime::now();
        match now.duration_since(UNIX_EPOCH) {
            Ok(duration) => i64::try_from(duration.as_secs())
                .map_err(|_| date_error("system clock is outside supported epoch range")),
            Err(before_epoch) => {
                let duration = before_epoch.duration();
                let rounded_seconds = duration.as_secs().checked_add(u64::from(duration.subsec_nanos() != 0))
                    .ok_or_else(|| date_error("system clock is outside supported epoch range"))?;
                i64::try_from(rounded_seconds).map(|seconds| -seconds)
                    .map_err(|_| date_error("system clock is outside supported epoch range"))
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TradingDate {
    year: u16,
    month: u8,
    day: u8,
}

impl TradingDate {
    pub fn new(year: u16, month: u8, day: u8) -> Result<Self> {
        if year == 0 || !(1..=12).contains(&month) || day == 0 || day > month_days(year, month) {
            return Err(Error::new(ErrorKind::InvalidArgument, "invalid Gregorian date"));
        }
        Ok(Self { year, month, day })
    }

    pub const fn year(self) -> u16 { self.year }
    pub const fn month(self) -> u8 { self.month }
    pub const fn day(self) -> u8 { self.day }

    pub fn shanghai_today(clock: &dyn Clock) -> Result<Self> {
        let shanghai_seconds = clock.unix_seconds()?.checked_add(8 * 60 * 60)
            .ok_or_else(|| date_error("Shanghai clock conversion overflows epoch seconds"))?;
        Self::from_days_since_epoch(shanghai_seconds.div_euclid(86_400))
    }

    pub fn add_days(self, days: i64) -> Result<Self> {
        let current = days_from_civil(self.year, self.month, self.day);
        let next = current.checked_add(days)
            .ok_or_else(|| date_error("date arithmetic overflows"))?;
        Self::from_days_since_epoch(next)
    }

    fn from_days_since_epoch(days: i64) -> Result<Self> {
        if days < days_from_civil(1, 1, 1) || days > days_from_civil(u16::MAX, 12, 31) {
            return Err(date_error("date is outside supported Gregorian range"));
        }
        let (year, month, day) = civil_from_days(days);
        let year = u16::try_from(year).map_err(|_| date_error("date is outside supported Gregorian range"))?;
        Self::new(year, month, day)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct EventTime(EventTimeKind);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
enum EventTimeKind {
    TradingDate(TradingDate),
    Shanghai {
        date: TradingDate,
        hour: u8,
        minute: u8,
        second: u8,
    },
    Local {
        date: TradingDate,
        hour: u8,
        minute: u8,
        second: u8,
    },
    TimeOfDay {
        hour: u8,
        minute: u8,
        second: u8,
    },
    UnknownRaw(i64),
}

impl EventTime {
    pub const fn trading_date(date: TradingDate) -> Self {
        Self(EventTimeKind::TradingDate(date))
    }

    pub fn shanghai(date: TradingDate, hour: u8, minute: u8, second: u8) -> Result<Self> {
        validate_clock(hour, minute, second)?;
        Ok(Self(EventTimeKind::Shanghai { date, hour, minute, second }))
    }

    pub fn local(date: TradingDate, hour: u8, minute: u8, second: u8) -> Result<Self> {
        validate_clock(hour, minute, second)?;
        Ok(Self(EventTimeKind::Local { date, hour, minute, second }))
    }

    pub fn time_of_day(hour: u8, minute: u8, second: u8) -> Result<Self> {
        validate_clock(hour, minute, second)?;
        Ok(Self(EventTimeKind::TimeOfDay { hour, minute, second }))
    }

    /// Retains an event-time scalar whose wire encoding has not been established.
    pub const fn unknown_raw(raw: i64) -> Self { Self(EventTimeKind::UnknownRaw(raw)) }

    pub const fn unknown_raw_value(self) -> Option<i64> {
        match self.0 { EventTimeKind::UnknownRaw(raw) => Some(raw), _ => None }
    }

    /// Adds elapsed minutes while preserving the source's known or unknown zone.
    /// Date-only values and time-of-day values have no unambiguous rollover date.
    pub fn add_minutes(self, minutes: i64) -> Result<Self> {
        let (date, hour, minute, second, shanghai) = match self.0 {
            EventTimeKind::Shanghai { date, hour, minute, second } => (date, hour, minute, second, true),
            EventTimeKind::Local { date, hour, minute, second } => (date, hour, minute, second, false),
            EventTimeKind::TradingDate(_) | EventTimeKind::TimeOfDay { .. } => {
                return Err(date_error("minute rollover requires a dated local time"));
            }
            EventTimeKind::UnknownRaw(_) => {
                return Err(date_error("unknown raw event time cannot be shifted"));
            }
        };
        let seconds = days_from_civil(date.year, date.month, date.day)
            .checked_mul(86_400)
            .and_then(|value| value.checked_add(i64::from(hour) * 3_600 + i64::from(minute) * 60 + i64::from(second)))
            .and_then(|value| value.checked_add(minutes.checked_mul(60)?))
            .ok_or_else(|| date_error("event time arithmetic overflows"))?;
        let days = seconds.div_euclid(86_400);
        let second_of_day = seconds.rem_euclid(86_400);
        let date = TradingDate::from_days_since_epoch(days)?;
        let hour = u8::try_from(second_of_day / 3_600).map_err(|_| date_error("invalid derived hour"))?;
        let minute = u8::try_from((second_of_day % 3_600) / 60).map_err(|_| date_error("invalid derived minute"))?;
        let second = u8::try_from(second_of_day % 60).map_err(|_| date_error("invalid derived second"))?;
        if shanghai { Self::shanghai(date, hour, minute, second) } else { Self::local(date, hour, minute, second) }
    }

    pub const fn source_kind(self) -> EventTimeSource {
        match self.0 {
            EventTimeKind::TradingDate(_) => EventTimeSource::TradingDate,
            EventTimeKind::Shanghai { .. } => EventTimeSource::Shanghai,
            EventTimeKind::Local { .. } => EventTimeSource::Local,
            EventTimeKind::TimeOfDay { .. } => EventTimeSource::TimeOfDay,
            EventTimeKind::UnknownRaw(_) => EventTimeSource::Unknown,
        }
    }

    pub const fn date(self) -> Option<TradingDate> {
        match self.0 {
            EventTimeKind::TradingDate(date)
            | EventTimeKind::Shanghai { date, .. }
            | EventTimeKind::Local { date, .. } => Some(date),
            EventTimeKind::TimeOfDay { .. } | EventTimeKind::UnknownRaw(_) => None,
        }
    }

    pub const fn clock(self) -> Option<(u8, u8, u8)> {
        match self.0 {
            EventTimeKind::TradingDate(_) => None,
            EventTimeKind::Shanghai { hour, minute, second, .. }
            | EventTimeKind::Local { hour, minute, second, .. }
            | EventTimeKind::TimeOfDay { hour, minute, second } => Some((hour, minute, second)),
            EventTimeKind::UnknownRaw(_) => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum EventTimeSource {
    TradingDate,
    Shanghai,
    Local,
    TimeOfDay,
    Unknown,
}

fn validate_clock(hour: u8, minute: u8, second: u8) -> Result<()> {
    if hour > 23 || minute > 59 || second > 59 {
        return Err(Error::new(ErrorKind::InvalidArgument, "invalid time of day"));
    }
    Ok(())
}

fn month_days(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => 0,
    }
}

fn days_from_civil(year: u16, month: u8, day: u8) -> i64 {
    let mut year = i64::from(year);
    let month = i64::from(month);
    year -= i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let adjusted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * adjusted_month + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, u8, u8) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month as u8, day as u8)
}

fn date_error(message: &str) -> Error {
    Error::new(ErrorKind::InvalidArgument, message).with_context("field", "date/time")
}
