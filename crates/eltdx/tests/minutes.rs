#[path = "support/hq_server.rs"]
mod hq_server;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use eltdx::{
    Clock, ErrorKind, EventTimeSource, OperationOptions, QuantityUnit, SecurityId, TdxClient, TradingDate,
};
use eltdx::types::{AuxKind, AuxValues, MarketDateChoice, MinuteQuery, SparklineQuery};
use eltdx_protocol::frame::RequestFrame;
use hq_server::{directory, directory_offset, Server};

struct ProbeClock(Arc<AtomicUsize>);

impl Clock for ProbeClock {
    fn unix_seconds(&self) -> eltdx::Result<i64> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(0)
    }
}

fn id(value: &str) -> SecurityId { value.parse().expect("valid security id") }

fn catalog(request: &RequestFrame) -> Vec<u8> {
    assert_eq!(request.msg_type, 0x044d);
    if directory_offset(request) == 0 { directory(&[("600000", 2)]) } else { directory(&[]) }
}

fn empty_history() -> Vec<u8> {
    let mut payload = vec![0, 0];
    payload.extend_from_slice(&10.0_f32.to_le_bytes());
    payload
}

fn empty_recent() -> Vec<u8> {
    let mut payload = vec![0, 0];
    payload.extend_from_slice(&10.0_f32.to_le_bytes());
    payload.extend_from_slice(&11.0_f32.to_le_bytes());
    payload
}

fn sparkline_payload(market: u8, code: &str, echo: u8, prices: &[f32]) -> Vec<u8> {
    let mut payload = vec![market, 0];
    payload.extend_from_slice(code.as_bytes());
    payload.extend_from_slice(&[0; 16]);
    payload.extend_from_slice(&[echo, 0]);
    payload.extend_from_slice(&0x1234_u32.to_le_bytes());
    payload.extend_from_slice(&[0; 4]);
    payload.extend_from_slice(&60_u16.to_le_bytes());
    payload.extend_from_slice(&10.0_f32.to_le_bytes());
    payload.extend_from_slice(&u16::try_from(prices.len()).expect("bounded sparkline").to_le_bytes());
    for price in prices { payload.extend_from_slice(&price.to_le_bytes()); }
    payload
}

#[test]
fn explicit_dates_do_not_resolve_current_date() -> Result<(), Box<dyn std::error::Error>> {
    let reads = Arc::new(AtomicUsize::new(0));
    let clock = Arc::new(ProbeClock(Arc::clone(&reads)));
    let server = Server::start(|request| match request.msg_type {
        0x044d => catalog(request),
        0x0fb4 => empty_history(),
        0x0feb => empty_recent(),
        _ => panic!("unexpected command 0x{:04x}", request.msg_type),
    })?;
    let client = TdxClient::builder().clock(clock).endpoints(vec![server.address]).heartbeat_interval(None).build()?;
    let date = TradingDate::new(2026, 11, 15)?;
    client.minutes().history(&id("sh600000"), date, &OperationOptions::default())?;
    reads.store(0, Ordering::SeqCst);
    client.minutes().history(&id("sh600000"), date, &OperationOptions::default())?;
    assert_eq!(reads.load(Ordering::SeqCst), 1, "only the metadata freshness check reads the clock");

    reads.store(0, Ordering::SeqCst);
    let query = MinuteQuery::new().with_date(MarketDateChoice::Date(date))?;
    client.minutes().recent(&id("sh600000"), &query, &OperationOptions::default())?;
    assert_eq!(reads.load(Ordering::SeqCst), 1, "explicit recent date does not add a current-date clock read");
    client.close()?;
    server.finish()?;
    Ok(())
}

