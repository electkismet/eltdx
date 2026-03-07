from __future__ import annotations

from ..exceptions import ProtocolError
from ..models import MinuteItem, MinuteResponse
from .constants import TYPE_HISTORY_MINUTE, TYPE_MINUTE
from .frame import RequestFrame, ResponseFrame
from .unit import consume_price, consume_varint, decode_code, little_u16, milli_to_float, minute_index_to_datetime, normalize_trading_date


def build_minute_frame(code: str, msg_id: int) -> RequestFrame:
    exchange, number = decode_code(code)
    payload = bytes([exchange, 0x00]) + number.encode("ascii") + b"\x00\x00\x00\x00"
    return RequestFrame(msg_id=msg_id, msg_type=TYPE_MINUTE, data=payload)


def build_history_minute_frame(code: str, trading_date, msg_id: int) -> RequestFrame:
    date_text, _ = normalize_trading_date(trading_date)
    exchange, number = decode_code(code)
    payload = int(date_text).to_bytes(4, "little", signed=False)
    payload += bytes([exchange])
    payload += number.encode("ascii")
    return RequestFrame(msg_id=msg_id, msg_type=TYPE_HISTORY_MINUTE, data=payload)


def parse_minute_payload(response: ResponseFrame, trading_date=None, *, include_raw: bool = False) -> MinuteResponse:
    payload = response.data
    if len(payload) < 11:
        raise ProtocolError("invalid minute payload")

    count = little_u16(payload[:2])
    _, trading_day = normalize_trading_date(trading_date)
    offset = _find_minute_payload_offset(payload, count)
    return _parse_minute_records(payload, offset=offset, count=count, trading_day=trading_day, response=response, include_raw=include_raw)


def parse_history_minute_payload(trading_date, response: ResponseFrame, *, include_raw: bool = False) -> MinuteResponse:
    payload = response.data
    if len(payload) < 6:
        raise ProtocolError("invalid history minute payload")

    count = little_u16(payload[:2])
    _, trading_day = normalize_trading_date(trading_date)
    return _parse_minute_records(payload, offset=6, count=count, trading_day=trading_day, response=response, include_raw=include_raw)


def _parse_minute_records(payload: bytes, *, offset: int, count: int, trading_day, response: ResponseFrame, include_raw: bool) -> MinuteResponse:
    items: list[MinuteItem] = []
    price_accumulator = 0

    for index in range(count):
        price_delta, offset = consume_price(payload, offset)
        _, offset = consume_price(payload, offset)
        volume, offset = consume_varint(payload, offset)
        price_accumulator += price_delta
        price_milli = price_accumulator * 10

        items.append(
            MinuteItem(
                time=minute_index_to_datetime(trading_day, index),
                price=milli_to_float(price_milli),
                price_milli=price_milli,
                volume=volume,
            )
        )

    if offset != len(payload):
        raise ProtocolError("unexpected trailing bytes in minute payload")

    return MinuteResponse(
        count=len(items),
        trading_date=trading_day,
        items=items,
        raw_frame_hex=response.raw.hex() if include_raw else None,
        raw_payload_hex=payload.hex() if include_raw else None,
    )


def _find_minute_payload_offset(payload: bytes, count: int) -> int:
    if count == 0:
        return len(payload)

    candidates: list[int] = []
    scan_end = min(len(payload), 160)

    for offset in range(11, scan_end):
        if payload[offset - 1] & 0x80:
            continue

        current = offset
        try:
            for _ in range(count):
                _, current = consume_price(payload, current)
                _, current = consume_price(payload, current)
                _, current = consume_varint(payload, current)
        except ProtocolError:
            continue

        if current == len(payload):
            candidates.append(offset)

    if not candidates:
        raise ProtocolError("unable to locate minute records in live minute payload")
    return candidates[0]
