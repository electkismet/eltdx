"""K-line/bar API."""

from __future__ import annotations

from collections.abc import Sequence
from concurrent.futures import ThreadPoolExecutor
from dataclasses import replace

from eltdx.protocol.constants import MAX_KLINE_PAGE_SIZE
from eltdx.protocol.unit import normalize_code

from .base import ApiBase


class BarApi(ApiBase):
    def get(
        self,
        code: str | Sequence[str],
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
        batch_size: int | None = None,
    ):
        _validate_batch_size(batch_size)
        if not all_pages:
            _validate_page_size(count)
        else:
            _validate_page_size(page_size)
            if max_pages is not None and max_pages <= 0:
                raise ValueError("max_pages must be positive or None")

        if not isinstance(code, str):
            codes = _normalize_codes(code)
            workers = _batch_workers(self._transport, len(codes), batch_size)
            def query(item: str):
                return self.get(
                    item,
                    period=period,
                    start=start,
                    count=count,
                    adjust=adjust,
                    anchor_date=anchor_date,
                    kind=kind,
                    include_raw=include_raw,
                    all_pages=all_pages,
                    page_size=page_size,
                    max_pages=max_pages,
                )

            with ThreadPoolExecutor(max_workers=workers) as executor:
                results: dict[str, object] = {}
                for offset in range(0, len(codes), workers):
                    chunk = codes[offset : offset + workers]
                    results.update(zip(chunk, executor.map(query, chunk)))
                return results

        if not all_pages:
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
    if value is not None and (
        isinstance(value, bool) or not isinstance(value, int) or value <= 0
    ):
        raise ValueError("batch_size must be a positive integer or None")


def _batch_workers(transport, code_count: int, batch_size: int | None) -> int:
    capacity = getattr(transport, "pool_size", 1)
    if isinstance(capacity, bool) or not isinstance(capacity, int) or capacity <= 0:
        capacity = 1
    requested = capacity if batch_size is None else batch_size
    return min(code_count, requested, capacity)


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
