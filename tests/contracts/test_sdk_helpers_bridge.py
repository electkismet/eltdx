"""Helper bridge sources; execution is deferred until all T01--T30 code exists."""
from __future__ import annotations

from dataclasses import replace
import math
from types import SimpleNamespace

import pytest

from eltdx.helpers.core import HelperApi
from eltdx.helpers.market import FundFlow, classify_fund_flow, fetch_today_fund_flow
from eltdx.models import QuoteLevel, QuoteRefreshRecord, QuoteSnapshot, TradeTick

CODE = "sh" + str(600_001)


class _Client(SimpleNamespace):
    """Weak-referenceable owner for the public helper services."""


def _tick(amount, side="buy", kind="trade"):
    return TradeTick(0, 0, 910, "15:10", None, amount / 100.0, 0, 1, 1,
                     5, side, 0, 0, event_kind=kind)


def _snapshot():
    return QuoteSnapshot("sh", 1, "600001", 0, 10.0, 9.0, 9.5, 11.0,
                         8.0, 0, 0, 15, 2, 15000.0, 123, 4, 5, 0,
                         10, 100.0, (), (), b"primary")


def _depth(**changes):
    levels = tuple(QuoteLevel(8.0, 3, 0) for _ in range(5))
    value = QuoteRefreshRecord(
        exchange="sh", market_id=1, code="600001", active=1, update_time_raw=1,
        last_price=20.0, last_close_price=19.0, open_price=19.5, high_price=21.0,
        low_price=18.0, status_or_reserved_raw=0, total_hand=30, current_hand=4,
        amount=30000.0, amount_raw=456, inside_dish=8, outer_disc=10,
        unknown_after_outer_raw=0, open_amount_raw=99, open_amount_yuan=990.0,
        buy_levels=levels, sell_levels=levels, tail_raw=b"depth",
    )
    return replace(value, **changes)


def test_fund_flow_exact_thresholds_and_trade_kinds():
    result = classify_fund_flow([
        _tick(40_000.0), _tick(200_000.0), _tick(1_000_000.0),
        _tick(1_000_001.0, kind="opening_match"),
        _tick(9_000_000.0, kind="auction_snapshot"),
        _tick(11.0, side="neutral"), _tick(40_001.0, side="sell"),
    ])
    assert result == FundFlow(1_000_001.0, 1_000_000.0, 200_000.0,
                              40_000.0, 0.0, 0.0, 40_001.0, 0.0)


def test_fund_flow_keeps_python_negative_and_nonfinite_arithmetic():
    negative = classify_fund_flow([_tick(-2.0)])
    assert negative.small_in == -2.0
    result = classify_fund_flow([_tick(float("nan")), _tick(float("inf"), side="sell")])
    assert math.isnan(result.small_in)
    assert result.super_out == float("inf")


def test_fund_flow_skips_fields_of_auction_and_neutral_custom_records():
    auction = SimpleNamespace(is_actual_trade=False)
    neutral = SimpleNamespace(is_actual_trade=True, side="neutral")
    assert classify_fund_flow([auction, neutral]) == FundFlow(*([0.0] * 8))


def test_fund_flow_custom_properties_keep_original_read_order():
    seen = []

    class Tick:
        def __getattribute__(self, name):
            seen.append(name)
            return {"is_actual_trade": True, "side": "sell", "price": 2.0, "volume": 3}[name]

    result = classify_fund_flow([Tick()])
    assert result.small_out == 600.0
    assert seen == ["is_actual_trade", "side", "side", "price", "volume"]


def test_fund_flow_empty_and_today_wrapper_keep_original_results():
    client = SimpleNamespace(trades=SimpleNamespace(all_today=lambda code: SimpleNamespace(ticks=None)))
    assert fetch_today_fund_flow(client, CODE) == FundFlow(*([0.0] * 8))
    error = ValueError("page failed")

    def fail(code):
        raise error

    client.trades.all_today = fail
    with pytest.raises(ValueError) as caught:
        fetch_today_fund_flow(client, CODE)
    assert caught.value is error


def test_full_quotes_chunks_duplicate_occurrences_and_calls_custom_merger():
    calls = []
    client = _Client(quotes=SimpleNamespace(
        get_snapshots=lambda codes: calls.append(tuple(codes)) or list(codes)))
    helper = HelperApi(client)
    helper._merge_quote_depths = lambda rows, codes: list(reversed(rows))
    codes = [CODE] * 81
    assert helper.full_quotes(codes) == codes
    assert [len(call) for call in calls] == [80, 1]


def test_full_quotes_custom_scalar_short_circuits_remaining_batches():
    scalar = object()
    calls = []
    client = _Client(quotes=SimpleNamespace(
        get_snapshots=lambda codes: calls.append(codes) or scalar))
    helper = HelperApi(client)
    assert helper.full_quotes([CODE] * 161) is scalar
    assert len(calls) == 1
    assert helper.full_quotes([]) == []


def test_depth_merge_last_duplicate_wins_and_only_four_fields_change():
    source = _snapshot()
    depth = _depth()
    later = replace(depth, open_amount_raw=1234, open_amount_yuan=4567.0)
    client = _Client(quotes=SimpleNamespace(
        get_depth=lambda codes: SimpleNamespace(records=(depth, later))))
    helper = HelperApi(client)
    result = helper._merge_quote_depths([source, source], [CODE, CODE])
    expected = replace(source, buy_levels=later.buy_levels, sell_levels=later.sell_levels,
                       open_amount_raw=later.open_amount_raw, open_amount_yuan=later.open_amount_yuan)
    assert result == [expected, expected]
    assert result[0] is not source


def test_depth_merge_incomplete_depth_keeps_original_container():
    source = [_snapshot()]
    depth = _depth(buy_levels=())
    client = _Client(quotes=SimpleNamespace(
        get_depth=lambda codes: SimpleNamespace(records=(depth,))))
    helper = HelperApi(client)
    assert helper._merge_quote_depths(source, [CODE]) is source


def test_depth_request_exception_degrades_but_base_exception_propagates():
    source = [_snapshot()]

    def fail(codes):
        raise RuntimeError("depth unavailable")

    client = _Client(quotes=SimpleNamespace(get_depth=fail))
    helper = HelperApi(client)
    assert helper._merge_quote_depths(source, [CODE]) is source

    def interrupt(codes):
        raise KeyboardInterrupt()

    client.quotes.get_depth = interrupt
    with pytest.raises(KeyboardInterrupt):
        helper._merge_quote_depths(source, [CODE])
