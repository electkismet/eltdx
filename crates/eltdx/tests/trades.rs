//! Synthetic protocol evidence; no server capture or production-price oracle.
#[path = "support/hq_server.rs"] mod hq_server;

use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use eltdx::{BatchItemState, CancellationToken, EndEvidence, ErrorKind, OperationOptions,
    PageOptions, RawPolicy, SecurityId, TdxClient};
use eltdx::types::{EventTimeSource, MarketDateChoice, NumericValue, TradeEventKind, TradeQuery, TradeSide, TradingDate};
use eltdx_protocol::frame::RequestFrame;
use hq_server::Server;

fn id() -> SecurityId { "sh600000".parse().unwrap() }
fn historical() -> TradeQuery {
    TradeQuery::new().with_date(MarketDateChoice::Date(TradingDate::new(2026, 8, 13).unwrap())).unwrap()
}
fn offset(request: &RequestFrame) -> u16 {
    let start = if request.msg_type == 0x0fc5 { 8 } else { 12 };
    u16::from_le_bytes(request.data[start..start + 2].try_into().unwrap())
}
fn count(request: &RequestFrame) -> u16 {
    let start = if request.msg_type == 0x0fc5 { 10 } else { 14 };
    u16::from_le_bytes(request.data[start..start + 2].try_into().unwrap())
}
fn varint(out: &mut Vec<u8>, value: i64) {
    let mut magnitude = value.unsigned_abs();
    let mut first = (magnitude & 0x3f) as u8 | if value < 0 { 0x40 } else { 0 };
    magnitude >>= 6;
    if magnitude != 0 { first |= 0x80; }
    out.push(first);
    while magnitude != 0 {
        let mut byte = (magnitude & 0x7f) as u8; magnitude >>= 7;
        if magnitude != 0 { byte |= 0x80; }
        out.push(byte);
    }
}
fn payload(request: &RequestFrame, rows: &[(u16, i64, i64, i64)]) -> Vec<u8> {
    let mut bytes = (rows.len() as u16).to_le_bytes().to_vec();
    if request.msg_type == 0x0fc6 { bytes.extend_from_slice(&12.25_f32.to_le_bytes()); }
    for &(minute, delta, status, tail) in rows {
        bytes.extend_from_slice(&minute.to_le_bytes());
        for value in [delta, 23, 2, status, tail] { varint(&mut bytes, value); }
    }
    bytes
}

#[test]
fn collect_keeps_page_local_order_absolute_indices_late_trades_and_raw_ranges() -> Result<(), Box<dyn std::error::Error>> {
    let seen = Arc::new(Mutex::new(Vec::new())); let observed = Arc::clone(&seen);
    let server = Server::start(move |request| {
        assert_eq!(request.msg_type, 0x0fc5);
        observed.lock().unwrap().push((offset(request), count(request)));
        match offset(request) {
            0 => payload(request, &[(910, 1234, 5, 7), (909, 1, 99, 9)]),
            2 => payload(request, &[(565, 1220, 8, 1), (565, 2, 0, 2)]),
            4 => payload(request, &[]),
            other => panic!("unexpected offset {other}"),
        }
    })?;
    let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).build()?;
    let query = TradeQuery::new().with_pages(PageOptions::new(3, 4, 20)?)?;
    let page = client.trades().collect(&id(), &query, &OperationOptions::default().with_raw(RawPolicy::RetainPayload))?;
    let ticks = page.ticks().items();
    assert_eq!(ticks.iter().map(|tick| tick.absolute_index()).collect::<Vec<_>>(), [2, 3, 0, 1]);
    assert_eq!(ticks.iter().map(|tick| tick.index()).collect::<Vec<_>>(), [0, 1, 0, 1]);
    assert_eq!(ticks[0].kind(), TradeEventKind::AuctionSnapshot);
    assert_eq!(ticks[1].kind(), TradeEventKind::OpeningMatch);
    assert_eq!(ticks[2].time().clock(), Some((15, 10, 0)));
    assert_eq!(ticks[3].side(), TradeSide::Status(99));
    assert_eq!(ticks[0].auction_matched(), None);
    assert_eq!(ticks[0].volume().unit(), eltdx::QuantityUnit::Unknown);
    assert_eq!(ticks[1].volume().unit(), eltdx::QuantityUnit::Lots);
    assert_eq!(ticks[2].unknown_tail(), Some(7));
    assert_eq!(page.ticks().end(), EndEvidence::EmptyPage);
    assert_eq!(page.pages_read(), 3);
    assert_eq!(*seen.lock().unwrap(), [(0, 3), (2, 3), (4, 3)]);
    let raw = page.meta().raw().unwrap();
    assert_eq!(raw.payloads().len(), 3);
    assert_eq!(raw.row_ranges().iter().map(|row| row.0).collect::<Vec<_>>(), [1, 1, 0, 0]);
    client.close()?; server.finish()?; Ok(())
}

