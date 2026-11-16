//! Shared GBK source cases. Authored only; do not execute before the plan gate.
#![allow(dead_code)]

pub use eltdx::{Error, ErrorKind, Result};

#[path = "../src/encoding/mod.rs"]
mod encoding;

use encoding::{gbk_scalar, scan_gbk, InvalidBytePolicy};

fn decode(bytes: &[u8], policy: InvalidBytePolicy) -> (String, bool) {
    let mut text = String::new();
    let result = scan_gbk(bytes, policy, |scalar| text.push(scalar), &mut || Ok::<(), ()>(()));
    (text, result.expect("infallible source fixture"))
}

#[test]
fn strict_gbk_pairs_and_ascii_preserve_original_mapping() {
    assert_eq!(gbk_scalar(0xb9, 0xab), Some('公'));
    assert_eq!(gbk_scalar(0xcb, 0xbe), Some('司'));
    assert_eq!(gbk_scalar(0x81, 0x40), Some('\u{4e02}'));
    assert_eq!(gbk_scalar(0xa3, 0xb9), Some('９'));
    assert_eq!(decode(b"ASCII\0\r\n\xb9\xab\xcb\xbe", InvalidBytePolicy::Replace),
        ("ASCII\0\r\n公司".to_owned(), false));
}

#[test]
fn invalid_leads_trails_and_undefined_pairs_are_not_whatwg_extensions() {
    for (lead, trail) in [(0x80, 0x40), (0xff, 0x40), (0x81, 0x3f),
        (0x81, 0x7f), (0x81, 0xff), (0xa1, 0x40), (b'A', b'B')] {
        assert_eq!(gbk_scalar(lead, trail), None);
    }
    let bytes = b"\x80A\xff\x81\x7f\xa1@\x81";
    assert_eq!(decode(bytes, InvalidBytePolicy::Replace), ("�A��\u{7f}�@�".to_owned(), true));
    assert_eq!(decode(bytes, InvalidBytePolicy::Ignore), ("A\u{7f}@".to_owned(), true));
}

#[test]
fn undefined_pair_consumes_only_its_lead_then_retries_ascii_or_valid_pair() {
    assert_eq!(decode(b"\xa1@\xb9\xab", InvalidBytePolicy::Replace), ("�@公".to_owned(), true));
    assert_eq!(decode(b"\xa1@\xb9\xab", InvalidBytePolicy::Ignore), ("@公".to_owned(), true));
    assert_eq!(decode(b"\x81\x20\xb9\xab", InvalidBytePolicy::Replace), ("� 公".to_owned(), true));
    assert_eq!(decode(b"\x81\x20\xb9\xab", InvalidBytePolicy::Ignore), (" 公".to_owned(), true));
}

#[test]
fn scanner_propagates_cancellation_before_emission_and_between_chunks() {
    let mut emitted = String::new();
    let error = scan_gbk(b"abc", InvalidBytePolicy::Replace,
        |scalar| emitted.push(scalar), &mut || Err("cancelled"));
    assert_eq!(error, Err("cancelled"));
    assert!(emitted.is_empty());

    let bytes = vec![b'a'; 32 * 1024];
    let mut calls = 0;
    let mut emitted = 0;
    let error = scan_gbk(&bytes, InvalidBytePolicy::Ignore, |_| emitted += 1, &mut || {
        calls += 1;
        if calls == 2 { Err("expired") } else { Ok(()) }
    });
    assert_eq!(error, Err("expired"));
    assert_eq!(emitted, 16 * 1024);
}

#[test]
fn final_boundary_can_reject_completed_output_and_empty_input_is_checked() {
    let mut calls = 0;
    let result = scan_gbk(b"a", InvalidBytePolicy::Ignore, |_| {}, &mut || {
        calls += 1;
        if calls == 2 { Err("late cancellation") } else { Ok(()) }
    });
    assert_eq!(result, Err("late cancellation"));
    assert_eq!(decode(b"", InvalidBytePolicy::Replace), (String::new(), false));
    assert_eq!(scan_gbk(b"", InvalidBytePolicy::Ignore, |_| {}, &mut || Err("closed")), Err("closed"));
}

#[test]
fn a_pair_crossing_the_chunk_edge_is_decoded_once() {
    let mut bytes = vec![b'a'; 16 * 1024 - 1];
    bytes.extend_from_slice(b"\xb9\xabz");
    let mut calls = 0;
    let mut text = String::new();
    let result = scan_gbk(&bytes, InvalidBytePolicy::Replace, |scalar| text.push(scalar), &mut || {
        calls += 1;
        Ok::<(), ()>(())
    });
    assert_eq!(result, Ok(false));
    assert!(text.ends_with("公z"));
    assert_eq!(text.chars().count(), 16 * 1024 + 1);
    assert_eq!(calls, 3);
}

#[cfg(feature = "offline")]
#[test]
fn public_offline_reader_preserves_replacement_policy() {
    let records = eltdx::offline::industry::parse(b"0|000001|\x80\xb9\xab\xcb\xbe|||\xa1@").expect("source industry row");
    assert_eq!(records.len(), 1);
    let row = records.get("000001").expect("known source identity");
    assert_eq!(row.tdx(), "�公司");
    assert_eq!(row.sw(), "�@");
}
