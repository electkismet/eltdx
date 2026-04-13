from __future__ import annotations

from datetime import date, datetime, timedelta, timezone
from itertools import cycle
from unittest.mock import Mock, call

import pytest

from eltdx.client import TdxClient
from eltdx.exceptions import ProtocolError
from eltdx.models import TradeItem, TradeProbe, TradeResponse
from eltdx.protocol.frame import ResponseFrame
from eltdx.protocol.model_trade import parse_history_trade_payload, parse_history_trade_probe_payload, parse_trade_payload


LIVE_PAYLOAD_HEX = "0a008003bb10a6010b00008003008803060000800300b104080000800300110200008003003a01000080034198020601008003019f02070000800341860107010081030109010000840341b0699f040200"
HIST_PAYLOAD_HEX = "0a00295c2b418003ba1025000080030025000080030098010000800341910201008003010200008003410501008003003d010080030023010081030103000084034191a1010200"
SHANGHAI_TZ = timezone(timedelta(hours=8))


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


def test_parse_history_trade_probe_payload_accepts_empty_header_only_payload() -> None:
    payload = bytes.fromhex("000000000000")
    response = ResponseFrame(
        control=0x1C,
        msg_id=6,
        msg_type=0x0FB5,
        zip_length=len(payload),
        length=len(payload),
        data=payload,
        raw=b"\xB1\xCB\x74\x00",
    )

    parsed = parse_history_trade_probe_payload("sz000001", "20260305", response)

    assert parsed.count == 0
    assert parsed.trading_date == date(2026, 3, 5)
    assert parsed.first_item is None
    assert parsed.item_0925 is None


def test_parse_history_trade_probe_payload_accepts_two_byte_empty_payload() -> None:
    payload = bytes.fromhex("0000")
    response = ResponseFrame(
        control=0x1C,
        msg_id=7,
        msg_type=0x0FB5,
        zip_length=len(payload),
        length=len(payload),
        data=payload,
        raw=b"\xB1\xCB\x74\x00",
    )

    parsed = parse_history_trade_probe_payload("sh600068", "20200102", response)

    assert parsed.count == 0
    assert parsed.trading_date == date(2020, 1, 2)
    assert parsed.first_item is None
    assert parsed.item_0925 is None


@pytest.mark.parametrize("payload_hex", ["", "00000000"])
def test_parse_history_trade_probe_payload_rejects_truncated_header(payload_hex: str) -> None:
    payload = bytes.fromhex(payload_hex)
    response = ResponseFrame(
        control=0x1C,
        msg_id=7,
        msg_type=0x0FB5,
        zip_length=len(payload),
        length=len(payload),
        data=payload,
        raw=b"\xB1\xCB\x74\x00",
    )

    with pytest.raises(ProtocolError, match="history trade probe payload too short"):
        parse_history_trade_probe_payload("sz000001", "20260305", response)


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


def test_get_auction_0925_fallback_hits_full_earliest_page() -> None:
    client = TdxClient()
    client.connect = Mock()
    connection = Mock()
    client._connections = [connection]
    client._round_robin = cycle(range(1))

    item_0925 = TradeItem(
        time=datetime(2026, 4, 9, 9, 25, tzinfo=SHANGHAI_TZ),
        price=10.0,
        price_milli=10000,
        volume=123,
        status=2,
        side="neutral",
    )
    empty_probe = TradeProbe(
        count=2000,
        trading_date=date(2026, 4, 9),
        first_item=None,
        item_0925=None,
    )
    hit_probe = TradeProbe(
        count=2000,
        trading_date=date(2026, 4, 9),
        first_item=item_0925,
        item_0925=item_0925,
    )
    responses = {
        0: empty_probe,
        2000: empty_probe,
        4000: empty_probe,
        6000: hit_probe,
    }

    call_order: list[int] = []

    def fake_request(code: str, trading_date: str, start: int, count: int) -> TradeProbe:
        call_order.append(start)
        return responses[start]

    connection.request_history_trade_probe.side_effect = fake_request

    result = client.get_auction_0925("sz000001", "20260409")

    assert result.has_auction_0925 is True
    assert result.price == 10.0
    assert result.volume == 123
    assert result.source_mode == "fallback_scan"
    assert result.pages_used == 7
    assert call_order == [4000, 2000, 0, 0, 2000, 4000, 6000]


def test_get_auction_0925_normalizes_result_code_on_hit() -> None:
    client = TdxClient()
    client.connect = Mock()
    connection = Mock()
    client._connections = [connection]
    client._round_robin = cycle(range(1))

    item_0925 = TradeItem(
        time=datetime(2026, 4, 9, 9, 25, tzinfo=SHANGHAI_TZ),
        price=11.17,
        price_milli=11170,
        volume=4233,
        status=2,
        side="neutral",
    )
    connection.request_history_trade_probe.return_value = TradeProbe(
        count=235,
        trading_date=date(2026, 4, 9),
        first_item=item_0925,
        item_0925=item_0925,
    )

    result = client.get_auction_0925("000001", "20260409")

    assert result.code == "sz000001"
    assert result.has_auction_0925 is True
    assert connection.request_history_trade_probe.call_args_list == [call("sz000001", "20260409", 4000, 2000)]


def test_get_auction_0925_normalizes_result_code_on_miss() -> None:
    client = TdxClient()
    client.connect = Mock()
    connection = Mock()
    client._connections = [connection]
    client._round_robin = cycle(range(1))

    connection.request_history_trade_probe.side_effect = [
        TradeProbe(count=0, trading_date=date(2026, 4, 9), first_item=None, item_0925=None),
        TradeProbe(count=0, trading_date=date(2026, 4, 9), first_item=None, item_0925=None),
        TradeProbe(count=1, trading_date=date(2026, 4, 9), first_item=None, item_0925=None),
    ]

    result = client.get_auction_0925("000001", "20260409")

    assert result.code == "sz000001"
    assert result.has_auction_0925 is False
    assert result.source_mode == "fast_no_0925@0"
    assert connection.request_history_trade_probe.call_args_list == [
        call("sz000001", "20260409", 4000, 2000),
        call("sz000001", "20260409", 2000, 2000),
        call("sz000001", "20260409", 0, 2000),
    ]


def test_get_auction_0925_raises_when_probe_exceeds_protocol_page_limit() -> None:
    client = TdxClient()
    client.connect = Mock()
    connection = Mock()
    client._connections = [connection]
    client._round_robin = cycle(range(1))

    connection.request_history_trade_probe.return_value = TradeProbe(
        count=2000,
        trading_date=date(2026, 4, 9),
        first_item=None,
        item_0925=None,
    )

    with pytest.raises(ProtocolError, match="history trade probe exceeded protocol page limit"):
        client.get_auction_0925("sz000001", "20260409")

    assert len(connection.request_history_trade_probe.call_args_list) == 36
    assert connection.request_history_trade_probe.call_args_list[-1] == call("sz000001", "20260409", 64000, 2000)
