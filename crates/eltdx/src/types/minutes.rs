use crate::error::{Error, ErrorKind, Result};
use crate::types::provenance::ResultMeta;
use crate::types::{EventTime, MarketDateChoice, Price, Quantity, SecurityId, TradingDate};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct MinuteQuery {
    date: MarketDateChoice,
}

impl MinuteQuery {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_date(mut self, value: MarketDateChoice) -> Result<Self> {
        if matches!(value, MarketDateChoice::Date(date) if date.year() > 9999) {
            return Err(query_error(
                "date",
                "HQ date year must be between 1 and 9999",
            ));
        }
        self.date = value;
        Ok(self)
    }
    pub fn with_current_date(mut self) -> Result<Self> {
        self.date = MarketDateChoice::Current;
        Ok(self)
    }
    pub const fn date(&self) -> MarketDateChoice {
        self.date
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum AuxKind {
    #[default]
    BuySellStrength,
    VolumeComparison,
}

impl AuxKind {
    pub(crate) const fn wire(self) -> eltdx_protocol::commands::minutes::IntradayAuxKind {
        match self {
            Self::BuySellStrength => {
                eltdx_protocol::commands::minutes::IntradayAuxKind::BuySellStrength
            }
            Self::VolumeComparison => {
                eltdx_protocol::commands::minutes::IntradayAuxKind::VolumeComparison
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SparklineQuery {
    selector: u8,
    window: u16,
}

impl SparklineQuery {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_selector(mut self, value: u8) -> Result<Self> {
        self.selector = value;
        Ok(self)
    }
    pub fn with_window(mut self, value: u16) -> Result<Self> {
        if value == 0 {
            return Err(query_error("window", "must be between 1 and 65535"));
        }
        self.window = value;
        Ok(self)
    }
    pub const fn selector(&self) -> u8 {
        self.selector
    }
    pub const fn window(&self) -> u16 {
        self.window
    }
    pub(crate) fn validate(&self) -> Result<()> {
        if self.window == 0 {
            return Err(query_error("window", "must be between 1 and 65535"));
        }
        if self.selector != 1 {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "sparkline selector semantics are not supported",
            )
            .with_context("selector", format!("{}", self.selector)));
        }
        Ok(())
    }
}

impl Default for SparklineQuery {
    fn default() -> Self {
        Self {
            selector: 1,
            window: 20,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct MinutePoint {
    index: u32,
    time: EventTime,
    price: Price,
    volume: Quantity,
    average: Option<Price>,
}

impl MinutePoint {
    pub(crate) const fn new(
        index: u32,
        time: EventTime,
        price: Price,
        volume: Quantity,
        average: Option<Price>,
    ) -> Self {
        Self {
            index,
            time,
            price,
            volume,
            average,
        }
    }
    pub const fn index(&self) -> u32 {
        self.index
    }
    pub const fn time(&self) -> EventTime {
        self.time
    }
    pub const fn price(&self) -> Price {
        self.price
    }
    pub const fn volume(&self) -> Quantity {
        self.volume
    }
    pub const fn average(&self) -> Option<Price> {
        self.average
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct MinuteSeries {
    id: SecurityId,
    trading_date: Option<TradingDate>,
    previous_close: Option<Price>,
    open: Option<Price>,
    points: Vec<MinutePoint>,
    volume_sum: Quantity,
    meta: ResultMeta,
}

impl MinuteSeries {
    pub(crate) fn new(
        id: SecurityId,
        trading_date: Option<TradingDate>,
        previous_close: Option<Price>,
        open: Option<Price>,
        points: Vec<MinutePoint>,
        volume_sum: Quantity,
        meta: ResultMeta,
    ) -> Self {
        Self {
            id,
            trading_date,
            previous_close,
            open,
            points,
            volume_sum,
            meta,
        }
    }
    pub fn id(&self) -> &SecurityId {
        &self.id
    }
    pub const fn trading_date(&self) -> Option<TradingDate> {
        self.trading_date
    }
    pub const fn previous_close(&self) -> Option<Price> {
        self.previous_close
    }
    pub const fn open(&self) -> Option<Price> {
        self.open
    }
    pub fn points(&self) -> &[MinutePoint] {
        &self.points
    }
    pub const fn volume_sum(&self) -> Quantity {
        self.volume_sum
    }
    pub fn meta(&self) -> &ResultMeta {
        &self.meta
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum AuxValues {
    BuySell {
        buy_commission: i64,
        sell_commission: i64,
    },
    Volumes {
        #[cfg_attr(
            feature = "serde",
            serde(serialize_with = "crate::types::numeric::serialize_finite_f32")
        )]
        previous: f32,
        #[cfg_attr(
            feature = "serde",
            serde(serialize_with = "crate::types::numeric::serialize_finite_f32")
        )]
        current: f32,
        #[cfg_attr(
            feature = "serde",
            serde(serialize_with = "crate::types::numeric::serialize_finite_f32")
        )]
        cumulative: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct MinuteAuxPoint {
    index: u32,
    time: EventTime,
    values: AuxValues,
}

impl MinuteAuxPoint {
    pub(crate) const fn new(index: u32, time: EventTime, values: AuxValues) -> Self {
        Self {
            index,
            time,
            values,
        }
    }
    pub const fn index(&self) -> u32 {
        self.index
    }
    pub const fn time(&self) -> EventTime {
        self.time
    }
    pub const fn values(&self) -> AuxValues {
        self.values
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct MinuteAuxSeries {
    id: SecurityId,
    kind: AuxKind,
    points: Vec<MinuteAuxPoint>,
    meta: ResultMeta,
}

impl MinuteAuxSeries {
    pub(crate) fn new(
        id: SecurityId,
        kind: AuxKind,
        points: Vec<MinuteAuxPoint>,
        meta: ResultMeta,
    ) -> Self {
        Self {
            id,
            kind,
            points,
            meta,
        }
    }
    pub fn id(&self) -> &SecurityId {
        &self.id
    }
    pub const fn kind(&self) -> AuxKind {
        self.kind
    }
    pub fn points(&self) -> &[MinuteAuxPoint] {
        &self.points
    }
    pub fn meta(&self) -> &ResultMeta {
        &self.meta
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SparklineSeries {
    id: SecurityId,
    selector: u8,
    selector_echo: u16,
    requested_window: u16,
    max_count: u16,
    base: Price,
    prices: Vec<Price>,
    reserved: u32,
    meta: ResultMeta,
}

impl SparklineSeries {
    #[allow(
        clippy::too_many_arguments,
        reason = "Keep the checked operation context, query and source provenance explicit."
    )]
    pub(crate) fn new(
        id: SecurityId,
        selector: u8,
        selector_echo: u16,
        requested_window: u16,
        max_count: u16,
        base: Price,
        prices: Vec<Price>,
        reserved: u32,
        meta: ResultMeta,
    ) -> Self {
        Self {
            id,
            selector,
            selector_echo,
            requested_window,
            max_count,
            base,
            prices,
            reserved,
            meta,
        }
    }
    pub fn id(&self) -> &SecurityId {
        &self.id
    }
    pub const fn selector(&self) -> u8 {
        self.selector
    }
    pub const fn selector_echo(&self) -> u16 {
        self.selector_echo
    }
    pub const fn requested_window(&self) -> u16 {
        self.requested_window
    }
    pub const fn max_count(&self) -> u16 {
        self.max_count
    }
    pub const fn base(&self) -> Price {
        self.base
    }
    pub fn prices(&self) -> &[Price] {
        &self.prices
    }
    pub const fn reserved(&self) -> u32 {
        self.reserved
    }
    pub fn meta(&self) -> &ResultMeta {
        &self.meta
    }
}

fn query_error(field: &'static str, message: &'static str) -> crate::error::Error {
    crate::error::Error::new(ErrorKind::InvalidArgument, message).with_context("field", field)
}
