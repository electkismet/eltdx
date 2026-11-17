//! MAC-EX queries use the canonical EX identity namespace. Wire encoding and
//! shared MAC validation stay in `mac::query` and `mac::commands`.
use super::bitmap::{encode_bitmap, BitmapLayout, FieldSelection};
use super::enums::{QuoteListOrder, SortOrder, SortType};
use super::models::{MacAdjust, MacInstrumentId, MacPeriod};
use super::query::{self as common, date_wire, invalid, page_defaults, span, validate_pages};
use crate::ex::{ExCode, ExInstrumentId, ExMarketId};
use crate::types::BarTime;
use crate::{PageOptions, Result, TradingDate};
use std::num::NonZeroU32;

fn positive(value: u32) -> Result<NonZeroU32> {
    NonZeroU32::new(value).ok_or_else(|| invalid("MAC-EX total target must be positive"))
}
fn identity(value: &ExInstrumentId) -> Result<MacInstrumentId> {
    MacInstrumentId::try_from(value)
}
fn hk(market: ExMarketId) -> bool {
    matches!(market.get(), 27 | 31 | 48 | 49 | 71 | 98)
}
fn transaction_span(id: &ExInstrumentId, start: u32, count: NonZeroU32) -> Result<()> {
    span(start, count)?;
    if hk(id.market())
        && start
            .checked_add(count.get())
            .is_none_or(|value| value > i32::MAX as u32)
    {
        return Err(invalid(
            "MAC-EX HK transaction offsets exceed the signed EX wire field",
        ));
    }
    Ok(())
}
fn directory_pages(value: &PageOptions) -> Result<()> {
    validate_pages(value, 1000)?;
    if value.max_pages() > 2000 || value.max_records() > 1_200_000 {
        return Err(invalid(
            "MAC-EX directory scan exceeds 2000 pages or 1200000 entries",
        ));
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct QuotesQuery {
    ids: Vec<ExInstrumentId>,
    wire_ids: Vec<MacInstrumentId>,
    fields: FieldSelection,
}
impl QuotesQuery {
    pub fn new(ids: Vec<ExInstrumentId>) -> Result<Self> {
        if ids.is_empty() || ids.len() > 80 {
            return Err(invalid(
                "MAC-EX quotes requires 1..=80 identity occurrences",
            ));
        }
        let wire_ids = ids.iter().map(identity).collect::<Result<Vec<_>>>()?;
        Ok(Self {
            ids,
            wire_ids,
            fields: FieldSelection::mac_ex_default(),
        })
    }
    pub fn ids(&self) -> &[ExInstrumentId] {
        &self.ids
    }
    pub fn fields(&self) -> &FieldSelection {
        &self.fields
    }
    pub(crate) fn wire_ids(&self) -> &[MacInstrumentId] {
        &self.wire_ids
    }
    pub fn with_ids(mut self, value: Vec<ExInstrumentId>) -> Result<Self> {
        let replacement = Self::new(value)?;
        self.ids = replacement.ids;
        self.wire_ids = replacement.wire_ids;
        Ok(self)
    }
    pub fn with_fields(mut self, value: FieldSelection) -> Result<Self> {
        encode_bitmap(&value, BitmapLayout::Quote20)?;
        self.fields = value;
        Ok(self)
    }
}

/// A source-order listing deliberately requests at most 80 goods, even when
/// the public count is 100. Completion metadata must expose that distinction.
#[derive(Clone, Debug)]
pub struct QuoteListQuery {
    market: ExMarketId,
    start: u32,
    count: NonZeroU32,
    order: QuoteListOrder,
    pages: PageOptions,
}
impl QuoteListQuery {
    #[allow(
        clippy::expect_used,
        reason = "Only fixed defaults or closed built-in field tables are unwrapped here."
    )]
    pub fn new(market: ExMarketId) -> Self {
        Self {
            market,
            start: 0,
            count: NonZeroU32::new(100).expect("positive"),
            order: QuoteListOrder::SourceOrder,
            pages: page_defaults(600, 2000, 1_200_000),
        }
    }
    pub const fn market(&self) -> ExMarketId {
        self.market
    }
    pub const fn start(&self) -> u32 {
        self.start
    }
    pub const fn count(&self) -> NonZeroU32 {
        self.count
    }
    pub const fn order(&self) -> QuoteListOrder {
        self.order
    }
    pub fn pages(&self) -> &PageOptions {
        &self.pages
    }
    pub const fn requested_goods(&self) -> u32 {
        if self.count.get() < 80 {
            self.count.get()
        } else {
            80
        }
    }
    pub fn with_market(mut self, value: ExMarketId) -> Self {
        self.market = value;
        self
    }
    pub fn with_start(mut self, value: u32) -> Result<Self> {
        span(value, self.count)?;
        self.start = value;
        Ok(self)
    }
    pub fn with_count(mut self, value: u32) -> Result<Self> {
        let value = positive(value)?;
        span(self.start, value)?;
        self.count = value;
        Ok(self)
    }
    pub fn with_order(mut self, value: QuoteListOrder) -> Self {
        self.order = value;
        self
    }
    /// Only the equivalent source-order pair is implemented. Accepting a
    /// ChangePct request here would falsely promise server-side sorting.
    pub fn with_sort(self, sort: SortType, order: SortOrder) -> Result<Self> {
        if sort != SortType::Code || order != SortOrder::None {
            return Err(invalid("MAC-EX quote_list supports only source order"));
        }
        Ok(self)
    }
    pub fn with_pages(mut self, value: PageOptions) -> Result<Self> {
        directory_pages(&value)?;
        self.pages = value;
        Ok(self)
    }
}

