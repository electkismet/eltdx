//! Literal request contracts transcribed from the frozen Python API and
//! extension specification. These cases are authored, not yet executed.
#![cfg(feature = "f10")]

use eltdx::f10::*;
use eltdx::{Result, TradingDate};

fn request(call: CallQuery, entry: &str, params: &[&str]) {
    assert_eq!(call.entry().as_str(), entry);
    assert_eq!(
        call.payload(),
        &RequestPayload::Params(params.iter().map(|value| JsonValue::from(*value)).collect())
    );
}
fn us() -> Result<UsTicker> {
    UsTicker::new("BRK.B")
}
fn hk() -> Result<HongKongCode> {
    HongKongCode::new("700")
}
fn sb() -> Result<ThirdBoardCode> {
    ThirdBoardCode::new("430047")
}
fn bond() -> Result<BondCode> {
    BondCode::new("sz123001")
}
fn industry() -> Result<IndustryCode> {
    IndustryCode::new("881319")
}
fn future() -> Result<FuturesCode> {
    FuturesCode::new("rb")
}

#[test]
fn us_default_entry_inventory_preserves_ticker_dots() -> Result<()> {
    request(
        MgNameQuery::new(us()?).to_call()?,
        "CWServ.skef10_mg_idreq",
        &["name", "BRK.B"],
    );
    request(
        MgTipsQuery::new(us()?).to_call()?,
        "CWServ.skef10_mg_zxdt",
        &["zxdt", "BRK.B", ""],
    );
    request(
        MgProfileQuery::new(us()?).to_call()?,
        "CWServ.skef10_mg_gsgk",
        &["001", "BRK.B"],
    );
    request(
        MgFinanceQuery::new(us()?).to_call()?,
        "CWServ.skef10_mg_cwfx",
        &["zcfzb", "BRK.B"],
    );
    request(
        MgFinanceIndicatorQuery::new(us()?).to_call()?,
        "CWServ.skef10_mg_cwzb",
        &["001", "BRK.B"],
    );
    request(
        MgBusinessQuery::new(us()?).to_call()?,
        "CWServ.skef10_mg_jyfx",
        &["BRK.B"],
    );
    request(
        MgIndustryQuery::new(us()?).to_call()?,
        "CWServ.skef10_mg_hyfx",
        &["001", "BRK.B", "4", "1"],
    );
    request(
        MgIndustryCompareQuery::new(us()?).to_call()?,
        "CWServ.skef10_mg_hydb",
        &["001", "BRK.B", "", "0", "0"],
    );
    request(
        MgDividendQuery::new(us()?).to_call()?,
        "CWServ.skef10_mg_fhrz",
        &["BRK.B", "fh"],
    );
    request(
        MgDirectorsQuery::new(us()?).to_call()?,
        "CWServ.skef10_mg_dsgg",
        &["BRK.B", "ds"],
    );
    request(
        MgAnnouncementsQuery::new(us()?).to_call()?,
        "CWServ.skef10_mg_gsbd",
        &["gsgg", "BRK.B"],
    );
    request(
        MgRatingQuery::new(us()?).to_call()?,
        "CWServ.skef10_mg_gspj",
        &["001", "BRK.B"],
    );
    request(
        MgTopicQuery::new(us()?).to_call()?,
        "CWServ.skef10_mg_rdtc",
        &["001", "BRK.B", "", "4", "1"],
    );
    request(
        MgDeepAnalysisQuery::new(us()?).to_call()?,
        "CWServ.skef10_mg_sdfx",
        &["001", "BRK.B"],
    );
    for (call, entry, expected) in [
        (
            MgCapitalQuery::new(us()?).to_call()?,
            "CWServ.skef10_mg_gbjg",
            br#"{"Params":["001","BRK.B",1]}"#.as_slice(),
        ),
        (
            MgShareholdersQuery::new(us()?).to_call()?,
            "CWServ.skef10_mg_gbgd",
            br#"{"Params":["006","BRK.B",1,10]}"#.as_slice(),
        ),
        (
            MgMarketQuery::new(us()?).to_call()?,
            "CWServ.skef10_mg_schq",
            br#"{"Params":["001","BRK.B",1,20]}"#.as_slice(),
        ),
    ] {
        assert_eq!(call.entry().as_str(), entry);
        let RequestPayload::Params(params) = call.payload() else {
            panic!("expected Params")
        };
        assert_eq!(
            JsonValue::Object(vec![(
                "Params".to_owned(),
                JsonValue::Array(params.clone())
            )])
            .to_json_bytes()?,
            expected
        );
    }
    Ok(())
}

#[test]
fn us_sections_keep_numeric_page_overrides_and_indicator_prefixes() -> Result<()> {
    for (section, wire) in [
        ("jbzb", "001"),
        ("ylnl", "002"),
        ("cznl", "003"),
        ("yynl", "004"),
        ("ccznl", "005"),
    ] {
        request(
            MgFinanceIndicatorQuery::new(us()?)
                .with_section(SectionCode::new("mg_finance_indicator", section)?)?
                .to_call()?,
            "CWServ.skef10_mg_cwzb",
            &[wire, "BRK.B"],
        );
    }
    for section in ["zxdt", "sypl", "gsgy", "zysj"] {
        request(
            MgTipsQuery::new(us()?)
                .with_section(SectionCode::new("mg_tips", section)?)?
                .to_call()?,
            "CWServ.skef10_mg_zxdt",
            &[section, "BRK.B", ""],
        );
    }
    for section in ["zcfzb", "lyb", "xjllb"] {
        request(
            MgFinanceQuery::new(us()?)
                .with_section(SectionCode::new("mg_finance", section)?)?
                .to_call()?,
            "CWServ.skef10_mg_cwfx",
            &[section, "BRK.B"],
        );
    }
    for section in ["fh", "cf"] {
        request(
            MgDividendQuery::new(us()?)
                .with_section(SectionCode::new("mg_dividend", section)?)?
                .to_call()?,
            "CWServ.skef10_mg_fhrz",
            &["BRK.B", section],
        );
    }
    for section in ["ds", "gg", "lrdjg"] {
        request(
            MgDirectorsQuery::new(us()?)
                .with_section(SectionCode::new("mg_directors", section)?)?
                .to_call()?,
            "CWServ.skef10_mg_dsgg",
            &["BRK.B", section],
        );
    }
    let page = std::num::NonZeroU32::new(7).unwrap();
    for (call, expected) in [
        (
            MgCapitalQuery::new(us()?).with_page(9)?.to_call()?,
            br#"["001","BRK.B",1]"#.as_slice(),
        ),
        (
            MgCapitalQuery::new(us()?)
                .with_section(SectionCode::new("mg_capital", "002")?)?
                .with_page(9)?
                .to_call()?,
            br#"["002","BRK.B",9]"#.as_slice(),
        ),
        (
            MgShareholdersQuery::new(us()?)
                .with_page(9)?
                .with_page_size(page)?
                .to_call()?,
            br#"["006","BRK.B",1,10]"#.as_slice(),
        ),
        (
            MgShareholdersQuery::new(us()?)
                .with_section(SectionCode::new("mg_shareholders", "003")?)?
                .with_page(9)?
                .with_page_size(page)?
                .to_call()?,
            br#"["003","BRK.B",9,7]"#.as_slice(),
        ),
        (
            MgMarketQuery::new(us()?)
                .with_section(SectionCode::new("mg_market", "003")?)?
                .with_page(9)?
                .with_page_size(page)?
                .to_call()?,
            br#"["003","BRK.B",0,0]"#.as_slice(),
        ),
    ] {
        let RequestPayload::Params(params) = call.payload() else {
            panic!("expected Params")
        };
        assert_eq!(JsonValue::Array(params.clone()).to_json_bytes()?, expected);
    }
    Ok(())
}

