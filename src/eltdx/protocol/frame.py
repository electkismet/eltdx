"""Public frame value objects backed by the Rust protocol decoder."""

from __future__ import annotations

from dataclasses import dataclass

from eltdx.transport.native import call_native, native_module

from .constants import CONTROL_DEFAULT

MAX_RESPONSE_PAYLOAD_SIZE = 0xFFFF


@dataclass(frozen=True, slots=True)
class RequestFrame:
    msg_id: int
    msg_type: int
    data: bytes = b""
    control: int = CONTROL_DEFAULT

    def to_bytes(self) -> bytes:
        """Encode this public value object through the Rust protocol core."""

        return call_native(
            native_module().encode_request_frame,
            self.msg_id,
            self.msg_type,
            self.data,
            self.control,
        )


@dataclass(frozen=True, slots=True)
class ResponseFrame:
    control: int
    msg_id: int
    msg_type: int
    zip_length: int
    length: int
    data: bytes
    raw: bytes
    response_header_reserved: int = 0


def decode_response(
    raw: bytes, *, max_payload_size: int = MAX_RESPONSE_PAYLOAD_SIZE
) -> ResponseFrame:
    """Decode one response frame through the private Rust extension."""

    native_dto = call_native(native_module().decode_response, raw, max_payload_size)
    from eltdx._native_models import _response_frame

    return _response_frame(native_dto)


__all__ = ["MAX_RESPONSE_PAYLOAD_SIZE", "RequestFrame", "ResponseFrame", "decode_response"]
