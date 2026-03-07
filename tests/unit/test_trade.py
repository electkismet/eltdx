from __future__ import annotations

from datetime import date
from itertools import cycle
from unittest.mock import Mock, call

from eltdx.client import TdxClient
from eltdx.models import TradeItem, TradeResponse
from eltdx.protocol.frame import ResponseFrame
from eltdx.protocol.model_trade import parse_history_trade_payload, parse_trade_payload


LIVE_PAYLOAD_HEX = "0a008003bb10a6010b00008003008803060000800300b104080000800300110200008003003a01000080034198020601008003019f02070000800341860107010081030109010000840341b0699f040200"
HIST_PAYLOAD_HEX = "0a00295c2b418003ba1025000080030025000080030098010000800341910201008003010200008003410501008003003d010080030023010081030103000084034191a1010200"


def test_parse_trade_payload() -> None:
    payload = bytes.fromhex(LIVE_PAYLOAD_HEX)
    response = ResponseFrame(
        control=0x1C,
        msg_id=4,
        msg_type=0x0FC5,
        zip_length=len(payload),
        length=len(payload),
        data=payload,
        raw=b"\xB1\xCB\x74\x00",
    )

    parsed = parse_trade_payload("sz000001", "20260306", response, include_raw=True)

    assert parsed.count == 10
    assert parsed.trading_date == date(2026, 3, 6)
    assert parsed.items[0].time.strftime("%H:%M") == "14:56"
    assert parsed.items[0].price_milli == 10830
    assert parsed.items[0].volume == 102
    assert parsed.items[0].order_count == 11
    assert parsed.items[0].status == 0
    assert parsed.items[0].side == "buy"
    assert parsed.items[-1].time.strftime("%H:%M") == "15:00"
    assert parsed.items[-1].status == 2
    assert parsed.items[-1].side == "neutral"
    assert parsed.raw_payload_hex == LIVE_PAYLOAD_HEX


def test_parse_history_trade_payload() -> None:
    payload = bytes.fromhex(HIST_PAYLOAD_HEX)
    response = ResponseFrame(
        control=0x1C,
        msg_id=5,
        msg_type=0x0FB5,
        zip_length=len(payload),
        length=len(payload),
        data=payload,
        raw=b"\xB1\xCB\x74\x00",
    )

    parsed = parse_history_trade_payload("sz000001", "20260305", response, include_raw=True)

    assert parsed.count == 10
    assert parsed.trading_date == date(2026, 3, 5)
    assert parsed.items[0].time.strftime("%H:%M") == "14:56"
    assert parsed.items[0].price_milli == 10820
    assert parsed.items[0].volume == 37
    assert parsed.items[0].order_count is None
    assert parsed.items[0].status == 0
    assert parsed.items[0].side == "buy"
    assert parsed.items[-1].time.strftime("%H:%M") == "15:00"
    assert parsed.items[-1].price_milli == 10810
    assert parsed.items[-1].status == 2
    assert parsed.raw_payload_hex == HIST_PAYLOAD_HEX


def test_collect_trade_pages_orders_pages_chronologically() -> None:
    client = TdxClient()

    pages = [
        TradeResponse(
            count=2,
            trading_date=date(2026, 3, 6),
            items=[
                TradeItem(time=None, price=0.0, price_milli=0, volume=1, status=0, side="buy"),
                TradeItem(time=None, price=0.0, price_milli=0, volume=2, status=0, side="buy"),
            ],
        ),
        TradeResponse(
            count=1,
            trading_date=date(2026, 3, 6),
            items=[TradeItem(time=None, price=0.0, price_milli=0, volume=3, status=1, side="sell")],
        ),
    ]

    def fetch_page(start: int, count: int) -> TradeResponse:
        assert count == 2
        return pages[start // 2]

    parsed = client._collect_trade_pages(fetch_page, "20260306", 2)

    assert parsed.count == 3
    assert [item.volume for item in parsed.items] == [3, 1, 2]


def test_collect_history_trade_pages_orders_pages_chronologically() -> None:
    client = TdxClient()

    pages = [
        TradeResponse(
            count=2,
            trading_date=date(2026, 3, 5),
            items=[
                TradeItem(time=None, price=0.0, price_milli=0, volume=10, status=0, side="buy"),
                TradeItem(time=None, price=0.0, price_milli=0, volume=20, status=0, side="buy"),
            ],
        ),
        TradeResponse(
            count=1,
            trading_date=date(2026, 3, 5),
            items=[TradeItem(time=None, price=0.0, price_milli=0, volume=30, status=1, side="sell")],
        ),
    ]

    def fetch_page(start: int, count: int) -> TradeResponse:
        assert count == 2
        return pages[start // 2]

    parsed = client._collect_trade_pages(fetch_page, "20260305", 2)

    assert parsed.count == 3
    assert [item.volume for item in parsed.items] == [30, 10, 20]


def test_get_trades_routes_history_requests_when_date_is_given() -> None:
    client = TdxClient()
    client.connect = Mock()
    connection = Mock()
    connection.request_history_trade.return_value = "history"
    client._connections = [connection]
    client._round_robin = cycle(range(1))

    parsed = client.get_trades("sz000001", "20260305", start=10, count=5000)

    assert parsed == "history"
    assert connection.request_history_trade.call_args_list == [call("sz000001", "20260305", 10, 2000, include_raw=False)]
