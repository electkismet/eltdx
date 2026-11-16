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
    assert_eq!(series.prefix_raw, Some([0x5a; 9]));
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

#[test]
fn context_finance_native_info_is_semantic_and_shared_only_when_retained() -> Result<(), ProtocolError> {
    let code = NormalizedCode::parse("sh600997")?;
    let mut record = [0_u8; FINANCE_RECORD_SIZE];
    record[..7].copy_from_slice(&marker(&code)[..7]);
    record[7..11].copy_from_slice(&0x7fc0_1234_u32.to_le_bytes());
    record[11..13].copy_from_slice(&0xabcd_u16.to_le_bytes());
    record[139..143].copy_from_slice(&0x8000_0000_u32.to_le_bytes());
    let mut data = vec![1, 0];
    data.extend_from_slice(&record);
    let payload = Bytes::from(data);
    let request = FinanceBatchRequest::new(vec![code])?;
    let omit = parse_finance_batch_payload_with_context(payload.clone(), request.clone(), &options(&Decimal(None), ParseRawPolicy::Omit))?;
    let retained = parse_finance_batch_payload_with_context(payload.clone(), request.clone(), &options(&Decimal(None), ParseRawPolicy::RetainPayload))?;
    let legacy = parse_finance_batch_payload(&payload, request)?;
    assert_eq!(omit.records[0].liu_tong_gu_ben_raw_float.to_bits(), 0x7fc0_1234);
    assert_eq!(omit.records[0].bao_liu_2_raw_float.to_bits(), 0x8000_0000);
    assert_eq!(omit.records[0].province_raw, 0xabcd);
    assert_eq!(omit.records[0].finance_info_raw, retained.records[0].finance_info_raw);
    assert_eq!(retained.records[0].finance_info_raw.as_ptr(), payload[9..].as_ptr());
    assert_ne!(omit.records[0].finance_info_raw.as_ptr(), payload[9..].as_ptr());
    assert!(omit.raw_payload.is_empty() && omit.records[0].record_hex.is_empty());
    assert_eq!(retained.raw_payload.as_ptr(), payload.as_ptr());
    assert_eq!(retained.records[0].record_range.hex(&payload)?, legacy.records[0].record_hex);
    Ok(())
}

#[test]
fn context_klines_and_delisted_keep_integer_prices_and_unknown_suffix() -> Result<(), ProtocolError> {
    use crate::commands::delisted_bars::*;
    let code = NormalizedCode::parse("sh600997")?;
    let request = KlinesRequest::new(code.clone(), KlinePeriod::new(4, 1), 0, 1, AdjustMode::None, 0, KlineKind::Stock)?;
    let mut data = vec![1, 0];
    data.extend_from_slice(&20_260_814_u32.to_le_bytes());
    varints(&mut data, &[125_528, 3, 5, -7]);
    data.extend_from_slice(&0x1234_5678_u32.to_le_bytes());
    data.extend_from_slice(&0x8765_4321_u32.to_le_bytes());
    let payload = Bytes::from(data);
    let omit = parse_klines_payload_with_context(payload.clone(), request.clone(), &options(&Decimal(None), ParseRawPolicy::Omit))?;
    let retained = parse_klines_payload_with_context(payload.clone(), request.clone(), &options(&Decimal(None), ParseRawPolicy::RetainPayload))?;
    let legacy = parse_klines_payload(&payload, request)?;
    assert_eq!(omit.bars[0].open_price_milli, 125_528);
    assert_eq!(omit.bars[0].time_raw, 20_260_814_u32.to_le_bytes());
    assert_eq!(omit.bars[0].amount_raw, 0x8765_4321);
    assert_eq!(omit.bars[0].volume_raw, retained.bars[0].volume_raw);
    assert!(omit.raw_payload.is_empty() && omit.bars[0].record_hex.is_empty());
    assert_eq!(retained.raw_payload.as_ptr(), payload.as_ptr());
    assert_eq!(retained.bars[0].record_range.hex(&payload)?, legacy.bars[0].record_hex);

    let mut data = vec![1, 0];
    data.extend_from_slice(&20_260_814_u32.to_le_bytes());
    varints(&mut data, &[125_528, 3, 5, -7, 1234]);
    data.extend_from_slice(&0x8765_4321_u32.to_le_bytes());
    data.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
    let payload = Bytes::from(data);
    let request = DelistedKlinesRequest::new(code, 0, 1, false)?;
    let omit = parse_delisted_klines_payload_with_context(payload.clone(), request.clone(), &options(&Decimal(None), ParseRawPolicy::Omit))?;
    let retained = parse_delisted_klines_payload_with_context(payload.clone(), request.clone(), &options(&Decimal(None), ParseRawPolicy::RetainPayload))?;
    let legacy = parse_delisted_klines_payload(&payload, request)?;
    assert_eq!(omit.bars[0].unknown_suffix_raw, Some([0xde, 0xad, 0xbe, 0xef]));
    assert_eq!(omit.bars[0].unknown_suffix_raw, retained.bars[0].unknown_suffix_raw);
    assert_eq!(omit.bars[0].volume_raw, 1234);
    assert!(omit.raw_payload.is_empty() && omit.bars[0].record_hex.is_empty());
    assert_eq!(retained.raw_payload.as_ptr(), payload.as_ptr());
    assert_eq!(retained.bars[0].record_range.hex(&payload)?, legacy.bars[0].record_hex);
    Ok(())
}

