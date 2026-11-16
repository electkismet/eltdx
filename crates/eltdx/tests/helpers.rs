//! Public cross-source helpers. Authored cases, deferred until the source gate.
#![cfg(feature = "f10")]
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};
use eltdx::f10::{AshareCode, BoardMembersQuery, F10Client, F10MarketCode,
    IndexMembersQuery, IndexTrackingProductsQuery};
use eltdx::{CancellationToken, ErrorKind, OperationOptions, TdxClient};

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

fn receive(stream: &mut TcpStream) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut bytes = Vec::new();
    let header_end = loop {
        let mut byte = [0];
        stream.read_exact(&mut byte)?;
        bytes.push(byte[0]);
        if bytes.ends_with(b"\r\n\r\n") { break bytes.len(); }
        if bytes.len() > 64 * 1024 { return Err("oversized fixture header".into()); }
    };
    let headers = std::str::from_utf8(&bytes)?;
    let size: usize = headers.lines().filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("Content-Length"))
        .ok_or("fixture needs content length")?.1.trim().parse()?;
    if size > 64 * 1024 { return Err("oversized fixture body".into()); }
    bytes.resize(header_end + size, 0);
    stream.read_exact(&mut bytes[header_end..])?;
    Ok(bytes)
}

fn server(replies: Vec<&'static str>) -> Result<(SocketAddr, thread::JoinHandle<TestResult>), Box<dyn std::error::Error + Send + Sync>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    listener.set_nonblocking(true)?;
    let worker = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(8);
        for reply in replies {
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => thread::yield_now(),
                    Err(error) => return Err(error.into()),
                }
            };
            let request = receive(&mut stream)?;
            assert!(request.starts_with(b"POST "));
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", reply.len(), reply)?;
        }
        Ok(())
    });
    Ok((address, worker))
}

fn code() -> eltdx::Result<AshareCode> { AshareCode::new(F10MarketCode::new(1), "000300") }
fn routed(address: SocketAddr) -> eltdx::f10::F10ClientBuilder {
    F10Client::builder().with_base_url(format!("http://{address}/"))
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
    let client = TdxClient::builder().endpoints(vec![hq.local_addr()?]).f10(routed(address)).build()?;
    let helper = client.helpers();
    let options = OperationOptions::default();
    assert_eq!(helper.index_members(&IndexMembersQuery::new(code()?), &options)?.rows()[0].name(), Some("member"));
    assert_eq!(helper.index_etfs(&IndexTrackingProductsQuery::new(code()?), &options)?.etfs().len(), 1);
    assert_eq!(helper.board_members(&BoardMembersQuery::new(code()?), &options)?.codes(), vec!["600000"]);
    assert_eq!(helper.ipo_calendar(&options)?.count(), 1);
    assert_eq!(client.f10().diagnostics()?.completed_requests(), 4);
    assert_eq!(hq.accept().unwrap_err().kind(), std::io::ErrorKind::WouldBlock);
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
        if close_f10 { f10.close()?; } else { client.close()?; }
        assert_eq!(helper.ipo_calendar(&OperationOptions::default()).unwrap_err().kind(), ErrorKind::Closed);
        assert_eq!(client.connect(&OperationOptions::default()).unwrap_err().kind(), ErrorKind::Closed);
        assert_eq!(f10.connect(&OperationOptions::default()).unwrap_err().kind(), ErrorKind::Closed);
    }
    let standalone = F10Client::builder().build()?;
    let client = TdxClient::builder().build()?;
    client.close()?;
    let cancelled = CancellationToken::default(); cancelled.cancel();
    assert_eq!(standalone.connect(&OperationOptions::default().with_cancellation(cancelled)).unwrap_err().kind(), ErrorKind::Cancelled);
    standalone.close()?;
    Ok(())
}

#[test]
fn cancelled_or_expired_f10_helper_never_starts_either_transport() -> TestResult {
    let http = TcpListener::bind("127.0.0.1:0")?; http.set_nonblocking(true)?;
    let hq = TcpListener::bind("127.0.0.1:0")?; hq.set_nonblocking(true)?;
    let client = TdxClient::builder().endpoints(vec![hq.local_addr()?]).f10(routed(http.local_addr()?)).build()?;
    let token = CancellationToken::default(); token.cancel();
    for (options, expected) in [
        (OperationOptions::default().with_cancellation(token), ErrorKind::Cancelled),
        (OperationOptions::default().with_timeout(Duration::ZERO), ErrorKind::Timeout),
    ] {
        assert_eq!(client.helpers().index_members(&IndexMembersQuery::new(code()?), &options).unwrap_err().kind(), expected);
        assert_eq!(client.helpers().ipo_calendar(&options).unwrap_err().kind(), expected);
    }
    assert_eq!(http.accept().unwrap_err().kind(), std::io::ErrorKind::WouldBlock);
    assert_eq!(hq.accept().unwrap_err().kind(), std::io::ErrorKind::WouldBlock);
    client.close()?;
    Ok(())
}

#[test]
fn required_f10_protocol_failure_is_not_downgraded_to_empty_or_hq() -> TestResult {
    let (address, worker) = server(vec!["not JSON"])?;
    let hq = TcpListener::bind("127.0.0.1:0")?; hq.set_nonblocking(true)?;
    let client = TdxClient::builder().endpoints(vec![hq.local_addr()?]).f10(routed(address)).build()?;
    assert_eq!(client.helpers().ipo_calendar(&OperationOptions::default()).unwrap_err().kind(), ErrorKind::Protocol);
    assert_eq!(hq.accept().unwrap_err().kind(), std::io::ErrorKind::WouldBlock);
    client.close()?;
    worker.join().map_err(|_| "helper fixture panicked")??;
    Ok(())
}