#[test]
fn today_and_history_columns_reconstruct_every_row_field_without_a_second_request() -> Result<(), Box<dyn std::error::Error>> {
    let calls = Arc::new(AtomicUsize::new(0)); let observed = Arc::clone(&calls);
    let server = Server::start(move |request| {
        assert!(matches!(request.msg_type, 0x0fc5 | 0x0fc6));
        observed.fetch_add(1, Ordering::SeqCst);
        payload(request, &[(565, 125_528, 8, 17), (916, -2, 1, 18)])
    })?;
    let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).build()?;
    let bond: SecurityId = "sh113692".parse()?;
    for (index, query) in [TradeQuery::new(), historical()].iter().enumerate() {
        let columns = client.trades().columns(&bond, query, &OperationOptions::default())?;
        let page = columns.to_page();
        assert_eq!(calls.load(Ordering::SeqCst), index + 1);
        assert_eq!(columns.len(), 2);
        assert_eq!(columns.columns().prices()[0].value(), NumericValue::ScaledInteger { value: 125_528, scale: 3 });
        assert_eq!(columns.tick(1)?, page.ticks().items()[1]);
        assert_eq!(columns.tick(2).unwrap_err().kind(), ErrorKind::InvalidArgument);
        assert_eq!(columns.tick(0)?.source().command(), Some(if index == 0 { 0x0fc5 } else { 0x0fc6 }));
        assert_eq!(columns.tick(0)?.time().source_kind(), if index == 0 { EventTimeSource::TimeOfDay } else { EventTimeSource::Local });
        assert_eq!(columns.tick(0)?.unknown_tail(), if index == 0 { Some(17) } else { None });
        assert_eq!(columns.tick(0)?.reserved_zero(), if index == 0 { None } else { Some(17) });
        assert_eq!(columns.columns().price_denominators(), [1000, 1000]);
        assert!(columns.columns().volumes().iter().all(|volume| volume.unit() == eltdx::QuantityUnit::Unknown));
        assert!(columns.meta().raw().is_none());
    }
    client.close()?; server.finish()?; Ok(())
}

#[test]
fn ordinary_ticks_keep_hundredths_and_ignore_quote_precision_metadata() -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::start(|request| { assert_eq!(request.msg_type, 0x0fc5); payload(request, &[(600, 12345, 0, 0)]) })?;
    let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).build()?;
    let page = client.trades().page(&id(), &TradeQuery::new(), &OperationOptions::default())?;
    assert_eq!(page.ticks().items()[0].price().value(), NumericValue::ScaledInteger { value: 12345, scale: 2 });
    assert_eq!(page.ticks().items()[0].price_acc_raw(), 12345);
    assert_eq!(page.ticks().items()[0].volume().unit(), eltdx::QuantityUnit::Lots);
    assert_eq!(page.ticks().items()[0].volume().provenance(), eltdx::types::ValueProvenance::PrefixFallback);
    client.close()?; server.finish()?; Ok(())
}

