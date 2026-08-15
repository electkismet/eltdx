"""Deferred runtime checks for the compact native DTO model boundary."""

from __future__ import annotations

from datetime import date, datetime, timezone, timedelta

import pytest

from eltdx._native_models import response_from_dto
from eltdx.models import (
    HeartbeatAck,
    KlineSeries,
    QuoteSnapshot,
    SecurityCode,
    TradePage,
    TradeTick,
)


def test_heartbeat_dto_rebuilds_date_and_bytes() -> None:
    result = response_from_dto(
        (
            "heartbeat",
            (b"header", 20260815, (2026, 8, 15), b"payload"),
        )
    )
    assert isinstance(result, HeartbeatAck)
    assert result.server_date == date(2026, 8, 15)
    assert result.raw_payload == b"payload"


def test_security_list_preserves_top_level_list() -> None:
    item = (
        "sz",
        0,
        "000001",
        "name",
        100,
        2,
        10.0,
        1.0,
        b"\x00" * 4,
        b"\x00" * 4,
        b"\x00" * 4,
        "a_share",
        "reason",
        "szse_main_board",
        "reason",
    )
    result = response_from_dto(("security_list", [item]))
    assert isinstance(result, list)
    assert isinstance(result[0], SecurityCode)


def test_kline_dto_rebuilds_aware_datetime_and_nested_tuple() -> None:
    bar = (
        (2026, 8, 15, 9, 30, 0, 28800),
        1.0,
        1.1,
        1.2,
        0.9,
        1000,
        1100,
        1200,
        900,
        None,
        10,
        20,
        10.0,
        0.1,
        20.0,
        1,
        2,
        3,
        4,
        None,
        None,
        "",
    )
    payload = (
        "sz",
        0,
        "000001",
        4,
        1,
        "day",
        0,
        1,
        0,
        "none",
        0,
        (2026, 8, 15),
        (bar,),
        b"",
    )
    result = response_from_dto(("klines", payload))
    assert isinstance(result, KlineSeries)
    assert result.anchor_date == date(2026, 8, 15)
    assert result.bars[0].time == datetime(2026, 8, 15, 9, 30, tzinfo=timezone(timedelta(hours=8)))
    assert result.bars[0].time.tzname() == "Asia/Shanghai"


def test_snapshot_dto_rebuilds_nested_quote_levels() -> None:
    fields = [
        "sz",
        0,
        "000001",
        1,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1,
        0,
        1,
        1,
        1.0,
        1,
        1,
        1,
        0,
        0,
        0.0,
        1.0,
        2,
        0,
        1.1,
        3,
        0,
        b"",
    ]
    result = response_from_dto(("snapshots", tuple(fields)))
    assert isinstance(result[0], QuoteSnapshot)
    assert result[0].buy_levels[0].volume == 2


def test_today_tick_dto_reuses_none_datetime_tuple_without_field_drift() -> None:
    tick = (
        0,
        5,
        570,
        "09:30",
        None,
        1.25,
        1250,
        100,
        2,
        0,
        "buy",
        125,
        125,
        0,
        None,
        "50030a14030000",
        "trade",
        None,
        None,
    )
    payload = ("sz", 0, "000001", 5, 1800, tick, None, None, b"payload")
    result = response_from_dto(("today_ticks", payload))
    assert isinstance(result, TradePage)
    assert result.ticks == (TradeTick(*tick),)
    assert result.trading_date is None
    assert result.raw_payload == b"payload"


def test_flat_record_dtos_reject_partial_strides() -> None:
    with pytest.raises(ValueError, match="snapshots length must be a multiple of 27"):
        response_from_dto(("snapshots", ("partial",)))
    payload = ("sz", 0, "000001", 0, 1800, ("partial",), None, None, b"")
    with pytest.raises(ValueError, match="trade ticks length must be a multiple of 19"):
        response_from_dto(("today_ticks", payload))
