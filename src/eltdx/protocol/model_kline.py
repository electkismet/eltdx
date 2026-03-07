from __future__ import annotations

from ..enums import KlinePeriod
from ..exceptions import ProtocolError
from ..models import KlineItem, KlineResponse
from .constants import TYPE_KLINE
from .frame import RequestFrame, ResponseFrame
from .unit import (
    consume_price,
    decode_code,
    decode_kline_datetime,
    fix_kline_times,
    get_volume,
    little_u16,
    little_u32,
    milli_to_float,
    normalize_kline_period,
)

PERIOD_TO_WIRE = {
    KlinePeriod.MINUTE_5: 0,
    KlinePeriod.MINUTE_15: 1,
    KlinePeriod.MINUTE_30: 2,
    KlinePeriod.MINUTE_60: 3,
    KlinePeriod.WEEK: 5,
    KlinePeriod.MONTH: 6,
    KlinePeriod.MINUTE_1: 7,
    KlinePeriod.DAY: 9,
    KlinePeriod.QUARTER: 10,
    KlinePeriod.YEAR: 11,
}
INTRADAY_WIRE_PERIODS = {0, 1, 2, 3, 7}
VALID_KLINE_KINDS = {"stock", "index"}


def build_kline_frame(period: str | KlinePeriod, code: str, start: int, count: int, msg_id: int) -> RequestFrame:
    if start < 0:
        raise ValueError("start must be >= 0")
    if count <= 0:
        raise ValueError("count must be > 0")
    if count > 800:
        raise ValueError("count must be <= 800")

    resolved_period = normalize_kline_period(period)
    exchange, number = decode_code(code)
    payload = bytes([exchange, 0x00])
    payload += number.encode("ascii")
    payload += bytes([PERIOD_TO_WIRE[resolved_period], 0x00, 0x01, 0x00])
    payload += start.to_bytes(2, "little", signed=False)
    payload += count.to_bytes(2, "little", signed=False)
    payload += b"\x00" * 10
    return RequestFrame(msg_id=msg_id, msg_type=TYPE_KLINE, data=payload)


def parse_kline_payload(
    period: str | KlinePeriod,
    code: str,
    response: ResponseFrame,
    *,
    kind: str = "stock",
    include_raw: bool = False,
) -> KlineResponse:
    payload = response.data
    if len(payload) < 2:
        raise ProtocolError("invalid kline payload")
    if kind not in VALID_KLINE_KINDS:
        raise ValueError("kind must be 'stock' or 'index'")

    resolved_period = normalize_kline_period(period)
    wire_period = PERIOD_TO_WIRE[resolved_period]
    count = little_u16(payload[:2])
    offset = 2
    last_close_milli = 0
    index_mode = kind == "index"
    items: list[KlineItem] = []

    for _ in range(count):
        if offset + 4 > len(payload):
            raise ProtocolError("truncated kline time field")
        try:
            item_time = decode_kline_datetime(payload[offset : offset + 4], resolved_period)
        except ValueError as exc:
            raise ProtocolError("invalid kline time field") from exc
        offset += 4

        open_delta, offset = consume_price(payload, offset)
        close_delta, offset = consume_price(payload, offset)
        high_delta, offset = consume_price(payload, offset)
        low_delta, offset = consume_price(payload, offset)

        item_last_close_milli = last_close_milli or None
        open_price_milli = open_delta + last_close_milli
        close_price_milli = last_close_milli + open_delta + close_delta
        high_price_milli = last_close_milli + open_delta + high_delta
        low_price_milli = last_close_milli + open_delta + low_delta
        last_close_milli = close_price_milli

        if offset + 4 > len(payload):
            raise ProtocolError("truncated kline volume field")
        volume = int(get_volume(little_u32(payload[offset : offset + 4])))
        offset += 4
        if wire_period in INTRADAY_WIRE_PERIODS:
            volume //= 100

        if offset + 4 > len(payload):
            raise ProtocolError("truncated kline amount field")
        amount_milli = int(get_volume(little_u32(payload[offset : offset + 4])) * 1000)
        offset += 4

        up_count = None
        down_count = None
        if index_mode:
            volume *= 100
            if offset + 4 > len(payload):
                raise ProtocolError("truncated kline breadth field")
            up_count = little_u16(payload[offset : offset + 2])
            down_count = little_u16(payload[offset + 2 : offset + 4])
            offset += 4

        items.append(
            KlineItem(
                time=item_time,
                open_price=milli_to_float(open_price_milli),
                open_price_milli=open_price_milli,
                high_price=milli_to_float(high_price_milli),
                high_price_milli=high_price_milli,
                low_price=milli_to_float(low_price_milli),
                low_price_milli=low_price_milli,
                close_price=milli_to_float(close_price_milli),
                close_price_milli=close_price_milli,
                last_close_price=milli_to_float(item_last_close_milli) if item_last_close_milli is not None else None,
                last_close_price_milli=item_last_close_milli,
                volume=volume,
                amount=milli_to_float(amount_milli),
                amount_milli=amount_milli,
                order_count=None,
                up_count=up_count,
                down_count=down_count,
            )
        )

    if offset != len(payload):
        raise ProtocolError(f"unexpected trailing kline payload bytes: {len(payload) - offset}")

    fix_kline_times(items)
    return KlineResponse(
        count=len(items),
        items=items,
        raw_frame_hex=response.raw.hex() if include_raw else None,
        raw_payload_hex=payload.hex() if include_raw else None,
    )
