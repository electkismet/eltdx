//! Synthetic source contracts for the public HQ financial service.
//! Authored during implementation; execution waits for the whole-project gate.
#![cfg(feature = "offline")]
#[path = "support/hq_server.rs"]
mod hq_server;

use eltdx::financial::{
    open_calc_client, CalcClientConfig, DownloadOptions, FinancialArchiveName,
    FinancialReadOptions, FinancialRecordsQuery,
};
use eltdx::offline::financial::{FinancialCompletion, ReportDateSource};
use eltdx::{
    CancellationToken, DownloadCompletion, ErrorKind, OperationOptions, RawPolicy, TdxClient,
    TradingDate,
};
use eltdx_protocol::frame::RequestFrame;
use std::io::{Cursor, Write};
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;
fn dat() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0_i16.to_le_bytes());
    bytes.extend_from_slice(&20240331_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&4_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(b"600000");
    bytes.push(1);
    bytes.extend_from_slice(&31_u32.to_le_bytes());
    bytes.extend_from_slice(&0x7fc12345_u32.to_le_bytes());
    bytes
}
fn archive() -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file(
            "report.dat",
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
    writer.write_all(&dat()).unwrap();
    writer.finish().unwrap().into_inner()
}
fn range(request: &RequestFrame) -> (usize, usize) {
    (
        u32::from_le_bytes(request.data[..4].try_into().unwrap()) as usize,
        u32::from_le_bytes(request.data[4..8].try_into().unwrap()) as usize,
    )
}
fn path(request: &RequestFrame) -> &str {
    std::str::from_utf8(request.data[8..].split(|byte| *byte == 0).next().unwrap()).unwrap()
}
fn chunk(data: &[u8], offset: usize, size: usize) -> Vec<u8> {
    let body = &data[offset.min(data.len())..offset.saturating_add(size).min(data.len())];
    let mut reply = (body.len() as u32).to_le_bytes().to_vec();
    reply.extend_from_slice(body);
    reply
}
fn client(server: &hq_server::Server) -> TdxClient {
    TdxClient::builder()
        .endpoints(vec![server.address])
        .heartbeat_interval(None)
        .pool_size(1)
        .build()
        .unwrap()
}

