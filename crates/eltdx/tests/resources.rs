//! Authored synthetic source cases; execution waits for the whole-plan gate.
#[path = "support/hq_server.rs"] mod hq_server;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};
use eltdx::{CancellationToken, DownloadCompletion, ErrorKind, OperationOptions, RawPolicy, StatsQuery, TdxClient};
use eltdx_protocol::frame::RequestFrame;

fn client(server: &hq_server::Server) -> TdxClient {
    TdxClient::builder().endpoints(vec![server.address]).pool_size(1).heartbeat_interval(None).build().unwrap()
}
fn range(request: &RequestFrame) -> (usize, usize) {
    (u32::from_le_bytes(request.data[..4].try_into().unwrap()) as usize,
        u32::from_le_bytes(request.data[4..8].try_into().unwrap()) as usize)
}
fn chunk(bytes: &[u8]) -> Vec<u8> { let mut output = (bytes.len() as u32).to_le_bytes().to_vec(); output.extend_from_slice(bytes); output }

#[test]
fn zero_caps_ranges_and_invalid_paths_do_not_start_the_runtime() {
    let server = hq_server::Server::start(|_| panic!("no file I/O expected")).unwrap(); let client = client(&server);
    let options = OperationOptions::default();
    let zero = client.resources().read(" file ", 9, 0, &options).unwrap();
    assert!(zero.data.is_empty()); assert_eq!(zero.completion, DownloadCompletion::LimitReached); assert_eq!(zero.next_offset, Some(9));
    assert_eq!(client.resources().download_file("file", 0, &options).unwrap().next_offset, Some(0));
    assert_eq!(client.resources().download_complete("file", 0, &options).unwrap_err().kind(), ErrorKind::LimitExceeded);
    for path in ["", "统计.zip", "a\0b"] {
        assert_eq!(client.resources().read(path, 0, 0, &options).unwrap_err().kind(), ErrorKind::InvalidArgument);
    }
    for (offset, size) in [(u64::from(u32::MAX) + 1, 0), (u64::from(u32::MAX), 1), (0, 60_001)] {
        assert_eq!(client.resources().read("file", offset, size, &options).unwrap_err().kind(), ErrorKind::InvalidArgument);
    }
    assert!(StatsQuery::new().with_chunk_size(0).is_err()); assert!(StatsQuery::new().with_chunk_size(60_001).is_err());
    assert_eq!(server.connection_count(), 0); client.close().unwrap(); server.finish().unwrap();
}

#[test]
fn expired_entry_is_not_revived_by_an_empty_download() {
    let server = hq_server::Server::start(|_| panic!("expired operation must not send")).unwrap(); let client = client(&server);
    let options = OperationOptions::default().with_timeout(std::time::Duration::ZERO);
    assert_eq!(client.resources().download_file("file", 0, &options).unwrap_err().kind(), ErrorKind::Timeout);
    assert_eq!(client.resources().read("file", 0, 0, &options).unwrap_err().kind(), ErrorKind::Timeout);
    assert_eq!(server.connection_count(), 0); client.close().unwrap(); server.finish().unwrap();
}

#[test]
fn exact_cap_is_uncertain_but_one_more_byte_allows_the_empty_eof_probe() {
    let requests = Arc::new(Mutex::new(Vec::new())); let seen = requests.clone();
    let server = hq_server::Server::start(move |request| {
        assert_eq!(request.msg_type, 0x06b9); let (offset, size) = range(request);
        seen.lock().unwrap().push((offset, size));
        let end = (offset + size).min(6); chunk(&b"abc123"[offset.min(6)..end])
    }).unwrap(); let client = client(&server); let resources = client.resources(); let options = OperationOptions::default();
    let exact = resources.download_file("file", 6, &options).unwrap();
    assert_eq!(&exact.data[..], b"abc123"); assert_eq!(exact.completion, DownloadCompletion::LimitReached); assert_eq!(exact.next_offset, Some(6));
    assert_eq!(*requests.lock().unwrap(), vec![(0, 6)]);
    assert_eq!(resources.download_complete("file", 6, &options).unwrap_err().kind(), ErrorKind::LimitExceeded);
    let complete = resources.download_file("file", 7, &options).unwrap();
    assert_eq!(&complete.data[..], b"abc123"); assert_eq!(complete.completion, DownloadCompletion::Complete); assert_eq!(complete.next_offset, None);
    assert_eq!(&resources.download_complete("file", 7, &options).unwrap()[..], b"abc123");
    let part = resources.read("file", 3, 3, &options).unwrap(); assert_eq!(&part.data[..], b"123"); assert_eq!(part.next_offset, Some(6));
    let eof = resources.read("file", 6, 1, &options).unwrap(); assert_eq!(eof.completion, DownloadCompletion::Complete);
    client.close().unwrap(); server.finish().unwrap();
}

