//! Synthetic wire layouts: these are independent parser contract examples,
//! not captures or proof that an unverified command is deployed by a server.

use super::*;
use crate::commands::{corporate::*, klines::*, limits::*, minutes::*, quotes::*, security::*, trades::*};
use crate::unit::{DateParts, KlinePeriod, Market, NormalizedCode, AdjustMode};
use crate::{CommandRequest, CommandResponse};

struct Decimal(Option<u8>);
impl PricePrecisionSource for Decimal {
    fn security_decimal(&self, _: &NormalizedCode) -> Option<u8> { self.0 }
}

fn options(source: &dyn PricePrecisionSource, raw: ParseRawPolicy) -> ParseOptions<'_> {
    ParseOptions { price_precision: source, raw }
}

fn varints(out: &mut Vec<u8>, values: &[i64]) {
    for value in values {
        let mut magnitude = value.unsigned_abs();
        let mut first = (magnitude & 0x3f) as u8;
        if *value < 0 { first |= 0x40; }
        magnitude >>= 6;
        if magnitude != 0 { first |= 0x80; }
        out.push(first);
        while magnitude != 0 {
            let mut byte = (magnitude & 0x7f) as u8;
            magnitude >>= 7;
            if magnitude != 0 { byte |= 0x80; }
            out.push(byte);
        }
    }
}

fn marker(code: &NormalizedCode) -> Vec<u8> {
    let mut data = vec![code.market().id()];
    data.extend_from_slice(code.number().as_bytes());
    data.extend_from_slice(&7_u16.to_le_bytes());
    data
}

fn quote_payload(code: &NormalizedCode, kind: u8) -> Bytes {
    let mut record = marker(code);
    varints(&mut record, &[125_528, -11, 3, 5, -7]);
    if kind == 3 {
        record.extend_from_slice(&0x1122_3344_u32.to_le_bytes());
        varints(&mut record, &[17, 41, 43]);
    } else {
        varints(&mut record, &[123, 17, 41, 43]);
    }
    record.extend_from_slice(&0x1234_5678_u32.to_le_bytes());
    varints(&mut record, &[47, 53, 59, 61]);
    let levels = if kind == 1 || kind == 3 { 5 } else { 1 };
    for _ in 0..levels { varints(&mut record, &[2, 4, 7, 9]); }
    match kind {
        1 => {
            record.extend_from_slice(&0x1234_u16.to_le_bytes());
            varints(&mut record, &[13, 17, 19, 23]);
        }
        2 => {
            let mut tail = [0x5a; 56];
            tail[6..10].copy_from_slice(&0x8000_0000_u32.to_le_bytes());
            record.extend_from_slice(&tail);
        }
        _ => record.extend_from_slice(&[0xde, 0xad]),
    }
    let mut payload = if kind == 3 { vec![1, 0] } else { vec![0, 0, 1, 0] };
    payload.extend_from_slice(&record);
    if kind == 3 { for byte in &mut payload { *byte ^= 0x93; } }
    Bytes::from(payload)
}

fn quote_request(code: NormalizedCode, kind: u8) -> Result<CommandRequest, ProtocolError> {
    Ok(match kind {
        0 => CommandRequest::Snapshots(SnapshotsRequest::new(vec![code])?),
        1 => CommandRequest::LegacyQuotes(LegacyQuotesRequest::new(vec![code])?),
        2 => CommandRequest::CategoryQuotes(CategoryQuotesRequest::new(6, 0, 0, 1, false, None, 0)),
        _ => CommandRequest::RefreshStream(RefreshStreamRequest::new(vec![RefreshCursor { code, cursor: 0 }])?),
    })
}

fn raw_prices(response: &CommandResponse) -> QuotePriceRaw {
    match response {
        CommandResponse::Snapshots(rows) => rows[0].price_raw,
        CommandResponse::LegacyQuotes(rows) => rows[0].price_raw,
        CommandResponse::CategoryQuotes(page) => page.records[0].price_raw,
        CommandResponse::RefreshStream(page) => page.records[0].price_raw,
        _ => panic!("expected synthetic quote"),
    }
}

