// Source cases only. Do not execute before the T01-T30 implementation gate.
#![cfg(feature = "mac")]
use eltdx::{mac, TradingDate};
use mac::{
    decode,
    models::{MacCode, RemoteFileName},
    responses::JsonPresence,
};

fn word(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}
fn scalar(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn gbk(text: &str) -> Vec<u8> {
    let (bytes, _, failed) = encoding_rs::GBK.encode(text);
    assert!(!failed);
    bytes.into_owned()
}
fn json(text: &str) -> Vec<u8> {
    // The prefix deliberately contains invalid JSON and invalid GBK. A codec
    // that decodes the entire body cannot pass this independent fixture.
    let mut bytes = vec![0xff; 27];
    bytes.extend_from_slice(&gbk(text));
    bytes
}
fn chart_tail() -> Vec<u8> {
    let mut bytes = vec![0; 120];
    bytes[..4].copy_from_slice(b"NAME");
    bytes[44] = 3;
    word(&mut bytes, 45, 74);
    scalar(&mut bytes, 47, 0x3f80_0000);
    scalar(&mut bytes, 56, 20261117);
    scalar(&mut bytes, 60, 100102);
    for index in 0..6 {
        scalar(&mut bytes, 64 + index * 4, 0x3f80_0000 + index as u32);
    }
    scalar(&mut bytes, 88, 1234);
    scalar(&mut bytes, 92, 0x8000_0000);
    bytes[96..108].fill(0xaa);
    scalar(&mut bytes, 108, 0x7fc0_1234);
    scalar(&mut bytes, 112, 0x4000_0000);
    scalar(&mut bytes, 116, 900);
    bytes
}

#[test]
fn remote_filename_uses_seventy_gbk_bytes_separately_from_mac_code() {
    let thirty = "a".repeat(30);
    assert!(MacCode::new(&thirty).is_err());
    let name =
        RemoteFileName::new(&thirty).expect("resource name exceeds code limit but fits filename");
    assert_eq!(name.encoded().len(), 30);
    let packet = mac::commands::file_meta_request(name.as_str(), 17).expect("filename request");
    assert_eq!(&packet.body[..4], &17_u32.to_le_bytes());
    assert_eq!(&packet.body[4..34], thirty.as_bytes());
    assert_eq!(
        RemoteFileName::new("中".repeat(35))
            .expect("exact 70 GBK bytes")
            .encoded()
            .len(),
        70
    );
    for value in [
        "a".repeat(71),
        "中".repeat(36),
        "file\0name".into(),
        "😀".into(),
        String::new(),
    ] {
        assert!(RemoteFileName::new(value).is_err());
    }
}

#[test]
fn single_day_chart_keeps_tick_and_tail_float_bits_and_zero_chart_is_valid() {
    let mut bytes = vec![0; 35 + 18];
    word(&mut bytes, 0, 74);
    bytes[2..6].copy_from_slice(b"MSFT");
    scalar(&mut bytes, 24, 20261117);
    word(&mut bytes, 33, 1);
    word(&mut bytes, 35, 571);
    scalar(&mut bytes, 37, 0x8000_0000);
    scalar(&mut bytes, 41, 0x3f80_0000);
    scalar(&mut bytes, 45, 987);
    scalar(&mut bytes, 49, 0x7fc0_1234);
    bytes.extend_from_slice(&chart_tail());
    let result = decode::tick_chart(&bytes).expect("one complete tick plus 120-byte tail");
    assert_eq!(result.id().market().get(), 74);
    assert_eq!(result.ticks()[0].time().clock(), Some((9, 31, 0)));
    assert_eq!(result.ticks()[0].price().value().bits(), 0x8000_0000);
    assert_eq!(
        result.ticks()[0]
            .momentum()
            .expect("present source momentum")
            .value()
            .bits(),
        0x7fc0_1234
    );
    assert_eq!(*result.snapshot().vol().value(), 1234);
    assert_eq!(result.snapshot().amount().value().bits(), 0x8000_0000);
    assert_eq!(result.snapshot().turnover().value().bits(), 0x7fc0_1234);
    assert_eq!(result.snapshot().tail_pad(), &[0xaa; 12]);
    assert!(decode::tick_chart(&bytes[..bytes.len() - 1]).is_err());
    let mut empty = vec![0; 35];
    empty.extend_from_slice(&chart_tail());
    assert!(decode::tick_chart(&empty)
        .expect("header and tail with zero ticks")
        .ticks()
        .is_empty());
    assert!(decode::tick_chart(&[]).is_err());
}

#[test]
fn multi_day_chart_assigns_short_first_day_without_cross_day_shift() {
    let mut bytes = vec![0; 71 + 3 * 14];
    scalar(&mut bytes, 24, 20261116);
    scalar(&mut bytes, 28, 20261117);
    scalar(&mut bytes, 44, 0x3f80_0000);
    scalar(&mut bytes, 48, 0x4000_0000);
    word(&mut bytes, 64, 2);
    word(&mut bytes, 67, 2);
    word(&mut bytes, 69, 3);
    for index in 0..3 {
        let offset = 71 + index * 14;
        word(&mut bytes, offset, 570 + index as u16);
        scalar(&mut bytes, offset + 2, 0x3f80_0000 + index as u32);
        word(&mut bytes, offset + 10, 10 + index as u16);
        word(&mut bytes, offset + 12, 100 + index as u16);
    }
    bytes.extend_from_slice(&chart_tail());
    let result = decode::tick_charts(&bytes).expect("short first day then full second day");
    assert_eq!(
        result
            .days()
            .iter()
            .map(|day| day.ticks().len())
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(
        result.days()[1].date(),
        TradingDate::new(2026, 11, 17).expect("date")
    );
    assert_eq!(result.days()[1].ticks()[0].time().clock(), Some((9, 31, 0)));
    assert_eq!(result.days()[1].ticks()[1].reserved(), Some(102));
    assert!(result.days()[0].ticks()[0].momentum().is_none());
    let mut damaged = bytes.clone();
    word(&mut damaged, 67, 0);
    assert!(decode::tick_charts(&damaged).is_err());
    damaged = bytes.clone();
    scalar(&mut damaged, 28, 20260230);
    assert!(decode::tick_charts(&damaged).is_err());
    damaged = bytes.clone();
    word(&mut damaged, 64, 6);
    assert!(decode::tick_charts(&damaged).is_err());
    assert!(decode::tick_charts(&bytes[..bytes.len() - 1]).is_err());
}

#[test]
fn empty_multi_day_chart_requires_valid_header_and_tail() {
    let mut bytes = vec![0; 71];
    bytes.extend_from_slice(&chart_tail());
    assert!(decode::tick_charts(&bytes)
        .expect("empty chart")
        .days()
        .is_empty());
    word(&mut bytes, 69, 1);
    assert!(decode::tick_charts(&bytes).is_err());
    assert!(decode::tick_charts(&[]).is_err());
}

#[test]
fn symbol_snapshot_uses_source_hhmmss_and_preserves_optional_time() {
    let mut bytes = vec![0; 196];
    word(&mut bytes, 8, 47);
    bytes[10..14].copy_from_slice(b"IFL0");
    scalar(&mut bytes, 96, 20261117);
    scalar(&mut bytes, 100, 100102);
    scalar(&mut bytes, 104, 99);
    scalar(&mut bytes, 108, 0x8000_0000);
    scalar(&mut bytes, 132, 12345);
    scalar(&mut bytes, 186, 0x7fc0_1111);
    bytes[194..].copy_from_slice(&[0xcc, 0xdd]);
    let result = decode::symbol_info(&bytes).expect("snapshot source offsets");
    assert_eq!(
        result.time().expect("present time").clock(),
        Some((10, 1, 2))
    );
    assert_eq!(result.pre_close().value().bits(), 0x8000_0000);
    assert_eq!(result.turnover().value().bits(), 0x7fc0_1111);
    assert_eq!(*result.vol().value(), 12345);
    assert_eq!(result.unknown_tail(), &[0xcc, 0xdd]);
    scalar(&mut bytes, 96, 0);
    scalar(&mut bytes, 100, 0);
    assert!(decode::symbol_info(&bytes)
        .expect("absent time")
        .time()
        .is_none());
    scalar(&mut bytes, 96, 20261117);
    scalar(&mut bytes, 100, 240000);
    assert!(decode::symbol_info(&bytes).is_err());
    assert!(decode::symbol_info(&bytes[..193]).is_err());
}

#[test]
fn board_and_leader_have_distinct_identities_and_paired_count_is_exact() {
    let mut bytes = vec![0; 164];
    word(&mut bytes, 0, 2);
    word(&mut bytes, 2, 20);
    word(&mut bytes, 4, 10000);
    bytes[6..12].copy_from_slice(b"880686");
    bytes[12..28].fill(0xaa);
    word(&mut bytes, 84, 1);
    bytes[86..92].copy_from_slice(b"600000");
    scalar(&mut bytes, 72, 0x3f80_0000);
    scalar(&mut bytes, 152, 0x4000_0000);
    let result = decode::boards(&bytes).expect("one paired board/leader");
    assert_eq!(result.total_raw(), 20);
    assert_eq!(result.items()[0].board().id().market().get(), 10000);
    assert_eq!(result.items()[0].board().id().code().as_str(), "880686");
    assert_eq!(result.items()[0].leader().id().code().as_str(), "600000");
    assert_eq!(result.items()[0].board().reserved(), &[0xaa; 16]);
    word(&mut bytes, 0, 3);
    assert!(decode::boards(&bytes).is_err());
    assert!(decode::boards(&[0; 4])
        .expect("zero paired rows")
        .items()
        .is_empty());
}

#[test]
fn goods_placeholder_floats_are_preserved_without_becoming_security_codes() {
    let mut bytes = vec![0; 50];
    word(&mut bytes, 0, 1);
    word(&mut bytes, 2, 123);
    bytes[4..9].copy_from_slice(b"1_GDP");
    word(&mut bytes, 27, 7);
    scalar(&mut bytes, 29, 98765);
    bytes[33] = 9;
    scalar(&mut bytes, 34, 0x7fc0_1234);
    scalar(&mut bytes, 38, 0x8000_0000);
    word(&mut bytes, 46, 10);
    word(&mut bytes, 48, 11);
    let result = decode::goods(&bytes).expect("48-byte goods record");
    assert_eq!(result[0].name().as_str(), "1_GDP");
    assert_eq!(result[0].category(), 123);
    assert_eq!(result[0].index(), 98765);
    assert_eq!(result[0].code()[0].bits(), 0x7fc0_1234);
    assert_eq!(result[0].code()[1].bits(), 0x8000_0000);
    assert_eq!((result[0].c1(), result[0].c2()), (10, 11));
    assert!(decode::goods(&bytes[..49]).is_err());
    assert!(decode::goods(&[0, 0]).expect("no goods").is_empty());
}

#[test]
fn auction_preserves_signed_unmatched_and_seconds() {
    let mut bytes = vec![0; 52];
    scalar(&mut bytes, 24, 1);
    scalar(&mut bytes, 36, 34201);
    scalar(&mut bytes, 40, 0x3f80_0000);
    scalar(&mut bytes, 44, 300);
    bytes[48..52].copy_from_slice(&(-17_i32).to_le_bytes());
    let result = decode::auction(&bytes).expect("one auction");
    assert_eq!(result[0].time().clock(), Some((9, 30, 1)));
    assert_eq!(*result[0].unmatched().value(), -17);
    assert!(decode::auction(&bytes[..51]).is_err());
    scalar(&mut bytes, 36, 86400);
    assert!(decode::auction(&bytes).is_err());
}

#[test]
fn unusual_unknown_kind_keeps_raw_record_and_decodes_names_after_binary_rows() {
    let mut bytes = vec![0; 34];
    word(&mut bytes, 0, 1);
    word(&mut bytes, 2, 74);
    bytes[4..8].copy_from_slice(b"MSFT");
    bytes[11] = 0xfe;
    word(&mut bytes, 13, 77);
    bytes[31] = 10;
    word(&mut bytes, 32, 1234);
    bytes.extend_from_slice(&gbk(",微软,"));
    let result = decode::unusual(&bytes).expect("unknown kind with source name");
    assert_eq!(result[0].time().clock(), Some((10, 12, 34)));
    assert_eq!(result[0].unusual_type(), 0xfe);
    assert_eq!(result[0].name(), "微软");
    assert_eq!(result[0].description(), "异动类型0xfe");
    assert_eq!(result[0].raw()[9], 0xfe);
    assert!(decode::unusual(&bytes[..33]).is_err());
}

#[test]
fn server_dates_keep_invalid_source_without_fabricating_calendar_values() {
    let mut bytes = vec![0; 87];
    scalar(&mut bytes, 22, 20261117);
    scalar(&mut bytes, 63, 20260230);
    word(&mut bytes, 30, 570);
    word(&mut bytes, 32, 690);
    scalar(&mut bytes, 71, 12);
    scalar(&mut bytes, 75, 34);
    bytes[79..].fill(0xaa);
    let result =
        decode::server_info(&bytes).expect("complete fields, invalid last-day remains observed");
    assert_eq!(
        result.today().parsed(),
        Some(TradingDate::new(2026, 11, 17).expect("date"))
    );
    assert_eq!(result.last_trading_day().parsed(), None);
    assert_eq!(result.last_trading_day().source_text(), "2026-02-30");
    assert_eq!(result.sessions_1()[0].open_minutes(), 570);
    assert_eq!(result.market_param_2(), Some(34));
    assert_eq!(result.unknown_tail(), &[0xaa; 8]);
    assert_eq!(
        decode::server_info(&bytes[..71])
            .expect("absent optional params")
            .market_param_1(),
        None
    );
    assert!(decode::server_info(&bytes[..73]).is_err());
    assert!(decode::server_info(&[]).is_err());
}

#[test]
fn sampling_is_declared_count_without_fixed_240_assumption() {
    let mut bytes = vec![0; 50];
    word(&mut bytes, 40, 2);
    scalar(&mut bytes, 42, 0x7fc0_1234);
    scalar(&mut bytes, 46, 0x8000_0000);
    let result = decode::chart_sampling(&bytes).expect("two samples");
    assert_eq!(
        result.iter().map(|n| n.bits()).collect::<Vec<_>>(),
        vec![0x7fc0_1234, 0x8000_0000]
    );
    assert!(decode::chart_sampling(&bytes[..49]).is_err());
    assert!(decode::chart_sampling(&[0; 42])
        .expect("zero samples")
        .is_empty());
}

#[test]
fn file_responses_keep_unknown_hash_and_require_complete_download_prefix() {
    let mut bytes = vec![0; 43];
    scalar(&mut bytes, 0, 17);
    scalar(&mut bytes, 4, 30001);
    bytes[8] = 0xff;
    bytes[9..13].copy_from_slice(b"abcd");
    bytes[41..].copy_from_slice(&[0x12, 0x34]);
    let result = decode::file_meta(&bytes).expect("meta and unknown tail");
    assert_eq!(
        (result.offset(), result.size(), result.flag()),
        (17, 30001, -1)
    );
    assert_eq!(result.hash(), "abcd");
    assert_eq!(result.unknown_tail(), &[0x12, 0x34]);
    assert!(decode::file_meta(&bytes[..40]).is_err());
    let chunk = decode::download_chunk(&[1, 2, 3, 4, 5, 6, 7, 8, 0xab, 0xcd], 2)
        .expect("complete prefix and data");
    assert_eq!(chunk.prefix(), &[1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(chunk.bytes(), &[0xab, 0xcd]);
    assert!(decode::download_chunk(&[0; 7], 1).is_err());
    assert!(decode::download_chunk(&[0; 10], 1).is_err());
    assert!(decode::download_chunk(&[0; 8], 1)
        .expect("complete empty chunk")
        .bytes()
        .is_empty());
}

#[test]
fn belonging_boards_skips_exact_prefix_and_keeps_missing_null_zero_distinct() {
    let bytes = json(r#"[[3,10004,"880001","中文板块",0,null,1,2,3]]"#);
    let rows = decode::belonging_boards(&bytes).expect("GBK JSON after opaque 27-byte prefix");
    assert_eq!(rows[0].name(), "中文板块");
    assert_eq!(rows[0].close().value(), Some(0.0));
    assert_eq!(rows[0].close().presence(), JsonPresence::Present);
    assert_eq!(rows[0].pre_close().value(), None);
    assert_eq!(rows[0].pre_close().presence(), JsonPresence::Null);
    assert_eq!(rows[0].source().len(), 9);
    assert!(decode::belonging_boards(&json("[]"))
        .expect("no boards")
        .is_empty());
    assert!(decode::belonging_boards(&json("[[1,2,3]]")).is_err());
    assert!(decode::belonging_boards(&json("{")).is_err());
    assert!(decode::belonging_boards(&[]).is_err());
}

#[test]
fn capital_flow_has_its_own_two_table_layout_and_no_invented_zero_amounts() {
    let bytes = json(r#"[["10.5",2,0,null],[0,0,0,"7.5",3]]"#);
    let result = decode::capital_flow(&bytes)
        .expect("two tables")
        .expect("present object");
    assert_eq!(result.main_net().value(), Some(8.5));
    assert_eq!(result.main_net().presence(), JsonPresence::Derived);
    assert_eq!(result.small_in().value(), Some(0.0));
    assert_eq!(result.small_out().presence(), JsonPresence::Null);
    assert_eq!(result.small_net().value(), None);
    assert_eq!(result.mid_in().presence(), JsonPresence::Missing);
    assert_eq!(result.mid_net().value(), Some(3.0));
    assert_eq!(result.large_net().value(), Some(7.5));
    assert_eq!(result.date(), None);
    assert!(decode::capital_flow(&json("[]"))
        .expect("valid empty object")
        .is_none());
    assert!(decode::capital_flow(&json("[[true],[]]")).is_err());
    assert!(decode::capital_flow(&json("[[\"NaN\"],[]]")).is_err());
    assert!(decode::capital_flow(&json("[1,2]")).is_err());
}
