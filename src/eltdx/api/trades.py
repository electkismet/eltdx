"""Tick/trade API."""

from __future__ import annotations

from dataclasses import replace

from .base import ApiBase


class TradeApi(ApiBase):
    def today(self, code: str, *, start: int = 0, count: int = 1800, include_raw: bool = False):
        return self._execute("today_ticks", code=code, start=start, count=count, include_raw=include_raw)

    def history(self, code: str, trading_date, *, start: int = 0, count: int = 2000, include_raw: bool = False):
        return self._execute(
            "historical_ticks",
            code=code,
            trading_date=trading_date,
            start=start,
            count=count,
            include_raw=include_raw,
        )

    def all_today(self, code: str, *, page_size: int = 1800, max_pages: int | None = 100, include_raw: bool = False):
        return self._all(code, None, page_size=page_size, max_pages=max_pages, include_raw=include_raw)

    def all_history(self, code: str, trading_date, *, page_size: int = 2000, max_pages: int | None = 100, include_raw: bool = False):
        return self._all(code, trading_date, page_size=page_size, max_pages=max_pages, include_raw=include_raw)

    def _all(self, code: str, trading_date, *, page_size: int, max_pages: int | None, include_raw: bool):
        if page_size <= 0 or page_size > 0xFFFF:
            raise ValueError("page_size must be between 1 and 65535")
        if max_pages is not None and max_pages <= 0:
            raise ValueError("max_pages must be positive or None")
        start = 0
        pages = 0
        first_page = None
        ticks = []
        while True:
            page = self.today(code, start=start, count=page_size, include_raw=include_raw) if trading_date is None else self.history(code, trading_date, start=start, count=page_size, include_raw=include_raw)
            if not hasattr(page, "ticks") or not hasattr(page, "count"):
                return page
            first_page = first_page or page
            ticks.extend(page.ticks)
            pages += 1
            if page.count < page_size:
                return replace(first_page, start=0, request_count=len(ticks), ticks=tuple(ticks))
            if max_pages is not None and pages >= max_pages:
                raise RuntimeError("trade pagination reached max_pages before a short page")
            start += page_size