#[derive(Clone, Debug)]
pub struct BarsQuery {
    id: ExInstrumentId,
    common: common::BarsQuery,
}
impl BarsQuery {
    pub fn new(id: ExInstrumentId) -> Result<Self> {
        let common = common::BarsQuery::new(identity(&id)?);
        Ok(Self { id, common })
    }
    pub fn id(&self) -> &ExInstrumentId {
        &self.id
    }
    pub const fn period(&self) -> MacPeriod {
        self.common.period()
    }
    pub const fn times(&self) -> std::num::NonZeroU16 {
        self.common.times()
    }
    pub const fn start(&self) -> u32 {
        self.common.start()
    }
    pub const fn count(&self) -> NonZeroU32 {
        self.common.count()
    }
    pub const fn adjust(&self) -> MacAdjust {
        self.common.adjust()
    }
    pub const fn bar_time(&self) -> BarTime {
        self.common.bar_time()
    }
    pub fn pages(&self) -> &PageOptions {
        self.common.pages()
    }
    pub const fn strict_adjustment(&self) -> bool {
        self.common.strict_adjustment()
    }
    pub(crate) fn common(&self) -> &common::BarsQuery {
        &self.common
    }
    pub fn with_id(mut self, value: ExInstrumentId) -> Result<Self> {
        self.common = self.common.with_id(identity(&value)?);
        self.id = value;
        Ok(self)
    }
    pub fn with_period(mut self, value: MacPeriod) -> Result<Self> {
        self.common = self.common.with_period(value)?;
        Ok(self)
    }
    pub fn with_times(mut self, value: u32) -> Result<Self> {
        self.common = self.common.with_times(value)?;
        Ok(self)
    }
    pub fn with_start(mut self, value: u32) -> Result<Self> {
        self.common = self.common.with_start(value)?;
        Ok(self)
    }
    pub fn with_count(mut self, value: u32) -> Result<Self> {
        self.common = self.common.with_count(value)?;
        Ok(self)
    }
    pub fn with_adjust(mut self, value: MacAdjust) -> Self {
        self.common = self.common.with_adjust(value);
        self
    }
    pub fn with_bar_time(mut self, value: BarTime) -> Result<Self> {
        self.common = self.common.with_bar_time(value)?;
        Ok(self)
    }
    pub fn with_pages(mut self, value: PageOptions) -> Result<Self> {
        self.common = self.common.with_pages(value)?;
        Ok(self)
    }
    pub fn with_strict_adjustment(mut self, value: bool) -> Self {
        self.common = self.common.with_strict_adjustment(value);
        self
    }
}
#[derive(Clone, Debug)]
pub struct TickChartQuery {
    id: ExInstrumentId,
    common: common::TickChartQuery,
}
impl TickChartQuery {
    pub fn new(id: ExInstrumentId) -> Result<Self> {
        let common = common::TickChartQuery::new(identity(&id)?);
        Ok(Self { id, common })
    }
    pub fn id(&self) -> &ExInstrumentId {
        &self.id
    }
    pub const fn date(&self) -> Option<TradingDate> {
        self.common.date()
    }
    pub(crate) fn common(&self) -> &common::TickChartQuery {
        &self.common
    }
    pub fn with_id(mut self, value: ExInstrumentId) -> Result<Self> {
        self.common = self.common.with_id(identity(&value)?);
        self.id = value;
        Ok(self)
    }
    pub fn with_date(mut self, value: Option<TradingDate>) -> Result<Self> {
        self.common = self.common.with_date(value)?;
        Ok(self)
    }
}
#[derive(Clone, Debug)]
pub struct ChartSamplingQuery {
    id: ExInstrumentId,
    common: common::ChartSamplingQuery,
}
impl ChartSamplingQuery {
    pub fn new(id: ExInstrumentId) -> Result<Self> {
        let common = common::ChartSamplingQuery::new(identity(&id)?);
        Ok(Self { id, common })
    }
    pub fn id(&self) -> &ExInstrumentId {
        &self.id
    }
    pub(crate) fn common(&self) -> &common::ChartSamplingQuery {
        &self.common
    }
    pub fn with_id(mut self, value: ExInstrumentId) -> Result<Self> {
        self.common = self.common.with_id(identity(&value)?);
        self.id = value;
        Ok(self)
    }
}

