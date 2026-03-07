from __future__ import annotations

from ..models import TradeItem, TradeResponse
from .constants import TYPE_HISTORY_TRADE, TYPE_TRADE
from .frame import RequestFrame, ResponseFrame
from .unit import (
    clock_minutes_to_datetime,
    consume_price,
    consume_varint,
    decode_code,
    little_u16,
    milli_to_float,
    normalize_trading_date,
    price_divisor,
)


def build_trade_frame(code: str, start: int, count: int, msg_id: int) -> RequestFrame:
    exchange, number = decode_code(code)
    payload = bytes([exchange, 0x00]) + number.encode("ascii")
    payload += start.to_bytes(2, "little", signed=False)
    payload += count.to_bytes(2, "little", signed=False)
    return RequestFrame(msg_id=msg_id, msg_type=TYPE_TRADE, data=payload)


def build_history_trade_frame(code: str, trading_date, start: int, count: int, msg_id: int) -> RequestFrame:
    date_text, _ = normalize_trading_date(trading_date)
    exchange, number = decode_code(code)
    payload = int(date_text).to_bytes(4, "little", signed=False)
    payload += bytes([exchange, 0x00])
    payload += number.encode("ascii")
    payload += start.to_bytes(2, "little", signed=False)
    payload += count.to_bytes(2, "little", signed=False)
    return RequestFrame(msg_id=msg_id, msg_type=TYPE_HISTORY_TRADE, data=payload)


def parse_trade_payload(code: str, trading_date, response: ResponseFrame, *, include_raw: bool = False) -> TradeResponse:
    return _parse_trade_response(code, trading_date, response, include_raw=include_raw, history=False)


def parse_history_trade_payload(code: str, trading_date, response: ResponseFrame, *, include_raw: bool = False) -> TradeResponse:
    return _parse_trade_response(code, trading_date, response, include_raw=include_raw, history=True)


def _parse_trade_response(code: str, trading_date, response: ResponseFrame, *, include_raw: bool, history: bool) -> TradeResponse:
    payload = response.data
    count = little_u16(payload[:2])
    offset = 6 if history else 2
    _, trading_day = normalize_trading_date(trading_date)
    divisor = price_divisor(code)
    price_accumulator = 0
    items: list[TradeItem] = []

    for _ in range(count):
        total_minutes = little_u16(payload[offset : offset + 2])
        offset += 2
        price_delta, offset = consume_price(payload, offset)
        price_accumulator += price_delta * 10
        price_milli = price_accumulator // divisor
        volume, offset = consume_varint(payload, offset)

        if history:
            status, offset = consume_varint(payload, offset)
            order_count = None
        else:
            order_count, offset = consume_varint(payload, offset)
            status, offset = consume_varint(payload, offset)

        _, offset = consume_varint(payload, offset)
        items.append(
            TradeItem(
                time=clock_minutes_to_datetime(trading_day, total_minutes),
                price=milli_to_float(price_milli),
                price_milli=price_milli,
                volume=volume,
                status=status,
                side=_status_to_side(status),
                order_count=order_count,
            )
        )

    return TradeResponse(
        count=len(items),
        trading_date=trading_day,
        items=items,
        raw_frame_hex=response.raw.hex() if include_raw else None,
        raw_payload_hex=payload.hex() if include_raw else None,
    )


def _status_to_side(status: int) -> str | None:
    if status == 0:
        return "buy"
    if status == 1:
        return "sell"
    if status == 2:
        return "neutral"
    return None