#[test]
fn context_all_nested_quote_branches_keep_exact_prices_and_ranges() -> Result<(), ProtocolError> {
    let code = NormalizedCode::parse("sh600997")?;
    for kind in 0..4 {
        let payload = quote_payload(&code, kind);
        let request = quote_request(code.clone(), kind)?;
        let first = CommandResponse::parse_with_context(request.clone(), payload.clone(), &options(&Decimal(Some(2)), ParseRawPolicy::Omit))?;
        let second = CommandResponse::parse_with_context(request, payload.clone(), &options(&Decimal(Some(4)), ParseRawPolicy::RetainPayload))?;
        let a = raw_prices(&first);
        let b = raw_prices(&second);
        assert_eq!(a.current, b.current);
        assert_eq!(a.last_close, b.last_close);
        assert_eq!(a.divisor, 1);
        assert_eq!(b.divisor, 100);
        assert_eq!(a.metadata_decimal, Some(2));
        assert_eq!(b.metadata_decimal, Some(4));
        assert_eq!(a.current, if kind == 0 || kind == 3 { 1_255_280 } else { 125_528 });
        let range = match (&first, &second) {
            (CommandResponse::Snapshots(a), CommandResponse::Snapshots(b)) => {
                assert_eq!(a[0].unknown_after_time_raw, b[0].unknown_after_time_raw);
                assert_eq!(a[0].amount_raw, 0x1234_5678);
                assert_eq!(b[0].tail_raw.as_ref(), &[0xde, 0xad]);
                assert_eq!(a[0].last_price, 1255.28);
                assert_eq!(b[0].last_price, 12.552);
                b[0].record_range
            }
            (CommandResponse::LegacyQuotes(a), CommandResponse::LegacyQuotes(b)) => {
                assert!(a[0].record_hex.is_empty() && b[0].record_hex.is_empty());
                assert_eq!(a[0].tail_metrics_raw, [13, 17, 19, 23]);
                assert_eq!(a[0].tail_metrics_raw, b[0].tail_metrics_raw);
                b[0].record_range
            }
            (CommandResponse::CategoryQuotes(a), CommandResponse::CategoryQuotes(b)) => {
                assert!(a.raw_payload.is_empty());
                assert_eq!(b.raw_payload.as_ptr(), payload.as_ptr());
                assert!(a.records[0].record_hex.is_empty() && b.records[0].record_hex.is_empty());
                assert_eq!(a.records[0].tail_raw, b.records[0].tail_raw);
                assert_eq!(a.records[0].min2_amount.to_bits(), 0x8000_0000);
                assert_eq!(a.records[0].min2_amount.to_bits(), b.records[0].min2_amount.to_bits());
                assert_eq!(b.records[0].bid1_delta_raw, 2);
                b.records[0].record_range
            }
            (CommandResponse::RefreshStream(a), CommandResponse::RefreshStream(b)) => {
                assert!(a.raw_payload.is_empty() && a.decoded_payload.is_empty());
                assert!(b.decoded_payload.is_empty());
                assert_eq!(b.raw_payload.as_ptr(), payload.as_ptr());
                assert!(a.records[0].record_hex.is_empty() && b.records[0].record_hex.is_empty());
                assert_eq!(a.records[0].update_time_raw, 0x1122_3344);
                assert_eq!(a.records[0].tail_raw, b.records[0].tail_raw);
                assert_eq!(b.records[0].record_range.encoding, RecordEncoding::RefreshXor93);
                b.records[0].record_range
            }
            _ => panic!("mismatched synthetic quote variants"),
        };
        let bytes = range.bytes(&payload)?;
        assert_eq!(&bytes[..7], &marker(&code)[..7]);
        assert_eq!(range.offset, if kind == 3 { 2 } else { 4 });
        assert_eq!(range.len, payload.len() - range.offset);
        assert_eq!(range.hex(&payload)?.len(), range.len * 2);
    }
    Ok(())
}

