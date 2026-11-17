#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Public-method integration layer, complementary to the independent literal
//! wire snapshots in f10_route_inventory. Authored before final verification.
#![cfg(feature = "f10")]

use eltdx::f10::*;
use eltdx::{OperationOptions, RawPolicy, TradingDate};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::{Duration, Instant};

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;
type Invocation = Box<dyn Fn(&F10Client, &OperationOptions) -> eltdx::Result<Response>>;
struct Case {
    name: &'static str,
    expected: CallQuery,
    invoke: Invocation,
}

// The separate literal inventory fixes query-to-wire behavior. This suite
// checks that the actual named public methods execute those query objects,
// select the configured route, and retain their own response structure.
macro_rules! case {
    ($method:ident, $query:expr) => {{
        let query = $query;
        Case {
            name: stringify!($method),
            expected: query.to_call()?,
            invoke: Box::new(move |client, options| client.$method(&query, options)),
        }
    }};
}
fn ashare() -> eltdx::Result<AshareCode> {
    AshareCode::new(F10MarketCode::new(1), "600000")
}
fn us() -> eltdx::Result<UsTicker> {
    UsTicker::new("BRK.B")
}
fn hk() -> eltdx::Result<HongKongCode> {
    HongKongCode::new("700")
}
fn sb() -> eltdx::Result<ThirdBoardCode> {
    ThirdBoardCode::new("430047")
}
fn bond() -> eltdx::Result<BondCode> {
    BondCode::new("sz123001")
}
fn industry() -> eltdx::Result<IndustryCode> {
    IndustryCode::new("881319")
}
fn future() -> eltdx::Result<FuturesCode> {
    FuturesCode::new("rb")
}

