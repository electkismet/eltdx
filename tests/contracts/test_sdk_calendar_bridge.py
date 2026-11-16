"""Authored legacy calendar obligations; no execution during implementation."""
from __future__ import annotations

from datetime import date, datetime
from types import SimpleNamespace
import gc

import pytest

from eltdx.workday import WorkdayService, _weekday_range


class Client:
    def __init__(self, days):
        self.calls = []
        self.days = days
        self.bars = SimpleNamespace(get=self.get)

    def get(self, *args, **kwargs):
        self.calls.append((args, kwargs))
        return SimpleNamespace(bars=[SimpleNamespace(time=datetime.combine(day, datetime.min.time())) for day in self.days])


def test_native_calendar_refresh_keeps_sorted_unique_benchmark_and_cache():
    client = Client([date(2026, 6, 2), date(2026, 5, 29), date(2026, 6, 2)])
    service = WorkdayService(client, benchmark_code="sh000300")
    assert service.refresh() == 2
    assert service.range("20260529", "20260602") == [date(2026, 5, 29), date(2026, 6, 2)]
    assert client.calls == [(("sh000300",), dict(period="day", kind="index", all_pages=True))]
    assert service.is_workday("20260602") and not service.is_workday("20260601")
    assert len(client.calls) == 1
    service.clear()
    assert service.is_workday("20260602") and len(client.calls) == 2


def test_native_calendar_range_retains_inclusive_reversal_and_descending():
    client = Client([date(2026, 5, 29), date(2026, 6, 1), date(2026, 6, 2)])
    service = WorkdayService(client)
    assert service.range("20260602", "20260529", descending=True) == [date(2026, 6, 2), date(2026, 6, 1), date(2026, 5, 29)]
    assert list(service.iter_days("20260601", "20260601")) == [date(2026, 6, 1)]
    assert service.range("20270101", "20270103") == []


@pytest.mark.parametrize("include_self", [False, True])
def test_native_calendar_neighbors_keep_observed_coverage_none(include_self):
    client = Client([date(2026, 5, 29), date(2026, 6, 1)])
    service = WorkdayService(client)
    assert service.next_workday("20260529", include_self=include_self) == (date(2026, 5, 29) if include_self else date(2026, 6, 1))
    assert service.previous_workday("20260601", include_self=include_self) == (date(2026, 6, 1) if include_self else date(2026, 5, 29))
    assert service.next_workday("20270101") is None
    assert service.previous_workday("20250101") is None


def test_native_calendar_empty_benchmark_does_not_use_weekday_approximation():
    client = Client([])
    service = WorkdayService(client)
    assert not service.is_workday("20260601")
    assert service.range("20260529", "20260602") == []
    assert service.next_workday("20260601") is None
    assert service.previous_workday("20260601") is None


def test_native_calendar_unbound_retains_weekday_approximation():
    service = WorkdayService()
    assert service.refresh() == 0
    assert not service.is_workday("20260530") and service.is_workday("20260601")
    assert service.next_workday("20260530") == date(2026, 6, 1)
    assert service.previous_workday("20260530") == date(2026, 5, 29)
    assert service.range("20260602", "20260529") == [date(2026, 5, 29), date(2026, 6, 1), date(2026, 6, 2)]


def test_native_calendar_date_boundaries_retain_original_overflow():
    service = WorkdayService()
    with pytest.raises(OverflowError, match="date value out of range"):
        service.next_workday(date.max)
    with pytest.raises(OverflowError, match="date value out of range"):
        service.previous_workday(date.min)
    with pytest.raises(OverflowError, match="date value out of range"):
        _weekday_range(date.max, date.max)
    assert service.previous_workday(date.min, include_self=True) == date.min


def test_native_calendar_weak_client_does_not_fall_back_after_collection():
    client = Client([])
    service = WorkdayService(client)
    del client
    gc.collect()
    with pytest.raises(ReferenceError, match="TdxClient 已被回收"):
        service.is_workday("20260601")


def test_native_calendar_respects_direct_cache_mutation_and_duplicate_neighbors():
    client = Client([])
    service = WorkdayService(client)
    service._loaded = True
    service._days = [date(2026, 6, 1), date(2026, 6, 1), date(2026, 6, 2)]
    service._day_set = {date(2026, 6, 2)}
    assert not service.is_workday("20260601")
    assert service.next_workday("20260601") == date(2026, 6, 2)
    assert service.range("20260601", "20260601") == [date(2026, 6, 1)] * 2


def test_native_calendar_non_boolean_include_self_keeps_truth_evaluation_once():
    class Flag:
        calls = 0

        def __bool__(self):
            self.calls += 1
            return True

    flag = Flag()
    assert WorkdayService().next_workday("20260601", include_self=flag) == date(2026, 6, 1)
    assert flag.calls == 1