#[test]
fn opening_match_scans_source_order_skips_status_eight_and_keeps_its_origin() -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::start(|request| match offset(request) {
        0 => payload(request, &[(565, 1000, 8, 0)]),
        1 => payload(request, &[(565, 1001, 1, 0), (565, 1, 0, 0)]),
        _ => panic!("opening match must stop at its first semantic match"),
    })?;
    let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).build()?;
    let query = historical().with_pages(PageOptions::new(2, 4, 10)?)?;
    let tick = client.trades().opening_match(&id(), &query, &OperationOptions::default())?.unwrap();
    assert_eq!(tick.absolute_index(), 1);
    assert_eq!(tick.side(), TradeSide::Sell);
    assert_eq!(tick.kind(), TradeEventKind::OpeningMatch);
    assert_eq!(tick.source().endpoint(), Some(server.address));
    assert_eq!(tick.source().trading_date(), Some(TradingDate::new(2026, 8, 13)?));
    client.close()?; server.finish()?; Ok(())
}

#[test]
fn opening_none_requires_empty_page_and_a_cap_is_an_error() -> Result<(), Box<dyn std::error::Error>> {
    for cap in [1, 2] {
        let server = Server::start(|request| if offset(request) == 0 { payload(request, &[(565, 100, 8, 0)]) } else { payload(request, &[]) })?;
        let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).build()?;
        let query = TradeQuery::new().with_pages(PageOptions::new(1, cap, 10)?)?;
        let result = client.trades().opening_match(&id(), &query, &OperationOptions::default());
        if cap == 1 { assert_eq!(result.unwrap_err().kind(), ErrorKind::LimitExceeded); }
        else { assert!(result?.is_none()); }
        client.close()?; server.finish()?;
    }
    Ok(())
}

#[test]
fn repeated_page_overflow_oversized_time_and_trailing_bytes_are_rejected() -> Result<(), Box<dyn std::error::Error>> {
    for case in 0..5 {
        let server = Server::start(move |request| {
            let mut bytes = match case {
                2 => payload(request, &[(600, 100, 0, 0), (601, 1, 1, 0)]),
                3 => payload(request, &[(1440, 100, 0, 0)]),
                _ => payload(request, &[(600, 100, 0, 0)]),
            };
            if case == 4 { bytes.push(0); }
            bytes
        })?;
        let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).build()?;
        let mut query = TradeQuery::new().with_count(1)?.with_pages(PageOptions::new(1, 4, 10)?)?;
        if case == 1 { query = query.with_start(u16::MAX)?; }
        let result = if case <= 1 { client.trades().collect(&id(), &query, &OperationOptions::default()) }
            else { client.trades().page(&id(), &query, &OperationOptions::default()) };
        assert_eq!(result.unwrap_err().kind(), if case == 1 { ErrorKind::LimitExceeded } else { ErrorKind::Protocol });
        client.close()?; server.finish()?;
    }
    Ok(())
}

#[test]
fn many_preserves_duplicates_and_empty_series_is_present() -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::start(|request| payload(request, &[]))?;
    let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).build()?;
    let ids = [id(), id()]; let options = OperationOptions::default(); let query = historical();
    let pages = client.trades().page_many(&ids, &query, &options)?;
    for (index, item) in pages.items().iter().enumerate() {
        assert_eq!(item.input_index(), index);
        match item.state() { BatchItemState::Present(page) => assert!(page.ticks().items().is_empty()), _ => panic!("empty is present") }
    }
    assert_eq!(client.trades().collect_many(&ids, &query, &options)?.len(), 2);
    assert_eq!(client.trades().columns_many(&ids, &query, &options)?.len(), 2);
    assert_eq!(client.trades().collect_columns_many(&ids, &query, &options)?.len(), 2);
    let matches = client.trades().opening_match_many(&ids, &query, &options)?;
    assert!(matches.items().iter().all(|item| matches!(item.state(), BatchItemState::Present(None))));
    client.close()?; server.finish()?; Ok(())
}

