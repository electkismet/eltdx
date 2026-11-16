//! Independent loopback result fixtures; intentionally not run during source implementation.
#![cfg(feature = "f10")]
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use std::thread;
use std::time::{Duration, Instant};
use eltdx::f10::*;
use eltdx::{OperationOptions, RawPolicy, TradingDate};

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;
type Fixture = (String, Vec<u8>, Vec<u8>);
fn fixture(calls: Vec<Fixture>) -> Result<(String, thread::JoinHandle<TestResult>), Box<dyn std::error::Error + Send + Sync>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let url = format!("http://{}/TQLEX", listener.local_addr()?);
    let worker = thread::spawn(move || -> TestResult {
        for (entry, expected, response) in calls {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => thread::sleep(Duration::from_millis(2)),
                    Err(error) => return Err(error.into()),
                }
            };
            stream.set_read_timeout(Some(Duration::from_secs(2)))?;
            stream.set_write_timeout(Some(Duration::from_secs(2)))?;
            let mut headers = Vec::new();
            while !headers.ends_with(b"\r\n\r\n") {
                if headers.len() >= 64 * 1024 { return Err("oversized request header".into()); }
                let mut byte = [0]; stream.read_exact(&mut byte)?; headers.push(byte[0]);
            }
            let headers = String::from_utf8(headers)?;
            assert!(headers.starts_with(&format!("POST /TQLEX?Entry={entry} HTTP/1.1\r\n")));
            let length: usize = headers.lines().filter_map(|line| line.split_once(':'))
                .find(|(name, _)| name.eq_ignore_ascii_case("Content-Length")).ok_or("missing content length")?.1.trim().parse()?;
            if length > 1024 * 1024 { return Err("oversized fixture request".into()); }
            let mut body = vec![0; length]; stream.read_exact(&mut body)?;
            assert_eq!(body, expected);
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", response.len())?;
            stream.write_all(&response)?;
        }
        Ok(())
    });
    Ok((url, worker))
}

