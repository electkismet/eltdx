from __future__ import annotations

from ..enums import Exchange
from ..exceptions import ProtocolError
from .constants import TYPE_COUNT
from .frame import RequestFrame, ResponseFrame
from .unit import exchange_to_wire, little_u16, normalize_exchange


COUNT_TRAILER = bytes([0x75, 0xC7, 0x33, 0x01])


def build_count_frame(exchange: str | Exchange, msg_id: int) -> RequestFrame:
    resolved = normalize_exchange(exchange)
    payload = bytes([exchange_to_wire(resolved), 0x00]) + COUNT_TRAILER
    return RequestFrame(msg_id=msg_id, msg_type=TYPE_COUNT, data=payload)


def parse_count_payload(response: ResponseFrame) -> int:
    payload = response.data
    if len(payload) < 2:
        raise ProtocolError("invalid count payload")
    return little_u16(payload[:2])