fn international_cases() -> eltdx::Result<Vec<Case>> {
    Ok(vec![
        case!(mg_name, MgNameQuery::new(us()?)),
        case!(mg_tips, MgTipsQuery::new(us()?)),
        case!(mg_profile, MgProfileQuery::new(us()?)),
        case!(mg_finance, MgFinanceQuery::new(us()?)),
        case!(mg_finance_indicator, MgFinanceIndicatorQuery::new(us()?)),
        case!(mg_business, MgBusinessQuery::new(us()?)),
        case!(mg_industry, MgIndustryQuery::new(us()?)),
        case!(mg_industry_compare, MgIndustryCompareQuery::new(us()?)),
        case!(mg_dividend, MgDividendQuery::new(us()?)),
        case!(mg_capital, MgCapitalQuery::new(us()?)),
        case!(mg_shareholders, MgShareholdersQuery::new(us()?)),
        case!(mg_directors, MgDirectorsQuery::new(us()?)),
        case!(mg_market, MgMarketQuery::new(us()?)),
        case!(mg_announcements, MgAnnouncementsQuery::new(us()?)),
        case!(mg_rating, MgRatingQuery::new(us()?)),
        case!(mg_topic, MgTopicQuery::new(us()?)),
        case!(mg_deep_analysis, MgDeepAnalysisQuery::new(us()?)),
        case!(hk_name, HkNameQuery::new(hk()?)),
        case!(hk_tips, HkTipsQuery::new(hk()?)),
        case!(hk_events, HkEventsQuery::new(hk()?)),
        case!(hk_profile, HkProfileQuery::new(hk()?)),
        case!(hk_related, HkRelatedQuery::new(hk()?)),
        case!(hk_executives, HkExecutivesQuery::new(hk()?)),
        case!(hk_capital, HkCapitalQuery::new(hk()?)),
        case!(hk_shareholders, HkShareholdersQuery::new(hk()?)),
        case!(hk_finance, HkFinanceQuery::new(hk()?)),
        case!(hk_finance_indicator, HkFinanceIndicatorQuery::new(hk()?)),
        case!(hk_business, HkBusinessQuery::new(hk()?)),
        case!(hk_industry_compare, HkIndustryCompareQuery::new(hk()?)),
        case!(hk_market, HkMarketQuery::new(hk()?)),
        case!(hk_capital_flow, HkCapitalFlowQuery::new(hk()?)),
        case!(hk_news, HkNewsQuery::new(hk()?)),
        case!(hk_topic, HkTopicQuery::new(hk()?)),
        case!(hk_pepb_band, HkPepbBandQuery::new(hk()?)),
        case!(hk_valuation, HkValuationQuery::new(hk()?)),
        case!(hk_price_chart, HkPriceChartQuery::new(hk()?)),
        case!(hk_analyst_forecast, HkAnalystForecastQuery::new(hk()?)),
        case!(sb_name, SbNameQuery::new(sb()?)),
        case!(sb_profile, SbProfileQuery::new(sb()?)),
        case!(sb_tips, SbTipsQuery::new(sb()?)),
        case!(sb_events, SbEventsQuery::new(sb()?)),
        case!(sb_capital, SbCapitalQuery::new(sb()?)),
        case!(sb_shareholders, SbShareholdersQuery::new(sb()?)),
        case!(sb_compliance, SbComplianceQuery::new(sb()?)),
        case!(sb_dividend, SbDividendQuery::new(sb()?)),
        case!(sb_finance, SbFinanceQuery::new(sb()?)),
        case!(sb_business, SbBusinessQuery::new(sb()?)),
        case!(sb_major_events, SbMajorEventsQuery::new(sb()?)),
        case!(sb_news, SbNewsQuery::new(sb()?)),
        case!(sb_industry_compare, SbIndustryCompareQuery::new(sb()?)),
    ])
}
fn bond_cases() -> eltdx::Result<Vec<Case>> {
    Ok(vec![
        case!(bond_profile, BondProfileQuery::new(bond()?)),
        case!(bond_finance, BondFinanceQuery::new(bond()?)),
        case!(bond_rating, BondRatingQuery::new(bond()?)),
        case!(bond_conversion, BondConversionQuery::new(bond()?)),
        case!(bond_rate, BondRateQuery::new(bond()?)),
        case!(bond_announcements, BondAnnouncementsQuery::new(bond()?)),
        case!(gz_profile, GzProfileQuery::new(bond()?)),
        case!(gz_interest, GzInterestQuery::new(bond()?)),
        case!(gz_market_data, GzMarketDataQuery::new(bond()?)),
        case!(gz_rating, GzRatingQuery::new(bond()?)),
        case!(gz_finance, GzFinanceQuery::new(bond()?)),
        case!(qyz_profile, QyzProfileQuery::new(bond()?)),
        case!(qyz_interest, QyzInterestQuery::new(bond()?)),
        case!(qyz_guarantee, QyzGuaranteeQuery::new(bond()?)),
        case!(qyz_rating, QyzRatingQuery::new(bond()?)),
        case!(qyz_rate, QyzRateQuery::new(bond()?)),
        case!(qyz_review, QyzReviewQuery::new(bond()?)),
        case!(gzhg_info, GzhgInfoQuery::new(bond()?)),
        case!(gzhg_quotes, GzhgQuotesQuery::new(bond()?)),
        case!(zczq_profile, ZczqProfileQuery::new(bond()?)),
        case!(zczq_finance, ZczqFinanceQuery::new(bond()?)),
        case!(zczq_rating, ZczqRatingQuery::new(bond()?)),
        case!(kjhz_profile, KjhzProfileQuery::new(bond()?)),
        case!(kjhz_finance, KjhzFinanceQuery::new(bond()?)),
        case!(kjhz_interest, KjhzInterestQuery::new(bond()?)),
        case!(kjhz_guarantee, KjhzGuaranteeQuery::new(bond()?)),
        case!(kjhz_rating, KjhzRatingQuery::new(bond()?)),
        case!(kjhz_exchange, KjhzExchangeQuery::new(bond()?)),
        case!(kjhz_rate, KjhzRateQuery::new(bond()?)),
        case!(bond_terms, BondTermsQuery::new(bond()?)),
        case!(bond_bulletin, BondBulletinQuery::new(bond()?)),
    ])
}
fn sector_cases() -> eltdx::Result<Vec<Case>> {
    Ok(vec![
        case!(hy_summary, HySummaryQuery::new(industry()?)),
        case!(hy_stage_rise, HyStageRiseQuery::new(industry()?)),
        case!(hy_profile, HyProfileQuery::new(industry()?)),
        case!(hy_commentary, HyCommentaryQuery::new(industry()?)),
        case!(hy_news, HyNewsQuery::new(industry()?)),
        case!(hy_members_rank, HyMembersRankQuery::new(industry()?)),
        case!(hy_capital_stats, HyCapitalStatsQuery::new(industry()?)),
        case!(hy_fund_holdings, HyFundHoldingsQuery::new(industry()?)),
        case!(hy_capital_flow, HyCapitalFlowQuery::new(industry()?)),
        case!(hy_analyst_index, HyAnalystIndexQuery::new(industry()?)),
        case!(hy_major_holders, HyMajorHoldersQuery::new(industry()?)),
        case!(hy_rating_summary, HyRatingSummaryQuery::new(industry()?)),
        case!(hy_index, HyIndexQuery::new(industry()?)),
        case!(qh_position_report, QhPositionReportQuery::new(future()?)),
        case!(qh_basis_spread, QhBasisSpreadQuery::new(future()?)),
        case!(
            qh_warehouse_receipt,
            QhWarehouseReceiptQuery::new(future()?)
        ),
        case!(qh_inventory, QhInventoryQuery::new(future()?)),
        case!(qh_variety_info, QhVarietyInfoQuery::new(future()?)),
        case!(qh_index_info, QhIndexInfoQuery::new(future()?)),
        case!(qh_index_valuation, QhIndexValuationQuery::default()),
        case!(qh_weights, QhWeightsQuery::default()),
    ])
}
fn report_cases() -> eltdx::Result<Vec<Case>> {
    Ok(vec![
        case!(stock_info, StockInfoQuery::new(ashare()?)),
        case!(business_periods, BusinessPeriodsQuery::new(ashare()?)),
        case!(topic_ids, TopicIdsQuery::new(ashare()?)),
        case!(company_profile, CompanyProfileQuery::new(ashare()?)),
        case!(shareholders, ShareholdersQuery::new(ashare()?)),
        case!(
            shareholder_change_plans,
            ShareholderChangePlansQuery::new(ashare()?)
        ),
        case!(capital_structure, CapitalStructureQuery::new(ashare()?)),
        case!(business_report, BusinessReportQuery::new(ashare()?)),
        case!(industry_analysis, IndustryAnalysisQuery::new(ashare()?)),
        case!(trading_report, TradingReportQuery::new(ashare()?)),
        case!(fund_report, FundReportQuery::new(ashare()?)),
        case!(market_report, MarketReportQuery::new(Some(ashare()?))),
        case!(dividend_financing, DividendFinancingQuery::new(ashare()?)),
        case!(allotment_dates, AllotmentDatesQuery::new(ashare()?)),
        case!(
            allotment_details,
            AllotmentDetailsQuery::new(ashare()?, TradingDate::new(2024, 6, 30)?)
        ),
        case!(finance_report, FinanceReportQuery::new(ashare()?)),
        case!(finance_diagnosis, FinanceDiagnosisQuery::new(ashare()?)),
        case!(stock_score, StockScoreQuery::new(ashare()?)),
        case!(profit_forecast, ProfitForecastQuery::new(ashare()?)),
        case!(ranking_detail, RankingDetailQuery::new(ashare()?)),
        case!(governance, GovernanceQuery::new(ashare()?)),
        case!(hot_topics, HotTopicsQuery::new(ashare()?)),
        case!(
            stock_finance_weights,
            StockFinanceWeightsQuery::new(ashare()?)
        ),
    ])
}
fn news_and_valuation_cases() -> eltdx::Result<Vec<Case>> {
    Ok(vec![
        case!(topic_compare, TopicCompareQuery::new(ashare()?, "001")?),
        case!(company_news, CompanyNewsQuery::new(ashare()?)),
        case!(digest, DigestQuery::new(ashare()?)),
        case!(northbound_holding, NorthboundHoldingQuery::new(ashare()?)),
        case!(detail, DetailQuery::new("news", "record-1")?),
        case!(cache_list, CacheListQuery::new(ashare()?)),
        case!(announcements, AnnouncementsQuery::new(ashare()?)),
        case!(news, NewsQuery::new(ashare()?)),
        case!(roadshows, RoadshowsQuery::new(ashare()?)),
        case!(market_news, MarketNewsQuery::new()),
        case!(
            channel_news,
            ChannelNewsQuery::new(NewsChannel::FinanceLatest)
        ),
        case!(theme_market, ThemeMarketQuery::new(ashare()?)),
        case!(stock_stage_gain, StockStageGainQuery::new(ashare()?)),
        case!(stock_main_funds, StockMainFundsQuery::new(ashare()?)),
        case!(stock_industry_rank, StockIndustryRankQuery::new(ashare()?)),
        case!(
            stock_industry_pct_rank,
            StockIndustryPctRankQuery::new(ashare()?)
        ),
        case!(
            stock_board_member_quotes,
            StockBoardMemberQuotesQuery::new(ashare()?)
        ),
        case!(
            stock_valuation_percentile,
            StockValuationPercentileQuery::new(ashare()?)
        ),
        case!(
            stock_pepb_percentile,
            StockPepbPercentileQuery::new(ashare()?)
        ),
        case!(stock_history_trend, StockHistoryTrendQuery::new(ashare()?)),
        case!(valuation, ValuationQuery::new(ashare()?)),
    ])
}

