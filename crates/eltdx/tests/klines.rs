#[path = "support/hq_server.rs"]
mod hq_server;

use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eltdx::types::{AdjustMode, BarTime, DelistedQuery, KlineKind, KlinePeriod, KlineQuery};
use eltdx::{
    CancellationToken, EndEvidence, ErrorKind, OperationOptions, PageOptions, RawPolicy,
    SecurityId, TdxClient,
};
use eltdx_protocol::frame::RequestFrame;
use hq_server::{directory, directory_offset, Server};

fn id(value: &str) -> SecurityId {
    value.parse().expect("valid security id")
}

fn catalog(request: &RequestFrame) -> Vec<u8> {
    assert_eq!(request.msg_type, 0x044d);
    if directory_offset(request) == 0 {
        directory(&[("600000", 2)])
    } else {
        directory(&[])
    }
}

fn kline_payload(request: &RequestFrame, dates: &[(u32, u16, u16)]) -> Vec<u8> {
    let mut payload = u16::try_from(dates.len())
        .expect("bounded fixture count")
        .to_le_bytes()
        .to_vec();
    let period = u16::from_le_bytes(request.data[8..10].try_into().expect("period field"));
    let index = &request.data[2..8] == b"000001";
    for &(yyyymmdd, hour, minute) in dates {
        if matches!(period, 0 | 1 | 2 | 3 | 7 | 8) {
            let year = yyyymmdd / 10_000;
            let month = (yyyymmdd / 100) % 100;
            let day = yyyymmdd % 100;
            let packed = ((year - 2004) << 11) | (month * 100 + day);
            payload.extend_from_slice(&u16::try_from(packed).expect("packed date").to_le_bytes());
            payload.extend_from_slice(&(hour * 60 + minute).to_le_bytes());
        } else {
            payload.extend_from_slice(&yyyymmdd.to_le_bytes());
        }
        payload.extend_from_slice(&[0, 0, 0, 0]);
        payload.extend_from_slice(&[0; 8]);
        if index {
            payload.extend_from_slice(&[0; 4]);
        }
    }
    payload
}

fn delisted_payload(rows: &[(u32, u8)]) -> Vec<u8> {
    let mut payload = u16::try_from(rows.len())
        .expect("bounded fixture count")
        .to_le_bytes()
        .to_vec();
    for &(date, volume) in rows {
        payload.extend_from_slice(&date.to_le_bytes());
        payload.extend_from_slice(&[0, 0, 0, 0]);
        payload.push(volume);
        payload.extend_from_slice(&[0; 4]);
        payload.extend_from_slice(&[volume, 0, 0, 0]);
    }
    payload
}

fn request_offset(request: &RequestFrame, delisted: bool) -> u16 {
    let start = if delisted { 8 } else { 12 };
    u16::from_le_bytes(
        request.data[start..start + 2]
            .try_into()
            .expect("start offset"),
    )
}

fn request_count(request: &RequestFrame) -> u16 {
    u16::from_le_bytes(request.data[14..16].try_into().expect("count field"))
}

#[test]
fn collect_continues_after_short_pages_and_reverses_only_page_order(
) -> Result<(), Box<dyn std::error::Error>> {
    let offsets = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&offsets);
    let server = Server::start(move |request| match request.msg_type {
        0x044d => catalog(request),
        0x052d => {
            let offset = request_offset(request, false);
            observed
                .lock()
                .unwrap()
                .push((offset, request_count(request)));
            match offset {
                0 => kline_payload(request, &[(2026_0813, 9, 40), (2026_0813, 9, 45)]),
                2 => kline_payload(request, &[(2026_0813, 9, 30), (2026_0813, 9, 35)]),
                4 => kline_payload(request, &[]),
                _ => panic!("unexpected K-line offset {offset}"),
            }
        }
        _ => panic!("unexpected command 0x{:04x}", request.msg_type),
    })?;
    let client = TdxClient::builder()
        .endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()?;
    let query = KlineQuery::new()
        .with_period(KlinePeriod::Min5)?
        .with_pages(PageOptions::new(3, 4, 20)?)?;
    let result = client.klines().collect(
        &id("sh600000"),
        &query,
        &OperationOptions::default().with_raw(RawPolicy::RetainPayload),
    )?;

    let times = result
        .bars()
        .items()
        .iter()
        .map(|bar| bar.time().clock().expect("minute K-line time"))
        .collect::<Vec<_>>();
    assert_eq!(times, vec![(9, 30, 0), (9, 35, 0), (9, 40, 0), (9, 45, 0)]);
    assert_eq!(*offsets.lock().unwrap(), vec![(0, 3), (2, 3), (4, 3)]);
    assert_eq!(result.pages_read(), 3);
    assert_eq!(result.bars().end(), EndEvidence::EmptyPage);
    let raw = result.meta().raw().expect("retained page payloads");
    assert_eq!(raw.payloads().len(), 3);
    assert_eq!(raw.row_ranges().len(), 4);
    assert_eq!(
        raw.row_ranges()
            .iter()
            .map(|&(page, _, _)| page)
            .collect::<Vec<_>>(),
        vec![1, 1, 0, 0]
    );
    client.close()?;
    server.finish()?;
    Ok(())
}

