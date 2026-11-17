//! Typed HQ trade rows and column views. Conversion never performs I/O.

use super::{
    DataPage, EndEvidence, EventTime, MarketDateChoice, PageCursor, PageOptions, Price, Quantity,
    ResultMeta, SecurityId, SourceStamp, TradingDate,
};
use crate::error::{Error, ErrorKind, Result};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TradeQuery {
    date: MarketDateChoice,
    start: u16,
    count: usize,
    pages: PageOptions,
    cursor: Option<PageCursor>,
}

impl TradeQuery {
    #[allow(
        clippy::expect_used,
        reason = "Only fixed defaults or closed built-in field tables are unwrapped here."
    )]
    pub fn new() -> Self {
        Self {
            date: MarketDateChoice::Current,
            start: 0,
            count: 1800,
            pages: PageOptions::new(1800, 100, 180_000).expect("positive trade defaults"),
            cursor: None,
        }
    }
    pub fn with_date(mut self, value: MarketDateChoice) -> Result<Self> {
        self.date = value;
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
        self.count = value;
        self.validate()?;
        Ok(self)
    }
    pub fn with_pages(mut self, value: PageOptions) -> Result<Self> {
        self.pages = value;
        self.validate()?;
        Ok(self)
    }
    pub fn with_cursor(mut self, value: &PageCursor) -> Result<Self> {
        self.cursor = Some(value.clone());
        self.validate()?;
        Ok(self)
    }
    pub const fn date(&self) -> MarketDateChoice {
        self.date
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
    pub(crate) fn validate(&self) -> Result<()> {
        if !(1..=1800).contains(&self.count) || !(1..=1800).contains(&self.pages.page_size()) {
            return Err(Error::new(
                ErrorKind::InvalidArgument,
                "trade count and page size must be between 1 and 1800",
            ));
        }
        if let MarketDateChoice::Date(date) = self.date {
            if date.year() > 9999 {
                return Err(Error::new(
                    ErrorKind::InvalidArgument,
                    "HQ trade date year must be at most 9999",
                ));
            }
        }
        Ok(())
    }
}