#[test]
fn ladder_keeps_raw_unknowns_and_counts_only_positive_up_rows_per_day() -> TestResult {
    let detail = br#"{"ErrorCode":0,"HitCache":true,"ResultSets":[{"ColName":["rqex","lbts","ZQDM","SC","ztlb","fde","kbcs","opaque"],"Content":[["20240628",1,"600000","1",1,"20",0,{"x":18446744073709551616}],["20240628",1,"600001","1",1,30,0,null],["20240628",2,"600002","1",1,40,0,null],["20240628",9,"600003","1",3,50,0,null],["20240628",0,"600004","1",0,60,0,null],["20240701",1,"600005","1",1,70,0,null],["20240701",2,"600006","1",1,80,0,null],["20240701",2,"600007","1",1,90,0,null],["bad",3,"600008","1",1,"bad","bad",null],["20240701",4,"600009","1",99,100,0,null]]}]}"#;
    let summary = br#"{"ErrorCode":7,"ResultSets":[{"ColName":["N001","N002","N007","N009","N010"],"Content":[["20240628",10,200,"topic",2]]}]}"#;
    let (url, worker) = fixture(vec![
        ("CWServ.cfg_fx_lbtt".to_owned(), br#"{"Params":["1","20240628","20240701"]}"#.to_vec(), detail.to_vec()),
        ("CWServ.cfg_fx_lbtt".to_owned(), br#"{"Params":["2","20240628","20240701"]}"#.to_vec(), summary.to_vec()),
    ])?;
    let client = F10Client::builder().with_ladder_url(url).build()?;
    let query = LimitUpDownListQuery::new().with_dates(Some(TradingDate::new(2024, 6, 28)?), Some(TradingDate::new(2024, 7, 1)?))?.with_include_summary(true);
    let result = client.limit_up_down_list(&query, &OperationOptions::default().with_raw(RawPolicy::RetainPayload))?;
    assert!(result.ok());
    assert_eq!(result.summary_ok(), Some(false));
    assert_eq!(result.summary_error_code(), Some(7));
    assert_eq!(result.rows().len(), 10);
    assert_eq!(result.rows()[3].status(), LimitBoardType::Broken);
    assert_eq!(result.rows()[3].board_level(), None);
    assert_eq!(result.rows()[8].trade_date(), None);
    assert_eq!(result.rows()[8].seal_amount(), None);
    assert_eq!(result.rows()[9].status(), LimitBoardType::Unknown(Some(99)));
    assert_eq!(result.ladder_counts(), None);
    assert_eq!(result.ladder_counts_by_date()[0].counts(), &[(2, 1), (1, 2)]);
    assert_eq!(result.ladder_counts_by_date()[0].promotion_rates(), vec![(2, 0.5)]);
    assert_eq!(result.ladder_counts_by_date()[1].counts(), &[(2, 2), (1, 1)]);
    assert!(result.ladder_counts_by_date()[1].promotion_rates().is_empty());
    let unknown = result.rows()[0].raw().get_all("opaque").next().ok_or("missing unknown field")?.value();
    assert_eq!(unknown.get("x").and_then(JsonValue::as_number).map(JsonNumber::lexeme), Some("18446744073709551616"));
    assert_eq!(result.meta().sources().len(), 2);
    assert_eq!(result.meta().raw().ok_or("missing raw")?.payloads()[0].as_ref(), detail);
    assert_eq!(result.meta().raw().ok_or("missing raw")?.payloads()[1].as_ref(), summary);
    assert!(result.meta().diagnostics().iter().any(|value| value.message().is_some_and(|message| message.contains("promotion_rate"))));
    assert_eq!(result.market_overview()[0].today_amount(), Some(200.0));
    client.close()?;
    worker.join().map_err(|_| "fixture panicked")??;
    Ok(())
}

struct FixedDate { calls: Arc<AtomicUsize> }
impl LocalDateClock for FixedDate {
    fn today(&self) -> eltdx::Result<TradingDate> { self.calls.fetch_add(1, Ordering::SeqCst); TradingDate::new(2024, 7, 1) }
}
#[test]
fn omitted_ladder_date_uses_one_lazy_injected_local_clock_and_alias() -> TestResult {
    let calls = Arc::new(AtomicUsize::new(0));
    let (url, worker) = fixture(vec![("CWServ.cfg_fx_lbtt".to_owned(), br#"{"Params":["1","20240701","20240701"]}"#.to_vec(), br#"{"ResultSets":[]}"#.to_vec())])?;
    let client = F10Client::builder().with_ladder_url(url).with_local_date_clock(Arc::new(FixedDate { calls: calls.clone() })).build()?;
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let result = client.limit_board_ladder(&LimitBoardLadderQuery::new(), &OperationOptions::default())?;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(result.date_source(), LadderDateSource::LocalClock);
    assert_eq!(result.trade_date(), Some(TradingDate::new(2024, 7, 1)?));
    assert_eq!(result.ladder_counts(), Some(&[][..]));
    client.close()?;
    worker.join().map_err(|_| "fixture panicked")??;
    Ok(())
}

#[test]
fn dynamic_columns_news_position_and_business_failure_preserve_source_structure() -> TestResult {
    let value = JsonValue::parse(br#"{"ErrorCode":17,"ResultSets":[{"ResultSetKey":"same","ColName":["x","x",""],"Content":[[1,2,null,4]]},{"ResultSetKey":"same","ColName":["news"],"Content":[["second"]]}]}"#)?;
    let response = parse_response(&EntryName::new("fixture")?, &RequestPayload::Params(Vec::new()), value, RawPolicy::RetainPayload)?;
    assert!(!response.ok());
    assert_eq!(response.result_sets().len(), 2);
    let cells = response.first_table().ok_or("missing first table")?.rows()[0].cells();
    assert_eq!(cells[0].name(), "x"); assert_eq!(cells[1].name(), "x");
    assert_eq!(cells[2].name(), "col_2"); assert_eq!(cells[3].name(), "col_3");
    let news = eltdx::f10::news::extract_rows(&response);
    assert_eq!(news.len(), 1);
    assert_eq!(news[0].cells()[0].value().as_str(), Some("second"));
    Ok(())
}

#[test]
fn field_labels_require_the_correct_entry_report_and_industry_template() -> TestResult {
    use eltdx::f10::fields::*;
    let entry = EntryName::new("CWServ.tdxf10_gg_cwfx")?;
    for (report, template, field, expected) in [
        ("zcfzb", 0, "T039", "资产总计"),
        ("zcfzb", 1, "T039", "固定资产"),
        ("zcfzb", 2, "T048", "资产总计"),
        ("zcfzb", 3, "T119", "使用权资产"),
        ("lyb", 0, "T008", "营业收入"),
        ("lyb", 1, "T009", "利息净收入"),
        ("lyb", 2, "T015", "代理买卖证券业务净收入"),
        ("lyb", 3, "T030", "赔付支出"),
        ("xjllb", 0, "T008", "销售商品、提供劳务收到的现金"),
        ("xjllb", 1, "T009", "同业及其他金融机构存放款项净增加额"),
        ("xjllb", 2, "T088", "存货的减少"),
        ("xjllb", 3, "T088", "交易性金融资产的减少"),
    ] {
        let context = FieldContext::from_request(&entry, &RequestPayload::Params(vec!["600000".into(), report.into(), "".into()]),
            FieldMetadata::new().with_industry_template(Some(template)))?;
        assert_eq!(describe_field(&context, field).label(), Some(expected));
        assert_eq!(describe_field(&context, field).unit(), FieldUnit::Unknown);
    }
    let body = RequestPayload::Params(vec!["600000".into(), "zcfzb".into(), "".into()]);
    let missing = FieldContext::from_request(&entry, &body, FieldMetadata::new())?;
    assert!(field_labels_for(&missing)?.is_empty());
    let unknown = FieldContext::from_request(&entry, &body, FieldMetadata::new().with_industry_template(Some(99)))?;
    assert!(field_labels_for(&unknown)?.is_empty());
    let conflict = FieldContext::from_request(&entry, &body, FieldMetadata::new().with_industry_template(Some(0)).with_report_type(Some("lyb".to_owned()))?)?;
    assert!(field_labels_for(&conflict)?.is_empty());
    let research = FieldContext::from_request(&EntryName::new("CWServ.tdxf10_gg_gszx")?,
        &RequestPayload::Params(vec!["600000".into(), "gsyj".into()]), FieldMetadata::new())?;
    assert_eq!(describe_field(&research, "T039").label(), Some("研报标题"));
    Ok(())
}

#[test]
fn bank_cashflow_dynamic_field_has_only_a_note_and_duplicate_projection_is_explicit() -> TestResult {
    use eltdx::f10::fields::*;
    let context = FieldContext::new(EntryName::new("CWServ.tdxf10_gg_cwfx")?, Some("xjllb".to_owned()),
        FieldMetadata::new().with_industry_template(Some(1)))?;
    let field = describe_field(&context, "T088");
    assert_eq!(field.label(), None);
    assert_eq!(field.evidence(), FieldEvidence::Unknown);
    assert_eq!(field.note(), Some("银行历史报表的动态扩展调节项，含义随证券及报告期变化；不能使用统一字段名。"));
    assert_eq!(describe_field(&context, "T009__2").label(), None);
    assert_eq!(describe_projected_field(&context, "T009__02").label(), Some("同业及其他金融机构存放款项净增加额（2）"));
    assert_eq!(describe_projected_field(&context, "T009__1").label(), None);
    assert_eq!(describe_field_notes(&context, &["T088__2", "T999"])?[0].0, "T088__2");
    let descriptions = describe_fields(&context, &["T009", "T009", "T999"])?;
    assert_eq!(descriptions.len(), 3);
    assert_eq!(descriptions[0], descriptions[1]);
    assert_eq!(descriptions[2].label(), None);
    Ok(())
}

#[test]
fn international_field_scope_indices_wildcards_and_statement_overlays_are_preserved() -> TestResult {
    use eltdx::f10::fields::*;
    for (entry, params, field, label) in [
        ("CWServ.skef10_mg_fhrz", vec!["NVDA", "fh"], "T005", "每股股利"),
        ("CWServ.skef10_mg_dsgg", vec!["NVDA", "ds"], "T002", "姓名"),
        ("CWServ.skef10_mg_jyfx", vec!["future-subscope", "NVDA"], "T005", "业务名称"),
        ("CWServ.skef10_mg_cwfx", vec!["zcfzb", "NVDA"], "N029", "资产总计"),
        ("CWServ.skef10_mg_cwfx", vec!["lyb", "NVDA"], "N003", "营业总收入"),
        ("CWServ.skef10_mg_cwfx", vec!["xjllb", "NVDA"], "N031", "经营活动现金流净额"),
        ("CWServ.skef10_hk_cwfx", vec!["2", "00700"], "TAssets", "总资产"),
        ("CWServ.skef10_hk_cwzb", vec!["5", "00700"], "EPS1", "每股收益"),
        ("CWServ.skef10_hk_schq", vec!["2", "00700"], "ahzj", "A/H溢价(%)"),
        ("CWServ.skef10_hk_sdfx_gzfx", vec!["any", "00700"], "N002", "市盈率"),
    ] {
        let context = FieldContext::from_request(&EntryName::new(entry)?,
            &RequestPayload::Params(params.into_iter().map(JsonValue::from).collect()), FieldMetadata::new())?;
        assert_eq!(describe_field(&context, field).label(), Some(label));
    }
    let context = FieldContext::new(EntryName::new("CWServ.skef10_mg_cwfx")?, Some("zcfzb".to_owned()), FieldMetadata::new())?;
    let labels = field_labels_for(&context)?;
    assert_eq!(labels[0], ("year", "年度"));
    assert_eq!(labels[4], ("N003", "现金及现金等价物"));
    let unknown = FieldContext::new(EntryName::new("CWServ.skef10_hk_cwfx")?, Some("unsupported".to_owned()), FieldMetadata::new())?;
    assert!(field_labels_for(&unknown)?.is_empty());
    Ok(())
}

#[test]
fn money_flow_history_comp050_reverses_then_slices_without_sorting() -> TestResult {
    for (dates, expected) in [
        (["2026-11-12", "2026-11-13", "2026-11-16"], TradingDate::new(2026, 11, 13)?),
        (["2026-11-16", "2026-11-12", "2026-11-13"], TradingDate::new(2026, 11, 12)?),
    ] {
        let reply = format!(r#"{{"ErrorCode":0,"ResultSets":[{{"ColName":["N001","N002","N099"],"Content":[["{}","600000",{{"unknown":1}}],["{}","600000",{{"unknown":2}}],["{}","600000",{{"unknown":3}}]]}}]}}"#,
            dates[0], dates[1], dates[2]).into_bytes();
        let (url, worker) = fixture(vec![("CWServ.tdxf10_gg_sj".to_owned(),
            br#"{"Params":["zjlx_sj","600000","5","600000"]}"#.to_vec(), reply.clone())])?;
        let client = F10Client::builder().with_base_url(url).build()?;
        let query = MoneyFlowHistoryQuery::new(AshareCode::new(F10MarketCode::new(1), "600000")?)
            .with_days(5)?.with_start(1).with_count(std::num::NonZeroU32::new(1))?;
        let result = client.money_flow_history().history(&query, &OperationOptions::default().with_raw(RawPolicy::RetainPayload))?;
        assert_eq!(result.source_row_count(), 3);
        assert_eq!(result.rows().len(), 1);
        assert_eq!(result.rows()[0].trade_date(), Some(expected));
        assert_eq!(result.rows()[0].raw().get_all("N099").next().ok_or("missing unknown")?.value()
            .get("unknown").and_then(JsonValue::as_number).and_then(JsonNumber::as_i64), Some(2));
        assert_eq!(result.meta().raw().ok_or("missing payload")?.payloads()[0].as_ref(), reply);
        assert_eq!(client.diagnostics()?.completed_requests(), 1);
        client.close()?;
        worker.join().map_err(|_| "fixture panicked")??;
    }
    Ok(())
}

#[test]
fn money_flow_history_keeps_business_status_unknown_units_and_additional_tables() -> TestResult {
    let reply = br#"{"ErrorCode":17,"HitCache":false,"ResultSets":[{"ColName":["N001","N002","N003","N004","N005","N006","N007","N009","N099","N099"],"Content":[["20260230","600000","source name","bad",20.50,-12,"NaN","2024/02/29",9007199254740993,{"future":true}]]},{"ResultSetKey":"extra","ColName":["data"],"Content":[[42]]}]}"#;
    let (url, worker) = fixture(vec![("CWServ.tdxf10_gg_sj".to_owned(),
        br#"{"Params":["zjlx_sj","600000","60","600000"]}"#.to_vec(), reply.to_vec())])?;
    let client = F10Client::builder().with_base_url(url).build()?;
    let result = client.money_flow_history().range(&MoneyFlowHistoryQuery::new(
        AshareCode::new(F10MarketCode::new(1), "600000")?), &OperationOptions::default().with_raw(RawPolicy::RetainPayload))?;
    assert!(!result.ok());
    assert_eq!(result.error_code(), Some(17));
    let row = &result.rows()[0];
    assert_eq!(row.trade_date(), None);
    assert_eq!(row.listing_date(), Some(TradingDate::new(2024, 2, 29)?));
    assert_eq!(row.name(), Some("source name"));
    assert_eq!(row.main_net_amount(), None);
    assert_eq!(row.extra_large_net_amount(), Some(20.5));
    assert_eq!(row.large_net_amount(), Some(-12.0));
    assert_eq!(row.active_buy_net_amount(), None);
    assert_eq!(row.amount_unit(), eltdx::AmountUnit::Unknown);
    assert_eq!(row.raw().get_all("N099").count(), 2);
    assert_eq!(row.raw().get_all("N099").next().ok_or("missing number")?.value()
        .as_number().map(JsonNumber::lexeme), Some("9007199254740993"));
    assert_eq!(result.response().result_sets().len(), 2);
    assert!(result.response().first_table().ok_or("missing table")?.rows().is_empty());
    assert_eq!(result.response().result_sets()[1].rows().len(), 1);
    assert_eq!(result.meta().raw().ok_or("missing payload")?.payloads()[0].as_ref(), reply);
    for field in ["N001", "N004", "N007"] {
        assert!(result.meta().diagnostics().iter().any(|diagnostic| diagnostic.message().is_some_and(|message| message.contains(field))));
    }
    client.close()?;
    worker.join().map_err(|_| "fixture panicked")??;
    Ok(())
}

#[test]
fn money_flow_history_empty_response_and_outside_window_make_one_request_each() -> TestResult {
    for (start, reply) in [(0, &br#"{"ResultSets":[]}"#[..]),
        (usize::MAX, &br#"{"ResultSets":[{"ColName":["N001"],"Content":[["20261116"]]}]}"#[..])] {
        let (url, worker) = fixture(vec![("CWServ.tdxf10_gg_sj".to_owned(),
            br#"{"Params":["zjlx_sj","600000","60","600000"]}"#.to_vec(), reply.to_vec())])?;
        let client = F10Client::builder().with_base_url(url).build()?;
        let result = client.money_flow_history().history(&MoneyFlowHistoryQuery::new(
            AshareCode::new(F10MarketCode::new(1), "600000")?).with_start(start), &OperationOptions::default())?;
        assert!(result.rows().is_empty());
        assert!(result.meta().raw().is_none());
        assert_eq!(client.diagnostics()?.completed_requests(), 1);
        client.close()?;
        worker.join().map_err(|_| "fixture panicked")??;
    }
    Ok(())
}

#[test]
fn invalid_history_days_do_not_connect_and_owned_service_observes_shared_close() -> TestResult {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let client = F10Client::builder().with_base_url(format!("http://{}/TQLEX", listener.local_addr()?)).build()?;
    let service = client.money_flow_history();
    let code = AshareCode::new(F10MarketCode::new(1), "600000")?;
    assert_eq!(MoneyFlowHistoryQuery::new(code.clone()).with_days(7).unwrap_err().kind(), eltdx::ErrorKind::InvalidArgument);
    assert_eq!(client.diagnostics()?.completed_requests(), 0);
    assert_eq!(listener.accept().unwrap_err().kind(), std::io::ErrorKind::WouldBlock);
    client.close()?;
    assert_eq!(service.history(&MoneyFlowHistoryQuery::new(code), &OperationOptions::default()).unwrap_err().kind(), eltdx::ErrorKind::Closed);
    assert_eq!(listener.accept().unwrap_err().kind(), std::io::ErrorKind::WouldBlock);
    Ok(())
}

#[test]
fn topic_compare_first_skips_unusable_ids_and_preserves_both_sources() -> TestResult {
    let first = br#"{"ResultSets":[{"ColName":["t001"],"Content":[[null],[""],["   "],[true],["001"]]}]}"#;
    let second = br#"{"ErrorCode":0,"ResultSets":[{"Content":[[42]]}]}"#;
    let (url, worker) = fixture(vec![
        ("CWServ.tdxf10_gg_comreq".to_owned(), br#"{"Params":["rdtcgn","600000"]}"#.to_vec(), first.to_vec()),
        ("CWServ.tdxf10_gg_rdtc_gndb".to_owned(), br#"{"Params":["gndbzfsj","600000","001","zdf"]}"#.to_vec(), second.to_vec()),
    ])?;
    let client = F10Client::builder().with_base_url(url).build()?;
    let result = client.topic_compare_first(&TopicCompareFirstQuery::new(
        AshareCode::new(F10MarketCode::new(1), "600000")?), &OperationOptions::default().with_raw(RawPolicy::RetainPayload))?;
    assert_eq!(result.meta().sources().len(), 2);
    let raw = result.meta().raw().ok_or("missing topic raw")?;
    assert_eq!(raw.payloads()[0].as_ref(), first);
    assert_eq!(raw.payloads()[1].as_ref(), second);
    assert_eq!(client.diagnostics()?.completed_requests(), 2);
    client.close()?;
    worker.join().map_err(|_| "fixture panicked")??;
    Ok(())
}

#[test]
fn topic_compare_first_stops_on_absent_or_failed_prerequisite() -> TestResult {
    for first in [
        &br#"{"ErrorCode":17,"ResultSets":[{"ColName":["t001"],"Content":[["001"]]}]}"#[..],
        &br#"{"ErrorCode":0,"ResultSets":[{"ColName":["t001"],"Content":[[null],[false],[""]]}]}"#[..],
    ] {
        let (url, worker) = fixture(vec![("CWServ.tdxf10_gg_comreq".to_owned(),
            br#"{"Params":["rdtcgn","600000"]}"#.to_vec(), first.to_vec())])?;
        let client = F10Client::builder().with_base_url(url).build()?;
        let result = client.topic_compare_first(&TopicCompareFirstQuery::new(
            AshareCode::new(F10MarketCode::new(1), "600000")?), &OperationOptions::default());
        assert_eq!(result.unwrap_err().kind(), eltdx::ErrorKind::MetadataUnavailable);
        assert_eq!(client.diagnostics()?.completed_requests(), 1);
        client.close()?;
        worker.join().map_err(|_| "fixture panicked")??;
    }
    Ok(())
}
