"""Intraday minute-chart API."""

from __future__ import annotations

from collections.abc import Sequence
from concurrent.futures import ThreadPoolExecutor

from eltdx.protocol.unit import normalize_code
from eltdx.protocol.commands import command_code
from eltdx.transport.native import native_module

from .base import ApiBase


class MinuteApi(ApiBase):
    def today(self, code: str | Sequence[str], *, include_raw: bool = False, batch_size: int | None = None):
        if not isinstance(code, str):
            return self._run_many(code, lambda item: self.today(item, include_raw=include_raw), batch_size)
        return self._query("today_intraday", priced=True, code=code, include_raw=include_raw)

    def history(self, code: str | Sequence[str], trading_date, *, include_raw: bool = False, batch_size: int | None = None):
        if not isinstance(code, str):
            return self._run_many(code, lambda item: self.history(item, trading_date, include_raw=include_raw), batch_size)
        return self._query("historical_intraday", priced=True, code=code, trading_date=trading_date, include_raw=include_raw)

    def recent(self, code: str | Sequence[str], trading_date=None, *, include_raw: bool = False, batch_size: int | None = None):
        if not isinstance(code, str):
            return self._run_many(code, lambda item: self.recent(item, trading_date, include_raw=include_raw), batch_size)
        return self._query("recent_intraday", priced=True, code=code, trading_date=trading_date, include_raw=include_raw)

    def _query(self, name: str, *, priced: bool, **payload):
        if not self._native_services:
            if priced:
                return self._execute_priced(name, price_codes=[payload["code"]], **payload)
            return self._execute(name, **payload)
        resolver = self._price_resolver() if priced and self._price_resolver is not None else None
        codes = [payload["code"]]
        if resolver is not None:
            resolver(name, None, codes)
        result = self._transport.sdk_minutes(command_code(name), payload)
        if resolver is not None:
            return resolver(name, result, codes)
        return result

    def _run_many(self, codes: Sequence[str], query, batch_size: int | None):
        if batch_size is not None and (isinstance(batch_size, bool) or not isinstance(batch_size, int) or batch_size <= 0):
            raise ValueError("batch_size must be a positive integer or None")
        normalized = []
        seen = set()
        for code in codes:
            value = normalize_code(code)
            if value not in seen:
                normalized.append(value)
                seen.add(value)
        if not normalized:
            raise ValueError("codes must not be empty")
        capacity = getattr(self._transport, "pool_size", 1)
        if not isinstance(capacity, int) or isinstance(capacity, bool) or capacity <= 0:
            capacity = 1
        workers = min(len(normalized), capacity if batch_size is None else batch_size)
        with ThreadPoolExecutor(max_workers=workers) as executor:
            return native_module().map_batches(executor, normalized, query, None)

    def aux(self, code: str, kind: str | int = "buy_sell_strength", *, include_raw: bool = False):
        return self._query("intraday_aux", priced=False, code=code, kind=kind, include_raw=include_raw)

    def buy_sell_strength(self, code: str, *, include_raw: bool = False):
        """Return the buy/sell commission-strength series with named fields."""
        return self.aux(code, kind="buy_sell_strength", include_raw=include_raw)

    def volume_comparison(self, code: str, *, include_raw: bool = False):
        """Return current versus previous-day cumulative volume fields."""
        return self.aux(code, kind="volume_comparison", include_raw=include_raw)

    def sparkline(self, code: str, *, selector: int = 1, window: int = 20, include_raw: bool = False):
        return self._query("sparkline", priced=False, code=code, selector=selector, window=window, include_raw=include_raw)
