"""K-line command builder and parser."""

from __future__ import annotations

import re
from datetime import date, datetime

from eltdx.exceptions import ProtocolError
from eltdx.models import KlineBar, KlineSeries
from eltdx.protocol.constants import MAX_KLINE_PAGE_SIZE, TYPE_KLINES
from eltdx.protocol.frame import RequestFrame, ResponseFrame
from eltdx.protocol.unit import (
    consume_price,
    date_from_yyyymmdd,
    decode_kline_datetime,
    get_volume,
    little_u16,
    little_u32,
    market_to_id,
    milli_to_float,
    normalize_market,
    split_code,
    yyyymmdd,
)

PERIOD_ALIASES: dict[str, tuple[int, int]] = {
    "5m": (0, 1),
    "5min": (0, 1),
    "15m": (1, 1),
    "15min": (1, 1),
    "30m": (2, 1),
    "30min": (2, 1),
    "60m": (3, 1),
    "60min": (3, 1),
    "1h": (3, 1),
    "hour": (3, 1),
    "day": (4, 1),
    "1d": (4, 1),
    "d": (4, 1),
    "daily": (4, 1),
    "week": (5, 1),
    "1w": (5, 1),
    "w": (5, 1),
    "month": (6, 1),
    "1mo": (6, 1),
    "mo": (6, 1),
    "1m": (7, 1),
    "1min": (7, 1),
    "minute": (7, 1),
    "quarter": (10, 1),
    "1q": (10, 1),
    "q": (10, 1),
    "year": (11, 1),
    "1y": (11, 1),
    "y": (11, 1),
}

ADJUST_TO_RAW = {
    None: 0,
    "none": 0,
    "": 0,
    "qfq": 1,
    "front": 1,
    "hfq": 2,
    "back": 2,
    "fixed_qfq": 3,
    "fixed_front": 3,
    "fixed_hfq": 4,
    "fixed_back": 4,
}
RAW_TO_ADJUST = {0: "none", 1: "qfq", 2: "hfq", 3: "fixed_qfq", 4: "fixed_hfq"}


def build_klines_frame(payload: dict, msg_id: int) -> RequestFrame:
    market_id, _, number = split_code(payload["code"])
    period_raw, period_param_raw = normalize_period(payload.get("period", "day"))
    start = int(payload.get("start", 0))
    count = int(payload.get("count", 800))
    if start < 0 or start > 0xFFFF:
        raise ValueError("start must be between 0 and 65535")
    if count <= 0 or count > MAX_KLINE_PAGE_SIZE:
        raise ValueError(f"count must be between 1 and {MAX_KLINE_PAGE_SIZE}")

    adjust_mode_raw = normalize_adjust(payload.get("adjust"))
    anchor_date_raw = normalize_anchor_date(payload.get("anchor_date", payload.get("anchor_date_raw", 0)))
    data = (
        market_id.to_bytes(2, "little", signed=False)
        + number.encode("ascii")
        + period_raw.to_bytes(2, "little", signed=False)
        + period_param_raw.to_bytes(2, "little", signed=False)
        + start.to_bytes(2, "little", signed=False)
        + count.to_bytes(2, "little", signed=False)
        + adjust_mode_raw.to_bytes(2, "little", signed=False)
        + anchor_date_raw.to_bytes(4, "little", signed=False)
        + b"\x00" * 20
    )
    return RequestFrame(msg_id=msg_id, msg_type=TYPE_KLINES, data=data)