#[test]
fn hong_kong_default_entry_inventory_preserves_five_digit_identity() -> Result<()> {
    request(
        HkNameQuery::new(hk()?).to_call()?,
        "CWServ.skef10_hk_idreq",
        &["name", "00700"],
    );
    request(
        HkTipsQuery::new(hk()?).to_call()?,
        "CWServ.skef10_hk_zxts",
        &["1", "00700"],
    );
    request(
        HkEventsQuery::new(hk()?).to_call()?,
        "CWServ.skef10_hk_gsds",
        &["1", "00700"],
    );
    request(
        HkProfileQuery::new(hk()?).to_call()?,
        "CWServ.skef10_hk_gsgk",
        &["1", "00700"],
    );
    request(
        HkRelatedQuery::new(hk()?).to_call()?,
        "CWServ.skef10_hk_glzq",
        &["6", "00700"],
    );
    request(
        HkExecutivesQuery::new(hk()?).to_call()?,
        "CWServ.skef10_hk_ggzl",
        &["1", "00700"],
    );
    request(
        HkCapitalQuery::new(hk()?).to_call()?,
        "CWServ.skef10_hk_gbjg",
        &["1", "00700"],
    );
    request(
        HkShareholdersQuery::new(hk()?).to_call()?,
        "CWServ.skef10_hk_gdcg",
        &["1", "00700", ""],
    );
    request(
        HkFinanceQuery::new(hk()?).to_call()?,
        "CWServ.skef10_hk_cwfx",
        &["2", "00700"],
    );
    request(
        HkFinanceIndicatorQuery::new(hk()?).to_call()?,
        "CWServ.skef10_hk_cwzb",
        &["1", "00700"],
    );
    request(
        HkBusinessQuery::new(hk()?).to_call()?,
        "CWServ.skef10_hk_jyfx",
        &["1", "00700"],
    );
    request(
        HkIndustryCompareQuery::new(hk()?).to_call()?,
        "CWServ.skef10_hk_hybj",
        &["1", "00700", "0", "0"],
    );
    request(
        HkMarketQuery::new(hk()?).to_call()?,
        "CWServ.skef10_hk_schq",
        &["3", "00700"],
    );
    request(
        HkCapitalFlowQuery::new(hk()?).to_call()?,
        "CWServ.skef10_hk_schq_zjlx",
        &["001", "00700", "0"],
    );
    request(
        HkNewsQuery::new(hk()?).to_call()?,
        "CWServ.skef10_hk_gszx",
        &["3", "00700"],
    );
    request(
        HkTopicQuery::new(hk()?).to_call()?,
        "CWServ.skef10_hk_rdtc",
        &["001", "00700", "", "0", "0"],
    );
    request(
        HkPepbBandQuery::new(hk()?).to_call()?,
        "CWServ.skef10_hk_sdfx_pepb_band",
        &["001", "00700", "", "", ""],
    );
    request(
        HkValuationQuery::new(hk()?).to_call()?,
        "CWServ.skef10_hk_sdfx_gzfx",
        &["00700", ""],
    );
    request(
        HkPriceChartQuery::new(hk()?).to_call()?,
        "CWServ.skef10_hk_sdfx_hqzft",
        &["00700"],
    );
    request(
        HkAnalystForecastQuery::new(hk()?).to_call()?,
        "CWServ.skef10_hk_sdfx_fxysyfx",
        &["00700", "", ""],
    );
    Ok(())
}

#[test]
fn hong_kong_branches_dates_and_explicit_legacy_identity() -> Result<()> {
    for (section, wire) in [("zhsyb", "1"), ("zcfzb", "2"), ("xjllb", "3")] {
        request(
            HkFinanceQuery::new(hk()?)
                .with_section(SectionCode::new("hk_finance", section)?)?
                .to_call()?,
            "CWServ.skef10_hk_cwfx",
            &[wire, "00700"],
        );
    }
    for (section, wire) in [
        ("jbzb", "1"),
        ("ylnl", "2"),
        ("cznl", "3"),
        ("yynl", "4"),
        ("ccznl", "5"),
    ] {
        request(
            HkFinanceIndicatorQuery::new(hk()?)
                .with_section(SectionCode::new("hk_finance_indicator", section)?)?
                .to_call()?,
            "CWServ.skef10_hk_cwzb",
            &[wire, "00700"],
        );
    }
    request(
        HkNameQuery::new(HongKongCode::from_wire("123456")?).to_call()?,
        "CWServ.skef10_hk_idreq",
        &["name", "123456"],
    );
    request(
        HkPepbBandQuery::new(hk()?)
            .with_sjfw("1Y")?
            .with_tab_value("pb")?
            .with_hqzq("week")?
            .to_call()?,
        "CWServ.skef10_hk_sdfx_pepb_band",
        &["001", "00700", "1Y", "pb", "week"],
    );
    request(
        HkValuationQuery::new(hk()?)
            .with_item_id("metric")?
            .to_call()?,
        "CWServ.skef10_hk_sdfx_gzfx",
        &["00700", "metric"],
    );
    request(
        HkAnalystForecastQuery::new(hk()?)
            .with_start(Some(TradingDate::new(2024, 2, 29)?))?
            .with_end(Some(TradingDate::new(2024, 3, 1)?))?
            .to_call()?,
        "CWServ.skef10_hk_sdfx_fxysyfx",
        &["00700", "20240229", "20240301"],
    );
    Ok(())
}

#[test]
fn third_board_default_entry_inventory() -> Result<()> {
    request(
        SbNameQuery::new(sb()?).to_call()?,
        "CWServ.skef10_sb_idreq",
        &["name", "430047"],
    );
    request(
        SbProfileQuery::new(sb()?).to_call()?,
        "CWServ.skef10_sb_gsgk",
        &["430047", "001"],
    );
    request(
        SbTipsQuery::new(sb()?).to_call()?,
        "CWServ.skef10_sb_zxts",
        &["430047", "001"],
    );
    request(
        SbEventsQuery::new(sb()?).to_call()?,
        "CWServ.skef10_sb_jyds",
        &["430047", "ydjy"],
    );
    request(
        SbCapitalQuery::new(sb()?).to_call()?,
        "CWServ.skef10_sb_gbjg",
        &["430047", "gbjg"],
    );
    request(
        SbShareholdersQuery::new(sb()?).to_call()?,
        "CWServ.skef10_sb_gdyj",
        &["430047", "sdgd"],
    );
    request(
        SbComplianceQuery::new(sb()?).to_call()?,
        "CWServ.skef10_sb_hgyy",
        &["430047", "001"],
    );
    request(
        SbDividendQuery::new(sb()?).to_call()?,
        "CWServ.skef10_sb_fhrz",
        &["430047", "fh"],
    );
    request(
        SbFinanceQuery::new(sb()?).to_call()?,
        "CWServ.skef10_sb_cwfx",
        &["430047", "zcfzb"],
    );
    request(
        SbBusinessQuery::new(sb()?).to_call()?,
        "CWServ.skef10_sb_jyfx",
        &["430047", "001"],
    );
    request(
        SbMajorEventsQuery::new(sb()?).to_call()?,
        "CWServ.skef10_sb_zdsx",
        &["430047", "001"],
    );
    request(
        SbNewsQuery::new(sb()?).to_call()?,
        "CWServ.skef10_sb_gszx",
        &["430047", "001"],
    );
    request(
        SbIndustryCompareQuery::new(sb()?).to_call()?,
        "CWServ.skef10_sb_hybj",
        &["430047", "hyxx"],
    );
    Ok(())
}

#[test]
fn bond_default_entry_inventory_keeps_market_parameter_positions() -> Result<()> {
    request(
        BondProfileQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_kzz_zqgk",
        &["sdcyrbgq", "123001", ""],
    );
    request(
        BondFinanceQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_kzz_cwfx",
        &["zcfzb", "123001"],
    );
    request(
        BondRatingQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_kzz_zqpj",
        &["pjzst", "123001"],
    );
    request(
        BondConversionQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_kzz_zg",
        &["zgjg", "123001"],
    );
    request(
        BondRateQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_kzz_zqll",
        &["zqll", "123001"],
    );
    request(
        BondAnnouncementsQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_kzz_zjzx",
        &["123001", "1"],
    );
    request(
        GzProfileQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_gz_zjgk",
        &["zqgk", "123001", "0"],
    );
    request(
        GzInterestQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_gz_fxqk",
        &["fxqk", "123001", "0"],
    );
    request(
        GzMarketDataQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_gz_zjsj",
        &["dzjy", "123001", "0"],
    );
    request(
        GzRatingQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_gz_zjpj",
        &["dfzf", "123001", "0"],
    );
    request(
        GzFinanceQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_gz_cwfx",
        &["t", "123001", "0"],
    );
    request(
        QyzProfileQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_qyz_zqgk",
        &["zqzl", "123001", "0"],
    );
    request(
        QyzInterestQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_qyz_xjlfx",
        &["fxqk", "123001", "0"],
    );
    request(
        QyzGuaranteeQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_qyz_zqdb",
        &["dbsm", "123001", "0"],
    );
    request(
        QyzRatingQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_qyz_zjpj",
        &["zjpjzst", "123001", "0"],
    );
    request(
        QyzRateQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_qyz_zjlv",
        &["zqlv", "123001", "0"],
    );
    request(
        QyzReviewQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_qyz_gsps",
        &["123001", "0"],
    );
    request(
        GzhgInfoQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_gzhg_zqxx",
        &["zqgk", "123001"],
    );
    request(
        GzhgQuotesQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_gzhg_zqxx",
        &["bpzs", "123001"],
    );
    request(
        ZczqProfileQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_zczqh_zqgk",
        &["123001", "0"],
    );
    request(
        ZczqFinanceQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_zczqh_cwfx",
        &["t", "123001", "0"],
    );
    request(
        ZczqRatingQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_zczqh_zqpj",
        &["pjzst", "123001", "0"],
    );
    request(
        KjhzProfileQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_kjhz_zqgk",
        &["123001", "0"],
    );
    request(
        KjhzFinanceQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_kjhz_cwfx",
        &["t", "123001"],
    );
    request(
        KjhzInterestQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_kjhz_fxqk",
        &["fxqk", "123001"],
    );
    request(
        KjhzGuaranteeQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_kjhz_zqdb",
        &["dbsm", "123001"],
    );
    request(
        KjhzRatingQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_kjhz_zqpj",
        &["pjzst", "123001"],
    );
    request(
        KjhzExchangeQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_kjhz_jhqk",
        &["jgtzzst", "123001"],
    );
    request(
        KjhzRateQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_kjhz_zqll",
        &["zqll", "123001"],
    );
    request(
        BondTermsQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_kzz_lltk",
        &["hstk", "123001"],
    );
    request(
        BondBulletinQuery::new(bond()?).to_call()?,
        "CWServ.skef10_bond_zjzx",
        &["123001", "0"],
    );
    request(
        GzProfileQuery::new(BondCode::new("123001")?).to_call()?,
        "CWServ.skef10_bond_gz_zjgk",
        &["zqgk", "123001", "1"],
    );
    Ok(())
}

