//! Public cross-source helpers. Authored cases, deferred until the source gate.
#![cfg(feature = "f10")]
use eltdx::f10::{
    AshareCode, BoardMembersQuery, F10Client, F10MarketCode, IndexMembersQuery,
    IndexTrackingProductsQuery,
};
use eltdx::{CancellationToken, ErrorKind, OperationOptions, TdxClient};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

fn receive(stream: &mut TcpStream) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut bytes = Vec::new();
    let header_end = loop {
        let mut byte = [0];
        stream.read_exact(&mut byte)?;
        bytes.push(byte[0]);
        if bytes.ends_with(b"\r\n\r\n") {
            break bytes.len();
        }
        if bytes.len() > 64 * 1024 {
            return Err("oversized fixture header".into());
        }
    };
    let headers = std::str::from_utf8(&bytes)?;
    let size: usize = headers
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("Content-Length"))
        .ok_or("fixture needs content length")?
        .1
        .trim()
        .parse()?;
    if size > 64 * 1024 {
        return Err("oversized fixture body".into());
    }
    bytes.resize(header_end + size, 0);
    stream.read_exact(&mut bytes[header_end..])?;
    Ok(bytes)
}

fn server(
    replies: Vec<&'static str>,
) -> Result<(SocketAddr, thread::JoinHandle<TestResult>), Box<dyn std::error::Error + Send + Sync>>
{
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    listener.set_nonblocking(true)?;
    let worker = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(8);
        for reply in replies {
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::yield_now()
                    }
                    Err(error) => return Err(error.into()),
                }
            };
            stream.set_nonblocking(false)?;
            stream.set_read_timeout(Some(Duration::from_secs(3)))?;
            stream.set_write_timeout(Some(Duration::from_secs(3)))?;
            let request = receive(&mut stream)?;
            assert!(request.starts_with(b"POST "));
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                reply.len(),
                reply
            )?;
        }
        Ok(())
    });
    Ok((address, worker))
}

fn code() -> eltdx::Result<AshareCode> {
    AshareCode::new(F10MarketCode::new(1), "000300")
}
fn routed(address: SocketAddr) -> eltdx::f10::F10ClientBuilder {
    F10Client::builder()
        .with_base_url(format!("http://{address}/"))
        .with_timeout(Duration::from_secs(2))
}

