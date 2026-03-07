from __future__ import annotations

from datetime import datetime

from eltdx.adjustment import apply_factors_to_kline, apply_xdxr_to_last_close, build_factor_response, normalize_adjust_mode
from eltdx.enums import AdjustMode
from eltdx.models import FactorResponse, KlineItem, KlineResponse, XdxrItem
from eltdx.protocol.unit import SHANGHAI_TZ


def _make_kline(day: int, close_price_milli: int, last_close_price_milli: int | None, month: int = 1) -> KlineItem:
    return KlineItem(
        time=datetime(2026, month, day, 15, 0, tzinfo=SHANGHAI_TZ),
        open_price=close_price_milli / 1000.0,
        open_price_milli=close_price_milli,
        high_price=close_price_milli / 1000.0,
        high_price_milli=close_price_milli,
        low_price=close_price_milli / 1000.0,
        low_price_milli=close_price_milli,
        close_price=close_price_milli / 1000.0,
        close_price_milli=close_price_milli,
        last_close_price=None if last_close_price_milli is None else last_close_price_milli / 1000.0,
        last_close_price_milli=last_close_price_milli,
        volume=100,
        amount=1000.0,
        amount_milli=1_000_000,
    )


def test_apply_xdxr_to_last_close() -> None:
    xdxr = XdxrItem(
        code="sz000001",
        time=datetime(2026, 1, 3, 15, 0, tzinfo=SHANGHAI_TZ),
        fenhong=5.0,
        peigujia=0.0,
        songzhuangu=0.0,
        peigu=0.0,
    )

    assert apply_xdxr_to_last_close(10_000, xdxr) == 9_500


def test_build_factor_response_maps_non_trading_day_event_to_next_kline() -> None:
    day_kline = KlineResponse(
        count=3,
        items=[
            _make_kline(2, 10_000, None),
            _make_kline(5, 11_000, 10_000),
            _make_kline(6, 12_000, 11_000),
        ],
    )
    xdxr = [
        XdxrItem(
            code="sz000001",
            time=datetime(2026, 1, 4, 15, 0, tzinfo=SHANGHAI_TZ),
            fenhong=5.0,
            peigujia=0.0,
            songzhuangu=0.0,
            peigu=0.0,
        )
    ]

    factors = build_factor_response(day_kline, xdxr)

    assert factors.count == 3
    assert factors.items[0].pre_last_close_price_milli is None
    assert factors.items[1].pre_last_close_price_milli == 9_500
    assert factors.items[2].pre_last_close_price_milli == 11_000
    assert round(factors.items[0].qfq_factor, 6) == 0.95
    assert round(factors.items[1].qfq_factor, 6) == 1.0
    assert round(factors.items[2].qfq_factor, 6) == 1.0
    assert round(factors.items[0].hfq_factor, 6) == 1.0
    assert round(factors.items[1].hfq_factor, 6) == round(10_000 / 9_500, 6)
    assert round(factors.items[2].hfq_factor, 6) == round(10_000 / 9_500, 6)


def test_apply_factors_to_kline_preserves_ex_right_last_close_continuity() -> None:
    day_kline = KlineResponse(
        count=3,
        items=[
            _make_kline(2, 10_000, None),
            _make_kline(3, 11_000, 10_000),
            _make_kline(6, 12_000, 11_000),
        ],
    )
    xdxr = [
        XdxrItem(
            code="sz000001",
            time=datetime(2026, 1, 3, 15, 0, tzinfo=SHANGHAI_TZ),
            fenhong=5.0,
            peigujia=0.0,
            songzhuangu=0.0,
            peigu=0.0,
        )
    ]

    factors = build_factor_response(day_kline, xdxr)
    qfq = apply_factors_to_kline(day_kline, factors, "qfq")
    hfq = apply_factors_to_kline(day_kline, factors, AdjustMode.HFQ)

    assert qfq.count == 3
    assert qfq.items[0].close_price_milli == 9_500
    assert qfq.items[1].close_price_milli == 11_000
    assert qfq.items[1].last_close_price_milli == 9_500
    assert qfq.items[2].last_close_price_milli == 11_000

    assert hfq.items[0].close_price_milli == 10_000
    assert hfq.items[1].close_price_milli == 11_579
    assert hfq.items[1].last_close_price_milli == 10_000
    assert hfq.items[2].close_price_milli == 12_632


def test_normalize_adjust_mode() -> None:
    assert normalize_adjust_mode("qfq") is AdjustMode.QFQ
    assert normalize_adjust_mode("HFQ") is AdjustMode.HFQ