#[test]
fn industry_and_futures_default_entry_inventory() -> Result<()> {
    request(
        HySummaryQuery::new(industry()?).to_call()?,
        "CWServ.skef10_hy_hqtj",
        &["881319"],
    );
    request(
        HyStageRiseQuery::new(industry()?).to_call()?,
        "CWServ.skef10_hy_jdzf",
        &["881319", "e"],
    );
    request(
        HyProfileQuery::new(industry()?).to_call()?,
        "CWServ.skef10_hy_hyzl_hyjs",
        &["881319"],
    );
    request(
        HyCommentaryQuery::new(industry()?).to_call()?,
        "CWServ.skef10_hy_hyzl_hyplk",
        &["001", "881319"],
    );
    request(
        HyNewsQuery::new(industry()?).to_call()?,
        "CWServ.skef10_hy_zxdt",
        &["hydt", "881319"],
    );
    request(
        HyMembersRankQuery::new(industry()?).to_call()?,
        "CWServ.skef10_hy_hydw_cwzkpm",
        &["002", "881319", "", "e", ""],
    );
    request(
        HyCapitalStatsQuery::new(industry()?).to_call()?,
        "CWServ.skef10_hy_hysj_bxzjtj",
        &["001", "881319", "n"],
    );
    request(
        HyFundHoldingsQuery::new(industry()?).to_call()?,
        "CWServ.skef10_hy_hysj_jjzb",
        &["1", "881319", ""],
    );
    request(
        HyCapitalFlowQuery::new(industry()?).to_call()?,
        "CWServ.skef10_hy_hysj_zjlx",
        &["001", "881319"],
    );
    request(
        HyAnalystIndexQuery::new(industry()?).to_call()?,
        "CWServ.skef10_hy_hyyj_fxszs",
        &["001", "881319", "", ""],
    );
    request(
        HyMajorHoldersQuery::new(industry()?).to_call()?,
        "CWServ.skef10_hy_hyyj_hymzgd",
        &["881319"],
    );
    request(
        HyRatingSummaryQuery::new(industry()?).to_call()?,
        "CWServ.skef10_hy_hyyj_hypjhz",
        &["001", "881319", ""],
    );
    request(
        HyIndexQuery::new(industry()?).to_call()?,
        "CWServ.skef10_hy_idsy",
        &["002", "881319"],
    );
    request(
        QhPositionReportQuery::new(future()?).to_call()?,
        "CWServ.qhf9_sp_ccbg_cjcc",
        &["1", "rb", ""],
    );
    request(
        QhBasisSpreadQuery::new(future()?).to_call()?,
        "CWServ.qhf9_sp_jcjcfx",
        &["001", "rb", "1Y", "", ""],
    );
    request(
        QhWarehouseReceiptQuery::new(future()?).to_call()?,
        "CWServ.qhf9_sp_ckrb_ckrb",
        &["1", "rb", ""],
    );
    request(
        QhInventoryQuery::new(future()?).to_call()?,
        "CWServ.qhf9_sp_kcbg_kcbg",
        &["1", "rb"],
    );
    request(
        QhVarietyInfoQuery::new(future()?).to_call()?,
        "CWServ.qhf9_sp_pzzl_pzjj",
        &["rb", "1"],
    );
    request(
        QhIndexInfoQuery::new(future()?).to_call()?,
        "CWServ.qhf9_gz_zszl_zsgk",
        &["1", "rb"],
    );
    request(
        QhIndexValuationQuery::default().to_call()?,
        "CWServ.qhf9_gz_zsgz_dtpe",
        &["1", "000300", "", ""],
    );
    request(
        QhWeightsQuery::default().to_call()?,
        "CWServ.qhf9_gz_cfg_zxcfg",
        &["1", "399300", ""],
    );
    Ok(())
}

#[test]
fn industry_and_futures_switches_use_each_documented_wire_shape() -> Result<()> {
    for section in ["hqtj", "hyzq", "hygy", "zszfb"] {
        request(
            HySummaryQuery::new(industry()?)
                .with_section(SectionCode::new("hy_summary", section)?)?
                .to_call()?,
            &format!("CWServ.skef10_hy_{section}"),
            &["881319"],
        );
    }
    for (section, params) in [
        ("cwzkpm", vec!["002", "881319", "", "e", ""]),
        ("gsgmpm", vec!["881319"]),
        ("gzsppm", vec!["01", "881319", "o"]),
        ("scbxpm", vec!["scpm", "", "881319"]),
        ("zysrpm", vec!["001", "881319", ""]),
    ] {
        request(
            HyMembersRankQuery::new(industry()?)
                .with_section(SectionCode::new("hy_members_rank", section)?)?
                .to_call()?,
            &format!("CWServ.skef10_hy_hydw_{section}"),
            &params,
        );
    }
    request(
        HyCapitalStatsQuery::new(industry()?)
            .with_section(SectionCode::new("hy_capital_stats", "002")?)?
            .to_call()?,
        "CWServ.skef10_hy_hysj_bxzjtj",
        &["002", "881319", ""],
    );
    let date = TradingDate::new(2024, 6, 28)?;
    let end = TradingDate::new(2024, 7, 1)?;
    for index in [false, true] {
        for (section, params) in [
            ("cjcc", vec!["1", "rb", "20240628"]),
            ("cjjg", vec!["1", "rb", "20240628", ""]),
            ("jcgc", vec!["1", "rb", "20240628", "20240701", ""]),
            ("ccjj", vec!["1", "rb", "20240628", "20240701", ""]),
            ("ykfx", vec!["1", "rb", "20240628", "20240701", ""]),
        ] {
            request(
                QhPositionReportQuery::new(future()?)
                    .with_section(SectionCode::new("qh_position_report", section)?)?
                    .with_date(Some(date))?
                    .with_end_date(Some(end))?
                    .with_index(index)
                    .to_call()?,
                &format!(
                    "CWServ.qhf9_{}_ccbg_{section}",
                    if index { "gz" } else { "sp" }
                ),
                &params,
            );
        }
    }
    for (section, params) in [
        ("pzjj", vec!["rb", "1"]),
        ("hyjj", vec!["rb"]),
        ("xgzq", vec!["rb"]),
    ] {
        request(
            QhVarietyInfoQuery::new(future()?)
                .with_section(SectionCode::new("qh_variety_info", section)?)?
                .to_call()?,
            &format!("CWServ.qhf9_sp_pzzl_{section}"),
            &params,
        );
    }
    request(
        QhWarehouseReceiptQuery::new(future()?)
            .with_date(Some(date))
            .to_call()?,
        "CWServ.qhf9_sp_ckrb_ckrb",
        &["1", "rb", "20240628"],
    );
    request(
        QhIndexInfoQuery::new(future()?)
            .with_section(SectionCode::new("qh_index_info", "lsjg")?)?
            .to_call()?,
        "CWServ.qhf9_gz_zszl_lsjg",
        &["2", "rb"],
    );
    for section in ["dtpe", "lspe", "xjfh"] {
        request(
            QhIndexValuationQuery::default()
                .with_section(SectionCode::new("qh_index_valuation", section)?)?
                .with_start(Some(date))?
                .with_end(Some(end))?
                .to_call()?,
            &format!("CWServ.qhf9_gz_zsgz_{section}"),
            &["1", "000300", "20240628", "20240701"],
        );
    }
    for section in ["zxcfg", "lscfg", "cfgqz", "cfghyfb", "xwpl"] {
        request(
            QhWeightsQuery::default()
                .with_section(SectionCode::new("qh_weights", section)?)?
                .with_date(Some(date))
                .to_call()?,
            &format!("CWServ.qhf9_gz_cfg_{section}"),
            if section == "cfghyfb" {
                &["1", "399300"]
            } else {
                &["1", "399300", "20240628"]
            },
        );
    }
    Ok(())
}

fn ashare() -> Result<AshareCode> {
    AshareCode::new(F10MarketCode::new(1), "600000")
}

