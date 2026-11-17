//! The ladder's omitted date follows the caller's local civil date. HQ's
//! Shanghai trading-day clock has different semantics and is not substituted.
use crate::{Error, ErrorKind, Result, TradingDate};
use std::sync::Arc;

pub trait LocalDateClock: Send + Sync {
    fn today(&self) -> Result<TradingDate>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemLocalDateClock;
impl LocalDateClock for SystemLocalDateClock {
    fn today(&self) -> Result<TradingDate> {
        use chrono::Datelike;
        let date = chrono::Local::now().date_naive();
        let year = u16::try_from(date.year()).map_err(|_| {
            Error::new(
                ErrorKind::InvalidArgument,
                "local year is outside supported Gregorian range",
            )
        })?;
        let month = u8::try_from(date.month())
            .map_err(|_| Error::new(ErrorKind::Internal, "invalid local month"))?;
        let day = u8::try_from(date.day())
            .map_err(|_| Error::new(ErrorKind::Internal, "invalid local day"))?;
        TradingDate::new(year, month, day)
    }
}

#[derive(Clone)]
pub(crate) struct LocalDateClockHandle(pub Arc<dyn LocalDateClock>);
impl Default for LocalDateClockHandle {
    fn default() -> Self {
        Self(Arc::new(SystemLocalDateClock))
    }
}
impl std::fmt::Debug for LocalDateClockHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("LocalDateClockHandle(..)")
    }
}
