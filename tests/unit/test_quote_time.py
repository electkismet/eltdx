from __future__ import annotations

from datetime import datetime

from eltdx.protocol.unit import SHANGHAI_TZ, decode_quote_datetime


def test_decode_quote_datetime_centiseconds() -> None:
    parsed = decode_quote_datetime(15330719, now=datetime(2026, 3, 6, 9, 30, 0, tzinfo=SHANGHAI_TZ))

    assert parsed is not None
    assert parsed.tzinfo == SHANGHAI_TZ
    assert parsed.year == 2026
    assert parsed.month == 3
    assert parsed.day == 6
    assert parsed.hour == 15
    assert parsed.minute == 33
    assert parsed.second == 7
    assert parsed.microsecond == 190000


def test_decode_quote_datetime_milliseconds() -> None:
    parsed = decode_quote_datetime(153315924, now=datetime(2026, 3, 6, 9, 30, 0, tzinfo=SHANGHAI_TZ))

    assert parsed is not None
    assert parsed.hour == 15
    assert parsed.minute == 33
    assert parsed.second == 15
    assert parsed.microsecond == 924000


def test_decode_quote_datetime_invalid_returns_none() -> None:
    parsed = decode_quote_datetime(14999512, now=datetime(2026, 3, 6, 9, 30, 0, tzinfo=SHANGHAI_TZ))

    assert parsed is None
