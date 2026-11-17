#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "support/hq_server.rs"]
mod hq_server;
use eltdx::{
    BatchItemState, CancellationToken, CapitalQuery, ErrorKind, FactorQuery, FinanceField,
    FinanceQuery, FinanceValue, MissingPolicy, NumericValue, OperationOptions, QuantityUnit,
    RawPolicy, SecurityId, TdxClient,
};
use eltdx_protocol::frame::RequestFrame;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

fn codes(request: &RequestFrame) -> Vec<String> {
    request.data[2..]
        .as_chunks::<7>()
        .0
        .iter()
        .map(|row| String::from_utf8(row[1..].to_vec()).unwrap())
        .collect()
}
fn ids(numbers: &[&str]) -> Vec<SecurityId> {
    numbers
        .iter()
        .map(|number| format!("sh{number}").parse().unwrap())
        .collect()
}
fn client(server: &hq_server::Server) -> TdxClient {
    TdxClient::builder()
        .endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()
        .unwrap()
}
fn capital(numbers: &[String], declaration: u16) -> Vec<u8> {
    let mut bytes = declaration.to_le_bytes().to_vec();
    for number in numbers {
        bytes.push(1);
        bytes.extend_from_slice(number.as_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
    }
    bytes
}
fn finance(rows: &[(&str, f32)]) -> Vec<u8> {
    let mut payload = (rows.len() as u16).to_le_bytes().to_vec();
    for &(number, circulation) in rows {
        payload.push(1);
        payload.extend_from_slice(number.as_bytes());
        payload.extend_from_slice(&circulation.to_le_bytes());
        payload.extend_from_slice(&3_u16.to_le_bytes());
        payload.extend_from_slice(&7_u16.to_le_bytes());
        payload.extend_from_slice(&20260601_u32.to_le_bytes());
        payload.extend_from_slice(&19910101_u32.to_le_bytes());
        // Remaining30 values in exact fixed-wire order. Reserved2 retains NaN.
        for index in 1..31 {
            let value = if index == 30 {
                f32::from_bits(0x7fc12345)
            } else {
                index as f32 + 0.25
            };
            payload.extend_from_slice(&value.to_le_bytes());
        }
    }
    payload
}

#[test]
fn partial_capital_responses_consume_only_the_exact_ordered_prefix() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let captured = seen.clone();
    let server = hq_server::Server::start(move |request| {
        assert_eq!(request.msg_type, 0x000f);
        let numbers = codes(request);
        captured.lock().unwrap().push(numbers.clone());
        capital(&numbers[..1], 9)
    })
    .unwrap();
    let client = client(&server);
    let input = ids(&["600000", "600001", "600000"]);
    let outcome = client
        .corporate()
        .capital_changes(
            &input,
            &CapitalQuery::new(),
            &OperationOptions::default().with_raw(RawPolicy::RetainPayload),
        )
        .unwrap();
    assert_eq!(outcome.len(), 3);
    for (index, item) in outcome.items().iter().enumerate() {
        assert_eq!(item.input_index(), index);
        assert_eq!(item.security(), &input[index]);
        let BatchItemState::Present(block) = item.state() else {
            panic!("empty event list is Present")
        };
        assert!(block.records().is_empty());
        assert_eq!(block.declared_blocks(), 9);
        assert_eq!(block.meta().raw().unwrap().payloads().len(), 1);
    }
    assert_eq!(
        seen.lock()
            .unwrap()
            .iter()
            .map(Vec::len)
            .collect::<Vec<_>>(),
        vec![3, 2, 1]
    );
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn empty_foreign_or_nonprefix_capital_responses_are_protocol_errors() {
    for mode in 0..3 {
        let server = hq_server::Server::start(move |request| {
            let numbers = codes(request);
            match mode {
                0 => capital(&[], 0),
                1 => capital(&["600009".to_owned()], 1),
                _ => capital(&[numbers[1].clone()], 1),
            }
        })
        .unwrap();
        let client = client(&server);
        assert_eq!(
            client
                .corporate()
                .capital_changes(
                    &ids(&["600000", "600001"]),
                    &CapitalQuery::new(),
                    &OperationOptions::default()
                )
                .unwrap_err()
                .kind(),
            ErrorKind::Protocol
        );
        client.close().unwrap();
        server.finish().unwrap();
    }
}

#[test]
fn capital_continuation_obeys_cancellation_and_empty_batches_do_no_io() {
    let token = CancellationToken::default();
    let cancel = token.clone();
    let requests = Arc::new(AtomicUsize::new(0));
    let observed = requests.clone();
    let server = hq_server::Server::start(move |request| {
        observed.fetch_add(1, Ordering::SeqCst);
        cancel.cancel();
        capital(&codes(request)[..1], 1)
    })
    .unwrap();
    let client = client(&server);
    assert!(client
        .corporate()
        .capital_changes(&[], &CapitalQuery::new(), &OperationOptions::default())
        .unwrap()
        .is_empty());
    assert!(client
        .corporate()
        .finance(&[], &FinanceQuery::new(), &OperationOptions::default())
        .unwrap()
        .is_empty());
    assert_eq!(server.connection_count(), 0);
    assert_eq!(
        client
            .corporate()
            .capital_changes(
                &ids(&["600000", "600001"]),
                &CapitalQuery::new(),
                &OperationOptions::default().with_cancellation(token)
            )
            .unwrap_err()
            .kind(),
        ErrorKind::Cancelled
    );
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn finance_preserves_duplicate_occurrences_original_bits_and_field_order() {
    let server = hq_server::Server::start(|request| {
        assert_eq!(request.msg_type, 0x0010);
        finance(&[("600000", 1.), ("600001", 2.), ("600000", 3.)])
    })
    .unwrap();
    let client = client(&server);
    let input = ids(&["600000", "600000", "600001"]);
    let batch = client
        .corporate()
        .finance(&input, &FinanceQuery::new(), &OperationOptions::default())
        .unwrap();
    let shares = |index: usize| {
        let BatchItemState::Present(row) = batch.items()[index].state() else {
            panic!()
        };
        assert_eq!(row.values().len(), 31);
        let FinanceValue::Shares(value) = row.value(FinanceField::CirculatingShares).unwrap()
        else {
            panic!()
        };
        assert_eq!(value.unit(), QuantityUnit::Shares);
        assert_eq!(value.source_unit(), QuantityUnit::TenThousandShares);
        assert!(matches!(value.source_value(), NumericValue::F32(_)));
        value.value().as_f64()
    };
    assert_eq!(
        [shares(0), shares(1), shares(2)],
        [10_000., 30_000., 20_000.]
    );
    let fields = [
        FinanceField::Reserved2,
        FinanceField::CirculatingShares,
        FinanceField::Reserved2,
    ];
    let sparse = client
        .corporate()
        .finance_fields(
            &input,
            &fields,
            &FinanceQuery::new(),
            &OperationOptions::default(),
        )
        .unwrap();
    let BatchItemState::Present(row) = sparse.items()[0].state() else {
        panic!()
    };
    assert_eq!(
        row.fields()
            .iter()
            .map(|(field, _)| *field)
            .collect::<Vec<_>>(),
        fields
    );
    let FinanceValue::Unclassified { bits, .. } = row.fields()[0].1 else {
        panic!()
    };
    assert_eq!(u32::from_le_bytes(bits), 0x7fc12345);
    let empty = client
        .corporate()
        .finance_fields(
            &input,
            &[],
            &FinanceQuery::new(),
            &OperationOptions::default(),
        )
        .unwrap();
    let BatchItemState::Present(empty_row) = empty.items()[0].state() else {
        panic!()
    };
    assert!(empty_row.fields().is_empty());
    assert_eq!(empty_row.id(), &input[0]);
    assert!(empty_row.updated().is_some());
    let BatchItemState::Present(full) = batch.items()[0].state() else {
        panic!()
    };
    let FinanceValue::Money(assets) = full.value(FinanceField::TotalAssets).unwrap() else {
        panic!()
    };
    let FinanceValue::Money(profit) = full.value(FinanceField::NetProfit).unwrap() else {
        panic!()
    };
    assert_eq!(assets.value().as_f64(), 8250.);
    assert_eq!(profit.value().as_f64(), 27250.);
    assert!(matches!(
        full.value(FinanceField::StateShares),
        Some(FinanceValue::Unclassified { .. })
    ));
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn finance_recovery_confirms_only_empty_singletons_and_preserves_missing_positions() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured = requests.clone();
    let server = hq_server::Server::start(move |request| {
        let numbers = codes(request);
        captured.lock().unwrap().push(numbers.clone());
        if numbers.len() != 1 || numbers[0] == "600001" {
            finance(&[])
        } else {
            finance(&[(&numbers[0], 4.)])
        }
    })
    .unwrap();
    let client = client(&server);
    let input = ids(&["600000", "600001"]);
    let batch = client
        .corporate()
        .finance(
            &input,
            &FinanceQuery::new()
                .with_missing(MissingPolicy::AllowPartial)
                .unwrap(),
            &OperationOptions::default(),
        )
        .unwrap();
    assert!(matches!(
        batch.items()[0].state(),
        BatchItemState::Present(_)
    ));
    assert!(matches!(
        batch.items()[1].state(),
        BatchItemState::ConfirmedMissing(_)
    ));
    assert_eq!(
        requests
            .lock()
            .unwrap()
            .iter()
            .map(Vec::len)
            .collect::<Vec<_>>(),
        vec![2, 1, 1]
    );
    assert_eq!(
        client
            .corporate()
            .finance(&input, &FinanceQuery::new(), &OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::MissingData
    );
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn finite_required_finance_failure_carries_wire_field_bits_and_source() {
    let server = hq_server::Server::start(|_| finance(&[("600000", f32::INFINITY)])).unwrap();
    let client = client(&server);
    let error = client
        .corporate()
        .finance(
            &ids(&["600000"]),
            &FinanceQuery::new(),
            &OperationOptions::default(),
        )
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Protocol);
    for key in ["field", "bits", "endpoint", "security"] {
        assert!(error.context().iter().any(|(found, _)| found == key));
    }
    // Sparse selection still decodes and validates the whole fixed record.
    assert_eq!(
        client
            .corporate()
            .finance_fields(
                &ids(&["600000"]),
                &[],
                &FinanceQuery::new(),
                &OperationOptions::default()
            )
            .unwrap_err()
            .kind(),
        ErrorKind::Protocol
    );
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn empty_capital_events_yield_empty_adjustment_series_and_keep_identity() {
    let server = hq_server::Server::start(|request| capital(&codes(request), 1)).unwrap();
    let client = client(&server);
    let factors = client
        .corporate()
        .adjustment_factors(
            &ids(&["600000"]),
            &FactorQuery::new(),
            &OperationOptions::default(),
        )
        .unwrap();
    let BatchItemState::Present(series) = factors.items()[0].state() else {
        panic!()
    };
    assert!(series.rows().is_empty());
    assert_eq!(series.id(), &ids(&["600000"])[0]);
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn finance_recovery_limit_is_shared_across_the_entire_operation() {
    let requests = Arc::new(AtomicUsize::new(0));
    let observed = requests.clone();
    let server = hq_server::Server::start(move |_| {
        observed.fetch_add(1, Ordering::SeqCst);
        finance(&[])
    })
    .unwrap();
    let client = client(&server);
    let input = (0..64)
        .map(|index| format!("sh{:06}", 600000 + index).parse().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        client
            .corporate()
            .finance(
                &input,
                &FinanceQuery::new()
                    .with_missing(MissingPolicy::AllowPartial)
                    .unwrap(),
                &OperationOptions::default()
            )
            .unwrap_err()
            .kind(),
        ErrorKind::LimitExceeded
    );
    assert!(
        requests.load(Ordering::SeqCst) <= 33,
        "one initial request plus at most32 recovery sends"
    );
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn malformed_finance_singleton_is_not_confirmed_absence() {
    let server = hq_server::Server::start(|_| vec![1]).unwrap();
    let client = client(&server);
    assert_eq!(
        client
            .corporate()
            .finance(
                &ids(&["600000"]),
                &FinanceQuery::new()
                    .with_missing(MissingPolicy::AllowPartial)
                    .unwrap(),
                &OperationOptions::default()
            )
            .unwrap_err()
            .kind(),
        ErrorKind::Protocol
    );
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn financial_unit_conversion_preserves_f32_bits_and_avoids_f32_multiply_overflow() {
    let raw = f32::MAX;
    let shares =
        eltdx::Quantity::shares_from_ten_thousands(raw, eltdx::types::ValueProvenance::WireRule)
            .unwrap();
    assert_eq!(shares.source_value().wire_f32_bits(), Some(raw.to_bits()));
    assert_eq!(shares.value().as_f64(), f64::from(raw) * 10_000.);
    let amount =
        eltdx::Amount::cny_from_thousands(raw, eltdx::types::ValueProvenance::WireRule).unwrap();
    assert_eq!(amount.source_value().wire_f32_bits(), Some(raw.to_bits()));
    assert_eq!(amount.source_unit(), eltdx::AmountUnit::ThousandCny);
    assert!(amount.value().as_f64().is_finite());
}
