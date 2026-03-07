from __future__ import annotations

from datetime import datetime
from pathlib import Path

from eltdx.equity import compute_turnover, filter_equity_items, pick_equity
from eltdx.models import EquityItem
from eltdx.protocol.frame import ResponseFrame
from eltdx.protocol.model_gbbq import parse_gbbq_payload
from eltdx.protocol.unit import SHANGHAI_TZ


FIXTURE = Path(__file__).resolve().parents[1] / "fixtures" / "gbbq_sz000001.hex"


def _load_equity_response():
    payload_hex = FIXTURE.read_text(encoding="utf-8").strip()
    payload = bytes.fromhex(payload_hex)
    response = ResponseFrame(
        control=0x1C,
        msg_id=9,
        msg_type=0x000F,
        zip_length=len(payload),
        length=len(payload),
        data=payload,
        raw=b"\xB1\xCB\x74\x00",
    )
    parsed = parse_gbbq_payload(response)
    return filter_equity_items(parsed.items)


def test_filter_equity_items() -> None:
    equities = _load_equity_response()

    assert equities.count == 47
    assert equities.items[0].time.isoformat() == "1991-04-03T15:00:00+08:00"
    assert equities.items[0].category == 5
    assert equities.items[0].category_name == "股本变化"
    assert equities.items[0].float_shares == 26_500_000
    assert equities.items[0].total_shares == 48_500_170
    assert equities.items[-1].time.isoformat() == "2025-06-30T15:00:00+08:00"
    assert equities.items[-1].float_shares == 19_405_601_250
    assert equities.items[-1].total_shares == 19_405_918_750


def test_pick_equity_effective_same_day_and_forward() -> None:
    equities = _load_equity_response()

    same_day = pick_equity(equities.items, "2024-12-31")
    later_day = pick_equity(equities.items, "2025-03-01")

    assert same_day is not None
    assert same_day.time.isoformat() == "2024-12-31T15:00:00+08:00"
    assert later_day is not None
    assert later_day.time.isoformat() == "2024-12-31T15:00:00+08:00"


def test_compute_turnover_share_and_hand() -> None:
    equity = EquityItem(
        code="sz000001",
        time=datetime(2026, 1, 1, 15, 0, tzinfo=SHANGHAI_TZ),
        category=5,
        category_name="股本变化",
        float_shares=26_500_000,
        total_shares=48_500_170,
    )

    assert round(compute_turnover(equity, 265_000, unit="share"), 6) == 1.0
    assert round(compute_turnover(equity, 2_650, unit="hand"), 6) == 1.0