impl Default for TradeQuery {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub enum TradeSide {
    Buy,
    Sell,
    Neutral,
    Status(i64),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub enum TradeEventKind {
    Trade,
    OpeningMatch,
    AuctionSnapshot,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TradeTick {
    pub(crate) index: u32,
    pub(crate) absolute_index: u32,
    pub(crate) time: EventTime,
    pub(crate) price: Price,
    pub(crate) volume: Quantity,
    pub(crate) order_count: i64,
    pub(crate) status_raw: i64,
    pub(crate) side: TradeSide,
    pub(crate) kind: TradeEventKind,
    pub(crate) auction_matched: Option<Quantity>,
    pub(crate) auction_unmatched_signed: Option<i64>,
    pub(crate) unknown_tail: Option<i64>,
    pub(crate) price_delta_raw: i64,
    pub(crate) price_acc_raw: i64,
    pub(crate) price_denominator: u16,
    pub(crate) reserved_zero: Option<i64>,
    pub(crate) source: SourceStamp,
}

impl TradeTick {
    pub const fn index(&self) -> u32 {
        self.index
    }
    pub const fn absolute_index(&self) -> u32 {
        self.absolute_index
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
    pub const fn order_count(&self) -> i64 {
        self.order_count
    }
    pub const fn status_raw(&self) -> i64 {
        self.status_raw
    }
    pub const fn side(&self) -> TradeSide {
        self.side
    }
    pub const fn kind(&self) -> TradeEventKind {
        self.kind
    }
    pub const fn auction_matched(&self) -> Option<Quantity> {
        self.auction_matched
    }
    pub const fn auction_unmatched_signed(&self) -> Option<i64> {
        self.auction_unmatched_signed
    }
    pub const fn unknown_tail(&self) -> Option<i64> {
        self.unknown_tail
    }
    pub const fn price_delta_raw(&self) -> i64 {
        self.price_delta_raw
    }
    pub const fn price_acc_raw(&self) -> i64 {
        self.price_acc_raw
    }
    pub const fn price_denominator(&self) -> u16 {
        self.price_denominator
    }
    pub const fn reserved_zero(&self) -> Option<i64> {
        self.reserved_zero
    }
    pub fn source(&self) -> &SourceStamp {
        &self.source
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TradePage {
    id: SecurityId,
    trading_date: Option<TradingDate>,
    price_base: Option<Price>,
    ticks: DataPage<TradeTick>,
    pages_read: usize,
}

impl TradePage {
    pub(crate) fn new(
        id: SecurityId,
        trading_date: Option<TradingDate>,
        price_base: Option<Price>,
        ticks: DataPage<TradeTick>,
        pages_read: usize,
    ) -> Self {
        Self {
            id,
            trading_date,
            price_base,
            ticks,
            pages_read,
        }
    }
    pub fn id(&self) -> &SecurityId {
        &self.id
    }
    pub const fn trading_date(&self) -> Option<TradingDate> {
        self.trading_date
    }
    pub const fn price_base(&self) -> Option<Price> {
        self.price_base
    }
    pub fn ticks(&self) -> &DataPage<TradeTick> {
        &self.ticks
    }
    pub const fn pages_read(&self) -> usize {
        self.pages_read
    }
    pub fn meta(&self) -> &ResultMeta {
        self.ticks.meta()
    }
}

/// Extra raw numeric columns are business evidence and survive RawPolicy::Omit.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TradeColumnData {
    indices: Vec<u32>,
    absolute_indices: Vec<u32>,
    times: Vec<EventTime>,
    prices: Vec<Price>,
    volumes: Vec<Quantity>,
    order_counts: Vec<i64>,
    statuses: Vec<i64>,
    sides: Vec<TradeSide>,
    kinds: Vec<TradeEventKind>,
    auction_matched: Vec<Option<Quantity>>,
    auction_unmatched_signed: Vec<Option<i64>>,
    unknown_tails: Vec<Option<i64>>,
    price_deltas_raw: Vec<i64>,
    price_accumulators_raw: Vec<i64>,
    price_denominators: Vec<u16>,
    reserved_zeros: Vec<Option<i64>>,
    sources: Vec<SourceStamp>,
}

impl TradeColumnData {
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }
    pub fn absolute_indices(&self) -> &[u32] {
        &self.absolute_indices
    }
    pub fn times(&self) -> &[EventTime] {
        &self.times
    }
    pub fn prices(&self) -> &[Price] {
        &self.prices
    }
    pub fn volumes(&self) -> &[Quantity] {
        &self.volumes
    }
    pub fn order_counts(&self) -> &[i64] {
        &self.order_counts
    }
    pub fn statuses(&self) -> &[i64] {
        &self.statuses
    }
    pub fn sides(&self) -> &[TradeSide] {
        &self.sides
    }
    pub fn kinds(&self) -> &[TradeEventKind] {
        &self.kinds
    }
    pub fn auction_matched(&self) -> &[Option<Quantity>] {
        &self.auction_matched
    }
    pub fn auction_unmatched_signed(&self) -> &[Option<i64>] {
        &self.auction_unmatched_signed
    }
    pub fn unknown_tails(&self) -> &[Option<i64>] {
        &self.unknown_tails
    }
    pub fn price_deltas_raw(&self) -> &[i64] {
        &self.price_deltas_raw
    }
    pub fn price_accumulators_raw(&self) -> &[i64] {
        &self.price_accumulators_raw
    }
    pub fn price_denominators(&self) -> &[u16] {
        &self.price_denominators
    }
    pub fn reserved_zeros(&self) -> &[Option<i64>] {
        &self.reserved_zeros
    }
    pub fn sources(&self) -> &[SourceStamp] {
        &self.sources
    }

    fn from_rows(rows: Vec<TradeTick>) -> Result<Self> {
        let mut columns = Self::default();
        let count = rows.len();
        macro_rules! reserve { ($($field:ident),+ $(,)?) => { $(
            columns.$field.try_reserve(count).map_err(|error| Error::new(ErrorKind::LimitExceeded,
                "unable to reserve trade columns").with_source(error))?;
        )+ }; }
        reserve!(
            indices,
            absolute_indices,
            times,
            prices,
            volumes,
            order_counts,
            statuses,
            sides,
            kinds,
            auction_matched,
            auction_unmatched_signed,
            unknown_tails,
            price_deltas_raw,
            price_accumulators_raw,
            price_denominators,
            reserved_zeros,
            sources
        );
        for row in rows {
            columns.indices.push(row.index);
            columns.absolute_indices.push(row.absolute_index);
            columns.times.push(row.time);
            columns.prices.push(row.price);
            columns.volumes.push(row.volume);
            columns.order_counts.push(row.order_count);
            columns.statuses.push(row.status_raw);
            columns.sides.push(row.side);
            columns.kinds.push(row.kind);
            columns.auction_matched.push(row.auction_matched);
            columns
                .auction_unmatched_signed
                .push(row.auction_unmatched_signed);
            columns.unknown_tails.push(row.unknown_tail);
            columns.price_deltas_raw.push(row.price_delta_raw);
            columns.price_accumulators_raw.push(row.price_acc_raw);
            columns.price_denominators.push(row.price_denominator);
            columns.reserved_zeros.push(row.reserved_zero);
            columns.sources.push(row.source);
        }
        Ok(columns)
    }

    fn row(&self, index: usize) -> TradeTick {
        TradeTick {
            index: self.indices[index],
            absolute_index: self.absolute_indices[index],
            time: self.times[index],
            price: self.prices[index],
            volume: self.volumes[index],
            order_count: self.order_counts[index],
            status_raw: self.statuses[index],
            side: self.sides[index],
            kind: self.kinds[index],
            auction_matched: self.auction_matched[index],
            auction_unmatched_signed: self.auction_unmatched_signed[index],
            unknown_tail: self.unknown_tails[index],
            price_delta_raw: self.price_deltas_raw[index],
            price_acc_raw: self.price_accumulators_raw[index],
            price_denominator: self.price_denominators[index],
            reserved_zero: self.reserved_zeros[index],
            source: self.sources[index].clone(),
        }
    }
}

// No Serialize for opaque continuation tokens; rows/columns themselves are serializable.
#[derive(Clone, Debug, PartialEq)]
pub struct TradeColumns {
    id: SecurityId,
    trading_date: Option<TradingDate>,
    price_base: Option<Price>,
    start: u32,
    requested: usize,
    end: EndEvidence,
    next_cursor: Option<PageCursor>,
    columns: TradeColumnData,
    pages_read: usize,
    meta: ResultMeta,
}

impl TradeColumns {
    pub(crate) fn from_page(page: TradePage) -> Result<Self> {
        let start = u32::try_from(page.ticks.start()).map_err(|_| {
            Error::new(
                ErrorKind::Internal,
                "trade page start is outside the wire range",
            )
        })?;
        let requested = page.ticks.requested();
        let end = page.ticks.end();
        let next_cursor = page.ticks.next_cursor().cloned();
        let meta = page.ticks.meta().clone();
        let columns = TradeColumnData::from_rows(page.ticks.into_items())?;
        Ok(Self {
            id: page.id,
            trading_date: page.trading_date,
            price_base: page.price_base,
            start,
            requested,
            end,
            next_cursor,
            columns,
            pages_read: page.pages_read,
            meta,
        })
    }
    pub fn id(&self) -> &SecurityId {
        &self.id
    }
    pub const fn trading_date(&self) -> Option<TradingDate> {
        self.trading_date
    }
    pub const fn price_base(&self) -> Option<Price> {
        self.price_base
    }
    pub const fn start(&self) -> u32 {
        self.start
    }
    pub const fn requested(&self) -> usize {
        self.requested
    }
    pub const fn end(&self) -> EndEvidence {
        self.end
    }
    pub fn next_cursor(&self) -> Option<&PageCursor> {
        self.next_cursor.as_ref()
    }
    pub fn len(&self) -> usize {
        self.columns.indices.len()
    }
    pub fn is_empty(&self) -> bool {
        self.columns.indices.is_empty()
    }
    pub fn columns(&self) -> &TradeColumnData {
        &self.columns
    }
    pub fn meta(&self) -> &ResultMeta {
        &self.meta
    }
    pub const fn pages_read(&self) -> usize {
        self.pages_read
    }
    pub fn tick(&self, index: usize) -> Result<TradeTick> {
        if index >= self.len() {
            return Err(Error::new(
                ErrorKind::InvalidArgument,
                "trade column index is out of bounds",
            ));
        }
        Ok(self.columns.row(index))
    }
    pub fn to_page(&self) -> TradePage {
        let rows = (0..self.len())
            .map(|index| self.columns.row(index))
            .collect();
        TradePage::new(
            self.id.clone(),
            self.trading_date,
            self.price_base,
            DataPage::new(
                rows,
                u64::from(self.start),
                self.requested,
                self.next_cursor.clone(),
                self.end,
                self.meta.clone(),
            ),
            self.pages_read,
        )
    }
}
