from __future__ import annotations

from datetime import date, datetime
from unittest.mock import Mock

from eltdx.client import TdxClient
from eltdx.models import TradeItem, TradeResponse
from eltdx.protocol.unit import SHANGHAI_TZ
from eltdx.trade_kline import build_trade_minute_kline


def _trade(hour: int, minute: int, price_milli: int, volume: int, *, order_count: int | None, status: int = 0) -> TradeItem:
    return TradeItem(
        time=datetime(2026, 3, 6, hour, minute, tzinfo=SHANGHAI_TZ),
        price=price_milli / 1000.0,
        price_milli=price_milli,
        volume=volume,
        status=status,
        side="buy" if status == 0 else "sell",
        order_count=order_count,
    )


def test_build_trade_minute_kline_aggregates_bucket_values() -> None:
    response = TradeResponse(
        count=3,
        trading_date=date(2026, 3, 6),
        items=[
            _trade(9, 25, 10_000, 1, order_count=2),
            _trade(9, 26, 11_000, 2, order_count=3),
            _trade(9, 27, 0, 99, order_count=99),
        ],
    )

    bars = build_trade_minute_kline(response)
    first = bars.items[0]
    second = bars.items[1]

    assert bars.count == 241
    assert first.time.isoformat() == "2026-03-06T09:30:00+08:00"
    assert first.last_close_price_milli == 10_000
    assert first.open_price_milli == 10_000
    assert first.high_price_milli == 11_000
    assert first.low_price_milli == 10_000
    assert first.close_price_milli == 11_000
    assert first.volume == 3
    assert first.amount_milli == 3_200_000
    assert first.order_count == 5

    assert second.time.isoformat() == "2026-03-06T09:31:00+08:00"
    assert second.last_close_price_milli == 11_000
    assert second.open_price_milli == 11_000
    assert second.close_price_milli == 11_000
    assert second.volume == 0
    assert second.amount_milli == 0
    assert second.order_count is None


def test_build_trade_minute_kline_maps_session_boundaries_and_gaps() -> None:
    response = TradeResponse(
        count=5,
        trading_date=date(2026, 3, 6),
        items=[
            _trade(9, 25, 10_000, 1, order_count=1),
            _trade(9, 30, 10_100, 2, order_count=2),
            _trade(11, 29, 10_200, 3, order_count=3),
            _trade(13, 0, 10_300, 4, order_count=4),
            _trade(15, 1, 10_400, 5, order_count=5),
        ],
    )

    bars = build_trade_minute_kline(response)
    by_time = {item.time.strftime("%H:%M"): item for item in bars.items}

    assert bars.count == 241
    assert bars.items[0].time.strftime("%H:%M") == "09:30"
    assert bars.items[120].time.strftime("%H:%M") == "11:30"
    assert bars.items[121].time.strftime("%H:%M") == "13:01"
    assert bars.items[-1].time.strftime("%H:%M") == "15:00"

    assert by_time["09:30"].close_price_milli == 10_000
    assert by_time["09:31"].close_price_milli == 10_100
    assert by_time["09:32"].close_price_milli == 10_100
    assert by_time["11:30"].close_price_milli == 10_200
    assert by_time["13:01"].close_price_milli == 10_300
    assert by_time["15:00"].close_price_milli == 10_400
    assert by_time["15:00"].order_count == 5


def test_build_trade_minute_kline_keeps_order_count_none_for_history_like_data() -> None:
    response = TradeResponse(
        count=2,
        trading_date=date(2026, 3, 6),
        items=[
            _trade(9, 25, 10_000, 1, order_count=None),
            _trade(9, 26, 10_100, 2, order_count=None),
        ],
    )

    bars = build_trade_minute_kline(response)

    assert bars.items[0].order_count is None
    assert bars.items[0].volume == 3
    assert bars.items[1].order_count is None


def test_client_trade_minute_kline_wrappers() -> None:
    client = TdxClient()
    response = TradeResponse(
        count=1,
        trading_date=date(2026, 3, 6),
        items=[_trade(9, 25, 10_000, 1, order_count=1)],
    )
    client.get_trade_all = Mock(return_value=response)
    client.get_history_trade_day = Mock(return_value=response)

    live = client.get_trade_minute_kline("sz000001")
    hist = client.get_history_trade_minute_kline("sz000001", "20260306")

    client.get_trade_all.assert_called_once_with("sz000001")
    client.get_history_trade_day.assert_called_once_with("sz000001", "20260306")
    assert live.count == 241
    assert hist.count == 241