#[test]
fn context_auction_and_money_flow_keep_wire_bits_and_unknown_header() -> Result<(), ProtocolError> {
    use crate::commands::{auctions::*, money_flow::*};
    let code = NormalizedCode::parse("sh600997")?;
    let mut data = vec![1, 0];
    data.extend_from_slice(&571_u16.to_le_bytes());
    data.extend_from_slice(&0x42fb_0e56_u32.to_le_bytes());
    data.extend_from_slice(&123_u32.to_le_bytes());
    data.extend_from_slice(&(-47_i32).to_le_bytes());
    data.extend_from_slice(&[0x5a, 17]);
    let payload = Bytes::from(data);
    let request = AuctionSeriesRequest::with_defaults(code.clone());
    let omit = parse_auction_series_payload_with_context(payload.clone(), request.clone(), &options(&Decimal(None), ParseRawPolicy::Omit))?;
    let retained = parse_auction_series_payload_with_context(payload.clone(), request.clone(), &options(&Decimal(None), ParseRawPolicy::RetainPayload))?;
    let legacy = parse_auction_series_payload(&payload, request)?;
    assert_eq!(omit.points[0].price.to_bits(), 0x42fb_0e56);
    assert_eq!(omit.points[0].price_raw, retained.points[0].price_raw);
    assert_eq!(omit.points[0].unmatched_signed_raw, -47);
    assert_eq!(omit.points[0].reserved_zero_0e, 0x5a);
    assert_eq!(omit.points[0].second_raw, 17);
    assert!(omit.raw_payload.is_empty() && omit.points[0].record_hex.is_empty());
    assert_eq!(retained.raw_payload.as_ptr(), payload.as_ptr());
    assert_eq!(retained.points[0].record_range.hex(&payload)?, legacy.points[0].record_hex);

    let mut header = [0x5a; MONEY_FLOW_HEADER_SIZE];
    header[0] = code.market().id();
    header[2..8].copy_from_slice(code.number().as_bytes());
    header[0x26..0x28].copy_from_slice(&1_u16.to_le_bytes());
    let mut record = [0_u8; MONEY_FLOW_RECORD_SIZE];
    record[..4].copy_from_slice(&20_260_814_u32.to_le_bytes());
    record[8..12].copy_from_slice(&0x8000_0000_u32.to_le_bytes());
    record[84..88].copy_from_slice(&0x7fc0_1234_u32.to_le_bytes());
    let mut data = header.to_vec();
    data.extend_from_slice(&record);
    let payload = Bytes::from(data);
    let request = MoneyFlowRequest::new(code);
    let omit = parse_money_flow_payload_with_context(payload.clone(), request.clone(), &options(&Decimal(None), ParseRawPolicy::Omit))?;
    let retained = parse_money_flow_payload_with_context(payload.clone(), request.clone(), &options(&Decimal(None), ParseRawPolicy::RetainPayload))?;
    let legacy = parse_money_flow_payload(&payload, request)?;
    assert_eq!(omit.blocks[0].header_raw, header);
    assert_eq!(omit.blocks[0].records[0].raw, retained.blocks[0].records[0].raw);
    assert_eq!(omit.blocks[0].records[0].raw[20], 0x7fc0_1234);
    assert_eq!(omit.blocks[0].records[0].total_amount.to_bits(), 0x8000_0000);
    assert!(omit.raw_payload.is_empty() && omit.blocks[0].records[0].record_hex.is_empty());
    assert_eq!(retained.raw_payload.as_ptr(), payload.as_ptr());
    assert_eq!(retained.blocks[0].records[0].record_range.hex(&payload)?, legacy.blocks[0].records[0].record_hex);
    Ok(())
}

