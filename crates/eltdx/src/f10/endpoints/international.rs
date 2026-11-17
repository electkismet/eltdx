//! US, Hong Kong and third-board query-to-wire mappings. Numeric JSON values
//! below intentionally differ from the text pagination used by A-share pages.
use std::num::NonZeroU32;
use std::time::Instant;

use super::SectionMap;
use super::SimpleArgument::{Code, Literal, MappedSection, Section};
use crate::f10::values::{count, date_pair, date_text, j, page, text};
use crate::f10::{
    CallQuery, F10Client, HongKongCode, Response, SectionCode, ThirdBoardCode, UsTicker,
};
use crate::types::TradingDate;
use crate::{OperationOptions, Result};

code_endpoint!(MgNameQuery, mg_name, UsTicker, code, "CWServ.skef10_mg_idreq", wire [Literal("name"), Code]);
section_endpoint!(MgTipsQuery, mg_tips, UsTicker, "zxdt", code, section, "CWServ.skef10_mg_zxdt", wire [Section, Code, Literal("")]);
code_endpoint!(MgProfileQuery, mg_profile, UsTicker, code, "CWServ.skef10_mg_gsgk", wire [Literal("001"), Code]);
section_endpoint!(MgFinanceQuery, mg_finance, UsTicker, "zcfzb", code, section, "CWServ.skef10_mg_cwfx", wire [Section, Code]);
section_endpoint!(MgFinanceIndicatorQuery, mg_finance_indicator, UsTicker, "jbzb", code, section, "CWServ.skef10_mg_cwzb", wire [MappedSection(SectionMap::UsIndicator), Code]);
code_endpoint!(
    MgBusinessQuery,
    mg_business,
    UsTicker,
    code,
    "CWServ.skef10_mg_jyfx",
    wire[Code]
);
code_endpoint!(MgIndustryQuery, mg_industry, UsTicker, code, "CWServ.skef10_mg_hyfx", wire [Literal("001"), Code, Literal("4"), Literal("1")]);
code_endpoint!(MgIndustryCompareQuery, mg_industry_compare, UsTicker, code, "CWServ.skef10_mg_hydb", wire [Literal("001"), Code, Literal(""), Literal("0"), Literal("0")]);
section_endpoint!(MgDividendQuery, mg_dividend, UsTicker, "fh", code, section, "CWServ.skef10_mg_fhrz", wire [Code, Section]);
section_endpoint!(MgDirectorsQuery, mg_directors, UsTicker, "ds", code, section, "CWServ.skef10_mg_dsgg", wire [Code, Section]);
code_endpoint!(MgAnnouncementsQuery, mg_announcements, UsTicker, code, "CWServ.skef10_mg_gsbd", wire [Literal("gsgg"), Code]);
section_endpoint!(MgRatingQuery, mg_rating, UsTicker, "001", code, section, "CWServ.skef10_mg_gspj", wire [Section, Code]);
code_endpoint!(MgTopicQuery, mg_topic, UsTicker, code, "CWServ.skef10_mg_rdtc", wire [Literal("001"), Code, Literal(""), Literal("4"), Literal("1")]);
section_endpoint!(MgDeepAnalysisQuery, mg_deep_analysis, UsTicker, "001", code, section, "CWServ.skef10_mg_sdfx", wire [Section, Code]);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MgCapitalQuery {
    code: UsTicker,
    section: SectionCode,
    page: u32,
}
impl MgCapitalQuery {
    pub fn new(code: UsTicker) -> Self {
        Self {
            code,
            section: SectionCode::default_for("mg_capital", "001"),
            page: 1,
        }
    }
    pub fn code(&self) -> &UsTicker {
        &self.code
    }
    pub fn section(&self) -> &SectionCode {
        &self.section
    }
    pub fn page(&self) -> u32 {
        self.page
    }
    pub fn with_section(mut self, value: SectionCode) -> Result<Self> {
        value.checked_for("mg_capital")?;
        self.section = value;
        Ok(self)
    }
    pub fn with_page(mut self, value: u32) -> Result<Self> {
        self.page = page(value)?;
        Ok(self)
    }
    pub fn to_call(&self) -> Result<CallQuery> {
        super::template::native_page(
            "mg_capital",
            &self.code,
            self.section.as_str(),
            self.page,
            1,
        )
    }
}

