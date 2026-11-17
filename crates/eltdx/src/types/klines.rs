use crate::error::{Error, ErrorKind, Result};
use crate::types::batch::{DataPage, PageCursor, PageOptions};
use crate::types::provenance::ResultMeta;
use crate::types::{Amount, EventTime, Price, Quantity, SecurityId, TradingDate};

const MAX_KLINE_PAGE: usize = 800;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum KlinePeriod {
    Min1,
    Min5,
    Min15,
    Min30,
    Min60,
    Day,
    Week,
    Month,
    Quarter,
    Year,
    Minutes(u16),
    Days(u16),
    Seconds(u16),
}

impl KlinePeriod {
    pub fn minutes(value: u16) -> Result<Self> {
        positive_parameter(value, "minutes").map(Self::Minutes)
    }
    pub fn days(value: u16) -> Result<Self> {
        positive_parameter(value, "days").map(Self::Days)
    }
    pub fn seconds(value: u16) -> Result<Self> {
        positive_parameter(value, "seconds").map(Self::Seconds)
    }

    pub(crate) fn wire(self) -> Result<(u16, u16)> {
        let (raw, parameter) = match self {
            Self::Min5 => (0, 1),
            Self::Min15 => (1, 1),
            Self::Min30 => (2, 1),
            Self::Min60 => (3, 1),
            Self::Day => (4, 1),
            Self::Week => (5, 1),
            Self::Month => (6, 1),
            Self::Min1 => (7, 1),
            Self::Minutes(value) => (8, value),
            Self::Days(value) => (9, value),
            Self::Quarter => (10, 1),
            Self::Year => (11, 1),
            Self::Seconds(value) => (13, value),
        };
        if parameter == 0 {
            return Err(query_error("period", "custom period must be positive"));
        }
        Ok((raw, parameter))
    }

