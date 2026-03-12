from __future__ import annotations

from datetime import datetime

from ..models import CallAuctionItem, CallAuctionResponse
from .constants import TYPE_CALL_AUCTION
from .frame import RequestFrame, ResponseFrame
from .unit import SHANGHAI_TZ, decode_code, little_f32, little_i32, little_u16, little_u32


def build_call_auction_frame(code: str, msg_id: int) -> RequestFrame:
    exchange, number = decode_code(code)
    payload = bytes([exchange, 0x00]) + number.encode("ascii")
    payload += bytes(
        [
            0x00,
            0x00,
            0x00,
            0x00,
            0x03,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0xF4,
            0x01,
            0x00,
            0x00,
        ]
    )
    return RequestFrame(msg_id=msg_id, msg_type=TYPE_CALL_AUCTION, data=payload)


def parse_call_auction_payload(
    code: str,
    response: ResponseFrame,
    *,
    include_raw: bool = False,
    now: datetime | None = None,
) -> CallAuctionResponse:
    del code
    payload = response.data
    count = little_u16(payload[:2])
    offset = 2
    items: list[CallAuctionItem] = []
    now = now or datetime.now(SHANGHAI_TZ)

    for _ in range(count):
        record = payload[offset : offset + 16]
        if len(record) < 16:
            break
        offset += 16

        minutes = little_u16(record[:2])
        hour = minutes // 60
        minute = minutes % 60
        second = record[15]
        price_milli = int(round(little_f32(record[2:6]) * 1000))
        # TDX uses 4-byte fields for both matched and unmatched volume here.
        match = little_u32(record[6:10])
        unmatched_signed = little_i32(record[10:14])
        flag = 1 if unmatched_signed >= 0 else -1
        unmatched = abs(unmatched_signed)

        items.append(
            CallAuctionItem(
                time=now.replace(hour=hour, minute=minute, second=second, microsecond=0),
                price=price_milli / 1000.0,
                price_milli=price_milli,
                match=match,
                unmatched=unmatched,
                flag=flag,
                raw_hex=record.hex() if include_raw else None,
            )
        )

    return CallAuctionResponse(
        count=len(items),
        items=items,
        raw_frame_hex=response.raw.hex() if include_raw else None,
        raw_payload_hex=payload.hex() if include_raw else None,
    )
