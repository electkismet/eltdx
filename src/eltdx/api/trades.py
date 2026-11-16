"""Tick/trade API."""

from __future__ import annotations

from collections.abc import Callable, Sequence
from concurrent.futures import ThreadPoolExecutor
from dataclasses import replace
from datetime import date, datetime
import math
from typing import Any

from eltdx.protocol.constants import MAX_TRADE_PAGE_SIZE
from eltdx.protocol.unit import normalize_code
from eltdx.protocol.commands import command_code
from eltdx.models import TradeBatch, TradePage
from eltdx.transport.native import native_module

from .base import ApiBase


class TradeApi(ApiBase):
    def today(
        self,
        code: str | Sequence[str],
        *,
        start: int = 0,
        count: int = 1800,
        include_raw: bool = False,
        batch_size: int | None = None,
    ):
        _validate_page_size(count)
        _validate_batch_size(batch_size)
        if not isinstance(code, str):
            return self._run_many(
                code,
                lambda item: self.today(
                    item, start=start, count=count, include_raw=include_raw
                ),
                batch_size,
            )
        return self._query("today_ticks", code=code, start=start, count=count, include_raw=include_raw)

    def history(
        self,
        code: str | Sequence[str],
        trading_date,
        *,
        start: int = 0,
        count: int = 1800,
        include_raw: bool = False,
        batch_size: int | None = None,
    ):
        _validate_page_size(count)
        _validate_batch_size(batch_size)
        if not isinstance(code, str):
            return self._run_many(
                code,
                lambda item: self.history(
                    item,
                    trading_date,
                    start=start,
                    count=count,
                    include_raw=include_raw,
                ),
                batch_size,
            )
        return self._query(
            "historical_ticks",
            code=code,
            trading_date=trading_date,
            start=start,
            count=count,
            include_raw=include_raw,
        )

    def all_today(
        self,
        code: str | Sequence[str],
        *,
        page_size: int = 1800,
        max_pages: int | None = 100,
        include_raw: bool = False,
        batch_size: int | None = None,
    ):
        _validate_batch_size(batch_size)
        if not isinstance(code, str):
            return self._run_many(
                code,
                lambda item: self._all(
                    item,
                    None,
                    page_size=page_size,
                    max_pages=max_pages,
                    include_raw=include_raw,
                ),
                batch_size,
            )
        return self._all(code, None, page_size=page_size, max_pages=max_pages, include_raw=include_raw)

    def all_history(
        self,
        code: str | Sequence[str],
        trading_date,
        *,
        page_size: int = 1800,
        max_pages: int | None = 100,
        include_raw: bool = False,
        batch_size: int | None = None,
    ):
        _validate_batch_size(batch_size)
        if not isinstance(code, str):
            return self._run_many(
                code,
                lambda item: self._all(
                    item,
                    trading_date,
                    page_size=page_size,
                    max_pages=max_pages,
                    include_raw=include_raw,
                ),
                batch_size,
            )
        return self._all(code, trading_date, page_size=page_size, max_pages=max_pages, include_raw=include_raw)

    def history_batch(
        self,
        code: str | Sequence[str],
        trading_date,
        *,
        start: int = 0,
        count: int = 1800,
        include_raw: bool = False,
        batch_size: int | None = None,
    ) -> TradeBatch | dict[str, TradeBatch]:
        """Read one historical page as bulk fields, without creating tick objects."""
        _validate_page_size(count)
        _validate_batch_size(batch_size)
        if not isinstance(code, str):
            return self._run_many(
                code,
                lambda item: self.history_batch(
                    item, trading_date, start=start, count=count,
                    include_raw=include_raw,
                ),
                batch_size,
            )
        payload = dict(code=code, trading_date=trading_date, start=start,
                       count=count, include_raw=include_raw)
        bulk_execute = getattr(self._transport, "execute_trade_batch", None)
        if self._native_services:
            bulk_execute = self._transport.sdk_trade_batch
        if callable(bulk_execute):
            result = bulk_execute(command_code("historical_ticks"), payload)
            if not isinstance(result, TradeBatch):
                raise TypeError("execute_trade_batch() must return TradeBatch")
            return result
        # Preserve custom transports implementing only the original protocol.
        page = self._execute("historical_ticks", **payload)
        if not isinstance(page, TradePage):
            raise TypeError("historical tick transport must return TradePage or support execute_trade_batch()")
        return TradeBatch.from_page(page)

    def today_batch(
        self,
        code: str | Sequence[str],
        *,
        start: int = 0,
        count: int = 1800,
        include_raw: bool = False,
        batch_size: int | None = None,
    ) -> TradeBatch | dict[str, TradeBatch]:
        """Read today's trade page as bulk fields, without creating tick objects."""
        _validate_page_size(count)
        _validate_batch_size(batch_size)
        if not isinstance(code, str):
            return self._run_many(
                code,
                lambda item: self.today_batch(
                    item, start=start, count=count, include_raw=include_raw,
                ),
                batch_size,
            )
        payload = dict(code=code, start=start, count=count, include_raw=include_raw)
        bulk_execute = getattr(self._transport, "execute_trade_batch", None)
        if self._native_services:
            bulk_execute = self._transport.sdk_trade_batch
        if callable(bulk_execute):
            result = bulk_execute(command_code("today_ticks"), payload)
            if not isinstance(result, TradeBatch):
                raise TypeError("execute_trade_batch() must return TradeBatch")
            return result
        page = self._execute("today_ticks", **payload)
        if not isinstance(page, TradePage):
            raise TypeError("trade tick transport must return TradePage or support execute_trade_batch()")
        return TradeBatch.from_page(page)

    def all_today_batch(
        self,
        code: str | Sequence[str],
        *,
        page_size: int = 1800,
        max_pages: int | None = 100,
        include_raw: bool = False,
        batch_size: int | None = None,
    ) -> TradeBatch | dict[str, TradeBatch]:
        """Read all today's ticks as bulk fields, merging pages without tick objects."""
        _validate_page_size(page_size)
        _validate_batch_size(batch_size)
        if max_pages is not None and max_pages <= 0:
            raise ValueError("max_pages must be positive or None")
        if not isinstance(code, str):
            return self._run_many(
                code,
                lambda item: self.all_today_batch(
                    item, page_size=page_size, max_pages=max_pages,
                    include_raw=include_raw,
                ),
                batch_size,
            )
        start = 0
        pages: list[TradeBatch] = []
        while True:
            page = self.today_batch(
                code, start=start, count=page_size, include_raw=include_raw,
            )
            assert isinstance(page, TradeBatch)
            pages.append(page)
            if page.count == 0:
                blocks = tuple(block for part in reversed(pages) for block in part._blocks)
                return replace(pages[0], start=0, request_count=start, _blocks=blocks, raw_payload=b"")
            if max_pages is not None and len(pages) >= max_pages:
                raise RuntimeError("trade pagination reached max_pages before an empty page")
            start += page.count

    def all_history_batch(
        self,
        code: str | Sequence[str],
        trading_date,
        *,
        page_size: int = 1800,
        max_pages: int | None = 100,
        include_raw: bool = False,
        batch_size: int | None = None,
    ) -> TradeBatch | dict[str, TradeBatch]:
        """Read complete historical ticks as bulk fields; merge pages without rows."""
        _validate_page_size(page_size)
        _validate_batch_size(batch_size)
        if max_pages is not None and max_pages <= 0:
            raise ValueError("max_pages must be positive or None")
        if not isinstance(code, str):
            return self._run_many(
                code,
                lambda item: self.all_history_batch(
                    item, trading_date, page_size=page_size, max_pages=max_pages,
                    include_raw=include_raw,
                ),
                batch_size,
            )
        if self._native_collect_arguments(code, trading_date, page_size, include_raw, max_pages, bulk=True):
            return self._transport.sdk_trade_read(
                command_code("historical_ticks"),
                dict(code=code, trading_date=trading_date, start=0, count=page_size, include_raw=include_raw),
                collect=True, opening=False, max_pages=_native_page_limit(max_pages), bulk=True,
            )
        start = 0
        pages: list[TradeBatch] = []
        while True:
            page = self.history_batch(
                code, trading_date, start=start, count=page_size, include_raw=include_raw,
            )
            assert isinstance(page, TradeBatch)
            pages.append(page)
            if page.count == 0:
                # As with all_history(), reverse pages, not rows within a page.
                blocks = tuple(block for part in reversed(pages) for block in part._blocks)
                return replace(pages[0], start=0, request_count=start, _blocks=blocks, raw_payload=b"")
            if max_pages is not None and len(pages) >= max_pages:
                raise RuntimeError("trade pagination reached max_pages before an empty page")
            start += page.count

    def opening_match_today(
        self,
        code: str | Sequence[str],
        *,
        page_size: int = 1800,
        max_pages: int | None = 100,
        include_raw: bool = False,
        batch_size: int | None = None,
    ):
        """Return today's 09:25 formal opening match from ``0x0fc5``."""

        _validate_batch_size(batch_size)
        if not isinstance(code, str):
            return self._run_many(
                code,
                lambda item: self._find_opening_match(
                    item,
                    None,
                    page_size=page_size,
                    max_pages=max_pages,
                    include_raw=include_raw,
                ),
                batch_size,
            )
        return self._find_opening_match(code, None, page_size=page_size, max_pages=max_pages, include_raw=include_raw)

    def opening_match_history(
        self,
        code: str | Sequence[str],
        trading_date,
        *,
        page_size: int = 1800,
        max_pages: int | None = 100,
        include_raw: bool = False,
        batch_size: int | None = None,
    ):
        """Return a historical day's 09:25 formal opening match from ``0x0fc6``."""

        _validate_batch_size(batch_size)
        if not isinstance(code, str):
            return self._run_many(
                code,
                lambda item: self._find_opening_match(
                    item,
                    trading_date,
                    page_size=page_size,
                    max_pages=max_pages,
                    include_raw=include_raw,
                ),
                batch_size,
            )
        return self._find_opening_match(code, trading_date, page_size=page_size, max_pages=max_pages, include_raw=include_raw)

    def _run_many(
        self,
        codes: Sequence[str],
        query: Callable[[str], Any],
        batch_size: int | None,
    ) -> dict[str, Any]:
        normalized = _normalize_codes(codes)
        workers = _batch_workers(self._transport, len(normalized), batch_size)
        with ThreadPoolExecutor(max_workers=workers) as executor:
            # Submit only one worker-sized chunk at a time so large code lists
            # do not fill the transport's pending-request queue.
            results = native_module().map_batches(executor, normalized, query, workers)
        return results

    def _query(self, name: str, **payload):
        if self._native_services:
            return self._transport.sdk_trades(command_code(name), payload)
        return self._execute(name, **payload)

    def _find_opening_match(self, code: str, trading_date, *, page_size: int, max_pages: int | None, include_raw: bool):
        _validate_page_size(page_size)
        if max_pages is not None and max_pages <= 0:
            raise ValueError("max_pages must be positive or None")
        if self._native_collect_arguments(code, trading_date, page_size, include_raw, max_pages):
            return self._native_read(code, trading_date, page_size, max_pages, include_raw, opening=True)
        start = 0
        pages = 0
        while True:
            page = self.today(code, start=start, count=page_size, include_raw=include_raw) if trading_date is None else self.history(code, trading_date, start=start, count=page_size, include_raw=include_raw)
            matches = getattr(page, "opening_matches", ())
            if matches:
                return matches[0]
            if not hasattr(page, "count") or page.count == 0:
                return None
            pages += 1
            if max_pages is not None and pages >= max_pages:
                raise RuntimeError("trade pagination reached max_pages before an empty page")
            start += page.count

    def _all(self, code: str, trading_date, *, page_size: int, max_pages: int | None, include_raw: bool):
        _validate_page_size(page_size)
        if max_pages is not None and max_pages <= 0:
            raise ValueError("max_pages must be positive or None")
        if self._native_collect_arguments(code, trading_date, page_size, include_raw, max_pages):
            return self._native_read(code, trading_date, page_size, max_pages, include_raw, opening=False)
        start = 0
        pages = 0
        first_page = None
        pages_ticks = []
        while True:
            page = self.today(code, start=start, count=page_size, include_raw=include_raw) if trading_date is None else self.history(code, trading_date, start=start, count=page_size, include_raw=include_raw)
            if not hasattr(page, "ticks") or not hasattr(page, "count"):
                return page
            first_page = first_page or page
            pages_ticks.append(tuple(page.ticks))
            pages += 1
            if page.count == 0:
                # The server returns the newest page at start=0 and older
                # pages at increasing offsets. Reverse page order while
                # preserving each page's wire order to build chronological
                # output without sorting away same-minute trade order.
                ticks = tuple(tick for page_ticks in reversed(pages_ticks) for tick in page_ticks)
                return replace(first_page, start=0, request_count=len(ticks), ticks=ticks, raw_payload=b"")
            if max_pages is not None and pages >= max_pages:
                raise RuntimeError("trade pagination reached max_pages before an empty page")
            start += page.count

    def _native_collect_arguments(self, code, trading_date, page_size, include_raw, max_pages, *, bulk=False):
        # Replaced page methods and custom objects retain their per-page Python
        # evaluation/extension behavior; ordinary calls use the shared pager.
        methods = ("history_batch",) if bulk else ("today", "history")
        return (self._native_services and type(self) is TradeApi
                and type(code) is str
                and type(trading_date) in (type(None), str, int, bool, date, datetime)
                and type(page_size) in (int, bool)
                and type(include_raw) is bool
                and type(max_pages) in (type(None), int, bool, float)
                and all(name not in self.__dict__
                        and getattr(TradeApi, name) is _NATIVE_TRADE_PAGE_METHODS[name]
                        for name in methods))

    def _native_read(self, code, trading_date, page_size, max_pages, include_raw, *, opening):
        payload = dict(code=code, start=0, count=page_size, include_raw=include_raw)
        name = "today_ticks"
        if trading_date is not None:
            name = "historical_ticks"
            payload["trading_date"] = trading_date
        return self._transport.sdk_trade_read(command_code(name), payload, collect=not opening,
                                               opening=opening, max_pages=_native_page_limit(max_pages))


