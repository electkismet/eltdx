//! T24 public-client source cases. Authored only: no compilation, collection,
//! server thread or socket in this file is executed before the T01–T30 gate.
#![cfg(feature = "ex")]
use std::error::Error as StdError;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::num::NonZeroUsize;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;
use eltdx::{CancellationToken, ErrorKind, OperationOptions, RawPolicy, TradingDate};
use eltdx::ex::*;
use eltdx::ex::hosts::{DiscoveryOptions, Endpoint, ProbeMode, ProbeOptions, ProbeStatus};

type TestResult<T = ()> = Result<T, Box<dyn StdError + Send + Sync>>;
fn peer(script: impl FnOnce(TcpStream) -> TestResult + Send + 'static) -> TestResult<(Endpoint, JoinHandle<TestResult>)> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let endpoint = listener.local_addr()?;
    let handle = thread::spawn(move || {
        let (stream, _) = listener.accept()?;
        stream.set_read_timeout(Some(Duration::from_secs(3)))?;
        stream.set_write_timeout(Some(Duration::from_secs(3)))?;
        script(stream)
    });
    Ok((endpoint, handle))
}
fn joined(handle: JoinHandle<TestResult>) -> TestResult {
    handle.join().map_err(|_| std::io::Error::other("EX peer panicked"))?
}
fn envelope(encoded: &[u8], decoded_len: usize) -> TestResult<Vec<u8>> {
    // Independent 16-byte response envelope; no production frame encoder.
    let mut bytes = 7_654_321_u32.to_le_bytes().to_vec();
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&u16::try_from(encoded.len())?.to_le_bytes());
    bytes.extend_from_slice(&u16::try_from(decoded_len)?.to_le_bytes());
    bytes.extend_from_slice(encoded);
    Ok(bytes)
}
fn send(stream: &mut TcpStream, body: &[u8]) -> TestResult {
    let bytes = envelope(body, body.len())?;
    // Multiple writes exercise the real exact-read path. The pure wire suite
    // independently covers every possible split; TCP may coalesce these writes.
    for chunk in bytes.chunks(7) { stream.write_all(chunk)?; }
    Ok(())
}
fn setup(stream: &mut TcpStream) -> TestResult {
    let mut request = [0_u8; 92];
    stream.read_exact(&mut request)?;
    assert_eq!(request, frame::SETUP_REQUEST);
    let body = [b'R'; 80];
    let mut zlib = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    zlib.write_all(&body)?;
    let encoded = zlib.finish()?;
    assert_ne!(encoded.len(), body.len());
    stream.write_all(&envelope(&encoded, body.len())?)?;
    Ok(())
}
fn request(stream: &mut TcpStream) -> TestResult<(u16, Vec<u8>)> {
    let mut header = [0_u8; 12];
    stream.read_exact(&mut header)?;
    assert_eq!(&header[6..8], &header[8..10]);
    let declared = usize::from(u16::from_le_bytes([header[6], header[7]]));
    let mut body = vec![0; declared.checked_sub(2).ok_or_else(|| std::io::Error::other("bad request length"))?];
    stream.read_exact(&mut body)?;
    Ok((u16::from_le_bytes([header[10], header[11]]), body))
}
fn counted(header: usize, count_at: usize, width: usize, count: u16) -> Vec<u8> {
    let mut body = vec![0; header + width * usize::from(count)];
    body[count_at..count_at + 2].copy_from_slice(&count.to_le_bytes());
    body
}
fn count_body(value: u32) -> Vec<u8> {
    let mut body = vec![0; 23];
    body[19..23].copy_from_slice(&value.to_le_bytes());
    body
}
fn id() -> eltdx::Result<ExInstrumentId> {
    Ok(ExInstrumentId::new(ExMarketId::new(31)?, ExCode::new("00700")?))
}
fn wait_eof(stream: &mut TcpStream) -> TestResult {
    let mut byte = [0_u8];
    match stream.read(&mut byte) {
        Err(error) if matches!(error.kind(), std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted) => {}
        result => assert_eq!(result?, 0),
    }
    Ok(())
}

