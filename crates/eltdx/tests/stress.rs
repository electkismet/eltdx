//! Explicitly ignored candidate stress sources. No public server is contacted.
//! Run through rust_sdk_stress.py only after all implementation sources exist.
#[path = "support/hq_server.rs"] mod hq_server;
#[path = "support/stress_log.rs"] mod stress_log;
use std::io;
use std::sync::{mpsc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use eltdx::{CancellationToken, CodeQuery, CodeScanQuery, ErrorKind, HqMarket,
    OperationOptions, PageOptions, PushEvent, RawPolicy, SubscriptionOptions, TdxClient};
use hq_server::Server;
use stress_log::{count, Log};
type Case = Result<(), Box<dyn std::error::Error + Send + Sync>>;

fn client(server: &Server, pool: usize) -> eltdx::Result<TdxClient> {
    TdxClient::builder().endpoints(vec![server.address]).pool_size(pool)
        .heartbeat_interval(None).request_timeout(Duration::from_secs(5)).build()
}
fn closed(server: &Server) -> io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while server.active_connection_count() != 0 && Instant::now() < deadline {
        thread::park_timeout(Duration::from_millis(1));
    }
    if server.active_connection_count() != 0 { return Err(io::Error::other("explicit close left live fixture sockets")); }
    Ok(())
}
fn one(client: &TdxClient, id: usize, options: &OperationOptions) -> eltdx::Result<()> {
    let query = CodeQuery::new().with_start(u32::try_from(id).map_err(|error|
        eltdx::Error::new(ErrorKind::InvalidArgument, error.to_string()))?)?.with_limit(1)?;
    let rows = client.codes().list(HqMarket::Shanghai, &query, options)?;
    assert_eq!(rows.items().len(), 1);
    assert_eq!(rows.items()[0].id().number(), format!("{:06}", 100_000 + id));
    Ok(())
}
fn directory(id: usize) -> Vec<u8> {
    hq_server::directory(&[(&format!("{:06}", 100_000 + id), 2)])
}

#[test]
#[ignore = "candidate-only: >=100000 actual wire requests; raw event evidence required"]
fn stress_unique_terminals_and_bounded_concurrency() -> Case {
    let total = count("ELTDX_RUST_STRESS_REQUESTS", 125_000, 125_000, 500_000)?;
    let workers = count("ELTDX_RUST_STRESS_WORKERS", 8, 2, 64)?;
    let log = Log::start("unique", total, workers)?;
    let captured = log.clone();
    let server = Server::start_controlled(move |request, connection| {
        assert_eq!(request.msg_type, 0x044d);
        let id = hq_server::directory_offset(request) as usize;
        if id < total {
            captured.wire_for(id, connection.identity(), request.msg_id, request.msg_type).expect("durable wire log");
        } else { captured.wire(connection.identity(), request.msg_id, request.msg_type).expect("warmup wire log"); }
        directory(id)
    })?;
    let client = client(&server, 4)?;
    log.phase("warmup")?;
    for _ in 0..128 { one(&client, total, &OperationOptions::default())?; }
    log.phase("measurement")?;
    let operations = (|| -> Case {
        let mut jobs = Vec::new();
        for worker in 0..workers {
            let client = client.clone(); let log = log.clone();
            jobs.push(thread::spawn(move || -> Case {
                for id in (worker..total).step_by(workers) {
                    let started = Instant::now();
                    if id % 5 == 0 {
                        let token = CancellationToken::default(); token.cancel();
                        let options = OperationOptions::default().with_cancellation(token);
                        assert_eq!(one(&client, id, &options).expect_err("cancelled request admitted").kind(), ErrorKind::Cancelled);
                        log.terminal(id, "cancelled", started.elapsed())?;
                    } else {
                        one(&client, id, &OperationOptions::default().with_raw(
                            if id % 2 == 0 { RawPolicy::RetainWire } else { RawPolicy::Omit }))?;
                        log.terminal(id, "completed", started.elapsed())?;
                    }
                }
                Ok(())
            }));
        }
        let mut failure = None;
        for job in jobs {
            match job.join() {
                Ok(Ok(())) => {}, Ok(Err(error)) => { failure.get_or_insert(error); },
                Err(_) => { failure.get_or_insert(io::Error::other("stress caller panicked").into()); },
            }
        }
        if let Some(error) = failure { return Err(error); }
        Ok(())
    })();
    let cleanup = client.close();
    let sockets = closed(&server);
    let peers = server.finish();
    operations?; cleanup?; sockets?; peers?;
    log.finish(total)?; Ok(())
}