#[test]
fn context_security_list_is_read_only_but_legacy_wrapper_registers() -> Result<(), ProtocolError> {
    let code = NormalizedCode::parse("sh601997")?;
    let before = crate::unit::security_decimal(&code);
    let mut record = [0_u8; CODE_RECORD_SIZE];
    record[..6].copy_from_slice(code.number().as_bytes());
    record[28] = 4;
    record[24..28].copy_from_slice(&0x8000_0000_u32.to_le_bytes());
    record[29..33].copy_from_slice(&0x3f80_0001_u32.to_le_bytes());
    record[33..37].copy_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
    let mut data = vec![1, 0];
    data.extend_from_slice(&record);
    let payload = Bytes::from(data);
    let rows = parse_security_list_payload_with_context(payload.clone(), Market::Shanghai, &options(&Decimal(None), ParseRawPolicy::Omit))?;
    assert_eq!(crate::unit::security_decimal(&code), before);
    assert_eq!(rows[0].volume_ratio_base.to_bits(), 0x8000_0000);
    assert_eq!(rows[0].previous_close_price.to_bits(), 0x3f80_0001);
    assert_eq!(rows[0].unknown3_raw, [0xde, 0xad, 0xbe, 0xef]);
    assert_eq!(rows[0].record_range.bytes(&payload)?.as_ref(), record.as_slice());
    let legacy = parse_security_list_payload(&payload, Market::Shanghai)?;
    assert_eq!(legacy[0].decimal, 4);
    assert_eq!(crate::unit::security_decimal(&code), Some(4));
    // A populated legacy cache must not satisfy a missing explicit resolver.
    let result = CommandResponse::parse_with_context(quote_request(code.clone(), 0)?, quote_payload(&code, 0), &options(&Decimal(None), ParseRawPolicy::Omit));
    assert!(matches!(result, Err(ProtocolError::InvalidData { context: "price precision", .. })));
    Ok(())
}

#[test]
fn context_three_minute_branches_keep_snapshot_scale_and_float_bits() -> Result<(), ProtocolError> {
    let code = NormalizedCode::parse("sh600997")?;
    let date = DateParts::new(2026, 8, 14)?;
    for kind in 0..3 {
        let request = match kind {
            0 => CommandRequest::TodayIntraday(TodayIntradayRequest::new(code.clone(), DEFAULT_TODAY_RESERVED_TAIL)),
            1 => CommandRequest::HistoricalIntraday(HistoricalIntradayRequest::new(code.clone(), date)?),
            _ => CommandRequest::RecentIntraday(RecentIntradayRequest::new(code.clone(), date)?),
        };
        let mut data = vec![1, 0];
        match kind {
            0 => data.extend_from_slice(&[0, 0]),
            1 => data.extend_from_slice(&0x8000_0000_u32.to_le_bytes()),
            _ => {
                data.extend_from_slice(&0x8000_0000_u32.to_le_bytes());
                data.extend_from_slice(&0x3f80_0001_u32.to_le_bytes());
            }
        }
        varints(&mut data, &[125_528, -3, 11]);
        let payload = Bytes::from(data);
        let a = CommandResponse::parse_with_context(request.clone(), payload.clone(), &options(&Decimal(Some(2)), ParseRawPolicy::Omit))?;
        let b = CommandResponse::parse_with_context(request, payload.clone(), &options(&Decimal(Some(4)), ParseRawPolicy::RetainPayload))?;
        macro_rules! compare {
            ($a:expr, $b:expr) => {{
                let (a, b) = ($a, $b);
                assert!(a.raw_payload.is_empty());
                assert_eq!(b.raw_payload.as_ptr(), payload.as_ptr());
                assert_eq!(a.points[0].price_raw, Some(125_528));
                assert_eq!(a.points[0].price_raw, b.points[0].price_raw);
                assert_eq!(a.points[0].price_milli, 1_255_280);
                assert_eq!(b.points[0].price_milli, 12_552);
                assert_eq!(b.points[0].price_divisor, 100);
                assert!(a.points[0].record_hex.is_empty() && b.points[0].record_hex.is_empty());
                assert_eq!(b.points[0].record_range.bytes(&payload)?.len(), payload.len() - b.points[0].record_range.offset);
                if let Some(value) = a.prev_close { assert_eq!(value.to_bits(), 0x8000_0000); }
                assert_eq!(a.prev_close_raw, b.prev_close_raw);
                assert_eq!(a.open_price_raw, b.open_price_raw);
            }};
        }
        match (a, b) {
            (CommandResponse::TodayIntraday(a), CommandResponse::TodayIntraday(b)) => compare!(a, b),
            (CommandResponse::HistoricalIntraday(a), CommandResponse::HistoricalIntraday(b)) => compare!(a, b),
            (CommandResponse::RecentIntraday(a), CommandResponse::RecentIntraday(b)) => compare!(a, b),
            _ => panic!("expected matching minute series"),
        }
    }
    Ok(())
}

