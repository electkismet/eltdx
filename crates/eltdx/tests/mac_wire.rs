// Authored Stage A source cases. Execution is deferred until T01-T30 code is complete.
#![cfg(feature = "mac")]
#[path = "../src/mac/mod.rs"]
mod mac;

// Until Stage B registers the complete MAC client, these source modules are
// included by path and use the same crate-local imports as their final home.
pub use eltdx::{Error, ErrorKind, EventTime, Result, TradingDate};

use mac::bitmap::{decode_bitmap, decode_slot, encode_bitmap, FieldId, FieldSelection, BitmapLayout};
use mac::commands::{bars_request, build_request, decode_bars, decode_quote_rows, decode_hk_transaction, mac_ex_login_request, rewrite_mac_ex_head, MacResponseHeader};
use mac::models::{FieldContext, MacAdjust, MacCode, MacExMarketId, MacInstrumentId, MacMarketId, MacPeriod, MacExRoute, SlotValue, TransactionPrice, WireFloat};

fn set(bitmap: &mut [u8], bit: usize) { bitmap[bit / 8] |= 1 << (bit % 8); }

#[test]
fn legal_unknown_slots_consume_wire_space_and_reserved_flags_do_not() {
    let mut payload = vec![0; 26 + 68 + 12];
    set(&mut payload[..20], 0);
    set(&mut payload[..20], 47);
    set(&mut payload[..20], 72);
    set(&mut payload[..20], 151);
    set(&mut payload[..20], 159);
    payload[20..24].copy_from_slice(&1_u32.to_le_bytes());
    payload[24..26].copy_from_slice(&1_u16.to_le_bytes());
    payload[26 + 68..26 + 72].copy_from_slice(&0x3f80_0000_u32.to_le_bytes());
    payload[26 + 72..26 + 76].copy_from_slice(&0xdead_beef_u32.to_le_bytes());
    payload[26 + 76..26 + 80].copy_from_slice(&0x4148_0000_u32.to_le_bytes());

    let (total, rows, bitmap) = decode_quote_rows(&payload, FieldContext::Instrument).expect("valid literal response");
    assert_eq!(total, 1);
    assert_eq!(bitmap.slots.len(), 3);
    assert_eq!(rows[0].fields.len(), 3);
    assert_eq!(rows[0].fields[0].value, SlotValue::F32(WireFloat::from_bits(0x3f80_0000)));
    assert_eq!(rows[0].fields[1].value, SlotValue::Unknown([0xef, 0xbe, 0xad, 0xde]));
    assert_eq!(rows[0].fields[2].bit.get(), 72);
    assert_eq!(rows[0].fields[2].value, SlotValue::F32(WireFloat::from_bits(0x4148_0000)));
    assert_eq!(bitmap.reserved[18] & 0x80, 0x80);
    assert_eq!(bitmap.reserved[19] & 0x80, 0x80);
}

#[test]
fn bitmap_selection_deduplicates_and_board_layout_rejects_high_fields() {
    let mut selection = FieldSelection::from_ids([FieldId::new(128).expect("valid"), FieldId::new(0).expect("valid"), FieldId::new(128).expect("valid")]);
    assert_eq!(selection.ids().iter().map(|id| id.get()).collect::<Vec<_>>(), vec![0, 128]);
    assert!(encode_bitmap(&selection, BitmapLayout::Board16).is_err());
    selection = FieldSelection::from_ids([FieldId::new(127).expect("valid")]);
    let board = encode_bitmap(&selection, BitmapLayout::Board16).expect("bit 127 fits");
    assert_eq!(board.len(), 16);
    assert_eq!(board[15], 0x80);
    assert!(FieldId::new(151).is_err());
}

