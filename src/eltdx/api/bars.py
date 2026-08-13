"""K-line/bar API."""

from __future__ import annotations

from dataclasses import replace

from eltdx.protocol.constants import MAX_KLINE_PAGE_SIZE

from .base import ApiBase


class BarApi(ApiBase):
    def get(
        self,
        code: str,
        *,
        period: str = "day",
        start: int = 0,
        count: int = 800,
        adjust: str | None = None,
        anchor_date=None,
        kind: str = "stock",
        include_raw: bool = False,
    ):
        _validate_page_size(count)
        return self._execute(
            "klines",
            code=code,
            period=period,
            start=start,
            count=count,
            adjust=adjust,
            anchor_date=anchor_date,
            kind=kind,
            include_raw=include_raw,
        )

    def all(
        self,
        code: str,
        *,
        period: str = "day",
        adjust: str | None = None,
        anchor_date=None,
        kind: str = "stock",
        page_size: int = 800,
        max_pages: int | None = 200,
        include_raw: bool = False,
    ):
        _validate_page_size(page_size)
        if max_pages is not None and max_pages <= 0:
            raise ValueError("max_pages must be positive or None")

        start = 0
        pages = 0
        first_page = None
        bars = []
        while True:
            page = self.get(
                code,
                period=period,
                start=start,
                count=page_size,
                adjust=adjust,
                anchor_date=anchor_date,
                kind=kind,
                include_raw=include_raw,
            )
            if not hasattr(page, "bars") or not hasattr(page, "count"):
                return page
            if first_page is None:
                first_page = page
            bars.extend(page.bars)
            pages += 1
            if page.count == 0:
                return replace(first_page, request_count=len(bars), bars=tuple(bars))
            if max_pages is not None and pages >= max_pages:
                raise RuntimeError("bars.all reached max_pages before the server returned an empty page")
            start += page.count


def _validate_page_size(value: int) -> None:
    if value <= 0 or value > MAX_KLINE_PAGE_SIZE:
        raise ValueError(f"page size must be between 1 and {MAX_KLINE_PAGE_SIZE}")
