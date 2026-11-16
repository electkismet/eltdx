//! Independent HTTP consumer. Synthetic requests/responses, std plus eltdx.
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use eltdx::f10::{AshareCode, EntryName, F10Client, F10MarketCode, IndexMembersQuery,
    JsonValue, ParamsQuery, StockInfoQuery};
use eltdx::{ErrorKind, OperationOptions, RawPolicy, StockTopicsQuery, TdxClient, TopicStocksQuery};

struct Case { entry: &'static str, body: &'static [u8], reply: &'static str }
struct HttpFixture { url: String, stop: Arc<AtomicBool>, worker: Option<JoinHandle<io::Result<usize>>> }
impl HttpFixture {
    fn start(cases: Vec<Case>) -> io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let url = format!("http://{}/TQLEX", listener.local_addr()?);
        let stop = Arc::new(AtomicBool::new(false)); let stopping = stop.clone();
        let worker = thread::spawn(move || {
            let mut observed = 0;
            for case in cases {
                let mut stream = loop {
                    if stopping.load(Ordering::Acquire) { return Err(io::Error::other("consumer ended before expected HTTP request")); }
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => thread::park_timeout(Duration::from_millis(1)),
                        Err(error) => return Err(error),
                    }
                };
                let (line, body) = receive(&mut stream)?;
                super::require(line.starts_with("POST /TQLEX?") && line.contains(case.entry), "F10 method or Entry routing changed")?;
                super::require(body == case.body, "F10 Params or field types changed")?;
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    case.reply.len(), case.reply)?;
                observed += 1;
            }
            Ok(observed)
        });
        Ok(Self { url, stop, worker: Some(worker) })
    }
    fn finish(mut self) -> io::Result<usize> {
        self.stop.store(true, Ordering::Release);
        self.worker.take().ok_or_else(|| io::Error::other("HTTP fixture already joined"))?.join()
            .map_err(|_| io::Error::other("HTTP fixture panicked"))?
    }
}
impl Drop for HttpFixture { fn drop(&mut self) { self.stop.store(true, Ordering::Release); } }

