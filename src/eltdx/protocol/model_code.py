from __future__ import annotations

from ..enums import Exchange
from ..exceptions import ProtocolError
from ..models import SecurityCode
from .constants import TYPE_CODE
from .frame import RequestFrame, ResponseFrame
from .unit import decode_gbk_text, exchange_to_wire, get_volume2, little_u16, little_u32, normalize_exchange


CODE_RECORD_SIZE = 29


def build_code_frame(exchange: str | Exchange, start: int, msg_id: int) -> RequestFrame:
    if start < 0 or start > 0xFFFF:
        raise ValueError("start must be between 0 and 65535")
    resolved = normalize_exchange(exchange)
    payload = bytes([exchange_to_wire(resolved), 0x00]) + start.to_bytes(2, "little", signed=False)
    return RequestFrame(msg_id=msg_id, msg_type=TYPE_CODE, data=payload)


def parse_code_payload(exchange: str | Exchange, response: ResponseFrame) -> list[SecurityCode]:
    payload = response.data
    if len(payload) < 2:
        raise ProtocolError("invalid code payload")

    resolved = normalize_exchange(exchange)
    count = little_u16(payload[:2])
    offset = 2
    items: list[SecurityCode] = []

    for _ in range(count):
        record = payload[offset : offset + CODE_RECORD_SIZE]
        if len(record) < CODE_RECORD_SIZE:
            raise ProtocolError("truncated code record")
        offset += CODE_RECORD_SIZE

        last_price_raw = little_u32(record[21:25])
        items.append(
            SecurityCode(
                exchange=resolved.value,
                code=record[:6].decode("ascii"),
                name=decode_gbk_text(record[8:16]),
                multiple=little_u16(record[6:8]),
                decimal=int.from_bytes(record[20:21], "little", signed=True),
                last_price=get_volume2(last_price_raw),
                last_price_raw=last_price_raw,
            )
        )

    return items