#[test]
#[ignore = "candidate-only: repeat cancellation of admitted requests and late reply retirement"]
fn stress_inflight_cancel_then_recovery_never_crosses_identity() -> Case {
    let cycles = count("ELTDX_RUST_STRESS_CANCEL_CYCLES", 1_000, 100, 10_000)?;
    let log = Log::start("inflight", cycles * 2, 1)?;
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let release_rx = Mutex::new(release_rx); let captured = log.clone();
    let server = Server::start_controlled(move |request, connection| {
        assert_eq!(request.msg_type, 0x044d);
        let id = hq_server::directory_offset(request) as usize;
        captured.wire_for(id, connection.identity(), request.msg_id, request.msg_type).expect("wire log");
        if id % 2 == 0 {
            entered_tx.send(id).expect("cancel controller");
            assert_eq!(release_rx.lock().expect("release mutex").recv_timeout(Duration::from_secs(10)).expect("release"), id);
        }
        directory(id)
    })?;
    let client = client(&server, 1)?;
    let operation = (|| -> Case {
        for cycle in 0..cycles {
            let id = cycle * 2;
            let token = CancellationToken::default();
            let options = OperationOptions::default().with_cancellation(token.clone());
            let child = client.clone();
            let started = Instant::now();
            let job = thread::spawn(move || one(&child, id, &options));
            let arrival = entered_rx.recv_timeout(Duration::from_secs(5));
            token.cancel();
            let result = job.join().map_err(|_| io::Error::other("cancelled caller panicked"));
            // Always release the source, including cleanup errors. Its late
            // result must never become the next call's different identity.
            let released = release_tx.send(id);
            assert_eq!(arrival?, id);
            released?;
            assert_eq!(result?.expect_err("cancelled call succeeded").kind(), ErrorKind::Cancelled);
            log.terminal(id, "cancelled", started.elapsed())?;
            let started = Instant::now();
            one(&client, id + 1, &OperationOptions::default())?;
            log.terminal(id + 1, "completed", started.elapsed())?;
        }
        Ok(())
    })();
    let cleanup = client.close(); let sockets = closed(&server); let peers = server.finish();
    operation?; cleanup?; sockets?; peers?; log.finish(cycles * 2)?; Ok(())
}

#[test]
#[ignore = "candidate-only: platform close cycles and unique generations"]
fn stress_connected_owner_cycles_retire_all_sockets_and_service_handles() -> Case {
    let default = if cfg!(windows) { 2_000 } else { 10_000 };
    let cycles = count("ELTDX_RUST_STRESS_CLOSE_CYCLES", default, 100, 20_000)?;
    let log = Log::start("close", cycles, 1)?;
    let captured = log.clone();
    let server = Server::start_controlled(move |request, connection| {
        let id = hq_server::directory_offset(request) as usize;
        captured.wire_for(id, connection.identity(), request.msg_id, request.msg_type).expect("wire log");
        directory(id)
    })?;
    for id in 0..cycles {
        let started = Instant::now(); let client = client(&server, 1)?;
        let codes = client.codes(); let quotes = client.quotes();
        one(&client, id, &OperationOptions::default())?;
        let mut pin = client.pin(&OperationOptions::default())?;
        pin.close()?;
        client.clone().close()?; client.close()?;
        assert_eq!(codes.list(HqMarket::Shanghai, &CodeQuery::new(), &OperationOptions::default()).expect_err("old handle reopened").kind(), ErrorKind::Closed);
        assert_eq!(quotes.snapshots(&[], &OperationOptions::default()).expect_err("old quotes reopened").kind(), ErrorKind::Closed);
        closed(&server)?;
        log.terminal(id, "completed", started.elapsed())?;
    }
    assert_eq!(server.connection_count(), cycles);
    server.finish()?; log.finish(cycles)?; Ok(())
}

