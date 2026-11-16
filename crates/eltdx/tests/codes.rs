#[path = "support/hq_server.rs"]
mod hq_server;

use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eltdx::{CodeQuery, CodeScanQuery, EndEvidence, ErrorKind, HqMarket, OperationOptions, PageOptions, TdxClient};
use hq_server::{directory, directory_offset, Server};

#[test]
fn short_short_empty_scan_does_not_query_count_and_retains_page_origins() -> Result<(), Box<dyn std::error::Error>> {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let calls = Arc::clone(&observed);
    let server = Server::start(move |request| {
        assert_eq!(request.msg_type, 0x044d);
        let offset = directory_offset(request);
        calls.lock().unwrap().push(offset);
        match offset { 0 => directory(&[("600000", 2)]), 1 => directory(&[("600001", 3)]), 2 => directory(&[]), _ => panic!("unexpected offset") }
    })?;
    let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).build()?;
    let options = OperationOptions::default().with_raw(eltdx::RawPolicy::RetainPayload);
    let result = client.codes().all(HqMarket::Shanghai, &CodeScanQuery::default(), &options)?;
    assert_eq!(*observed.lock().unwrap(), vec![0, 1, 2]);
    assert_eq!(result.items().len(), 2);
    assert_eq!(result.pages(), 3);
    assert_eq!(result.end(), EndEvidence::EmptyPage);
    assert_eq!(result.meta().sources().len(), 3);
    assert_eq!(result.meta().raw().unwrap().payloads().len(), 3);
    client.close()?;
    server.finish()?;
    Ok(())
}

#[test]
fn repeated_directory_identity_is_protocol_and_cap_before_eof_is_limit() -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::start(|request| { assert_eq!(request.msg_type, 0x044d); directory(&[("600000", 2)]) })?;
    let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).build()?;
    let error = client.codes().all(HqMarket::Shanghai, &CodeScanQuery::default(), &OperationOptions::default()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Protocol);
    let query = CodeScanQuery::default().with_pages(PageOptions::new(1_600, 1, 320_000)?)?;
    let error = client.codes().all(HqMarket::Shanghai, &query, &OperationOptions::default()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::LimitExceeded);
    client.close()?;
    server.finish()?;
    Ok(())
}

#[test]
fn empty_window_and_empty_batches_do_not_connect() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let client = TdxClient::builder().endpoints(vec![listener.local_addr()?]).build()?;
    let page = client.codes().list(HqMarket::Shanghai, &CodeQuery::default().with_limit(0)?, &OperationOptions::default())?;
    assert_eq!(page.end(), EndEvidence::RequestedWindow);
    assert!(client.quotes().snapshots(&[], &OperationOptions::default())?.is_empty());
    assert!(client.quotes().depth(&[], &OperationOptions::default())?.updates().is_empty());
    assert!(matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock));
    client.close()?;
    Ok(())
}

#[test]
fn continuation_checks_owner_size_and_metadata_before_send_and_collects_remaining_rows() -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::start(|request| {
        assert_eq!(request.msg_type, 0x044d);
        match directory_offset(request) { 0 => directory(&[("600000", 2)]), 1 => directory(&[("600001", 2)]), 2 => directory(&[]), _ => panic!("offset") }
    })?;
    let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).build()?;
    let other = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).build()?;
    let options = OperationOptions::default().with_timeout(Duration::from_secs(3));
    let first = client.codes().list(HqMarket::Shanghai, &CodeQuery::default(), &options)?;
    let token = first.next_cursor().unwrap();
    let query = CodeQuery::default().with_cursor(token)?;
    assert_eq!(other.codes().list(HqMarket::Shanghai, &query, &options).unwrap_err().kind(), ErrorKind::InvalidArgument);
    assert_eq!(client.codes().list(HqMarket::Shanghai, &query.clone().with_limit(80)?, &options).unwrap_err().kind(), ErrorKind::InvalidArgument);
    let remaining = client.codes().all(HqMarket::Shanghai, &CodeScanQuery::default().with_cursor(token)?, &options)?;
    assert_eq!(remaining.items().len(), 1);
    assert_eq!(remaining.items()[0].id().number(), "600001");
    client.codes().all(HqMarket::Shanghai, &CodeScanQuery::default(), &options)?;
    assert_eq!(client.codes().list(HqMarket::Shanghai, &query, &options).unwrap_err().kind(), ErrorKind::InvalidArgument);
    client.close()?;
    other.close()?;
    server.finish()?;
    Ok(())
}
