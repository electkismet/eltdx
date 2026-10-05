"""Verified delisted-security daily K-line API."""

from __future__ import annotations

from dataclasses import replace

from eltdx.protocol.constants import MAX_KLINE_PAGE_SIZE

from .base import ApiBase


class DelistedBarApi(ApiBase):
    """Read unadjusted daily bars through the verified ``0x052B`` command."""

    def get(
        self,
        code: str,
        *,
        period: str = "day",
        start: int = 0,
        count: int = MAX_KLINE_PAGE_SIZE,
        adjust: str | None = None,
        include_raw: bool = False,
        all_pages: bool = False,
        page_size: int = MAX_KLINE_PAGE_SIZE,
        max_pages: int | None = 200,
    ):
        _validate_period(period)
        _validate_adjust(adjust)
        if not all_pages:
            _validate_page_size(count)
            return self._get_page(code, start=start, count=count, include_raw=include_raw)

        _validate_page_size(page_size)
        if max_pages is not None and max_pages <= 0:
            raise ValueError("max_pages must be positive or None")
        next_start = start
        pages = 0
        first_page = None
        bars = []
        while True:
            page = self._get_page(
                code, start=next_start, count=page_size, include_raw=include_raw
            )
            if not hasattr(page, "bars") or not hasattr(page, "count"):
                return page
            if first_page is None:
                first_page = page
            bars.extend(page.bars)
            pages += 1
            if page.count == 0:
                bars.sort(key=lambda bar: bar.time)
                return replace(first_page, request_count=len(bars), bars=tuple(bars))
            if max_pages is not None and pages >= max_pages:
                raise RuntimeError(
                    "delisted_bars.get reached max_pages before the server returned an empty page"
                )
            next_start += page.count

    def _get_page(self, code: str, *, start: int, count: int, include_raw: bool):
        return self._execute(
            "delisted_bars",
            code=code,
            start=start,
            count=count,
            include_raw=include_raw,
        )


def _validate_period(value: str) -> None:
    if not isinstance(value, str) or value.strip().lower() not in {"day", "1d", "d", "daily"}:
        raise ValueError("period must be 'day'; 0x052B other periods are not verified")


def _validate_adjust(value: str | None) -> None:
    if value is not None and (
        not isinstance(value, str) or value.strip().lower() not in {"", "none"}
    ):
        raise ValueError("adjust must be None or 'none'; 0x052B adjustment modes are not verified")


def _validate_page_size(value: int) -> None:
    if isinstance(value, bool) or not isinstance(value, int) or not 0 < value <= MAX_KLINE_PAGE_SIZE:
        raise ValueError(f"page size must be between 1 and {MAX_KLINE_PAGE_SIZE}")