    pub(crate) const fn minute_width(self) -> Option<u16> {
        match self {
            Self::Min1 => Some(1),
            Self::Min5 => Some(5),
            Self::Min15 => Some(15),
            Self::Min30 => Some(30),
            Self::Min60 => Some(60),
            Self::Minutes(value) if value > 0 => Some(value),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum AdjustMode {
    #[default]
    None,
    Qfq,
    Hfq,
    FixedQfq,
    FixedHfq,
}

impl AdjustMode {
    pub(crate) const fn wire(self) -> eltdx_protocol::unit::AdjustMode {
        match self {
            Self::None => eltdx_protocol::unit::AdjustMode::None,
            Self::Qfq => eltdx_protocol::unit::AdjustMode::Qfq,
            Self::Hfq => eltdx_protocol::unit::AdjustMode::Hfq,
            Self::FixedQfq => eltdx_protocol::unit::AdjustMode::FixedQfq,
            Self::FixedHfq => eltdx_protocol::unit::AdjustMode::FixedHfq,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum KlineKind {
    Stock,
    Index,
    #[default]
    Auto,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum BarTime {
    #[default]
    Start,
    End,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KlineQuery {
    period: KlinePeriod,
    start: u16,
    count: usize,
    adjust: AdjustMode,
    anchor_date: Option<TradingDate>,
    kind: KlineKind,
    bar_time: BarTime,
    pages: PageOptions,
    cursor: Option<PageCursor>,
}

impl KlineQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_period(mut self, value: KlinePeriod) -> Result<Self> {
        self.period = value;
        self.validate()?;
        Ok(self)
    }
    pub fn with_start(mut self, value: u16) -> Result<Self> {
        self.start = value;
        self.cursor = None;
        self.validate()?;
        Ok(self)
    }
    pub fn with_count(mut self, value: usize) -> Result<Self> {
        if !(1..=MAX_KLINE_PAGE).contains(&value) {
            return Err(query_error("count", "must be between 1 and 800"));
        }
        self.count = value;
        self.validate()?;
        Ok(self)
    }
    pub fn with_adjust(mut self, value: AdjustMode) -> Result<Self> {
        self.adjust = value;
        self.validate()?;
        Ok(self)
    }
    pub fn with_anchor_date(mut self, value: Option<TradingDate>) -> Result<Self> {
        self.anchor_date = value;
        self.validate()?;
        Ok(self)
    }
    pub fn with_kind(mut self, value: KlineKind) -> Result<Self> {
        self.kind = value;
        self.validate()?;
        Ok(self)
    }
    pub fn with_bar_time(mut self, value: BarTime) -> Result<Self> {
        self.bar_time = value;
        self.validate()?;
        Ok(self)
    }
    pub fn with_pages(mut self, value: PageOptions) -> Result<Self> {
        validate_page_options(&value)?;
        if value.page_size() > MAX_KLINE_PAGE {
            return Err(query_error("page_size", "must not exceed 800"));
        }
        self.pages = value;
        self.validate()?;
        Ok(self)
    }
    pub fn with_cursor(mut self, cursor: &PageCursor) -> Result<Self> {
        self.start = u16::try_from(cursor.next_offset()).map_err(|_| {
            query_error(
                "cursor",
                "continuation offset exceeds the K-line wire range",
            )
        })?;
        self.cursor = Some(cursor.clone());
        self.validate()?;
        Ok(self)
    }

    pub const fn period(&self) -> KlinePeriod {
        self.period
    }
    pub const fn start(&self) -> u16 {
        self.start
    }
    pub const fn count(&self) -> usize {
        self.count
    }
    pub const fn adjust(&self) -> AdjustMode {
        self.adjust
    }
    pub const fn anchor_date(&self) -> Option<TradingDate> {
        self.anchor_date
    }
    pub const fn kind(&self) -> KlineKind {
        self.kind
    }
    pub const fn bar_time(&self) -> BarTime {
        self.bar_time
    }
    pub fn pages(&self) -> &PageOptions {
        &self.pages
    }
    pub fn cursor(&self) -> Option<&PageCursor> {
        self.cursor.as_ref()
    }

    pub(crate) fn validate(&self) -> Result<u32> {
        self.period.wire()?;
        if self.count == 0 || self.count > MAX_KLINE_PAGE {
            return Err(query_error("count", "must be between 1 and 800"));
        }
        validate_page_options(&self.pages)?;
        if self.pages.page_size() > MAX_KLINE_PAGE {
            return Err(query_error("page_size", "must not exceed 800"));
        }
        if self.anchor_date.is_some()
            && !matches!(self.adjust, AdjustMode::FixedQfq | AdjustMode::FixedHfq)
        {
            return Err(query_error(
                "anchor_date",
                "anchor date requires a fixed adjustment mode",
            ));
        }
        if self.anchor_date.is_some_and(|date| date.year() > 9999) {
            return Err(query_error(
                "anchor_date",
                "HQ date year must be between 1 and 9999",
            ));
        }
        Ok(u32::from(self.start))
    }
}

impl Default for KlineQuery {
    fn default() -> Self {
        Self {
            period: KlinePeriod::Day,
            start: 0,
            count: MAX_KLINE_PAGE,
            adjust: AdjustMode::None,
            anchor_date: None,
            kind: KlineKind::Auto,
            bar_time: BarTime::Start,
            pages: PageOptions::new(800, 200, 160_000).unwrap_or_else(|_| unreachable!()),
            cursor: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DelistedQuery {
    start: u16,
    count: usize,
    pages: PageOptions,
    cursor: Option<PageCursor>,
}

impl DelistedQuery {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_start(mut self, value: u16) -> Result<Self> {
        self.start = value;
        self.cursor = None;
        self.validate()?;
        Ok(self)
    }
    pub fn with_count(mut self, value: usize) -> Result<Self> {
        if !(1..=MAX_KLINE_PAGE).contains(&value) {
            return Err(query_error("count", "must be between 1 and 800"));
        }
        self.count = value;
        self.validate()?;
        Ok(self)
    }
    pub fn with_pages(mut self, value: PageOptions) -> Result<Self> {
        validate_page_options(&value)?;
        if value.page_size() > MAX_KLINE_PAGE {
            return Err(query_error("page_size", "must not exceed 800"));
        }
        self.pages = value;
        self.validate()?;
        Ok(self)
    }
    pub fn with_cursor(mut self, cursor: &PageCursor) -> Result<Self> {
        self.start = u16::try_from(cursor.next_offset()).map_err(|_| {
            query_error(
                "cursor",
                "continuation offset exceeds the delisted wire range",
            )
        })?;
        self.cursor = Some(cursor.clone());
        self.validate()?;
        Ok(self)
    }
    pub const fn start(&self) -> u16 {
        self.start
    }
    pub const fn count(&self) -> usize {
        self.count
    }
    pub fn pages(&self) -> &PageOptions {
        &self.pages
    }
    pub fn cursor(&self) -> Option<&PageCursor> {
        self.cursor.as_ref()
    }
    pub(crate) fn validate(&self) -> Result<u32> {
        if self.count == 0 || self.count > MAX_KLINE_PAGE {
            return Err(query_error("count", "must be between 1 and 800"));
        }
        validate_page_options(&self.pages)?;
        if self.pages.page_size() > MAX_KLINE_PAGE {
            return Err(query_error("page_size", "must not exceed 800"));
        }
        Ok(u32::from(self.start))
    }
}

impl Default for DelistedQuery {
    fn default() -> Self {
        Self {
            start: 0,
            count: 800,
            pages: PageOptions::new(800, 200, 160_000).unwrap_or_else(|_| unreachable!()),
            cursor: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct KlineBar {
    time: EventTime,
    source_time: EventTime,
    open: Price,
    close: Price,
    high: Price,
    low: Price,
    last_close: Option<Price>,
    volume: Quantity,
    amount: Amount,
    index_up: Option<u32>,
    index_down: Option<u32>,
    volume_word: u32,
    amount_word: u32,
    unknown_suffix: Option<[u8; 4]>,
}

impl KlineBar {
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn new(
        time: EventTime,
        source_time: EventTime,
        open: Price,
        close: Price,
        high: Price,
        low: Price,
        last_close: Option<Price>,
        volume: Quantity,
        amount: Amount,
        index_up: Option<u32>,
        index_down: Option<u32>,
        volume_word: u32,
        amount_word: u32,
        unknown_suffix: Option<[u8; 4]>,
    ) -> Self {
        Self {
            time,
            source_time,
            open,
            close,
            high,
            low,
            last_close,
            volume,
            amount,
            index_up,
            index_down,
            volume_word,
            amount_word,
            unknown_suffix,
        }
    }
    pub const fn time(&self) -> EventTime {
        self.time
    }
    /// Original exchange-local timestamp, before optional end-label alignment.
    pub const fn source_time(&self) -> EventTime {
        self.source_time
    }
    pub const fn open(&self) -> Price {
        self.open
    }
    pub const fn close(&self) -> Price {
        self.close
    }
    pub const fn high(&self) -> Price {
        self.high
    }
    pub const fn low(&self) -> Price {
        self.low
    }
    pub const fn last_close(&self) -> Option<Price> {
        self.last_close
    }
    pub const fn volume(&self) -> Quantity {
        self.volume
    }
    pub const fn amount(&self) -> Amount {
        self.amount
    }
    pub const fn index_up(&self) -> Option<u32> {
        self.index_up
    }
    pub const fn index_down(&self) -> Option<u32> {
        self.index_down
    }
    pub const fn volume_word(&self) -> u32 {
        self.volume_word
    }
    pub const fn amount_word(&self) -> u32 {
        self.amount_word
    }
    pub const fn unknown_suffix(&self) -> Option<[u8; 4]> {
        self.unknown_suffix
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct KlineSeries {
    id: SecurityId,
    period: KlinePeriod,
    kind: KlineKind,
    adjust: AdjustMode,
    anchor_date: Option<TradingDate>,
    time_label: BarTime,
    bars: DataPage<KlineBar>,
    pages_read: usize,
    legacy_raw: Option<bytes::Bytes>,
}

impl KlineSeries {
    #[allow(
        clippy::too_many_arguments,
        reason = "Keep the checked operation context, query and source provenance explicit."
    )]
    pub(crate) fn new(
        id: SecurityId,
        period: KlinePeriod,
        kind: KlineKind,
        adjust: AdjustMode,
        anchor_date: Option<TradingDate>,
        time_label: BarTime,
        bars: DataPage<KlineBar>,
        pages_read: usize,
        legacy_raw: Option<bytes::Bytes>,
    ) -> Self {
        Self {
            id,
            period,
            kind,
            adjust,
            anchor_date,
            time_label,
            bars,
            pages_read,
            legacy_raw,
        }
    }
    pub fn id(&self) -> &SecurityId {
        &self.id
    }
    pub const fn period(&self) -> KlinePeriod {
        self.period
    }
    pub const fn kind(&self) -> KlineKind {
        self.kind
    }
    pub const fn adjust(&self) -> AdjustMode {
        self.adjust
    }
    pub const fn anchor_date(&self) -> Option<TradingDate> {
        self.anchor_date
    }
    pub const fn time_label(&self) -> BarTime {
        self.time_label
    }
    pub fn bars(&self) -> &DataPage<KlineBar> {
        &self.bars
    }
    pub const fn pages_read(&self) -> usize {
        self.pages_read
    }
    pub fn meta(&self) -> &ResultMeta {
        self.bars.meta()
    }
}

pub(crate) fn validate_page_options(value: &PageOptions) -> Result<()> {
    if value.page_size() == 0 || value.max_pages() == 0 || value.max_records() == 0 {
        return Err(query_error(
            "pages",
            "page and collection limits must be positive",
        ));
    }
    Ok(())
}

fn positive_parameter(value: u16, field: &'static str) -> Result<u16> {
    if value == 0 {
        Err(query_error(field, "must be between 1 and 65535"))
    } else {
        Ok(value)
    }
}

fn query_error(field: &'static str, message: &'static str) -> Error {
    Error::new(ErrorKind::InvalidArgument, message).with_context("field", field)
}