#[test]
fn cursor_collection_starts_at_the_cursor_and_returns_only_remaining_rows(
) -> Result<(), Box<dyn std::error::Error>> {
    let offsets = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&offsets);
    let server = Server::start(move |request| match request.msg_type {
        0x044d => catalog(request),
        0x052d => {
            let offset = request_offset(request, false);
            observed.lock().unwrap().push(offset);
            match offset {
                0 => kline_payload(request, &[(2026_0813, 15, 0)]),
                1 => kline_payload(request, &[(2026_0812, 15, 0)]),
                2 => kline_payload(request, &[]),
                _ => panic!("unexpected K-line offset {offset}"),
            }
        }
        _ => panic!("unexpected command 0x{:04x}", request.msg_type),
    })?;
    let client = TdxClient::builder()
        .endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()?;
    let query = KlineQuery::new()
        .with_count(3)?
        .with_pages(PageOptions::new(3, 3, 10)?)?;
    let first = client
        .klines()
        .page(&id("sh600000"), &query, &OperationOptions::default())?;
    let cursor = first
        .bars()
        .next_cursor()
        .expect("short nonempty page has continuation");
    let remaining = client.klines().collect(
        &id("sh600000"),
        &query.with_cursor(cursor)?,
        &OperationOptions::default(),
    )?;

    assert_eq!(remaining.bars().start(), 1);
    assert_eq!(remaining.bars().items().len(), 1);
    assert_eq!(remaining.pages_read(), 2);
    assert_eq!(*offsets.lock().unwrap(), vec![0, 1, 2]);
    client.close()?;
    server.finish()?;
    Ok(())
}

#[test]
fn ordinary_collect_rejects_a_repeated_wire_page() -> Result<(), Box<dyn std::error::Error>> {
    let offsets = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&offsets);
    let server = Server::start(move |request| match request.msg_type {
        0x044d => catalog(request),
        0x052d => {
            observed
                .lock()
                .unwrap()
                .push(request_offset(request, false));
            kline_payload(request, &[(2026_0813, 9, 30)])
        }
        _ => panic!("unexpected command 0x{:04x}", request.msg_type),
    })?;
    let client = TdxClient::builder()
        .endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()?;
    let query = KlineQuery::new().with_pages(PageOptions::new(1, 3, 3)?)?;
    let error = client
        .klines()
        .collect(&id("sh600000"), &query, &OperationOptions::default())
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Protocol);
    assert_eq!(*offsets.lock().unwrap(), vec![0, 1]);
    client.close()?;
    server.finish()?;
    Ok(())
}

#[test]
fn ordinary_collect_rejects_offset_overflow_before_requesting_an_unrepresentable_page(
) -> Result<(), Box<dyn std::error::Error>> {
    let offsets = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&offsets);
    let server = Server::start(move |request| match request.msg_type {
        0x044d => catalog(request),
        0x052d => {
            observed
                .lock()
                .unwrap()
                .push(request_offset(request, false));
            kline_payload(request, &[(2026_0813, 9, 30)])
        }
        _ => panic!("unexpected command 0x{:04x}", request.msg_type),
    })?;
    let client = TdxClient::builder()
        .endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()?;
    let query = KlineQuery::new()
        .with_start(u16::MAX.into())?
        .with_pages(PageOptions::new(1, 3, 3)?)?;
    let error = client
        .klines()
        .collect(&id("sh600000"), &query, &OperationOptions::default())
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::LimitExceeded);
    assert_eq!(*offsets.lock().unwrap(), vec![u16::MAX]);
    client.close()?;
    server.finish()?;
    Ok(())
}

