from __future__ import annotations

from datetime import datetime
from itertools import cycle
from unittest.mock import Mock, call

import pytest

from eltdx import KlineItem, KlineResponse, TdxClient
from eltdx.exceptions import ProtocolError
from eltdx.protocol.frame import ResponseFrame
from eltdx.protocol.model_kline import parse_kline_payload
from eltdx.protocol.unit import SHANGHAI_TZ, fix_kline_times


DAY_PAYLOAD_HEX = "0a0078da340198b8018404bc055ee8b3e949ad2b094f79da34010af801a002cc0260dec949859ded4e7ada34016882028e04e603b8f91e4a111f394f7dda3401e401c20200f604f84d2b4ad4d0444f7eda3401721eaa0268d87bc549ee80e34e7fda34011e288601c601d08db849230ed54e80da3401727c32da013023584999a0784e81da3401147c0ad001d0fa86498d989a4e84da34015e6800d60278c28e491ca6a14e85da340154d001b801da01403e924989d6a54e"
INDEX_DAY_PAYLOAD_HEX = "0300d02535018efdf203e850aa9f02dcf703de82e94a6e94815364029606d1253501aeaa03fa12a0f701d8ab02727ed24a5eac7853be063202d2253501cee20292d604a8a805009260c54a61e563531007ec01"


def _make_response(payload_hex: str) -> ResponseFrame:
    payload = bytes.fromhex(payload_hex)
    return ResponseFrame(
        control=0x1C,
        msg_id=8,
        msg_type=0x052D,
        zip_length=len(payload),
        length=len(payload),
        data=payload,
        raw=b"\xB1\xCB\x74\x00",
    )


def _make_kline_item(day: int, close_price_milli: int, last_close_price_milli: int | None = None, hour: int = 15, minute: int = 0) -> KlineItem:
    return KlineItem(
        time=datetime(2026, 3, day, hour, minute, tzinfo=SHANGHAI_TZ),
        open_price=close_price_milli / 1000,
        open_price_milli=close_price_milli,
        high_price=close_price_milli / 1000,
        high_price_milli=close_price_milli,
        low_price=close_price_milli / 1000,
        low_price_milli=close_price_milli,
        close_price=close_price_milli / 1000,
        close_price_milli=close_price_milli,
        last_close_price=None if last_close_price_milli is None else last_close_price_milli / 1000,
        last_close_price_milli=last_close_price_milli,
        volume=100,
        amount=1000.0,
        amount_milli=1_000_000,
    )


def test_parse_day_kline_payload() -> None:
    parsed = parse_kline_payload("day", "sz000001", _make_response(DAY_PAYLOAD_HEX), kind="stock", include_raw=True)

    assert parsed.count == 10
    assert parsed.items[0].time.isoformat() == "2024-10-16T15:00:00+08:00"
    assert parsed.items[0].open_price_milli == 11800
    assert parsed.items[0].high_price_milli == 12180
    assert parsed.items[0].low_price_milli == 11770
    assert parsed.items[0].close_price_milli == 12060
    assert parsed.items[0].last_close_price is None
    assert parsed.items[-1].time.isoformat() == "2024-10-29T15:00:00+08:00"
    assert parsed.items[-1].last_close_price_milli == 11640
    assert parsed.items[-1].close_price_milli == 11540
    assert parsed.raw_payload_hex == DAY_PAYLOAD_HEX


def test_parse_index_day_kline_payload() -> None:
    parsed = parse_kline_payload("day", "sh000001", _make_response(INDEX_DAY_PAYLOAD_HEX), kind="index", include_raw=True)

    assert parsed.count == 3
    assert parsed.items[0].time.isoformat() == "2026-03-04T15:00:00+08:00"
    assert parsed.items[0].open_price_milli == 4087630
    assert parsed.items[0].volume == 765169500
    assert parsed.items[0].up_count == 612
    assert parsed.items[0].down_count == 1686
    assert parsed.items[-1].time.isoformat() == "2026-03-06T15:00:00+08:00"
    assert parsed.items[-1].last_close_price_milli == 4108570
    assert parsed.items[-1].close_price_milli == 4124190
    assert parsed.items[-1].up_count == 1808
    assert parsed.items[-1].down_count == 492
    assert parsed.raw_payload_hex == INDEX_DAY_PAYLOAD_HEX