#[test]
fn f10_helpers_return_nonempty_typed_results_without_connecting_hq() -> TestResult {
    let (address, worker) = server(vec![
        r#"{"ErrorCode":0,"ResultSets":[{"ColName":["N001","N002","N004"],"Content":[["600000","member",1]]}]}"#,
        r#"{"ErrorCode":0,"ResultSets":[{"ColName":["N001","N002","sc"],"Content":[["510300","ETF",1]]}]}"#,
        r#"{"ErrorCode":0,"ResultSets":[{"ColName":["N001","N002","N003","sc"],"Content":[["600000","member","dividend",1]]}]}"#,
        r#"{"ErrorCode":0,"ResultSets":[{"ColName":["N001","N002","N003","sc"],"Content":[["20240630","600000","IPO",1]]}]}"#,
    ])?;
    let hq = TcpListener::bind("127.0.0.1:0")?;
    hq.set_nonblocking(true)?;
    let client = TdxClient::builder()
        .endpoints(vec![hq.local_addr()?])
        .f10(routed(address))
        .build()?;
    let helper = client.helpers();
    let options = OperationOptions::default();
    assert_eq!(
        helper
            .index_members(&IndexMembersQuery::new(code()?), &options)?
            .rows()[0]
            .name(),
        Some("member")
    );
    assert_eq!(
        helper
            .index_etfs(&IndexTrackingProductsQuery::new(code()?), &options)?
            .etfs()
            .len(),
        1
    );
    assert_eq!(
        helper
            .board_members(&BoardMembersQuery::new(code()?), &options)?
            .codes(),
        vec!["600000"]
    );
    assert_eq!(helper.ipo_calendar(&options)?.count(), 1);
    assert_eq!(client.f10().diagnostics()?.completed_requests(), 4);
    assert_eq!(
        hq.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    client.close()?;
    worker.join().map_err(|_| "helper fixture panicked")??;
    Ok(())
}

#[test]
fn closing_either_shared_domain_closes_the_whole_owner() -> TestResult {
    for close_f10 in [false, true] {
        let client = TdxClient::builder().build()?;
        let helper = client.helpers();
        let f10 = client.f10();
        if close_f10 {
            f10.close()?;
        } else {
            client.close()?;
        }
        assert_eq!(
            helper
                .ipo_calendar(&OperationOptions::default())
                .unwrap_err()
                .kind(),
            ErrorKind::Closed
        );
        assert_eq!(
            client
                .connect(&OperationOptions::default())
                .unwrap_err()
                .kind(),
            ErrorKind::Closed
        );
        assert_eq!(
            f10.connect(&OperationOptions::default())
                .unwrap_err()
                .kind(),
            ErrorKind::Closed
        );
    }
    let standalone = F10Client::builder().build()?;
    let client = TdxClient::builder().build()?;
    client.close()?;
    let cancelled = CancellationToken::default();
    cancelled.cancel();
    assert_eq!(
        standalone
            .connect(&OperationOptions::default().with_cancellation(cancelled))
            .unwrap_err()
            .kind(),
        ErrorKind::Cancelled
    );
    standalone.close()?;
    Ok(())
}

#[test]
fn cancelled_or_expired_f10_helper_never_starts_either_transport() -> TestResult {
    let http = TcpListener::bind("127.0.0.1:0")?;
    http.set_nonblocking(true)?;
    let hq = TcpListener::bind("127.0.0.1:0")?;
    hq.set_nonblocking(true)?;
    let client = TdxClient::builder()
        .endpoints(vec![hq.local_addr()?])
        .f10(routed(http.local_addr()?))
        .build()?;
    let token = CancellationToken::default();
    token.cancel();
    for (options, expected) in [
        (
            OperationOptions::default().with_cancellation(token),
            ErrorKind::Cancelled,
        ),
        (
            OperationOptions::default().with_timeout(Duration::ZERO),
            ErrorKind::Timeout,
        ),
    ] {
        assert_eq!(
            client
                .helpers()
                .index_members(&IndexMembersQuery::new(code()?), &options)
                .unwrap_err()
                .kind(),
            expected
        );
        assert_eq!(
            client.helpers().ipo_calendar(&options).unwrap_err().kind(),
            expected
        );
    }
    assert_eq!(
        http.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(
        hq.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    client.close()?;
    Ok(())
}

#[test]
fn required_f10_protocol_failure_is_not_downgraded_to_empty_or_hq() -> TestResult {
    let (address, worker) = server(vec!["not JSON"])?;
    let hq = TcpListener::bind("127.0.0.1:0")?;
    hq.set_nonblocking(true)?;
    let client = TdxClient::builder()
        .endpoints(vec![hq.local_addr()?])
        .f10(routed(address))
        .build()?;
    assert_eq!(
        client
            .helpers()
            .ipo_calendar(&OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::Protocol
    );
    assert_eq!(
        hq.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    client.close()?;
    worker.join().map_err(|_| "helper fixture panicked")??;
    Ok(())
}

#[test]
fn stock_topics_merge_both_sources_in_discovery_order_with_last_source_raw() -> TestResult {
    let (address, worker) = server(vec![
        r#"{"ErrorCode":0,"ResultSets":[{"ColName":["t001","t002"],"Content":[["1","old"],["2","keep"],["1","last"]]}]}"#,
        r#"{"ErrorCode":0,"ResultSets":[{"ColName":["id","ztmc","gld","sslb"],"Content":[[1,null,3,{"unknown":true}],[3,"third",null,17]]}]}"#,
    ])?;
    let client = TdxClient::builder().f10(routed(address)).build()?;
    let result = client.helpers().stock_topics(
        &eltdx::StockTopicsQuery::new(code()?),
        &OperationOptions::default(),
    )?;
    assert_eq!(result.count(), 3);
    assert!(result.ok());
    assert_eq!(result.responses().len(), 2);
    assert_eq!(
        result
            .topics()
            .iter()
            .map(|topic| topic.topic_id())
            .collect::<Vec<_>>(),
        vec![Some("1"), Some("2"), Some("3")]
    );
    let first = &result.topics()[0];
    assert_eq!(first.topic_name(), None);
    assert_eq!(first.relation_level(), Some(3.0));
    assert_eq!(
        first.sources().collect::<Vec<_>>(),
        vec![eltdx::TopicSource::TopicIds, eltdx::TopicSource::HotTopics]
    );
    assert_eq!(first.raw()[0].1.cells()[1].value().as_str(), Some("last"));
    assert!(first
        .category_raw()
        .and_then(|value| value.get("unknown"))
        .is_some());
    assert_eq!(result.meta().sources().len(), 2);
    client.close()?;
    worker.join().map_err(|_| "topic fixture panicked")??;
    Ok(())
}

#[test]
fn explicit_topic_id_needs_only_compare_and_retains_nonempty_row_order() -> TestResult {
    let (address, worker) = server(vec![
        r#"{"ErrorCode":0,"ResultSets":[{"ColName":["pm","zqdm","zqjc","sc","zdf"],"Content":[[2,"300975","second",0,19.97],[1,"600000","first",1,2]]}]}"#,
    ])?;
    let client = TdxClient::builder().f10(routed(address)).build()?;
    let query = eltdx::TopicStocksQuery::new(code()?, "42")?;
    let result = client
        .helpers()
        .topic_stocks(&query, &OperationOptions::default())?;
    assert_eq!(result.topic_id(), "42");
    assert_eq!(result.count(), 2);
    assert_eq!(
        result
            .rows()
            .iter()
            .map(|row| row.rank())
            .collect::<Vec<_>>(),
        vec![Some(2), Some(1)]
    );
    assert_eq!(result.rows()[0].id(), Some(&"sz300975".parse()?));
    assert_eq!(result.rows()[0].change_pct(), Some(19.97));
    assert_eq!(client.f10().diagnostics()?.completed_requests(), 1);
    client.close()?;
    worker.join().map_err(|_| "compare fixture panicked")??;
    Ok(())
}

#[test]
fn named_topic_resolves_before_compare_and_retains_all_sources() -> TestResult {
    let (address, worker) = server(vec![
        r#"{"ErrorCode":0,"ResultSets":[{"ColName":["t001","t002"],"Content":[["1","one"],["2","two"]]}]}"#,
        r#"{"ErrorCode":0,"ResultSets":[]}"#,
        r#"{"ErrorCode":0,"ResultSets":[{"ColName":["pm","zqdm","sc"],"Content":[[1,"600000",1]]}]}"#,
    ])?;
    let client = TdxClient::builder().f10(routed(address)).build()?;
    let result = client.helpers().topic_stocks(
        &eltdx::TopicStocksQuery::named(code()?, "two")?,
        &OperationOptions::default(),
    )?;
    assert_eq!(result.topic_id(), "2");
    assert_eq!(result.topic_name(), Some("two"));
    assert_eq!(result.responses().len(), 3);
    assert_eq!(result.meta().sources().len(), 3);
    client.close()?;
    worker
        .join()
        .map_err(|_| "named topic fixture panicked")??;
    Ok(())
}

#[test]
fn required_hot_topic_failure_propagates_and_does_not_return_partial_topics() -> TestResult {
    let (address, worker) = server(vec![
        r#"{"ErrorCode":0,"ResultSets":[{"ColName":["t001","t002"],"Content":[["1","one"]]}]}"#,
        "invalid",
    ])?;
    let client = TdxClient::builder().f10(routed(address)).build()?;
    assert_eq!(
        client
            .helpers()
            .stock_topics(
                &eltdx::StockTopicsQuery::new(code()?),
                &OperationOptions::default()
            )
            .unwrap_err()
            .kind(),
        ErrorKind::Protocol
    );
    client.close()?;
    worker
        .join()
        .map_err(|_| "required topic fixture panicked")??;
    Ok(())
}

#[test]
fn topic_source_record_cap_applies_to_input_rows_before_deduplication() -> TestResult {
    let (address, worker) = server(vec![
        r#"{"ErrorCode":0,"ResultSets":[{"ColName":["t001","t002"],"Content":[["1","one"],["1","one"]]}]}"#,
        r#"{"ErrorCode":0,"ResultSets":[]}"#,
    ])?;
    let client = TdxClient::builder().f10(routed(address)).build()?;
    let query = eltdx::StockTopicsQuery::new(code()?).with_max_records(std::num::NonZeroUsize::MIN);
    assert_eq!(
        client
            .helpers()
            .stock_topics(&query, &OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::LimitExceeded
    );
    client.close()?;
    worker.join().map_err(|_| "topic cap fixture panicked")??;
    Ok(())
}

#[test]
fn theme_empty_selection_is_lazy_and_ladder_top_n_cannot_truncate_input() -> TestResult {
    let http = TcpListener::bind("127.0.0.1:0")?;
    http.set_nonblocking(true)?;
    let hq = TcpListener::bind("127.0.0.1:0")?;
    hq.set_nonblocking(true)?;
    let client = TdxClient::builder()
        .endpoints(vec![hq.local_addr()?])
        .f10(routed(http.local_addr()?))
        .build()?;
    let ladder = eltdx::LadderQuery::new()
        .with_selection(eltdx::SecuritySelection::Explicit(vec![]))?
        .with_count(Some(1))?;
    let query = eltdx::ThemeStrengthQuery::new()
        .with_ladder(ladder)?
        .with_count(Some(1))?;
    assert_eq!(query.ladder().count(), None);
    let result = client
        .helpers()
        .theme_strength_rank(&query, &OperationOptions::default())?;
    assert_eq!(result.count(), 0);
    assert_eq!(result.trade_date(), None);
    assert_eq!(result.missing_ids().count(), 0);
    assert_eq!(
        http.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(
        hq.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    client.close()?;
    Ok(())
}

#[test]
fn theme_entry_guards_reject_cancel_timeout_and_shared_close_without_io() -> TestResult {
    let http = TcpListener::bind("127.0.0.1:0")?;
    http.set_nonblocking(true)?;
    let hq = TcpListener::bind("127.0.0.1:0")?;
    hq.set_nonblocking(true)?;
    let client = TdxClient::builder()
        .endpoints(vec![hq.local_addr()?])
        .f10(routed(http.local_addr()?))
        .build()?;
    let query = eltdx::ThemeStrengthQuery::new().with_ladder(
        eltdx::LadderQuery::new().with_selection(eltdx::SecuritySelection::Explicit(vec![]))?,
    )?;
    let token = CancellationToken::default();
    token.cancel();
    assert_eq!(
        client
            .helpers()
            .theme_strength_rank(
                &query,
                &OperationOptions::default().with_cancellation(token)
            )
            .unwrap_err()
            .kind(),
        ErrorKind::Cancelled
    );
    assert_eq!(
        client
            .helpers()
            .theme_strength_rank(
                &query,
                &OperationOptions::default().with_timeout(Duration::ZERO)
            )
            .unwrap_err()
            .kind(),
        ErrorKind::Timeout
    );
    client.f10().close()?;
    assert_eq!(
        client
            .helpers()
            .theme_strength_rank(&query, &OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::Closed
    );
    assert_eq!(
        http.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(
        hq.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    Ok(())
}
