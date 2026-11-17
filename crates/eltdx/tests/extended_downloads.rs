#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Authored public download contracts; execution waits for all T01-T30 sources.
//! Setup replies and file bytes are synthetic, not production protocol proof.
#![cfg(feature = "mac")]
#[path = "support/extension_peer.rs"]
mod wire;
use eltdx::mac::{DownloadFileQuery, MacClient, RemoteFileName};
use eltdx::{
    CancellationToken, DownloadCompletion, DownloadOptions, ErrorKind, OperationOptions, RawPolicy,
};
use std::io::{ErrorKind as IoKind, Write};
use std::net::{TcpListener, TcpStream};
use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use wire::{eof, frame, frame_bytes, join, peer, request, setup_mac, Case};

const PREFIX: [u8; 8] = [0x91, 0x82, 0x73, 0x64, 0x55, 0x46, 0x37, 0x28];
fn query(options: DownloadOptions) -> eltdx::Result<DownloadFileQuery> {
    DownloadFileQuery::new(RemoteFileName::new("fixture.dat")?).with_options(options)
}
fn small(expected: Option<u64>) -> eltdx::Result<DownloadOptions> {
    DownloadOptions::new()
        .with_chunk_size(NonZeroU32::new(4).unwrap())?
        .with_expected_size(expected)
}
fn meta_request(stream: &mut TcpStream) -> Case {
    let body = request(stream, 0x1c, 0x1215)?;
    assert_eq!(body.len(), 104);
    assert_eq!(&body[..4], &0_u32.to_le_bytes());
    assert_eq!(&body[4..15], b"fixture.dat");
    Ok(())
}
fn meta_bytes(size: u32) -> Vec<u8> {
    let mut body = vec![0; 41];
    body[..4].copy_from_slice(&0x7654_3210_u32.to_le_bytes());
    body[4..8].copy_from_slice(&size.to_le_bytes());
    body[8] = 0xff;
    body[9..41].copy_from_slice(b"0123456789abcdef0123456789abcdef");
    body
}
fn metadata(stream: &mut TcpStream, size: u32) -> Case {
    meta_request(stream)?;
    frame(stream, &meta_bytes(size))
}
fn chunk_request(stream: &mut TcpStream, index: u32, offset: u32, size: u32) -> Case {
    let body = request(stream, 0x1c, 0x1217)?;
    assert_eq!(body.len(), 112);
    for (actual, expected) in body[..12]
        .as_chunks::<4>()
        .0
        .iter()
        .zip([index, offset, size])
    {
        assert_eq!(u32::from_le_bytes(*actual), expected);
    }
    assert_eq!(&body[12..23], b"fixture.dat");
    Ok(())
}
fn chunk(stream: &mut TcpStream, index: u32, offset: u32, requested: u32, bytes: &[u8]) -> Case {
    chunk_request(stream, index, offset, requested)?;
    let mut body = PREFIX.to_vec();
    body.extend_from_slice(bytes);
    frame(stream, &body)
}
fn context(error: &eltdx::Error, name: &str, value: &str) -> bool {
    error
        .context()
        .iter()
        .any(|(key, item)| key == name && item == value)
}
fn diagnostic(client: &MacClient, stage: &str) -> Case<bool> {
    Ok(client
        .diagnostics()?
        .latest_meta()
        .is_some_and(|meta| meta.diagnostics().iter().any(|item| item.stage() == stage)))
}
fn no_connection(listener: &TcpListener) -> Case {
    listener.set_nonblocking(true)?;
    assert_eq!(
        listener
            .accept()
            .expect_err("no connection is permitted")
            .kind(),
        IoKind::WouldBlock
    );
    Ok(())
}