#[test]
fn expired_operation_deadline_rejects_collection_before_network_work(
) -> Result<(), Box<dyn std::error::Error>> {
    let commands = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&commands);
    let server = Server::start(move |request| {
        observed.lock().unwrap().push(request.msg_type);
        match request.msg_type {
            0x044d => catalog(request),
            0x052d => kline_payload(request, &[]),
            _ => panic!("unexpected command 0x{:04x}", request.msg_type),
        }
    })?;
    let client = TdxClient::builder()
        .endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()?;
    let options = OperationOptions::default().with_deadline(std::time::Instant::now());
    let error = client
        .klines()
        .collect(&id("sh600000"), &KlineQuery::new(), &options)
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Timeout);
    assert!(commands.lock().unwrap().is_empty());
    client.close()?;
    server.finish()?;
    Ok(())
}

#[test]
fn delisted_collection_stably_sorts_equal_dates_and_retains_every_page(
) -> Result<(), Box<dyn std::error::Error>> {
    let offsets = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&offsets);
    let server = Server::start(move |request| match request.msg_type {
        0x044d => catalog(request),
        0x052b => {
            let offset = request_offset(request, true);
            observed.lock().unwrap().push(offset);
            match offset {
                0 => delisted_payload(&[(2026_0814, 4), (2026_0811, 1)]),
                2 => delisted_payload(&[(2026_0812, 3), (2026_0811, 2)]),
                4 => delisted_payload(&[]),
                _ => panic!("unexpected delisted K-line offset {offset}"),
            }
        }
        _ => panic!("unexpected command 0x{:04x}", request.msg_type),
    })?;
    let client = TdxClient::builder()
        .endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()?;
    let query = DelistedQuery::new()
        .with_count(2)?
        .with_pages(PageOptions::new(2, 3, 10)?)?;
    let result = client.klines().delisted_collect(
        &id("sh600000"),
        &query,
        &OperationOptions::default().with_raw(RawPolicy::RetainPayload),
    )?;

    let bars = result.bars().items();
    assert_eq!(
        bars.iter()
            .map(|bar| bar.time().date().expect("dated delisted bar").day())
            .collect::<Vec<_>>(),
        vec![11, 11, 12, 14]
    );
    assert_eq!(
        bars.iter().map(|bar| bar.volume_word()).collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
    assert_eq!(*offsets.lock().unwrap(), vec![0, 2, 4]);
    assert_eq!(
        result
            .meta()
            .raw()
            .expect("retained page payloads")
            .payloads()
            .len(),
        3
    );
    assert_eq!(
        result
            .meta()
            .raw()
            .unwrap()
            .row_ranges()
            .iter()
            .map(|&(page, _, _)| page)
            .collect::<Vec<_>>(),
        vec![0, 1, 1, 0]
    );
    assert_eq!(bars[0].unknown_suffix(), Some([1, 0, 0, 0]));
    client.close()?;
    server.finish()?;
    Ok(())
}

#[test]
fn category_periods_keep_distinct_wire_ids_and_parameters() -> Result<(), Box<dyn std::error::Error>>
{
    let periods = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&periods);
    let server = Server::start(move |request| match request.msg_type {
        0x044d => catalog(request),
        0x052d => {
            observed.lock().unwrap().push((
                u16::from_le_bytes(request.data[8..10].try_into().expect("period")),
                u16::from_le_bytes(request.data[10..12].try_into().expect("parameter")),
            ));
            kline_payload(request, &[])
        }
        _ => panic!("unexpected command 0x{:04x}", request.msg_type),
    })?;
    let client = TdxClient::builder()
        .endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()?;
    for period in [
        KlinePeriod::Minutes(7),
        KlinePeriod::Days(2),
        KlinePeriod::Seconds(13),
        KlinePeriod::Week,
        KlinePeriod::Month,
        KlinePeriod::Quarter,
        KlinePeriod::Year,
    ] {
        let query = KlineQuery::new().with_period(period)?;
        client
            .klines()
            .page(&id("sh600000"), &query, &OperationOptions::default())?;
    }
    assert_eq!(
        *periods.lock().unwrap(),
        vec![(8, 7), (9, 2), (13, 13), (5, 1), (6, 1), (10, 1), (11, 1),]
    );
    client.close()?;
    server.finish()?;
    Ok(())
}