fn generic_cases() -> eltdx::Result<Vec<Case>> {
    let call = CallQuery::new(
        EntryName::new("fixture.call")?,
        RequestPayload::Body(JsonValue::parse(
            "{\"中文\":1,\"nested\":[true,null]}".as_bytes(),
        )?),
    );
    let params = ParamsQuery::new(
        EntryName::new("fixture.params")?,
        vec!["中文".into(), 7_i64.into()],
    );
    Ok(vec![
        Case {
            name: "call",
            expected: call.clone(),
            invoke: Box::new(move |client, options| client.call(&call, options)),
        },
        Case {
            name: "params",
            expected: params.clone().into_call(),
            invoke: Box::new(move |client, options| client.params(&params, options)),
        },
    ])
}

fn encoded(payload: &RequestPayload) -> eltdx::Result<Vec<u8>> {
    match payload {
        RequestPayload::Body(value) => value.to_json_bytes(),
        RequestPayload::Params(params) => JsonValue::Object(vec![(
            "Params".to_owned(),
            JsonValue::Array(params.clone()),
        )])
        .to_json_bytes(),
    }
}
fn run_cases(cases: Vec<Case>) -> TestResult {
    let expected = cases
        .iter()
        .enumerate()
        .map(|(index, case)| {
            Ok((
                case.expected.entry().as_str().to_owned(),
                encoded(case.expected.payload())?,
                response_bytes(index),
            ))
        })
        .collect::<eltdx::Result<Vec<_>>>()?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let url = format!("http://{}/TQLEX", listener.local_addr()?);
    let worker = thread::spawn(move || -> TestResult {
        for (entry, body, reply) in expected {
            let until = Instant::now() + Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < until =>
                    {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(error) => return Err(error.into()),
                }
            };
            stream.set_nonblocking(false)?;
            stream.set_read_timeout(Some(Duration::from_secs(2)))?;
            stream.set_write_timeout(Some(Duration::from_secs(2)))?;
            let mut headers = Vec::new();
            while !headers.ends_with(b"\r\n\r\n") {
                if headers.len() >= 64 * 1024 {
                    return Err("oversized request header".into());
                }
                let mut byte = [0];
                stream.read_exact(&mut byte)?;
                headers.push(byte[0]);
            }
            let headers = String::from_utf8(headers)?;
            assert!(headers.starts_with(&format!("POST /TQLEX?Entry={entry} HTTP/1.1\r\n")));
            let size: usize = headers
                .lines()
                .filter_map(|line| line.split_once(':'))
                .find(|(key, _)| key.eq_ignore_ascii_case("Content-Length"))
                .ok_or("missing body length")?
                .1
                .trim()
                .parse()?;
            assert_eq!(size, body.len());
            let mut actual = vec![0; size];
            stream.read_exact(&mut actual)?;
            assert_eq!(actual, body);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                reply.len()
            )?;
            stream.write_all(&reply)?;
        }
        Ok(())
    });
    let client = F10Client::builder()
        .with_base_url(url.clone())
        .with_international_url(url.clone())
        .with_ladder_url(url)
        .build()?;
    for (index, case) in cases.iter().enumerate() {
        let response = (case.invoke)(
            &client,
            &OperationOptions::default().with_raw(RawPolicy::RetainPayload),
        )?;
        assert_eq!(
            response.entry(),
            case.expected.entry().as_str(),
            "{}",
            case.name
        );
        assert_eq!(response.request(), case.expected.payload(), "{}", case.name);
        assert_eq!(response.ok(), index % 2 == 0, "{}", case.name);
        assert_eq!(response.result_sets().len(), 2, "{}", case.name);
        let first = &response.first_table().ok_or("missing first table")?.rows()[0];
        assert_eq!(first.get_all("duplicate").count(), 2, "{}", case.name);
        assert_eq!(
            first.cells()[0]
                .value()
                .as_number()
                .and_then(JsonNumber::as_u64),
            Some(index as u64)
        );
        assert_eq!(
            response.result_sets()[1].rows()[0].cells()[0]
                .value()
                .as_str(),
            Some("新闻")
        );
        assert_eq!(
            response
                .meta()
                .raw()
                .ok_or("missing response payload")?
                .payloads()[0]
                .as_ref(),
            response_bytes(index)
        );
    }
    assert_eq!(
        client.diagnostics()?.completed_requests(),
        cases.len() as u64
    );
    client.close()?;
    for case in &cases {
        assert_eq!(
            (case.invoke)(&client, &OperationOptions::default())
                .unwrap_err()
                .kind(),
            eltdx::ErrorKind::Closed,
            "{}",
            case.name
        );
    }
    worker
        .join()
        .map_err(|_| "named-method fixture panicked")??;
    Ok(())
}
fn response_bytes(index: usize) -> Vec<u8> {
    format!(r#"{{"ErrorCode":{},"HitCache":false,"ResultSets":[{{"ColName":["marker","duplicate","duplicate","unknown"],"Content":[[{index},1,2,{{"future":true}}]]}},{{"ColName":["news"],"Content":[["新闻"]]}}]}}"#,
        if index % 2 == 0 { 0 } else { 17 }).into_bytes()
}

#[test]
fn every_international_named_method_executes_its_query_and_retains_response() -> TestResult {
    let cases = international_cases()?;
    assert_eq!(cases.len(), 50);
    run_cases(cases)
}
#[test]
fn every_bond_named_method_executes_its_query_and_retains_response() -> TestResult {
    let cases = bond_cases()?;
    assert_eq!(cases.len(), 31);
    run_cases(cases)
}
#[test]
fn every_sector_named_method_executes_its_query_and_retains_response() -> TestResult {
    let cases = sector_cases()?;
    assert_eq!(cases.len(), 21);
    run_cases(cases)
}
#[test]
fn every_report_named_single_request_method_executes_its_query() -> TestResult {
    let cases = report_cases()?;
    assert_eq!(cases.len(), 23);
    run_cases(cases)
}
#[test]
fn every_news_and_valuation_named_single_request_method_executes_its_query() -> TestResult {
    let cases = news_and_valuation_cases()?;
    assert_eq!(cases.len(), 21);
    run_cases(cases)
}
#[test]
fn generic_call_and_params_execute_their_distinct_body_shapes() -> TestResult {
    run_cases(generic_cases()?)
}
