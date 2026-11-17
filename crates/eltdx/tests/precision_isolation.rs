#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Source-level integration cases for operation-captured immutable precision.
//! The payload is synthetic; metadata owner/store installation belongs to T10.

use std::sync::{Arc, Barrier};

use bytes::Bytes;
use eltdx_protocol::commands::minutes::{TodayIntradayRequest, DEFAULT_TODAY_RESERVED_TAIL};
use eltdx_protocol::commands::security::{
    parse_security_list_payload_with_context, CODE_RECORD_SIZE,
};
use eltdx_protocol::unit::{Market, NormalizedCode};
use eltdx_protocol::{
    CommandRequest, CommandResponse, ParseOptions, ParseRawPolicy, PricePrecisionSource,
    ProtocolError,
};

#[derive(Clone)]
struct CapturedPrecision {
    code: NormalizedCode,
    decimal: u8,
}

impl PricePrecisionSource for CapturedPrecision {
    fn security_decimal(&self, code: &NormalizedCode) -> Option<u8> {
        (code == &self.code).then_some(self.decimal)
    }
}

fn decode_page(
    source: &CapturedPrecision,
    raw: ParseRawPolicy,
) -> Result<(i64, Bytes), ProtocolError> {
    // First record = price 1, average 2, volume 3; delta record = +4,+5,6.
    let payload = Bytes::from_static(&[2, 0, 0, 0, 1, 2, 3, 4, 5, 6]);
    let request = CommandRequest::TodayIntraday(TodayIntradayRequest::new(
        source.code.clone(),
        DEFAULT_TODAY_RESERVED_TAIL,
    ));
    let options = ParseOptions {
        price_precision: source,
        raw,
    };
    let response = CommandResponse::parse_with_context(request, payload.clone(), &options)?;
    let CommandResponse::TodayIntraday(page) = response else {
        panic!("expected minute page");
    };
    assert_eq!(page.points[0].price_raw, Some(1));
    assert_eq!(page.points[1].price_raw, Some(5));
    assert_eq!(page.points[0].metadata_decimal, Some(source.decimal));
    assert!(page.points.iter().all(|point| point.record_hex.is_empty()));
    assert_eq!(page.points[0].record_range.offset, 4);
    assert_eq!(page.points[1].record_range.offset, 7);
    if raw == ParseRawPolicy::RetainPayload {
        assert_eq!(page.raw_payload.as_ptr(), payload.as_ptr());
        assert_eq!(
            page.points[1]
                .record_range
                .bytes(&page.raw_payload)?
                .as_ref(),
            &[4, 5, 6]
        );
    } else {
        assert!(page.raw_payload.is_empty());
    }
    Ok((page.points[1].price_milli, page.raw_payload))
}

#[test]
fn interleaved_operation_snapshots_keep_their_precision_after_list_decode(
) -> Result<(), ProtocolError> {
    let code = NormalizedCode::parse("sh600996")?;
    let a = CapturedPrecision {
        code: code.clone(),
        decimal: 2,
    };
    let b = CapturedPrecision {
        code: code.clone(),
        decimal: 3,
    };
    let before = eltdx_protocol::unit::security_decimal(&code);
    for _ in 0..8 {
        assert_eq!(decode_page(&a, ParseRawPolicy::Omit)?.0, 50);
        assert_eq!(decode_page(&b, ParseRawPolicy::RetainPayload)?.0, 5);
        let mut record = [0_u8; CODE_RECORD_SIZE];
        record[..6].copy_from_slice(code.number().as_bytes());
        record[28] = 4;
        let mut payload = vec![1, 0];
        payload.extend_from_slice(&record);
        let options = ParseOptions {
            price_precision: &a,
            raw: ParseRawPolicy::Omit,
        };
        let rows = parse_security_list_payload_with_context(
            Bytes::from(payload),
            Market::Shanghai,
            &options,
        )?;
        assert_eq!(rows[0].decimal, 4);
        assert_eq!(eltdx_protocol::unit::security_decimal(&code), before);
        assert_eq!(decode_page(&b, ParseRawPolicy::Omit)?.0, 5);
        assert_eq!(decode_page(&a, ParseRawPolicy::RetainPayload)?.0, 50);
    }
    Ok(())
}

#[test]
fn concurrent_pages_share_each_operations_single_immutable_source(
) -> Result<(), Box<dyn std::error::Error>> {
    let code = NormalizedCode::parse("sh600996")?;
    let a = Arc::new(CapturedPrecision {
        code: code.clone(),
        decimal: 2,
    });
    let b = Arc::new(CapturedPrecision { code, decimal: 3 });
    let barrier = Arc::new(Barrier::new(2));
    std::thread::scope(|scope| -> Result<(), Box<dyn std::error::Error>> {
        let spawn = |source: Arc<CapturedPrecision>, barrier: Arc<Barrier>| {
            scope.spawn(move || -> Result<Vec<i64>, ProtocolError> {
                barrier.wait();
                (0..64)
                    .map(|_| decode_page(&source, ParseRawPolicy::Omit).map(|result| result.0))
                    .collect()
            })
        };
        let first = spawn(a, Arc::clone(&barrier));
        let second = spawn(b, barrier);
        let first = first
            .join()
            .map_err(|_| std::io::Error::other("first parser thread panicked"))??;
        let second = second
            .join()
            .map_err(|_| std::io::Error::other("second parser thread panicked"))??;
        assert_eq!(first, vec![50; 64]);
        assert_eq!(second, vec![5; 64]);
        Ok(())
    })
}
