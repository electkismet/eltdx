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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TopicCompareQuery { code: AshareCode, section: SectionCode, topic_id: String, sort_by: String }
impl TopicCompareQuery {
    pub fn new(code: AshareCode, topic_id: impl Into<String>) -> Result<Self> {
        let topic_id = topic_id.into(); valid_topic(&topic_id)?;
        Ok(Self { code, section: SectionCode::default_for("topic_compare", "gndbzfsj"), topic_id, sort_by: "zdf".to_owned() })
    }
    pub fn with_topic(code: AshareCode, topic_id: impl Into<String>) -> Result<Self> { Self::new(code, topic_id) }
    pub fn code(&self) -> &AshareCode { &self.code }
    pub fn section(&self) -> &SectionCode { &self.section }
    pub fn topic_id(&self) -> &str { &self.topic_id }
    pub fn sort_by(&self) -> &str { &self.sort_by }
    pub fn with_section(mut self, value: SectionCode) -> Result<Self> {
        value.checked_for("topic_compare")?; self.section = value; Ok(self)
    }
    pub fn with_topic_id(mut self, value: impl Into<String>) -> Result<Self> {
        let value = value.into(); valid_topic(&value)?; self.topic_id = value; Ok(self)
    }
    pub fn with_sort_by(mut self, value: impl Into<String>) -> Result<Self> {
        let value = value.into(); text(&value, true)?; self.sort_by = value; Ok(self)
    }
    pub fn to_call(&self) -> Result<CallQuery> {
        super::template::native_named("topic_compare", &self.code, self.section.as_str(), |name|
            Ok(j(match name { "topic_id" => self.topic_id.as_str(), "sort_by" => self.sort_by.as_str(), _ => unreachable!() })))
    }
}
fn valid_topic(value: &str) -> Result<()> {
    text(value, false)?;
    if value.trim().is_empty() { return Err(crate::f10::values::invalid("topic id must not be blank")); }
    Ok(())
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
        let definition = super::page_template("company_news", |value| Ok::<_, crate::Error>(self.section.as_str() == value))?.expect("built-in company news");
        super::template::native_inputs(definition, &self.code, self.section.as_str(), |name| Ok(match name {
            "keyword" => j(self.keyword.as_str()), "rating" => j(self.rating.as_str()),
            "page" => j(format!("{}", self.page)), "page_size" => j(format!("{}", self.page_size.get())), _ => unreachable!(),
        }))
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
        super::template::native_named("northbound_holding", &self.code, self.section.as_str(), |name| Ok(match name {
            "filter_value" => j(self.filter_value.as_str()), "page" => j(format!("{}", self.page)),
            "page_size" => j(format!("{}", self.page_size.get())), _ => unreachable!(),
        }))
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
    pub fn to_call(&self) -> Result<CallQuery> { super::template::native_named("detail", &(), "", |name| Ok(j(match name {
        "detail_type" => self.detail_type.as_str(), "record_id" => self.record_id.as_str(), _ => unreachable!(),
    }))) }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheListQuery { code: AshareCode, kind: String }
impl CacheListQuery {
    pub fn new(code: AshareCode) -> Self { Self { code, kind: "gg".to_owned() } }
    pub fn code(&self) -> &AshareCode { &self.code }
    pub fn kind(&self) -> &str { &self.kind }
    pub fn with_kind(mut self, value: impl Into<String>) -> Result<Self> {
        let value = value.into(); if !super::CACHE_KINDS.contains(&value.as_str()) { return Err(crate::f10::values::invalid("cache kind must be gg, xw or ly")); }
        self.kind = value; Ok(self)
    }
    pub fn to_call(&self) -> Result<CallQuery> { cache_call(&self.code, &self.kind) }
}
fn cache_call(code: &AshareCode, kind: &str) -> Result<CallQuery> {
    let payload = super::cache_payload(|value| Ok::<_, crate::Error>(match value {
        super::CacheValue::Literal(value) => j(value), super::CacheValue::Qsid => j("tdx"),
        super::CacheValue::Key => j(format!("{kind}:{}_{}", code.market().get(), code.as_str())),
    }))?.into_iter().map(|(name, value)| (name.to_owned(), value)).collect();
    Ok(CallQuery::new(crate::f10::EntryName::new(super::CACHE_ENTRY)?, RequestPayload::Body(JsonValue::Object(payload))))
}
macro_rules! cache_query { ($name:ident, $method:literal) => {
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct $name { code: AshareCode }
    impl $name {
        pub fn new(code: AshareCode) -> Self { Self { code } }
        pub fn code(&self) -> &AshareCode { &self.code }
        pub fn to_call(&self) -> Result<CallQuery> { cache_call(&self.code, super::cache_alias($method).expect("built-in cache alias")) }
    }
}; }
cache_query!(AnnouncementsQuery, "announcements"); cache_query!(NewsQuery, "news"); cache_query!(RoadshowsQuery, "roadshows");

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
    pub fn to_call(&self) -> Result<CallQuery> { super::template::native_named("market_news", &(), "", |name| Ok(j(match name {
        "tab" => format!("{}", self.tab.wire()), "page" => format!("{}", self.page),
        "page_size" => format!("{}", self.page_size.get()), _ => unreachable!(),
    }))) }
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
    pub fn to_call(&self) -> Result<CallQuery> { super::template::native_named("channel_news", &(), "", |name| Ok(match name {
        "query1" => j(self.query1.as_str()), "channel" => j(format!("{}", self.channel.wire())),
        "page" => j(format!("{}", self.page)), "page_size" => j(format!("{}", self.page_size.get())),
        "keyword" => j(self.keyword.as_str()), _ => unreachable!(),
    })) }
}

