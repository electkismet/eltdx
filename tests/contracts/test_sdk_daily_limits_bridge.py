"""Daily-limit compatibility cases authored for the final unified test pass."""
from datetime import date, datetime
from types import SimpleNamespace

import pytest

from eltdx.helpers import core, shortline

CODE = "sh" + str(600_001)
DAY = date(2024, 6, 3)
PRIOR = date(2024, 5, 31)


class Client(SimpleNamespace):
    pass


def test_daily_limit_policy_keeps_legacy_difference_from_shortline():
    assert core._price_limit_ratio(CODE, "ST example") == 10.0
    assert shortline._price_limit_ratio(CODE, "ST example") == 5.0
    assert core._price_limit_rule(CODE, "ST example") == "st_main_10pct"
    assert core._price_limit_ratio("bj430001", "ST example") == 30.0
    assert core._price_limit_rule("sh688001", "ST example") == "star_20pct"
    assert core._price_limit_rule("sz300001", None) == "chinext_20pct"
    assert core._price_limit_ratio(CODE, "N example") == 10.0


def test_daily_limit_row_applies_events_in_original_order_and_rounds_prices():
    first = SimpleNamespace(c1_value=10, c2_value=0, c3_value=10, c4_value=0)
    second = SimpleNamespace(c1_value=5, c2_value=0, c3_value=0, c4_value=0)
    result = core._build_daily_price_limit(CODE, DAY, PRIOR, SimpleNamespace(name="normal"),
                                          10.0, (first, second))
    assert result.pre_close == 4.0
    assert result.limit_up_price == 4.4 and result.limit_down_price == 3.6
    assert result.pre_close_source == "kline_unadjusted+capital_changes"
    assert result.pre_close_trade_date == PRIOR and result.limit_status == "normal"


def test_daily_limit_row_preserves_missing_and_new_stock_states():
    missing = core._build_daily_price_limit(CODE, DAY, PRIOR, None)
    assert missing.limit_status == "missing_pre_close"
    assert missing.pre_close_source is None and missing.limit_up_price is None
    new = core._build_daily_price_limit(CODE, DAY, PRIOR, None, 10.0, (), True)
    assert new.limit_status == "no_price_limit" and new.limit_ratio_pct is None
    assert new.limit_rule == "ipo_first_5_days" and new.pre_close == 10.0


def test_daily_limit_events_downgrade_only_original_error_classes():
    bad = SimpleNamespace(c1_value=0, c2_value=0, c3_value=-10, c4_value=0)
    result = core._build_daily_price_limit(CODE, DAY, PRIOR, None, 10.0, (bad,))
    assert result.pre_close is None and result.pre_close_source is None
    assert result.limit_status == "missing_pre_close"

    class Failure:
        @property
        def c3_value(self):
            raise RuntimeError("event failure")

    with pytest.raises(RuntimeError, match="event failure"):
        core._build_daily_price_limit(CODE, DAY, PRIOR, None, 10.0, (Failure(),))


def test_daily_limit_events_read_rights_twice_in_original_order():
    seen = []

    class Event:
        def __getattr__(self, name):
            seen.append(name)
            return 0.0

    core._build_daily_price_limit(CODE, DAY, PRIOR, None, 10.0, (Event(),))
    assert seen == ["c3_value", "c4_value", "c1_value", "c4_value", "c2_value"]


def test_daily_limit_nan_reference_keeps_legacy_numeric_result():
    import math

    result = core._build_daily_price_limit(CODE, DAY, PRIOR, None, float("nan"))
    assert math.isnan(result.limit_up_price) and math.isnan(result.limit_down_price)
    assert result.limit_status == "normal"


def test_daily_new_stock_uses_inclusive_calendar_length_and_error_policy():
    days = Client(normalize=lambda value: value, range=lambda start, end: [start] * 5)
    assert core._new_stock_phase(PRIOR, DAY, days)
    days.range = lambda start, end: [start] * 6
    assert not core._new_stock_phase(PRIOR, DAY, days)
    assert not core._new_stock_phase(None, DAY, days)
    assert not core._new_stock_phase(DAY, PRIOR, days)

    def invalid(value):
        raise ValueError("bad date")

    days.normalize = invalid
    assert not core._new_stock_phase(PRIOR, DAY, days)

    def failure(value):
        raise RuntimeError("calendar failure")

    days.normalize = failure
    with pytest.raises(RuntimeError, match="calendar failure"):
        core._new_stock_phase(PRIOR, DAY, days)


