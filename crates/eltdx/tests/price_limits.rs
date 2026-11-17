#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Policy defaults are evidence decisions, not claimed exchange-rule examples.
#[path = "support/hq_server.rs"]
mod hq_server;
use eltdx::{
    ErrorKind, LimitStatus, NumericValue, OperationOptions, PageOptions, RawPolicy,
    SpecialLimitQuery, TdxClient, TradingDate,
};
use std::sync::{Arc, Mutex};

fn payload(rows: &[(u8, u32, f32, f32)]) -> Vec<u8> {
    let mut bytes = (rows.len() as u16).to_le_bytes().to_vec();
    for &(market, code, upper, lower) in rows {
        bytes.push(market);
        bytes.extend_from_slice(&code.to_le_bytes());
        bytes.extend_from_slice(&upper.to_le_bytes());
        bytes.extend_from_slice(&lower.to_le_bytes());
    }
    bytes
}

#[test]
fn all_six_disputed_real_policy_cases_remain_unknown() {
    let date = TradingDate::new(2026, 10, 27).unwrap();
    for code in [
        "sh600000", "sz300001", "sz303001", "bj920001", "sh880001", "sz000038",
    ] {
        let decision = eltdx::price_limits::evaluate(
            &code.parse().unwrap(),
            date,
            Some(eltdx::Price::from_f64(10.)),
        )
        .unwrap();
        assert_eq!(decision.status(), LimitStatus::UnknownRule);
        assert!(decision.upper().is_none() && decision.lower().is_none());
        assert!(!decision.evidence().is_empty());
    }
}

#[test]
fn special_scan_uses_actual_counts_and_empty_eof_without_encoding_a_page_size() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let observed = seen.clone();
    let upper = f32::from_bits(0x43548800);
    let server = hq_server::Server::start(move |request| {
        assert_eq!(request.msg_type, 0x0452);
        assert_eq!(request.data.len(), 14);
        assert!(request.data[2..].iter().all(|byte| *byte == 0));
        let offset = u16::from_le_bytes(request.data[..2].try_into().unwrap());
        observed.lock().unwrap().push(offset);
        match offset {
            0 => payload(&[(1, 600000, upper, 9.), (255, u32::MAX, 12., 8.)]),
            2 => payload(&[(1, 600001, 11., 9.)]),
            3 => payload(&[]),
            _ => panic!(),
        }
    })
    .unwrap();
    let client = TdxClient::builder()
        .endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()
        .unwrap();
    let query = SpecialLimitQuery::new()
        .with_pages(PageOptions::new(1, 5, 20).unwrap())
        .unwrap();
    let rows = client
        .limits()
        .scan_special(
            &query,
            &OperationOptions::default().with_raw(RawPolicy::RetainPayload),
        )
        .unwrap();
    assert_eq!(*seen.lock().unwrap(), vec![0, 2, 3]);
    assert_eq!(rows.pages(), 3);
    assert_eq!(rows.items().len(), 3);
    assert_eq!(
        rows.items()[0].upper().value().wire_f32_bits(),
        Some(upper.to_bits())
    );
    assert!(rows.items()[1].id().is_none());
    assert_eq!(rows.items()[1].code_raw(), u32::MAX);
    assert_eq!(rows.meta().raw().unwrap().payloads().len(), 3);
    assert_eq!(server.connection_count(), 1);
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn cap_without_empty_eof_is_an_error_and_releases_the_scan_pin() {
    let server = hq_server::Server::start(|_| payload(&[(1, 600000, 11., 9.)])).unwrap();
    let client = TdxClient::builder()
        .endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()
        .unwrap();
    let query = SpecialLimitQuery::new()
        .with_pages(PageOptions::new(800, 2, 1).unwrap())
        .unwrap();
    assert_eq!(
        client
            .limits()
            .scan_special(&query, &OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::LimitExceeded
    );
    let mut pin = client.pin(&OperationOptions::default()).unwrap();
    pin.close().unwrap();
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn special_continuation_rejects_another_owner_and_preserves_price_bits() {
    let server = hq_server::Server::start(|_| payload(&[(1, 600000, 11., 9.)])).unwrap();
    let first = TdxClient::builder()
        .endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()
        .unwrap();
    let second = TdxClient::builder()
        .endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()
        .unwrap();
    let page = first
        .limits()
        .special(&SpecialLimitQuery::new(), &OperationOptions::default())
        .unwrap();
    assert!(matches!(
        page.items()[0].upper().value(),
        NumericValue::F32(_)
    ));
    let continuation = SpecialLimitQuery::new()
        .with_cursor(page.next_cursor().unwrap())
        .unwrap();
    assert_eq!(
        second
            .limits()
            .special(&continuation, &OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidArgument
    );
    first.close().unwrap();
    second.close().unwrap();
    server.finish().unwrap();
}
