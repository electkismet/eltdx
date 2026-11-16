"""Profile/helper cache compatibility; authored for final unified verification."""
from __future__ import annotations

from types import SimpleNamespace
from datetime import date

import pytest

from eltdx.helpers import core

CODE = "sh" + str(600_001)
OTHER = "sh" + str(600_002)


class Client(SimpleNamespace):
    pass


def test_profile_retains_input_occurrences_last_source_and_original_objects():
    quote1 = SimpleNamespace(full_code=CODE, last_price=10.0, total_hand=100, market_id=None)
    quote2 = SimpleNamespace(full_code=CODE, last_price=12.0, total_hand=200, market_id=None)
    security = SimpleNamespace(full_code=CODE, name="name", market_id=1)
    finance = SimpleNamespace(full_code=CODE, circulating_shares=100_000.0, total_shares=200_000.0)
    client = Client()
    helper = core.HelperApi(client)
    helper.full_quotes = lambda codes: [quote1, quote2]
    helper._security_map = lambda codes: {CODE: security}
    helper._finance_map = lambda codes: {CODE: finance}
    result = helper.stock_profile_table([CODE, OTHER, CODE])
    assert result.codes == (CODE, OTHER, CODE)
    assert [row.full_code for row in result.rows] == list(result.codes)
    first, absent, last = result.rows
    assert first.quote is last.quote is quote2
    assert first.security is security and first.finance is finance
    assert first.market_id == 1 and first.turnover_rate == 20.0
    assert first.circulating_market_value == 1_200_000.0
    assert first.total_market_value == 2_400_000.0
    assert absent.quote is None and absent.turnover_rate is None


def test_profile_flags_skip_sources_and_builder_hook_remains_callable(monkeypatch):
    client = Client()
    helper = core.HelperApi(client)
    helper.full_quotes = lambda codes: []
    helper._security_map = lambda codes: pytest.fail("security disabled")
    helper._finance_map = lambda codes: pytest.fail("finance disabled")
    calls = []
    monkeypatch.setattr(core, "_build_stock_profile", lambda *values: calls.append(values) or values)
    result = helper.stock_profile_table([CODE], include_security=False, include_finance=False)
    assert result.rows == ((CODE, None, None, None),) and len(calls) == 1


def test_profile_math_keeps_zero_missing_and_nonfinite_values():
    assert core._turnover_rate(None, 1) is None
    assert core._turnover_rate(1, 0.0) is None
    assert core._turnover_rate(-2, -10.0) == 2000.0
    assert core._market_value(None, 0.0) is None
    assert core._market_value(-2.0, 3.0) == -6.0
    assert core._market_value(float("inf"), 2.0) == float("inf")


def test_finance_helper_cache_keeps_failed_chunk_and_ordered_keys():
    calls = []
    row = SimpleNamespace(full_code=CODE)

    def fetch(codes):
        calls.append(tuple(codes))
        if len(calls) == 1:
            raise RuntimeError("optional source failed")
        return SimpleNamespace(records=(row,))

    client = Client(corporate=SimpleNamespace(finance_batch=fetch))
    helper = core.HelperApi(client)
    codes = [CODE] * 81
    assert helper._finance_map(codes) == {CODE: row}
    assert [len(call) for call in calls] == [80, 1]
    assert helper._finance_map(codes) == {CODE: row} and len(calls) == 2
    helper._finance_map([OTHER, CODE])
    helper._finance_map([CODE, OTHER])
    assert calls[-2:] == [(OTHER, CODE), (CODE, OTHER)]


def test_finance_helper_caches_empty_but_keeps_iteration_error_visible():
    calls = []
    client = Client(corporate=SimpleNamespace(finance_batch=lambda codes: calls.append(codes) or object()))
    helper = core.HelperApi(client)
    assert helper._finance_map([CODE]) == {}
    assert helper._finance_map([CODE]) == {} and len(calls) == 1
    helper.clear_cache()
    assert helper._finance_map([CODE]) == {} and len(calls) == 2
    helper.clear_cache()
    client.corporate.finance_batch = lambda codes: SimpleNamespace(records=1)
    with pytest.raises(TypeError):
        helper._finance_map([CODE])
    assert (CODE,) not in helper._finance_cache


def test_security_helper_preserves_eager_fallback_and_market_cache():
    reads = []

    class Row:
        full_code = CODE

        @property
        def exchange(self):
            reads.append("exchange")
            return "sh"

        @property
        def code(self):
            reads.append("code")
            return "600001"

    row = Row()
    calls = []
    client = Client(codes=SimpleNamespace(all=lambda market: calls.append(market) or [row]))
    helper = core.HelperApi(client)
    assert helper._security_map([CODE]) == {CODE: row}
    assert helper._security_map([CODE]) == {CODE: row}
    assert calls == ["sh"] and reads == ["exchange", "code", "exchange", "code"]