#[test]
fn current_date_is_resolved_once_for_duplicate_batch_occurrences() -> Result<(), Box<dyn std::error::Error>> {
    let reads = Arc::new(AtomicUsize::new(0));
    let clock = Arc::new(ProbeClock(Arc::clone(&reads)));
    let today_calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&today_calls);
    let server = Server::start(move |request| match request.msg_type {
        0x044d => catalog(request),
        0x0fb4 => empty_history(),
        0x0537 => {
            observed.fetch_add(1, Ordering::SeqCst);
            vec![0, 0, 0, 0]
        }
        _ => panic!("unexpected command 0x{:04x}", request.msg_type),
    })?;
    let client = TdxClient::builder().clock(clock).endpoints(vec![server.address]).heartbeat_interval(None).build()?;
    client.minutes().history(&id("sh600000"), TradingDate::new(2026, 11, 15)?, &OperationOptions::default())?;
    reads.store(0, Ordering::SeqCst);
    let duplicate = id("sh600000");
    let result = client.minutes().today_many(&[duplicate.clone(), duplicate], &OperationOptions::default())?;

    assert_eq!(result.len(), 2);
    assert_eq!(result.items().iter().map(|item| item.input_index()).collect::<Vec<_>>(), vec![0, 1]);
    assert!(result.items().iter().all(|item| matches!(item.state(), eltdx::BatchItemState::Present(series)
        if series.trading_date() == Some(TradingDate::new(1970, 1, 1).expect("valid epoch date")))));
    assert_eq!(today_calls.load(Ordering::SeqCst), 2);
    assert_eq!(reads.load(Ordering::SeqCst), 2, "one metadata freshness read and one shared operation date read");
    client.close()?;
    server.finish()?;
    Ok(())
}

#[test]
fn minute_wire_families_keep_their_time_and_average_semantics() -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::start(|request| match request.msg_type {
        0x044d => catalog(request),
        0x0537 => vec![1, 0, 0, 0, 10, 12, 3],
        0x0fb4 => {
            let mut payload = vec![1, 0];
            payload.extend_from_slice(&10.0_f32.to_le_bytes());
            payload.extend_from_slice(&[10, 20, 5]);
            payload
        }
        0x0feb => {
            let mut payload = vec![1, 0];
            payload.extend_from_slice(&10.0_f32.to_le_bytes());
            payload.extend_from_slice(&11.0_f32.to_le_bytes());
            payload.extend_from_slice(&[10, 12, 5]);
            payload
        }
        _ => panic!("unexpected command 0x{:04x}", request.msg_type),
    })?;
    let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).build()?;
    let id = id("sh600000");
    let date = TradingDate::new(2026, 11, 15)?;
    let today = client.minutes().today(&id, &OperationOptions::default())?;
    assert_eq!(today.points().len(), 1);
    assert_eq!(today.points()[0].time().source_kind(), EventTimeSource::TimeOfDay);
    assert_eq!(today.points()[0].average().expect("today average").as_f64(), 0.0012);
    assert_eq!(today.points()[0].volume().unit(), QuantityUnit::Unknown);
    assert_eq!(today.volume_sum().value().as_f64(), 3.0);

    let history = client.minutes().history(&id, date, &OperationOptions::default())?;
    assert_eq!(history.points().len(), 1);
    assert_eq!(history.points()[0].average(), None);
    assert_eq!(history.points()[0].time().date(), Some(date));

    let recent = client.minutes().recent(&id, &MinuteQuery::new().with_date(MarketDateChoice::Date(date))?, &OperationOptions::default())?;
    assert_eq!(recent.points().len(), 1);
    assert_eq!(recent.points()[0].average().expect("recent average").as_f64(), 0.0012);
    assert_eq!(recent.previous_close().unwrap().as_f64(), 10.0);
    assert_eq!(recent.open().unwrap().as_f64(), 11.0);
    assert_eq!(recent.points()[0].volume().value().as_f64(), 5.0);
    client.close()?;
    server.finish()?;
    Ok(())
}