fn receive(stream: &mut TcpStream) -> io::Result<(String, Vec<u8>)> {
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        let mut byte = [0]; stream.read_exact(&mut byte)?; header.push(byte[0]);
        if header.len() > 64 * 1024 { return Err(io::Error::other("HTTP fixture header exceeded its bound")); }
    }
    let text = std::str::from_utf8(&header).map_err(io::Error::other)?;
    let size = text.lines().filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("Content-Length"))
        .ok_or_else(|| io::Error::other("missing HTTP Content-Length"))?.1.trim()
        .parse::<usize>().map_err(io::Error::other)?;
    if size > 64 * 1024 { return Err(io::Error::other("HTTP fixture body exceeded its bound")); }
    let mut body = vec![0; size]; stream.read_exact(&mut body)?;
    Ok((text.lines().next().unwrap_or("").to_owned(), body))
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    // Independent literal expectations: no SDK encoder/parser constructs these.
    let fixture = HttpFixture::start(vec![
        Case { entry: "consumer.business", body: br#"{"Params":["literal",7]}"#,
            reply: r#"{"ErrorCode":17,"HitCache":true,"ResultSets":[{"ColName":["x","x"],"Content":[[1,2]]}]}"# },
        Case { entry: "CWServ.tdxf10_gg_comreq", body: br#"{"Params":["gpquery","600000"]}"#,
            reply: r#"{"ErrorCode":0,"ResultSets":[{"ColName":["name"],"Content":[["stock"]]}]}"# },
        Case { entry: "CWServ.skef10_zs_cfgsj_cfgjcsj", body: br#"{"Params":["001","600000","1","100"]}"#,
            reply: r#"{"ErrorCode":0,"ResultSets":[{"ColName":["N001","N002","N004"],"Content":[["600000","member",1]]}]}"# },
        Case { entry: "CWServ.tdxf10_gg_comreq", body: br#"{"Params":["rdtcgn","600000"]}"#,
            reply: r#"{"ErrorCode":0,"ResultSets":[{"ColName":["t001","t002"],"Content":[["42","topic"]]}]}"# },
        Case { entry: "CWServ.tdxf10_gg_rdtc", body: br#"{"Params":["600000","zttzbkz"]}"#,
            reply: r#"{"ErrorCode":0,"ResultSets":[{"ColName":["id","ztmc","gld"],"Content":[["42","updated",3]]}]}"# },
        Case { entry: "CWServ.tdxf10_gg_rdtc_gndb", body: br#"{"Params":["gndbzfsj","600000","42","zdf"]}"#,
            reply: r#"{"ErrorCode":0,"ResultSets":[{"ColName":["pm","zqdm","sc"],"Content":[[2,"600001",1],[1,"600000",1]]}]}"# },
    ])?;
    let hq = TcpListener::bind("127.0.0.1:0")?; hq.set_nonblocking(true)?;
    let client = TdxClient::builder().endpoints(vec![hq.local_addr()?])
        .f10(F10Client::builder().with_base_url(fixture.url.clone()).with_timeout(Duration::from_secs(3))).build()?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let options = OperationOptions::default().with_raw(RawPolicy::RetainPayload);
        let f10 = client.f10();
        let generic = f10.params(&ParamsQuery::new(EntryName::new("consumer.business")?,
            vec![JsonValue::from("literal"), JsonValue::from(7_i64)]), &options)?;
        super::require(!generic.ok() && generic.error_code() == Some(17), "F10 business status was lost")?;
        let table = generic.first_table().ok_or("missing generic table")?;
        super::require(table.rows()[0].get_all("x").count() == 2, "duplicate F10 columns were collapsed")?;
        super::require(generic.meta().raw().is_some(), "F10 raw HTTP payload missing")?;
        super::require(generic.attributes().iter().any(|(name, value)| name == "HitCache" && value == &JsonValue::Bool(true)),
            "unknown top-level F10 attributes missing")?;
        let code = AshareCode::new(F10MarketCode::new(1), "600000")?;
        let stock = f10.stock_info(&StockInfoQuery::new(code.clone()), &options)?;
        super::require(stock.first_table().ok_or("missing stock table")?.rows().len() == 1, "named F10 result empty")?;
        let helper = client.helpers();
        let members = helper.index_members(&IndexMembersQuery::new(code.clone()).with_all_pages(false), &options)?;
        super::require(members.codes() == ["600000"], "typed index helper result changed")?;
        let topics = helper.stock_topics(&StockTopicsQuery::new(code.clone()), &options)?;
        super::require(topics.count() == 1 && topics.responses().len() == 2, "topic source composition changed")?;
        super::require(topics.topics()[0].topic_name() == Some("updated") && topics.topics()[0].relation_level() == Some(3.0),
            "hot topic overlay changed")?;
        let members = helper.topic_stocks(&TopicStocksQuery::new(code, "42")?, &options)?;
        super::require(members.rows().iter().map(|row| row.rank()).collect::<Vec<_>>() == [Some(2), Some(1)],
            "topic comparison source order changed")?;
        super::require(f10.diagnostics()?.completed_requests() == 6, "unexpected HTTP work count")?;
        super::require(hq.accept().is_err_and(|error| error.kind() == io::ErrorKind::WouldBlock), "F10-only helpers started HQ")?;
        f10.close()?;
        super::require(helper.stock_topics(&StockTopicsQuery::new(AshareCode::new(F10MarketCode::new(1), "600000")?), &options)
            .is_err_and(|error| error.kind() == ErrorKind::Closed), "F10 close did not close shared helper owner")?;
        super::require(client.connect(&options).is_err_and(|error| error.kind() == ErrorKind::Closed), "F10 close did not close HQ owner")?;
        Ok(())
    })();
    let cleanup = client.close(); let fixture_result = fixture.finish();
    result?; cleanup?;
    super::require(fixture_result? == 6, "HTTP consumer did not issue all six requests")?;
    Ok(())
}
