from __future__ import annotations

from bisect import bisect_left, bisect_right
from dataclasses import dataclass, field
from datetime import date, timedelta

from ..client import TdxClient

from ..protocol.unit import normalize_trading_date


@dataclass(slots=True)
class WorkdayService:
    client: TdxClient | None = None
    benchmark_code: str = "sh000001"
    _days: list[date] = field(default_factory=list, init=False, repr=False)
    _day_set: set[date] = field(default_factory=set, init=False, repr=False)
    _loaded: bool = field(default=False, init=False, repr=False)

    def today(self) -> date:
        _, trading_day = normalize_trading_date(None)
        return trading_day

    def normalize(self, value=None) -> date:
        _, trading_day = normalize_trading_date(value)
        return trading_day

    def text(self, value=None) -> str:
        text, _ = normalize_trading_date(value)
        return text

    def same_day(self, left, right) -> bool:
        return self.normalize(left) == self.normalize(right)

    def refresh(self) -> int:
        if self.client is None:
            self._days = []
            self._day_set = set()
            self._loaded = True
            return 0

        response = self.client.get_kline_all(self.benchmark_code, "day", kind="index")
        days = sorted({item.time.date() for item in response.items})
        self._days = days
        self._day_set = set(days)
        self._loaded = True
        return len(days)

    def is_workday(self, value=None) -> bool:
        trading_day = self.normalize(value)
        if self.client is None:
            return trading_day.weekday() < 5

        self._ensure_loaded()
        return trading_day in self._day_set

    def today_is_workday(self) -> bool:
        return self.is_workday(None)

    def range(self, start, end, *, descending: bool = False) -> list[date]:
        start_day = self.normalize(start)
        end_day = self.normalize(end)
        if start_day > end_day:
            start_day, end_day = end_day, start_day

        if self.client is None:
            days = []
            current = start_day
            while current <= end_day:
                if current.weekday() < 5:
                    days.append(current)
                current += timedelta(days=1)
        else:
            self._ensure_loaded()
            left = bisect_left(self._days, start_day)
            right = bisect_right(self._days, end_day)
            days = list(self._days[left:right])

        if descending:
            days.reverse()
        return days

    def iter_days(self, start, end, *, descending: bool = False):
        for trading_day in self.range(start, end, descending=descending):
            yield trading_day

    def next_workday(self, value=None, *, include_self: bool = False) -> date | None:
        trading_day = self.normalize(value)
        if self.client is None:
            current = trading_day if include_self else trading_day + timedelta(days=1)
            while True:
                if current.weekday() < 5:
                    return current
                current += timedelta(days=1)

        self._ensure_loaded()
        index = bisect_left(self._days, trading_day) if include_self else bisect_right(self._days, trading_day)
        if index >= len(self._days):
            return None
        return self._days[index]

    def previous_workday(self, value=None, *, include_self: bool = False) -> date | None:
        trading_day = self.normalize(value)
        if self.client is None:
            current = trading_day if include_self else trading_day - timedelta(days=1)
            while True:
                if current.weekday() < 5:
                    return current
                current -= timedelta(days=1)

        self._ensure_loaded()
        index = bisect_right(self._days, trading_day) - 1 if include_self else bisect_left(self._days, trading_day) - 1
        if index < 0:
            return None
        return self._days[index]

    def _ensure_loaded(self) -> None:
        if not self._loaded:
            self.refresh()
