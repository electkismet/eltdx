//! Authored synthetic HQ auction cases; execution deferred with the full SDK.
#[path = "support/hq_server.rs"] mod hq_server;
use std::sync::{Arc, Mutex};
use eltdx::{ErrorKind, OperationOptions, RawPolicy, SecurityId, TdxClient};
use eltdx::types::{AuctionQuery, EventTimeSource, MarketDateChoice, QuantityUnit, TradingDate};
use hq_server::Server;

fn id() -> SecurityId { "sh600000".parse().unwrap() }
fn payload(rows: &[(u16, u8, f32, u32, i32, u8)]) -> Vec<u8> {
    let mut bytes = (rows.len() as u16).to_le_bytes().to_vec();
    for &(minute, second, price, matched, unmatched, reserved) in rows {
        bytes.extend_from_slice(&minute.to_le_bytes()); bytes.extend_from_slice(&price.to_le_bytes());
        bytes.extend_from_slice(&matched.to_le_bytes()); bytes.extend_from_slice(&unmatched.to_le_bytes());
        bytes.push(reserved); bytes.push(second);
    }
    bytes
}

#[test]
fn auctions_preserve_source_sequence_seconds_bits_raw_fields_and_date_dispatch() -> Result<(), Box<dyn std::error::Error>> {
    let seen = Arc::new(Mutex::new(Vec::new())); let observed = Arc::clone(&seen);
    let server = Server::start(move |request| {
        assert_eq!(request.msg_type, 0x056a); observed.lock().unwrap().push(request.data.clone());
        payload(&[(940, 59, 162.12, 2568, i32::MIN, 9), (550, 3, 162.13, 8, 0, 0)])
    })?;
    let client = TdxClient::builder().auction_endpoints(vec![server.address])
        .endpoints(vec!["127.0.0.1:1".parse()?]).heartbeat_interval(None).build()?;
    let date = TradingDate::new(2026, 8, 13)?;
    for (index, query) in [AuctionQuery::new(), AuctionQuery::new().with_date(MarketDateChoice::Date(date))?].iter().enumerate() {
        let series = client.auctions().series(&id(), query, &OperationOptions::default().with_raw(RawPolicy::RetainWire))?;
        assert_eq!(series.mode_selector(), 3); assert_eq!(series.start(), 0); assert_eq!(series.requested(), 500);
        let rows = series.points(); assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].time().clock(), Some((15, 40, 59)));
        assert_eq!(rows[1].time().clock(), Some((9, 10, 3)));
        assert_eq!(rows[0].price().value().wire_f32_bits(), Some(162.12_f32.to_bits()));
        assert_eq!(rows[0].unmatched_signed(), i64::from(i32::MIN));
        assert_eq!(rows[0].unmatched().value().as_f64(), 2_147_483_648.0);
        assert_eq!(rows[0].matched().unit(), QuantityUnit::Unknown);
        assert_eq!(rows[0].reserved(), 9);
        assert_eq!(rows[0].time().source_kind(), if index == 0 { EventTimeSource::TimeOfDay } else { EventTimeSource::Local });
        let bytes = &seen.lock().unwrap()[index];
        assert_eq!(u32::from_le_bytes(bytes[8..12].try_into()?), if index == 0 { 0 } else { 2026_0813 });
        assert_eq!(u32::from_le_bytes(bytes[12..16].try_into()?), 3);
        let raw = series.meta().raw().unwrap();
        assert_eq!(raw.row_ranges(), [(0, 2, 16), (0, 18, 16)]);
        assert_eq!(raw.wire_frames().unwrap().len(), 1);
    }
    assert_eq!(server.connection_count(), 1);
    client.close()?; server.finish()?; Ok(())
}

#[test]
fn corrupt_auction_time_price_trailing_and_oversized_rows_are_errors() -> Result<(), Box<dyn std::error::Error>> {
    for case in 0..6 {
        let server = Server::start(move |_| match case {
            0 => payload(&[(1440, 0, 1.0, 0, 0, 0)]),
            1 => payload(&[(600, 60, 1.0, 0, 0, 0)]),
            2 => payload(&[(600, 0, f32::NAN, 0, 0, 0)]),
            3 => payload(&[(600, 0, f32::INFINITY, 0, 0, 0)]),
            4 => { let mut bytes = payload(&[]); bytes.push(0); bytes },
            _ => payload(&vec![(600, 0, 1.0, 0, 0, 0); 501]),
        })?;
        let client = TdxClient::builder().auction_endpoints(vec![server.address]).heartbeat_interval(None).build()?;
        assert_eq!(client.auctions().series(&id(), &AuctionQuery::new(), &OperationOptions::default()).unwrap_err().kind(), ErrorKind::Protocol);
        client.close()?; server.finish()?;
    }
    Ok(())
}

#[test]
fn lazy_getters_share_dedicated_pool_but_ordinary_uses_another_connection() -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::start(|request| { assert_eq!(request.msg_type, 0x056a); payload(&[]) })?;
    let client = TdxClient::builder().endpoints(vec![server.address]).auction_endpoints(vec![server.address])
        .heartbeat_interval(None).build()?;
    let first = client.auctions(); let second = client.clone().auctions();
    assert_eq!(server.connection_count(), 0);
    client.connect(&OperationOptions::default())?;
    first.series(&id(), &AuctionQuery::new(), &OperationOptions::default())?;
    second.series(&id(), &AuctionQuery::new(), &OperationOptions::default())?;
    assert_eq!(server.connection_count(), 2);
    client.close()?;
    assert_eq!(first.series(&id(), &AuctionQuery::new(), &OperationOptions::default()).unwrap_err().kind(), ErrorKind::Closed);
    server.finish()?; Ok(())
}

#[test]
fn physical_connection_limit_is_shared_across_domains() -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::start(|_| payload(&[]))?;
    let client = TdxClient::builder().endpoints(vec![server.address]).auction_endpoints(vec![server.address])
        .max_connections(1).heartbeat_interval(None).build()?;
    client.connect(&OperationOptions::default())?;
    let error = client.auctions().series(&id(), &AuctionQuery::new(), &OperationOptions::default()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::LimitExceeded);
    assert_eq!(server.connection_count(), 1);
    client.close()?; server.finish()?; Ok(())
}

#[test]
fn builder_rejects_invalid_dedicated_configuration_without_startup() {
    assert!(TdxClient::builder().auction_endpoints(vec![]).build().is_err());
    assert!(TdxClient::builder().money_flow_endpoints(vec!["127.0.0.1:0".parse().unwrap()]).build().is_err());
    assert!(TdxClient::builder().max_connections(0).build().is_err());
    assert!(TdxClient::builder().pool_size(2).max_connections(1).build().is_err());
    assert!(AuctionQuery::new().with_date(MarketDateChoice::Date(TradingDate::new(10_000, 1, 1).unwrap())).is_err());
}