#[test]
fn ashare_default_single_request_inventory() -> Result<()> {
    request(
        StockInfoQuery::new(ashare()?).to_call()?,
        "CWServ.tdxf10_gg_comreq",
        &["gpquery", "600000"],
    );
    request(
        BusinessPeriodsQuery::new(ashare()?).to_call()?,
        "CWServ.tdxf10_gg_comreq",
        &["zygcfx", "600000"],
    );
    request(
        TopicIdsQuery::new(ashare()?).to_call()?,
        "CWServ.tdxf10_gg_comreq",
        &["rdtcgn", "600000"],
    );
    request(
        CompanyProfileQuery::new(ashare()?).to_call()?,
        "CWServ.tdxf10_gg_gsgk",
        &["8", "600000", ""],
    );
    request(
        ShareholdersQuery::new(ashare()?).to_call()?,
        "CWServ.tdxf10_gg_gdyj",
        &["600000", "gdrs", "", "", "1", "1", "20"],
    );
    request(
        ShareholderChangePlansQuery::new(ashare()?).to_call()?,
        "CWServ.tdxf10_gg_gdyj",
        &["600000", "gdzjcjh", "", "", "1", "1", "20"],
    );
    request(
        CapitalStructureQuery::new(ashare()?).to_call()?,
        "CWServ.tdxf10_gg_gbjg",
        &["600000", "gbjg"],
    );
    request(
        BusinessReportQuery::new(ashare()?).to_call()?,
        "CWServ.tdxf10_gg_jyfx",
        &["600000", "zyyw", ""],
    );
    request(
        IndustryAnalysisQuery::new(ashare()?).to_call()?,
        "CWServ.tdxf10_gg_hyfx",
        &["cwgz", "600000", "000"],
    );
    request(
        TradingReportQuery::new(ashare()?).to_call()?,
        "CWServ.tdxf10_gg_jyds",
        &["600000", "rzrq", ""],
    );
    request(
        FundReportQuery::new(ashare()?).to_call()?,
        "CWServ.skef10_jj_jjzl",
        &["4", "600000", "", "1", "1", "20"],
    );
    request(
        DividendFinancingQuery::new(ashare()?).to_call()?,
        "CWServ.tdxf10_gg_fhrz",
        &["600000", "fh"],
    );
    request(
        AllotmentDatesQuery::new(ashare()?).to_call()?,
        "CWServ.tdxf10_gg_fhrz_zfhpmx",
        &["zfpg_bgq", "600000", ""],
    );
    request(
        AllotmentDetailsQuery::new(ashare()?, TradingDate::new(2024, 6, 30)?).to_call()?,
        "CWServ.tdxf10_gg_fhrz_zfhpmx",
        &["zfpg", "600000", "20240630"],
    );
    request(
        FinanceReportQuery::new(ashare()?).to_call()?,
        "CWServ.tdxf10_gg_cwfx",
        &["600000", "zcfzb", ""],
    );
    request(
        FinanceDiagnosisQuery::new(ashare()?).to_call()?,
        "CWServ.tdxf10_gg_cwzd",
        &["zzyj", "600000", ""],
    );
    request(
        StockScoreQuery::new(ashare()?).to_call()?,
        "CWServ.tdxf10_gg_ggzp",
        &["pf", "600000", "", ""],
    );
    request(
        ProfitForecastQuery::new(ashare()?).to_call()?,
        "CWServ.tdxf10_gg_ybpj",
        &["600000", "ylyctj"],
    );
    request(
        RankingDetailQuery::new(ashare()?).to_call()?,
        "CWServ.tdxf10_gg_zxts_rqpm",
        &["600000", "scpmdela"],
    );
    request(
        GovernanceQuery::new(ashare()?).to_call()?,
        "CWServ.tdxf10_gg_zbyz",
        &["gljy", "600000", ""],
    );
    request(
        HotTopicsQuery::new(ashare()?).to_call()?,
        "CWServ.tdxf10_gg_rdtc",
        &["600000", "zttzbkz"],
    );
    request(
        TopicCompareQuery::new(ashare()?, "topic")?.to_call()?,
        "CWServ.tdxf10_gg_rdtc_gndb",
        &["gndbzfsj", "600000", "topic", "zdf"],
    );
    request(
        CompanyNewsQuery::new(ashare()?).to_call()?,
        "CWServ.tdxf10_gg_gszx",
        &["600000", "gsyj", "", "0", "1", "20"],
    );
    request(
        DigestQuery::new(ashare()?).to_call()?,
        "CWServ.tdxf10_gg_zxts",
        &["600000", "zxts", ""],
    );
    request(
        NorthboundHoldingQuery::new(ashare()?).to_call()?,
        "CWServ.tdxf10_gg_zlcc",
        &["600000", "bszj", "", "1", "1", "20"],
    );
    request(
        DetailQuery::new("news", "001")?.to_call()?,
        "CWServ.tdxf10_gg_idreq",
        &["news", "001"],
    );
    request(
        MarketNewsQuery::new().to_call()?,
        "CWServ.tdxi_zxxwsy",
        &["3", "1", "50"],
    );
    request(
        StockFinanceWeightsQuery::new(ashare()?).to_call()?,
        "CWServ.tdxf10_gg_zxtscwbq",
        &["600000", "zcfzb"],
    );
    Ok(())
}

#[test]
fn shareholder_capital_business_and_industry_branches() -> Result<()> {
    for (section, wire) in [
        ("count", "gdrs"),
        ("ranking", "thygdrs"),
        ("controller", "kggd"),
        ("top_circulating", "ltgd"),
        ("top", "sdgdbgq"),
        ("bonds", "sdzqcyr"),
        ("changes", "cgbd"),
        ("institutions", "jgcg"),
        ("plan", "gdzjcjh"),
    ] {
        request(
            ShareholdersQuery::new(ashare()?)
                .with_section(SectionCode::new("shareholders", section)?)?
                .with_filter1("f1")?
                .with_filter2("f2")?
                .with_page(3)?
                .with_page_size(std::num::NonZeroU32::new(7).unwrap())?
                .to_call()?,
            "CWServ.tdxf10_gg_gdyj",
            &["600000", wire, "f1", "f2", "3", "3", "7"],
        );
    }
    let date = TradingDate::new(2024, 6, 30)?;
    request(
        ShareholdersQuery::new(ashare()?)
            .with_section(SectionCode::new("shareholders", "institution_types")?)?
            .with_filter1("f1")?
            .with_filter2("ignored")?
            .with_report_date(Some(date))
            .to_call()?,
        "CWServ.tdxf10_gg_gdyj",
        &["600000", "jgcgz", "f1", "20240630", "1", "1", "20"],
    );
    request(
        ShareholdersQuery::new(ashare()?)
            .with_section(SectionCode::new("shareholders", "institution_holdings")?)?
            .with_kind("001")?
            .with_report_date(Some(date))
            .to_call()?,
        "CWServ.tdxf10_gg_gdyj_jgcgmx",
        &["600000", "001", "20240630", "99", "1", "1", "20"],
    );
    for (section, wire) in [
        ("structure", "gbjg"),
        ("changes", "gbbd"),
        ("buyback", "gphg"),
        ("restricted", "xslt"),
    ] {
        request(
            CapitalStructureQuery::new(ashare()?)
                .with_section(SectionCode::new("capital_structure", section)?)?
                .to_call()?,
            "CWServ.tdxf10_gg_gbjg",
            &["600000", wire],
        );
    }
    request(
        CapitalStructureQuery::new(ashare()?)
            .with_section(SectionCode::new("capital_structure", "restricted_detail")?)?
            .with_date(Some(date))
            .to_call()?,
        "CWServ.tdxf10_gg_gbjg_xsjjmx",
        &["600000", "20240630"],
    );
    assert!(CapitalStructureQuery::new(ashare()?)
        .with_section(SectionCode::new("capital_structure", "restricted_detail")?)?
        .to_call()
        .is_err());
    for (section, wire, date_wire) in [
        ("business", "zyyw", ""),
        ("composition", "zygc", "20240630"),
        ("customers", "qwm", "20240630"),
        ("suppliers", "qwmgys", "20240630"),
        ("price", "cpjg", ""),
        ("changes", "0", "20240630"),
    ] {
        request(
            BusinessReportQuery::new(ashare()?)
                .with_section(SectionCode::new("business_report", section)?)?
                .with_report_date(Some(date))
                .to_call()?,
            "CWServ.tdxf10_gg_jyfx",
            &["600000", wire, date_wire],
        );
    }
    for (section, wire, arg) in [
        ("peers", "cwgz", "000"),
        ("dividend_rank", "fhrzb", "100"),
        ("scale", "gsgm", "000"),
        ("valuation", "gzsp", "011"),
        ("news", "hyxw", ""),
        ("reports", "hyyb", ""),
        ("market_share", "scbx", "000"),
        ("summary", "tot", ""),
    ] {
        request(
            IndustryAnalysisQuery::new(ashare()?)
                .with_section(SectionCode::new("industry_analysis", section)?)?
                .to_call()?,
            "CWServ.tdxf10_gg_hyfx",
            &[wire, "600000", arg],
        );
    }
    for (section, wire) in [("comparable", "1"), ("comparable_profile", "0")] {
        request(
            IndustryAnalysisQuery::new(ashare()?)
                .with_section(SectionCode::new("industry_analysis", section)?)?
                .to_call()?,
            "CWServ.tdxf10_gg_kbgs",
            &[wire, "600000"],
        );
    }
    Ok(())
}

#[test]
fn trading_report_ignores_retained_date_and_pagination_inputs() -> Result<()> {
    for (section, wire, arg) in [
        ("margin", "rzrq", ""),
        ("block_trades", "dzjy", ""),
        ("dragon_board", "lgt", "2"),
        ("northbound", "islgt", ""),
        ("limit_down", "dtfx", ""),
        ("limit_up", "ztfx", ""),
        ("bonds", "zrq", ""),
        ("main_force", "zlpskcj", ""),
        ("reports", "yxsbxx", ""),
        ("kcb", "checkkcb", ""),
    ] {
        request(
            TradingReportQuery::new(ashare()?)
                .with_section(SectionCode::new("trading_report", section)?)?
                .with_date(Some(TradingDate::new(2024, 6, 30)?))
                .with_page(9)?
                .to_call()?,
            "CWServ.tdxf10_gg_jyds",
            &["600000", wire, arg],
        );
    }
    request(
        TradingReportQuery::new(ashare()?)
            .with_section(SectionCode::new("trading_report", "margin_history")?)?
            .to_call()?,
        "CWServ.tdxf10_gg_sj",
        &["rzrq_sj", "600000", "0", "600000"],
    );
    Ok(())
}