def parse_klines_payload(response: ResponseFrame, request_payload: dict | None = None) -> KlineSeries:
    request_payload = request_payload or {}
    payload = response.data
    if len(payload) < 2:
        raise ProtocolError("invalid klines payload")

    requested_code = request_payload.get("code", "sz000001")
    market_id, exchange, number = split_code(requested_code)
    period_raw, period_param_raw = normalize_period(request_payload.get("period", "day"))
    start = int(request_payload.get("start", 0))
    request_count = int(request_payload.get("count", 800))
    adjust_mode_raw = normalize_adjust(request_payload.get("adjust"))
    anchor_date_raw = normalize_anchor_date(request_payload.get("anchor_date", request_payload.get("anchor_date_raw", 0)))
    kind = str(request_payload.get("kind", "stock")).lower()
    index_mode = kind == "index"

    count = little_u16(payload[:2])
    offset = 2
    last_close_milli = 0
    bars: list[KlineBar] = []
    for _ in range(count):
        record_start = offset
        if offset + 4 > len(payload):
            raise ProtocolError("truncated kline time field")
        item_time = decode_kline_datetime(payload[offset : offset + 4], period_raw)
        offset += 4

        open_delta, offset = consume_price(payload, offset)
        close_delta, offset = consume_price(payload, offset)
        high_delta, offset = consume_price(payload, offset)
        low_delta, offset = consume_price(payload, offset)

        item_last_close = last_close_milli if bars else None
        open_milli = last_close_milli + open_delta
        close_milli = open_milli + close_delta
        high_milli = open_milli + high_delta
        low_milli = open_milli + low_delta
        last_close_milli = close_milli

        if offset + 8 > len(payload):
            raise ProtocolError("truncated kline volume or amount field")
        volume_raw = little_u32(payload[offset : offset + 4])
        volume_wire_value = get_volume(volume_raw)
        offset += 4
        amount_raw = little_u32(payload[offset : offset + 4])
        amount = get_volume(amount_raw)
        offset += 4

        up_count = None
        down_count = None
        if index_mode:
            if offset + 4 > len(payload):
                raise ProtocolError("truncated kline index breadth field")
            up_count = little_u16(payload[offset : offset + 2])
            down_count = little_u16(payload[offset + 2 : offset + 4])
            offset += 4

        bars.append(
            KlineBar(
                time=item_time,
                open=milli_to_float(open_milli),
                close=milli_to_float(close_milli),
                high=milli_to_float(high_milli),
                low=milli_to_float(low_milli),
                open_price_milli=open_milli,
                close_price_milli=close_milli,
                high_price_milli=high_milli,
                low_price_milli=low_milli,
                last_close_price_milli=item_last_close,
                volume_raw=volume_raw,
                amount_raw=amount_raw,
                volume_wire_value=volume_wire_value,
                volume_lots=volume_wire_value / 100.0,
                amount=amount,
                open_delta_raw=open_delta,
                close_delta_raw=close_delta,
                high_delta_raw=high_delta,
                low_delta_raw=low_delta,
                up_count=up_count,
                down_count=down_count,
                record_hex=payload[record_start:offset].hex(),
            )
        )

    if offset != len(payload):
        raise ProtocolError(f"unexpected trailing kline payload bytes: {len(payload) - offset}")

    return KlineSeries(
        exchange=exchange,
        market_id=market_id,
        code=number,
        period_raw=period_raw,
        period_param_raw=period_param_raw,
        period_name=period_name(period_raw, period_param_raw),
        start=start,
        request_count=request_count,
        adjust_mode_raw=adjust_mode_raw,
        adjust_mode=RAW_TO_ADJUST.get(adjust_mode_raw, f"unknown_{adjust_mode_raw}"),
        anchor_date_raw=anchor_date_raw,
        anchor_date=None if anchor_date_raw == 0 else date_from_yyyymmdd(anchor_date_raw),
        bars=tuple(bars),
        raw_payload=payload,
    )


def normalize_period(value) -> tuple[int, int]:
    if isinstance(value, tuple) and len(value) == 2:
        return int(value[0]), int(value[1])
    key = str(value).strip().lower()
    if key in PERIOD_ALIASES:
        return PERIOD_ALIASES[key]

    match = re.fullmatch(r"(\d+)(m|min|d|s)", key)
    if not match:
        raise ProtocolError(f"invalid kline period: {value!r}")
    number = int(match.group(1))
    unit = match.group(2)
    if unit in {"m", "min"}:
        if number == 1:
            return 7, 1
        if number in {5, 15, 30, 60}:
            return PERIOD_ALIASES[f"{number}m"]
        return 8, number
    if unit == "d":
        if number == 1:
            return 4, 1
        return 9, number
    if unit == "s":
        return 13, number
    raise ProtocolError(f"invalid kline period: {value!r}")


def period_name(period_raw: int, period_param_raw: int) -> str:
    names = {
        (0, 1): "5m",
        (1, 1): "15m",
        (2, 1): "30m",
        (3, 1): "60m",
        (4, 1): "day",
        (5, 1): "week",
        (6, 1): "month",
        (7, 1): "1m",
        (10, 1): "quarter",
        (11, 1): "year",
    }
    if (period_raw, period_param_raw) in names:
        return names[(period_raw, period_param_raw)]
    if period_raw == 8:
        return f"{period_param_raw}m"
    if period_raw == 9:
        return f"{period_param_raw}d"
    if period_raw == 13:
        return f"{period_param_raw}s"
    return f"{period_raw}/{period_param_raw}"


def normalize_adjust(value) -> int:
    if isinstance(value, int):
        return value
    key = None if value is None else str(value).strip().lower()
    try:
        return ADJUST_TO_RAW[key]
    except KeyError as exc:
        raise ProtocolError(f"invalid adjust mode: {value!r}") from exc


def normalize_anchor_date(value) -> int:
    if value in (None, "", 0):
        return 0
    if isinstance(value, int):
        return value
    if isinstance(value, (date, datetime)):
        return yyyymmdd(value)
    return yyyymmdd(str(value))