#[test]
fn calc_construction_and_zero_download_are_lazy_and_service_shares_close() -> TestResult {
    let server =
        hq_server::Server::start(|_| panic!("zero-byte financial download must not connect"))?;
    let defaults = CalcClientConfig::new();
    assert!(!defaults.probe_hosts());
    assert_eq!(defaults.endpoint().port(), 7709);
    let config = CalcClientConfig::new()
        .with_endpoint(server.address)
        .with_builder(TdxClient::builder().heartbeat_interval(None));
    let client = open_calc_client(&config)?;
    assert_eq!(client.config().request_timeout(), Duration::from_secs(8));
    let service = client.financial();
    let name = FinancialArchiveName::new("gpcw20240630.zip")?;
    let zero = DownloadOptions::new().with_max_bytes(0)?;
    let result = service.download_archive(&name, &zero, &OperationOptions::default())?;
    assert!(result.data.is_empty());
    assert_eq!(result.completion, DownloadCompletion::LimitReached);
    assert_eq!(result.next_offset, Some(0));
    assert_eq!(server.connection_count(), 0);
    client.clone().close()?;
    assert_eq!(
        service
            .list_archive_files(&OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::Closed
    );
    assert_eq!(
        service
            .download_archive(&name, &zero, &OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::Closed
    );
    server.finish()?;
    Ok(())
}

#[test]
fn records_compose_list_hq_archive_zip_and_dat_with_source_provenance() -> TestResult {
    let zip = archive();
    let expected_zip = zip.clone();
    // Deliberately stale size/hash. Neither is trusted as an EOF/integrity oracle.
    let listing = b"gpcw20240630.zip,not-a-verified-hash,1\nnegative.zip,digest,-8\n".to_vec();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let seen = requests.clone();
    let server = hq_server::Server::start(move |request| {
        assert_eq!(request.msg_type, 0x06b9);
        let (offset, size) = range(request);
        let path = path(request);
        seen.lock().unwrap().push((path.to_owned(), offset, size));
        match path {
            "tdxfin/gpcw.txt" => chunk(&listing, offset, size),
            "tdxfin/gpcw20240630.zip" => chunk(&zip, offset, size),
            other => panic!("unexpected HQ resource {other}"),
        }
    })?;
    let client = client(&server);
    let query = FinancialRecordsQuery::new(FinancialArchiveName::new("gpcw20240630.zip")?);
    let result = client.financial().download_records_detailed(
        &query,
        &OperationOptions::default().with_raw(RawPolicy::RetainWire),
    )?;
    assert_eq!(result.completion(), FinancialCompletion::Complete);
    assert_eq!(
        result.rows()[0].report_date(),
        Some(TradingDate::new(2024, 6, 30)?)
    );
    assert_eq!(result.date_source(), ReportDateSource::ArchiveFilename);
    assert_eq!(result.rows()[0].fields()[0].bits(), 0x7fc12345);
    assert_eq!(result.archive_raw(), Some(expected_zip.as_slice()));
    let meta = result.source_meta().ok_or("missing source metadata")?;
    assert_eq!(meta.sources().len(), 2);
    assert_eq!(
        meta.raw()
            .ok_or("missing raw")?
            .wire_frames()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(requests.lock().unwrap().len(), 2);
    assert_eq!(server.connection_count(), 1);
    client.close()?;
    server.finish()?;
    Ok(())
}

#[test]
fn list_retains_unverified_hash_and_invalid_signed_size_metadata() -> TestResult {
    let server = hq_server::Server::start(|request| {
        assert_eq!(path(request), "tdxfin/gpcw.txt");
        let (offset, size) = range(request);
        chunk(
            b"a.zip,opaque,123\nb.zip,negative,-1\nc.zip,overflow,18446744073709551616\n",
            offset,
            size,
        )
    })?;
    let client = client(&server);
    let list = client
        .financial()
        .list_archive_files(&OperationOptions::default())?;
    assert_eq!(list.len(), 3);
    assert_eq!(list[0].hash(), "opaque");
    assert_eq!(list[0].filesize(), Some(123));
    assert_eq!(list[1].filesize(), None);
    assert!(!list[1].diagnostics().is_empty());
    assert_eq!(list[2].filesize(), None);
    client.close()?;
    server.finish()?;
    Ok(())
}

#[test]
fn capped_download_is_not_complete_even_when_expected_size_matches() -> TestResult {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let server = hq_server::Server::start(move |request| {
        seen.fetch_add(1, Ordering::SeqCst);
        let (offset, size) = range(request);
        chunk(b"abcdefgh", offset, size)
    })?;
    let client = client(&server);
    let service = client.financial();
    let name = FinancialArchiveName::new("report.zip")?;
    let exact = DownloadOptions::new()
        .with_max_bytes(8)?
        .with_expected_size(Some(8))?;
    let result = service.download_archive(&name, &exact, &OperationOptions::default())?;
    assert_eq!(result.completion, DownloadCompletion::LimitReached);
    assert_eq!(result.next_offset, Some(8));
    let one_chunk = DownloadOptions::new()
        .with_chunk_size(NonZeroU32::new(4).unwrap())?
        .with_max_chunks(NonZeroU32::new(1).unwrap())?;
    let result = service.download_archive(&name, &one_chunk, &OperationOptions::default())?;
    assert_eq!(&result.data[..], b"abcd");
    assert_eq!(result.completion, DownloadCompletion::LimitReached);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    // A third call proves capped operations returned their pinned connection.
    let complete =
        service.download_archive(&name, &DownloadOptions::new(), &OperationOptions::default())?;
    assert_eq!(complete.completion, DownloadCompletion::Complete);
    client.close()?;
    server.finish()?;
    Ok(())
}

#[test]
fn incomplete_archive_and_incomplete_dat_never_become_record_success() -> TestResult {
    for truncated_dat in [false, true] {
        let zip = if truncated_dat {
            let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
            writer
                .start_file("data.dat", zip::write::FileOptions::default())
                .unwrap();
            writer.write_all(&[1, 2, 3]).unwrap();
            writer.finish().unwrap().into_inner()
        } else {
            archive()
        };
        let cap = if truncated_dat {
            zip.len() + 1
        } else {
            zip.len()
        };
        let server = hq_server::Server::start(move |request| {
            let (offset, size) = range(request);
            if path(request) == "tdxfin/gpcw.txt" {
                chunk(b"", offset, size)
            } else {
                chunk(&zip, offset, size)
            }
        })?;
        let client = client(&server);
        let query = FinancialRecordsQuery::new(FinancialArchiveName::new("report.zip")?);
        let result = client.financial().download_records_with_options(
            &query,
            &FinancialReadOptions::new(),
            &DownloadOptions::new().with_max_bytes(cap as u64)?,
            &OperationOptions::default(),
        );
        assert_eq!(
            result.unwrap_err().kind(),
            if truncated_dat {
                ErrorKind::ResourceFormat
            } else {
                ErrorKind::LimitExceeded
            }
        );
        client.close()?;
        server.finish()?;
    }
    Ok(())
}

#[test]
fn invalid_cancelled_and_expired_compositions_do_not_connect() -> TestResult {
    let server = hq_server::Server::start(|_| panic!("preflight must not connect"))?;
    let client = client(&server);
    let service = client.financial();
    let name = FinancialArchiveName::new("report.zip")?;
    let query = FinancialRecordsQuery::new(name.clone());
    let token = CancellationToken::default();
    token.cancel();
    assert_eq!(
        service
            .download_records(
                &query,
                &OperationOptions::default().with_cancellation(token)
            )
            .unwrap_err()
            .kind(),
        ErrorKind::Cancelled
    );
    assert_eq!(
        service
            .list_archive_files(&OperationOptions::default().with_timeout(Duration::ZERO))
            .unwrap_err()
            .kind(),
        ErrorKind::Timeout
    );
    let range = DownloadOptions::new().with_start_offset(1)?;
    assert_eq!(
        service
            .download_archive(&name, &range, &OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidArgument
    );
    let invalid = DownloadOptions::new().with_max_bytes(65 * 1024 * 1024)?;
    assert_eq!(
        service
            .download_archive(&name, &invalid, &OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidArgument
    );
    assert_eq!(
        service
            .download_records_with_options(
                &query,
                &FinancialReadOptions::new(),
                &DownloadOptions::new().with_max_bytes(0)?,
                &OperationOptions::default()
            )
            .unwrap_err()
            .kind(),
        ErrorKind::LimitExceeded
    );
    assert_eq!(server.connection_count(), 0);
    client.close()?;
    server.finish()?;
    Ok(())
}

#[test]
fn cancellation_after_list_does_not_start_the_archive_download() -> TestResult {
    let token = CancellationToken::default();
    let stop = token.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let server = hq_server::Server::start(move |request| {
        seen.fetch_add(1, Ordering::SeqCst);
        assert_eq!(path(request), "tdxfin/gpcw.txt");
        stop.cancel();
        let (offset, size) = range(request);
        chunk(b"report.zip,hash,20", offset, size)
    })?;
    let client = client(&server);
    let query = FinancialRecordsQuery::new(FinancialArchiveName::new("report.zip")?);
    let result = client.financial().download_records(
        &query,
        &OperationOptions::default().with_cancellation(token),
    );
    assert_eq!(result.unwrap_err().kind(), ErrorKind::Cancelled);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    client.close()?;
    server.finish()?;
    Ok(())
}

#[test]
fn expired_total_deadline_after_list_is_not_restarted_for_archive() -> TestResult {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let server = hq_server::Server::start(move |request| {
        seen.fetch_add(1, Ordering::SeqCst);
        assert_eq!(path(request), "tdxfin/gpcw.txt");
        std::thread::sleep(Duration::from_millis(100));
        let (offset, size) = range(request);
        chunk(b"report.zip,hash,20", offset, size)
    })?;
    let client = client(&server);
    let query = FinancialRecordsQuery::new(FinancialArchiveName::new("report.zip")?);
    let result = client.financial().download_records(
        &query,
        &OperationOptions::default().with_timeout(Duration::from_millis(20)),
    );
    assert_eq!(result.unwrap_err().kind(), ErrorKind::Timeout);
    assert!(calls.load(Ordering::SeqCst) <= 1);
    client.close()?;
    server.finish()?;
    Ok(())
}
