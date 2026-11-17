//! Pure checked MAC queries. Total targets remain u32; only the eventual wire
//! page is narrowed. A query never creates an owner or performs hidden I/O.
use super::bitmap::{encode_bitmap, BitmapLayout, FieldSelection, PresetField};
use super::enums::{
    BoardRankKey, BoardSymbol, BoardType, Category, FilterMask, SortOrder, SortType,
};
use super::models::{MacAdjust, MacInstrumentId, MacMarketId, MacPeriod, RemoteFileName};
use crate::types::BarTime;
use crate::{DownloadOptions, Error, ErrorKind, PageOptions, Result, TradingDate};
use std::num::{NonZeroU16, NonZeroU32};

pub(crate) fn invalid(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::InvalidArgument, message).with_context("domain", "mac")
}
fn count(value: u32) -> Result<NonZeroU32> {
    NonZeroU32::new(value).ok_or_else(|| invalid("MAC total target must be positive"))
}
pub(crate) fn span(start: u32, target: NonZeroU32) -> Result<()> {
    start
        .checked_add(target.get())
        .ok_or_else(|| invalid("MAC start plus target exceeds u32"))?;
    Ok(())
}
pub(crate) fn validate_pages(pages: &PageOptions, maximum: usize) -> Result<()> {
    if pages.page_size() > maximum
        || pages.max_pages() == usize::MAX
        || pages.max_records() == usize::MAX
        || pages.page_size().checked_mul(pages.max_pages()).is_none()
    {
        return Err(invalid(
            "MAC page policy exceeds wire size or finite collection arithmetic",
        ));
    }
    Ok(())
}
#[allow(
    clippy::expect_used,
    reason = "Only fixed defaults or closed built-in field tables are unwrapped here."
)]
pub(crate) fn page_defaults(size: usize, pages: usize, rows: usize) -> PageOptions {
    PageOptions::new(size, pages, rows).expect("positive fixed MAC page defaults")
}
pub(crate) fn date_wire(value: Option<TradingDate>) -> Result<Option<u32>> {
    value
        .map(|date| {
            if date.year() > 9999 {
                return Err(invalid("MAC date exceeds YYYYMMDD wire range"));
            }
            Ok(u32::from(date.year()) * 10000
                + u32::from(date.month()) * 100
                + u32::from(date.day()))
        })
        .transpose()
}
fn fields(value: &FieldSelection, layout: BitmapLayout) -> Result<()> {
    encode_bitmap(value, layout).map(|_| ())
}
fn list_fields() -> FieldSelection {
    let mut selection = FieldSelection::preset(PresetField::Basic);
    for field in FieldSelection::preset(PresetField::Volume).ids() {
        selection.insert(field);
    }
    selection
}
pub(crate) fn quote_ids(ids: &[MacInstrumentId]) -> Result<()> {
    if ids.is_empty() || ids.len() > 80 {
        return Err(invalid("MAC quotes requires 1..=80 identity occurrences"));
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct QuotesQuery {
    ids: Vec<MacInstrumentId>,
    fields: FieldSelection,
}
impl QuotesQuery {
    pub fn new(ids: Vec<MacInstrumentId>) -> Result<Self> {
        quote_ids(&ids)?;
        Ok(Self {
            ids,
            fields: FieldSelection::preset(PresetField::Common),
        })
    }
    pub fn ids(&self) -> &[MacInstrumentId] {
        &self.ids
    }
    pub fn fields(&self) -> &FieldSelection {
        &self.fields
    }
    pub fn with_ids(mut self, ids: Vec<MacInstrumentId>) -> Result<Self> {
        quote_ids(&ids)?;
        self.ids = ids;
        Ok(self)
    }
    pub fn with_fields(mut self, value: FieldSelection) -> Result<Self> {
        fields(&value, BitmapLayout::Quote20)?;
        self.fields = value;
        Ok(self)
    }
}
#[derive(Clone, Debug)]
pub struct QuoteListQuery {
    category: Category,
    start: u32,
    count: NonZeroU32,
    sort: SortType,
    order: SortOrder,
    exclude: FilterMask,
    fields: FieldSelection,
    pages: PageOptions,
}
impl QuoteListQuery {
    #[allow(
        clippy::expect_used,
        reason = "Only fixed defaults or closed built-in field tables are unwrapped here."
    )]
    pub fn new(category: Category) -> Self {
        Self {
            category,
            start: 0,
            count: NonZeroU32::new(80).expect("positive"),
            sort: SortType::ChangePct,
            order: SortOrder::Desc,
            exclude: FilterMask::empty(),
            fields: list_fields(),
            pages: page_defaults(80, 2000, 160000),
        }
    }
    pub const fn category(&self) -> Category {
        self.category
    }
    pub const fn start(&self) -> u32 {
        self.start
    }
    pub const fn count(&self) -> NonZeroU32 {
        self.count
    }
    pub const fn sort(&self) -> SortType {
        self.sort
    }
    pub const fn order(&self) -> SortOrder {
        self.order
    }
    pub const fn exclude(&self) -> FilterMask {
        self.exclude
    }
    pub fn fields(&self) -> &FieldSelection {
        &self.fields
    }
    pub fn pages(&self) -> &PageOptions {
        &self.pages
    }
    pub fn with_category(mut self, value: Category) -> Self {
        self.category = value;
        self
    }
    pub fn with_start(mut self, value: u32) -> Result<Self> {
        span(value, self.count)?;
        self.start = value;
        Ok(self)
    }
    pub fn with_count(mut self, value: u32) -> Result<Self> {
        let value = count(value)?;
        span(self.start, value)?;
        self.count = value;
        Ok(self)
    }
    pub fn with_sort(mut self, value: SortType) -> Self {
        self.sort = value;
        self
    }
    pub fn with_order(mut self, value: SortOrder) -> Self {
        self.order = value;
        self
    }
    pub fn with_exclude(mut self, value: FilterMask) -> Self {
        self.exclude = value;
        self
    }
    pub fn with_fields(mut self, value: FieldSelection) -> Result<Self> {
        fields(&value, BitmapLayout::Board16)?;
        self.fields = value;
        Ok(self)
    }
    pub fn with_pages(mut self, value: PageOptions) -> Result<Self> {
        validate_pages(&value, 80)?;
        self.pages = value;
        Ok(self)
    }
}
#[derive(Clone, Debug)]
pub struct BarsQuery {
    id: MacInstrumentId,
    period: MacPeriod,
    times: NonZeroU16,
    start: u32,
    count: NonZeroU32,
    adjust: MacAdjust,
    bar_time: BarTime,
    pages: PageOptions,
    strict_adjustment: bool,
}
impl BarsQuery {
    #[allow(
        clippy::expect_used,
        reason = "Only fixed defaults or closed built-in field tables are unwrapped here."
    )]
    pub fn new(id: MacInstrumentId) -> Self {
        Self {
            id,
            period: MacPeriod::Daily,
            times: NonZeroU16::MIN,
            start: 0,
            count: NonZeroU32::new(800).expect("positive"),
            adjust: MacAdjust::None,
            bar_time: BarTime::Start,
            pages: page_defaults(700, 200, 140000),
            strict_adjustment: false,
        }
    }
    pub fn id(&self) -> &MacInstrumentId {
        &self.id
    }
    pub const fn period(&self) -> MacPeriod {
        self.period
    }
    pub const fn times(&self) -> NonZeroU16 {
        self.times
    }
    pub const fn start(&self) -> u32 {
        self.start
    }
    pub const fn count(&self) -> NonZeroU32 {
        self.count
    }
    pub const fn adjust(&self) -> MacAdjust {
        self.adjust
    }
    pub const fn bar_time(&self) -> BarTime {
        self.bar_time
    }
    pub fn pages(&self) -> &PageOptions {
        &self.pages
    }
    pub const fn strict_adjustment(&self) -> bool {
        self.strict_adjustment
    }
    pub fn with_id(mut self, value: MacInstrumentId) -> Self {
        self.id = value;
        self
    }
    pub fn with_period(mut self, value: MacPeriod) -> Result<Self> {
        if self.bar_time == BarTime::End && !value.has_known_semantics() {
            return Err(invalid("unknown MAC period cannot align End time"));
        }
        self.period = value;
        Ok(self)
    }
    pub fn with_times(mut self, value: u32) -> Result<Self> {
        self.times = u16::try_from(value)
            .ok()
            .and_then(NonZeroU16::new)
            .ok_or_else(|| invalid("MAC times must be in 1..=65535"))?;
        Ok(self)
    }
    pub fn with_start(mut self, value: u32) -> Result<Self> {
        span(value, self.count)?;
        self.start = value;
        Ok(self)
    }
    pub fn with_count(mut self, value: u32) -> Result<Self> {
        let value = count(value)?;
        span(self.start, value)?;
        self.count = value;
        Ok(self)
    }
    pub fn with_adjust(mut self, value: MacAdjust) -> Self {
        self.adjust = value;
        self
    }
    pub fn with_bar_time(mut self, value: BarTime) -> Result<Self> {
        if value == BarTime::End && !self.period.has_known_semantics() {
            return Err(invalid("unknown MAC period cannot align End time"));
        }
        self.bar_time = value;
        Ok(self)
    }
    pub fn with_pages(mut self, value: PageOptions) -> Result<Self> {
        validate_pages(&value, 700)?;
        self.pages = value;
        Ok(self)
    }
    pub fn with_strict_adjustment(mut self, value: bool) -> Self {
        self.strict_adjustment = value;
        self
    }
}

