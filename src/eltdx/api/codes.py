"""Security code table API."""

from __future__ import annotations

import builtins

from eltdx.models.security import SecurityCode
from eltdx.protocol.commands import command_code
from eltdx.protocol.constants import DEFAULT_CODE_PAGE_SIZE, MAX_CODE_PAGE_SIZE

from .base import ApiBase


class CodeApi(ApiBase):
    def count(self, market: str):
        return self._query("security_count", market=market)

    def list(self, market: str, *, start: int = 0, limit: int = 1600):
        _validate_limit(limit)
        result = self._query("security_list", market=market, start=start, limit=limit)
        sink = self._metadata_sink() if self._metadata_sink is not None else None
        if sink is not None:
            sink(result)
        return result

    def all(self, market: str, *, page_size: int = DEFAULT_CODE_PAGE_SIZE):
        _validate_page_size(page_size)
        if (
            self._native_services
            and isinstance(market, str)
            and isinstance(page_size, int)
            and not isinstance(page_size, bool)
        ):
            items = []
            for page in self._transport.sdk_security_pages(market, page_size):
                if not isinstance(page, (list, tuple)):
                    return items
                sink = self._metadata_sink() if self._metadata_sink is not None else None
                if sink is not None:
                    sink(page)
                items.extend(page)
                if not page:
                    return items
            return items
        start = 0
        items = []
        while True:
            page = self.list(market, start=start, limit=page_size)
            # Native parsing returns a list of ``SecurityRow`` values.  Keep
            # pagination finite when a diagnostic/in-memory transport returns
            # an envelope or another non-list payload instead.
            if not isinstance(page, (list, tuple)):
                return items
            items.extend(page)
            if not page:
                return items
            start += len(page)

    def stock_count(self, market: str) -> int:
        rows = self.all(market)
        selected = self._native_selection(rows, "stocks")
        if selected is not None:
            return len(selected)
        return sum(item.category in {"a_share", "b_share"} for item in rows)

    def a_share_count(self, market: str) -> int:
        rows = self.all(market)
        selected = self._native_selection(rows, "a_shares")
        if selected is not None:
            return len(selected)
        return sum(item.category == "a_share" for item in rows)

    def stocks(self, market: str):
        return self._select(self.all(market), "stocks")

    def a_shares(self, market: str):
        return self._select(self.all(market), "a_shares")

    def etfs(self, market: str):
        return self._select(self.all(market), "etfs")

    def indices(self, market: str):
        return self._select(self.all(market), "indices")

    def all_markets(self) -> builtins.list:
        items = []
        for market in ("sh", "sz", "bj"):
            items.extend(self.all(market))
        return items

    def all_stocks(self) -> builtins.list[str]:
        rows = self.all_markets()
        selected = self._native_selection(rows, "stocks", identifiers=True)
        if selected is not None:
            return [item.full_code for item in selected]
        return [item.full_code for item in rows if item.category in {"a_share", "b_share"}]

    def all_a_shares(self) -> builtins.list[str]:
        rows = self.all_markets()
        selected = self._native_selection(rows, "a_shares", identifiers=True)
        if selected is not None:
            return [item.full_code for item in selected]
        return [item.full_code for item in rows if item.category == "a_share"]

    def all_etfs(self) -> builtins.list[str]:
        rows = self.all_markets()
        selected = self._native_selection(rows, "etfs", identifiers=True)
        if selected is not None:
            return [item.full_code for item in selected]
        return [item.full_code for item in rows if item.category == "etf"]

    def all_indices(self) -> builtins.list[str]:
        rows = self.all_markets()
        selected = self._native_selection(rows, "indices", identifiers=True)
        if selected is not None:
            return [item.full_code for item in selected]
        return [item.full_code for item in rows if item.category == "index"]

    def latest_stock_list(self, market: str | None = None):
        """Return the current A-share security rows from the 0x044d table."""
        if market is None:
            return self._select(self.all_markets(), "a_shares")
        return [item for item in self.a_shares(market)]

    def latest_st(self, market: str | None = None):
        """Return the latest ST/*ST list using the current code-table names."""
        rows = self.latest_stock_list(market)
        return self._select(rows, "st")

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

    def _query(self, name: str, **payload):
        if self._native_services:
            return self._transport.sdk_security_request(command_code(name), payload)
        return self._execute(name, **payload)

    def _native_selection(self, rows, selection: str, *, identifiers: bool = False):
        # Preserve arbitrary custom row attributes/equality/iteration semantics.
        # Native directory rows are the exact frozen dataclass with str fields.
        if self._native_services and type(rows) is builtins.list:
            if not rows:
                return []
            if builtins.all(type(row) is SecurityCode and type(getattr(row, "category", None)) is str
                            and type(getattr(row, "name", None)) is str
                            and (not identifiers or (type(getattr(row, "exchange", None)) is str
                                                     and type(getattr(row, "code", None)) is str))
                            for row in rows):
                indices = self._transport.sdk_select_codes(
                    [(row.category, row.name) for row in rows], selection,
                )
                return [rows[index] for index in indices]
        return None

    def _select(self, rows, selection: str):
        selected = self._native_selection(rows, selection)
        if selected is not None:
            return selected
        if selection == "stocks":
            return [item for item in rows if item.category in {"a_share", "b_share"}]
        if selection == "a_shares":
            return [item for item in rows if item.category == "a_share"]
        if selection == "etfs":
            return [item for item in rows if item.category == "etf"]
        if selection == "indices":
            return [item for item in rows if item.category == "index"]
        return [item for item in rows if _is_st_name(getattr(item, "name", ""))]


def _validate_page_size(value: int) -> None:
    if value <= 0 or value > MAX_CODE_PAGE_SIZE:
        raise ValueError(f"page size must be between 1 and {MAX_CODE_PAGE_SIZE}")


def _validate_limit(value: int) -> None:
    if value < 0 or value > MAX_CODE_PAGE_SIZE:
        raise ValueError(f"limit must be between 0 and {MAX_CODE_PAGE_SIZE}")


def _is_st_name(name: str) -> bool:
    text = str(name or "").strip().upper()
    return text.startswith(("ST", "*ST", "SST", "S*ST"))
