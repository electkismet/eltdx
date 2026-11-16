//! The HQ boundary between owned runtime replies and context-aware protocol data.

use bytes::Bytes;
use eltdx_protocol::frame::ResponseFrame;
use eltdx_protocol::{
    CommandRequest, CommandResponse, ParseOptions, ParseRawPolicy, PricePrecisionSource,
    ProtocolError, RecordRange,
};
use eltdx_runtime::slot::{EngineEpoch, GenerationId, RequestId, SlotId};
use eltdx_runtime::RawReply;

use crate::RawPolicy;

/// Reply identity remains available even when diagnostic buffers are omitted.
#[derive(Clone, Debug)]
pub(crate) struct DecodeSource {
    pub(crate) receive_sequence: u64,
    pub(crate) request_id: Option<RequestId>,
    pub(crate) engine_epoch: EngineEpoch,
    pub(crate) slot_id: SlotId,
    pub(crate) generation: GenerationId,
    pub(crate) host: String,
}

#[derive(Clone, Debug)]
pub(crate) struct DecodedReply {
    pub(crate) request: CommandRequest,
    pub(crate) response: CommandResponse,
    pub(crate) source: DecodeSource,
    /// A single shared payload for all record ranges; empty under Omit.
    pub(crate) payload: Bytes,
    /// Original matching framing, including compression lengths, for RetainWire.
    pub(crate) wire: Option<ResponseFrame>,
}

impl DecodedReply {
    pub(crate) fn record_bytes(&self, range: RecordRange) -> Result<Bytes, ProtocolError> {
        range.bytes(&self.payload)
    }

    pub(crate) fn record_hex(&self, range: RecordRange) -> Result<String, ProtocolError> {
        range.hex(&self.payload)
    }
}

/// `precision` must be the snapshot captured by the operation before its first
/// decode. This adapter never loads, publishes, or replaces metadata.
pub(crate) fn decode_reply(
    reply: RawReply,
    raw: RawPolicy,
    precision: &dyn PricePrecisionSource,
) -> Result<DecodedReply, ProtocolError> {
    decode_reply_profile(reply, raw, precision, false)
}

pub(crate) fn decode_reply_profile(
    reply: RawReply, raw: RawPolicy, precision: &dyn PricePrecisionSource, legacy: bool,
) -> Result<DecodedReply, ProtocolError> {
    let RawReply { request, response, request_id, engine_epoch, slot_id, generation, host, receive_sequence } = reply;
    decode_frame_profile(request, response, DecodeSource { request_id: Some(request_id),
        engine_epoch, slot_id, generation, host, receive_sequence }, raw, precision, legacy)
}

pub(crate) fn decode_push(
    frame: &eltdx_runtime::push::PushFrame, activation: CommandRequest,
    raw: RawPolicy, precision: &dyn PricePrecisionSource,
) -> Result<DecodedReply, ProtocolError> {
    decode_frame_profile(activation, frame.response.clone(), DecodeSource { request_id: None,
        receive_sequence: frame.receive_sequence,
        engine_epoch: frame.engine_epoch, slot_id: frame.slot_id, generation: frame.generation,
        host: frame.connected_host.to_string() }, raw, precision, false)
}