macro_rules! us_page_query {
    ($name:ident, $method:ident, $default:literal, $size:literal) => {
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct $name {
            code: UsTicker,
            section: SectionCode,
            page: u32,
            page_size: NonZeroU32,
        }
        impl $name {
            pub fn new(code: UsTicker) -> Self {
                Self {
                    code,
                    section: SectionCode::default_for(stringify!($method), $default),
                    page: 1,
                    page_size: NonZeroU32::new($size).unwrap_or(NonZeroU32::MIN),
                }
            }
            pub fn code(&self) -> &UsTicker {
                &self.code
            }
            pub fn section(&self) -> &SectionCode {
                &self.section
            }
            pub fn page(&self) -> u32 {
                self.page
            }
            pub fn page_size(&self) -> NonZeroU32 {
                self.page_size
            }
            pub fn with_section(mut self, value: SectionCode) -> Result<Self> {
                value.checked_for(stringify!($method))?;
                self.section = value;
                Ok(self)
            }
            pub fn with_page(mut self, value: u32) -> Result<Self> {
                self.page = page(value)?;
                Ok(self)
            }
            pub fn with_page_size(mut self, value: NonZeroU32) -> Result<Self> {
                self.page_size = count(value)?;
                Ok(self)
            }
        }
    };
}
us_page_query!(MgShareholdersQuery, mg_shareholders, "006", 10);
us_page_query!(MgMarketQuery, mg_market, "001", 20);
impl MgShareholdersQuery {
    pub fn to_call(&self) -> Result<CallQuery> {
        super::template::native_page(
            "mg_shareholders",
            &self.code,
            self.section.as_str(),
            self.page,
            self.page_size.get(),
        )
    }
}
impl MgMarketQuery {
    pub fn to_call(&self) -> Result<CallQuery> {
        super::template::native_page(
            "mg_market",
            &self.code,
            self.section.as_str(),
            self.page,
            self.page_size.get(),
        )
    }
}
impl F10Client {
    pub fn mg_capital(
        &self,
        query: &MgCapitalQuery,
        options: &OperationOptions,
    ) -> Result<Response> {
        self.endpoint(
            "f10.mg_capital",
            Instant::now(),
            || query.to_call(),
            options,
        )
    }
    pub fn mg_shareholders(
        &self,
        query: &MgShareholdersQuery,
        options: &OperationOptions,
    ) -> Result<Response> {
        self.endpoint(
            "f10.mg_shareholders",
            Instant::now(),
            || query.to_call(),
            options,
        )
    }
    pub fn mg_market(&self, query: &MgMarketQuery, options: &OperationOptions) -> Result<Response> {
        self.endpoint("f10.mg_market", Instant::now(), || query.to_call(), options)
    }
}

