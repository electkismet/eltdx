//! Shared fixed-request definitions. Execution is deferred to the final gate.
#![cfg(feature = "f10")]

use eltdx::f10::{
    simple_endpoint, BondCode, MgTipsQuery, RequestPayload, SectionCode, SimpleArgument, UsTicker,
};

#[test]
fn fixed_definition_and_typed_query_share_nondefault_arguments() -> eltdx::Result<()> {
    let definition = simple_endpoint("mg_tips").unwrap();
    let values = definition
        .compose(|slot| {
            Ok::<_, std::convert::Infallible>(match slot {
                SimpleArgument::Literal(value) => eltdx::f10::JsonValue::from(value),
                SimpleArgument::Code => eltdx::f10::JsonValue::from("AAPL"),
                SimpleArgument::Section => eltdx::f10::JsonValue::from("sypl"),
                SimpleArgument::Market => unreachable!(),
                _ => unreachable!(),
            })
        })
        .unwrap();
    let query = MgTipsQuery::new(UsTicker::new("AAPL")?)
        .with_section(SectionCode::new("mg_tips", "sypl")?)?
        .to_call()?;
    assert_eq!(query.entry().as_str(), definition.entry);
    assert_eq!(query.payload(), &RequestPayload::Params(values));
    assert_eq!(definition.code_helper, Some("_us_code"));
    assert_eq!(
        definition.valid_sections,
        Some(&["zxdt", "sypl", "gsgy", "zysj"][..])
    );
    Ok(())
}

#[test]
fn bond_templates_keep_market_and_legacy_helper_differences() -> eltdx::Result<()> {
    let call = eltdx::f10::GzProfileQuery::new(BondCode::new("sz123001")?).to_call()?;
    assert_eq!(call.entry().as_str(), "CWServ.skef10_bond_gz_zjgk");
    assert_eq!(
        call.payload(),
        &RequestPayload::Params(vec!["zqgk".into(), "123001".into(), "0".into()])
    );
    assert_eq!(
        simple_endpoint("gz_profile").unwrap().code_helper,
        Some("_bond_code")
    );
    assert_eq!(
        simple_endpoint("bond_profile").unwrap().code_helper,
        Some("_code6")
    );
    assert_eq!(
        simple_endpoint("bond_bulletin").unwrap().code_helper,
        Some("_bond_code")
    );
    assert_eq!(
        simple_endpoint("gzhg_quotes").unwrap().code_helper,
        Some("_code6")
    );
    assert!(simple_endpoint("hy_profile").unwrap().code_helper.is_none());
    assert!(simple_endpoint("qh_inventory")
        .unwrap()
        .code_helper
        .is_none());
    assert!(simple_endpoint("unregistered").is_none());
    Ok(())
}

#[test]
fn shared_argument_visitor_stops_at_the_original_error() {
    let definition = simple_endpoint("gz_profile").unwrap();
    let mut visited = Vec::new();
    let result = definition.compose(|slot| {
        visited.push(slot);
        match slot {
            SimpleArgument::Code => Err("conversion"),
            _ => Ok(()),
        }
    });
    assert_eq!(result, Err("conversion"));
    assert!(matches!(
        visited.as_slice(),
        [SimpleArgument::Section, SimpleArgument::Code]
    ));
}

#[test]
fn international_mapped_and_page_templates_keep_wire_types() -> eltdx::Result<()> {
    use eltdx::f10::{
        HkFinanceQuery, HongKongCode, MgCapitalQuery, MgFinanceIndicatorQuery, MgMarketQuery,
        MgShareholdersQuery,
    };
    let indicator = MgFinanceIndicatorQuery::new(UsTicker::new("AAPL")?)
        .with_section(SectionCode::new("mg_finance_indicator", "ylnl")?)?
        .to_call()?;
    assert_eq!(
        indicator.payload(),
        &RequestPayload::Params(vec!["002".into(), "AAPL".into()])
    );
    let hk = HkFinanceQuery::new(HongKongCode::new("700")?).to_call()?;
    assert_eq!(
        hk.payload(),
        &RequestPayload::Params(vec!["2".into(), "00700".into()])
    );
    let capital = MgCapitalQuery::new(UsTicker::new("AAPL")?)
        .with_page(17)?
        .to_call()?;
    assert_eq!(
        capital.payload(),
        &RequestPayload::Params(vec!["001".into(), "AAPL".into(), 1_i64.into()])
    );
    let owners = MgShareholdersQuery::new(UsTicker::new("AAPL")?)
        .with_page(17)?
        .to_call()?;
    assert_eq!(
        owners.payload(),
        &RequestPayload::Params(vec![
            "006".into(),
            "AAPL".into(),
            1_i64.into(),
            10_i64.into()
        ])
    );
    let market = MgMarketQuery::new(UsTicker::new("AAPL")?)
        .with_section(SectionCode::new("mg_market", "003")?)?
        .with_page(17)?
        .to_call()?;
    assert_eq!(
        market.payload(),
        &RequestPayload::Params(vec![
            "003".into(),
            "AAPL".into(),
            0_i64.into(),
            0_i64.into()
        ])
    );
    Ok(())
}

