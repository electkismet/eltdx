"""Legacy wall-clock/cross-day contracts authored without executing them."""
from __future__ import annotations

from dataclasses import dataclass
from datetime import date, datetime, timedelta
from types import SimpleNamespace

import pytest

from eltdx import _bar_time as bt
from eltdx.mac import client as mac


@dataclass(frozen=True)
class Clock:
    hour: int
    minute: int
    marker: object


@dataclass(frozen=True)
class Timed:
    time: datetime
    datetime: datetime
    marker: object


@pytest.mark.parametrize("method,options", [
    (bt.align_hour_minute_bars, {}), (bt.align_time_bars, {}), (bt.align_datetime_bars, {}),
])
def test_native_bar_alignment_start_keeps_original_container_and_skips_validation(monkeypatch, method, options):
    rows = object()
    monkeypatch.setattr(bt, "check_bar_time", lambda value: pytest.fail("start short-circuits validator"))
    assert method(rows, delta_minutes=object(), bar_time="start", **options) is rows


def test_native_bar_alignment_keeps_within_day_wrap_separate_from_cross_day():
    marker = object()
    when = datetime(2026, 11, 17, 23, 58, 37, 123456)
    original = Timed(when, when, marker)
    wrapped = bt.align_time_bars([original], delta_minutes=5, bar_time="end")[0]
    shifted = bt.align_datetime_bars([original], delta_minutes=5, bar_time="end")[0]
    assert wrapped.time == datetime(2026, 11, 17, 0, 3, 37, 123456)
    assert shifted.datetime == datetime(2026, 11, 18, 0, 3, 37, 123456)
    assert original.time is when and wrapped.marker is marker and shifted.marker is marker
    clock = Clock(23, 58, marker)
    assert bt.align_hour_minute_bars([clock], delta_minutes=5, bar_time="end") == [Clock(0, 3, marker)]


@pytest.mark.parametrize("delta", [None, 0, -2])
def test_native_bar_alignment_end_without_positive_duration_keeps_identity(delta):
    bars = [object()]
    assert bt.align_hour_minute_bars(bars, delta_minutes=delta, bar_time="end") is bars
    assert bt.align_time_bars(bars, delta_minutes=delta, bar_time="end") is bars
    assert bt.align_datetime_bars(bars, delta_minutes=delta, bar_time="end") is bars
    with pytest.raises(ValueError, match="得到: 'invalid'"):
        bt.align_datetime_bars(bars, delta_minutes=delta, bar_time="invalid")


def test_native_bar_time_needed_returns_original_final_comparison_object(monkeypatch):
    marker = object()
    calls = []

    class Duration:
        def __gt__(self, other):
            calls.append(other)
            return marker

    monkeypatch.setattr(bt, "check_bar_time", lambda value: calls.append(value))
    assert bt._needs_align("end", Duration()) is marker
    assert calls == ["end", 0]


def test_native_bar_time_check_keeps_live_valid_set_and_original_exception_instance(monkeypatch):
    monkeypatch.setattr(bt, "VALID_BAR_TIMES", ("custom",))
    assert bt.check_bar_time("custom") is None
    failure = ValueError("custom constructor")
    messages = []
    monkeypatch.setattr(bt, "ValueError", lambda message: messages.append(message) or failure, raising=False)
    with pytest.raises(ValueError) as caught:
        bt.check_bar_time("start")
    assert caught.value is failure
    assert messages == ["bar_time 必须是 'start' 或 'end'，得到: 'start'"]


def test_native_bar_time_error_formats_source_constants_in_original_order(monkeypatch):
    events = []

    class Start:
        def __repr__(self):
            events.append("start-repr")
            monkeypatch.setattr(bt, "BAR_TIME_END", "replaced-end")
            return "START"

    monkeypatch.setattr(bt, "BAR_TIME_START", Start())
    with pytest.raises(ValueError) as caught:
        bt.check_bar_time("invalid")
    assert str(caught.value) == "bar_time 必须是 START 或 'replaced-end'，得到: 'invalid'"
    assert events == ["start-repr"]


def test_native_bar_minutes_keep_ex_mac_and_hq_period_eight_distinct():
    assert bt.category_to_minutes(8) == 3
    assert bt.period_to_minutes(8, 3) == 15
    assert bt.kline_period_minutes(8, 3) == 3
    assert bt.period_to_minutes(8, 0) == 5 and bt.kline_period_minutes(8, 0) == 1
    assert bt.category_to_minutes(13) is None and bt.period_to_minutes(13, object()) is None


