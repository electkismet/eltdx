"""K-line/bar API."""

from __future__ import annotations

from dataclasses import replace

from eltdx.protocol.constants import MAX_KLINE_PAGE_SIZE
from eltdx.protocol.unit import normalize_code

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
        kind: str | None = None,
        include_raw: bool = False,
        all_pages: bool = False,
        page_size: int = 800,
        max_pages: int | None = 200,
    ):
        if not all_pages:
            _validate_page_size(count)
            return self._get_page(
                code,
                period=period,
                start=start,
                count=count,
                adjust=adjust,
                anchor_date=anchor_date,
                kind=kind,
                include_raw=include_raw,
            )

        _validate_page_size(page_size)
        if max_pages is not None and max_pages <= 0:
            raise ValueError("max_pages must be positive or None")

        next_start = start
        pages = 0
        first_page = None
        bars = []
        while True:
            page = self._get_page(
                code,
                period=period,
                start=next_start,
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
                raise RuntimeError("bars.get reached max_pages before the server returned an empty page")
            next_start += page.count

    def _get_page(
        self,
        code: str,
        *,
        period: str,
        start: int,
        count: int,
        adjust: str | None,
        anchor_date,
        kind: str | None,
        include_raw: bool,
    ):
        resolved_kind = _resolve_kline_kind(code, kind)
        return self._execute(
            "klines",
            code=code,
            period=period,
            start=start,
            count=count,
            adjust=adjust,
            anchor_date=anchor_date,
            kind=resolved_kind,
            include_raw=include_raw,
        )


def _validate_page_size(value: int) -> None:
    if value <= 0 or value > MAX_KLINE_PAGE_SIZE:
        raise ValueError(f"page size must be between 1 and {MAX_KLINE_PAGE_SIZE}")


def _resolve_kline_kind(code: str, kind: str | None) -> str:
    """Choose the wire record layout when callers leave kind unspecified."""
    if kind is not None:
        return kind
    full_code = normalize_code(code)
    number = full_code[2:]
    if (
        (
            full_code.startswith("sh")
            and number.startswith(("000", "880", "881", "999"))
        )
        or (full_code.startswith("sz") and number.startswith("399"))
        or (full_code.startswith("bj") and number.startswith("899"))
    ):
        return "index"
    return "stock"
