"""Intraday minute-chart API."""

from __future__ import annotations

from .base import ApiBase


class MinuteApi(ApiBase):
    def today(self, code: str, *, include_raw: bool = False):
        return self._execute_priced("today_intraday", code=code, include_raw=include_raw, price_codes=[code])

    def history(self, code: str, trading_date, *, include_raw: bool = False):
        return self._execute_priced("historical_intraday", code=code, trading_date=trading_date, include_raw=include_raw, price_codes=[code])

    def recent(self, code: str, trading_date=None, *, include_raw: bool = False):
        return self._execute_priced("recent_intraday", code=code, trading_date=trading_date, include_raw=include_raw, price_codes=[code])

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