#[test]
fn automatic_index_layout_retains_breadth_counts() -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::start(|request| match request.msg_type {
        0x044d => catalog(request),
        0x052d => {
            let mut payload = kline_payload(request, &[(2026_0813, 15, 0)]);
            let end = payload.len();
            payload[end - 4..end - 2].copy_from_slice(&17_u16.to_le_bytes());
            payload[end - 2..end].copy_from_slice(&23_u16.to_le_bytes());
            payload
        }
        _ => panic!("unexpected command 0x{:04x}", request.msg_type),
    })?;
    let client = TdxClient::builder()
        .endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()?;
    let result = client.klines().page(
        &id("sh000001"),
        &KlineQuery::new(),
        &OperationOptions::default(),
    )?;
    assert_eq!(result.kind(), KlineKind::Index);
    assert_eq!(result.bars().items()[0].index_up(), Some(17));
    assert_eq!(result.bars().items()[0].index_down(), Some(23));
    client.close()?;
    server.finish()?;
    Ok(())
}

#[test]
fn end_labeled_five_minute_bar_rolls_over_leap_day() -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::start(|request| match request.msg_type {
        0x044d => catalog(request),
        0x052d => kline_payload(request, &[(2024_0229, 23, 58)]),
        _ => panic!("unexpected command 0x{:04x}", request.msg_type),
    })?;
    let client = TdxClient::builder()
        .endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()?;
    let query = KlineQuery::new()
        .with_count(1)?
        .with_period(KlinePeriod::Minutes(5))?
        .with_bar_time(BarTime::End)?;
    let result = client
        .klines()
        .page(&id("sh600000"), &query, &OperationOptions::default())?;
    let time = result.bars().items()[0].time();
    assert_eq!(
        time.date().expect("dated time"),
        eltdx::TradingDate::new(2024, 3, 1)?
    );
    assert_eq!(time.clock(), Some((0, 3, 0)));
    let original = result.bars().items()[0].source_time();
    assert_eq!(original.date(), Some(eltdx::TradingDate::new(2024, 2, 29)?));
    assert_eq!(original.clock(), Some((23, 58, 0)));
    client.close()?;
    server.finish()?;
    Ok(())
}

#[test]
fn response_count_and_collection_limits_fail_without_truncating_success(
) -> Result<(), Box<dyn std::error::Error>> {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&calls);
    let server = Server::start(move |request| match request.msg_type {
        0x044d => catalog(request),
        0x052d => {
            let offset = request_offset(request, false);
            let mut calls = observed.lock().unwrap();
            calls.push(offset);
            let call_index = calls.len();
            drop(calls);
            match call_index {
                1 => kline_payload(request, &[(2026_0814, 15, 0), (2026_0813, 15, 0)]),
                _ => kline_payload(request, &[(2026_0812 - u32::from(offset), 15, 0)]),
            }
        }
        _ => panic!("unexpected command 0x{:04x}", request.msg_type),
    })?;
    let client = TdxClient::builder()
        .endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()?;
    let count_error = client
        .klines()
        .page(
            &id("sh600000"),
            &KlineQuery::new().with_count(1)?,
            &OperationOptions::default(),
        )
        .unwrap_err();
    assert_eq!(count_error.kind(), ErrorKind::Protocol);
    let limit_query = KlineQuery::new().with_pages(PageOptions::new(1, 2, 20)?)?;
    let limit_error = client
        .klines()
        .collect(&id("sh600000"), &limit_query, &OperationOptions::default())
        .unwrap_err();
    assert_eq!(limit_error.kind(), ErrorKind::LimitExceeded);
    assert_eq!(*calls.lock().unwrap(), vec![0, 0, 1]);
    client.close()?;
    server.finish()?;
    Ok(())
}

#[test]
fn cancelled_second_page_does_not_return_a_partial_collection(
) -> Result<(), Box<dyn std::error::Error>> {
    let token = CancellationToken::default();
    let cancel = token.clone();
    let offsets = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&offsets);
    let server = Server::start(move |request| match request.msg_type {
        0x044d => catalog(request),
        0x052d => {
            let offset = request_offset(request, false);
            observed.lock().unwrap().push(offset);
            if offset == 0 {
                kline_payload(request, &[(2026_0814, 15, 0)])
            } else {
                cancel.cancel();
                kline_payload(request, &[(2026_0813, 15, 0)])
            }
        }
        _ => panic!("unexpected command 0x{:04x}", request.msg_type),
    })?;
    let client = TdxClient::builder()
        .endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()?;
    let query = KlineQuery::new().with_pages(PageOptions::new(1, 4, 10)?)?;
    let error = client
        .klines()
        .collect(
            &id("sh600000"),
            &query,
            &OperationOptions::default()
                .with_cancellation(token)
                .with_timeout(Duration::from_secs(3)),
        )
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Cancelled);
    assert_eq!(*offsets.lock().unwrap(), vec![0, 1]);
    client.close()?;
    server.finish()?;
    Ok(())
}

