"""Security code table API."""

from __future__ import annotations

import builtins

from eltdx.protocol.constants import DEFAULT_CODE_PAGE_SIZE, MAX_CODE_PAGE_SIZE

from .base import ApiBase


class CodeApi(ApiBase):
    def count(self, market: str):
        return self._execute("security_count", market=market)

    def list(self, market: str, *, start: int = 0, limit: int = 1600):
        _validate_limit(limit)
        result = self._execute("security_list", market=market, start=start, limit=limit)
        if self._metadata_sink is not None:
            self._metadata_sink(result)
        return result

    def all(self, market: str, *, page_size: int = DEFAULT_CODE_PAGE_SIZE):
        _validate_page_size(page_size)
        start = 0
        items = []
        while True:
            page = self.list(market, start=start, limit=page_size)
            items.extend(page)
            if not page:
                return items
            start += len(page)

    def stock_count(self, market: str) -> int:
        return sum(item.category in {"a_share", "b_share"} for item in self.all(market))

    def a_share_count(self, market: str) -> int:
        return sum(item.category == "a_share" for item in self.all(market))

    def stocks(self, market: str):
        return [item for item in self.all(market) if item.category in {"a_share", "b_share"}]

    def a_shares(self, market: str):
        return [item for item in self.all(market) if item.category == "a_share"]

    def etfs(self, market: str):
        return [item for item in self.all(market) if item.category == "etf"]

    def indices(self, market: str):
        return [item for item in self.all(market) if item.category == "index"]

    def all_markets(self) -> builtins.list:
        items = []
        for market in ("sh", "sz", "bj"):
            items.extend(self.all(market))
        return items

    def all_stocks(self) -> builtins.list[str]:
        return [item.full_code for item in self.all_markets() if item.category in {"a_share", "b_share"}]

    def all_a_shares(self) -> builtins.list[str]:
        return [item.full_code for item in self.all_markets() if item.category == "a_share"]

    def all_etfs(self) -> builtins.list[str]:
        return [item.full_code for item in self.all_markets() if item.category == "etf"]

    def all_indices(self) -> builtins.list[str]:
        return [item.full_code for item in self.all_markets() if item.category == "index"]

    def latest_stock_list(self, market: str | None = None):
        """Return the current A-share security rows from the 0x044d table."""
        if market is None:
            return [item for item in self.all_markets() if item.category == "a_share"]
        return [item for item in self.a_shares(market)]

    def latest_st(self, market: str | None = None):
        """Return the latest ST/*ST list using the current code-table names."""
        rows = self.latest_stock_list(market)
        return [item for item in rows if _is_st_name(getattr(item, "name", ""))]

    def st(self, market: str | None = None):
        """Short alias for :meth:`latest_st`."""
        return self.latest_st(market)

    def latest_stocks(self, market: str | None = None):
        """Alias for :meth:`latest_stock_list` used by data-oriented callers."""
        return self.latest_stock_list(market)

    def latest_suspended(self, market: str | None = None):
        """Return stocks whose native 0x053e trading status has bit 0x20 set."""
        rows = self.latest_stock_list(market)
        if not rows:
            return []
        result = []
        for start in range(0, len(rows), 80):
            quotes = self._execute(
                "legacy_quotes",
                codes=[item.full_code for item in rows[start : start + 80]],
            )
            for quote in quotes or ():
                if int(getattr(quote, "trading_status_raw", 0)) & 0x20:
                    result.append(quote.full_code)
        return result

    def suspended(self, market: str | None = None):
        """Short alias for :meth:`latest_suspended`."""
        return self.latest_suspended(market)


def _validate_page_size(value: int) -> None:
    if value <= 0 or value > MAX_CODE_PAGE_SIZE:
        raise ValueError(f"page size must be between 1 and {MAX_CODE_PAGE_SIZE}")


def _validate_limit(value: int) -> None:
    if value < 0 or value > MAX_CODE_PAGE_SIZE:
        raise ValueError(f"limit must be between 0 and {MAX_CODE_PAGE_SIZE}")


def _is_st_name(name: str) -> bool:
    text = str(name or "").strip().upper()
    return text.startswith(("ST", "*ST", "SST", "S*ST"))