#[test]
fn metadata_size_and_short_second_chunk_return_complete_and_keep_all_raw_sources() -> Case {
    let first = vec![0xa5; 30000];
    let last = vec![0x5a; 15000];
    let first_wire = first.clone();
    let last_wire = last.clone();
    let (endpoint, task) = peer(move |mut stream| {
        setup_mac(&mut stream)?;
        metadata(&mut stream, 45000)?;
        chunk(&mut stream, 1, 0, 30000, &first_wire)?;
        chunk(&mut stream, 2, 30000, 30000, &last_wire)?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .with_raw(RawPolicy::RetainWire)
        .build()?;
    let result = client.download_file(
        &query(DownloadOptions::new())?,
        &OperationOptions::default(),
    )?;
    assert_eq!(result.completion, DownloadCompletion::Complete);
    assert_eq!(result.next_offset, None);
    assert_eq!(&result.data[..30000], first.as_slice());
    assert_eq!(&result.data[30000..], last.as_slice());
    let snapshot = client.diagnostics()?;
    let meta = snapshot.latest_meta().expect("final metadata");
    assert_eq!(meta.sources().len(), 3);
    let stamp = meta.sources()[0].session_id();
    for source in meta.sources() {
        assert_eq!(source.endpoint(), Some(endpoint));
        assert_eq!(source.session_id(), stamp);
    }
    let raw = meta.raw().expect("retained file provenance");
    assert_eq!(raw.payloads().len(), 3);
    assert_eq!(raw.wire_frames().expect("wire frames").len(), 3);
    assert_eq!(raw.row_ranges(), &[(1, 8, 30000), (2, 8, 15000)]);
    assert_eq!(&raw.payloads()[1][..8], &PREFIX);
    assert_eq!(&raw.payloads()[2][..8], &PREFIX);
    assert_eq!(&raw.payloads()[0][..], &meta_bytes(45000));
    assert!(diagnostic(&client, "FileSizeHint")?);
    assert!(diagnostic(&client, "ObservedEof")?);
    client.close()?;
    join(task)
}

#[test]
fn exact_multiple_of_chunk_size_requires_a_third_empty_reply() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        setup_mac(&mut stream)?;
        metadata(&mut stream, 60000)?;
        chunk(&mut stream, 1, 0, 30000, &vec![1; 30000])?;
        chunk(&mut stream, 2, 30000, 30000, &vec![2; 30000])?;
        chunk(&mut stream, 3, 60000, 30000, &[])?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .build()?;
    let result = client.download_complete(
        &query(DownloadOptions::new())?,
        &OperationOptions::default(),
    )?;
    assert_eq!(result.len(), 60000);
    assert!(diagnostic(&client, "ObservedEof")?);
    client.close()?;
    join(task)
}

#[test]
fn exact_size_at_byte_cap_is_limited_and_strict_convenience_rejects_it() -> Case {
    for strict in [false, true] {
        let (endpoint, task) = peer(|mut stream| {
            setup_mac(&mut stream)?;
            metadata(&mut stream, 4)?;
            chunk(&mut stream, 1, 0, 4, b"abcd")?;
            eof(&mut stream)
        })?;
        let client = MacClient::builder()
            .with_endpoints(vec![endpoint])
            .build()?;
        let query = query(small(None)?.with_max_bytes(4)?)?;
        if strict {
            let error = client
                .download_complete(&query, &OperationOptions::default())
                .expect_err("EOF not proved");
            assert_eq!(error.kind(), ErrorKind::LimitExceeded);
            assert!(context(&error, "reason", "LimitReached"));
            assert!(context(&error, "next_offset", "4"));
        } else {
            let result = client.download_file(&query, &OperationOptions::default())?;
            assert_eq!(result.completion, DownloadCompletion::LimitReached);
            assert_eq!(result.next_offset, Some(4));
            assert_eq!(&result.data[..], b"abcd");
        }
        assert!(diagnostic(&client, "LimitReached")?);
        assert!(!diagnostic(&client, "ObservedEof")?);
        client.close()?;
        join(task)?;
    }
    Ok(())
}

#[test]
fn zero_cap_performs_no_metadata_setup_or_connect_and_keeps_resume_offset() -> Case {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let client = MacClient::builder()
        .with_endpoints(vec![listener.local_addr()?])
        .build()?;
    let query = query(small(None)?.with_max_bytes(0)?.with_start_offset(17)?)?;
    let result = client.download_file(&query, &OperationOptions::default())?;
    assert!(result.data.is_empty());
    assert_eq!(result.completion, DownloadCompletion::LimitReached);
    assert_eq!(result.next_offset, Some(17));
    assert_eq!(client.host()?, None);
    no_connection(&listener)?;
    client.close()?;
    Ok(())
}

#[test]
fn strict_complete_requires_offset_zero_before_performing_any_io() -> Case {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let client = MacClient::builder()
        .with_endpoints(vec![listener.local_addr()?])
        .build()?;
    let query = query(small(Some(10))?.with_start_offset(7)?)?;
    let error = client
        .download_complete(&query, &OperationOptions::default())
        .expect_err("suffix is not a complete file");
    assert_eq!(error.kind(), ErrorKind::InvalidArgument);
    no_connection(&listener)?;
    client.close()?;
    Ok(())
}

#[test]
fn full_chunk_count_cap_is_limited_even_if_declared_size_is_already_read() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        setup_mac(&mut stream)?;
        chunk(&mut stream, 1, 0, 4, b"abcd")?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .build()?;
    let result = client.download_file(
        &query(small(Some(4))?.with_max_chunks(NonZeroU32::MIN)?)?,
        &OperationOptions::default(),
    )?;
    assert_eq!(result.completion, DownloadCompletion::LimitReached);
    assert_eq!(result.next_offset, Some(4));
    assert!(client
        .diagnostics()?
        .latest_meta()
        .expect("meta")
        .diagnostics()
        .iter()
        .any(|item| item
            .message()
            .is_some_and(|message| message.contains("max_chunks"))));
    client.close()?;
    join(task)
}

