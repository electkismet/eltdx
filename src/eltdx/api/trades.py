"""Tick/trade API."""

from __future__ import annotations

from dataclasses import replace

from eltdx.protocol.constants import MAX_TRADE_PAGE_SIZE

from .base import ApiBase


class TradeApi(ApiBase):
    def today(self, code: str, *, start: int = 0, count: int = 1800, include_raw: bool = False):
        _validate_page_size(count)
        return self._execute("today_ticks", code=code, start=start, count=count, include_raw=include_raw)

    def history(self, code: str, trading_date, *, start: int = 0, count: int = 1800, include_raw: bool = False):
        _validate_page_size(count)
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

    def all_history(self, code: str, trading_date, *, page_size: int = 1800, max_pages: int | None = 100, include_raw: bool = False):
        return self._all(code, trading_date, page_size=page_size, max_pages=max_pages, include_raw=include_raw)

    def auction_snapshots(
        self,
        code: str,
        trading_date=None,
        *,
        page_size: int = 1800,
        max_pages: int | None = 100,
        include_raw: bool = False,
    ):
        """Return embedded ``status=8`` auction snapshots from 0x0fc5/0x0fc6."""

        page = (
            self.all_today(code, page_size=page_size, max_pages=max_pages, include_raw=include_raw)
            if trading_date is None
            else self.all_history(code, trading_date, page_size=page_size, max_pages=max_pages, include_raw=include_raw)
        )
        return tuple(page.auction_snapshots) if hasattr(page, "auction_snapshots") else ()

    def _all(self, code: str, trading_date, *, page_size: int, max_pages: int | None, include_raw: bool):
        _validate_page_size(page_size)
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
            if page.count == 0:
                return replace(first_page, start=0, request_count=len(ticks), ticks=tuple(ticks))
            if max_pages is not None and pages >= max_pages:
                raise RuntimeError("trade pagination reached max_pages before an empty page")
            start += page.count


def _validate_page_size(value: int) -> None:
    if value <= 0 or value > MAX_TRADE_PAGE_SIZE:
        raise ValueError(f"page size must be between 1 and {MAX_TRADE_PAGE_SIZE}")