#[derive(Clone, Debug)]
pub struct TransactionsQuery {
    id: ExInstrumentId,
    wire_id: MacInstrumentId,
    date: Option<TradingDate>,
    start: u32,
    count: NonZeroU32,
    pages: PageOptions,
}
impl TransactionsQuery {
    #[allow(
        clippy::expect_used,
        reason = "Only fixed defaults or closed built-in field tables are unwrapped here."
    )]
    pub fn new(id: ExInstrumentId) -> Result<Self> {
        let wire_id = identity(&id)?;
        let size = if hk(id.market()) { 1800 } else { 1000 };
        Ok(Self {
            id,
            wire_id,
            date: None,
            start: 0,
            count: NonZeroU32::new(2000).expect("positive"),
            pages: page_defaults(size, 100, 100000),
        })
    }
    pub fn id(&self) -> &ExInstrumentId {
        &self.id
    }
    pub const fn date(&self) -> Option<TradingDate> {
        self.date
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
    pub const fn wire_page_maximum(&self) -> u16 {
        if matches!(self.id.market().get(), 27 | 31 | 48 | 49 | 71 | 98) {
            1800
        } else {
            1000
        }
    }
    pub fn with_id(mut self, value: ExInstrumentId) -> Result<Self> {
        transaction_span(&value, self.start, self.count)?;
        let wire_id = identity(&value)?;
        let maximum = if hk(value.market()) { 1800 } else { 1000 };
        if self.pages.page_size() > maximum {
            return Err(invalid(
                "new MAC-EX transaction market requires a smaller explicit page size",
            ));
        }
        self.id = value;
        self.wire_id = wire_id;
        Ok(self)
    }
    pub fn with_date(mut self, value: Option<TradingDate>) -> Result<Self> {
        date_wire(value)?;
        self.date = value;
        Ok(self)
    }
    pub fn with_start(mut self, value: u32) -> Result<Self> {
        transaction_span(&self.id, value, self.count)?;
        self.start = value;
        Ok(self)
    }
    pub fn with_count(mut self, value: u32) -> Result<Self> {
        let value = positive(value)?;
        transaction_span(&self.id, self.start, value)?;
        self.count = value;
        Ok(self)
    }
    pub fn with_pages(mut self, value: PageOptions) -> Result<Self> {
        validate_pages(&value, usize::from(self.wire_page_maximum()))?;
        self.pages = value;
        Ok(self)
    }
}

