#![allow(clippy::unwrap_used, clippy::expect_used)]
// Authored source cases only. Execution is deferred until T01-T30 sources are complete.
#![cfg(feature = "mac")]
use bytes::Bytes;
use eltdx::{advanced::mac as advanced, mac, ErrorKind};
use mac::bitmap::{BitmapLayout, FieldId, FieldSelection, PresetField};
use mac::models::{FieldContext, SlotKind};

fn set(bytes: &mut [u8], bit: usize) {
    bytes[bit / 8] |= 1 << (bit % 8);
}

#[test]
fn named_fields_keep_alias_context_and_omit_unknown_slots_without_changing_wire_width() {
    let mut bytes = [0; 20];
    for bit in [0, 47, 93, 94, 136, 139, 151, 159] {
        set(&mut bytes, bit);
    }
    let instrument = mac::fields::get_active_fields(&bytes).expect("named instrument view");
    let board =
        mac::fields::active_fields_in(&bytes, FieldContext::Board).expect("named board view");
    assert_eq!(
        instrument
            .iter()
            .map(|field| field.bit().get())
            .collect::<Vec<_>>(),
        [0, 93, 94, 136, 139]
    );
    assert_eq!(
        instrument
            .iter()
            .map(|field| field.name())
            .collect::<Vec<_>>(),
        [
            "PRE_CLOSE",
            "BID2_VOLUME",
            "ASK2_VOLUME",
            "BID5_VOLUME",
            "ASK5_VOLUME"
        ]
    );
    assert_eq!(
        board.iter().map(|field| field.name()).collect::<Vec<_>>(),
        [
            "PRE_CLOSE",
            "LIMIT_UP_COUNT",
            "LIMIT_DOWN_COUNT",
            "UP_COUNT",
            "DOWN_COUNT"
        ]
    );
    assert_eq!(instrument[0].kind(), SlotKind::F32);
    assert_eq!(instrument[1].kind(), SlotKind::U32);
    let wire = mac::bitmap::decode_bitmap(&bytes).expect("wire bitmap");
    assert_eq!(wire.slots.len(), 6); // Unknown legal bit47 consumes its own slot.
    assert_eq!(wire.slots[1].bit.get(), 47);
    assert!(wire.slots[1].definition.is_none());
    assert_eq!(mac::bitmap::quote_row_width(&wire).expect("wire width"), 92);
    assert_eq!(wire.reserved[18], 0x80);
    assert_eq!(wire.reserved[19], 0x80);
}

#[test]
fn sixteen_byte_field_views_zero_extend_and_other_widths_fail() {
    let mut board = [0; 16];
    for bit in [4, 47, 93, 127] {
        set(&mut board, bit);
    }
    let fields = mac::fields::get_active_fields(&board).expect("16-byte source layout");
    assert_eq!(
        fields
            .iter()
            .map(|field| field.bit().get())
            .collect::<Vec<_>>(),
        [4, 93]
    );
    let mut expanded = [0; 20];
    expanded[..16].copy_from_slice(&board);
    assert_eq!(
        fields,
        mac::fields::get_active_fields(&expanded).expect("explicit expanded layout")
    );
    for width in [0, 15, 17, 19, 21] {
        assert_eq!(
            mac::fields::get_active_fields(&vec![0; width])
                .expect_err("invalid width")
                .kind(),
            ErrorKind::Protocol
        );
    }
}

#[test]
fn ordinary_bitmap_builder_is_checked_and_debug_is_confined_to_advanced_output() {
    let selection = FieldSelection::from_ids([
        FieldId::new(93).expect("field93"),
        FieldId::new(0).expect("field0"),
        FieldId::new(93).expect("duplicate93"),
    ]);
    let mut expected = vec![0; 16];
    expected[0] = 1;
    expected[11] = 0x20;
    assert_eq!(
        mac::fields::build_bitmap(&selection, BitmapLayout::Board16).expect("board bitmap"),
        expected
    );
    let high = FieldSelection::from_ids([FieldId::new(136).expect("field136")]);
    assert_eq!(
        mac::fields::build_bitmap(&high, BitmapLayout::Board16)
            .expect_err("high field rejected")
            .kind(),
        ErrorKind::InvalidArgument
    );
    assert_eq!(
        mac::fields::build_bitmap(&high, BitmapLayout::Quote20).expect("20-byte bitmap")[17],
        1
    );
    let debug = FieldSelection::preset(PresetField::Debug);
    assert_eq!(
        mac::fields::build_bitmap(&debug, BitmapLayout::Quote20)
            .expect_err("ordinary DEBUG rejected")
            .kind(),
        ErrorKind::InvalidArgument
    );
    assert_eq!(
        advanced::build_bitmap(&debug, BitmapLayout::Quote20).expect("advanced DEBUG"),
        vec![0xff; 20]
    );
    let mut board_and_control =
        advanced::build_bitmap(&debug, BitmapLayout::Board16).expect("advanced16 DEBUG");
    board_and_control.extend_from_slice(&[0, 0x40, 0, 1]);
    assert_eq!(&board_and_control[..16], &[0xff; 16]);
    assert_eq!(&board_and_control[16..], &[0, 0x40, 0, 1]);
}

