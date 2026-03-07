from __future__ import annotations

from datetime import datetime

import pytest

from eltdx.models import Quote
from eltdx.protocol.frame import ResponseFrame
from eltdx.protocol.model_quote import parse_quote_payload
from eltdx.protocol.unit import SHANGHAI_TZ


def test_quote_uses_last_price_and_last_close_price_fields() -> None:
    quote = Quote(
        exchange="sz",
        code="000001",
        active1=3860,
        active2=3860,
        server_time_raw=15330719,
        server_time=datetime(2026, 3, 7, 15, 33, 7, 190000, tzinfo=SHANGHAI_TZ),
        last_price=10.82,
        last_price_milli=10820,
        open_price=10.78,
        open_price_milli=10780,
        high_price=10.84,
        high_price_milli=10840,
        low_price=10.77,
        low_price_milli=10770,
        last_close_price=10.81,
        last_close_price_milli=10810,
        total_hand=476576,
        current_hand=6768,
        amount=514733536.0,
        inside_dish=206012,
        outer_disc=270565,
        buy_levels=[],
        sell_levels=[],
        rate=0.0,
        call_auction_amount=123400.0,
        call_auction_rate=-0.2775208140610546,
    )

    assert quote.last_price == 10.82
    assert quote.last_price_milli == 10820
    assert quote.last_close_price == 10.81
    assert quote.last_close_price_milli == 10810
    assert quote.current_hand == 6768
    assert quote.call_auction_amount == 123400.0
    assert quote.call_auction_rate == pytest.approx(-0.2775208140610546)
    assert not hasattr(quote, "close_price")
    assert not hasattr(quote, "intuition")
    assert not hasattr(quote, "latest_price")


def test_parse_quote_payload_maps_current_and_last_close_correctly(monkeypatch: pytest.MonkeyPatch) -> None:
    payload = bytearray(80)
    payload[2:4] = (1).to_bytes(2, "little", signed=False)
    payload[4] = 0
    payload[5:11] = b"000001"
    payload[11:13] = (3860).to_bytes(2, "little", signed=False)
    payload[52:54] = (0).to_bytes(2, "little", signed=False)
    payload[54:56] = (3860).to_bytes(2, "little", signed=False)

    response = ResponseFrame(
        control=0,
        msg_id=1,
        msg_type=0,
        zip_length=len(payload),
        length=len(payload),
        data=bytes(payload),
        raw=b"",
    )

    monkeypatch.setattr(
        "eltdx.protocol.model_quote.decode_k",
        lambda payload, offset: (
            {
                "current": 10820,
                "last_close": 10810,
                "open": 10780,
                "high": 10840,
                "low": 10770,
            },
            offset + 1,
        ),
    )

    varints = iter(
        [
            15330719,
            0,
            476576,
            6768,
            206012,
            270565,
            7,
            1234,
            6320,
            6878,
            6980,
            8123,
            11800,
            12083,
            5530,
            6855,
            8690,
            5569,
            0,
            0,
            0,
            0,
        ]
    )
    monkeypatch.setattr("eltdx.protocol.model_quote.consume_varint", lambda payload, offset: (next(varints), offset + 1))

    prices = iter([0, 1, -1, 2, -2, 3, -3, 4, -4, 5])
    monkeypatch.setattr("eltdx.protocol.model_quote.consume_price", lambda payload, offset: (next(prices), offset + 1))

    monkeypatch.setattr("eltdx.protocol.model_quote.get_volume", lambda value: 514733536.0)
    monkeypatch.setattr(
        "eltdx.protocol.model_quote.decode_quote_datetime",
        lambda raw: datetime(2026, 3, 7, 15, 33, 7, 190000, tzinfo=SHANGHAI_TZ),
    )

    quote = parse_quote_payload(["sz000001"], response)[0]

    assert quote.last_price == 10.82
    assert quote.last_close_price == 10.81
    assert quote.current_hand == 6768
    assert quote.call_auction_amount == 123400.0
    assert quote.call_auction_rate == pytest.approx(-0.2775208140610546)
    assert not hasattr(quote, "close_price")
    assert not hasattr(quote, "intuition")
    assert quote.buy_levels[0].price == 10.82
    assert quote.sell_levels[0].price == 10.83
