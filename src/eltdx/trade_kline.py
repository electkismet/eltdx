from __future__ import annotations

from .models import KlineItem, KlineResponse, TradeItem, TradeResponse
from .protocol.unit import clock_minutes_to_datetime

PRE_OPEN_CLAMP_MINUTE = 9 * 60 + 29
MORNING_END_MINUTE = 11 * 60 + 30
AFTERNOON_START_MINUTE = 13 * 60
SESSION_END_MINUTE = 15 * 60
MINUTE_BAR_KEYS = tuple(range(9 * 60 + 30, MORNING_END_MINUTE + 1)) + tuple(range(AFTERNOON_START_MINUTE + 1, SESSION_END_MINUTE + 1))


def build_trade_minute_kline(response: TradeResponse) -> KlineResponse:
    if not response.items:
        return KlineResponse(count=0, items=[])

    buckets = {minute_key: [] for minute_key in MINUTE_BAR_KEYS}
    opening_price_milli = _find_opening_price_milli(response.items)

    for item in response.items:
        total_minutes = item.time.hour * 60 + item.time.minute
        buckets[_bucket_minute(total_minutes)].append(item)

    items: list[KlineItem] = []
    last_close_milli = opening_price_milli
    for minute_key in MINUTE_BAR_KEYS:
        kline = _build_bucket_kline(response.trading_date, minute_key, buckets[minute_key], last_close_milli)
        items.append(kline)
        last_close_milli = kline.close_price_milli

    return KlineResponse(count=len(items), items=items)


def _find_opening_price_milli(items: list[TradeItem]) -> int:
    for item in items:
        if item.price_milli > 0:
            return item.price_milli
    return 0


def _bucket_minute(total_minutes: int) -> int:
    bucket = PRE_OPEN_CLAMP_MINUTE if total_minutes < PRE_OPEN_CLAMP_MINUTE else total_minutes
    bucket += 1
    if MORNING_END_MINUTE < bucket <= AFTERNOON_START_MINUTE:
        return MORNING_END_MINUTE
    if bucket > SESSION_END_MINUTE:
        return SESSION_END_MINUTE
    return bucket


def _build_bucket_kline(trading_date, minute_key: int, trades: list[TradeItem], last_close_milli: int) -> KlineItem:
    open_milli = last_close_milli
    high_milli = last_close_milli
    low_milli = last_close_milli
    close_milli = last_close_milli
    volume = 0
    amount_milli = 0
    order_count_sum = 0
    has_order_count = False
    first = True

    for trade in trades:
        if trade.price_milli <= 0:
            continue

        if first:
            open_milli = trade.price_milli
            high_milli = trade.price_milli
            low_milli = trade.price_milli
            close_milli = trade.price_milli
            first = False
        else:
            high_milli = max(high_milli, trade.price_milli)
            low_milli = min(low_milli, trade.price_milli)
            close_milli = trade.price_milli

        volume += trade.volume
        amount_milli += trade.price_milli * trade.volume * 100
        if trade.order_count is not None:
            order_count_sum += trade.order_count
            has_order_count = True

    return KlineItem(
        time=clock_minutes_to_datetime(trading_date, minute_key),
        open_price=open_milli / 1000.0,
        open_price_milli=open_milli,
        high_price=high_milli / 1000.0,
        high_price_milli=high_milli,
        low_price=low_milli / 1000.0,
        low_price_milli=low_milli,
        close_price=close_milli / 1000.0,
        close_price_milli=close_milli,
        last_close_price=last_close_milli / 1000.0,
        last_close_price_milli=last_close_milli,
        volume=volume,
        amount=amount_milli / 1000.0,
        amount_milli=amount_milli,
        order_count=order_count_sum if has_order_count else None,
    )