#[derive(Clone, Debug)]
pub struct AllTransactionsQuery {
    id: ExInstrumentId,
    date: Option<TradingDate>,
    start: u32,
    pages: PageOptions,
}
impl AllTransactionsQuery {
    pub fn new(id: ExInstrumentId) -> Result<Self> {
        if !hk(id.market()) {
            return Err(invalid(
                "MAC-EX all_transactions supports only HK stock markets 27/31/48/49/71/98",
            ));
        }
        identity(&id)?;
        Ok(Self {
            id,
            date: None,
            start: 0,
            pages: page_defaults(1800, 50, 90000),
        })
    }
    pub fn id(&self) -> &ExInstrumentId {
        &self.id
    }
    pub const fn date(&self) -> Option<TradingDate> {
        self.date
    }
    pub const fn start(&self) -> u32 {
        self.start
    }
    pub fn pages(&self) -> &PageOptions {
        &self.pages
    }
    pub fn with_id(self, value: ExInstrumentId) -> Result<Self> {
        Self::new(value)?
            .with_date(self.date)?
            .with_pages(self.pages)?
            .with_start(self.start)
    }
    pub fn with_date(mut self, value: Option<TradingDate>) -> Result<Self> {
        date_wire(value)?;
        self.date = value;
        Ok(self)
    }
    pub fn with_start(mut self, value: u32) -> Result<Self> {
        let capacity = u32::try_from(self.pages.max_records())
            .map_err(|_| invalid("MAC-EX HK record cap exceeds u32"))?;
        if value
            .checked_add(capacity)
            .is_none_or(|end| end > i32::MAX as u32)
        {
            return Err(invalid(
                "MAC-EX HK all_transactions offset exceeds signed EX field",
            ));
        }
        self.start = value;
        Ok(self)
    }
    pub fn with_pages(mut self, value: PageOptions) -> Result<Self> {
        validate_pages(&value, 1800)?;
        if value.max_pages() > 50 || value.max_records() > 90000 {
            return Err(invalid(
                "MAC-EX HK all_transactions safety cap is 50 pages and 90000 rows",
            ));
        }
        self.pages = value;
        let start = self.start;
        self.with_start(start)
    }
}

#[derive(Clone, Debug, Default)]
pub struct GoodsCountQuery {
    market: Option<ExMarketId>,
    pages: Option<PageOptions>,
}
impl GoodsCountQuery {
    pub fn new() -> Self {
        Self::default()
    }
    pub const fn market(&self) -> Option<ExMarketId> {
        self.market
    }
    pub fn pages(&self) -> PageOptions {
        self.pages
            .clone()
            .unwrap_or_else(|| page_defaults(600, 2000, 1_200_000))
    }
    pub fn with_market(mut self, value: Option<ExMarketId>) -> Self {
        self.market = value;
        self
    }
    pub fn with_pages(mut self, value: PageOptions) -> Result<Self> {
        directory_pages(&value)?;
        self.pages = Some(value);
        Ok(self)
    }
}
macro_rules! directory_query {
    ($($name:ident),+ $(,)?) => { $(
        #[derive(Clone, Debug)]
        pub struct $name { market: ExMarketId, start: u32, count: NonZeroU32, pages: PageOptions }
        impl $name {
            #[allow(clippy::expect_used, reason = "Only fixed defaults or closed built-in field tables are unwrapped here.")]
            pub fn new(market: ExMarketId) -> Self { Self { market, start: 0, count: NonZeroU32::new(600).expect("positive"), pages: page_defaults(600,2000,1_200_000) } }
            pub const fn market(&self) -> ExMarketId { self.market }
            pub const fn start(&self) -> u32 { self.start }
            pub const fn count(&self) -> NonZeroU32 { self.count }
            pub fn pages(&self) -> &PageOptions { &self.pages }
            pub fn with_market(mut self, value: ExMarketId) -> Self { self.market = value; self }
            pub fn with_start(mut self, value: u32) -> Result<Self> { span(value,self.count)?; self.start = value; Ok(self) }
            pub fn with_count(mut self, value: u32) -> Result<Self> { let value = positive(value)?; span(self.start,value)?; self.count = value; Ok(self) }
            pub fn with_pages(mut self, value: PageOptions) -> Result<Self> { directory_pages(&value)?; self.pages = value; Ok(self) }
        }
    )+ };
}
directory_query!(GoodsQuery, InstrumentsQuery);