def test_profile_scalar_objects_use_original_python_arithmetic_order():
    reads = []

    class Scalar:
        def __mul__(self, other):
            reads.append(("mul", other))
            return self

        def __truediv__(self, other):
            reads.append(("div", other))
            return self

    value = Scalar()
    assert core._turnover_rate(value, 2.0) is value
    assert reads == [("mul", 100.0), ("div", 2.0), ("mul", 100.0)]


def test_by_code_supports_mapping_and_ignores_none_identity():
    row1 = SimpleNamespace(full_code=CODE)
    row2 = SimpleNamespace(full_code=CODE)
    assert core._by_full_code({"a": row1, "b": object(), "c": row2}) == {CODE: row2}
    assert core._by_full_code(None) == {}


def test_daily_shares_keeps_stats_date_units_sources_and_duplicates(monkeypatch):
    day = date(2024, 6, 3)
    finance = SimpleNamespace(total_shares=100.0, circulating_shares=50.0, updated_date="updated")
    stats = SimpleNamespace(row=lambda market, code: (SimpleNamespace(free_float_shares_10k=1.23456789), None))
    client = Client()
    helper = core.HelperApi(client)
    monkeypatch.setattr(core, "_resolve_market_date_context", lambda client:
                        SimpleNamespace(target_trade_date=day, previous_trade_date=date(2024, 5, 31)))
    monkeypatch.setattr(core, "_validate_stats_resource_dates", lambda *args, **kwargs: date(2024, 5, 31))
    helper._shortline._stats_resource = lambda *args, **kwargs: (stats, True)
    helper._finance_map = lambda codes: {CODE: finance}
    result = helper.daily_share_capital([CODE, OTHER, CODE])
    assert result.codes == (CODE, OTHER, CODE)
    assert [row.trade_date for row in result.rows] == [date(2024, 5, 31)] * 3
    assert result.rows[0].free_float_shares == 12345.6789
    assert result.rows[0].share_source == "finance_snapshot+tdx_stats"
    assert result.rows[1].share_source == "tdx_stats"
    assert result.rows[0].finance_updated_date == "updated"


def test_daily_shares_explicit_empty_still_resolves_stats_and_all_selection(monkeypatch):
    day = date(2024, 6, 3)
    calls = []
    client = Client(codes=SimpleNamespace(all_a_shares=lambda: calls.append("codes") or [CODE]))
    helper = core.HelperApi(client)
    monkeypatch.setattr(core, "_resolve_market_date_context", lambda client:
                        calls.append("context") or SimpleNamespace(target_trade_date=day, previous_trade_date=day))
    monkeypatch.setattr(core, "_validate_stats_resource_dates", lambda *args, **kwargs:
                        calls.append("validate") or day)
    stats = SimpleNamespace(row=lambda *args: (None, None))
    helper._shortline._stats_resource = lambda *args, **kwargs: (calls.append("stats") or stats, False)
    helper._finance_map = lambda codes: calls.append(("finance", tuple(codes))) or {}
    assert helper.daily_share_capital([]).rows == ()
    assert calls == ["context", "stats", "validate", ("finance", ())]
    calls.clear()
    result = helper.daily_share_capital()
    assert result.codes == (CODE,) and result.rows[0].share_source == "missing"
    assert calls[0] == "codes"


def test_daily_shares_preserves_numeric_hook_and_second_property_read(monkeypatch):
    day = date(2024, 6, 3)
    reads = []

    class Stat:
        @property
        def free_float_shares_10k(self):
            reads.append("shares")
            return 2.0

    client = Client()
    helper = core.HelperApi(client)
    monkeypatch.setattr(core, "_resolve_market_date_context", lambda client:
                        SimpleNamespace(target_trade_date=day, previous_trade_date=day))
    monkeypatch.setattr(core, "_validate_stats_resource_dates", lambda *args, **kwargs: day)
    helper._shortline._stats_resource = lambda *args, **kwargs: (SimpleNamespace(row=lambda *args: (Stat(), None)), False)
    helper._finance_map = lambda codes: {}
    monkeypatch.setattr(core, "_float", lambda value: "hook")
    result = helper.daily_share_capital([CODE])
    assert reads == ["shares", "shares"]
    assert result.rows[0].total_shares == "hook" and result.rows[0].free_float_shares == 20000.0