code_endpoint!(HkNameQuery, hk_name, HongKongCode, code, "CWServ.skef10_hk_idreq", wire [Literal("name"), Code]);
section_endpoint!(HkTipsQuery, hk_tips, HongKongCode, "1", code, section, "CWServ.skef10_hk_zxts", wire [Section, Code]);
section_endpoint!(HkEventsQuery, hk_events, HongKongCode, "1", code, section, "CWServ.skef10_hk_gsds", wire [Section, Code]);
section_endpoint!(HkProfileQuery, hk_profile, HongKongCode, "1", code, section, "CWServ.skef10_hk_gsgk", wire [Section, Code]);
section_endpoint!(HkRelatedQuery, hk_related, HongKongCode, "6", code, section, "CWServ.skef10_hk_glzq", wire [Section, Code]);
section_endpoint!(HkExecutivesQuery, hk_executives, HongKongCode, "1", code, section, "CWServ.skef10_hk_ggzl", wire [Section, Code]);
section_endpoint!(HkCapitalQuery, hk_capital, HongKongCode, "1", code, section, "CWServ.skef10_hk_gbjg", wire [Section, Code]);
section_endpoint!(HkShareholdersQuery, hk_shareholders, HongKongCode, "1", code, section, "CWServ.skef10_hk_gdcg", wire [Section, Code, Literal("")]);
section_endpoint!(HkFinanceQuery, hk_finance, HongKongCode, "zcfzb", code, section, "CWServ.skef10_hk_cwfx", wire [MappedSection(SectionMap::HkStatement), Code]);
section_endpoint!(HkFinanceIndicatorQuery, hk_finance_indicator, HongKongCode, "jbzb", code, section, "CWServ.skef10_hk_cwzb", wire [MappedSection(SectionMap::HkIndicator), Code]);
section_endpoint!(HkBusinessQuery, hk_business, HongKongCode, "1", code, section, "CWServ.skef10_hk_jyfx", wire [Section, Code]);
section_endpoint!(HkIndustryCompareQuery, hk_industry_compare, HongKongCode, "1", code, section, "CWServ.skef10_hk_hybj", wire [Section, Code, Literal("0"), Literal("0")]);
section_endpoint!(HkMarketQuery, hk_market, HongKongCode, "3", code, section, "CWServ.skef10_hk_schq", wire [Section, Code]);
section_endpoint!(HkCapitalFlowQuery, hk_capital_flow, HongKongCode, "001", code, section, "CWServ.skef10_hk_schq_zjlx", wire [Section, Code, Literal("0")]);
section_endpoint!(HkNewsQuery, hk_news, HongKongCode, "3", code, section, "CWServ.skef10_hk_gszx", wire [Section, Code]);
code_endpoint!(HkTopicQuery, hk_topic, HongKongCode, code, "CWServ.skef10_hk_rdtc", wire [Literal("001"), Code, Literal(""), Literal("0"), Literal("0")]);
code_endpoint!(
    HkPriceChartQuery,
    hk_price_chart,
    HongKongCode,
    code,
    "CWServ.skef10_hk_sdfx_hqzft",
    wire[Code]
);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HkPepbBandQuery {
    code: HongKongCode,
    sjfw: String,
    tab_value: String,
    hqzq: String,
}
impl HkPepbBandQuery {
    pub fn new(code: HongKongCode) -> Self {
        Self {
            code,
            sjfw: String::new(),
            tab_value: String::new(),
            hqzq: String::new(),
        }
    }
    pub fn code(&self) -> &HongKongCode {
        &self.code
    }
    pub fn sjfw(&self) -> &str {
        &self.sjfw
    }
    pub fn tab_value(&self) -> &str {
        &self.tab_value
    }
    pub fn hqzq(&self) -> &str {
        &self.hqzq
    }
    pub fn with_sjfw(mut self, value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        text(&value, true)?;
        self.sjfw = value;
        Ok(self)
    }
    pub fn with_tab_value(mut self, value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        text(&value, true)?;
        self.tab_value = value;
        Ok(self)
    }
    pub fn with_hqzq(mut self, value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        text(&value, true)?;
        self.hqzq = value;
        Ok(self)
    }
    pub fn to_call(&self) -> Result<CallQuery> {
        super::template::native_named("hk_pepb_band", &self.code, "", |name| {
            Ok(j(match name {
                "sjfw" => self.sjfw.as_str(),
                "tab_value" => self.tab_value.as_str(),
                "hqzq" => self.hqzq.as_str(),
                _ => unreachable!(),
            }))
        })
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HkValuationQuery {
    code: HongKongCode,
    item_id: String,
}
impl HkValuationQuery {
    pub fn new(code: HongKongCode) -> Self {
        Self {
            code,
            item_id: String::new(),
        }
    }
    pub fn code(&self) -> &HongKongCode {
        &self.code
    }
    pub fn item_id(&self) -> &str {
        &self.item_id
    }
    pub fn with_item_id(mut self, value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        text(&value, true)?;
        self.item_id = value;
        Ok(self)
    }
    pub fn to_call(&self) -> Result<CallQuery> {
        super::template::native_named("hk_valuation", &self.code, "", |_| {
            Ok(j(self.item_id.as_str()))
        })
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HkAnalystForecastQuery {
    code: HongKongCode,
    start: Option<TradingDate>,
    end: Option<TradingDate>,
}
impl HkAnalystForecastQuery {
    pub fn new(code: HongKongCode) -> Self {
        Self {
            code,
            start: None,
            end: None,
        }
    }
    pub fn code(&self) -> &HongKongCode {
        &self.code
    }
    pub fn start(&self) -> Option<TradingDate> {
        self.start
    }
    pub fn end(&self) -> Option<TradingDate> {
        self.end
    }
    pub fn with_start(mut self, value: Option<TradingDate>) -> Result<Self> {
        date_pair(value, self.end)?;
        self.start = value;
        Ok(self)
    }
    pub fn with_end(mut self, value: Option<TradingDate>) -> Result<Self> {
        date_pair(self.start, value)?;
        self.end = value;
        Ok(self)
    }
    pub fn to_call(&self) -> Result<CallQuery> {
        super::template::native_named("hk_analyst_forecast", &self.code, "", |name| {
            Ok(j(date_text(match name {
                "start" => self.start,
                "end" => self.end,
                _ => unreachable!(),
            })))
        })
    }
}
impl F10Client {
    pub fn hk_pepb_band(
        &self,
        query: &HkPepbBandQuery,
        options: &OperationOptions,
    ) -> Result<Response> {
        self.endpoint(
            "f10.hk_pepb_band",
            Instant::now(),
            || query.to_call(),
            options,
        )
    }
    pub fn hk_valuation(
        &self,
        query: &HkValuationQuery,
        options: &OperationOptions,
    ) -> Result<Response> {
        self.endpoint(
            "f10.hk_valuation",
            Instant::now(),
            || query.to_call(),
            options,
        )
    }
    pub fn hk_analyst_forecast(
        &self,
        query: &HkAnalystForecastQuery,
        options: &OperationOptions,
    ) -> Result<Response> {
        self.endpoint(
            "f10.hk_analyst_forecast",
            Instant::now(),
            || query.to_call(),
            options,
        )
    }
}

code_endpoint!(SbNameQuery, sb_name, ThirdBoardCode, code, "CWServ.skef10_sb_idreq", wire [Literal("name"), Code]);
section_endpoint!(SbProfileQuery, sb_profile, ThirdBoardCode, "001", code, section, "CWServ.skef10_sb_gsgk", wire [Code, Section]);
section_endpoint!(SbTipsQuery, sb_tips, ThirdBoardCode, "001", code, section, "CWServ.skef10_sb_zxts", wire [Code, Section]);
section_endpoint!(SbEventsQuery, sb_events, ThirdBoardCode, "ydjy", code, section, "CWServ.skef10_sb_jyds", wire [Code, Section]);
section_endpoint!(SbCapitalQuery, sb_capital, ThirdBoardCode, "gbjg", code, section, "CWServ.skef10_sb_gbjg", wire [Code, Section]);
section_endpoint!(SbShareholdersQuery, sb_shareholders, ThirdBoardCode, "sdgd", code, section, "CWServ.skef10_sb_gdyj", wire [Code, Section]);
section_endpoint!(SbComplianceQuery, sb_compliance, ThirdBoardCode, "001", code, section, "CWServ.skef10_sb_hgyy", wire [Code, Section]);
section_endpoint!(SbDividendQuery, sb_dividend, ThirdBoardCode, "fh", code, section, "CWServ.skef10_sb_fhrz", wire [Code, Section]);
section_endpoint!(SbFinanceQuery, sb_finance, ThirdBoardCode, "zcfzb", code, section, "CWServ.skef10_sb_cwfx", wire [Code, Section]);
section_endpoint!(SbBusinessQuery, sb_business, ThirdBoardCode, "001", code, section, "CWServ.skef10_sb_jyfx", wire [Code, Section]);
section_endpoint!(SbMajorEventsQuery, sb_major_events, ThirdBoardCode, "001", code, section, "CWServ.skef10_sb_zdsx", wire [Code, Section]);
section_endpoint!(SbNewsQuery, sb_news, ThirdBoardCode, "001", code, section, "CWServ.skef10_sb_gszx", wire [Code, Section]);
section_endpoint!(SbIndustryCompareQuery, sb_industry_compare, ThirdBoardCode, "hyxx", code, section, "CWServ.skef10_sb_hybj", wire [Code, Section]);