#[test]
fn many_rejects_a_single_security_cursor() -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::start(|request| match request.msg_type {
        0x044d => catalog(request),
        0x052d => kline_payload(request, &[(2026_0814, 15, 0)]),
        _ => panic!("unexpected command 0x{:04x}", request.msg_type),
    })?;
    let client = TdxClient::builder()
        .endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()?;
    let query = KlineQuery::new()
        .with_count(1)?
        .with_pages(PageOptions::new(1, 2, 4)?)?;
    let first = client
        .klines()
        .page(&id("sh600000"), &query, &OperationOptions::default())?;
    let cursor = first
        .bars()
        .next_cursor()
        .expect("one row page has a continuation");
    let query = query.with_cursor(cursor)?;
    let error = client
        .klines()
        .page_many(&[id("sh600000")], &query, &OperationOptions::default())
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidArgument);
    client.close()?;
    server.finish()?;
    Ok(())
}

struct MutableClock(Arc<AtomicI64>);

impl eltdx::Clock for MutableClock {
    fn unix_seconds(&self) -> eltdx::Result<i64> {
        Ok(self.0.load(Ordering::SeqCst))
    }
}

#[test]
fn continuation_rejects_foreign_owner_identity_shape_day_and_metadata_before_send(
) -> Result<(), Box<dyn std::error::Error>> {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    let now = Arc::new(AtomicI64::new(0));
    let server = Server::start(move |request| match request.msg_type {
        0x044d => catalog(request),
        0x052d => {
            observed.fetch_add(1, Ordering::SeqCst);
            kline_payload(request, &[(2026_0813, 9, 30)])
        }
        _ => panic!("unexpected command 0x{:04x}", request.msg_type),
    })?;
    let client = TdxClient::builder()
        .clock(Arc::new(MutableClock(Arc::clone(&now))))
        .endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()?;
    let other = TdxClient::builder()
        .endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()?;
    let query = KlineQuery::new()
        .with_count(1)?
        .with_pages(PageOptions::new(1, 4, 4)?)?;
    let first = client
        .klines()
        .page(&id("sh600000"), &query, &OperationOptions::default())?;
    let query = query.with_cursor(first.bars().next_cursor().expect("continuation"))?;

    assert_eq!(
        other
            .klines()
            .page(&id("sh600000"), &query, &OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidArgument
    );
    assert_eq!(
        client
            .klines()
            .page(&id("sh600001"), &query, &OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidArgument
    );
    assert_eq!(
        client
            .klines()
            .page(
                &id("sh600000"),
                &query.clone().with_period(KlinePeriod::Min5)?,
                &OperationOptions::default()
            )
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidArgument
    );
    assert_eq!(
        client
            .klines()
            .collect(
                &id("sh600000"),
                &query.clone().with_pages(PageOptions::new(2, 4, 8)?)?,
                &OperationOptions::default()
            )
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidArgument
    );

    now.store(86_400, Ordering::SeqCst);
    assert_eq!(
        client
            .klines()
            .collect(&id("sh600000"), &query, &OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidArgument
    );
    now.store(0, Ordering::SeqCst);
    client.codes().all(
        eltdx::HqMarket::Shanghai,
        &eltdx::CodeScanQuery::default(),
        &OperationOptions::default(),
    )?;
    assert_eq!(
        client
            .klines()
            .collect(&id("sh600000"), &query, &OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidArgument
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "invalid continuations submit no K-line request"
    );

    client.klines().page(
        &id("sh600000"),
        &query.with_start(0)?,
        &OperationOptions::default(),
    )?;
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "explicit start clears the token"
    );
    client.close()?;
    other.close()?;
    server.finish()?;
    Ok(())
}

#[test]
fn repeated_times_in_distinct_pages_are_kept_and_many_preserves_occurrences(
) -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::start(|request| match request.msg_type {
        0x052d => match request_offset(request, false) {
            offset @ 0..=1 => {
                let mut payload = kline_payload(request, &[(2026_0813, 9, 30)]);
                payload[10..14].copy_from_slice(&u32::from(offset + 1).to_le_bytes());
                payload
            }
            2 => kline_payload(request, &[]),
            offset => panic!("unexpected offset {offset}"),
        },
        _ => panic!("unexpected command 0x{:04x}", request.msg_type),
    })?;
    let client = TdxClient::builder()
        .endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()?;
    let query = KlineQuery::new()
        .with_count(1)?
        .with_pages(PageOptions::new(1, 3, 4)?)?;
    let series = client
        .klines()
        .collect(&id("sh600000"), &query, &OperationOptions::default())?;
    assert_eq!(series.bars().items().len(), 2);
    assert_eq!(
        series.bars().items()[0].time(),
        series.bars().items()[1].time()
    );
    assert_eq!(
        series
            .bars()
            .items()
            .iter()
            .map(|bar| bar.volume_word())
            .collect::<Vec<_>>(),
        vec![2, 1]
    );
    assert!(series.meta().raw().is_none());

    let ids = vec![id("sh600001"), id("sh600000"), id("sh600001")];
    let batch = client
        .klines()
        .page_many(&ids, &query, &OperationOptions::default())?;
    assert_eq!(batch.len(), 3);
    for (index, item) in batch.items().iter().enumerate() {
        assert_eq!(item.input_index(), index);
        assert_eq!(item.security(), &ids[index]);
        assert!(
            matches!(item.state(), eltdx::BatchItemState::Present(series) if series.id() == &ids[index])
        );
    }
    client.close()?;
    server.finish()?;
    Ok(())
}

#[test]
fn invalid_wire_dates_truncated_records_and_trailing_bytes_are_protocol_errors(
) -> Result<(), Box<dyn std::error::Error>> {
    for case in 0..3 {
        let server = Server::start(move |request| match request.msg_type {
            0x052d => match case {
                0 => kline_payload(request, &[(2026_0230, 0, 0)]),
                1 => vec![1, 0, 0],
                _ => {
                    let mut payload = kline_payload(request, &[]);
                    payload.push(0x7f);
                    payload
                }
            },
            _ => panic!("unexpected command 0x{:04x}", request.msg_type),
        })?;
        let client = TdxClient::builder()
            .endpoints(vec![server.address])
            .heartbeat_interval(None)
            .build()?;
        let error = client
            .klines()
            .page(
                &id("sh600000"),
                &KlineQuery::new(),
                &OperationOptions::default(),
            )
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Protocol);
        client.close()?;
        server.finish()?;
    }
    Ok(())
}

#[test]
fn query_setters_reject_invalid_combinations_and_empty_many_does_not_fetch(
) -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        KlineQuery::new()
            .with_period(KlinePeriod::Minutes(0))
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidArgument
    );
    assert_eq!(
        KlineQuery::new().with_count(0).unwrap_err().kind(),
        ErrorKind::InvalidArgument
    );
    assert_eq!(
        KlineQuery::new()
            .with_pages(PageOptions::new(801, 2, 1000)?)
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidArgument
    );
    let anchor = eltdx::TradingDate::new(2026, 8, 13)?;
    assert_eq!(
        KlineQuery::new()
            .with_anchor_date(Some(anchor))
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidArgument
    );
    let fixed = KlineQuery::new()
        .with_adjust(AdjustMode::FixedQfq)?
        .with_anchor_date(Some(anchor))?;
    assert_eq!(
        fixed.with_adjust(AdjustMode::None).unwrap_err().kind(),
        ErrorKind::InvalidArgument
    );
    assert_eq!(
        KlineQuery::new()
            .with_adjust(AdjustMode::FixedHfq)?
            .with_anchor_date(Some(eltdx::TradingDate::new(10_000, 1, 1)?))
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidArgument
    );

    let server =
        Server::start(|request| panic!("empty batch submitted 0x{:04x}", request.msg_type))?;
    let client = TdxClient::builder()
        .endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()?;
    let options = OperationOptions::default();
    assert!(client
        .klines()
        .page_many(&[], &KlineQuery::new(), &options)?
        .is_empty());
    assert!(client
        .klines()
        .collect_many(&[], &KlineQuery::new(), &options)?
        .is_empty());
    assert!(client
        .klines()
        .delisted_page_many(&[], &DelistedQuery::new(), &options)?
        .is_empty());
    assert!(client
        .klines()
        .delisted_collect_many(&[], &DelistedQuery::new(), &options)?
        .is_empty());
    client.close()?;
    server.finish()?;
    Ok(())
}