#[test]
#[ignore = "candidate-only: saturated subscription queues, paused reader and pin turnover"]
fn stress_subscription_overflow_and_release() -> Case {
    let cycles = count("ELTDX_RUST_STRESS_SUBSCRIPTIONS", 1_000, 100, 10_000)?;
    let log = Log::start("subscription", cycles, 1)?;
    let (source_tx, source_rx) = mpsc::channel();
    let server = Server::start_controlled(move |request, connection| match request.msg_type {
        0x044d => if hq_server::directory_offset(request) == 0 {
            hq_server::directory(&[("600000", 2), ("600001", 2), ("600002", 2)])
        } else { hq_server::directory(&[]) },
        0x054c => hq_server::snapshots(&[("600000", 1000), ("600001", 1000), ("600002", 1000)]),
        0x0547 => { source_tx.send(connection.clone()).expect("subscription source"); hq_server::refresh(&[]) },
        _ => panic!("unexpected stress subscription command"),
    })?;
    let client = client(&server, 1)?;
    let ids = ["sh600000", "sh600001", "sh600002"].iter().map(|value| value.parse()).collect::<eltdx::Result<Vec<_>>>()?;
    let options = OperationOptions::default();
    for id in 0..cycles {
        let started = Instant::now();
        // A successful close waits for its source work to stop; discard any
        // earlier activation notification before acquiring the next lease.
        while source_rx.try_recv().is_ok() {}
        let policy = SubscriptionOptions::default().with_renew_interval(None).with_queue_limits(2, 8 * 1024 * 1024);
        let mut subscription = client.quotes().subscribe(&ids, &policy, &options)?;
        assert!(matches!(subscription.next_event(&options)?, Some(PushEvent::Initial(_))));
        let source = source_rx.recv_timeout(Duration::from_secs(5))?;
        source.push(0x0547, &hq_server::refresh(&[("600000", 93001), ("600001", 93002), ("600002", 93003)]))?;
        assert!(matches!(subscription.next_event(&options)?, Some(PushEvent::Gap(gap)) if gap.dropped() == 1));
        for expected in [93002, 93003] {
            assert!(matches!(subscription.next_event(&options)?, Some(PushEvent::Update(row)) if row.cursor() == expected && row.desynchronized()));
        }
        subscription.close()?;
        let mut pin = client.pin(&options)?; pin.close()?;
        log.terminal(id, "completed", started.elapsed())?;
    }
    client.close()?; closed(&server)?; server.finish()?; log.finish(cycles)?; Ok(())
}

#[test]
#[ignore = "candidate-only: malformed replies and bounded collectors/downloads"]
fn stress_protocol_errors_and_aggregate_limits() -> Case {
    let cycles = count("ELTDX_RUST_STRESS_LIMIT_CYCLES", 1_000, 100, 10_000)?;
    let log = Log::start("limits", cycles, 1)?;
    let server = Server::start(|request| match request.msg_type {
        0x044d => directory(hq_server::directory_offset(request) as usize),
        0x06b9 => { let mut body = 30_000_u32.to_le_bytes().to_vec(); body.extend_from_slice(&vec![0; 30_000]); body },
        0x053e => vec![0xff],
        _ => panic!("unexpected resource stress command"),
    })?;
    let client = client(&server, 1)?;
    for id in 0..cycles {
        let started = Instant::now();
        let query = CodeScanQuery::new().with_pages(PageOptions::new(1, 4, 4)?)?;
        assert_eq!(client.codes().all(HqMarket::Shanghai, &query, &OperationOptions::default())
            .expect_err("infinite directory was reported complete").kind(), ErrorKind::LimitExceeded);
        let security = eltdx::SecurityId::new(HqMarket::Shanghai, "600000")?;
        assert_eq!(client.company_info().category(&security, &OperationOptions::default())
            .expect_err("persistent malformed category accepted").kind(), ErrorKind::Protocol);
        let data = client.resources().download_file("bounded", 30_000, &OperationOptions::default())?;
        assert_eq!(data.data.len(), 30_000);
        assert_eq!(data.completion, eltdx::DownloadCompletion::LimitReached);
        log.terminal(id, "completed", started.elapsed())?;
    }
    client.close()?; closed(&server)?; server.finish()?; log.finish(cycles)?; Ok(())
}