#[test]
fn cursor_binds_security_owner_date_and_page_size_before_submission() -> Result<(), Box<dyn std::error::Error>> {
    let calls = Arc::new(AtomicUsize::new(0)); let observed = Arc::clone(&calls);
    let server = Server::start(move |request| { observed.fetch_add(1, Ordering::SeqCst); payload(request, &[(600, 100, 0, 0)]) })?;
    let builder = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None);
    let client = builder.clone().build()?; let other = builder.build()?;
    let options = OperationOptions::default(); let query = historical().with_count(1)?;
    let page = client.trades().page(&id(), &query, &options)?;
    let resumed = query.with_cursor(page.ticks().next_cursor().unwrap())?;
    assert_eq!(client.trades().page(&"sh600001".parse()?, &resumed, &options).unwrap_err().kind(), ErrorKind::InvalidArgument);
    assert_eq!(other.trades().page(&id(), &resumed, &options).unwrap_err().kind(), ErrorKind::InvalidArgument);
    assert_eq!(client.trades().page(&id(), &resumed.clone().with_count(2)?, &options).unwrap_err().kind(), ErrorKind::InvalidArgument);
    assert_eq!(client.trades().page(&id(), &resumed.clone().with_date(MarketDateChoice::Current)?, &options).unwrap_err().kind(), ErrorKind::InvalidArgument);
    assert_eq!(client.trades().page_many(&[id()], &resumed, &options).unwrap_err().kind(), ErrorKind::InvalidArgument);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    client.close()?; other.close()?; server.finish()?; Ok(())
}

#[test]
fn zero_work_invalid_queries_and_stopped_operations_do_not_start_network() -> eltdx::Result<()> {
    let client = TdxClient::builder().endpoints(vec!["127.0.0.1:1".parse().unwrap()]).build()?;
    let query = TradeQuery::new(); let options = OperationOptions::default();
    assert!(client.trades().page_many(&[], &query, &options)?.is_empty());
    assert!(client.trades().collect_many(&[], &query, &options)?.is_empty());
    assert!(client.trades().columns_many(&[], &query, &options)?.is_empty());
    assert!(client.trades().collect_columns_many(&[], &query, &options)?.is_empty());
    assert!(client.trades().opening_match_many(&[], &query, &options)?.is_empty());
    assert!(TradeQuery::new().with_count(0).is_err());
    assert!(TradeQuery::new().with_pages(PageOptions::new(1801, 2, 10)?).is_err());
    assert!(TradeQuery::new().with_date(MarketDateChoice::Date(TradingDate::new(10_000, 1, 1)?)).is_err());
    let token = CancellationToken::default(); token.cancel();
    assert_eq!(client.trades().page(&id(), &query, &options.clone().with_cancellation(token)).unwrap_err().kind(), ErrorKind::Cancelled);
    assert_eq!(client.trades().collect(&id(), &query, &options.with_timeout(Duration::ZERO)).unwrap_err().kind(), ErrorKind::Timeout);
    client.close()
}

#[test]
fn second_page_cancellation_does_not_return_partial_rows() -> Result<(), Box<dyn std::error::Error>> {
    let token = CancellationToken::default(); let worker_token = token.clone();
    let server = Server::start(move |request| {
        if offset(request) != 0 { worker_token.cancel(); }
        payload(request, &[(600 + offset(request), 100, 0, 0)])
    })?;
    let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).build()?;
    let query = TradeQuery::new().with_pages(PageOptions::new(1, 4, 10)?)?;
    let error = client.trades().collect(&id(), &query, &OperationOptions::default().with_cancellation(token)).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Cancelled);
    client.close()?; server.finish()?; Ok(())
}