#[test]
fn context_file_bytes_are_business_data_while_trailing_diagnostics_are_optional() -> Result<(), ProtocolError> {
    use crate::commands::resources::*;
    let request = FileContentRequest::new("synthetic.dat", 0, 3)?;
    let payload = Bytes::from_static(&[3, 0, 0, 0, 0xde, 0xad, 0xbe, 0x5a, 0x5b]);
    let omit = parse_file_content_payload_with_context(payload.clone(), request.clone(), &options(&Decimal(None), ParseRawPolicy::Omit))?;
    let retained = parse_file_content_payload_with_context(payload.clone(), request.clone(), &options(&Decimal(None), ParseRawPolicy::RetainPayload))?;
    let legacy = parse_file_content_payload(&payload, request)?;
    assert_eq!(omit.content, retained.content);
    assert_eq!(omit.content.as_ref(), &[0xde, 0xad, 0xbe]);
    assert_ne!(omit.content.as_ptr(), payload[4..].as_ptr());
    assert_eq!(retained.content.as_ptr(), payload[4..].as_ptr());
    assert!(omit.raw_payload.is_empty() && omit.trailing_payload.is_empty());
    assert_eq!(retained.trailing_payload, legacy.trailing_payload);
    assert_eq!(retained.raw_payload.as_ptr(), payload.as_ptr());
    assert_eq!(retained.content_range.bytes(&payload)?, retained.content);
    Ok(())
}

#[test]
fn context_unrepresentable_precision_never_uses_a_saturated_divisor() -> Result<(), ProtocolError> {
    let code = NormalizedCode::parse("sh600997")?;
    let result = CommandResponse::parse_with_context(quote_request(code.clone(), 0)?, quote_payload(&code, 0), &options(&Decimal(Some(255)), ParseRawPolicy::Omit));
    assert!(matches!(result, Err(ProtocolError::InvalidData { context: "price precision", .. })));
    Ok(())
}

#[test]
fn context_native_minute_aux_and_sparkline_fields_keep_their_bits() -> Result<(), ProtocolError> {
    let code = NormalizedCode::parse("sh600997")?;
    let payload = Bytes::from_static(&[1, 0, 0x34, 0x12, 0xc0, 0x7f, 0, 0, 0, 0x80]);
    let request = IntradayAuxRequest::new(code.clone(), IntradayAuxKind::VolumeComparison);
    let omit = parse_intraday_aux_payload_with_context(payload.clone(), request.clone(), &options(&Decimal(None), ParseRawPolicy::Omit))?;
    let retained = parse_intraday_aux_payload_with_context(payload.clone(), request, &options(&Decimal(None), ParseRawPolicy::RetainPayload))?;
    let MinuteAuxPoint::VolumeComparison { previous_day_cumulative_volume, current_day_cumulative_volume, previous_day_raw, record_hex, .. } = &omit.points[0] else { panic!("expected volume comparison"); };
    assert_eq!(previous_day_cumulative_volume.to_bits(), 0x7fc0_1234);
    assert_eq!(current_day_cumulative_volume.to_bits(), 0x8000_0000);
    assert_eq!(*previous_day_raw, [0x34, 0x12, 0xc0, 0x7f]);
    assert!(record_hex.is_empty() && omit.raw_payload.is_empty());
    assert_eq!(retained.raw_payload.as_ptr(), payload.as_ptr());
    assert_eq!(retained.record_ranges[0].bytes(&payload)?.as_ref(), &payload[2..]);

    let mut data = vec![0_u8; 46];
    data[0] = code.market().id();
    data[2..8].copy_from_slice(code.number().as_bytes());
    data[24] = 1;
    data[26..30].copy_from_slice(&0xdead_beef_u32.to_le_bytes());
    data[36..40].copy_from_slice(&0x8000_0000_u32.to_le_bytes());
    data[40..42].copy_from_slice(&1_u16.to_le_bytes());
    data[42..46].copy_from_slice(&0x7fc0_1234_u32.to_le_bytes());
    let payload = Bytes::from(data);
    let request = SparklineRequest::new(code, 1, 20, DEFAULT_SPARKLINE_FIXED_RAW);
    let omit = parse_sparkline_payload_with_context(payload.clone(), request.clone(), &options(&Decimal(None), ParseRawPolicy::Omit))?;
    let retained = parse_sparkline_payload_with_context(payload.clone(), request, &options(&Decimal(None), ParseRawPolicy::RetainPayload))?;
    assert_eq!(omit.reserved_param_u32, 0xdead_beef);
    assert_eq!(omit.base_price.to_bits(), 0x8000_0000);
    assert_eq!(omit.prices[0].value.to_bits(), 0x7fc0_1234);
    assert_eq!(omit.prices[0].raw, retained.prices[0].raw);
    assert!(omit.raw_payload.is_empty());
    assert_eq!(retained.raw_payload.as_ptr(), payload.as_ptr());
    Ok(())
}