#[test]
fn advanced_request_and_header_are_literal_and_header_requires_exactly_sixteen_bytes() {
    assert_eq!(
        advanced::build_mac_request(0x122b, &[0xaa, 0xbb], 0x1c).expect("request"),
        [0x1c, 0, 0, 0, 0, 1, 4, 0, 4, 0, 0x2b, 0x12, 0xaa, 0xbb]
    );
    assert_eq!(
        advanced::build_mac_request(0x122b, &vec![0; 65534], 0x1c)
            .expect_err("u16 body overflow")
            .kind(),
        ErrorKind::InvalidArgument
    );
    let bytes = [
        1, 0, 0, 0, 0x44, 0x33, 0x22, 0x11, 0x2e, 0x12, 0, 0, 13, 0, 5, 0,
    ];
    let header = advanced::parse_mac_header(&bytes).expect("literal16B header");
    assert_eq!(
        (
            header.magic,
            header.sequence,
            header.method,
            header.zip_size,
            header.unzip_size
        ),
        (1, 0x1122_3344, 0x122e, 13, 5)
    );
    for width in [0, 15, 17, 32] {
        assert_eq!(
            advanced::parse_mac_header(&vec![0; width])
                .expect_err("exact header width")
                .kind(),
            ErrorKind::Protocol
        );
    }
}

#[test]
fn advanced_decompression_reuses_exact_stream_and_caps_with_literal_zlib() {
    // Independently specified zlib encoding of ASCII hello; Adler32=062c0215.
    let encoded = Bytes::from_static(&[
        0x78, 0x9c, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x07, 0, 0x06, 0x2c, 0x02, 0x15,
    ]);
    let header = advanced::MacResponseHeader {
        magic: 1,
        sequence: 2,
        method: 0x122e,
        zip_size: 13,
        unzip_size: 5,
    };
    assert_eq!(
        advanced::decompress_mac_body(header, encoded.clone(), advanced::FrameLimits::default())
            .expect("complete zlib")
            .as_ref(),
        b"hello"
    );
    for decoded in [0, 4, 6, 65535] {
        assert_eq!(
            advanced::decompress_mac_body(
                advanced::MacResponseHeader {
                    unzip_size: decoded,
                    ..header
                },
                encoded.clone(),
                advanced::FrameLimits::default()
            )
            .expect_err("decoded length mismatch")
            .kind(),
            ErrorKind::Protocol
        );
    }
    for limits in [
        advanced::FrameLimits::new(12, 5).expect("encoded cap"),
        advanced::FrameLimits::new(13, 4).expect("decoded cap"),
    ] {
        assert_eq!(
            advanced::decompress_mac_body(header, encoded.clone(), limits)
                .expect_err("frame cap")
                .kind(),
            ErrorKind::LimitExceeded
        );
    }
    assert_eq!(
        advanced::decompress_mac_body(
            header,
            encoded.slice(..12),
            advanced::FrameLimits::default()
        )
        .expect_err("encoded width")
        .kind(),
        ErrorKind::Protocol
    );
    let mut trailing = encoded.to_vec();
    trailing.push(0);
    assert_eq!(
        advanced::decompress_mac_body(
            advanced::MacResponseHeader {
                zip_size: 14,
                ..header
            },
            Bytes::from(trailing),
            advanced::FrameLimits::default()
        )
        .expect_err("trailing byte")
        .kind(),
        ErrorKind::Protocol
    );
    let mut concatenated = encoded.to_vec();
    concatenated.extend_from_slice(&encoded);
    assert_eq!(
        advanced::decompress_mac_body(
            advanced::MacResponseHeader {
                zip_size: 26,
                unzip_size: 10,
                ..header
            },
            Bytes::from(concatenated),
            advanced::FrameLimits::default()
        )
        .expect_err("second stream")
        .kind(),
        ErrorKind::Protocol
    );
    let plain = advanced::MacResponseHeader {
        zip_size: 3,
        unzip_size: 3,
        ..header
    };
    assert_eq!(
        advanced::decompress_mac_body(
            plain,
            Bytes::from_static(b"abc"),
            advanced::FrameLimits::default()
        )
        .expect("uncompressed body")
        .as_ref(),
        b"abc"
    );
}

#[test]
fn board_helper_preserves_checked_source_conversion() {
    for (source, expected) in [
        (" 880001 ", 20001),
        ("399001", 30001),
        ("899001", 32001),
        ("000001", 31001),
        ("HK001", 20001),
        ("US001", 30001),
    ] {
        let board = mac::BoardSymbol::new(source).expect("checked board");
        assert_eq!(board.as_str(), source);
        assert_eq!(
            mac::util::convert_board_code(&board).expect("pure conversion"),
            expected
        );
    }
    assert!(mac::BoardSymbol::new("HK4294967295").is_err());
    assert!(mac::BoardSymbol::new("hk001").is_err());
}
