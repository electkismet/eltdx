"""Tick/trade API."""

from __future__ import annotations

from collections.abc import Callable, Sequence
from concurrent.futures import ThreadPoolExecutor
from dataclasses import replace
from typing import Any

from eltdx.protocol.constants import MAX_TRADE_PAGE_SIZE
from eltdx.protocol.unit import normalize_code

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
        return self._execute("today_ticks", code=code, start=start, count=count, include_raw=include_raw)

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
        return self._execute(
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
        results: dict[str, Any] = {}
        with ThreadPoolExecutor(max_workers=workers) as executor:
            # Submit only one worker-sized chunk at a time so large code lists
            # do not fill the transport's pending-request queue.
            for start in range(0, len(normalized), workers):
                chunk = normalized[start : start + workers]
                results.update(zip(chunk, executor.map(query, chunk)))
        return results

    def _find_opening_match(self, code: str, trading_date, *, page_size: int, max_pages: int | None, include_raw: bool):
        _validate_page_size(page_size)
        if max_pages is not None and max_pages <= 0:
            raise ValueError("max_pages must be positive or None")
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
                return replace(first_page, start=0, request_count=len(ticks), ticks=ticks)
            if max_pages is not None and pages >= max_pages:
                raise RuntimeError("trade pagination reached max_pages before an empty page")
            start += page.count


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