#[test]
fn all_fund_report_sections_have_literal_parameter_contracts() -> Result<()> {
    let date = DateOrWindow::Date(TradingDate::new(2024, 6, 30)?);
    for (section, suffix, params) in [
        ("name", "idreq", vec!["name", "600000"]),
        ("managers", "jjzl", vec!["1", "600000", "", "1", "1", "20"]),
        ("company", "jjzl", vec!["2", "600000", "", "1", "1", "20"]),
        ("basic", "jjzl", vec!["4", "600000", "", "1", "1", "20"]),
        ("seats", "jjzl", vec!["5", "600000", "", "1", "1", "20"]),
        ("scale", "gmfe", vec!["0", "600000", "", ""]),
        ("scale_history", "gmfe", vec!["0", "600000", "", "20240630"]),
        ("nav", "jzhb", vec!["0", "600000", "0", ""]),
        ("nav_period", "jzhb", vec!["1", "600000", "0", ""]),
        ("nav_rank", "jzhb", vec!["3", "600000", "0", ""]),
        ("dividends", "jzhb", vec!["5", "600000", "0", ""]),
        ("asset_allocation", "tzzh", vec!["zcpz", "600000", ""]),
        (
            "industry_allocation",
            "tzzh",
            vec!["hyfb", "600000", "20240630"],
        ),
        ("holdings_compare", "tzzh", vec!["cgjzd", "600000", ""]),
        ("holdings_rank", "tzzh", vec!["zjpz", "600000", ""]),
        ("holding_summary", "tzzh", vec!["jjxtzzh", "600000", ""]),
        (
            "bond_allocation",
            "tzzh",
            vec!["zczq", "600000", "20240630"],
        ),
        (
            "futures_allocation",
            "tzzh",
            vec!["ccqhmx", "600000", "20240630"],
        ),
        (
            "holdings",
            "cgqk",
            vec!["cgqkbgq", "600000", "", "", "1", "1", "1"],
        ),
        (
            "holdings_detail",
            "cgqk",
            vec!["cgmxbgq", "600000", "", "", "1", "1", "1"],
        ),
        (
            "holdings_changes",
            "cgqk",
            vec!["cyjjmxbgq", "600000", "", "", "1", "1", "1"],
        ),
        (
            "fund_of_funds",
            "cgqk",
            vec!["sxjjzcbgq", "600000", "", "", "1", "1", "1"],
        ),
        ("finance", "cwzk", vec!["zcfzb", "600000"]),
        ("sales_info", "xsxx", vec!["0", "600000"]),
        ("sales_announcements", "xsxx", vec!["4", "600000"]),
        ("sales_news", "xsxx", vec!["5", "600000"]),
        ("etf", "etfzj", vec!["etfzjlx", "600000", "20240630"]),
        ("margin", "etfzj", vec!["etfrzrq", "600000", "20240630"]),
        ("ratings", "jjpj", vec!["001", "600000"]),
        ("categories", "fxsy", vec!["bgqzf", "600000", ""]),
    ] {
        request(
            FundReportQuery::new(ashare()?)
                .with_section(SectionCode::new("fund_report", section)?)?
                .with_date(Some(date.clone()))?
                .to_call()?,
            &format!("CWServ.skef10_jj_{suffix}"),
            &params,
        );
    }
    for (section, wire) in [("etf", "etfzjlx"), ("margin", "etfrzrq")] {
        request(
            FundReportQuery::new(ashare()?)
                .with_section(SectionCode::new("fund_report", section)?)?
                .to_call()?,
            "CWServ.skef10_jj_etfzj",
            &[wire, "600000", "3M"],
        );
    }
    Ok(())
}

#[test]
fn every_market_report_section_and_alias_has_a_literal_request() -> Result<()> {
    for (section, entry, params) in [
        ("board_info", "CWServ.skef10_bk_getinfo", vec!["600000"]),
        (
            "board_basics",
            "CWServ.skef10_bk_cpbd_jczl",
            vec!["001", "600000", ""],
        ),
        (
            "board_description",
            "CWServ.skef10_bk_cpbd_jczl",
            vec!["002", "600000", ""],
        ),
        (
            "board_trading",
            "CWServ.skef10_bk_cpbd_jczl",
            vec!["004", "600000", ""],
        ),
        (
            "board_flow",
            "CWServ.skef10_bk_zjlx",
            vec!["001_1", "600000", ""],
        ),
        (
            "board_members",
            "CWServ.skef10_bk_scgl",
            vec!["001", "600000", "1", "20"],
        ),
        (
            "board_analysis",
            "CWServ.skef10_bk_sdfx",
            vec!["001", "600000", ""],
        ),
        (
            "board_finance_compare",
            "CWServ.skef10_bk_ggdb_cwdb",
            vec!["600000"],
        ),
        (
            "board_candidates",
            "CWServ.skef10_bk_cfgsj",
            vec!["001", "600000"],
        ),
        ("board_trend", "CWServ.skef10_bk_ydfx", vec!["600000"]),
        ("index_info", "CWServ.skef10_zs_getinfo", vec!["600000"]),
        (
            "index_profile",
            "CWServ.skef10_zs_jbxx_zsgk",
            vec!["600000"],
        ),
        (
            "index_valuation",
            "CWServ.skef10_zs_jbxx_gzxx",
            vec!["001", "600000", "", ""],
        ),
        (
            "index_tracking",
            "CWServ.skef10_zs_jbxx_gzbd",
            vec!["600000", "1", "20"],
        ),
        (
            "index_etfs",
            "CWServ.skef10_zs_jbxx_gzbd",
            vec!["600000", "1", "20"],
        ),
        (
            "index_flow",
            "CWServ.skef10_zs_zjdx",
            vec!["004_1", "600000", "", "", "1", "20"],
        ),
        (
            "index_flow_members",
            "CWServ.skef10_zs_cfgsj_cfgjcsj",
            vec!["001", "600000", "1", "20"],
        ),
        (
            "index_members",
            "CWServ.skef10_zs_cfgsj_cfgjcsj",
            vec!["001", "600000", "1", "20"],
        ),
        (
            "index_notices",
            "CWServ.skef10_zs_jrts",
            vec!["001", "600000", "1", "20"],
        ),
        (
            "market_basic",
            "CWServ.skef10_dpzx_jbxx",
            vec!["002", "600000"],
        ),
        (
            "market_margin",
            "CWServ.skef10_dpzx_rzrq",
            vec!["001", "600000"],
        ),
        (
            "market_margin_summary",
            "CWServ.skef10_dpzx_rzrq",
            vec!["002", "600000"],
        ),
        (
            "market_margin_investors",
            "CWServ.skef10_dpzx_rzrq",
            vec!["003", "1m"],
        ),
        ("market_new_shares", "CWServ.skef10_dpzx_xgfx", vec!["002"]),
        ("ipo_calendar", "CWServ.skef10_dpzx_xgfx", vec!["002"]),
        ("market_capital", "CWServ.skef10_dpzx_scgl", vec!["004"]),
    ] {
        let code = if matches!(
            section,
            "market_new_shares" | "ipo_calendar" | "market_capital"
        ) {
            None
        } else {
            Some(ashare()?)
        };
        request(
            MarketReportQuery::new(code)
                .with_section(SectionCode::new("market_report", section)?)?
                .with_date(Some(TradingDate::new(2024, 2, 29)?))
                .to_call()?,
            entry,
            &params,
        );
    }
    request(
        MarketReportQuery::new(Some(ashare()?))
            .with_section(SectionCode::new("market_report", "index_flow")?)?
            .with_arg("custom")?
            .with_page(3)?
            .with_page_size(std::num::NonZeroU32::new(7).unwrap())?
            .to_call()?,
        "CWServ.skef10_zs_zjdx",
        &["custom", "600000", "", "", "3", "7"],
    );
    Ok(())
}

fn object(call: CallQuery, entry: &str, expected: &[u8]) -> Result<()> {
    assert_eq!(call.entry().as_str(), entry);
    assert_eq!(
        call.payload(),
        &RequestPayload::Body(JsonValue::parse(expected)?)
    );
    Ok(())
}