#[test]
fn context_raw_omit_preserves_f32_bits_unknowns_and_legacy_hex() -> Result<(), ProtocolError> {
    let mut data = vec![1, 0, 1];
    data.extend_from_slice(&600_997_u32.to_le_bytes());
    data.extend_from_slice(&0x7fc0_1234_u32.to_le_bytes());
    data.extend_from_slice(&0x8000_0000_u32.to_le_bytes());
    let payload = Bytes::from(data);
    let source = Decimal(None);
    let omit = parse_special_limits_payload_with_context(payload.clone(), SpecialLimitsRequest::new(0), &options(&source, ParseRawPolicy::Omit))?;
    let retained = parse_special_limits_payload_with_context(payload.clone(), SpecialLimitsRequest::new(0), &options(&source, ParseRawPolicy::RetainPayload))?;
    let legacy = parse_special_limits_payload(&payload, SpecialLimitsRequest::new(0))?;
    assert!(omit.raw_payload.is_empty());
    assert_eq!(retained.raw_payload.as_ptr(), payload.as_ptr());
    assert_eq!(omit.records[0].upper_price_raw, retained.records[0].upper_price_raw);
    assert_eq!(omit.records[0].upper_price_raw_f32.to_bits(), 0x7fc0_1234);
    assert_eq!(omit.records[0].lower_price_raw_f32.to_bits(), 0x8000_0000);
    assert_eq!(omit.records[0].upper_price_raw_f32.to_bits(), legacy.records[0].upper_price_raw_f32.to_bits());
    assert!(omit.records[0].record_hex.is_empty() && retained.records[0].record_hex.is_empty());
    assert_eq!(retained.records[0].record_range.hex(&payload)?, legacy.records[0].record_hex);
    Ok(())
}

#[test]
fn context_tick_milli_yuan_does_not_consult_quote_precision() -> Result<(), ProtocolError> {
    let mut data = vec![1, 0];
    data.extend_from_slice(&571_u16.to_le_bytes());
    varints(&mut data, &[125_528, 11, 13, 17, -19]);
    let payload = Bytes::from(data);
    let request = TodayTicksRequest::new(NormalizedCode::parse("sh118075")?, 0, 1)?;
    let omit = parse_today_ticks_payload_with_context(payload.clone(), request.clone(), &options(&Decimal(None), ParseRawPolicy::Omit))?;
    let retained = parse_today_ticks_payload_with_context(payload.clone(), request.clone(), &options(&Decimal(Some(4)), ParseRawPolicy::RetainPayload))?;
    let legacy = parse_today_ticks_payload(&payload, request)?;
    assert_eq!(omit.ticks[0].price, 125.528);
    assert_eq!(omit.ticks[0].price_milli, 125_528);
    assert_eq!(omit.ticks[0].price_denominator, 1000);
    assert_eq!(omit.ticks[0].unknown_tail_raw, Some(-19));
    assert_eq!(omit.ticks[0].price_delta_raw, retained.ticks[0].price_delta_raw);
    assert!(omit.ticks[0].record_hex.is_empty() && retained.ticks[0].record_hex.is_empty());
    assert_eq!(retained.raw_payload.as_ptr(), payload.as_ptr());
    assert_eq!(retained.ticks[0].record_range.hex(&payload)?.as_str(), legacy.ticks[0].record_hex.as_ref());
    Ok(())
}