def test_parse_index_day_kline_payload_rejects_stock_kind() -> None:
    with pytest.raises(ProtocolError, match="invalid kline time field|unexpected trailing kline payload bytes"):
        parse_kline_payload("day", "sh000001", _make_response(INDEX_DAY_PAYLOAD_HEX), kind="stock")


def test_parse_stock_day_kline_payload_rejects_index_kind() -> None:
    with pytest.raises(ProtocolError):
        parse_kline_payload("day", "sz000001", _make_response(DAY_PAYLOAD_HEX), kind="index")


def test_collect_kline_pages_orders_pages_chronologically() -> None:
    client = TdxClient()
    pages = [
        KlineResponse(
            count=2,
            items=[
                _make_kline_item(5, 10500),
                _make_kline_item(6, 10600, 10500),
            ],
        ),
        KlineResponse(
            count=1,
            items=[_make_kline_item(4, 10400)],
        ),
    ]

    def fetch_page(start: int, count: int) -> KlineResponse:
        assert count == 2
        return pages[start // 2]

    parsed = client._collect_kline_pages(fetch_page, 2)

    assert parsed.count == 3
    assert [item.time.day for item in parsed.items] == [4, 5, 6]
    assert parsed.items[1].last_close_price_milli == 10400
    assert parsed.items[2].last_close_price_milli == 10500


def test_fix_kline_times_repairs_midday_boundary() -> None:
    items = [
        _make_kline_item(6, 10100, hour=13, minute=0),
        _make_kline_item(6, 10200, 10100, hour=13, minute=1),
    ]

    fixed = fix_kline_times(items, now=datetime(2026, 3, 6, 14, 0, tzinfo=SHANGHAI_TZ))

    assert fixed[0].time.isoformat() == "2026-03-06T11:30:00+08:00"
    assert fixed[1].time.isoformat() == "2026-03-06T13:01:00+08:00"


def test_get_kline_accepts_code_then_freq_order() -> None:
    client = TdxClient()
    client.connect = Mock()
    connection = Mock()
    connection.request_kline.return_value = "kline"
    client._connections = [connection]
    client._round_robin = cycle(range(1))

    parsed = client.get_kline("sz000001", "day", start=5, count=999)

    assert parsed == "kline"
    assert connection.request_kline.call_args_list == [
        call("day", "sz000001", 5, 800, kind="stock", include_raw=False)
    ]


def test_get_kline_passes_explicit_kind_to_transport() -> None:
    client = TdxClient()
    client.connect = Mock()
    connection = Mock()
    connection.request_kline.return_value = "index-kline"
    client._connections = [connection]
    client._round_robin = cycle(range(1))

    parsed = client.get_kline("sh000001", "day", count=5, kind="index", include_raw=True)

    assert parsed == "index-kline"
    assert connection.request_kline.call_args_list == [
        call("day", "sh000001", 0, 5, kind="index", include_raw=True)
    ]


def test_get_kline_all_passes_explicit_kind_to_transport() -> None:
    client = TdxClient()
    client.connect = Mock()
    connection = Mock()
    connection.request_kline.return_value = KlineResponse(count=1, items=[_make_kline_item(6, 10200)])
    client._connections = [connection]
    client._round_robin = cycle(range(1))

    parsed = client.get_kline_all("sh000001", "day", kind="index")

    assert parsed.count == 1
    assert connection.request_kline.call_args_list == [call("day", "sh000001", 0, 800, kind="index")]
