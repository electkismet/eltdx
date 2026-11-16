//! Report layouts shared by strict queries and the Python eager-table adapter.
use crate::f10::{CallQuery, JsonValue};

#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub enum ReportArgument {
    Literal(&'static str), Code, Input(&'static str), OrInput(&'static str, &'static str),
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct ReportEntry {
    pub section: &'static str,
    pub entry: &'static str,
    pub arguments: &'static [ReportArgument],
}

impl ReportEntry {
    pub fn compose<T, E>(&self, mut value: impl FnMut(ReportArgument) -> Result<T, E>) -> Result<Vec<T>, E> {
        self.arguments.iter().map(|&argument| value(argument)).collect()
    }
}

macro_rules! table {
    ($($section:literal => $entry:literal [$($argument:expr),* $(,)?]),* $(,)?) => {
        &[$(ReportEntry { section: $section, entry: $entry, arguments: &[$($argument),*] }),*]
    };
}

#[doc(hidden)]
pub fn report_table(method: &str) -> &'static [ReportEntry] {
    use ReportArgument::{Code as C, Input as I, Literal as L, OrInput as O};
    match method {
        "digest" => table![
            "tips" => "CWServ.tdxf10_gg_zxts" [C, L("zxts"), L("")],
            "profile" => "CWServ.tdxf10_gg_zxts" [C, L("gsgy"), L("")],
            "events" => "CWServ.tdxf10_gg_zxts" [C, L("gsds"), L("")],
            "news" => "CWServ.tdxf10_gg_zxts" [C, L("gsxw"), L("")],
            "announcements" => "CWServ.tdxf10_gg_zxts" [C, L("gsgg"), L("")],
            "reports" => "CWServ.tdxf10_gg_zxts" [C, L("gsyj"), L("")],
            "roadshows" => "CWServ.tdxf10_gg_zxts" [C, L("lyhd"), L("")],
            "interaction" => "CWServ.tdxf10_gg_zxtstzzhd" [C, L("1"), L("1"), I("page_size")],
        ],
        "shareholders" => table![
            "count" => "CWServ.tdxf10_gg_gdyj" [C, L("gdrs"), I("filter1"), I("filter2"), I("page"), I("page"), I("page_size")],
            "ranking" => "CWServ.tdxf10_gg_gdyj" [C, L("thygdrs"), I("filter1"), I("filter2"), I("page"), I("page"), I("page_size")],
            "controller" => "CWServ.tdxf10_gg_gdyj" [C, L("kggd"), I("filter1"), I("filter2"), I("page"), I("page"), I("page_size")],
            "top_circulating" => "CWServ.tdxf10_gg_gdyj" [C, L("ltgd"), I("filter1"), I("filter2"), I("page"), I("page"), I("page_size")],
            "top" => "CWServ.tdxf10_gg_gdyj" [C, L("sdgdbgq"), I("filter1"), I("filter2"), I("page"), I("page"), I("page_size")],
            "bonds" => "CWServ.tdxf10_gg_gdyj" [C, L("sdzqcyr"), I("filter1"), I("filter2"), I("page"), I("page"), I("page_size")],
            "changes" => "CWServ.tdxf10_gg_gdyj" [C, L("cgbd"), I("filter1"), I("filter2"), I("page"), I("page"), I("page_size")],
            "institutions" => "CWServ.tdxf10_gg_gdyj" [C, L("jgcg"), I("filter1"), I("filter2"), I("page"), I("page"), I("page_size")],
            "institution_types" => "CWServ.tdxf10_gg_gdyj" [C, L("jgcgz"), I("filter1"), I("date"), I("page"), I("page"), I("page_size")],
            "institution_holdings" => "CWServ.tdxf10_gg_gdyj_jgcgmx" [C, I("kind"), I("date"), L("99"), I("page"), I("page"), I("page_size")],
            "plan" => "CWServ.tdxf10_gg_gdyj" [C, L("gdzjcjh"), I("filter1"), I("filter2"), I("page"), I("page"), I("page_size")],
        ],
        "capital_structure" => table![
            "structure" => "CWServ.tdxf10_gg_gbjg" [C, L("gbjg")],
            "changes" => "CWServ.tdxf10_gg_gbjg" [C, L("gbbd")],
            "buyback" => "CWServ.tdxf10_gg_gbjg" [C, L("gphg")],
            "restricted" => "CWServ.tdxf10_gg_gbjg" [C, L("xslt")],
            "restricted_detail" => "CWServ.tdxf10_gg_gbjg_xsjjmx" [C, I("date")],
        ],
        "business_report" => table![
            "business" => "CWServ.tdxf10_gg_jyfx" [C, L("zyyw"), L("")],
            "composition" => "CWServ.tdxf10_gg_jyfx" [C, L("zygc"), I("date")],
            "customers" => "CWServ.tdxf10_gg_jyfx" [C, L("qwm"), I("date")],
            "suppliers" => "CWServ.tdxf10_gg_jyfx" [C, L("qwmgys"), I("date")],
            "price" => "CWServ.tdxf10_gg_jyfx" [C, L("cpjg"), L("")],
            "changes" => "CWServ.tdxf10_gg_jyfx" [C, L("0"), I("date")],
        ],
        "industry_analysis" => table![
            "peers" => "CWServ.tdxf10_gg_hyfx" [L("cwgz"), C, O("arg", "000")],
            "dividend_rank" => "CWServ.tdxf10_gg_hyfx" [L("fhrzb"), C, O("arg", "100")],
            "scale" => "CWServ.tdxf10_gg_hyfx" [L("gsgm"), C, O("arg", "000")],
            "valuation" => "CWServ.tdxf10_gg_hyfx" [L("gzsp"), C, O("arg", "011")],
            "news" => "CWServ.tdxf10_gg_hyfx" [L("hyxw"), C, O("arg", "")],
            "reports" => "CWServ.tdxf10_gg_hyfx" [L("hyyb"), C, O("arg", "")],
            "market_share" => "CWServ.tdxf10_gg_hyfx" [L("scbx"), C, O("arg", "000")],
            "summary" => "CWServ.tdxf10_gg_hyfx" [L("tot"), C, O("arg", "")],
            "comparable" => "CWServ.tdxf10_gg_kbgs" [L("1"), C],
            "comparable_profile" => "CWServ.tdxf10_gg_kbgs" [L("0"), C],
        ],
        "trading_report" => table![
            "margin" => "CWServ.tdxf10_gg_jyds" [C, L("rzrq"), L("")],
            "margin_history" => "CWServ.tdxf10_gg_sj" [L("rzrq_sj"), C, L("0"), C],
            "block_trades" => "CWServ.tdxf10_gg_jyds" [C, L("dzjy"), L("")],
            "dragon_board" => "CWServ.tdxf10_gg_jyds" [C, L("lgt"), O("arg", "2")],
            "northbound" => "CWServ.tdxf10_gg_jyds" [C, L("islgt"), L("")],
            "limit_down" => "CWServ.tdxf10_gg_jyds" [C, L("dtfx"), L("")],
            "limit_up" => "CWServ.tdxf10_gg_jyds" [C, L("ztfx"), L("")],
            "bonds" => "CWServ.tdxf10_gg_jyds" [C, L("zrq"), L("")],
            "main_force" => "CWServ.tdxf10_gg_jyds" [C, L("zlpskcj"), L("")],
            "reports" => "CWServ.tdxf10_gg_jyds" [C, L("yxsbxx"), L("")],
            "kcb" => "CWServ.tdxf10_gg_jyds" [C, L("checkkcb"), L("")],
        ],
        "fund_report" => table![
            "name" => "CWServ.skef10_jj_idreq" [L("name"), C],
            "managers" => "CWServ.skef10_jj_jjzl" [L("1"), C, L(""), I("page"), I("page"), I("page_size")],
            "company" => "CWServ.skef10_jj_jjzl" [L("2"), C, L(""), I("page"), I("page"), I("page_size")],
            "basic" => "CWServ.skef10_jj_jjzl" [L("4"), C, L(""), I("page"), I("page"), I("page_size")],
            "seats" => "CWServ.skef10_jj_jjzl" [L("5"), C, I("arg"), I("page"), I("page"), I("page_size")],
            "scale" => "CWServ.skef10_jj_gmfe" [L("0"), C, L(""), L("")],
            "scale_history" => "CWServ.skef10_jj_gmfe" [O("arg", "0"), C, L(""), I("date")],
            "nav" => "CWServ.skef10_jj_jzhb" [L("0"), C, L("0"), L("")],
            "nav_period" => "CWServ.skef10_jj_jzhb" [L("1"), C, L("0"), L("")],
            "nav_rank" => "CWServ.skef10_jj_jzhb" [L("3"), C, L("0"), L("")],
            "dividends" => "CWServ.skef10_jj_jzhb" [L("5"), C, L("0"), L("")],
            "asset_allocation" => "CWServ.skef10_jj_tzzh" [L("zcpz"), C, L("")],
            "industry_allocation" => "CWServ.skef10_jj_tzzh" [L("hyfb"), C, I("date")],
            "holdings_compare" => "CWServ.skef10_jj_tzzh" [L("cgjzd"), C, L("")],
            "holdings_rank" => "CWServ.skef10_jj_tzzh" [L("zjpz"), C, L("")],
            "holding_summary" => "CWServ.skef10_jj_tzzh" [L("jjxtzzh"), C, L("")],
            "bond_allocation" => "CWServ.skef10_jj_tzzh" [L("zczq"), C, I("date")],
            "futures_allocation" => "CWServ.skef10_jj_tzzh" [L("ccqhmx"), C, I("date")],
            "holdings" => "CWServ.skef10_jj_cgqk" [L("cgqkbgq"), C, L(""), L(""), L("1"), L("1"), L("1")],
            "holdings_detail" => "CWServ.skef10_jj_cgqk" [L("cgmxbgq"), C, L(""), L(""), L("1"), L("1"), L("1")],
            "holdings_changes" => "CWServ.skef10_jj_cgqk" [L("cyjjmxbgq"), C, L(""), L(""), L("1"), L("1"), L("1")],
            "fund_of_funds" => "CWServ.skef10_jj_cgqk" [L("sxjjzcbgq"), C, L(""), L(""), L("1"), L("1"), L("1")],
            "finance" => "CWServ.skef10_jj_cwzk" [O("arg", "zcfzb"), C],
            "sales_info" => "CWServ.skef10_jj_xsxx" [L("0"), C],
            "sales_announcements" => "CWServ.skef10_jj_xsxx" [L("4"), C],
            "sales_news" => "CWServ.skef10_jj_xsxx" [L("5"), C],
            "etf" => "CWServ.skef10_jj_etfzj" [L("etfzjlx"), C, O("date", "3M")],
            "margin" => "CWServ.skef10_jj_etfzj" [L("etfrzrq"), C, O("date", "3M")],
            "ratings" => "CWServ.skef10_jj_jjpj" [O("arg", "001"), C],
            "categories" => "CWServ.skef10_jj_fxsy" [O("arg", "bgqzf"), C, L("")],
        ],
        "market_report" => table![
            "board_info" => "CWServ.skef10_bk_getinfo" [C],
            "board_basics" => "CWServ.skef10_bk_cpbd_jczl" [L("001"), C, L("")],
            "board_description" => "CWServ.skef10_bk_cpbd_jczl" [L("002"), C, L("")],
            "board_trading" => "CWServ.skef10_bk_cpbd_jczl" [L("004"), C, L("")],
            "board_flow" => "CWServ.skef10_bk_zjlx" [O("arg", "001_1"), C, L("")],
            "board_members" => "CWServ.skef10_bk_scgl" [O("arg", "001"), C, I("page"), I("page_size")],
            "board_analysis" => "CWServ.skef10_bk_sdfx" [L("001"), C, L("")],
            "board_finance_compare" => "CWServ.skef10_bk_ggdb_cwdb" [C],
            "board_candidates" => "CWServ.skef10_bk_cfgsj" [O("arg", "001"), C],
            "board_trend" => "CWServ.skef10_bk_ydfx" [C],
            "index_info" => "CWServ.skef10_zs_getinfo" [C],
            "index_profile" => "CWServ.skef10_zs_jbxx_zsgk" [C],
            "index_valuation" => "CWServ.skef10_zs_jbxx_gzxx" [L("001"), C, L(""), L("")],
            "index_tracking" => "CWServ.skef10_zs_jbxx_gzbd" [C, I("page"), I("page_size")],
            "index_flow" => "CWServ.skef10_zs_zjdx" [O("arg", "004_1"), C, L(""), L(""), I("page"), I("page_size")],
            "index_flow_members" => "CWServ.skef10_zs_cfgsj_cfgjcsj" [O("arg", "001"), C, I("page"), I("page_size")],
            "index_notices" => "CWServ.skef10_zs_jrts" [O("arg", "001"), C, I("page"), I("page_size")],
            "market_basic" => "CWServ.skef10_dpzx_jbxx" [L("002"), C],
            "market_margin" => "CWServ.skef10_dpzx_rzrq" [L("001"), C],
            "market_margin_summary" => "CWServ.skef10_dpzx_rzrq" [L("002"), C],
            "market_margin_investors" => "CWServ.skef10_dpzx_rzrq" [L("003"), O("arg", "1m")],
            "market_new_shares" => "CWServ.skef10_dpzx_xgfx" [L("002")],
            "market_capital" => "CWServ.skef10_dpzx_scgl" [O("arg", "004")],
        ],
        _ => &[],
    }
}