fn decode_frame_profile(
    request: CommandRequest, mut response: ResponseFrame, source: DecodeSource,
    raw: RawPolicy, precision: &dyn PricePrecisionSource, legacy: bool,
) -> Result<DecodedReply, ProtocolError> {
    struct QuoteLayout<'a> { precision: &'a dyn PricePrecisionSource, legacy: bool }
    impl PricePrecisionSource for QuoteLayout<'_> {
        fn security_decimal(&self, code: &eltdx_protocol::unit::NormalizedCode) -> Option<u8> {
            self.precision.security_decimal(code)
        }
        fn legacy_quote_layout(&self) -> bool { self.legacy }
    }
    let layout = QuoteLayout { precision, legacy };
    let parse_raw = match raw {
        RawPolicy::Omit => ParseRawPolicy::Omit,
        RawPolicy::RetainPayload | RawPolicy::RetainWire => ParseRawPolicy::RetainPayload,
    };
    let options = ParseOptions {
        price_precision: &layout,
        raw: parse_raw,
    };
    // A runtime Bytes slice can retain the full staging allocation. Queued DTO
    // budgets must own only the declared payload/frame, so normalize retained
    // buffers once here and share those exact-sized copies across all rows.
    if !legacy && raw != RawPolicy::Omit {
        response.data = Bytes::copy_from_slice(&response.data);
        if raw == RawPolicy::RetainWire { response.raw = Bytes::copy_from_slice(&response.raw); }
    }
    let parsed = CommandResponse::parse_with_context(request.clone(), response.data.clone(), &options)?;
    let payload = if parse_raw == ParseRawPolicy::Omit {
        Bytes::new()
    } else {
        response.data.clone()
    };
    Ok(DecodedReply {
        request,
        response: parsed,
        source,
        payload,
        wire: if raw == RawPolicy::RetainWire { Some(response) } else { None },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use eltdx_protocol::commands::session::{HeartbeatRequest, TYPE_HEARTBEAT};
    use eltdx_protocol::unit::NormalizedCode;

    struct NoPrecision;
    impl PricePrecisionSource for NoPrecision {
        fn security_decimal(&self, _: &NormalizedCode) -> Option<u8> { None }
    }

    // Synthetic framing: tests retention and identity, not live-server provenance.
    fn heartbeat() -> Result<RawReply, eltdx_runtime::RuntimeError> {
        Ok(RawReply {
            receive_sequence: 1,
            request: CommandRequest::Heartbeat(HeartbeatRequest),
            response: ResponseFrame {
                control: 1,
                msg_id: 7,
                msg_type: TYPE_HEARTBEAT,
                zip_length: 10,
                length: 10,
                data: Bytes::from_static(&[1, 2, 3, 4, 5, 6, 0xa8, 0x26, 0x35, 0x01]),
                raw: Bytes::from_static(b"synthetic matching wire"),
                response_header_reserved: 0,
            },
            request_id: RequestId::new(1)?,
            engine_epoch: EngineEpoch::new(1)?,
            slot_id: SlotId::new(0),
            generation: GenerationId::new(1)?,
            host: "synthetic.invalid:7709".to_owned(),
        })
    }

    #[test]
    fn context_adapter_owns_only_requested_diagnostics() -> Result<(), Box<dyn std::error::Error>> {
        let original = heartbeat()?;
        let payload = original.response.data.clone();
        let wire = original.response.raw.clone();
        for raw in [RawPolicy::Omit, RawPolicy::RetainPayload, RawPolicy::RetainWire] {
            let decoded = decode_reply(original.clone(), raw, &NoPrecision)?;
            assert_eq!(decoded.source.request_id.expect("requested reply identity").get(), 1);
            assert_eq!(decoded.source.host, "synthetic.invalid:7709");
            let CommandResponse::Heartbeat(ack) = decoded.response else {
                panic!("expected heartbeat");
            };
            assert_eq!(&ack.reserved[..], &[1, 2, 3, 4, 5, 6]);
            assert_eq!(ack.server_date_raw, 20_260_520);
            if raw == RawPolicy::Omit {
                assert!(decoded.payload.is_empty());
                assert!(ack.raw_payload.is_empty());
            } else {
                assert_eq!(decoded.payload, payload);
                assert_ne!(decoded.payload.as_ptr(), payload.as_ptr());
                assert_eq!(ack.raw_payload.as_ptr(), decoded.payload.as_ptr());
            }
            assert_eq!(decoded.wire.is_some(), raw == RawPolicy::RetainWire);
            if let Some(frame) = decoded.wire {
                assert_eq!(frame.raw, wire);
                assert_ne!(frame.raw.as_ptr(), wire.as_ptr());
                assert_eq!(frame.msg_id, 7);
            }
        }
        Ok(())
    }
}