#[test]
fn lazy_configuration_clone_close_and_no_reopen() -> TestResult {
    let client = ExClient::builder().build()?;
    assert_eq!(client.config().timeout(), Duration::from_secs(8));
    assert_eq!(client.config().raw(), RawPolicy::Omit);
    assert!(client.host()?.is_none());
    assert!(client.diagnostics()?.probes().is_empty());
    let clone = client.clone();
    clone.close()?;
    assert_eq!(client.connect(&OperationOptions::default()).unwrap_err().kind(), ErrorKind::Closed);
    client.disconnect()?;
    assert!(ExClient::builder().with_endpoints(Vec::new()).build().is_err());
    assert!(ExClient::builder().with_timeout(Duration::ZERO).build().is_err());
    Ok(())
}

#[test]
fn eleven_typed_methods_share_one_ready_socket_and_keep_native_values() -> TestResult {
    let (endpoint, server) = peer(|mut stream| {
        setup(&mut stream)?;
        for expected in [0x23f4, 0x23f0, 0x23f5, 0x23fa, 0x2400, 0x23ff, 0x240d, 0x240b, 0x240c, 0x23fc, 0x2406] {
            let (command, wire_query) = request(&mut stream)?;
            assert_eq!(command, expected);
            let body = match command {
                0x23f4 => {
                    let mut body = counted(2, 0, 64, 1);
                    body[2] = 1; body[35] = 31; body[3..5].copy_from_slice(b"HK"); body
                },
                0x23f0 => count_body(42),
                0x23f5 => {
                    let mut body = counted(6, 4, 64, 1);
                    body[..4].copy_from_slice(&100_u32.to_le_bytes());
                    body[6] = 2; body[7] = 31; body[11..16].copy_from_slice(b"00700"); body[69] = 0xa5; body
                },
                0x23fa => {
                    let mut body = vec![0; 150]; body[0] = 31; body[1..6].copy_from_slice(b"00700");
                    body[30..34].copy_from_slice(&0x80000000_u32.to_le_bytes()); body
                },
                0x2400 => {
                    let mut body = counted(2, 0, 300, 1); body[2] = 31; body[3..8].copy_from_slice(b"00700");
                    body[152..].fill(0xa5); body
                },
                0x23ff => {
                    let mut body = counted(20, 18, 32, 1);
                    body[20..24].copy_from_slice(&20240102_u32.to_le_bytes()); body
                },
                0x240d => {
                    assert_eq!(wire_query.len(), 20);
                    let mut body = counted(14, 12, 32, 1);
                    body[14..16].copy_from_slice(&((20_u16 << 11) | 102).to_le_bytes());
                    body[16..18].copy_from_slice(&570_u16.to_le_bytes()); body
                },
                0x240b | 0x240c => {
                    let header = if command == 0x240b { 12 } else { 20 };
                    let mut body = counted(header, header - 2, 18, 1);
                    body[header..header + 2].copy_from_slice(&570_u16.to_le_bytes()); body
                },
                0x23fc | 0x2406 => {
                    let mut body = counted(16, 14, 16, 1);
                    body[16..18].copy_from_slice(&570_u16.to_le_bytes());
                    body[30..32].copy_from_slice(&10007_u16.to_le_bytes()); body
                },
                _ => return Err(std::io::Error::other("unexpected command").into()),
            };
            send(&mut stream, &body)?;
        }
        wait_eof(&mut stream)
    })?;
    let client = ExClient::builder().with_endpoints(vec![endpoint]).build()?;
    let options = OperationOptions::default();
    client.connect(&options)?;
    client.ensure_connected(&options)?;
    assert_eq!(client.markets(&options)?[0].market().get(), 31);
    assert_eq!(client.instrument_count(&options)?, 42);
    let instruments = client.instruments(&InstrumentsQuery::new(0), &options)?;
    assert_eq!(instruments.rows()[0].code(), "00700");
    assert_eq!(instruments.rows()[0].record_raw()[63], 0xa5);
    assert_eq!(instruments.next_offset(), Some(101));
    assert_eq!(instruments.completion(), PageCompletion::Unknown);
    let quote = client.quote(&QuoteQuery::new(id()?), &options)?.ok_or_else(|| std::io::Error::other("missing quote"))?;
    assert_eq!(quote.price().value().bits(), 0x80000000);
    assert_eq!(quote.price().meaning().unit(), &NativeUnit::Unknown);
    let quotes = client.quote_list(&QuoteListQuery::new(ExMarketId::new(31)?, QuoteCategory::HongKong), &options)?;
    assert_eq!(quotes.rows()[0].unused_tail(), &[0xa5; 150]);
    assert!(quotes.meta().raw().is_none());
    assert!(quotes.meta().diagnostics().iter().any(|item| item.stage() == "association"));
    assert_eq!(client.bars(&BarsQuery::new(id()?, ExPeriod::Day), &options)?.rows().len(), 1);
    let date = TradingDate::new(2024, 1, 2)?;
    let range = client.bars_range(&BarsRangeQuery::new(id()?, date, date)?, &options)?;
    assert_eq!(range[0].last_slot_source(), BarLastSlotSource::RangeSettlement);
    assert_eq!(client.minutes(&MinutesQuery::new(id()?), &options)?.len(), 1);
    assert_eq!(client.history_minutes(&HistoryMinutesQuery::new(id()?, date)?, &options)?.len(), 1);
    assert_eq!(client.transactions(&TransactionsQuery::new(id()?), &options)?.rows()[0].nature(), 1);
    assert_eq!(client.history_transactions(&HistoryTransactionsQuery::new(id()?, date)?, &options)?.rows().len(), 1);
    assert_eq!(client.host()?, Some(endpoint));
    assert!(client.diagnostics()?.latest_meta().is_some());
    client.close()?;
    joined(server)
}