def test_daily_limits_orchestration_preserves_source_order_and_input_occurrences(monkeypatch):
    calls = []
    workdays = Client(normalize=lambda day: calls.append("normalize") or DAY,
                     is_workday=lambda day: calls.append("workday") or True,
                     previous_workday=lambda day: calls.append("previous") or PRIOR)
    client = Client(workdays=workdays)
    helper = core.HelperApi(client)
    helper._security_map = lambda codes: calls.append("security") or {}
    helper._kline_previous_close_map = lambda codes, day: calls.append("bars") or {CODE: (None, 10.0)}
    helper._finance_map = lambda codes: calls.append("finance") or {}
    helper._today_capital_change_map = lambda codes, day: calls.append("events") or {}
    monkeypatch.setattr(core, "_new_stock_phase", lambda *args: calls.append("phase") or False)
    result = helper.daily_price_limits([CODE], trade_date="custom")
    assert calls == ["normalize", "workday", "security", "previous", "bars", "finance", "events", "phase"]
    assert result.rows[0].pre_close_trade_date == PRIOR
    assert result.codes == (CODE,)
    # Direct native entry accepts normalized occurrences without deduplicating.
    from eltdx.transport.native import native_module
    rows = native_module().helper_daily_limits(helper, [CODE, CODE], DAY, vars(core))
    assert len(rows) == 2 and rows[0] == rows[1]


def test_daily_limits_invalid_date_fails_before_source_queries():
    client = Client(workdays=Client(previous_workday=lambda day: PRIOR))
    helper = core.HelperApi(client)
    helper._security_map = lambda codes: pytest.fail("source called for invalid date")
    with pytest.raises(ValueError, match="invalid trade_date"):
        helper.daily_price_limits([CODE], trade_date="invalid")
    client.workdays.is_workday = lambda day: False
    with pytest.raises(ValueError, match="not a trading day: 2024-06-03"):
        helper.daily_price_limits([CODE], trade_date=DAY)


def test_daily_prior_close_uses_last_qualifying_row_not_latest_date():
    calls = []
    rows = [SimpleNamespace(time=datetime(2024, 5, 31), close=10),
            SimpleNamespace(time=datetime(2024, 6, 3), close=20),
            SimpleNamespace(time=datetime(2024, 5, 30), close=9)]

    def getter(code, **kwargs):
        calls.append((code, kwargs))
        return SimpleNamespace(bars=rows)

    client = Client(bars=Client(get=getter))
    helper = core.HelperApi(client)
    assert helper._kline_previous_close_map([CODE], DAY) == {CODE: (date(2024, 5, 30), 9.0)}
    assert calls == [(CODE, {"period": "day", "start": 0, "count": 800, "adjust": "none"})]


def test_daily_prior_close_preserves_optional_failure_and_base_exception():
    from eltdx.transport.native import native_module

    def failure(code, **kwargs):
        raise ValueError("optional failed")

    assert native_module().daily_prior_close(failure, DAY, CODE) == (CODE, None)

    def interrupted(code, **kwargs):
        raise KeyboardInterrupt("cancelled")

    with pytest.raises(KeyboardInterrupt, match="cancelled"):
        native_module().daily_prior_close(interrupted, DAY, CODE)


def test_daily_capital_events_filters_today_and_keeps_last_nonempty_block():
    first = SimpleNamespace(category_raw=1, date=DAY)
    last = SimpleNamespace(category_raw=1, date=DAY)
    skip = SimpleNamespace(category_raw=2, date=DAY)
    blocks = [SimpleNamespace(full_code=CODE, records=(first, skip)),
              SimpleNamespace(full_code=CODE, records=(last,)),
              SimpleNamespace(full_code=CODE, records=())]
    client = Client(corporate=Client(capital_changes=lambda codes: Client(blocks=blocks)))
    helper = core.HelperApi(client)
    assert helper._today_capital_change_map([CODE], DAY) == {CODE: (last,)}


def test_daily_capital_events_downgrades_request_not_bad_record_iteration():
    def failure(codes):
        raise ValueError("optional request")

    client = Client(corporate=Client(capital_changes=failure))
    helper = core.HelperApi(client)
    assert helper._today_capital_change_map([CODE], DAY) == {}

    class BadRecords:
        def __iter__(self):
            raise ValueError("bad records")

    client.corporate.capital_changes = lambda codes: Client(full_code=CODE, records=BadRecords())
    with pytest.raises(ValueError, match="bad records"):
        helper._today_capital_change_map([CODE], DAY)
