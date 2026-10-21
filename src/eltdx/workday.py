"""Trading-day helper service."""

from __future__ import annotations

import weakref
from bisect import bisect_left, bisect_right
from dataclasses import dataclass, field
from datetime import date, timedelta
from typing import TYPE_CHECKING

from eltdx.protocol.unit import date_from_yyyymmdd, yyyymmdd

if TYPE_CHECKING:
    from eltdx.client import TdxClient


@dataclass(slots=True, init=False)
class WorkdayService:
    """Trading-day helper backed by a benchmark daily K-line series.

    Without a client it falls back to weekday-only logic, which is useful for
    tests and simple date normalization. With a client it loads actual trading
    dates from ``benchmark_code`` daily bars.

    ``client`` 以弱引用保存（见 ``__init__``）：本服务由 ``TdxClient`` 持有，
    强引用回 client 会构成引用环，让 client 及其底层 worker 线程只能等分代 GC
    回收。``__init__`` 手工声明是为了在保持 ``WorkdayService(client=...)`` /
    ``client=None`` 语义不变的前提下，把 ``client`` 换成解析用的 property。
    """

    benchmark_code: str = "sh000001"
    _days: list[date] = field(default_factory=list, init=False, repr=False)
    _day_set: set[date] = field(default_factory=set, init=False, repr=False)
    _loaded: bool = field(default=False, init=False, repr=False)
    _client_ref: weakref.ref[TdxClient] | None = field(
        default=None, init=False, repr=False, compare=False
    )

    def __init__(self, client: TdxClient | None = None, benchmark_code: str = "sh000001") -> None:
        self.benchmark_code = benchmark_code
        self._days = []
        self._day_set = set()
        self._loaded = False
        self._client_ref = None if client is None else weakref.ref(client)

    @property
    def client(self) -> TdxClient | None:
        """解析 client 弱引用。

        未绑定 client 时返回 ``None``（星期几降级逻辑依赖它）；client 已被
        回收时抛 ``ReferenceError``，而不是悄悄退回星期几推算 —— 那会把
        「拿不到真实交易日」伪装成一份看似正常的结果。
        """

        ref = self._client_ref
        if ref is None:
            return None
        client = ref()
        if client is None:
            raise ReferenceError("TdxClient 已被回收")
        return client

    def today(self) -> date:
        return self.normalize(None)

    def normalize(self, value=None) -> date:
        parsed = date_from_yyyymmdd(yyyymmdd(value))
        if parsed is None:
            raise ValueError(f"invalid date: {value!r}")
        return parsed

    def text(self, value=None) -> str:
        return self.normalize(value).isoformat()

    def same_day(self, left, right) -> bool:
        return self.normalize(left) == self.normalize(right)

    def refresh(self) -> int:
        if self.client is None:
            self._days = []
            self._day_set = set()
            self._loaded = True
            return 0

        series = self.client.bars.get(self.benchmark_code, period="day", kind="index", all_pages=True)
        days = sorted({bar.time.date() for bar in series.bars})
        self._days = days
        self._day_set = set(days)
        self._loaded = True
        return len(days)

    def clear(self) -> None:
        self._days = []
        self._day_set = set()
        self._loaded = False

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
            days = _weekday_range(start_day, end_day)
        else:
            self._ensure_loaded()
            left = bisect_left(self._days, start_day)
            right = bisect_right(self._days, end_day)
            days = list(self._days[left:right])

        if descending:
            days.reverse()
        return days

    def iter_days(self, start, end, *, descending: bool = False):
        yield from self.range(start, end, descending=descending)

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


def _weekday_range(start_day: date, end_day: date) -> list[date]:
    days: list[date] = []
    current = start_day
    while current <= end_day:
        if current.weekday() < 5:
            days.append(current)
        current += timedelta(days=1)
    return days