macro_rules! single_id {
    ($($name:ident),+ $(,)?) => { $(
        #[derive(Clone, Debug)]
        pub struct $name { id: MacInstrumentId }
        impl $name {
            pub fn new(id: MacInstrumentId) -> Self { Self { id } }
            pub fn id(&self) -> &MacInstrumentId { &self.id }
            pub fn with_id(mut self, value: MacInstrumentId) -> Self { self.id = value; self }
        }
    )+ };
}
single_id!(ChartSamplingQuery, SymbolInfoQuery, AuctionQuery);
macro_rules! short_code_id {
    ($($name:ident),+ $(,)?) => { $(
        #[derive(Clone, Debug)]
        pub struct $name { id: MacInstrumentId }
        impl $name {
            pub fn new(id: MacInstrumentId) -> Result<Self> {
                if id.code().encoded().len() > 8 { return Err(invalid("MAC 0x1218 query code exceeds eight GBK bytes")); }
                Ok(Self { id })
            }
            pub fn id(&self) -> &MacInstrumentId { &self.id }
            pub fn with_id(self, value: MacInstrumentId) -> Result<Self> { Self::new(value) }
        }
    )+ };
}
short_code_id!(BelongingBoardsQuery, CapitalFlowQuery);
#[derive(Clone, Debug)]
pub struct TickChartQuery {
    id: MacInstrumentId,
    date: Option<TradingDate>,
}
impl TickChartQuery {
    pub fn new(id: MacInstrumentId) -> Self {
        Self { id, date: None }
    }
    pub fn id(&self) -> &MacInstrumentId {
        &self.id
    }
    pub const fn date(&self) -> Option<TradingDate> {
        self.date
    }
    pub fn with_id(mut self, value: MacInstrumentId) -> Self {
        self.id = value;
        self
    }
    pub fn with_date(mut self, value: Option<TradingDate>) -> Result<Self> {
        date_wire(value)?;
        self.date = value;
        Ok(self)
    }
}
#[derive(Clone, Debug)]
pub struct TickChartsQuery {
    chart: TickChartQuery,
    days: u8,
}
impl TickChartsQuery {
    pub fn new(id: MacInstrumentId) -> Self {
        Self {
            chart: TickChartQuery::new(id),
            days: 5,
        }
    }
    pub fn id(&self) -> &MacInstrumentId {
        self.chart.id()
    }
    pub const fn date(&self) -> Option<TradingDate> {
        self.chart.date()
    }
    pub const fn days(&self) -> u8 {
        self.days
    }
    pub fn with_id(mut self, value: MacInstrumentId) -> Self {
        self.chart = self.chart.with_id(value);
        self
    }
    pub fn with_date(mut self, value: Option<TradingDate>) -> Result<Self> {
        self.chart = self.chart.with_date(value)?;
        Ok(self)
    }
    pub fn with_days(mut self, value: u8) -> Result<Self> {
        if !(1..=5).contains(&value) {
            return Err(invalid("MAC tick charts requires 1..=5 days"));
        }
        self.days = value;
        Ok(self)
    }
}
#[derive(Clone, Debug)]
pub struct TransactionsQuery {
    chart: TickChartQuery,
    start: u32,
    count: NonZeroU32,
    pages: PageOptions,
}
impl TransactionsQuery {
    #[allow(
        clippy::expect_used,
        reason = "Only fixed defaults or closed built-in field tables are unwrapped here."
    )]
    pub fn new(id: MacInstrumentId) -> Self {
        Self {
            chart: TickChartQuery::new(id),
            start: 0,
            count: NonZeroU32::new(2000).expect("positive"),
            pages: page_defaults(1000, 100, 100000),
        }
    }
    pub fn id(&self) -> &MacInstrumentId {
        self.chart.id()
    }
    pub const fn date(&self) -> Option<TradingDate> {
        self.chart.date()
    }
    pub const fn start(&self) -> u32 {
        self.start
    }
    pub const fn count(&self) -> NonZeroU32 {
        self.count
    }
    pub fn pages(&self) -> &PageOptions {
        &self.pages
    }
    pub fn with_id(mut self, value: MacInstrumentId) -> Self {
        self.chart = self.chart.with_id(value);
        self
    }
    pub fn with_date(mut self, value: Option<TradingDate>) -> Result<Self> {
        self.chart = self.chart.with_date(value)?;
        Ok(self)
    }
    pub fn with_start(mut self, value: u32) -> Result<Self> {
        span(value, self.count)?;
        self.start = value;
        Ok(self)
    }
    pub fn with_count(mut self, value: u32) -> Result<Self> {
        let value = count(value)?;
        span(self.start, value)?;
        self.count = value;
        Ok(self)
    }
    pub fn with_pages(mut self, value: PageOptions) -> Result<Self> {
        validate_pages(&value, 1000)?;
        self.pages = value;
        Ok(self)
    }
}
#[derive(Clone, Debug)]
pub struct BoardsQuery {
    board_type: BoardType,
    count: NonZeroU32,
    pages: PageOptions,
}
impl Default for BoardsQuery {
    #[allow(
        clippy::expect_used,
        reason = "Only fixed defaults or closed built-in field tables are unwrapped here."
    )]
    fn default() -> Self {
        Self {
            board_type: BoardType::All,
            count: NonZeroU32::new(10000).expect("positive"),
            pages: page_defaults(150, 2000, 160000),
        }
    }
}
impl BoardsQuery {
    pub fn new() -> Self {
        Self::default()
    }
    pub const fn board_type(&self) -> BoardType {
        self.board_type
    }
    pub const fn count(&self) -> NonZeroU32 {
        self.count
    }
    pub fn pages(&self) -> &PageOptions {
        &self.pages
    }
    pub fn with_board_type(mut self, value: BoardType) -> Self {
        self.board_type = value;
        self
    }
    pub fn with_count(mut self, value: u32) -> Result<Self> {
        self.count = count(value)?;
        Ok(self)
    }
    pub fn with_pages(mut self, value: PageOptions) -> Result<Self> {
        validate_pages(&value, 150)?;
        self.pages = value;
        Ok(self)
    }
}
#[derive(Clone, Debug)]
pub struct BoardMembersQuery {
    board: BoardSymbol,
    start: u32,
    count: NonZeroU32,
    sort: SortType,
    order: SortOrder,
    fields: FieldSelection,
    exclude: FilterMask,
    pages: PageOptions,
}
impl BoardMembersQuery {
    #[allow(
        clippy::expect_used,
        reason = "Only fixed defaults or closed built-in field tables are unwrapped here."
    )]
    pub fn new(board: BoardSymbol) -> Self {
        Self {
            board,
            start: 0,
            count: NonZeroU32::new(100000).expect("positive"),
            sort: SortType::ChangePct,
            order: SortOrder::Desc,
            fields: FieldSelection::preset(PresetField::Common),
            exclude: FilterMask::empty(),
            pages: page_defaults(80, 2000, 160000),
        }
    }
    pub fn board(&self) -> &BoardSymbol {
        &self.board
    }
    pub const fn start(&self) -> u32 {
        self.start
    }
    pub const fn count(&self) -> NonZeroU32 {
        self.count
    }
    pub const fn sort(&self) -> SortType {
        self.sort
    }
    pub const fn order(&self) -> SortOrder {
        self.order
    }
    pub const fn exclude(&self) -> FilterMask {
        self.exclude
    }
    pub fn fields(&self) -> &FieldSelection {
        &self.fields
    }
    pub fn pages(&self) -> &PageOptions {
        &self.pages
    }
    pub fn with_board(mut self, value: BoardSymbol) -> Self {
        self.board = value;
        self
    }
    pub fn with_start(mut self, value: u32) -> Result<Self> {
        span(value, self.count)?;
        self.start = value;
        Ok(self)
    }
    pub fn with_count(mut self, value: u32) -> Result<Self> {
        let value = count(value)?;
        span(self.start, value)?;
        self.count = value;
        Ok(self)
    }
    pub fn with_sort(mut self, value: SortType) -> Self {
        self.sort = value;
        self
    }
    pub fn with_order(mut self, value: SortOrder) -> Self {
        self.order = value;
        self
    }
    pub fn with_exclude(mut self, value: FilterMask) -> Self {
        self.exclude = value;
        self
    }
    pub fn with_fields(mut self, value: FieldSelection) -> Result<Self> {
        fields(&value, BitmapLayout::Board16)?;
        self.fields = value;
        Ok(self)
    }
    pub fn with_pages(mut self, value: PageOptions) -> Result<Self> {
        validate_pages(&value, 80)?;
        self.pages = value;
        Ok(self)
    }
}
#[derive(Clone, Debug)]
pub struct BoardSummaryQuery {
    board: BoardSymbol,
    sort: SortType,
    order: SortOrder,
    pages: PageOptions,
}
impl BoardSummaryQuery {
    pub fn new(board: BoardSymbol) -> Self {
        Self {
            board,
            sort: SortType::ChangePct,
            order: SortOrder::Desc,
            pages: page_defaults(80, 2000, 160000),
        }
    }
    pub fn board(&self) -> &BoardSymbol {
        &self.board
    }
    pub const fn sort(&self) -> SortType {
        self.sort
    }
    pub const fn order(&self) -> SortOrder {
        self.order
    }
    pub fn pages(&self) -> &PageOptions {
        &self.pages
    }
    pub fn with_board(mut self, value: BoardSymbol) -> Self {
        self.board = value;
        self
    }
    pub fn with_sort(mut self, value: SortType) -> Self {
        self.sort = value;
        self
    }
    pub fn with_order(mut self, value: SortOrder) -> Self {
        self.order = value;
        self
    }
    pub fn with_pages(mut self, value: PageOptions) -> Result<Self> {
        validate_pages(&value, 80)?;
        self.pages = value;
        Ok(self)
    }
}
#[derive(Clone, Debug)]
pub struct BoardRankingQuery {
    board_type: BoardType,
    top_n: Option<NonZeroU32>,
    sort: BoardRankKey,
    ascending: bool,
    pages: PageOptions,
    member_pages: PageOptions,
}
impl Default for BoardRankingQuery {
    fn default() -> Self {
        Self {
            board_type: BoardType::Hy,
            top_n: NonZeroU32::new(50),
            sort: BoardRankKey::ChangePct,
            ascending: false,
            pages: page_defaults(150, 2000, 160000),
            member_pages: page_defaults(80, 2000, 160000),
        }
    }
}
impl BoardRankingQuery {
    pub fn new() -> Self {
        Self::default()
    }
    pub const fn board_type(&self) -> BoardType {
        self.board_type
    }
    pub const fn top_n(&self) -> Option<NonZeroU32> {
        self.top_n
    }
    pub const fn sort(&self) -> BoardRankKey {
        self.sort
    }
    pub const fn ascending(&self) -> bool {
        self.ascending
    }
    pub fn pages(&self) -> &PageOptions {
        &self.pages
    }
    pub fn member_pages(&self) -> &PageOptions {
        &self.member_pages
    }
    pub fn with_board_type(mut self, value: BoardType) -> Self {
        self.board_type = value;
        self
    }
    /// None retains every candidate after sorting, within the composite limits.
    pub fn with_top_n(mut self, value: Option<u32>) -> Result<Self> {
        self.top_n = value.map(count).transpose()?;
        Ok(self)
    }
    pub fn with_sort(mut self, value: BoardRankKey) -> Self {
        self.sort = value;
        self
    }
    pub fn with_ascending(mut self, value: bool) -> Self {
        self.ascending = value;
        self
    }
    pub fn with_pages(mut self, value: PageOptions) -> Result<Self> {
        validate_pages(&value, 150)?;
        self.pages = value;
        Ok(self)
    }
    pub fn with_member_pages(mut self, value: PageOptions) -> Result<Self> {
        validate_pages(&value, 80)?;
        self.member_pages = value;
        Ok(self)
    }
}
#[derive(Clone, Debug)]
pub struct BoardChangeRankingQuery {
    board_type: BoardType,
    target_date: Option<TradingDate>,
    days: NonZeroU32,
    top_n: Option<NonZeroU32>,
    ascending: bool,
    pages: PageOptions,
    history_pages: PageOptions,
}
impl Default for BoardChangeRankingQuery {
    #[allow(
        clippy::expect_used,
        reason = "Only fixed defaults or closed built-in field tables are unwrapped here."
    )]
    fn default() -> Self {
        Self {
            board_type: BoardType::Hy,
            target_date: None,
            days: NonZeroU32::new(20).expect("positive"),
            top_n: None,
            ascending: false,
            pages: page_defaults(150, 2000, 160000),
            history_pages: page_defaults(700, 200, 140000),
        }
    }
}
impl BoardChangeRankingQuery {
    pub fn new() -> Self {
        Self::default()
    }
    pub const fn board_type(&self) -> BoardType {
        self.board_type
    }
    pub const fn target_date(&self) -> Option<TradingDate> {
        self.target_date
    }
    pub const fn days(&self) -> NonZeroU32 {
        self.days
    }
    pub const fn top_n(&self) -> Option<NonZeroU32> {
        self.top_n
    }
    pub const fn ascending(&self) -> bool {
        self.ascending
    }
    pub fn pages(&self) -> &PageOptions {
        &self.pages
    }
    pub fn history_pages(&self) -> &PageOptions {
        &self.history_pages
    }
    pub fn with_board_type(mut self, value: BoardType) -> Self {
        self.board_type = value;
        self
    }
    pub fn with_target_date(mut self, value: Option<TradingDate>) -> Result<Self> {
        date_wire(value)?;
        self.target_date = value;
        Ok(self)
    }
    pub fn with_days(mut self, value: u32) -> Result<Self> {
        value
            .checked_add(10)
            .ok_or_else(|| invalid("MAC board history day window plus buffer exceeds u32"))?;
        self.days = count(value)?;
        Ok(self)
    }
    pub fn with_top_n(mut self, value: Option<u32>) -> Result<Self> {
        self.top_n = value.map(count).transpose()?;
        Ok(self)
    }
    pub fn with_ascending(mut self, value: bool) -> Self {
        self.ascending = value;
        self
    }
    pub fn with_pages(mut self, value: PageOptions) -> Result<Self> {
        validate_pages(&value, 150)?;
        self.pages = value;
        Ok(self)
    }
    pub fn with_history_pages(mut self, value: PageOptions) -> Result<Self> {
        validate_pages(&value, 700)?;
        self.history_pages = value;
        Ok(self)
    }
}
#[derive(Clone, Debug)]
pub struct UnusualQuery {
    market: MacMarketId,
    start: u32,
    count: NonZeroU32,
    pages: PageOptions,
}
impl UnusualQuery {
    #[allow(
        clippy::expect_used,
        reason = "Only fixed defaults or closed built-in field tables are unwrapped here."
    )]
    pub fn new(market: MacMarketId) -> Self {
        Self {
            market,
            start: 0,
            count: NonZeroU32::new(600).expect("positive"),
            pages: page_defaults(600, 200, 120000),
        }
    }
    pub const fn market(&self) -> MacMarketId {
        self.market
    }
    pub const fn start(&self) -> u32 {
        self.start
    }
    pub const fn count(&self) -> NonZeroU32 {
        self.count
    }
    pub fn pages(&self) -> &PageOptions {
        &self.pages
    }
    pub fn with_market(mut self, value: MacMarketId) -> Self {
        self.market = value;
        self
    }
    pub fn with_start(mut self, value: u32) -> Result<Self> {
        u16::try_from(value).map_err(|_| invalid("MAC unusual start exceeds u16 wire field"))?;
        span(value, self.count)?;
        self.start = value;
        Ok(self)
    }
    pub fn with_count(mut self, value: u32) -> Result<Self> {
        let value = count(value)?;
        span(self.start, value)?;
        self.count = value;
        Ok(self)
    }
    pub fn with_pages(mut self, value: PageOptions) -> Result<Self> {
        validate_pages(&value, 600)?;
        self.pages = value;
        Ok(self)
    }
}
#[derive(Clone, Debug)]
pub struct KlineOffsetsQuery {
    offset: u32,
    count: NonZeroU32,
}
impl Default for KlineOffsetsQuery {
    #[allow(
        clippy::expect_used,
        reason = "Only fixed defaults or closed built-in field tables are unwrapped here."
    )]
    fn default() -> Self {
        Self {
            offset: 0,
            count: NonZeroU32::new(128000).expect("positive"),
        }
    }
}
impl KlineOffsetsQuery {
    pub fn new() -> Self {
        Self::default()
    }
    pub const fn offset(&self) -> u32 {
        self.offset
    }
    pub const fn count(&self) -> NonZeroU32 {
        self.count
    }
    pub fn with_offset(mut self, value: u32) -> Result<Self> {
        span(value, self.count)?;
        self.offset = value;
        Ok(self)
    }
    pub fn with_count(mut self, value: u32) -> Result<Self> {
        let value = count(value)?;
        span(self.offset, value)?;
        self.count = value;
        Ok(self)
    }
}
#[derive(Clone, Debug)]
pub struct FileMetaQuery {
    filename: RemoteFileName,
    offset: u32,
}
impl FileMetaQuery {
    pub fn new(filename: RemoteFileName) -> Self {
        Self {
            filename,
            offset: 0,
        }
    }
    pub fn filename(&self) -> &RemoteFileName {
        &self.filename
    }
    pub const fn offset(&self) -> u32 {
        self.offset
    }
    pub fn with_filename(mut self, value: RemoteFileName) -> Self {
        self.filename = value;
        self
    }
    pub fn with_offset(mut self, value: u32) -> Self {
        self.offset = value;
        self
    }
}
#[derive(Clone, Debug)]
pub struct DownloadChunkQuery {
    filename: RemoteFileName,
    index: NonZeroU32,
    offset: u32,
    size: NonZeroU32,
}
impl DownloadChunkQuery {
    #[allow(
        clippy::expect_used,
        reason = "Only fixed defaults or closed built-in field tables are unwrapped here."
    )]
    pub fn new(filename: RemoteFileName) -> Self {
        Self {
            filename,
            index: NonZeroU32::MIN,
            offset: 0,
            size: NonZeroU32::new(30000).expect("positive"),
        }
    }
    pub fn filename(&self) -> &RemoteFileName {
        &self.filename
    }
    pub const fn index(&self) -> NonZeroU32 {
        self.index
    }
    pub const fn offset(&self) -> u32 {
        self.offset
    }
    pub const fn size(&self) -> NonZeroU32 {
        self.size
    }
    pub fn with_filename(mut self, value: RemoteFileName) -> Self {
        self.filename = value;
        self
    }
    pub fn with_index(mut self, value: u32) -> Result<Self> {
        self.index =
            NonZeroU32::new(value).ok_or_else(|| invalid("MAC file chunk index starts at one"))?;
        Ok(self)
    }
    pub fn with_offset(mut self, value: u32) -> Result<Self> {
        span(value, self.size)?;
        self.offset = value;
        Ok(self)
    }
    pub fn with_size(mut self, value: u32) -> Result<Self> {
        if value > 30000 {
            return Err(invalid("MAC file chunk size exceeds 30000"));
        }
        let value = count(value)?;
        span(self.offset, value)?;
        self.size = value;
        Ok(self)
    }
}
#[derive(Clone, Debug)]
pub struct DownloadFileQuery {
    filename: RemoteFileName,
    options: DownloadOptions,
}
impl DownloadFileQuery {
    pub fn new(filename: RemoteFileName) -> Self {
        Self {
            filename,
            options: DownloadOptions::default(),
        }
    }
    pub fn filename(&self) -> &RemoteFileName {
        &self.filename
    }
    pub fn options(&self) -> &DownloadOptions {
        &self.options
    }
    pub fn with_filename(mut self, value: RemoteFileName) -> Self {
        self.filename = value;
        self
    }
    pub fn with_options(mut self, value: DownloadOptions) -> Result<Self> {
        if value.chunk_size().get() > 30000
            || value.max_chunks().get() > 4096
            || value
                .start_offset()
                .checked_add(value.max_bytes())
                .is_none_or(|end| end > u64::from(u32::MAX))
            || value
                .expected_size()
                .is_some_and(|size| size > u64::from(u32::MAX))
        {
            return Err(invalid(
                "MAC download policy exceeds chunk or u32 source bounds",
            ));
        }
        self.options = value;
        Ok(self)
    }
}
#[derive(Clone, Debug)]
pub struct GoodsQuery {
    market: MacMarketId,
    start: u32,
    count: NonZeroU32,
}
impl GoodsQuery {
    #[allow(
        clippy::expect_used,
        reason = "Only fixed defaults or closed built-in field tables are unwrapped here."
    )]
    pub fn new(market: MacMarketId) -> Self {
        Self {
            market,
            start: 0,
            count: NonZeroU32::new(600).expect("positive"),
        }
    }
    pub const fn market(&self) -> MacMarketId {
        self.market
    }
    pub const fn start(&self) -> u32 {
        self.start
    }
    pub const fn count(&self) -> NonZeroU32 {
        self.count
    }
    pub fn with_market(mut self, value: MacMarketId) -> Self {
        self.market = value;
        self
    }
    pub fn with_start(mut self, value: u32) -> Result<Self> {
        span(value, self.count)?;
        self.start = value;
        Ok(self)
    }
    pub fn with_count(mut self, value: u32) -> Result<Self> {
        if value > 1000 {
            return Err(invalid("MAC goods page count exceeds 1000"));
        }
        let value = count(value)?;
        span(self.start, value)?;
        self.count = value;
        Ok(self)
    }
}