#[test]
fn explicit_omit_overrides_builder_raw_and_empty_pages_do_not_invent_eof() -> TestResult {
    let (endpoint, server) = peer(|mut stream| {
        setup(&mut stream)?;
        for index in 0..3 {
            assert_eq!(request(&mut stream)?.0, 0x23f5);
            send(&mut stream, &counted(6, 4, 64, if index == 2 { 1 } else { 0 }))?;
        }
        wait_eof(&mut stream)
    })?;
    let client = ExClient::builder().with_endpoints(vec![endpoint]).with_raw(RawPolicy::RetainWire).build()?;
    let first = client.instruments(&InstrumentsQuery::new(0), &OperationOptions::default())?;
    assert_eq!(first.completion(), PageCompletion::Unknown);
    assert_eq!(first.next_offset(), Some(0));
    let retained = first.meta().raw().ok_or_else(|| std::io::Error::other("raw default missing"))?;
    assert_eq!(retained.wire_frames().map(|frames| frames.len()), Some(1));
    let second = client.instruments(&InstrumentsQuery::new(0), &OperationOptions::default().with_raw(RawPolicy::Omit))?;
    assert!(second.meta().raw().is_none());
    let fulfilled = client.instruments(&InstrumentsQuery::new(0).with_count(1)?, &OperationOptions::default())?;
    assert_eq!(fulfilled.completion(), PageCompletion::RequestedCount);
    assert_eq!(fulfilled.next_offset(), Some(1));
    client.close()?;
    joined(server)
}

#[test]
fn transport_failover_confirms_old_socket_retirement_before_new_exchange() -> TestResult {
    let (retired_tx, retired_rx) = mpsc::channel();
    let (bad, bad_server) = peer(move |mut stream| {
        setup(&mut stream)?;
        assert_eq!(request(&mut stream)?.0, 0x23f0);
        stream.shutdown(Shutdown::Write)?;
        wait_eof(&mut stream)?;
        retired_tx.send(())?;
        Ok(())
    })?;
    let (good, good_server) = peer(move |mut stream| {
        retired_rx.recv_timeout(Duration::from_secs(2))?;
        setup(&mut stream)?;
        assert_eq!(request(&mut stream)?.0, 0x23f0);
        send(&mut stream, &count_body(7))?;
        wait_eof(&mut stream)
    })?;
    let client = ExClient::builder().with_endpoints(vec![bad, good]).build()?;
    assert_eq!(client.instrument_count(&OperationOptions::default())?, 7);
    assert_eq!(client.host()?, Some(good));
    assert_eq!(client.unhealthy_hosts()?, vec![bad]);
    client.close()?;
    joined(bad_server)?;
    joined(good_server)
}