#[test]
#[ignore = "candidate-only: forced disconnection, finite retry and identity verification"]
fn stress_disconnect_retries_preserve_operation_identity() -> Case {
    use std::collections::HashSet;
    let cycles = count("ELTDX_RUST_STRESS_RETRY_CYCLES", 1_000, 100, 10_000)?;
    let log = Log::start("retry", cycles, 1)?;
    let seen = Mutex::new(HashSet::new()); let captured = log.clone();
    let server = Server::start_controlled(move |request, connection| {
        assert_eq!(request.msg_type, 0x044d);
        let id = hq_server::directory_offset(request) as usize;
        captured.wire_for(id, connection.identity(), request.msg_id, request.msg_type).expect("wire log");
        if seen.lock().expect("retry fixture mutex").insert(id) {
            connection.disconnect().expect("forced disconnect");
        }
        directory(id)
    })?;
    let client = client(&server, 1)?;
    let operation = (|| -> Case {
        for id in 0..cycles {
            let started = Instant::now();
            // The default finite wire-attempt policy must recover within the
            // same operation, without projecting a previous operation's row.
            one(&client, id, &OperationOptions::default())?;
            log.terminal(id, "completed", started.elapsed())?;
        }
        Ok(())
    })();
    let cleanup = client.close(); let sockets = closed(&server); let peers = server.finish();
    operation?; cleanup?; sockets?; peers?; log.finish(cycles)?; Ok(())
}

#[cfg(feature = "offline")]
#[test]
#[ignore = "candidate-only: actual financial DAT/ZIP decode, budgets and cancellation"]
fn stress_financial_zip_limits_and_cancel_release() -> Case {
    use eltdx::offline::financial::{self, FinancialParseOptions, FinancialReadOptions};
    use std::io::{Cursor, Write};
    let cycles = count("ELTDX_RUST_STRESS_FINANCIAL_CYCLES", 1_000, 100, 10_000)?;
    let log = Log::start("financial", cycles, 1)?;
    let rows = 256_u16; let fields = 64_u32;
    let mut dat = (-7_i16).to_le_bytes().to_vec();
    dat.extend_from_slice(&20240331_u32.to_le_bytes()); dat.extend_from_slice(&rows.to_le_bytes());
    dat.extend_from_slice(&0_u32.to_le_bytes()); dat.extend_from_slice(&(fields * 4).to_le_bytes());
    dat.extend_from_slice(&0_u32.to_le_bytes());
    for index in 0..u32::from(rows) {
        dat.extend_from_slice(format!("{:06}", 600_000 + index).as_bytes()); dat.push(1);
        dat.extend_from_slice(&(20 + u32::from(rows) * 11 + index * fields * 4).to_le_bytes());
    }
    for _ in 0..u32::from(rows) * fields { dat.extend_from_slice(&1_f32.to_bits().to_le_bytes()); }
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("report.dat", zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored))?;
    zip.write_all(&dat)?; let archive = zip.finish()?.into_inner();
    log.write(format!("\"kind\":\"configuration\",\"dat_bytes\":{},\"archive_bytes\":{},\"rows\":{rows},\"fields_per_row\":{fields}", dat.len(), archive.len()))?;
    for id in 0..cycles {
        let started = Instant::now();
        let token = CancellationToken::default();
        if id % 4 == 0 { token.cancel(); }
        let parse = FinancialParseOptions::new().with_cancel(Some(token)).with_raw(
            if id % 2 == 0 { RawPolicy::Omit } else { RawPolicy::RetainPayload });
        let options = FinancialReadOptions::new().with_parse(parse);
        if id % 4 == 0 {
            assert_eq!(financial::parse_zip_detailed(&archive, &options).expect_err("cancelled ZIP accepted").kind(), ErrorKind::Cancelled);
        } else {
            let result = financial::parse_zip_detailed(&archive, &options)?;
            assert_eq!(result.rows().len(), usize::from(rows));
            let byte_limit = FinancialReadOptions::new().with_parse(FinancialParseOptions::new().with_max_dat_bytes(dat.len() as u64 - 1)?);
            assert_eq!(financial::parse_zip_detailed(&archive, &byte_limit).expect_err("inflation cap ignored").kind(), ErrorKind::LimitExceeded);
            let field_limit = FinancialReadOptions::new().with_parse(FinancialParseOptions::new().with_max_total_fields(u64::from(rows) * u64::from(fields) - 1)?);
            assert_eq!(financial::parse_zip_detailed(&archive, &field_limit).expect_err("field allocation cap ignored").kind(), ErrorKind::ResourceFormat);
        }
        log.terminal(id, "completed", started.elapsed())?;
    }
    log.finish(cycles)?; Ok(())
}