_NATIVE_TRADE_PAGE_METHODS = {
    name: getattr(TradeApi, name) for name in ("today", "history", "history_batch")
}


def _native_page_limit(value):
    if value is None:
        return None
    if type(value) is float:
        if not math.isfinite(value):
            return None
        value = math.ceil(value)
    return int(value) if value <= 65536 else None


def _validate_page_size(value: int) -> None:
    if value <= 0 or value > MAX_TRADE_PAGE_SIZE:
        raise ValueError(f"page size must be between 1 and {MAX_TRADE_PAGE_SIZE}")


def _normalize_codes(codes: Sequence[str]) -> list[str]:
    normalized: list[str] = []
    seen: set[str] = set()
    for code in codes:
        full_code = normalize_code(code)
        if full_code not in seen:
            normalized.append(full_code)
            seen.add(full_code)
    if not normalized:
        raise ValueError("codes must not be empty")
    return normalized


def _validate_batch_size(value: int | None) -> None:
    if value is not None and (isinstance(value, bool) or not isinstance(value, int) or value <= 0):
        raise ValueError("batch_size must be a positive integer or None")


def _batch_workers(transport: Any, code_count: int, batch_size: int | None) -> int:
    capacity = getattr(transport, "pool_size", 1)
    if not isinstance(capacity, int) or isinstance(capacity, bool) or capacity <= 0:
        capacity = 1
    requested = capacity if batch_size is None else batch_size
    return min(code_count, requested, capacity)