#[test]
fn malformed_command_reply_is_protocol_and_never_connects_backup() -> TestResult {
    let (bad, server) = peer(|mut stream| {
        setup(&mut stream)?;
        request(&mut stream)?;
        send(&mut stream, &[0; 22])?;
        wait_eof(&mut stream)
    })?;
    let backup = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    backup.set_nonblocking(true)?;
    let client = ExClient::builder().with_endpoints(vec![bad, backup.local_addr()?]).build()?;
    assert_eq!(client.instrument_count(&OperationOptions::default()).unwrap_err().kind(), ErrorKind::Protocol);
    assert_eq!(backup.accept().unwrap_err().kind(), std::io::ErrorKind::WouldBlock);
    assert!(client.host()?.is_none());
    client.close()?;
    joined(server)
}

#[test]
fn cancellation_during_fragmented_setup_confirms_socket_retirement() -> TestResult {
    let (partial_tx, partial_rx) = mpsc::channel();
    let (endpoint, server) = peer(move |mut stream| {
        let mut setup = [0; 92]; stream.read_exact(&mut setup)?;
        let frame = envelope(&[0; 40], 40)?;
        stream.write_all(&frame[..5])?;
        partial_tx.send(())?;
        wait_eof(&mut stream)
    })?;
    let client = ExClient::builder().with_endpoints(vec![endpoint]).build()?;
    let token = CancellationToken::default();
    let worker_client = client.clone();
    let options = OperationOptions::default().with_cancellation(token.clone());
    let call = thread::spawn(move || worker_client.connect(&options));
    partial_rx.recv_timeout(Duration::from_secs(2))?;
    token.cancel();
    let error = call.join().map_err(|_| std::io::Error::other("EX caller panicked"))?.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Cancelled);
    assert!(client.host()?.is_none());
    client.close()?;
    joined(server)
}

#[test]
fn shared_close_interrupts_partial_business_body_and_cannot_reopen() -> TestResult {
    let (partial_tx, partial_rx) = mpsc::channel();
    let (endpoint, server) = peer(move |mut stream| {
        setup(&mut stream)?;
        request(&mut stream)?;
        let frame = envelope(&count_body(9), 23)?;
        stream.write_all(&frame[..20])?;
        partial_tx.send(())?;
        wait_eof(&mut stream)
    })?;
    let client = ExClient::builder().with_endpoints(vec![endpoint]).build()?;
    let worker_client = client.clone();
    let call = thread::spawn(move || worker_client.instrument_count(&OperationOptions::default()));
    partial_rx.recv_timeout(Duration::from_secs(2))?;
    client.close()?;
    assert_eq!(call.join().map_err(|_| std::io::Error::other("EX caller panicked"))?.unwrap_err().kind(), ErrorKind::Closed);
    assert_eq!(client.connect(&OperationOptions::default()).unwrap_err().kind(), ErrorKind::Closed);
    joined(server)
}

#[test]
fn tcp_only_is_separate_from_data_ready_and_probes_leave_no_idle_socket() -> TestResult {
    let (tcp_endpoint, tcp_server) = peer(|mut stream| wait_eof(&mut stream))?;
    let (data_endpoint, data_server) = peer(|mut stream| {
        setup(&mut stream)?;
        assert_eq!(request(&mut stream)?.0, 0x23f4);
        send(&mut stream, &counted(2, 0, 64, 0))?;
        wait_eof(&mut stream)
    })?;
    let client = ExClient::builder().with_endpoints(vec![tcp_endpoint, data_endpoint]).build()?;
    let tcp = DiscoveryOptions::default().with_probe(ProbeOptions::default().with_mode(ProbeMode::TcpOnly));
    let result = client.probe_endpoints(&[tcp_endpoint], &tcp, &OperationOptions::default())?;
    assert_eq!(result[0].status(), ProbeStatus::Ready);
    assert!(!result[0].data_ready());
    assert!(client.healthy_hosts()?.is_empty());
    let data = client.probe_endpoints(&[data_endpoint], &DiscoveryOptions::default(), &OperationOptions::default())?;
    assert!(data[0].data_ready());
    assert_eq!(client.healthy_hosts()?, vec![data_endpoint]);
    assert!(client.host()?.is_none());
    client.close()?;
    joined(tcp_server)?;
    joined(data_server)
}