#[test]
fn observed_short_or_empty_reply_must_agree_with_expected_absolute_size() -> Case {
    for (expected, bytes) in [
        (5, b"abc".as_slice()),
        (1, b"abc".as_slice()),
        (1, b"".as_slice()),
    ] {
        let (endpoint, task) = peer(move |mut stream| {
            setup_mac(&mut stream)?;
            chunk(&mut stream, 1, 0, 4, bytes)?;
            eof(&mut stream)
        })?;
        let client = MacClient::builder()
            .with_endpoints(vec![endpoint])
            .with_raw(RawPolicy::RetainPayload)
            .build()?;
        let error = client
            .download_file(
                &query(small(Some(expected))?)?,
                &OperationOptions::default(),
            )
            .expect_err("size differs");
        assert_eq!(error.kind(), ErrorKind::Protocol);
        assert!(context(&error, "reason", "ExpectedSizeMismatch"));
        assert!(context(&error, "eof_observed", "true"));
        assert!(diagnostic(&client, "ExpectedSizeMismatch")?);
        assert_eq!(
            client
                .diagnostics()?
                .latest_meta()
                .expect("final meta")
                .raw()
                .expect("raw")
                .payloads()
                .len(),
            1
        );
        client.close()?;
        join(task)?;
    }
    Ok(())
}

#[test]
fn zero_metadata_size_still_requires_an_observed_empty_chunk() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        setup_mac(&mut stream)?;
        metadata(&mut stream, 0)?;
        chunk(&mut stream, 1, 0, 4, &[])?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .build()?;
    let result = client.download_file(&query(small(None)?)?, &OperationOptions::default())?;
    assert_eq!(result.completion, DownloadCompletion::Complete);
    assert!(result.data.is_empty());
    assert_eq!(
        client
            .diagnostics()?
            .latest_meta()
            .expect("meta")
            .sources()
            .len(),
        2
    );
    client.close()?;
    join(task)
}

#[test]
fn resume_advances_by_actual_bytes_and_reports_the_absolute_continuation() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        setup_mac(&mut stream)?;
        chunk(&mut stream, 1, 7, 4, b"hijk")?;
        chunk(&mut stream, 2, 11, 2, b"lm")?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .build()?;
    let result = client.download_file(
        &query(small(Some(20))?.with_max_bytes(6)?.with_start_offset(7)?)?,
        &OperationOptions::default(),
    )?;
    assert_eq!(&result.data[..], b"hijklm");
    assert_eq!(result.completion, DownloadCompletion::LimitReached);
    assert_eq!(result.next_offset, Some(13));
    client.close()?;
    join(task)
}

#[test]
fn resume_eof_compares_absolute_size_and_preserves_only_the_requested_suffix() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        setup_mac(&mut stream)?;
        chunk(&mut stream, 1, 7, 4, b"hij")?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .build()?;
    let result = client.download_file(
        &query(small(Some(10))?.with_start_offset(7)?)?,
        &OperationOptions::default(),
    )?;
    assert_eq!(&result.data[..], b"hij");
    assert_eq!(result.completion, DownloadCompletion::Complete);
    assert_eq!(result.next_offset, None);
    client.close()?;
    join(task)
}