#[test]
fn mac_envelope_and_seconds_bar_are_literal_width_preserving() {
    let frame = build_request(0x122b, &[0xaa, 0xbb], 0x1c).expect("small request");
    assert_eq!(&frame.wire[..10], &[0x1c, 0, 0, 0, 0, 1, 4, 0, 4, 0]);
    assert_eq!(&frame.wire[10..], &[0x2b, 0x12, 0xaa, 0xbb]);
    assert_eq!(frame.wire.len(), 14);

    let mut response = vec![0; 69];
    response[27..29].copy_from_slice(&1_u16.to_le_bytes());
    response[33..37].copy_from_slice(&2026_1116_u32.to_le_bytes());
    response[37..41].copy_from_slice(&34_205_u32.to_le_bytes());
    response[41..45].copy_from_slice(&0x8000_0000_u32.to_le_bytes());
    let bars = decode_bars(&response, MacPeriod::Seconds).expect("one full source record");
    assert_eq!(bars[0].time_seconds, 34_205);
    assert_eq!(bars[0].values[0].bits(), 0x8000_0000);
    assert_eq!(bars[0].source_bytes.len(), 36);
    assert!(decode_bars(&response[..68], MacPeriod::Seconds).is_err());
}

#[test]
fn response_head_and_mac_ex_defaults_keep_distinct_domains() {
    let header = MacResponseHeader::parse(&[1,0,0,0,2,0,0,0,3,0,0,0,4,0,5,0]).expect("16-byte header");
    assert_eq!((header.magic, header.sequence, header.method, header.zip_size, header.unzip_size), (1,2,3,4,5));
    assert!(MacResponseHeader::parse(&[0;15]).is_err());

    let fields = mac::routing::default_quote_selection();
    assert_eq!(fields.ids().iter().map(|id| id.get()).collect::<Vec<_>>(), vec![0,1,2,3,4,5,7]);
    assert_eq!(mac::routing::transaction_route(MacExMarketId::new(31)), MacExRoute::ExHongKongTransactions);
    assert_eq!(mac::routing::transaction_route(MacExMarketId::new(23)), MacExRoute::MacCommand);
    let hk = decode_hk_transaction(36_062, 12_345, 700, 1);
    assert_eq!(hk.time_seconds, 36_062);
    assert_eq!(hk.price, TransactionPrice::HongKongScaledU32 { raw: 12_345, divisor: 1000 });
    assert!((hk.price.as_f64() - 12.345).abs() < f64::EPSILON);
    assert_eq!((hk.trade_count, hk.trade_count_provided, hk.nature), (0, false, 1));
}

#[test]
fn mac_ex_login_literal_and_head_rewrite_leave_payload_untouched() {
    let login = mac_ex_login_request().expect("fixed login payload");
    assert_eq!(login.command, 0x2454);
    assert_eq!(login.head_flag, 1);
    assert_eq!(login.body.len(), 80);
    assert_eq!(&login.wire[..10], &[1,0,0,0,0,1,82,0,82,0]);

    let source = build_request(0x122f, &[0x1c, 0x00, 0xff], 0x1c).expect("MAC request").wire;
    let rewritten = rewrite_mac_ex_head(&source).expect("MAC-EX adaptation");
    assert_eq!(rewritten[0], 0x01);
    assert_eq!(&rewritten[1..], &source[1..]);
    let standard = [0x0c, 0x1c, 0x01];
    assert_eq!(rewrite_mac_ex_head(&standard).expect("ordinary EX packet"), standard);
    assert!(rewrite_mac_ex_head(&[]).is_err());
}

#[test]
fn checked_mac_identity_and_request_schema_remain_separate_from_ex_ids() {
    assert!(MacCode::new("01234567890123456789012").is_err());
    assert!(MacMarketId::checked(65_536).is_err());
    let id = MacInstrumentId::new(MacMarketId::new(71), MacCode::new("00700").expect("GBK code"));
    let request = bars_request(&id, MacPeriod::Seconds, 5, 7, 800, MacAdjust::Qfq).expect("bar query");
    assert_eq!(request.command, 0x122e);
    assert_eq!(&request.body[..2], &71_u16.to_le_bytes());
    assert_eq!(&request.body[24..26], &13_u16.to_le_bytes());
    assert_eq!(&request.body[26..28], &5_u16.to_le_bytes());
    assert_eq!(&request.body[32..34], &800_u16.to_le_bytes());
}

#[test]
fn raw_slot_decoder_preserves_signed_and_float_bits() {
    let bitmap = decode_bitmap(&{ let mut value=vec![0;20];set(&mut value,46);value }).expect("bitmap");
    assert_eq!(decode_slot(bitmap.slots[0], [0xfd,0xff,0xff,0xff]), SlotValue::I32(-3));
    assert_eq!(WireFloat::from_bits(0x7fc0_1234).bits(), 0x7fc0_1234);
}
