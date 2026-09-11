"""Intraday minute-chart API."""

from __future__ import annotations

from collections.abc import Sequence
from concurrent.futures import ThreadPoolExecutor

from eltdx.protocol.unit import normalize_code

from .base import ApiBase


class MinuteApi(ApiBase):
    def today(self, code: str | Sequence[str], *, include_raw: bool = False, batch_size: int | None = None):
        if not isinstance(code, str):
            return self._run_many(code, lambda item: self.today(item, include_raw=include_raw), batch_size)
        return self._execute_priced("today_intraday", code=code, include_raw=include_raw, price_codes=[code])

    def history(self, code: str | Sequence[str], trading_date, *, include_raw: bool = False, batch_size: int | None = None):
        if not isinstance(code, str):
            return self._run_many(code, lambda item: self.history(item, trading_date, include_raw=include_raw), batch_size)
        return self._execute_priced("historical_intraday", code=code, trading_date=trading_date, include_raw=include_raw, price_codes=[code])

    def recent(self, code: str | Sequence[str], trading_date=None, *, include_raw: bool = False, batch_size: int | None = None):
        if not isinstance(code, str):
            return self._run_many(code, lambda item: self.recent(item, trading_date, include_raw=include_raw), batch_size)
        return self._execute_priced("recent_intraday", code=code, trading_date=trading_date, include_raw=include_raw, price_codes=[code])

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
            return dict(zip(normalized, executor.map(query, normalized)))

    def aux(self, code: str, kind: str | int = "buy_sell_strength", *, include_raw: bool = False):
        return self._execute("intraday_aux", code=code, kind=kind, include_raw=include_raw)

    def buy_sell_strength(self, code: str, *, include_raw: bool = False):
        """Return the buy/sell commission-strength series with named fields."""
        return self.aux(code, kind="buy_sell_strength", include_raw=include_raw)

    def volume_comparison(self, code: str, *, include_raw: bool = False):
        """Return current versus previous-day cumulative volume fields."""
        return self.aux(code, kind="volume_comparison", include_raw=include_raw)

    def sparkline(self, code: str, *, selector: int = 1, window: int = 20, include_raw: bool = False):
        return self._execute("sparkline", code=code, selector=selector, window=window, include_raw=include_raw)