#[test]
fn last_owner_drop_retires_idle_socket_without_explicit_close() -> TestResult {
    let (endpoint, server) = peer(|mut stream| { setup(&mut stream)?; wait_eof(&mut stream) })?;
    let client = ExClient::builder().with_endpoints(vec![endpoint]).build()?;
    client.connect(&OperationOptions::default())?;
    drop(client);
    joined(server)
}

#[test]
fn byte_cap_stops_after_header_before_declared_body_is_acquired() -> TestResult {
    let (endpoint, server) = peer(|mut stream| {
        let mut setup = [0; 92]; stream.read_exact(&mut setup)?;
        let frame = envelope(&[0; 100], 100)?;
        // A complete header announces a body larger than the shared allowance.
        stream.write_all(&frame[..16])?;
        wait_eof(&mut stream)
    })?;
    let maximum = NonZeroUsize::new(32).ok_or_else(|| std::io::Error::other("positive byte cap"))?;
    let client = ExClient::builder().with_endpoints(vec![endpoint]).with_max_operation_bytes(maximum).build()?;
    assert_eq!(client.connect(&OperationOptions::default()).unwrap_err().kind(), ErrorKind::LimitExceeded);
    client.close()?;
    joined(server)
}

#[test]
fn cancelled_partial_reply_cannot_leak_into_a_new_operation() -> TestResult {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let endpoint = listener.local_addr()?;
    let (partial_tx, partial_rx) = mpsc::channel();
    let server = thread::spawn(move || -> TestResult {
        for index in 0..2 {
            let (mut stream, _) = listener.accept()?;
            stream.set_read_timeout(Some(Duration::from_secs(3)))?;
            setup(&mut stream)?;
            assert_eq!(request(&mut stream)?.0, 0x23f0);
            if index == 0 {
                let stale = envelope(&count_body(111), 23)?;
                stream.write_all(&stale[..18])?;
                partial_tx.send(())?;
                // The replacement connection is accepted only after confirmed
                // retirement of the old partial-response socket.
                wait_eof(&mut stream)?;
            } else {
                send(&mut stream, &count_body(222))?;
                wait_eof(&mut stream)?;
            }
        }
        Ok(())
    });
    let client = ExClient::builder().with_endpoints(vec![endpoint]).build()?;
    let caller = client.clone();
    let token = CancellationToken::default();
    let options = OperationOptions::default().with_cancellation(token.clone());
    let first = thread::spawn(move || caller.instrument_count(&options));
    partial_rx.recv_timeout(Duration::from_secs(2))?;
    token.cancel();
    assert_eq!(first.join().map_err(|_| std::io::Error::other("EX caller panicked"))?.unwrap_err().kind(), ErrorKind::Cancelled);
    assert_eq!(client.instrument_count(&OperationOptions::default())?, 222);
    client.close()?;
    joined(server)
}

#[test]
fn two_owned_callers_serialize_different_complete_reply_layouts() -> TestResult {
    let (endpoint, server) = peer(|mut stream| {
        setup(&mut stream)?;
        let mut seen = Vec::new();
        for _ in 0..2 {
            let command = request(&mut stream)?.0;
            seen.push(command);
            let body = match command {
                0x23f0 => count_body(39),
                0x23f4 => counted(2, 0, 64, 0),
                _ => return Err(std::io::Error::other("unexpected concurrent command").into()),
            };
            send(&mut stream, &body)?;
        }
        seen.sort_unstable();
        assert_eq!(seen, vec![0x23f0, 0x23f4]);
        wait_eof(&mut stream)
    })?;
    let client = ExClient::builder().with_endpoints(vec![endpoint]).build()?;
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let count_client = client.clone(); let count_barrier = barrier.clone();
    let counts = thread::spawn(move || { count_barrier.wait(); count_client.instrument_count(&OperationOptions::default()) });
    let market_client = client.clone(); let market_barrier = barrier.clone();
    let markets = thread::spawn(move || { market_barrier.wait(); market_client.markets(&OperationOptions::default()) });
    barrier.wait();
    assert_eq!(counts.join().map_err(|_| std::io::Error::other("count caller panicked"))??, 39);
    assert!(markets.join().map_err(|_| std::io::Error::other("market caller panicked"))??.is_empty());
    client.close()?;
    joined(server)
}

