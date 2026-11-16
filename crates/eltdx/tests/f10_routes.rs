//! Request fixtures are source evidence until the final validation phase.
#![cfg(feature = "f10")]
use eltdx::f10::*;
use eltdx::{Error, ErrorKind, TradingDate};

fn body(call: &CallQuery) -> eltdx::Result<Vec<u8>> {
    match call.payload() {
        RequestPayload::Params(values) => JsonValue::Object(vec![("Params".to_owned(), JsonValue::Array(values.clone()))]).to_json_bytes(),
        RequestPayload::Body(value) => value.to_json_bytes(),
    }
}
fn assert_call(call: CallQuery, entry: &str, bytes: &[u8]) -> eltdx::Result<()> {
    assert_eq!(call.entry().as_str(), entry);
    assert_eq!(body(&call)?, bytes);
    Ok(())
}
fn ashare() -> eltdx::Result<AshareCode> { AshareCode::new(F10MarketCode::new(1), "600000") }

#[test]
fn ladder_date_ranges_use_string_selector_and_reject_reversed_range() -> eltdx::Result<()> {
    let start = TradingDate::new(2024, 6, 28)?;
    let end = TradingDate::new(2024, 7, 1)?;
    assert_call(LimitUpDownListQuery::new().with_dates(Some(start), Some(end))?.to_call()?,
        "CWServ.cfg_fx_lbtt", br#"{"Params":["1","20240628","20240701"]}"#)?;
    assert_call(LimitBoardLadderQuery::new().with_end_date(Some(end))?.to_call()?,
        "CWServ.cfg_fx_lbtt", br#"{"Params":["1","20240701","20240701"]}"#)?;
    assert!(LimitUpDownListQuery::new().with_dates(Some(end), Some(start)).is_err());
    assert_eq!(LimitUpDownListQuery::new().to_call().unwrap_err().kind(), ErrorKind::MetadataUnavailable);
    Ok(())
}

#[test]
fn market_aliases_keep_exact_entries_and_parameter_types() -> eltdx::Result<()> {
    assert_call(MarketReportQuery::new(Some(ashare()?)).with_section(SectionCode::new("market_report", "index_members")?)?.to_call()?,
        "CWServ.skef10_zs_cfgsj_cfgjcsj", br#"{"Params":["001","600000","1","20"]}"#)?;
    assert_call(MarketReportQuery::new(Some(ashare()?)).with_section(SectionCode::new("market_report", "index_etfs")?)?.to_call()?,
        "CWServ.skef10_zs_jbxx_gzbd", br#"{"Params":["600000","1","20"]}"#)?;
    assert_call(MarketReportQuery::new(None).with_section(SectionCode::new("market_report", "ipo_calendar")?)?.to_call()?,
        "CWServ.skef10_dpzx_xgfx", br#"{"Params":["002"]}"#)?;
    assert_call(MarketReportQuery::new(Some(ashare()?)).with_section(SectionCode::new("market_report", "board_members")?)?.to_call()?,
        "CWServ.skef10_bk_scgl", br#"{"Params":["001","600000","1","20"]}"#)?;
    assert!(MarketReportQuery::new(None).to_call().is_err());
    Ok(())
}

#[test]
fn report_period_prerequisites_and_explicit_business_period_keep_source_order() -> eltdx::Result<()> {
    assert_call(BusinessPeriodsQuery::new(ashare()?).to_call()?, "CWServ.tdxf10_gg_comreq", br#"{"Params":["zygcfx","600000"]}"#)?;
    assert_call(BusinessReportQuery::new(ashare()?).with_section(SectionCode::new("business_report", "composition")?)?
        .with_report_date(Some(TradingDate::new(2024, 6, 30)?)).to_call()?,
        "CWServ.tdxf10_gg_jyfx", br#"{"Params":["600000","zygc","20240630"]}"#)?;
    let fund = FundReportQuery::new(ashare()?).with_section(SectionCode::new("fund_report", "industry_allocation")?)?;
    assert_eq!(fund.to_call().unwrap_err().kind(), ErrorKind::MetadataUnavailable);
    assert_call(fund.with_date(Some(DateOrWindow::Date(TradingDate::new(2024, 6, 30)?)))?.to_call()?,
        "CWServ.skef10_jj_tzzh", br#"{"Params":["hyfb","600000","20240630"]}"#)?;
    Ok(())
}

#[test]
fn collection_query_limits_remain_checked_before_network() -> eltdx::Result<()> {
    let size = std::num::NonZeroU32::new(101).ok_or_else(|| Error::new(ErrorKind::Internal, "test constant"))?;
    assert!(IndexMembersQuery::new(ashare()?).with_page_size(size).is_err());
    assert!(IndexTrackingProductsQuery::new(ashare()?).with_page_size(size).is_err());
    assert!(BoardMembersQuery::new(ashare()?).with_page_size(size).is_err());
    Ok(())
}

#[test]
fn historical_money_flow_windows_are_strings_and_slicing_is_local() -> eltdx::Result<()> {
    assert_call(MoneyFlowHistoryQuery::new(ashare()?).to_call()?,
        "CWServ.tdxf10_gg_sj", br#"{"Params":["zjlx_sj","600000","60","600000"]}"#)?;
    for days in [3, 5, 10, 20, 60] {
        let query = MoneyFlowHistoryQuery::new(ashare()?).with_days(days)?.with_start(usize::MAX)
            .with_count(std::num::NonZeroU32::new(800))?;
        assert_eq!(query.days().get(), days);
        assert_eq!(query.start(), usize::MAX);
        assert_call(query.to_call()?, "CWServ.tdxf10_gg_sj",
            format!(r#"{{"Params":["zjlx_sj","600000","{days}","600000"]}}"#).as_bytes())?;
    }
    assert_eq!(MoneyFlowHistoryQuery::new(ashare()?).with_days(7).unwrap_err().kind(), ErrorKind::InvalidArgument);
    assert!(MoneyFlowHistoryQuery::new(ashare()?).with_count(std::num::NonZeroU32::new(801)).is_err());
    Ok(())
}

#[test]
fn topic_identity_and_fund_window_are_validated_by_builders() -> eltdx::Result<()> {
    for invalid in ["", "   ", "id\n"] { assert!(TopicCompareQuery::new(ashare()?, invalid).is_err()); }
    assert_call(TopicCompareQuery::new(ashare()?, "001")?.to_call()?,
        "CWServ.tdxf10_gg_rdtc_gndb", br#"{"Params":["gndbzfsj","600000","001","zdf"]}"#)?;
    assert!(TopicCompareQuery::new(ashare()?, "001")?.with_topic_id("").is_err());
    let dated = FundReportQuery::new(ashare()?).with_section(SectionCode::new("fund_report", "etf")?)?
        .with_date(Some(DateOrWindow::Window(FundWindow::three_months())))?;
    assert!(dated.clone().with_section(SectionCode::new("fund_report", "industry_allocation")?).is_err());
    assert_call(dated.with_section(SectionCode::new("fund_report", "margin")?)?.to_call()?,
        "CWServ.skef10_jj_etfzj", br#"{"Params":["etfrzrq","600000","3M"]}"#)?;
    Ok(())
}