#[test]
fn start_beyond_explicit_or_discovered_size_does_not_invent_an_eof_observation() -> Case {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let client = MacClient::builder()
        .with_endpoints(vec![listener.local_addr()?])
        .build()?;
    let error = client
        .download_file(
            &query(small(Some(5))?.with_start_offset(7)?)?,
            &OperationOptions::default(),
        )
        .expect_err("bad explicit start");
    assert_eq!(error.kind(), ErrorKind::InvalidArgument);
    no_connection(&listener)?;
    client.close()?;
    let (endpoint, task) = peer(|mut stream| {
        setup_mac(&mut stream)?;
        metadata(&mut stream, 5)?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .build()?;
    let error = client
        .download_file(
            &query(small(None)?.with_start_offset(7)?)?,
            &OperationOptions::default(),
        )
        .expect_err("metadata excludes start");
    assert_eq!(error.kind(), ErrorKind::Protocol);
    assert!(context(&error, "eof_observed", "false"));
    assert!(!diagnostic(&client, "ObservedEof")?);
    client.close()?;
    join(task)
}

#[test]
fn truncated_metadata_prefix_and_oversized_chunk_are_protocol_errors() -> Case {
    for mode in 0..3 {
        let (endpoint, task) = peer(move |mut stream| {
            setup_mac(&mut stream)?;
            if mode == 0 {
                meta_request(&mut stream)?;
                frame(&mut stream, &[0; 40])?;
            } else {
                chunk_request(&mut stream, 1, 0, 4)?;
                if mode == 1 {
                    frame(&mut stream, &[0; 7])?;
                } else {
                    let mut body = PREFIX.to_vec();
                    body.extend_from_slice(b"abcde");
                    frame(&mut stream, &body)?;
                }
            }
            eof(&mut stream)
        })?;
        let client = MacClient::builder()
            .with_endpoints(vec![endpoint])
            .build()?;
        let expected = if mode == 0 { None } else { Some(4) };
        let error = client
            .download_file(&query(small(expected)?)?, &OperationOptions::default())
            .expect_err("malformed file response");
        assert_eq!(error.kind(), ErrorKind::Protocol);
        client.close()?;
        join(task)?;
    }
    Ok(())
}

#[test]
fn accepted_metadata_or_chunk_pins_source_and_disallows_every_reconnect_or_fallback() -> Case {
    for use_metadata in [false, true] {
        let (first, task) = peer(move |mut stream| {
            setup_mac(&mut stream)?;
            if use_metadata {
                metadata(&mut stream, 8)?;
                chunk_request(&mut stream, 1, 0, 4)?;
            } else {
                chunk(&mut stream, 1, 0, 4, b"abcd")?;
                chunk_request(&mut stream, 2, 4, 4)?;
            }
            Ok(()) // Lose the pinned peer before a new reply.
        })?;
        let fallback = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        let client = MacClient::builder()
            .with_endpoints(vec![first, fallback.local_addr()?])
            .with_auto_failover(true)
            .with_wire_attempt_limit(NonZeroU32::new(3).unwrap())
            .with_extra_attempt_budget(3)
            .with_raw(RawPolicy::RetainWire)
            .build()?;
        let error = client
            .download_file(
                &query(small(if use_metadata { None } else { Some(8) })?)?,
                &OperationOptions::default(),
            )
            .expect_err("lost pinned source cannot be replaced");
        assert_eq!(error.kind(), ErrorKind::Connection);
        no_connection(&fallback)?;
        let snapshot = client.diagnostics()?;
        let meta = snapshot.latest_meta().expect("failed composite meta");
        assert_eq!(meta.sources().len(), 1);
        assert_eq!(meta.sources()[0].endpoint(), Some(first));
        assert_eq!(
            meta.raw()
                .expect("accepted source remains")
                .payloads()
                .len(),
            1
        );
        assert!(diagnostic(&client, "ChunkFailed")?);
        client.close()?;
        join(task)?;
    }
    Ok(())
}

#[test]
fn initial_request_may_failover_but_all_accepted_chunks_use_the_replacement_source() -> Case {
    let (first, first_task) = peer(|mut stream| {
        setup_mac(&mut stream)?;
        meta_request(&mut stream)
    })?;
    let (second, second_task) = peer(|mut stream| {
        setup_mac(&mut stream)?;
        metadata(&mut stream, 6)?;
        chunk(&mut stream, 1, 0, 4, b"abcd")?;
        chunk(&mut stream, 2, 4, 4, b"ef")?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![first, second])
        .with_raw(RawPolicy::RetainPayload)
        .with_wire_attempt_limit(NonZeroU32::new(3).unwrap())
        .with_extra_attempt_budget(2)
        .build()?;
    let result = client.download_file(&query(small(None)?)?, &OperationOptions::default())?;
    assert_eq!(&result.data[..], b"abcdef");
    assert_eq!(result.completion, DownloadCompletion::Complete);
    let snapshot = client.diagnostics()?;
    let meta = snapshot.latest_meta().expect("final sources");
    assert_eq!(meta.sources().len(), 3);
    for source in meta.sources() {
        assert_eq!(source.endpoint(), Some(second));
        assert_eq!(source.session_id(), meta.sources()[0].session_id());
    }
    client.close()?;
    join(first_task)?;
    join(second_task)
}

#[test]
fn cancellation_or_close_during_chunk_half_header_retires_the_one_registered_worker() -> Case {
    for close in [false, true] {
        let (ready, observed) = mpsc::channel();
        let (endpoint, task) = peer(move |mut stream| {
            setup_mac(&mut stream)?;
            metadata(&mut stream, 6)?;
            chunk(&mut stream, 1, 0, 4, b"abcd")?;
            chunk_request(&mut stream, 2, 4, 4)?;
            let encoded = frame_bytes(&[0; 10])?;
            stream.write_all(&encoded[..8])?;
            ready.send(())?;
            eof(&mut stream)
        })?;
        let token = CancellationToken::default();
        let client = MacClient::builder()
            .with_endpoints(vec![endpoint])
            .build()?;
        let caller = client.clone();
        let options = OperationOptions::default().with_cancellation(token.clone());
        let call = thread::spawn(move || caller.download_file(&query(small(None)?)?, &options));
        observed.recv_timeout(Duration::from_secs(2))?;
        if close {
            client.close()?;
        } else {
            token.cancel();
        }
        let error = call
            .join()
            .map_err(|_| std::io::Error::other("download fixture panicked"))?
            .expect_err("interrupted download");
        assert_eq!(
            error.kind(),
            if close {
                ErrorKind::Closed
            } else {
                ErrorKind::Cancelled
            }
        );
        assert!(diagnostic(&client, "ChunkFailed")?);
        client.close()?;
        join(task)?;
    }
    Ok(())
}

#[test]
fn all_chunks_keep_the_original_absolute_deadline() -> Case {
    let (ready, observed) = mpsc::channel();
    let (endpoint, task) = peer(move |mut stream| {
        setup_mac(&mut stream)?;
        metadata(&mut stream, 6)?;
        chunk(&mut stream, 1, 0, 4, b"abcd")?;
        chunk_request(&mut stream, 2, 4, 4)?;
        ready.send(())?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .build()?;
    let caller = client.clone();
    let deadline = Instant::now() + Duration::from_secs(2);
    let call = thread::spawn(move || {
        caller.download_file(
            &query(small(None)?)?,
            &OperationOptions::default().with_deadline(deadline),
        )
    });
    observed.recv_timeout(Duration::from_secs(2))?;
    let error = call
        .join()
        .map_err(|_| std::io::Error::other("deadline fixture panicked"))?
        .expect_err("parent deadline");
    assert_eq!(error.kind(), ErrorKind::Timeout);
    client.close()?;
    join(task)
}

#[test]
fn parent_memory_cap_rejects_output_reservation_before_a_chunk_is_sent() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        setup_mac(&mut stream)?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .with_max_operation_bytes(NonZeroUsize::new(1024).unwrap())
        .build()?;
    client.connect(&OperationOptions::default())?;
    let error = client
        .download_file(
            &query(DownloadOptions::new().with_expected_size(Some(30000))?)?,
            &OperationOptions::default(),
        )
        .expect_err("30000 bytes must be reserved before I/O");
    assert_eq!(error.kind(), ErrorKind::LimitExceeded);
    client.close()?;
    join(task)
}