#[test]
fn sector_query_selections_share_all_literal_entries() -> eltdx::Result<()> {
    use eltdx::f10::*;
    for &method in &[
        "hy_summary",
        "hy_members_rank",
        "qh_variety_info",
        "qh_position_report",
    ] {
        for &section in sector_choices(method) {
            let selected = sector_selection(method, section).unwrap();
            let call = match method {
                "hy_summary" => HySummaryQuery::new(IndustryCode::new("881319")?)
                    .with_section(SectionCode::new(method, section)?)?
                    .to_call()?,
                "hy_members_rank" => HyMembersRankQuery::new(IndustryCode::new("881319")?)
                    .with_section(SectionCode::new(method, section)?)?
                    .to_call()?,
                "qh_variety_info" => QhVarietyInfoQuery::new(FuturesCode::new("rb")?)
                    .with_section(SectionCode::new(method, section)?)?
                    .to_call()?,
                _ => QhPositionReportQuery::new(FuturesCode::new("rb")?)
                    .with_section(SectionCode::new(method, section)?)?
                    .to_call()?,
            };
            if method == "qh_position_report" {
                assert_eq!(
                    call.entry().as_str(),
                    format!("{}{}", position_prefix(false), selected.entry)
                );
            } else {
                assert_eq!(call.entry().as_str(), selected.entry);
            }
        }
    }
    Ok(())
}

#[test]
fn named_input_and_dynamic_sector_wire_payloads_remain_typed() -> eltdx::Result<()> {
    use eltdx::f10::*;
    let query = QhIndexInfoQuery::new(FuturesCode::new("IF")?)
        .with_section(SectionCode::new("qh_index_info", "lsjg")?)?
        .to_call()?;
    assert_eq!(query.entry().as_str(), "CWServ.qhf9_gz_zszl_lsjg");
    assert_eq!(
        query.payload(),
        &RequestPayload::Params(vec!["2".into(), "IF".into()])
    );
    let weights = QhWeightsQuery::new(FuturesCode::new("399300")?)
        .with_section(SectionCode::new("qh_weights", "cfghyfb")?)?
        .to_call()?;
    assert_eq!(
        weights.payload(),
        &RequestPayload::Params(vec!["1".into(), "399300".into()])
    );
    let bands = HkPepbBandQuery::new(HongKongCode::new("700")?)
        .with_sjfw("2Y")?
        .to_call()?;
    assert_eq!(
        bands.payload(),
        &RequestPayload::Params(vec![
            "001".into(),
            "00700".into(),
            "2Y".into(),
            "".into(),
            "".into()
        ])
    );
    Ok(())
}

#[test]
fn news_and_cache_definitions_keep_payload_shapes() -> eltdx::Result<()> {
    use eltdx::f10::*;
    let news = MarketNewsQuery::new().to_call()?;
    assert_eq!(news.entry().as_str(), "CWServ.tdxi_zxxwsy");
    assert_eq!(
        news.payload(),
        &RequestPayload::Params(vec!["3".into(), "1".into(), "50".into()])
    );
    let detail = DetailQuery::new("type", "123")?.to_call()?;
    assert_eq!(
        detail.payload(),
        &RequestPayload::Params(vec!["type".into(), "123".into()])
    );
    let company = CompanyNewsQuery::new(AshareCode::new(F10MarketCode::new(1), "600000")?)
        .with_section(SectionCode::new("company_news", "finance_review")?)?
        .with_page(3)?
        .to_call()?;
    assert_eq!(company.entry().as_str(), "CWServ.tdxf10_gg_cwfx_cbdp");
    assert_eq!(
        company.payload(),
        &RequestPayload::Params(vec!["600000".into(), "3".into()])
    );
    let cache = NewsQuery::new(AshareCode::new(F10MarketCode::new(1), "600000")?).to_call()?;
    assert_eq!(cache.entry().as_str(), CACHE_ENTRY);
    assert_eq!(
        cache.payload(),
        &RequestPayload::Body(JsonValue::Object(vec![
            ("action".into(), "get".into()),
            ("key".into(), "xw:1_600000".into()),
            ("bin".into(), "1".into()),
            ("qsid".into(), "tdx".into()),
        ]))
    );
    Ok(())
}

