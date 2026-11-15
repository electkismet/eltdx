use crate::error::{Error, ErrorKind, Result};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EventTime(EventTimeKind);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
