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