#[test]
fn context_corporate_keeps_native_float_bytes_without_diagnostic_payload() -> Result<(), ProtocolError> {
    let code = NormalizedCode::parse("sh600997")?;
    let mut record = [0_u8; CAPITAL_CHANGE_RECORD_SIZE];
    record[0] = code.market().id();
    record[1..7].copy_from_slice(code.number().as_bytes());
    record[7] = 0x5a;
    record[8..12].copy_from_slice(&20_260_814_u32.to_le_bytes());
    record[12] = 0xfe; // Unknown category stays unknown.
    record[13..17].copy_from_slice(&0x7fc0_1234_u32.to_le_bytes());
    let mut data = vec![1, 0];
    data.extend_from_slice(&marker(&code)[..7]);
    data.extend_from_slice(&1_u16.to_le_bytes());
    data.extend_from_slice(&record);
    let payload = Bytes::from(data);
    let request = CapitalChangesRequest::new(code);
    let omit = parse_capital_changes_payload_with_context(payload.clone(), request.clone(), &options(&Decimal(None), ParseRawPolicy::Omit))?;
    let retained = parse_capital_changes_payload_with_context(payload.clone(), request.clone(), &options(&Decimal(None), ParseRawPolicy::RetainPayload))?;
    let legacy = parse_capital_changes_payload(&payload, request)?;
    let row = &omit.blocks[0].records[0];
    assert_eq!(row.reserved_7, 0x5a);
    assert_eq!(row.category_name, None);
    assert_eq!(row.c1_float.to_bits(), 0x7fc0_1234);
    assert_eq!(row.c1_raw, retained.blocks[0].records[0].c1_raw);
    assert!(row.record_hex.is_empty() && omit.raw_payload.is_empty());
    assert_eq!(retained.raw_payload.as_ptr(), payload.as_ptr());
    assert_eq!(row.record_range.hex(&payload)?, legacy.blocks[0].records[0].record_hex);
    Ok(())
}

#[test]
fn context_history_flow_keeps_original_custom_float_words() -> Result<(), ProtocolError> {
    let code = NormalizedCode::parse("sh600997")?;
    let request = KlinesRequest::new(code, KlinePeriod::new(22, 1), 0, 1, AdjustMode::None, 0, KlineKind::Stock)?;
    let mut data = vec![0x5a; 9];
    data.extend_from_slice(&1_u16.to_le_bytes());
    data.extend_from_slice(&20_260_814_u32.to_le_bytes());
    let words = [0, 0x8000_0000, 0x1234_5678, 0x7fc0_1234, 4, 5, 6, 7];
    for word in words { data.extend_from_slice(&u32::to_le_bytes(word)); }
    let payload = Bytes::from(data);
    let result = CommandResponse::parse_with_context(CommandRequest::Klines(request), payload.clone(), &options(&Decimal(None), ParseRawPolicy::Omit))?;
    let CommandResponse::HistoryFundFlow(series) = result else { panic!("category 22 must route to flow"); };
    assert!(series.raw_payload.is_empty());
    assert_eq!(series.records[0].amounts_raw, words);
    assert_eq!(series.records[0].record_range.offset, 11);
    assert_eq!(series.records[0].record_range.len, 36);
    assert!(series.records[0].record_hex.is_empty());
    Ok(())
}

#[test]
fn context_record_ranges_reject_overflow_and_unavailable_payload() {
    let range = RecordRange { offset: usize::MAX, len: 2, encoding: RecordEncoding::Payload };
    assert!(range.bytes(&Bytes::new()).is_err());
    let range = RecordRange { offset: 1, len: 1, encoding: RecordEncoding::RefreshXor93 };
    assert!(range.hex(&Bytes::new()).is_err());
}
