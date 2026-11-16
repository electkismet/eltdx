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