#[test]
fn context_historical_ticks_keep_native_base_bits_and_reserved_scalar() -> Result<(), ProtocolError> {
    let mut data = vec![1, 0];
    data.extend_from_slice(&0x7fc0_1234_u32.to_le_bytes());
    data.extend_from_slice(&571_u16.to_le_bytes());
    varints(&mut data, &[125_528, 11, 13, 17, -19]);
    let payload = Bytes::from(data);
    let request = HistoricalTicksRequest::new(NormalizedCode::parse("sh118075")?, DateParts::new(2026, 8, 14)?, 0, 1)?;
    let omit = parse_historical_ticks_payload_with_context(payload.clone(), request.clone(), &options(&Decimal(None), ParseRawPolicy::Omit))?;
    let retained = parse_historical_ticks_payload_with_context(payload.clone(), request, &options(&Decimal(Some(4)), ParseRawPolicy::RetainPayload))?;
    assert_eq!(omit.price_base_raw_f32.map(f32::to_bits), Some(0x7fc0_1234));
    assert_eq!(omit.price_base_raw_bytes, Some([0x34, 0x12, 0xc0, 0x7f]));
    assert_eq!(omit.ticks[0].reserved_zero, Some(-19));
    assert_eq!(omit.ticks[0].price_acc_raw, retained.ticks[0].price_acc_raw);
    assert!(omit.raw_payload.is_empty() && omit.ticks[0].record_hex.is_empty());
    assert_eq!(retained.raw_payload.as_ptr(), payload.as_ptr());
    Ok(())
}

#[test]
fn context_handshake_keeps_small_unknown_fields_without_root_payload() -> Result<(), ProtocolError> {
    use crate::commands::session::*;
    let mut data = vec![0_u8; HANDSHAKE_PAYLOAD_MIN_SIZE];
    data[42..46].copy_from_slice(&20_260_814_u32.to_le_bytes());
    data[46..50].copy_from_slice(&0xdead_beef_u32.to_le_bytes());
    data[50..54].copy_from_slice(&20_260_814_u32.to_le_bytes());
    data[58..68].copy_from_slice(&[0x5a; 10]);
    data[152..160].copy_from_slice(&[0xa5; 8]);
    let payload = Bytes::from(data);
    let omit = parse_handshake_payload_with_context(payload.clone(), &options(&Decimal(None), ParseRawPolicy::Omit))?;
    let retained = parse_handshake_payload_with_context(payload.clone(), &options(&Decimal(None), ParseRawPolicy::RetainPayload))?;
    let legacy = parse_handshake_payload(&payload)?;
    assert_eq!(omit.unknown_time_1_raw, 0xdead_beef);
    assert_eq!(omit.flags_raw, retained.flags_raw);
    assert_eq!(omit.tail_control_raw, legacy.tail_control_raw);
    assert_ne!(omit.flags_raw.as_ptr(), payload[58..].as_ptr());
    assert_eq!(retained.flags_raw.as_ptr(), payload[58..].as_ptr());
    assert!(omit.raw_payload.is_empty());
    assert_eq!(retained.raw_payload.as_ptr(), payload.as_ptr());
    Ok(())
}
