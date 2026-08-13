"""Trade tick command builders and parsers."""

from __future__ import annotations

from datetime import date, datetime

from eltdx.exceptions import ProtocolError
from eltdx.models import TradePage, TradeTick
from eltdx.protocol.constants import MAX_TRADE_PAGE_SIZE, TYPE_HISTORICAL_TICKS, TYPE_TODAY_TICKS
from eltdx.protocol.frame import RequestFrame, ResponseFrame
from eltdx.protocol.unit import (
    consume_price,
    consume_varint,
    date_from_yyyymmdd,
    little_f32,
    little_u16,
    market_to_id,
    split_code,
    yyyymmdd,
)


def build_today_ticks_frame(payload: dict, msg_id: int) -> RequestFrame:
    market_id, _, number = split_code(payload["code"])
    start = _u16(payload.get("start", 0), "start")
    count = _trade_count(payload.get("count", 115))
    data = bytes([market_id, 0]) + number.encode("ascii") + start.to_bytes(2, "little") + count.to_bytes(2, "little")
    return RequestFrame(msg_id=msg_id, msg_type=TYPE_TODAY_TICKS, data=data)


def build_historical_ticks_frame(payload: dict, msg_id: int) -> RequestFrame:
    market_id, _, number = split_code(payload["code"])
    trading_date_raw = yyyymmdd(payload.get("trading_date"))
    start = _u16(payload.get("start", 0), "start")
    count = _trade_count(payload.get("count", 900))
    data = (
        trading_date_raw.to_bytes(4, "little")
        + market_id.to_bytes(2, "little")
        + number.encode("ascii")
        + start.to_bytes(2, "little")
        + count.to_bytes(2, "little")
    )
    return RequestFrame(msg_id=msg_id, msg_type=TYPE_HISTORICAL_TICKS, data=data)


def parse_today_ticks_payload(response: ResponseFrame, request_payload: dict | None = None) -> TradePage:
    request_payload = request_payload or {}
    payload = response.data
    if len(payload) < 2:
        raise ProtocolError("invalid today ticks payload")

    market_id, exchange, number = split_code(request_payload.get("code", "sz000001"))
    start = int(request_payload.get("start", 0))
    request_count = int(request_payload.get("count", 115))
    record_count = little_u16(payload[:2])
    ticks, offset = _parse_tick_records(
        payload,
        offset=2,
        record_count=record_count,
        start=start,
        trading_day=None,
        tail_field_name="unknown_tail_raw",
    )
    if offset != len(payload):
        raise ProtocolError(f"unexpected trailing today ticks payload bytes: {len(payload) - offset}")
    return TradePage(
        exchange=exchange,
        market_id=market_id,
        code=number,
        start=start,
        request_count=request_count,
        ticks=tuple(ticks),
        trading_date=None,
        raw_payload=payload,
    )


def parse_historical_ticks_payload(response: ResponseFrame, request_payload: dict | None = None) -> TradePage:
    request_payload = request_payload or {}
    payload = response.data
    if len(payload) < 6:
        raise ProtocolError("invalid historical ticks payload")

    market_id, exchange, number = split_code(request_payload.get("code", "sz000001"))
    trading_date_raw = yyyymmdd(request_payload.get("trading_date"))
    trading_day = date_from_yyyymmdd(trading_date_raw)
    if trading_day is None:
        raise ProtocolError(f"invalid trading date: {trading_date_raw}")
    start = int(request_payload.get("start", 0))
    request_count = int(request_payload.get("count", 900))
    record_count = little_u16(payload[:2])
    price_base_raw_f32 = little_f32(payload[2:6])
    ticks, offset = _parse_tick_records(
        payload,
        offset=6,
        record_count=record_count,
        start=start,
        trading_day=trading_day,
        tail_field_name="reserved_zero",
    )
    if offset != len(payload):
        raise ProtocolError(f"unexpected trailing historical ticks payload bytes: {len(payload) - offset}")
    return TradePage(
        exchange=exchange,
        market_id=market_id,
        code=number,
        start=start,
        request_count=request_count,
        ticks=tuple(ticks),
        trading_date=trading_day,
        price_base_raw_f32=price_base_raw_f32,
        raw_payload=payload,
    )


def _parse_tick_records(
    payload: bytes,
    *,
    offset: int,
    record_count: int,
    start: int,
    trading_day: date | None,
    tail_field_name: str,
) -> tuple[list[TradeTick], int]:
    ticks: list[TradeTick] = []
    price_acc_raw = 0
    for index in range(record_count):
        record_start = offset
        if offset + 2 > len(payload):
            raise ProtocolError("truncated tick time field")
        time_minutes = little_u16(payload[offset : offset + 2])
        offset += 2
        price_delta_raw, offset = consume_price(payload, offset)
        volume, offset = consume_varint(payload, offset)
        order_count, offset = consume_varint(payload, offset)
        status_raw, offset = consume_varint(payload, offset)
        tail_value, offset = consume_varint(payload, offset)
        price_acc_raw += price_delta_raw
        price = price_acc_raw / 100.0
        time_label = minute_of_day_label(time_minutes)
        ticks.append(
            TradeTick(
                index=index,
                absolute_index=start + index,
                time_minutes=time_minutes,
                time_label=time_label,
                trade_datetime=combine_trade_datetime(trading_day, time_minutes),
                price=price,
                price_milli=round(price * 1000),
                volume=volume,
                order_count=order_count,
                status_raw=status_raw,
                side=trade_side(status_raw),
                price_delta_raw=price_delta_raw,
                price_acc_raw=price_acc_raw,
                unknown_tail_raw=tail_value if tail_field_name == "unknown_tail_raw" else None,
                reserved_zero=tail_value if tail_field_name == "reserved_zero" else None,
                record_hex=payload[record_start:offset].hex(),
                event_kind=trade_event_kind(status_raw, time_minutes),
                auction_matched_volume=volume if status_raw == 8 else None,
                auction_unmatched_signed_volume=order_count if status_raw == 8 else None,
            )
        )
    return ticks, offset


def minute_of_day_label(value: int, *, with_seconds: int | None = None) -> str:
    if value < 0:
        raise ProtocolError(f"invalid minute of day: {value}")
    hour = value // 60
    minute = value % 60
    if with_seconds is None:
        return f"{hour:02d}:{minute:02d}"
    return f"{hour:02d}:{minute:02d}:{with_seconds:02d}"


def combine_trade_datetime(trading_day: date | None, time_minutes: int) -> datetime | None:
    if trading_day is None:
        return None
    return datetime(trading_day.year, trading_day.month, trading_day.day, time_minutes // 60, time_minutes % 60)


def trade_side(status_raw: int) -> str:
    return {0: "buy", 1: "sell", 2: "neutral"}.get(status_raw, f"status_{status_raw}")


def trade_event_kind(status_raw: int, time_minutes: int) -> str:
    """Classify the shared trade wire record without discarding raw fields."""

    if status_raw == 8:
        return "auction_snapshot"
    if time_minutes == 9 * 60 + 25:
        return "opening_match"
    return "trade"


def _u16(value, name: str) -> int:
    parsed = int(value)
    if parsed < 0 or parsed > 0xFFFF:
        raise ValueError(f"{name} must be between 0 and 65535")
    return parsed


def _trade_count(value) -> int:
    parsed = int(value)
    if parsed <= 0 or parsed > MAX_TRADE_PAGE_SIZE:
        raise ValueError(f"count must be between 1 and {MAX_TRADE_PAGE_SIZE}")
    return parsed
