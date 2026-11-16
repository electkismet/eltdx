//! Static news registry from the existing Python F10 facade. Availability of a
//! channel is a server property; unknown ids remain explicit wire selectors.
use super::{Response, Row};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum NewsHomeTab { Rolling, TodayFocus, Headlines, Positive, Negative, Essence, LatestTips, InstitutionResearch, Unknown(i32) }
impl NewsHomeTab {
    pub const fn wire(self) -> i32 { match self { Self::Rolling => 1, Self::TodayFocus => 2, Self::Headlines => 3, Self::Positive => 4, Self::Negative => 5, Self::Essence => 6, Self::LatestTips => 7, Self::InstitutionResearch => 9, Self::Unknown(value) => value } }
    pub const fn from_wire(value: i32) -> Self { match value { 1 => Self::Rolling, 2 => Self::TodayFocus, 3 => Self::Headlines, 4 => Self::Positive, 5 => Self::Negative, 6 => Self::Essence, 7 => Self::LatestTips, 9 => Self::InstitutionResearch, value => Self::Unknown(value) } }
}
pub fn home_tab_name(tab: NewsHomeTab) -> Option<&'static str> {
    Some(match tab.wire() { 1 => "滚动资讯", 2 => "今日焦点", 3 => "要闻速递", 4 => "正面消息", 5 => "负面消息", 6 => "精华集锦", 7 => "最新提示", 9 => "机构调研", _ => return None })
}

