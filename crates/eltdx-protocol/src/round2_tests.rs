use std::panic::{catch_unwind, AssertUnwindSafe};

use bytes::Bytes;
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;

use crate::frame::{decode_response, RequestFrame, ResponseFrameDecoder, RESPONSE_PREFIX};
use crate::limits::{MAX_REQUEST_DATA_SIZE, MAX_RESPONSE_PAYLOAD_SIZE, RESPONSE_HEADER_SIZE};

fn plain_response(msg_id: u32, msg_type: u16, payload: &[u8]) -> Option<Vec<u8>> {
    let length = u16::try_from(payload.len()).ok()?;
    let mut raw = Vec::with_capacity(RESPONSE_HEADER_SIZE + payload.len());
    raw.extend_from_slice(&RESPONSE_PREFIX);
    raw.push(0);
    raw.extend_from_slice(&msg_id.to_le_bytes());
    raw.push(0);
    raw.extend_from_slice(&msg_type.to_le_bytes());
    raw.extend_from_slice(&length.to_le_bytes());
    raw.extend_from_slice(&length.to_le_bytes());
    raw.extend_from_slice(payload);
    Some(raw)
}

proptest! {
    #[test]
    fn parser_properties_request_frame_roundtrip(
        msg_id in any::<u32>(),
        msg_type in any::<u16>(),
        control in any::<u8>(),
        data in proptest::collection::vec(any::<u8>(), 0..=1_024),
    ) {
        let frame = RequestFrame::with_control(
            msg_id,
            msg_type,
            Bytes::from(data),
            control,
        );
        let encoded = match frame.encode() {
            Ok(encoded) => encoded,
            Err(error) => return Err(TestCaseError::fail(error.to_string())),
        };
        let decoded = match RequestFrame::decode(&encoded) {
            Ok(decoded) => decoded,
            Err(error) => return Err(TestCaseError::fail(error.to_string())),
        };
        prop_assert_eq!(decoded, frame);
    }

    #[test]
    fn frame_decoder_parser_properties_arbitrary_fragmentation(
        msg_id in any::<u32>(),
        msg_type in any::<u16>(),
        payload in proptest::collection::vec(any::<u8>(), 0..=512),
        fragment_sizes in proptest::collection::vec(1_usize..=128, 0..64),
    ) {
        let raw = match plain_response(msg_id, msg_type, &payload) {
            Some(raw) => raw,
            None => return Err(TestCaseError::fail("bounded payload did not fit u16")),
        };
        let mut decoder = ResponseFrameDecoder::default();
        let mut frames = Vec::new();
        let mut offset = 0_usize;
        for fragment_size in fragment_sizes {
            if offset == raw.len() {
                break;
            }
            let end = offset.saturating_add(fragment_size).min(raw.len());
            match decoder.feed(&raw[offset..end]) {
                Ok(mut decoded) => frames.append(&mut decoded),
                Err(error) => return Err(TestCaseError::fail(error.to_string())),
            }
            offset = end;
        }
        if offset < raw.len() {
            match decoder.feed(&raw[offset..]) {
                Ok(mut decoded) => frames.append(&mut decoded),
                Err(error) => return Err(TestCaseError::fail(error.to_string())),
            }
        }
        match frames.as_slice() {
            [frame] => {
                prop_assert_eq!(frame.msg_id, msg_id);
                prop_assert_eq!(frame.msg_type, msg_type);
                prop_assert_eq!(frame.data.as_ref(), payload.as_slice());
            }
            _ => return Err(TestCaseError::fail(format!(
                "expected one decoded frame, got {}",
                frames.len(),
            ))),
        }
        prop_assert_eq!(decoder.buffered_bytes(), 0);
    }

    #[test]
    fn fuzz_corpus_arbitrary_bytes_remain_panic_free_and_bounded(
        raw in proptest::collection::vec(any::<u8>(), 0..=4_096),
    ) {
        const PAYLOAD_LIMIT: usize = 512;
        const BUFFER_LIMIT: usize = RESPONSE_HEADER_SIZE + PAYLOAD_LIMIT;
        let decoder = match ResponseFrameDecoder::with_limits(
            PAYLOAD_LIMIT,
            BUFFER_LIMIT,
            BUFFER_LIMIT,
        ) {
            Ok(decoder) => decoder,
            Err(error) => return Err(TestCaseError::fail(error.to_string())),
        };
        let result = catch_unwind(AssertUnwindSafe(|| {
            let mut decoder = decoder;
            let accepted = decoder.push(&raw);
            let decoded = decoder.decode_available(64, 4 * 1_024 * 1_024);
            (accepted, decoder.buffered_bytes(), decoded)
        }));
        match result {
            Ok((accepted, buffered, _)) => {
                prop_assert!(accepted <= raw.len());
                prop_assert!(buffered <= BUFFER_LIMIT);
            }
            Err(_) => return Err(TestCaseError::fail("response decoder panicked")),
        }
    }
}

#[test]
fn fuzz_corpus_fixed_malformed_frames_never_panic() {
    let corpus = [
        Vec::new(),
        vec![0xb1],
        vec![0xb1, 0xcb, 0x74, 0x00],
        vec![
            0xb1, 0xcb, 0x74, 0x00, 0, 1, 0, 0, 0, 0, 4, 0, 3, 0, 10, 0, 0x78, 0x9c, 0,
        ],
        vec![0xff; RESPONSE_HEADER_SIZE + 32],
    ];
    for raw in corpus {
        let result = catch_unwind(|| decode_response(&raw, MAX_RESPONSE_PAYLOAD_SIZE));
        assert!(result.is_ok(), "decoder panicked for {} bytes", raw.len());
    }
}

#[test]
fn protocol_limits_and_panic_boundaries_reject_oversized_inputs() {
    let result = catch_unwind(|| {
        let request = RequestFrame::new(1, 4, vec![0_u8; MAX_REQUEST_DATA_SIZE + 1]);
        assert!(request.encode().is_err());
        assert!(ResponseFrameDecoder::with_limits(
            MAX_RESPONSE_PAYLOAD_SIZE + 1,
            RESPONSE_HEADER_SIZE + MAX_RESPONSE_PAYLOAD_SIZE + 1,
            0,
        )
        .is_err());

        let raw = plain_response(1, 4, &[0_u8; 2]);
        assert!(matches!(
            raw.as_deref().map(|frame| decode_response(frame, 1)),
            Some(Err(_)),
        ));
    });
    assert!(result.is_ok(), "protocol limit path panicked");
}
