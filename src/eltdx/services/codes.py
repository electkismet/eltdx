from __future__ import annotations

from dataclasses import dataclass, field

from ..client import TdxClient
from ..models import CodePage, SecurityCode
from ..protocol.unit import add_prefix, is_etf_entry, is_index, is_stock_entry


@dataclass(slots=True)
class CodesService:
    client: TdxClient
    _items: list[SecurityCode] = field(default_factory=list, init=False, repr=False)
    _by_full_code: dict[str, SecurityCode] = field(default_factory=dict, init=False, repr=False)
    _loaded: bool = field(default=False, init=False, repr=False)

    def get_page(self, exchange: str, *, start: int = 0, limit: int | None = 1000) -> CodePage:
        return self.client.get_codes(exchange, start=start, limit=limit)

    def get_all(self, exchange: str) -> list[SecurityCode]:
        return self.client.get_codes_all(exchange)

    def refresh(self) -> int:
        items: list[SecurityCode] = []
        for exchange in ("sh", "sz", "bj"):
            items.extend(self.client.get_codes_all(exchange))

        self._items = items
        self._by_full_code = {item.full_code: item for item in items}
        self._loaded = True
        return len(items)

    def all(self) -> list[SecurityCode]:
        self._ensure_loaded()
        return list(self._items)

    def by_exchange(self, exchange: str) -> list[SecurityCode]:
        self._ensure_loaded()
        normalized = str(exchange).strip().lower()
        return [item for item in self._items if item.exchange == normalized]

    def get(self, code: str) -> SecurityCode | None:
        self._ensure_loaded()
        return self._by_full_code.get(add_prefix(code))

    def get_name(self, code: str) -> str | None:
        item = self.get(code)
        return None if item is None else item.name

    def stocks(self) -> list[SecurityCode]:
        self._ensure_loaded()
        return [item for item in self._items if is_stock_entry(item.full_code)]

    def etfs(self) -> list[SecurityCode]:
        self._ensure_loaded()
        return [item for item in self._items if is_etf_entry(item.full_code, item.name)]

    def indexes(self) -> list[SecurityCode]:
        self._ensure_loaded()
        return [item for item in self._items if is_index(item.full_code)]

    def get_stocks(self) -> list[str]:
        return [item.full_code for item in self.stocks()]

    def get_etfs(self) -> list[str]:
        return [item.full_code for item in self.etfs()]

    def get_indexes(self) -> list[str]:
        return [item.full_code for item in self.indexes()]

    def _ensure_loaded(self) -> None:
        if not self._loaded:
            self.refresh()