#[test]
fn auxiliary_float_bits_and_sparkline_identity_selector_are_preserved() -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::start(|request| match request.msg_type {
        0x051b => {
            let mut payload = vec![1, 0];
            payload.extend_from_slice(&100.5_f32.to_le_bytes());
            payload.extend_from_slice(&120.5_f32.to_le_bytes());
            payload
        }
        0x0fd1 => sparkline_payload(1, "600000", 1, &[10.1, 10.2]),
        _ => panic!("unexpected command 0x{:04x}", request.msg_type),
    })?;
    let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).build()?;
    let id = id("sh600000");
    let aux = client.minutes().aux(&id, AuxKind::VolumeComparison,
        &OperationOptions::default().with_raw(eltdx::RawPolicy::RetainPayload))?;
    assert!(matches!(aux.points()[0].values(), AuxValues::Volumes { previous, current, cumulative }
        if previous.to_bits() == 100.5_f32.to_bits()
            && current.to_bits() == 120.5_f32.to_bits()
            && cumulative.to_bits() == 120.5_f32.to_bits()));
    assert_eq!(aux.meta().raw().unwrap().payloads().len(), 1);

    let sparkline = client.minutes().sparkline(&id, &SparklineQuery::new().with_window(2)?, &OperationOptions::default())?;
    assert_eq!(sparkline.selector_echo(), 1);
    assert_eq!(sparkline.reserved(), 0x1234);
    assert_eq!(sparkline.prices().iter().map(|price| price.value().wire_f32_bits().unwrap()).collect::<Vec<_>>(),
        vec![10.1_f32.to_bits(), 10.2_f32.to_bits()]);
    client.close()?;
    server.finish()?;
    Ok(())
}

#[test]
fn sparkline_rejects_unsupported_selector_identity_echo_and_window_overflow() -> Result<(), Box<dyn std::error::Error>> {
    for (market, code, echo, prices, selector, expected) in [
        (1, "600000", 1, vec![10.0], 2, ErrorKind::Unsupported),
        (1, "600001", 1, vec![10.0], 1, ErrorKind::Protocol),
        (1, "600000", 2, vec![10.0], 1, ErrorKind::Protocol),
        (1, "600000", 1, vec![10.0, 10.1, 10.2], 1, ErrorKind::Protocol),
    ] {
        let server = Server::start(move |request| match request.msg_type {
            0x0fd1 => sparkline_payload(market, code, echo, &prices),
            _ => panic!("unexpected command 0x{:04x}", request.msg_type),
        })?;
        let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).build()?;
        let query = SparklineQuery::new().with_selector(selector)?.with_window(2)?;
        let error = client.minutes().sparkline(&id("sh600000"), &query, &OperationOptions::default()).unwrap_err();
        assert_eq!(error.kind(), expected);
        client.close()?;
        server.finish()?;
    }
    Ok(())
}

#[test]
fn empty_history_is_present_and_empty_batches_do_not_fetch_metadata() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(MinuteQuery::new().with_date(MarketDateChoice::Date(TradingDate::new(10_000, 1, 1)?))
        .unwrap_err().kind(), ErrorKind::InvalidArgument);
    let server = Server::start(|request| match request.msg_type {
        0x044d => catalog(request),
        0x0fb4 => empty_history(),
        _ => panic!("unexpected command 0x{:04x}", request.msg_type),
    })?;
    let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).build()?;
    let date = TradingDate::new(2026, 8, 13)?;
    let result = client.minutes().history_many(&[id("sh600000"), id("sh600000")], date, &OperationOptions::default())?;
    assert_eq!(result.len(), 2);
    for item in result.items() {
        assert!(matches!(item.state(), eltdx::BatchItemState::Present(series)
            if series.points().is_empty() && series.trading_date() == Some(date)));
    }
    client.close()?;
    server.finish()?;

    let server = Server::start(|request| panic!("empty minute batch submitted 0x{:04x}", request.msg_type))?;
    let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).build()?;
    assert!(client.minutes().today_many(&[], &OperationOptions::default())?.is_empty());
    assert!(client.minutes().history_many(&[], date, &OperationOptions::default())?.is_empty());
    assert!(client.minutes().recent_many(&[], &MinuteQuery::new(), &OperationOptions::default())?.is_empty());
    client.close()?;
    server.finish()?;
    Ok(())
}

#[test]
fn minute_truncation_and_unknown_trailing_data_are_rejected() -> Result<(), Box<dyn std::error::Error>> {
    for truncated in [true, false] {
        let server = Server::start(move |request| match request.msg_type {
            0x044d => catalog(request),
            0x0fb4 => {
                let mut payload = empty_history();
                if truncated { payload[0] = 1; } else { payload.push(0xff); }
                payload
            }
            _ => panic!("unexpected command 0x{:04x}", request.msg_type),
        })?;
        let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).build()?;
        let error = client.minutes().history(&id("sh600000"), TradingDate::new(2026, 8, 13)?, &OperationOptions::default()).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Protocol);
        client.close()?;
        server.finish()?;
    }
    Ok(())
}