/// A full source code or a suffix to resolve exactly, with case preserved.
/// This value does not claim that an indicator exists before directory lookup.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MacroCode(String);
impl MacroCode {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        let value = value.trim();
        if value.is_empty() || value.len() > 512 || value.contains('\0') {
            return Err(invalid(
                "macro selector must contain 1..512 UTF-8 bytes without NUL",
            ));
        }
        if value.contains('_') {
            let code = ExCode::new(value)?;
            super::models::MacCode::new(code.as_str())?;
        }
        Ok(Self(value.into()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn is_full_code(&self) -> bool {
        self.0.contains('_')
    }
}
#[derive(Clone, Debug)]
pub struct MacroQuotesQuery {
    codes: Vec<MacroCode>,
    fields: FieldSelection,
}
impl MacroQuotesQuery {
    pub fn new(codes: Vec<MacroCode>) -> Result<Self> {
        if codes.is_empty() || codes.len() > 65535 {
            return Err(invalid("macro_quotes requires 1..=65535 occurrences"));
        }
        let bytes = codes
            .iter()
            .try_fold(0_usize, |total, code| {
                total.checked_add(1 + 4 + code.as_str().len())
            })
            .ok_or_else(|| invalid("macro_quotes normalized input size overflow"))?;
        if bytes > 2 * 1024 * 1024 {
            return Err(invalid("macro_quotes normalized identities exceed 2 MiB"));
        }
        Ok(Self {
            codes,
            fields: FieldSelection::mac_ex_default(),
        })
    }
    pub fn codes(&self) -> &[MacroCode] {
        &self.codes
    }
    pub fn fields(&self) -> &FieldSelection {
        &self.fields
    }
    pub fn with_codes(mut self, value: Vec<MacroCode>) -> Result<Self> {
        self.codes = Self::new(value)?.codes;
        Ok(self)
    }
    pub fn with_fields(mut self, value: FieldSelection) -> Result<Self> {
        encode_bitmap(&value, BitmapLayout::Quote20)?;
        self.fields = value;
        Ok(self)
    }
}
#[derive(Clone, Debug)]
pub struct MacroBarsQuery {
    code: MacroCode,
    bars: BarsQuery,
}
impl MacroBarsQuery {
    pub fn new(code: MacroCode) -> Result<Self> {
        // A checked full request ID is installed only after directory matching.
        // Long exact display names are valid selectors, never sent as codes.
        let id = ExInstrumentId::new(ExMarketId::from_wire(38), ExCode::new("_")?);
        let bars = BarsQuery::new(id)?.with_period(MacPeriod::Monthly)?;
        Ok(Self { code, bars })
    }
    pub fn code(&self) -> &MacroCode {
        &self.code
    }
    pub const fn period(&self) -> MacPeriod {
        self.bars.period()
    }
    pub const fn times(&self) -> std::num::NonZeroU16 {
        self.bars.times()
    }
    pub const fn start(&self) -> u32 {
        self.bars.start()
    }
    pub const fn count(&self) -> NonZeroU32 {
        self.bars.count()
    }
    pub const fn adjust(&self) -> MacAdjust {
        self.bars.adjust()
    }
    pub const fn bar_time(&self) -> BarTime {
        self.bars.bar_time()
    }
    pub fn pages(&self) -> &PageOptions {
        self.bars.pages()
    }
    pub const fn strict_adjustment(&self) -> bool {
        self.bars.strict_adjustment()
    }
    pub(crate) fn bars(&self) -> &BarsQuery {
        &self.bars
    }
    pub fn with_code(mut self, value: MacroCode) -> Result<Self> {
        self.code = value;
        Ok(self)
    }
    pub fn with_period(mut self, value: MacPeriod) -> Result<Self> {
        self.bars = self.bars.with_period(value)?;
        Ok(self)
    }
    pub fn with_times(mut self, value: u32) -> Result<Self> {
        self.bars = self.bars.with_times(value)?;
        Ok(self)
    }
    pub fn with_start(mut self, value: u32) -> Result<Self> {
        self.bars = self.bars.with_start(value)?;
        Ok(self)
    }
    pub fn with_count(mut self, value: u32) -> Result<Self> {
        self.bars = self.bars.with_count(value)?;
        Ok(self)
    }
    pub fn with_adjust(mut self, value: MacAdjust) -> Self {
        self.bars = self.bars.with_adjust(value);
        self
    }
    pub fn with_bar_time(mut self, value: BarTime) -> Result<Self> {
        self.bars = self.bars.with_bar_time(value)?;
        Ok(self)
    }
    pub fn with_pages(mut self, value: PageOptions) -> Result<Self> {
        self.bars = self.bars.with_pages(value)?;
        Ok(self)
    }
    pub fn with_strict_adjustment(mut self, value: bool) -> Self {
        self.bars = self.bars.with_strict_adjustment(value);
        self
    }
}