def test_native_bar_minutes_keep_repeated_int_conversion_and_parameter_laziness():
    events = []

    class Period:
        def __init__(self, values):
            self.values = iter(values)

        def __int__(self):
            result = next(self.values)
            events.append(result)
            return result

    assert bt.period_to_minutes(Period([0, 8]), 2) == 10
    assert events == [0, 8]
    events.clear()
    assert bt.kline_period_minutes(Period([0, 7]), object()) == 1
    assert events == [0, 7]
    events.clear()
    assert bt.period_to_minutes(Period([13]), object()) is None
    assert events == [13]


def test_native_bar_minutes_lookup_hook_precedes_live_int_conversion(monkeypatch):
    events = []

    class Registry:
        @property
        def get(self):
            events.append("get")
            monkeypatch.setattr(bt, "int", lambda value: events.append(("int", value)) or "key", raising=False)
            return lambda key: events.append(("lookup", key)) or "custom"

    monkeypatch.setattr(bt, "EX_CATEGORY_MINUTES", Registry())
    assert bt.category_to_minutes("period") == "custom"
    assert events == ["get", ("int", "period"), ("lookup", "key")]


def test_native_bar_datetime_constructs_delta_once_before_iterating_even_empty_input(monkeypatch):
    events = []

    class Bars:
        def __iter__(self):
            events.append("iterate")
            return iter([])

    monkeypatch.setattr(bt, "timedelta", lambda **options: events.append(("delta", options)) or timedelta(minutes=5))
    assert bt.align_datetime_bars(Bars(), delta_minutes=5, bar_time="end") == []
    assert events == [("delta", {"minutes": 5}), "iterate"]


def test_native_bar_alignment_keeps_per_row_replace_and_field_hooks(monkeypatch):
    calls = []
    source = SimpleNamespace(custom=datetime(2026, 11, 17, 10, 0))
    marker = object()
    monkeypatch.setattr(bt, "getattr", lambda row, field: calls.append(("get", row, field)) or row.custom, raising=False)
    monkeypatch.setattr(bt, "replace", lambda row, **values: calls.append(("replace", row, values)) or marker)
    assert bt.align_datetime_bars([source], delta_minutes=5, bar_time="end", field="custom") == [marker]
    assert calls == [("get", source, "custom"), ("replace", source, {"custom": datetime(2026, 11, 17, 10, 5)})]


def test_native_bar_datetime_captures_replace_before_getattr_side_effect(monkeypatch):
    source = object()
    marker = object()
    calls = []
    monkeypatch.setattr(bt, "replace", lambda row, **values: calls.append(values) or marker)

    def get(row, field):
        monkeypatch.setattr(bt, "replace", lambda *args, **options: pytest.fail("replace was already captured"))
        return datetime(2026, 11, 17)

    monkeypatch.setattr(bt, "getattr", get, raising=False)
    assert bt.align_datetime_bars([source], delta_minutes=5, bar_time="end") == [marker]
    assert calls == [{"datetime": datetime(2026, 11, 17, 0, 5)}]


def test_native_bar_alignment_keeps_original_numeric_and_calendar_errors():
    with pytest.raises(OverflowError):
        bt.align_datetime_bars([Timed(datetime.max, datetime.max, None)], delta_minutes=1, bar_time="end")
    with pytest.raises(TypeError):
        bt.category_to_minutes(object())
    with pytest.raises(ValueError):
        mac._ymd_to_date(20260230)
    assert mac._ymd_to_date(None) is None and mac._ymd_to_date(20261117) == date(2026, 11, 17)


def test_native_mac_ymd_keeps_custom_floor_mod_order_and_captured_date_constructor(monkeypatch):
    events = []
    original = lambda *parts: events.append(("date", parts)) or parts
    monkeypatch.setattr(mac, "_date", original)

    class Number:
        def __floordiv__(self, value):
            events.append(("floor", value))
            monkeypatch.setattr(mac, "_date", lambda *parts: pytest.fail("date constructor captured before arithmetic"))
            return 2026

        def __mod__(self, value):
            events.append(("mod", value))
            return 1117 if value == 10000 else 17

    assert mac._ymd_to_date(Number()) == (2026, 11, 17)
    assert events == [("floor", 10000), ("mod", 10000), ("mod", 100), ("date", (2026, 11, 17))]


def test_native_mac_ymd_does_not_narrow_custom_date_input_to_rust_integer(monkeypatch):
    huge = 10 ** 100 + 20261117
    calls = []
    monkeypatch.setattr(mac, "_date", lambda *parts: calls.append(parts) or parts)
    assert mac._ymd_to_date(huge) == (huge // 10000, (huge % 10000) // 100, huge % 100)
    assert len(calls) == 1