impl F10Client {
    pub fn news_channels(&self) -> Vec<NewsChannelInfo> {
        let mut channels = crate::f10::news::CHANNELS.to_vec(); channels.sort_by_key(NewsChannelInfo::id); channels
    }
    pub fn topic_compare_first(&self, query: &TopicCompareFirstQuery, options: &OperationOptions) -> Result<Response> {
        let context = self.context(Instant::now(), "f10.topic_compare_first", options)?;
        let first = self.endpoint_call(&super::reports::TopicIdsQuery::new(query.code.clone()).to_call()?, &context)?;
        let mut topic = None;
        if first.ok() { if let Some(table) = first.first_table() { for row in table.rows() {
            context.check_boundary(TimeoutPhase::Decode)?;
            if let Some(cell) = row.get_all("t001").next() {
                let candidate = match cell.value() {
                    JsonValue::String(value) => Some(value.as_str()),
                    JsonValue::Number(value) => Some(value.lexeme()), _ => None,
                };
                if let Some(value) = candidate.filter(|value| valid_topic(value).is_ok()) {
                    context.record_work(0, value.len())?;
                    let mut bytes = Vec::new();
                    for chunk in value.as_bytes().chunks(4096) {
                        context.check_boundary(TimeoutPhase::Decode)?;
                        bytes.try_reserve(chunk.len()).map_err(|error| Error::new(ErrorKind::LimitExceeded,
                            "cannot reserve topic id").with_source(error))?;
                        bytes.extend_from_slice(chunk);
                    }
                    topic = Some(String::from_utf8(bytes).map_err(|error| Error::new(ErrorKind::Internal,
                        "invalid copied topic id").with_source(error))?);
                    break;
                }
            }
        } } }
        let topic = topic.ok_or_else(|| Error::new(ErrorKind::MetadataUnavailable, "no topic id for topic comparison")
            .with_context("server_error_code", first.error_code().map_or_else(|| "absent".to_owned(), |value| value.to_string())))?;
        let call = super::params("CWServ.tdxf10_gg_rdtc_gndb", vec![j(query.section.as_str()), j(query.code.as_str()), j(topic), j(query.sort_by.as_str())])?;
        let response = self.endpoint_call(&call, &context)?.with_prerequisite(&first, &context)?;
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
