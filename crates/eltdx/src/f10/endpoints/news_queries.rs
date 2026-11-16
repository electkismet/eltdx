//! Company, topic and market news requests.
use std::num::NonZeroU32;
use std::time::Instant;
use crate::{Error, ErrorKind, OperationOptions, Result, TimeoutPhase};
use crate::f10::{AshareCode, CallQuery, F10Client, JsonValue, NewsChannel, NewsChannelInfo, NewsHomeTab, RequestPayload, Response, SectionCode};
use crate::f10::values::{count, j, page, text};

macro_rules! stock_section_strings {
    ($name:ident, $method:literal, $default:literal, {$($field:ident / $setter:ident = $value:literal),* $(,)?}) => {
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct $name { code: AshareCode, section: SectionCode, $($field: String),* }
        impl $name {
            pub fn new(code: AshareCode) -> Self { Self { code, section: SectionCode::default_for($method, $default), $($field: $value.to_owned()),* } }
            pub fn code(&self) -> &AshareCode { &self.code }
            pub fn section(&self) -> &SectionCode { &self.section }
            pub fn with_section(mut self, value: SectionCode) -> Result<Self> { value.checked_for($method)?; self.section = value; Ok(self) }
            $(pub fn $field(&self) -> &str { &self.$field }
            pub fn $setter(mut self, value: impl Into<String>) -> Result<Self> { let value = value.into(); text(&value, true)?; self.$field = value; Ok(self) })*
        }
    };
}
stock_section_strings!(TopicCompareQuery, "topic_compare", "gndbzfsj", { topic_id / with_topic_id = "", sort_by / with_sort_by = "zdf" });
impl TopicCompareQuery {
    pub fn with_topic(code: AshareCode, topic_id: impl Into<String>) -> Result<Self> { Self::new(code).with_topic_id(topic_id) }
    pub fn to_call(&self) -> Result<CallQuery> {
        text(&self.topic_id, false)?;
        super::params("CWServ.tdxf10_gg_rdtc_gndb", vec![j(self.section.as_str()), j(self.code.as_str()), j(self.topic_id.as_str()), j(self.sort_by.as_str())])
    }
}
stock_section_strings!(TopicCompareFirstQuery, "topic_compare_first", "gndbzfsj", { sort_by / with_sort_by = "zdf" });

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompanyNewsQuery { code: AshareCode, section: SectionCode, keyword: String, rating: String, page: u32, page_size: NonZeroU32 }
impl CompanyNewsQuery {
    pub fn new(code: AshareCode) -> Self { Self { code, section: SectionCode::default_for("company_news", "gsyj"), keyword: String::new(), rating: "0".to_owned(), page: 1, page_size: NonZeroU32::new(20).unwrap_or(NonZeroU32::MIN) } }
    pub fn code(&self) -> &AshareCode { &self.code }
    pub fn section(&self) -> &SectionCode { &self.section }
    pub fn keyword(&self) -> &str { &self.keyword }
    pub fn rating(&self) -> &str { &self.rating }
    pub fn page(&self) -> u32 { self.page }
    pub fn page_size(&self) -> NonZeroU32 { self.page_size }
    pub fn with_section(mut self, value: SectionCode) -> Result<Self> { value.checked_for("company_news")?; self.section = value; Ok(self) }
    pub fn with_keyword(mut self, value: impl Into<String>) -> Result<Self> { let value = value.into(); text(&value, true)?; self.keyword = value; Ok(self) }
    pub fn with_rating(mut self, value: impl Into<String>) -> Result<Self> { let value = value.into(); text(&value, true)?; self.rating = value; Ok(self) }
    pub fn with_page(mut self, value: u32) -> Result<Self> { self.page = page(value)?; Ok(self) }
    pub fn with_page_size(mut self, value: NonZeroU32) -> Result<Self> { self.page_size = count(value)?; Ok(self) }
    pub fn to_call(&self) -> Result<CallQuery> {
        if self.section.as_str() == "finance_review" { return super::params("CWServ.tdxf10_gg_cwfx_cbdp", vec![j(self.code.as_str()), j(self.page.to_string())]); }
        super::params("CWServ.tdxf10_gg_gszx", vec![j(self.code.as_str()), j(self.section.as_str()), j(self.keyword.as_str()), j(self.rating.as_str()), j(self.page.to_string()), j(self.page_size.get().to_string())])
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DigestQuery { code: AshareCode, section: SectionCode, page: u32, page_size: NonZeroU32 }
impl DigestQuery {
    pub fn new(code: AshareCode) -> Self { Self { code, section: SectionCode::default_for("digest", "tips"), page: 1, page_size: NonZeroU32::new(20).unwrap_or(NonZeroU32::MIN) } }
    pub fn code(&self) -> &AshareCode { &self.code }
    pub fn section(&self) -> &SectionCode { &self.section }
    pub fn page(&self) -> u32 { self.page }
    pub fn page_size(&self) -> NonZeroU32 { self.page_size }
    pub fn with_section(mut self, value: SectionCode) -> Result<Self> { value.checked_for("digest")?; self.section = value; Ok(self) }
    pub fn with_page(mut self, value: u32) -> Result<Self> { self.page = page(value)?; Ok(self) }
    pub fn with_page_size(mut self, value: NonZeroU32) -> Result<Self> { self.page_size = count(value)?; Ok(self) }
    pub fn to_call(&self) -> Result<CallQuery> {
        if self.section.as_str() == "interaction" { return super::params("CWServ.tdxf10_gg_zxtstzzhd", vec![j(self.code.as_str()), j("1"), j("1"), j(self.page_size.get().to_string())]); }
        let wire = match self.section.as_str() { "tips" => "zxts", "profile" => "gsgy", "events" => "gsds", "news" => "gsxw", "announcements" => "gsgg", "reports" => "gsyj", "roadshows" => "lyhd", _ => unreachable!("closed digest registry") };
        super::params("CWServ.tdxf10_gg_zxts", vec![j(self.code.as_str()), j(wire), j("")])
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NorthboundHoldingQuery { code: AshareCode, section: SectionCode, filter_value: String, page: u32, page_size: NonZeroU32 }
impl NorthboundHoldingQuery {
    pub fn new(code: AshareCode) -> Self { Self { code, section: SectionCode::default_for("northbound_holding", "bszj"), filter_value: String::new(), page: 1, page_size: NonZeroU32::new(20).unwrap_or(NonZeroU32::MIN) } }
    pub fn code(&self) -> &AshareCode { &self.code }
    pub fn section(&self) -> &SectionCode { &self.section }
    pub fn filter_value(&self) -> &str { &self.filter_value }
    pub fn page(&self) -> u32 { self.page }
    pub fn page_size(&self) -> NonZeroU32 { self.page_size }
    pub fn with_section(mut self, value: SectionCode) -> Result<Self> { value.checked_for("northbound_holding")?; self.section = value; Ok(self) }
    pub fn with_filter_value(mut self, value: impl Into<String>) -> Result<Self> { let value = value.into(); text(&value, true)?; self.filter_value = value; Ok(self) }
    pub fn with_page(mut self, value: u32) -> Result<Self> { self.page = page(value)?; Ok(self) }
    pub fn with_page_size(mut self, value: NonZeroU32) -> Result<Self> { self.page_size = count(value)?; Ok(self) }
    pub fn to_call(&self) -> Result<CallQuery> {
        let p = self.page.to_string();
        super::params("CWServ.tdxf10_gg_zlcc", vec![j(self.code.as_str()), j(self.section.as_str()), j(self.filter_value.as_str()), j(p.clone()), j(p), j(self.page_size.get().to_string())])
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DetailQuery { detail_type: String, record_id: String }
impl DetailQuery {
    pub fn new(detail_type: impl Into<String>, record_id: impl Into<String>) -> Result<Self> {
        let detail_type = detail_type.into(); let record_id = record_id.into(); text(&detail_type, false)?; text(&record_id, false)?; Ok(Self { detail_type, record_id })
    }
    pub fn detail_type(&self) -> &str { &self.detail_type }
    pub fn record_id(&self) -> &str { &self.record_id }
    pub fn to_call(&self) -> Result<CallQuery> { super::params("CWServ.tdxf10_gg_idreq", vec![j(self.detail_type.as_str()), j(self.record_id.as_str())]) }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheListQuery { code: AshareCode, kind: String }
impl CacheListQuery {
    pub fn new(code: AshareCode) -> Self { Self { code, kind: "gg".to_owned() } }
    pub fn code(&self) -> &AshareCode { &self.code }
    pub fn kind(&self) -> &str { &self.kind }
    pub fn with_kind(mut self, value: impl Into<String>) -> Result<Self> {
        let value = value.into(); if !matches!(value.as_str(), "gg" | "xw" | "ly") { return Err(crate::f10::values::invalid("cache kind must be gg, xw or ly")); }
        self.kind = value; Ok(self)
    }
    pub fn to_call(&self) -> Result<CallQuery> { cache_call(&self.code, &self.kind) }
}
fn cache_call(code: &AshareCode, kind: &str) -> Result<CallQuery> {
    Ok(CallQuery::new(crate::f10::EntryName::new("CWSearch.tzx_rcache")?, RequestPayload::Body(JsonValue::Object(vec![
        ("action".to_owned(), j("get")), ("key".to_owned(), j(format!("{kind}:{}_{}", code.market().get(), code.as_str()))),
        ("bin".to_owned(), j("1")), ("qsid".to_owned(), j("tdx")),
    ]))))
}
macro_rules! cache_query { ($name:ident, $kind:literal) => {
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct $name { code: AshareCode }
    impl $name {
        pub fn new(code: AshareCode) -> Self { Self { code } }
        pub fn code(&self) -> &AshareCode { &self.code }
        pub fn to_call(&self) -> Result<CallQuery> { cache_call(&self.code, $kind) }
    }
}; }
cache_query!(AnnouncementsQuery, "gg"); cache_query!(NewsQuery, "xw"); cache_query!(RoadshowsQuery, "ly");

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarketNewsQuery { tab: NewsHomeTab, page: u32, page_size: NonZeroU32 }
impl Default for MarketNewsQuery { fn default() -> Self { Self { tab: NewsHomeTab::Headlines, page: 1, page_size: NonZeroU32::new(50).unwrap_or(NonZeroU32::MIN) } } }
impl MarketNewsQuery {
    pub fn new() -> Self { Self::default() }
    pub fn tab(&self) -> NewsHomeTab { self.tab }
    pub fn page(&self) -> u32 { self.page }
    pub fn page_size(&self) -> NonZeroU32 { self.page_size }
    pub fn with_tab(mut self, value: NewsHomeTab) -> Self { self.tab = value; self }
    pub fn with_page(mut self, value: u32) -> Result<Self> { self.page = page(value)?; Ok(self) }
    pub fn with_page_size(mut self, value: NonZeroU32) -> Result<Self> { self.page_size = count(value)?; Ok(self) }
    pub fn to_call(&self) -> Result<CallQuery> { super::params("CWServ.tdxi_zxxwsy", vec![j(self.tab.wire().to_string()), j(self.page.to_string()), j(self.page_size.get().to_string())]) }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelNewsQuery { channel: NewsChannel, page: u32, page_size: NonZeroU32, query1: String, keyword: String }
impl ChannelNewsQuery {
    pub fn new(channel: NewsChannel) -> Self { Self { channel, page: 1, page_size: NonZeroU32::new(50).unwrap_or(NonZeroU32::MIN), query1: "1".to_owned(), keyword: String::new() } }
    pub fn channel(&self) -> NewsChannel { self.channel }
    pub fn page(&self) -> u32 { self.page }
    pub fn page_size(&self) -> NonZeroU32 { self.page_size }
    pub fn query1(&self) -> &str { &self.query1 }
    pub fn keyword(&self) -> &str { &self.keyword }
    pub fn with_channel(mut self, value: NewsChannel) -> Self { self.channel = value; self }
    pub fn with_page(mut self, value: u32) -> Result<Self> { self.page = page(value)?; Ok(self) }
    pub fn with_page_size(mut self, value: NonZeroU32) -> Result<Self> { self.page_size = count(value)?; Ok(self) }
    pub fn with_query1(mut self, value: impl Into<String>) -> Result<Self> { let value = value.into(); text(&value, true)?; self.query1 = value; Ok(self) }
    pub fn with_keyword(mut self, value: impl Into<String>) -> Result<Self> { let value = value.into(); text(&value, true)?; self.keyword = value; Ok(self) }
    pub fn to_call(&self) -> Result<CallQuery> { super::params("CWServ.tdxi_zxxwnews", vec![j(self.query1.as_str()), j(self.channel.wire().to_string()), j(self.page.to_string()), j(self.page_size.get().to_string()), j(self.keyword.as_str())]) }
}

impl F10Client {
    pub fn news_channels(&self) -> Vec<NewsChannelInfo> {
        let mut channels = crate::f10::news::CHANNELS.to_vec(); channels.sort_by_key(NewsChannelInfo::id); channels
    }
    pub fn topic_compare_first(&self, query: &TopicCompareFirstQuery, options: &OperationOptions) -> Result<Response> {
        let context = self.context(Instant::now(), "f10.topic_compare_first", options)?;
        let first = self.endpoint_call(&super::reports::TopicIdsQuery::new(query.code.clone()).to_call()?, &context)?;
        let mut topic = None;
        if let Some(table) = first.first_table() { for row in table.rows() {
            context.check_boundary(TimeoutPhase::Decode)?;
            if let Some(cell) = row.get_all("t001").next() {
                topic = match cell.value() { JsonValue::String(value) if !value.is_empty() => Some(value.clone()), JsonValue::Number(value) => Some(value.lexeme().to_owned()), _ => None };
                break;
            }
        } }
        let topic = topic.ok_or_else(|| Error::new(ErrorKind::MetadataUnavailable, "no topic id for topic comparison")
            .with_context("server_error_code", first.error_code().map_or_else(|| "absent".to_owned(), |value| value.to_string())))?;
        let call = super::params("CWServ.tdxf10_gg_rdtc_gndb", vec![j(query.section.as_str()), j(query.code.as_str()), j(topic), j(query.sort_by.as_str())])?;
        let response = self.endpoint_call(&call, &context)?.with_prerequisite(&first);
        context.finish()?; Ok(response)
    }
}
macro_rules! execute_queries { ($($method:ident: $query:ident),* $(,)?) => { impl F10Client { $(
    pub fn $method(&self, query: &$query, options: &OperationOptions) -> Result<Response> {
        let response = self.endpoint(concat!("f10.", stringify!($method)), Instant::now(), || query.to_call(), options)?;
        Ok(response)
    }
)* } }; }
execute_queries!(topic_compare: TopicCompareQuery, company_news: CompanyNewsQuery, digest: DigestQuery,
    northbound_holding: NorthboundHoldingQuery, detail: DetailQuery, cache_list: CacheListQuery,
    announcements: AnnouncementsQuery, news: NewsQuery, roadshows: RoadshowsQuery,
    market_news: MarketNewsQuery, channel_news: ChannelNewsQuery);