pub(super) fn native_report(method: &str, code: &str, section: &str,
    mut input: impl FnMut(&str) -> crate::Result<JsonValue>) -> crate::Result<CallQuery>
{
    let definition = report_table(method).iter().find(|entry| entry.section == section)
        .ok_or_else(|| crate::f10::values::invalid("unknown report section"))?;
    let values = definition.compose(|slot| Ok::<_, crate::Error>(match slot {
        ReportArgument::Code => code.into(), ReportArgument::Literal(value) => value.into(),
        ReportArgument::Input(name) => input(name)?,
        ReportArgument::OrInput(name, default) => {
            let value = input(name)?;
            if value.as_str() == Some("") { default.into() } else { value }
        }
    }))?;
    super::params(definition.entry, values)
}

#[doc(hidden)]
pub const SHAREHOLDER_PLAN_SECTION: &str = "plan";

#[doc(hidden)]
pub fn fund_period_section(section: &str) -> Option<&'static str> {
    match section { "industry_allocation" => Some("hyfbbgq"), "bond_allocation" => Some("zczqbgq"),
        "futures_allocation" => Some("ccqhmxbgq"), _ => None }
}

#[doc(hidden)]
pub fn market_section(section: &str) -> &str {
    match section { "index_members" => "index_flow_members", "index_etfs" => "index_tracking",
        "ipo_calendar" => "market_new_shares", value => value }
}