macro_rules! channels {
    ($($name:ident = $id:literal => ($label:literal, $group:literal)),* $(,)?) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
        pub enum NewsChannel { $($name,)* Unknown(i32) }
        impl NewsChannel {
            pub const fn wire(self) -> i32 { match self { $(Self::$name => $id,)* Self::Unknown(value) => value } }
            pub const fn from_wire(value: i32) -> Self { match value { $($id => Self::$name,)* value => Self::Unknown(value) } }
        }
        pub const CHANNELS: &[NewsChannelInfo] = &[$(NewsChannelInfo { id: $id, label: $label, group: Some($group) },)*];
    };
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NewsChannelInfo { id: i32, label: &'static str, group: Option<&'static str> }
impl NewsChannelInfo {
    pub const fn id(&self) -> i32 { self.id }
    pub const fn label(&self) -> &'static str { self.label }
    pub const fn group(&self) -> Option<&'static str> { self.group }
}
channels! {
    FinanceLatest = 200 => ("最新", "财经"), FinanceHeadlines = 201 => ("财经要闻", "财经"), FinanceDomestic = 202 => ("国内财经", "财经"),
    FinanceInternational = 203 => ("国际财经", "财经"), FinanceMarket = 204 => ("证券市场", "财经"), FinanceMoney = 205 => ("金融货币", "财经"),
    FinanceMacro = 206 => ("宏观经济", "财经"), FinanceIndustryDynamic = 207 => ("产经动态", "财经"), FinanceDigital = 211 => ("数字财经", "财经"),
    FinancePeople = 212 => ("财经人物", "财经"), FinancePolitics = 213 => ("时政要闻", "财经"),
    StockLatest = 300 => ("最新", "股票"), StockHeadlines = 301 => ("股票要闻", "股票"), StockMarketAnalysis = 302 => ("大盘分析", "股票"),
    StockSector = 303 => ("板块聚焦", "股票"), StockCompanyNews = 306 => ("公司新闻", "股票"), StockStarMarket = 318 => ("科创板", "股票"),
    StockNeeq = 304 => ("三板动态", "股票"), StockOverseas = 309 => ("海外股市", "股票"), StockOverseasCompany = 310 => ("海外公司", "股票"),
    StockCompanyAnnouncement = 307 => ("公司公告", "股票"), StockBShare = 305 => ("B股资讯", "股票"),
    IpoLatest = 500 => ("最新", "新股"), IpoNews = 501 => ("新股要闻", "新股"), IpoAnnouncement = 502 => ("新股公告", "新股"),
    IpoComment = 507 => ("新股评论", "新股"), IpoShSz = 503 => ("沪深新股", "新股"), IpoHk = 504 => ("港股新股", "新股"), IpoUs = 505 => ("美股新股", "新股"),
    HkLatest = 600 => ("最新", "港股"), HkNews = 601 => ("港股要闻", "港股"), HkMarket = 602 => ("港股市场", "港股"), HkCompany = 603 => ("港股公司", "港股"),
    HkRating = 605 => ("投行评级", "港股"), HkWarrant = 606 => ("涡轮资讯", "港股"), HkAnnouncement = 604 => ("港股公告", "港股"), HkEconomy = 607 => ("香港经济", "港股"),
    UsLatest = 700 => ("最新", "美股"), UsNews = 701 => ("美股要闻", "美股"), UsMarket = 702 => ("美股市场", "美股"),
    UsStockNews = 703 => ("美股新闻", "美股"), UsChinaConcept = 704 => ("中概股", "美股"), UsComment = 705 => ("美股评论", "美股"), UsOverseasEconomy = 706 => ("海外经济", "美股"),
    FundLatest = 800 => ("最新", "基金"), FundNews = 801 => ("基金要闻", "基金"), FundInstitutionHolding = 808 => ("机构持仓", "基金"),
    FundPublic = 802 => ("公募基金", "基金"), FundPrivate = 803 => ("私募基金", "基金"), FundSocialSecurity = 804 => ("社保基金", "基金"),
    FundTrust = 805 => ("信托基金", "基金"), FundAnnouncement = 809 => ("基金公告", "基金"),
    FuturesLatest = 900 => ("最新", "期货"), FuturesNews = 901 => ("期货要闻", "期货"), FuturesMarketComment = 902 => ("市场评述", "期货"),
    FuturesFinancial = 903 => ("金融期货", "期货"), FuturesEnergyChem = 904 => ("能源化工", "期货"), FuturesMetal = 905 => ("金属期货", "期货"),
    FuturesAgriculture = 906 => ("农产品", "期货"), FuturesDelivery = 907 => ("持仓交割", "期货"), FuturesSpot = 908 => ("现货报价", "期货"),
    FuturesSupplyDemand = 910 => ("生产供求", "期货"), FuturesInventory = 909 => ("库存数据", "期货"),
    WealthLatest = 1000 => ("最新", "理财"), WealthNews = 1001 => ("理财要闻", "理财"), WealthBank = 1002 => ("银行理财", "理财"),
    WealthComprehensive = 1008 => ("综合理财", "理财"), WealthPreciousMetal = 1006 => ("贵金属", "理财"), WealthInternetFinance = 1011 => ("互联网金融", "理财"),
    ForexLatest = 1100 => ("最新", "外汇"), ForexNews = 1101 => ("外汇要闻", "外汇"), ForexMarketComment = 1111 => ("汇市评论", "外汇"),
    ForexCurrencyColumn = 1102 => ("货币专栏", "外汇"), ForexCentralBank = 1103 => ("央行专栏", "外汇"),
    BondLatest = 1200 => ("最新", "债券"), BondNews = 1201 => ("债券要闻", "债券"), BondDomestic = 1202 => ("国内债市", "债券"),
    BondInternational = 1203 => ("国际债市", "债券"), BondNewsBrief = 1204 => ("债券新闻", "债券"), BondAnnouncement = 1205 => ("债券公告", "债券"),
    OptionLatest = 1801 => ("最新", "期权"), OptionNews = 1802 => ("期权要闻", "期权"), OptionIndex = 1803 => ("指数期权", "期权"), OptionCommodity = 1804 => ("商品期权", "期权"),
}
pub fn channel_name(channel: NewsChannel) -> Option<&'static str> { CHANNELS.iter().find(|info| info.id == channel.wire()).map(|info| info.label) }
/// News is in the second table. Never substitute metadata in the first table.
pub fn extract_rows(response: &Response) -> Vec<&Row> { response.result_sets().get(1).map(|table| table.rows().iter().collect()).unwrap_or_default() }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_has_all_channels_and_unknown_values_roundtrip() {
        assert_eq!(CHANNELS.len(), 84);
        assert_eq!(NewsChannel::from_wire(1011), NewsChannel::WealthInternetFinance);
        assert_eq!(channel_name(NewsChannel::Unknown(98765)), None);
        assert_eq!(NewsHomeTab::from_wire(8), NewsHomeTab::Unknown(8));
        assert_eq!(home_tab_name(NewsHomeTab::Headlines), Some("要闻速递"));
    }
}
