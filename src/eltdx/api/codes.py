"""Security code table API."""

from __future__ import annotations

from eltdx.protocol.constants import DEFAULT_CODE_PAGE_SIZE

from .base import ApiBase


class CodeApi(ApiBase):
    def count(self, market: str):
        return self._execute("security_count", market=market)

    def list(self, market: str, *, start: int = 0, limit: int = 1600):
        return self._execute("security_list", market=market, start=start, limit=limit)

    def all(self, market: str, *, page_size: int = DEFAULT_CODE_PAGE_SIZE):
        start = 0
        items = []
        while True:
            page = self.list(market, start=start, limit=page_size)
            items.extend(page)
            if len(page) < page_size:
                return items
            start += page_size

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

    def all_markets(self) -> list:
        items = []
        for market in ("sh", "sz", "bj"):
            items.extend(self.all(market))
        return items

    def all_stocks(self) -> list[str]:
        return [item.full_code for item in self.all_markets() if item.category in {"a_share", "b_share"}]

    def all_a_shares(self) -> list[str]:
        return [item.full_code for item in self.all_markets() if item.category == "a_share"]

    def all_etfs(self) -> list[str]:
        return [item.full_code for item in self.all_markets() if item.category == "etf"]

    def all_indices(self) -> list[str]:
        return [item.full_code for item in self.all_markets() if item.category == "index"]