#[test]
fn exact_chunk_boundary_requires_an_empty_response_on_the_same_pin() {
    let requests = Arc::new(Mutex::new(Vec::new())); let seen = requests.clone();
    let server = hq_server::Server::start(move |request| {
        let (offset, size) = range(request); seen.lock().unwrap().push((offset, size));
        chunk(&vec![b'x'; size.min(30_000_usize.saturating_sub(offset))])
    }).unwrap(); let client = client(&server);
    let value = client.resources().download_file(" T0002\\file.dat ", 30_001, &OperationOptions::default()).unwrap();
    assert_eq!(value.data.len(), 30_000); assert_eq!(value.completion, DownloadCompletion::Complete);
    assert_eq!(*requests.lock().unwrap(), vec![(0, 30_000), (30_000, 1)]);
    assert_eq!(server.connection_count(), 1); client.close().unwrap(); server.finish().unwrap();
}

#[test]
fn malformed_chunks_fail_and_release_the_pin_before_a_subsequent_request() {
    for mode in 0..2 {
        let requests = Arc::new(AtomicUsize::new(0)); let seen = requests.clone();
        let server = hq_server::Server::start(move |_| {
            if seen.fetch_add(1, Ordering::SeqCst) == 0 {
                if mode == 0 { vec![6, 0, 0, 0, b'x'] } else { chunk(b"12345678") }
            } else { chunk(b"ok") }
        }).unwrap(); let client = client(&server); let options = OperationOptions::default();
        assert_eq!(client.resources().download_file("file", 7, &options).unwrap_err().kind(), ErrorKind::Protocol);
        assert_eq!(&client.resources().download_complete("file", 3, &options).unwrap()[..], b"ok");
        client.close().unwrap(); server.finish().unwrap();
    }
}

#[test]
fn cancellation_stops_paging_and_retires_before_close() {
    let token = CancellationToken::default(); let cancel = token.clone(); let count = Arc::new(AtomicUsize::new(0)); let seen = count.clone();
    let server = hq_server::Server::start(move |request| {
        seen.fetch_add(1, Ordering::SeqCst); cancel.cancel(); chunk(&vec![0; range(request).1])
    }).unwrap(); let client = client(&server);
    assert_eq!(client.resources().download_file("file", 60_001, &OperationOptions::default().with_cancellation(token)).unwrap_err().kind(), ErrorKind::Cancelled);
    assert_eq!(count.load(Ordering::SeqCst), 1); client.close().unwrap(); server.finish().unwrap();
}

#[test]
fn a_lost_pin_never_restarts_the_file_on_a_different_server() {
    let second_requests = Arc::new(AtomicUsize::new(0)); let seen_second = second_requests.clone();
    let fallback = hq_server::Server::start(move |_| { seen_second.fetch_add(1, Ordering::SeqCst); chunk(b"other") }).unwrap();
    let first_requests = Arc::new(AtomicUsize::new(0)); let seen_first = first_requests.clone();
    let primary = hq_server::Server::start_controlled(move |request, connection| {
        let current = seen_first.fetch_add(1, Ordering::SeqCst);
        if current == 0 { chunk(&vec![b'x'; range(request).1]) }
        else { connection.disconnect().unwrap(); chunk(b"") }
    }).unwrap();
    let client = TdxClient::builder().endpoints(vec![primary.address, fallback.address]).pool_size(1).heartbeat_interval(None).build().unwrap();
    let error = client.resources().download_file("file", 30_001, &OperationOptions::default()).unwrap_err();
    assert!(matches!(error.kind(), ErrorKind::PinLost | ErrorKind::Connection | ErrorKind::CancelUnconfirmed),
        "lost pin: {error:?}; primary requests={}; fallback requests={}",
        first_requests.load(Ordering::SeqCst), second_requests.load(Ordering::SeqCst));
    assert_eq!(second_requests.load(Ordering::SeqCst), 0); assert_eq!(first_requests.load(Ordering::SeqCst), 2);
    client.close().unwrap(); primary.finish().unwrap(); fallback.finish().unwrap();
}

#[test]
fn block_metadata_retains_server_checksum_and_optional_raw() {
    let server = hq_server::Server::start(|request| {
        assert_eq!(request.msg_type, 0x02c5); assert_eq!(request.data.len(), 40);
        let mut payload = vec![0; 38]; payload[..4].copy_from_slice(&123_u32.to_le_bytes());
        payload[5..37].copy_from_slice(b"0123456789abcdef0123456789abcdef"); payload
    }).unwrap(); let client = client(&server);
    let value = client.resources().block_meta("block.dat", &OperationOptions::default().with_raw(RawPolicy::RetainWire)).unwrap();
    assert_eq!(value.size(), 123); assert_eq!(value.md5(), "0123456789abcdef0123456789abcdef");
    assert_eq!(value.meta().source().command(), Some(0x02c5)); assert_eq!(value.meta().source().endpoint(), Some(server.address));
    assert_eq!(value.meta().raw().unwrap().wire_frames().unwrap().len(), 1);
    client.close().unwrap(); server.finish().unwrap();
}