#[test]
fn shared_report_layouts_preserve_nondefault_requests() -> eltdx::Result<()> {
    use eltdx::f10::*;
    let code = AshareCode::new(F10MarketCode::new(1), "600000")?;
    let holders = ShareholdersQuery::new(code.clone())
        .with_section(SectionCode::new("shareholders", "institution_holdings")?)?
        .with_kind("custom")?
        .with_page(3)?
        .to_call()?;
    assert_eq!(holders.entry().as_str(), "CWServ.tdxf10_gg_gdyj_jgcgmx");
    assert_eq!(
        holders.payload(),
        &RequestPayload::Params(vec![
            "600000".into(),
            "custom".into(),
            "".into(),
            "99".into(),
            "3".into(),
            "3".into(),
            "20".into()
        ])
    );
    let business = BusinessReportQuery::new(code.clone())
        .with_section(SectionCode::new("business_report", "composition")?)?
        .to_call()?;
    assert_eq!(
        business.payload(),
        &RequestPayload::Params(vec!["600000".into(), "zygc".into(), "".into()])
    );
    let industry = IndustryAnalysisQuery::new(code.clone())
        .with_section(SectionCode::new("industry_analysis", "comparable")?)?
        .to_call()?;
    assert_eq!(
        industry.payload(),
        &RequestPayload::Params(vec!["1".into(), "600000".into()])
    );
    let fund = FundReportQuery::new(code.clone())
        .with_section(SectionCode::new("fund_report", "finance")?)?
        .to_call()?;
    assert_eq!(
        fund.payload(),
        &RequestPayload::Params(vec!["zcfzb".into(), "600000".into()])
    );
    let market = MarketReportQuery::new(Some(code))
        .with_section(SectionCode::new("market_report", "index_members")?)?
        .to_call()?;
    assert_eq!(market.entry().as_str(), "CWServ.skef10_zs_cfgsj_cfgjcsj");
    assert_eq!(
        market.payload(),
        &RequestPayload::Params(vec!["001".into(), "600000".into(), "1".into(), "20".into()])
    );
    Ok(())
}

#[test]
fn shared_object_layouts_keep_wire_types_and_request_families() -> eltdx::Result<()> {
    use eltdx::f10::*;
    let code = AshareCode::new(F10MarketCode::new(1), "600000")?;
    let stage = StockMainFundsQuery::new(code.clone()).to_call()?;
    assert_eq!(stage.entry().as_str(), "HQServ.hq_nlp_tcihq");
    assert_eq!(
        stage.payload(),
        &RequestPayload::Body(JsonValue::Array(vec![JsonValue::Object(vec![
            ("ReqId".into(), "200742".into()),
            ("setcode".into(), 1_i64.into()),
            ("code".into(), "600000".into()),
            ("Page".into(), (-1_i64).into()),
            ("PageSize".into(), "10".into()),
            ("modname".into(), "mod_tcihq.dll".into()),
            ("zq_num".into(), "30".into()),
        ])]))
    );
    let valuation = StockValuationPercentileQuery::new(code.clone()).to_call()?;
    assert_eq!(valuation.entry().as_str(), "HQServ.hq_nlp_gpsj");
    assert_eq!(
        valuation.payload(),
        &RequestPayload::Body(JsonValue::Array(vec![JsonValue::Object(vec![
            ("ReqId".into(), "200192".into()),
            ("Code".into(), "600000|1".into()),
            ("BeginDate".into(), "0".into()),
            ("EndDate".into(), "0".into()),
            ("Page".into(), "-1".into()),
            ("PageSize".into(), "5".into()),
            ("Type".into(), "0".into()),
            ("modname".into(), "mod_gpsj.dll".into()),
        ])]))
    );
    let digest = DigestQuery::new(code).to_call()?;
    assert_eq!(
        digest.payload(),
        &RequestPayload::Params(vec!["600000".into(), "zxts".into(), "".into()])
    );
    Ok(())
}

#[test]
fn shared_collection_loop_stops_before_next_page_on_success_or_error() {
    use eltdx::f10::collection::collect_pages;
    let mut pages = Vec::new();
    let result = collect_pages(
        1,
        |page| {
            pages.push(*page);
            Ok::<_, &'static str>((*page == 3).then_some("complete"))
        },
        |page| Ok(page + 1),
    );
    assert_eq!(result, Ok("complete"));
    assert_eq!(pages, [1, 2, 3]);
    let mut pages = Vec::new();
    let result = collect_pages(
        1,
        |page| {
            pages.push(*page);
            if *page == 2 {
                Err("cancelled")
            } else {
                Ok(None::<()>)
            }
        },
        |page| Ok(page + 1),
    );
    assert_eq!(result, Err("cancelled"));
    assert_eq!(pages, [1, 2]);
}