#[cfg(feature = "f10")]
#[test]
#[ignore = "candidate-only: repeated actual F10 HTTP workers, retained handles and explicit close"]
fn stress_f10_connected_owner_worker_lifecycle() -> Case {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use eltdx::f10::{CallQuery, EntryName, F10Client, JsonValue, RequestPayload};
    let cycles = count("ELTDX_RUST_STRESS_F10_CYCLES", 1_000, 100, 10_000)?;
    let log = Log::start("f10", cycles, 1)?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let url = format!("http://{}/TQLEX", listener.local_addr()?);
    let stopped = Arc::new(AtomicBool::new(false)); let stop = Arc::clone(&stopped);
    let completed = Arc::new(AtomicUsize::new(0)); let captured = Arc::clone(&completed);
    let peer = thread::spawn(move || -> Case {
        while !stop.load(Ordering::Acquire) {
            let (mut stream, _) = match listener.accept() {
                Ok(pair) => pair,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::park_timeout(Duration::from_millis(1)); continue;
                }
                Err(error) => return Err(error.into()),
            };
            stream.set_read_timeout(Some(Duration::from_secs(5)))?;
            stream.set_write_timeout(Some(Duration::from_secs(5)))?;
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                let mut byte = [0]; stream.read_exact(&mut byte)?; header.push(byte[0]);
                assert!(header.len() < 16_384);
            }
            let header = String::from_utf8(header)?;
            assert!(header.starts_with("POST /TQLEX?Entry=stress.sample HTTP/1.1\r\n"));
            let size = header.lines().filter_map(|line| line.split_once(':'))
                .find(|(name, _)| name.eq_ignore_ascii_case("Content-Length"))
                .ok_or_else(|| io::Error::other("missing HTTP length"))?.1.trim().parse::<usize>()?;
            assert!(size < 1024); let mut body = vec![0; size]; stream.read_exact(&mut body)?;
            assert!(body.starts_with(b"{\"Params\":["));
            let response = br#"{"ErrorCode":0,"ResultSets":[{"ResultSetKey":"table","ColName":["value"],"Content":[[1]]}]}"#;
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", response.len())?;
            stream.write_all(response)?; stream.flush()?;
            assert_eq!(stream.read(&mut [0])?, 0, "F10 transport retained response socket");
            captured.fetch_add(1, Ordering::SeqCst);
        }
        Ok(())
    });
    let operation = (|| -> Case {
        for id in 0..cycles {
            let started = Instant::now();
            let client = F10Client::builder().with_base_url(url.clone()).build()?;
            let retained = client.clone();
            let query = CallQuery::new(EntryName::new("stress.sample")?, RequestPayload::Params(vec![JsonValue::from(id as u64)]));
            let response = client.call(&query, &OperationOptions::default().with_raw(
                if id % 2 == 0 { RawPolicy::Omit } else { RawPolicy::RetainWire }))?;
            assert!(response.ok()); assert_eq!(response.first_table().expect("F10 table").rows().len(), 1);
            client.close()?; retained.close()?;
            assert_eq!(retained.call(&query, &OperationOptions::default()).expect_err("closed HTTP owner reopened").kind(), ErrorKind::Closed);
            drop(retained); drop(client);
            log.terminal(id, "completed", started.elapsed())?;
        }
        Ok(())
    })();
    stopped.store(true, Ordering::Release);
    let peers = peer.join().map_err(|_| io::Error::other("HTTP lifecycle fixture panicked"));
    operation?; peers??;
    assert_eq!(completed.load(Ordering::SeqCst), cycles);
    log.finish(cycles)?; Ok(())
}