#[test]
fn news_cache_digest_home_and_all_channel_requests() -> Result<()> {
    for (call, kind) in [
        (CacheListQuery::new(ashare()?).to_call()?, "gg"),
        (AnnouncementsQuery::new(ashare()?).to_call()?, "gg"),
        (NewsQuery::new(ashare()?).to_call()?, "xw"),
        (RoadshowsQuery::new(ashare()?).to_call()?, "ly"),
    ] {
        object(
            call,
            "CWSearch.tzx_rcache",
            format!(r#"{{"action":"get","key":"{kind}:1_600000","bin":"1","qsid":"tdx"}}"#)
                .as_bytes(),
        )?;
    }
    for (section, wire) in [
        ("tips", "zxts"),
        ("profile", "gsgy"),
        ("events", "gsds"),
        ("news", "gsxw"),
        ("announcements", "gsgg"),
        ("reports", "gsyj"),
        ("roadshows", "lyhd"),
    ] {
        request(
            DigestQuery::new(ashare()?)
                .with_section(SectionCode::new("digest", section)?)?
                .with_page(7)?
                .to_call()?,
            "CWServ.tdxf10_gg_zxts",
            &["600000", wire, ""],
        );
    }
    request(
        DigestQuery::new(ashare()?)
            .with_section(SectionCode::new("digest", "interaction")?)?
            .with_page(7)?
            .with_page_size(std::num::NonZeroU32::new(9).unwrap())?
            .to_call()?,
        "CWServ.tdxf10_gg_zxtstzzhd",
        &["600000", "1", "1", "9"],
    );
    request(
        CompanyNewsQuery::new(ashare()?)
            .with_section(SectionCode::new("company_news", "finance_review")?)?
            .with_keyword("ignored")?
            .with_rating("2")?
            .with_page(3)?
            .to_call()?,
        "CWServ.tdxf10_gg_cwfx_cbdp",
        &["600000", "3"],
    );
    for id in [1, 2, 3, 4, 5, 6, 7, 9, -987] {
        request(
            MarketNewsQuery::new()
                .with_tab(NewsHomeTab::from_wire(id))
                .to_call()?,
            "CWServ.tdxi_zxxwsy",
            &[&format!("{id}"), "1", "50"],
        );
    }
    // Literal channel-id inventory is independent of the implementation's
    // CHANNELS expansion; removing or renumbering a known channel is visible.
    let channels = [
        200, 201, 202, 203, 204, 205, 206, 207, 211, 212, 213, 300, 301, 302, 303, 306, 318, 304,
        309, 310, 307, 305, 500, 501, 502, 507, 503, 504, 505, 600, 601, 602, 603, 605, 606, 604,
        607, 700, 701, 702, 703, 704, 705, 706, 800, 801, 808, 802, 803, 804, 805, 809, 900, 901,
        902, 903, 904, 905, 906, 907, 908, 910, 909, 1000, 1001, 1002, 1008, 1006, 1011, 1100,
        1101, 1111, 1102, 1103, 1200, 1201, 1202, 1203, 1204, 1205, 1801, 1802, 1803, 1804,
    ];
    assert_eq!(channels.len(), 84);
    let client = F10Client::builder()
        .with_base_url("http://127.0.0.1:1/unused")
        .build()?;
    let inventory = client.news_channels();
    assert_eq!(inventory.len(), 84);
    for id in channels {
        assert!(!matches!(
            NewsChannel::from_wire(id),
            NewsChannel::Unknown(_)
        ));
        assert!(inventory.iter().any(|item| item.id() == id));
        request(
            ChannelNewsQuery::new(NewsChannel::from_wire(id)).to_call()?,
            "CWServ.tdxi_zxxwnews",
            &["1", &format!("{id}"), "1", "50", ""],
        );
    }
    request(
        ChannelNewsQuery::new(NewsChannel::Unknown(-987))
            .with_keyword("中文")?
            .to_call()?,
        "CWServ.tdxi_zxxwnews",
        &["1", "-987", "1", "50", "中文"],
    );
    assert_eq!(client.diagnostics()?.completed_requests(), 0);
    client.close()?;
    Ok(())
}

#[test]
fn themes_stage_wrappers_and_valuation_request_ids_keep_mixed_json_types() -> Result<()> {
    object(ThemeMarketQuery::new(ashare()?).to_call()?, "HQServ.hq_nlp_tcihq",
        br#"[{"ReqId":"200743","setcode":1,"code":"600000","Page":-1,"PageSize":"10","modname":"mod_tcihq.dll"}]"#)?;
    for (call, id, days) in [
        (
            StockStageGainQuery::new(ashare()?).to_call()?,
            "200740",
            "60",
        ),
        (
            StockMainFundsQuery::new(ashare()?).to_call()?,
            "200742",
            "30",
        ),
        (
            StockIndustryRankQuery::new(ashare()?).to_call()?,
            "200745",
            "30",
        ),
        (
            StockIndustryPctRankQuery::new(ashare()?).to_call()?,
            "200747",
            "30",
        ),
    ] {
        object(call, "HQServ.hq_nlp_tcihq", format!(r#"[{{"ReqId":"{id}","setcode":1,"code":"600000","Page":-1,"PageSize":"10","modname":"mod_tcihq.dll","zq_num":"{days}"}}]"#).as_bytes())?;
    }
    object(StockBoardMemberQuotesQuery::new(ashare()?).to_call()?, "HQServ.hq_nlp_tcihq",
        br#"[{"ReqId":"200744","setcode":"1","code":"600000","Page":-1,"PageSize":"-1","modname":"mod_tcihq.dll"}]"#)?;
    object(ValuationQuery::new(ashare()?).to_call()?, "HQServ.hq_nlp_gpsj",
        br#"[{"ReqId":"200191","Code":"600000|1","BeginDate":"0","EndDate":"0","Page":"0","PageSize":"20","modname":"mod_gpsj.dll"}]"#)?;
    object(StockValuationPercentileQuery::new(ashare()?).to_call()?, "HQServ.hq_nlp_gpsj",
        br#"[{"ReqId":"200192","Code":"600000|1","BeginDate":"0","EndDate":"0","Page":"-1","PageSize":"5","Type":"0","modname":"mod_gpsj.dll"}]"#)?;
    object(StockPepbPercentileQuery::new(ashare()?).to_call()?, "HQServ.hq_nlp_gpsj",
        br#"[{"ReqId":"200124","code":"600000","setcode":"1","sjfw":"1Y","sdate":"","edate":"","zb":"0","hqzq":"0","cwppgz":1,"jglx":0,"Page":1,"PageSize":"300","modname":"mod_gpsj.dll"}]"#)?;
    object(
        StockHistoryTrendQuery::new(ashare()?).to_call()?,
        "HQServ.hq_nlp_gpsj",
        br#"[{"ReqId":"200135","code":"600000","setcode":"1","modname":"mod_gpsj.dll"}]"#,
    )?;
    object(ValuationQuery::new(ashare()?).with_req_id(RequestId::new("200192")?).to_call()?, "HQServ.hq_nlp_gpsj",
        br#"[{"ReqId":"200192","Code":"600000|1","BeginDate":"0","EndDate":"0","Page":"-1","PageSize":"5","Type":"0","modname":"mod_gpsj.dll"}]"#)?;
    object(ValuationQuery::new(ashare()?).with_req_id(RequestId::new("200124")?).to_call()?, "HQServ.hq_nlp_gpsj",
        br#"[{"ReqId":"200124","code":"600000","setcode":"1","sjfw":"1Y","sdate":"","edate":"","zb":"0","hqzq":"0","cwppgz":1,"jglx":0,"Page":1,"PageSize":"300","modname":"mod_gpsj.dll"}]"#)?;
    object(
        ValuationQuery::new(ashare()?)
            .with_req_id(RequestId::new("999999")?)
            .to_call()?,
        "HQServ.hq_nlp_gpsj",
        br#"[{"ReqId":"999999","code":"600000","setcode":"1","modname":"mod_gpsj.dll"}]"#,
    )?;
    for protected in [
        "ReqId", "Code", "code", "setcode", "Page", "PageSize", "modname",
    ] {
        assert!(ThemeMarketQuery::new(ashare()?)
            .with_extra(vec![(protected.to_owned(), "override".into())])
            .is_err());
        assert!(ValuationQuery::new(ashare()?)
            .with_extra(vec![(protected.to_owned(), "override".into())])
            .is_err());
    }
    assert!(ValuationQuery::new(ashare()?)
        .with_extra(vec![
            ("filter".to_owned(), 1_i64.into()),
            ("filter".to_owned(), 2_i64.into())
        ])
        .is_err());
    Ok(())
}

#[test]
fn open_section_families_preserve_explicit_wire_selectors() -> Result<()> {
    macro_rules! open {
        ($query:ident, $method:literal, $code:expr, $entry:literal, [$($prefix:expr),*], [$($suffix:expr),*]) => {
            request($query::new($code).with_section(SectionCode::new($method, "future_tag")?)?.to_call()?,
                $entry, &[$($prefix,)* "future_tag", $($suffix),*]);
        };
    }
    open!(
        MgRatingQuery,
        "mg_rating",
        us()?,
        "CWServ.skef10_mg_gspj",
        [],
        ["BRK.B"]
    );
    open!(
        MgDeepAnalysisQuery,
        "mg_deep_analysis",
        us()?,
        "CWServ.skef10_mg_sdfx",
        [],
        ["BRK.B"]
    );
    for (call, entry, suffix) in [
        (
            HkTipsQuery::new(hk()?)
                .with_section(SectionCode::new("hk_tips", "future_tag")?)?
                .to_call()?,
            "CWServ.skef10_hk_zxts",
            false,
        ),
        (
            HkEventsQuery::new(hk()?)
                .with_section(SectionCode::new("hk_events", "future_tag")?)?
                .to_call()?,
            "CWServ.skef10_hk_gsds",
            false,
        ),
        (
            HkProfileQuery::new(hk()?)
                .with_section(SectionCode::new("hk_profile", "future_tag")?)?
                .to_call()?,
            "CWServ.skef10_hk_gsgk",
            false,
        ),
        (
            HkRelatedQuery::new(hk()?)
                .with_section(SectionCode::new("hk_related", "future_tag")?)?
                .to_call()?,
            "CWServ.skef10_hk_glzq",
            false,
        ),
        (
            HkExecutivesQuery::new(hk()?)
                .with_section(SectionCode::new("hk_executives", "future_tag")?)?
                .to_call()?,
            "CWServ.skef10_hk_ggzl",
            false,
        ),
        (
            HkCapitalQuery::new(hk()?)
                .with_section(SectionCode::new("hk_capital", "future_tag")?)?
                .to_call()?,
            "CWServ.skef10_hk_gbjg",
            false,
        ),
        (
            HkShareholdersQuery::new(hk()?)
                .with_section(SectionCode::new("hk_shareholders", "future_tag")?)?
                .to_call()?,
            "CWServ.skef10_hk_gdcg",
            true,
        ),
        (
            HkBusinessQuery::new(hk()?)
                .with_section(SectionCode::new("hk_business", "future_tag")?)?
                .to_call()?,
            "CWServ.skef10_hk_jyfx",
            false,
        ),
        (
            HkMarketQuery::new(hk()?)
                .with_section(SectionCode::new("hk_market", "future_tag")?)?
                .to_call()?,
            "CWServ.skef10_hk_schq",
            false,
        ),
        (
            HkNewsQuery::new(hk()?)
                .with_section(SectionCode::new("hk_news", "future_tag")?)?
                .to_call()?,
            "CWServ.skef10_hk_gszx",
            false,
        ),
    ] {
        request(
            call,
            entry,
            if suffix {
                &["future_tag", "00700", ""]
            } else {
                &["future_tag", "00700"]
            },
        );
    }
    open!(
        HkIndustryCompareQuery,
        "hk_industry_compare",
        hk()?,
        "CWServ.skef10_hk_hybj",
        [],
        ["00700", "0", "0"]
    );
    open!(
        HkCapitalFlowQuery,
        "hk_capital_flow",
        hk()?,
        "CWServ.skef10_hk_schq_zjlx",
        [],
        ["00700", "0"]
    );
    open!(
        SbProfileQuery,
        "sb_profile",
        sb()?,
        "CWServ.skef10_sb_gsgk",
        ["430047"],
        []
    );
    open!(
        SbTipsQuery,
        "sb_tips",
        sb()?,
        "CWServ.skef10_sb_zxts",
        ["430047"],
        []
    );
    open!(
        SbEventsQuery,
        "sb_events",
        sb()?,
        "CWServ.skef10_sb_jyds",
        ["430047"],
        []
    );
    open!(
        SbCapitalQuery,
        "sb_capital",
        sb()?,
        "CWServ.skef10_sb_gbjg",
        ["430047"],
        []
    );
    open!(
        SbShareholdersQuery,
        "sb_shareholders",
        sb()?,
        "CWServ.skef10_sb_gdyj",
        ["430047"],
        []
    );
    open!(
        SbComplianceQuery,
        "sb_compliance",
        sb()?,
        "CWServ.skef10_sb_hgyy",
        ["430047"],
        []
    );
    open!(
        SbDividendQuery,
        "sb_dividend",
        sb()?,
        "CWServ.skef10_sb_fhrz",
        ["430047"],
        []
    );
    open!(
        SbFinanceQuery,
        "sb_finance",
        sb()?,
        "CWServ.skef10_sb_cwfx",
        ["430047"],
        []
    );
    open!(
        SbBusinessQuery,
        "sb_business",
        sb()?,
        "CWServ.skef10_sb_jyfx",
        ["430047"],
        []
    );
    open!(
        SbMajorEventsQuery,
        "sb_major_events",
        sb()?,
        "CWServ.skef10_sb_zdsx",
        ["430047"],
        []
    );
    open!(
        SbNewsQuery,
        "sb_news",
        sb()?,
        "CWServ.skef10_sb_gszx",
        ["430047"],
        []
    );
    open!(
        SbIndustryCompareQuery,
        "sb_industry_compare",
        sb()?,
        "CWServ.skef10_sb_hybj",
        ["430047"],
        []
    );
    open!(
        BondFinanceQuery,
        "bond_finance",
        bond()?,
        "CWServ.skef10_bond_kzz_cwfx",
        [],
        ["123001"]
    );
    open!(
        BondRatingQuery,
        "bond_rating",
        bond()?,
        "CWServ.skef10_bond_kzz_zqpj",
        [],
        ["123001"]
    );
    open!(
        BondConversionQuery,
        "bond_conversion",
        bond()?,
        "CWServ.skef10_bond_kzz_zg",
        [],
        ["123001"]
    );
    open!(
        BondRateQuery,
        "bond_rate",
        bond()?,
        "CWServ.skef10_bond_kzz_zqll",
        [],
        ["123001"]
    );
    open!(
        BondAnnouncementsQuery,
        "bond_announcements",
        bond()?,
        "CWServ.skef10_bond_kzz_zjzx",
        ["123001"],
        []
    );
    open!(
        GzProfileQuery,
        "gz_profile",
        bond()?,
        "CWServ.skef10_bond_gz_zjgk",
        [],
        ["123001", "0"]
    );
    open!(
        GzInterestQuery,
        "gz_interest",
        bond()?,
        "CWServ.skef10_bond_gz_fxqk",
        [],
        ["123001", "0"]
    );
    open!(
        GzMarketDataQuery,
        "gz_market_data",
        bond()?,
        "CWServ.skef10_bond_gz_zjsj",
        [],
        ["123001", "0"]
    );
    open!(
        GzRatingQuery,
        "gz_rating",
        bond()?,
        "CWServ.skef10_bond_gz_zjpj",
        [],
        ["123001", "0"]
    );
    open!(
        QyzProfileQuery,
        "qyz_profile",
        bond()?,
        "CWServ.skef10_bond_qyz_zqgk",
        [],
        ["123001", "0"]
    );
    open!(
        QyzInterestQuery,
        "qyz_interest",
        bond()?,
        "CWServ.skef10_bond_qyz_xjlfx",
        [],
        ["123001", "0"]
    );
    open!(
        QyzRatingQuery,
        "qyz_rating",
        bond()?,
        "CWServ.skef10_bond_qyz_zjpj",
        [],
        ["123001", "0"]
    );
    open!(
        GzhgQuotesQuery,
        "gzhg_quotes",
        bond()?,
        "CWServ.skef10_bond_gzhg_zqxx",
        [],
        ["123001"]
    );
    open!(
        ZczqRatingQuery,
        "zczq_rating",
        bond()?,
        "CWServ.skef10_bond_zczqh_zqpj",
        [],
        ["123001", "0"]
    );
    open!(
        KjhzInterestQuery,
        "kjhz_interest",
        bond()?,
        "CWServ.skef10_bond_kjhz_fxqk",
        [],
        ["123001"]
    );
    open!(
        KjhzRatingQuery,
        "kjhz_rating",
        bond()?,
        "CWServ.skef10_bond_kjhz_zqpj",
        [],
        ["123001"]
    );
    open!(
        KjhzExchangeQuery,
        "kjhz_exchange",
        bond()?,
        "CWServ.skef10_bond_kjhz_jhqk",
        [],
        ["123001"]
    );
    open!(
        KjhzRateQuery,
        "kjhz_rate",
        bond()?,
        "CWServ.skef10_bond_kjhz_zqll",
        [],
        ["123001"]
    );
    open!(
        BondTermsQuery,
        "bond_terms",
        bond()?,
        "CWServ.skef10_bond_kzz_lltk",
        [],
        ["123001"]
    );
    open!(
        HyCommentaryQuery,
        "hy_commentary",
        industry()?,
        "CWServ.skef10_hy_hyzl_hyplk",
        [],
        ["881319"]
    );
    open!(
        HyNewsQuery,
        "hy_news",
        industry()?,
        "CWServ.skef10_hy_zxdt",
        [],
        ["881319"]
    );
    open!(
        HyCapitalStatsQuery,
        "hy_capital_stats",
        industry()?,
        "CWServ.skef10_hy_hysj_bxzjtj",
        [],
        ["881319", ""]
    );
    open!(
        HyFundHoldingsQuery,
        "hy_fund_holdings",
        industry()?,
        "CWServ.skef10_hy_hysj_jjzb",
        [],
        ["881319", ""]
    );
    open!(
        HyCapitalFlowQuery,
        "hy_capital_flow",
        industry()?,
        "CWServ.skef10_hy_hysj_zjlx",
        [],
        ["881319"]
    );
    open!(
        HyAnalystIndexQuery,
        "hy_analyst_index",
        industry()?,
        "CWServ.skef10_hy_hyyj_fxszs",
        [],
        ["881319", "", ""]
    );
    open!(
        HyRatingSummaryQuery,
        "hy_rating_summary",
        industry()?,
        "CWServ.skef10_hy_hyyj_hypjhz",
        [],
        ["881319", ""]
    );
    open!(
        CompanyProfileQuery,
        "company_profile",
        ashare()?,
        "CWServ.tdxf10_gg_gsgk",
        [],
        ["600000", ""]
    );
    open!(
        DividendFinancingQuery,
        "dividend_financing",
        ashare()?,
        "CWServ.tdxf10_gg_fhrz",
        ["600000"],
        []
    );
    open!(
        RankingDetailQuery,
        "ranking_detail",
        ashare()?,
        "CWServ.tdxf10_gg_zxts_rqpm",
        ["600000"],
        []
    );
    open!(
        HotTopicsQuery,
        "hot_topics",
        ashare()?,
        "CWServ.tdxf10_gg_rdtc",
        ["600000"],
        []
    );
    open!(
        StockFinanceWeightsQuery,
        "stock_finance_weights",
        ashare()?,
        "CWServ.tdxf10_gg_zxtscwbq",
        ["600000"],
        []
    );
    open!(
        FinanceDiagnosisQuery,
        "finance_diagnosis",
        ashare()?,
        "CWServ.tdxf10_gg_cwzd",
        [],
        ["600000", ""]
    );
    open!(
        StockScoreQuery,
        "stock_score",
        ashare()?,
        "CWServ.tdxf10_gg_ggzp",
        [],
        ["600000", "", ""]
    );
    open!(
        GovernanceQuery,
        "governance",
        ashare()?,
        "CWServ.tdxf10_gg_zbyz",
        [],
        ["600000", ""]
    );
    open!(
        CompanyNewsQuery,
        "company_news",
        ashare()?,
        "CWServ.tdxf10_gg_gszx",
        ["600000"],
        ["", "0", "1", "20"]
    );
    open!(
        NorthboundHoldingQuery,
        "northbound_holding",
        ashare()?,
        "CWServ.tdxf10_gg_zlcc",
        ["600000"],
        ["", "1", "1", "20"]
    );
    request(
        TopicCompareQuery::new(ashare()?, "topic")?
            .with_section(SectionCode::new("topic_compare", "future_tag")?)?
            .to_call()?,
        "CWServ.tdxf10_gg_rdtc_gndb",
        &["future_tag", "600000", "topic", "zdf"],
    );
    // US shareholder pages remain JSON numbers even for an unknown selector.
    let call = MgShareholdersQuery::new(us()?)
        .with_section(SectionCode::new("mg_shareholders", "future_tag")?)?
        .to_call()?;
    assert_eq!(call.entry().as_str(), "CWServ.skef10_mg_gbgd");
    let RequestPayload::Params(params) = call.payload() else {
        unreachable!()
    };
    assert_eq!(
        params,
        &vec![
            "future_tag".into(),
            "BRK.B".into(),
            1_i64.into(),
            10_i64.into()
        ]
    );
    request(
        QhBasisSpreadQuery::new(future()?)
            .with_section(SectionCode::new("qh_basis_spread", "future_tag")?)?
            .with_period("6M")?
            .to_call()?,
        "CWServ.qhf9_sp_jcjcfx",
        &["future_tag", "rb", "6M", "", ""],
    );
    Ok(())
}

#[test]
fn checked_selectors_reject_unknown_sections_and_cross_method_reuse() -> Result<()> {
    for method in [
        "shareholders",
        "capital_structure",
        "business_report",
        "industry_analysis",
        "trading_report",
        "fund_report",
        "mg_tips",
        "mg_finance",
        "mg_finance_indicator",
        "mg_dividend",
        "mg_capital",
        "mg_directors",
        "mg_market",
        "hk_finance",
        "hk_finance_indicator",
        "hy_summary",
        "hy_members_rank",
        "qh_position_report",
        "qh_variety_info",
        "qh_index_info",
        "qh_index_valuation",
        "qh_weights",
        "market_report",
        "profit_forecast",
        "digest",
    ] {
        assert!(SectionCode::new(method, "future_tag").is_err(), "{method}");
    }
    assert!(MgTipsQuery::new(us()?)
        .with_section(SectionCode::new("hk_tips", "zxdt")?)
        .is_err());
    for section in ["ylyctj", "ylycmx", "yzyq", "tzpjtj", "ycpjyjbg"] {
        request(
            ProfitForecastQuery::new(ashare()?)
                .with_section(SectionCode::new("profit_forecast", section)?)?
                .to_call()?,
            "CWServ.tdxf10_gg_ybpj",
            &["600000", section],
        );
    }
    Ok(())
}

#[test]
fn nondefault_free_fields_and_wrapper_parameters_are_not_silently_ignored() -> Result<()> {
    let size = std::num::NonZeroU32::new(7).unwrap();
    request(
        ShareholderChangePlansQuery::new(ashare()?)
            .with_page(3)?
            .with_page_size(size)?
            .with_filter1("plan")?
            .with_filter2("active")?
            .to_call()?,
        "CWServ.tdxf10_gg_gdyj",
        &["600000", "gdzjcjh", "plan", "active", "3", "3", "7"],
    );
    request(
        FinanceReportQuery::new(ashare()?)
            .with_report_type("xjllb")?
            .to_call()?,
        "CWServ.tdxf10_gg_cwfx",
        &["600000", "xjllb", ""],
    );
    request(
        FinanceDiagnosisQuery::new(ashare()?)
            .with_scope("scope")?
            .to_call()?,
        "CWServ.tdxf10_gg_cwzd",
        &["zzyj", "600000", "scope"],
    );
    request(
        StockScoreQuery::new(ashare()?)
            .with_arg("range")?
            .to_call()?,
        "CWServ.tdxf10_gg_ggzp",
        &["pf", "600000", "range", ""],
    );
    request(
        GovernanceQuery::new(ashare()?)
            .with_arg("range")?
            .to_call()?,
        "CWServ.tdxf10_gg_zbyz",
        &["gljy", "600000", "range"],
    );
    request(
        IndustryAnalysisQuery::new(ashare()?)
            .with_arg("custom")?
            .to_call()?,
        "CWServ.tdxf10_gg_hyfx",
        &["cwgz", "600000", "custom"],
    );
    request(
        TradingReportQuery::new(ashare()?)
            .with_section(SectionCode::new("trading_report", "dragon_board")?)?
            .with_arg("1")?
            .to_call()?,
        "CWServ.tdxf10_gg_jyds",
        &["600000", "lgt", "1"],
    );
    request(
        CompanyNewsQuery::new(ashare()?)
            .with_keyword("keyword")?
            .with_rating("2")?
            .with_page(3)?
            .with_page_size(size)?
            .to_call()?,
        "CWServ.tdxf10_gg_gszx",
        &["600000", "gsyj", "keyword", "2", "3", "7"],
    );
    request(
        NorthboundHoldingQuery::new(ashare()?)
            .with_filter_value("filter")?
            .with_page(3)?
            .with_page_size(size)?
            .to_call()?,
        "CWServ.tdxf10_gg_zlcc",
        &["600000", "bszj", "filter", "3", "3", "7"],
    );
    request(
        ChannelNewsQuery::new(NewsChannel::FinanceLatest)
            .with_query1("2")?
            .with_page(3)?
            .with_page_size(size)?
            .with_keyword("keyword")?
            .to_call()?,
        "CWServ.tdxi_zxxwnews",
        &["2", "200", "3", "7", "keyword"],
    );
    request(
        MarketNewsQuery::new()
            .with_page(3)?
            .with_page_size(size)?
            .to_call()?,
        "CWServ.tdxi_zxxwsy",
        &["3", "3", "7"],
    );
    request(
        HyStageRiseQuery::new(industry()?)
            .with_flag("m")?
            .to_call()?,
        "CWServ.skef10_hy_jdzf",
        &["881319", "m"],
    );
    for (section, suffix, params) in [
        ("seats", "jjzl", vec!["5", "600000", "arg", "3", "3", "7"]),
        ("scale_history", "gmfe", vec!["arg", "600000", "", "6M"]),
        ("finance", "cwzk", vec!["arg", "600000"]),
        ("ratings", "jjpj", vec!["arg", "600000"]),
        ("categories", "fxsy", vec!["arg", "600000", ""]),
    ] {
        let mut query = FundReportQuery::new(ashare()?)
            .with_section(SectionCode::new("fund_report", section)?)?
            .with_arg("arg")?
            .with_page(3)?
            .with_page_size(size)?;
        if section == "scale_history" {
            query = query.with_date(Some(DateOrWindow::Window(FundWindow::new("6M")?)))?;
        }
        request(
            query.to_call()?,
            &format!("CWServ.skef10_jj_{suffix}"),
            &params,
        );
    }
    object(StockStageGainQuery::new(ashare()?).with_days(size)?.with_page_size(size)?.to_call()?, "HQServ.hq_nlp_tcihq",
        br#"[{"ReqId":"200740","setcode":1,"code":"600000","Page":-1,"PageSize":"7","modname":"mod_tcihq.dll","zq_num":"7"}]"#)?;
    object(StockBoardMemberQuotesQuery::new(ashare()?).with_board_market(F10MarketCode::new(0))
        .with_page(PageSelector::Number(3))?.with_page_size(PageSizeSelector::Count(size))?.to_call()?, "HQServ.hq_nlp_tcihq",
        br#"[{"ReqId":"200744","setcode":"0","code":"600000","Page":3,"PageSize":"7","modname":"mod_tcihq.dll"}]"#)?;
    object(StockValuationPercentileQuery::new(ashare()?).with_kind("2")?.with_page(PageSelector::Number(3))?
        .with_page_size(std::num::NonZeroU32::new(20).unwrap())?.to_call()?, "HQServ.hq_nlp_gpsj",
        br#"[{"ReqId":"200192","Code":"600000|1","BeginDate":"0","EndDate":"0","Page":"3","PageSize":"20","Type":"2","modname":"mod_gpsj.dll"}]"#)?;
    object(StockPepbPercentileQuery::new(ashare()?).with_indicator("1")?.with_window("3Y")?.with_period("2")?
        .with_page(3)?.with_page_size(size)?.to_call()?, "HQServ.hq_nlp_gpsj",
        br#"[{"ReqId":"200124","code":"600000","setcode":"1","sjfw":"3Y","sdate":"","edate":"","zb":"1","hqzq":"2","cwppgz":1,"jglx":0,"Page":3,"PageSize":"7","modname":"mod_gpsj.dll"}]"#)?;
    Ok(())
}
