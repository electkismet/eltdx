use super::{EventTime, MarketDateChoice, Price, Quantity, ResultMeta, SecurityId, TradingDate};
use crate::error::{Error, ErrorKind, Result};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AuctionQuery {
    date: MarketDateChoice,
}

impl AuctionQuery {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_date(mut self, date: MarketDateChoice) -> Result<Self> {
        self.date = date;
        self.validate()?;
        Ok(self)
    }
    pub const fn date(&self) -> MarketDateChoice {
        self.date
    }
    pub(crate) fn validate(&self) -> Result<()> {
        if matches!(self.date, MarketDateChoice::Date(date) if date.year() > 9999) {
            return Err(Error::new(
                ErrorKind::InvalidArgument,
                "HQ auction date year must be at most 9999",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct AuctionPoint {
    pub(crate) index: u32,
    pub(crate) time: EventTime,
    pub(crate) price: Price,
    pub(crate) matched: Quantity,
    pub(crate) unmatched_signed: i64,
    pub(crate) unmatched: Quantity,
    pub(crate) direction_raw: i64,
    pub(crate) minute_of_day_raw: u16,
    pub(crate) second_raw: u8,
    pub(crate) reserved: u8,
}

impl AuctionPoint {
    pub const fn index(&self) -> u32 {
        self.index
    }
    pub const fn time(&self) -> EventTime {
        self.time
    }
    pub const fn price(&self) -> Price {
        self.price
    }
    pub const fn matched(&self) -> Quantity {
        self.matched
    }
    pub const fn unmatched_signed(&self) -> i64 {
        self.unmatched_signed
    }
    pub const fn unmatched(&self) -> Quantity {
        self.unmatched
    }
    pub const fn direction_raw(&self) -> i64 {
        self.direction_raw
    }
    pub const fn minute_of_day_raw(&self) -> u16 {
        self.minute_of_day_raw
    }
    pub const fn second_raw(&self) -> u8 {
        self.second_raw
    }
    pub const fn reserved(&self) -> u8 {
        self.reserved
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct AuctionSeries {
    id: SecurityId,
    trading_date: Option<TradingDate>,
    mode_selector: u32,
    start: u32,
    requested: u32,
    points: Vec<AuctionPoint>,
    meta: ResultMeta,
}

impl AuctionSeries {
    pub(crate) fn new(
        id: SecurityId,
        trading_date: TradingDate,
        mode_selector: u32,
        start: u32,
        requested: u32,
        points: Vec<AuctionPoint>,
        meta: ResultMeta,
    ) -> Self {
        Self {
            id,
            trading_date: Some(trading_date),
            mode_selector,
            start,
            requested,
            points,
            meta,
        }
    }
    pub fn id(&self) -> &SecurityId {
        &self.id
    }
    pub const fn trading_date(&self) -> Option<TradingDate> {
        self.trading_date
    }
    pub const fn mode_selector(&self) -> u32 {
        self.mode_selector
    }
    pub const fn start(&self) -> u32 {
        self.start
    }
    pub const fn requested(&self) -> u32 {
        self.requested
    }
    pub fn points(&self) -> &[AuctionPoint] {
        &self.points
    }
    pub fn meta(&self) -> &ResultMeta {
        &self.meta
    }
}