#[test]
fn discovery_parent_cancellation_is_terminal_instead_of_a_failed_host_row() -> TestResult {
    let (partial_tx, partial_rx) = mpsc::channel();
    let (endpoint, server) = peer(move |mut stream| {
        let mut setup = [0; 92]; stream.read_exact(&mut setup)?;
        let frame = envelope(&[0; 40], 40)?;
        stream.write_all(&frame[..5])?;
        partial_tx.send(())?;
        wait_eof(&mut stream)
    })?;
    let client = ExClient::builder().with_endpoints(vec![endpoint]).build()?;
    let caller = client.clone();
    let token = CancellationToken::default();
    let options = OperationOptions::default().with_cancellation(token.clone());
    let call = thread::spawn(move || caller.discover(&DiscoveryOptions::default(), &options));
    partial_rx.recv_timeout(Duration::from_secs(2))?;
    token.cancel();
    assert_eq!(call.join().map_err(|_| std::io::Error::other("probe caller panicked"))?.unwrap_err().kind(), ErrorKind::Cancelled);
    assert!(client.healthy_hosts()?.is_empty());
    client.close()?;
    joined(server)
}

#[test]
fn public_range_sequences_are_independent_between_owners() -> TestResult {
    fn read_range(stream: &mut TcpStream, expected: u8) -> TestResult {
        let mut header = [0; 12]; stream.read_exact(&mut header)?;
        assert_eq!(header[1], expected);
        assert_eq!(&header[10..], &[0x0d, 0x24]);
        let mut body = [0; 20]; stream.read_exact(&mut body)?;
        send(stream, &counted(14, 12, 32, 0))
    }
    let (first, first_server) = peer(|mut stream| {
        setup(&mut stream)?; read_range(&mut stream, 1)?; read_range(&mut stream, 2)?; wait_eof(&mut stream)
    })?;
    let (second, second_server) = peer(|mut stream| {
        setup(&mut stream)?; read_range(&mut stream, 1)?; wait_eof(&mut stream)
    })?;
    let first_client = ExClient::builder().with_endpoints(vec![first]).build()?;
    let second_client = ExClient::builder().with_endpoints(vec![second]).build()?;
    let date = TradingDate::new(2024, 1, 2)?;
    let query = BarsRangeQuery::new(id()?, date, date)?;
    first_client.bars_range(&query, &OperationOptions::default())?;
    second_client.bars_range(&query, &OperationOptions::default())?;
    first_client.bars_range(&query, &OperationOptions::default())?;
    first_client.close()?; second_client.close()?;
    joined(first_server)?;
    joined(second_server)
}

#[test]
fn shared_extra_attempt_limit_prevents_backup_connect_and_preserves_failure() -> TestResult {
    let (bad, server) = peer(|mut stream| {
        setup(&mut stream)?;
        request(&mut stream)?;
        stream.shutdown(Shutdown::Write)?;
        wait_eof(&mut stream)
    })?;
    let backup = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    backup.set_nonblocking(true)?;
    let client = ExClient::builder().with_endpoints(vec![bad, backup.local_addr()?])
        .with_extra_attempt_budget(0).build()?;
    let error = client.instrument_count(&OperationOptions::default()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::LimitExceeded);
    let previous = StdError::source(&error).and_then(|source| source.downcast_ref::<eltdx::Error>())
        .ok_or_else(|| std::io::Error::other("last network failure missing"))?;
    assert_eq!(previous.kind(), ErrorKind::Connection);
    assert_eq!(backup.accept().unwrap_err().kind(), std::io::ErrorKind::WouldBlock);
    client.close()?;
    joined(server)
}
