"""MCP SDK 2 server entry for eltdx."""

from __future__ import annotations

import math
from collections import OrderedDict
from collections.abc import Callable, Iterator, Sequence
from contextlib import asynccontextmanager, contextmanager
from dataclasses import dataclass
from pathlib import Path
from threading import Condition, RLock
from typing import Any, TypeVar

from . import __version__
from .client import TdxClient
from .f10 import F10Client
from .hosts import normalize_host
from .serialization import to_jsonable

_MAX_CODES = 200
_MAX_DEPTH_CODES = 100
_MAX_KLINE_COUNT = 800
_MAX_TRADE_COUNT = 2000
_MAX_CLIENTS = 16
_MCP_POOL_SIZE = 4

_DOC_PATHS = {
    "overview": "index.md",
    "api": "API_REFERENCE.md",
    "methods": "METHOD_REFERENCE.md",
    "fields": "FIELD_REFERENCE.md",
    "commands": "COMMANDS_7709.md",
    "f10": "F10_7615.md",
    "helpers": "helpers/README.md",
    "mcp": "MCP.md",
}

_T = TypeVar("_T")


def quote(codes: str | Sequence[str], *, timeout: float = 8.0, host: str | None = None) -> Any:
    """Query quote snapshots."""

    code_list = _validate_codes(codes)
    return _call_once(lambda client: client.quotes.get_snapshots(code_list), timeout=timeout, host=host)


def quote_depth(codes: str | Sequence[str], *, timeout: float = 8.0, host: str | None = None) -> Any:
    """Query five-level quote depth."""

    code_list = _validate_codes(codes, maximum=_MAX_DEPTH_CODES)
    return _call_once(lambda client: client.quotes.get_depth(code_list), timeout=timeout, host=host)


def kline(
    code: str,
    *,
    period: str = "day",
    count: int = 120,
    start: int = 0,
    adjust: str | None = None,
    anchor_date: str | int | None = None,
    include_raw: bool = False,
    timeout: float = 8.0,
    host: str | None = None,
) -> Any:
    """Query one bounded K-line page."""

    count = _bounded_int("count", count, minimum=1, maximum=_MAX_KLINE_COUNT)
    start = _bounded_int("start", start, minimum=0, maximum=0xFFFF)
    return _call_once(
        lambda client: client.bars.get(
            code,
            period=period,
            start=start,
            count=count,
            adjust=adjust,
            anchor_date=anchor_date,
            include_raw=include_raw,
        ),
        timeout=timeout,
        host=host,
    )


def minute(
    code: str,
    *,
    trading_date: str | int | None = None,
    include_raw: bool = False,
    timeout: float = 8.0,
    host: str | None = None,
) -> Any:
    """Query today's or one historical day's minute series."""

    return _call_once(
        lambda client: client.minutes.today(code, include_raw=include_raw)
        if trading_date is None
        else client.minutes.history(code, trading_date, include_raw=include_raw),
        timeout=timeout,
        host=host,
    )


def trades(
    code: str,
    *,
    trading_date: str | int | None = None,
    start: int = 0,
    count: int = 500,
    include_raw: bool = False,
    timeout: float = 8.0,
    host: str | None = None,
) -> Any:
    """Query one bounded page of current or historical trades."""

    start = _bounded_int("start", start, minimum=0, maximum=0xFFFF)
    count = _bounded_int("count", count, minimum=1, maximum=_MAX_TRADE_COUNT)
    return _call_once(
        lambda client: client.trades.today(code, start=start, count=count, include_raw=include_raw)
        if trading_date is None
        else client.trades.history(
            code,
            trading_date,
            start=start,
            count=count,
            include_raw=include_raw,
        ),
        timeout=timeout,
        host=host,
    )


def call_auction(
    code: str,
    *,
    include_raw: bool = False,
    timeout: float = 8.0,
    host: str | None = None,
) -> Any:
    """Query the current call-auction series."""

    return _call_once(
        lambda client: client.auctions.series(code, include_raw=include_raw),
        timeout=timeout,
        host=host,
    )


def auction_0925(
    code: str,
    trading_date: str | int,
    *,
    timeout: float = 8.0,
    host: str | None = None,
    max_pages: int | None = 100,
) -> Any:
    """Query the 09:25 final tick from historical trade details."""

    max_pages = _optional_bounded_int("max_pages", max_pages, minimum=1, maximum=100)
    return _call_once(
        lambda client: client.helpers.auction_0925(code, trading_date, max_pages=max_pages),
        timeout=timeout,
        host=host,
    )


def auction_data(
    code: str,
    *,
    trading_date: str | int | None = None,
    include_series: bool = True,
    include_snapshot: bool = True,
    include_quote: bool = True,
    pre_close_price: float | None = None,
    timeout: float = 8.0,
    host: str | None = None,
) -> Any:
    """Return a combined current or historical auction view."""

    return _call_once(
        lambda client: client.helpers.auction_data(
            code,
            trading_date,
            include_series=include_series,
            include_snapshot=include_snapshot,
            include_quote=include_quote,
            pre_close_price=pre_close_price,
        ),
        timeout=timeout,
        host=host,
    )


def stock_profile(
    codes: str | Sequence[str],
    *,
    include_security: bool = True,
    include_finance: bool = True,
    timeout: float = 8.0,
    host: str | None = None,
) -> Any:
    """Return quote, code-table and finance fields in one table."""

    code_list = _validate_codes(codes)
    return _call_once(
        lambda client: client.helpers.stock_profile_table(
            code_list,
            include_security=include_security,
            include_finance=include_finance,
        ),
        timeout=timeout,
        host=host,
    )


def shortline_indicators(
    codes: str | Sequence[str],
    *,
    stats_path: str = "zhb.zip",
    refresh_stats: bool = False,
    timeout: float = 8.0,
    host: str | None = None,
) -> Any:
    """Return the 21 trading-date-safe shortline indicator fields."""

    code_list = _validate_codes(codes)
    return _call_once(
        lambda client: client.helpers.shortline_indicators(
            code_list,
            stats_path=stats_path,
            refresh_stats=refresh_stats,
        ),
        timeout=timeout,
        host=host,
    )


def stock_topics(code: str, *, timeout: float = 8.0) -> Any:
    """Query all known topics for one stock."""

    return _call_once(
        lambda client: client.helpers.stock_topics(code),
        timeout=timeout,
        host=None,
        connect=False,
    )


def topic_stocks(
    seed_code: str,
    *,
    topic_id: str | int | None = None,
    topic_name: str | None = None,
    sort_by: str = "zdf",
    section: str = "gndbzfsj",
    timeout: float = 8.0,
) -> Any:
    """Query stocks inside one topic."""

    return _call_once(
        lambda client: client.helpers.topic_stocks(
            seed_code,
            topic_id=topic_id,
            topic_name=topic_name,
            sort_by=sort_by,
            section=section,
        ),
        timeout=timeout,
        host=None,
        connect=False,
    )


def company_profile(code: str, *, section: str = "8", timeout: float = 8.0) -> Any:
    """Query an F10 company-profile section."""

    return _call_once(
        lambda client: client.f10.company_profile(code, section=section),
        timeout=timeout,
        host=None,
        connect=False,
    )


def hot_topics(code: str, *, section: str = "zttzbkz", timeout: float = 8.0) -> Any:
    """Query F10 hot-topic detail rows."""

    return _call_once(
        lambda client: client.f10.hot_topics(code, section=section),
        timeout=timeout,
        host=None,
        connect=False,
    )


def finance_report(code: str, *, report_type: str = "zcfzb", timeout: float = 8.0) -> Any:
    """Query an F10 finance report."""

    return _call_once(
        lambda client: client.f10.finance_report(code, report_type=report_type),
        timeout=timeout,
        host=None,
        connect=False,
    )


def company_news(
    code: str,
    *,
    section: str = "gsyj",
    keyword: str = "",
    rating: str | int = "0",
    page: int = 1,
    page_size: int = 20,
    timeout: float = 8.0,
) -> Any:
    """Query a bounded page of F10 company news or research."""

    page = _bounded_int("page", page, minimum=1, maximum=10_000)
    page_size = _bounded_int("page_size", page_size, minimum=1, maximum=100)
    return _call_once(
        lambda client: client.f10.company_news(
            code,
            section=section,
            keyword=keyword,
            rating=rating,
            page=page,
            page_size=page_size,
        ),
        timeout=timeout,
        host=None,
        connect=False,
    )


def docs_index() -> dict[str, str]:
    """Return documentation resource URIs available through MCP."""

    return {
        "overview": "eltdx://docs/overview",
        "API": "eltdx://docs/api",
        "methods": "eltdx://docs/methods",
        "fields": "eltdx://docs/fields",
        "7709_commands": "eltdx://docs/commands",
        "F10": "eltdx://docs/f10",
        "helpers": "eltdx://docs/helpers",
        "MCP": "eltdx://docs/mcp",
    }


@dataclass(slots=True)
class _ClientEntry:
    client: TdxClient
    active_calls: int = 0
    connecting: bool = False
    retiring: bool = False
    failed: bool = False


class _ClientRegistry:
    """Own and reuse TdxClient instances for one MCP server lifespan."""

    def __init__(self) -> None:
        self._clients: OrderedDict[tuple[str | None, float], _ClientEntry] = OrderedDict()
        self._pending_keys: set[tuple[str | None, float]] = set()
        self._lock = RLock()
        self._condition = Condition(self._lock)
        self._closing = False
        self._close_in_progress = False
        self._closed = False

    @contextmanager
    def use(self, *, timeout: float, host: str | None) -> Iterator[TdxClient]:
        key, entry = self._acquire(timeout=timeout, host=host)
        try:
            yield entry.client
        finally:
            self._release(key, entry)

    def _acquire(self, *, timeout: float, host: str | None) -> tuple[tuple[str | None, float], _ClientEntry]:
        timeout = _validate_timeout(timeout)
        host = _normalize_host(host)
        key = (host, timeout)
        owns_key = False

        while True:
            victim: tuple[tuple[str | None, float], _ClientEntry] | None = None
            owner: _ClientEntry | None = None

            with self._condition:
                if self._closing or self._closed:
                    if owns_key:
                        self._pending_keys.discard(key)
                        self._condition.notify_all()
                    raise RuntimeError("the eltdx MCP client registry is closing")

                entry = self._clients.get(key)
                if entry is not None:
                    if entry.connecting or entry.retiring:
                        self._condition.wait()
                        continue
                    if entry.failed:
                        entry.retiring = True
                        victim = (key, entry)
                    else:
                        entry.active_calls += 1
                        self._clients.move_to_end(key)
                        return key, entry
                elif key in self._pending_keys and not owns_key:
                    self._condition.wait()
                    continue
                else:
                    if not owns_key:
                        self._pending_keys.add(key)
                        owns_key = True

                if entry is None and len(self._clients) < _MAX_CLIENTS:
                    try:
                        client = TdxClient(
                            host=host,
                            timeout=timeout,
                            pool_size=_MCP_POOL_SIZE,
                            heartbeat_interval=None,
                        )
                    except BaseException:
                        self._pending_keys.discard(key)
                        owns_key = False
                        self._condition.notify_all()
                        raise
                    owner = _ClientEntry(
                        client=client,
                        active_calls=1,
                        connecting=True,
                    )
                    self._clients[key] = owner
                elif entry is None:
                    for victim_key, candidate in self._clients.items():
                        if candidate.active_calls == 0 and not candidate.connecting and not candidate.retiring:
                            candidate.retiring = True
                            victim = (victim_key, candidate)
                            break
                    if victim is None:
                        self._pending_keys.discard(key)
                        owns_key = False
                        self._condition.notify_all()
                        raise RuntimeError(
                            f"all {_MAX_CLIENTS} eltdx MCP market-data clients are currently in use"
                        )

            if victim is not None:
                victim_key, victim_entry = victim
                try:
                    victim_entry.client.close()
                except BaseException:
                    with self._condition:
                        victim_entry.retiring = False
                        if owns_key:
                            self._pending_keys.discard(key)
                            owns_key = False
                        self._condition.notify_all()
                    raise
                with self._condition:
                    if self._clients.get(victim_key) is victim_entry:
                        self._clients.pop(victim_key)
                    self._condition.notify_all()
                continue

            assert owner is not None
            try:
                owner.client.connect()
            except BaseException as connect_error:
                try:
                    owner.client.close()
                except BaseException as close_error:
                    with self._condition:
                        owner.active_calls = 0
                        owner.connecting = False
                        owner.failed = True
                        self._pending_keys.discard(key)
                        owns_key = False
                        self._condition.notify_all()
                    raise RuntimeError(
                        "eltdx MCP market-data client connection and cleanup both failed"
                    ) from close_error
                with self._condition:
                    if self._clients.get(key) is owner:
                        self._clients.pop(key)
                    owner.active_calls = 0
                    owner.connecting = False
                    self._pending_keys.discard(key)
                    owns_key = False
                    self._condition.notify_all()
                raise connect_error

            with self._condition:
                owner.connecting = False
                self._pending_keys.discard(key)
                owns_key = False
                if self._closing:
                    owner.active_calls = 0
                    self._condition.notify_all()
                    raise RuntimeError("the eltdx MCP client registry is closing")
                self._condition.notify_all()
                return key, owner

    def _release(self, key: tuple[str | None, float], entry: _ClientEntry) -> None:
        with self._condition:
            if self._clients.get(key) is entry:
                entry.active_calls -= 1
                if entry.active_calls < 0:  # pragma: no cover - internal invariant
                    raise RuntimeError("eltdx MCP client lease count became negative")
                self._clients.move_to_end(key)
            self._condition.notify_all()

    def close(self) -> None:
        with self._condition:
            self._closing = True
            while True:
                if self._closed:
                    return
                if self._close_in_progress or any(
                    entry.active_calls or entry.connecting or entry.retiring
                    for entry in self._clients.values()
                ):
                    self._condition.wait()
                    continue
                self._close_in_progress = True
                clients = list(self._clients.items())
                break

        failures: list[BaseException] = []
        closed: list[tuple[tuple[str | None, float], _ClientEntry]] = []
        for item in clients:
            _, entry = item
            try:
                entry.client.close()
            except BaseException as exc:  # pragma: no cover - defensive shutdown path
                failures.append(exc)
            else:
                closed.append(item)

        with self._condition:
            for key, entry in closed:
                if self._clients.get(key) is entry:
                    self._clients.pop(key)
            self._close_in_progress = False
            self._closed = not self._clients
            self._condition.notify_all()

        if failures:
            raise RuntimeError(f"failed to close {len(failures)} eltdx MCP client(s)") from failures[0]


class _McpTools:
    """MCP-facing tools backed by a shared client registry."""

    def __init__(self, clients: _ClientRegistry) -> None:
        self._clients = clients

    def quote(
        self,
        codes: str | list[str],
        timeout: float = 8.0,
        host: str | None = None,
    ) -> list[dict[str, Any]]:
        """Query quote snapshots for up to 200 securities."""

        code_list = _validate_codes(codes)
        with self._clients.use(timeout=timeout, host=host) as client:
            return _json(client.quotes.get_snapshots(code_list))

    def quote_depth(
        self,
        codes: str | list[str],
        timeout: float = 8.0,
        host: str | None = None,
    ) -> dict[str, Any]:
        """Query native five-level quotes for up to 100 securities."""

        code_list = _validate_codes(codes, maximum=_MAX_DEPTH_CODES)
        with self._clients.use(timeout=timeout, host=host) as client:
            return _json(client.quotes.get_depth(code_list))

    def kline(
        self,
        code: str,
        period: str = "day",
        count: int = 120,
        start: int = 0,
        adjust: str | None = None,
        anchor_date: str | int | None = None,
        include_raw: bool = False,
        timeout: float = 8.0,
        host: str | None = None,
    ) -> dict[str, Any]:
        """Query one K-line page; count is limited to 800 bars."""

        count = _bounded_int("count", count, minimum=1, maximum=_MAX_KLINE_COUNT)
        start = _bounded_int("start", start, minimum=0, maximum=0xFFFF)
        with self._clients.use(timeout=timeout, host=host) as client:
            return _json(
                client.bars.get(
                    code,
                    period=period,
                    start=start,
                    count=count,
                    adjust=adjust,
                    anchor_date=anchor_date,
                    include_raw=include_raw,
                )
            )

    def minute(
        self,
        code: str,
        trading_date: str | int | None = None,
        include_raw: bool = False,
        timeout: float = 8.0,
        host: str | None = None,
    ) -> dict[str, Any]:
        """Query today's or one historical day's minute series."""

        with self._clients.use(timeout=timeout, host=host) as client:
            return _json(
                client.minutes.today(code, include_raw=include_raw)
                if trading_date is None
                else client.minutes.history(code, trading_date, include_raw=include_raw)
            )

    def trades(
        self,
        code: str,
        trading_date: str | int | None = None,
        start: int = 0,
        count: int = 500,
        include_raw: bool = False,
        timeout: float = 8.0,
        host: str | None = None,
    ) -> dict[str, Any]:
        """Query one trade page; count is limited to 2000 ticks."""

        start = _bounded_int("start", start, minimum=0, maximum=0xFFFF)
        count = _bounded_int("count", count, minimum=1, maximum=_MAX_TRADE_COUNT)
        with self._clients.use(timeout=timeout, host=host) as client:
            return _json(
                client.trades.today(code, start=start, count=count, include_raw=include_raw)
                if trading_date is None
                else client.trades.history(code, trading_date, start=start, count=count, include_raw=include_raw)
            )

    def call_auction(
        self,
        code: str,
        include_raw: bool = False,
        timeout: float = 8.0,
        host: str | None = None,
    ) -> dict[str, Any]:
        """Query the current call-auction series."""

        with self._clients.use(timeout=timeout, host=host) as client:
            return _json(client.auctions.series(code, include_raw=include_raw))

    def auction_0925(
        self,
        code: str,
        trading_date: str | int,
        timeout: float = 8.0,
        host: str | None = None,
        max_pages: int | None = 100,
    ) -> dict[str, Any]:
        """Query the 09:25 final tick for one historical trading date."""

        max_pages = _optional_bounded_int("max_pages", max_pages, minimum=1, maximum=100)
        with self._clients.use(timeout=timeout, host=host) as client:
            return _json(client.helpers.auction_0925(code, trading_date, max_pages=max_pages))

    def auction_data(
        self,
        code: str,
        trading_date: str | int | None = None,
        include_series: bool = True,
        include_snapshot: bool = True,
        include_quote: bool = True,
        pre_close_price: float | None = None,
        timeout: float = 8.0,
        host: str | None = None,
    ) -> dict[str, Any]:
        """Return a combined current or historical auction view."""

        with self._clients.use(timeout=timeout, host=host) as client:
            return _json(
                client.helpers.auction_data(
                    code,
                    trading_date,
                    include_series=include_series,
                    include_snapshot=include_snapshot,
                    include_quote=include_quote,
                    pre_close_price=pre_close_price,
                )
            )

    def stock_profile(
        self,
        codes: str | list[str],
        include_security: bool = True,
        include_finance: bool = True,
        timeout: float = 8.0,
        host: str | None = None,
    ) -> dict[str, Any]:
        """Return quote, code-table and finance fields in one table."""

        code_list = _validate_codes(codes)
        with self._clients.use(timeout=timeout, host=host) as client:
            return _json(
                client.helpers.stock_profile_table(
                    code_list,
                    include_security=include_security,
                    include_finance=include_finance,
                )
            )

    def shortline_indicators(
        self,
        codes: str | list[str],
        stats_path: str = "zhb.zip",
        refresh_stats: bool = False,
        timeout: float = 8.0,
        host: str | None = None,
    ) -> dict[str, Any]:
        """Return the 21 trading-date-safe shortline indicator fields."""

        code_list = _validate_codes(codes)
        with self._clients.use(timeout=timeout, host=host) as client:
            return _json(
                client.helpers.shortline_indicators(
                    code_list,
                    stats_path=stats_path,
                    refresh_stats=refresh_stats,
                )
            )

    def stock_topics(self, code: str, timeout: float = 8.0) -> dict[str, Any]:
        """Query all known topics for one stock."""

        return _call_without_market(lambda client: client.helpers.stock_topics(code), timeout=timeout)

    def topic_stocks(
        self,
        seed_code: str,
        topic_id: str | int | None = None,
        topic_name: str | None = None,
        sort_by: str = "zdf",
        section: str = "gndbzfsj",
        timeout: float = 8.0,
    ) -> dict[str, Any]:
        """Query stocks inside one topic."""

        return _call_without_market(
            lambda client: client.helpers.topic_stocks(
                seed_code,
                topic_id=topic_id,
                topic_name=topic_name,
                sort_by=sort_by,
                section=section,
            ),
            timeout=timeout,
        )

    def company_profile(self, code: str, section: str = "8", timeout: float = 8.0) -> dict[str, Any]:
        """Query an F10 company-profile section."""

        return _call_f10(lambda client: client.company_profile(code, section=section), timeout=timeout)

    def hot_topics(self, code: str, section: str = "zttzbkz", timeout: float = 8.0) -> dict[str, Any]:
        """Query F10 hot-topic detail rows."""

        return _call_f10(lambda client: client.hot_topics(code, section=section), timeout=timeout)

    def finance_report(self, code: str, report_type: str = "zcfzb", timeout: float = 8.0) -> dict[str, Any]:
        """Query an F10 finance report."""

        return _call_f10(lambda client: client.finance_report(code, report_type=report_type), timeout=timeout)

    def company_news(
        self,
        code: str,
        section: str = "gsyj",
        keyword: str = "",
        rating: str | int = "0",
        page: int = 1,
        page_size: int = 20,
        timeout: float = 8.0,
    ) -> dict[str, Any]:
        """Query a bounded page of F10 company news or research."""

        page = _bounded_int("page", page, minimum=1, maximum=10_000)
        page_size = _bounded_int("page_size", page_size, minimum=1, maximum=100)
        return _call_f10(
            lambda client: client.company_news(
                code,
                section=section,
                keyword=keyword,
                rating=rating,
                page=page,
                page_size=page_size,
            ),
            timeout=timeout,
        )

    def docs_index(self) -> dict[str, str]:
        """Return MCP resource URIs for the main eltdx documents."""

        return docs_index()


def create_mcp_server():
    """Create the MCP SDK 2 server."""

    try:
        from mcp.server import MCPServer
    except ImportError as exc:  # pragma: no cover - depends on optional package install
        raise RuntimeError(
            "MCP support requires MCP Python SDK 2. Install with: pip install 'eltdx[mcp]'"
        ) from exc

    clients = _ClientRegistry()
    tools = _McpTools(clients)

    @asynccontextmanager
    async def lifespan(_server):
        try:
            yield clients
        finally:
            clients.close()

    server = MCPServer(
        "eltdx",
        title="eltdx A-share data",
        description="TongdaXin 7709 market data and 7615 F10 tools.",
        instructions=(
            "Use full stock codes such as sz000001 or sh600000. "
            "Prefer bounded page tools and request only the data needed."
        ),
        version=__version__,
        lifespan=lifespan,
    )

    registrations = (
        ("eltdx_quote", tools.quote),
        ("eltdx_quote_depth", tools.quote_depth),
        ("eltdx_kline", tools.kline),
        ("eltdx_minute", tools.minute),
        ("eltdx_trades", tools.trades),
        ("eltdx_call_auction", tools.call_auction),
        ("eltdx_auction_0925", tools.auction_0925),
        ("eltdx_auction_data", tools.auction_data),
        ("eltdx_stock_profile", tools.stock_profile),
        ("eltdx_shortline_indicators", tools.shortline_indicators),
        ("eltdx_stock_topics", tools.stock_topics),
        ("eltdx_topic_stocks", tools.topic_stocks),
        ("eltdx_company_profile", tools.company_profile),
        ("eltdx_hot_topics", tools.hot_topics),
        ("eltdx_finance_report", tools.finance_report),
        ("eltdx_company_news", tools.company_news),
        ("eltdx_docs_index", tools.docs_index),
    )
    for name, function in registrations:
        server.tool(name=name)(function)

    for name, relative_path in _DOC_PATHS.items():
        server.resource(
            f"eltdx://docs/{name}",
            name=f"eltdx_docs_{name}",
            title=f"eltdx {name} documentation",
            description=f"Bundled eltdx {name} documentation.",
            mime_type="text/markdown",
        )(_doc_reader(relative_path))

    return server


def main() -> int:
    """Run the MCP server over stdio."""

    create_mcp_server().run("stdio")
    return 0


def _call_once(
    operation: Callable[[TdxClient], _T],
    *,
    timeout: float,
    host: str | None,
    connect: bool = True,
) -> Any:
    client = _client(timeout=timeout, host=host)
    try:
        if connect:
            client.connect()
        return _json(operation(client))
    finally:
        client.close()


def _call_without_market(operation: Callable[[TdxClient], _T], *, timeout: float) -> Any:
    client = _client(timeout=timeout, host=None)
    try:
        return _json(operation(client))
    finally:
        client.close()


def _call_f10(operation: Callable[[F10Client], _T], *, timeout: float) -> Any:
    return _json(operation(F10Client(timeout=_validate_timeout(timeout))))


def _client(*, timeout: float, host: str | None) -> TdxClient:
    return TdxClient(
        host=_normalize_host(host),
        timeout=_validate_timeout(timeout),
        heartbeat_interval=None,
    )


def _validate_codes(codes: str | Sequence[str], *, maximum: int = _MAX_CODES) -> list[str]:
    if isinstance(codes, str):
        code_list = [codes]
    elif isinstance(codes, Sequence):
        code_list = list(codes)
    else:
        raise TypeError("codes must be a string or a sequence of strings")

    if not code_list:
        raise ValueError("codes must not be empty")
    if len(code_list) > maximum:
        raise ValueError(f"codes accepts at most {maximum} securities per call")
    if any(not isinstance(code, str) or not code.strip() for code in code_list):
        raise ValueError("each security code must be a non-empty string")
    return [code.strip() for code in code_list]


def _validate_timeout(timeout: float) -> float:
    if isinstance(timeout, bool) or not isinstance(timeout, (int, float)):
        raise TypeError("timeout must be a number")
    timeout = float(timeout)
    if not math.isfinite(timeout) or timeout <= 0 or timeout > 120:
        raise ValueError("timeout must be greater than 0 and no more than 120 seconds")
    return timeout


def _normalize_host(host: str | None) -> str | None:
    if host is None:
        return None
    if not isinstance(host, str):
        raise TypeError("host must be a string or None")
    host = host.strip()
    if not host:
        raise ValueError("host must not be empty")
    normalized = normalize_host(host)
    if normalized is None:
        raise ValueError("host must use host:port with a port between 1 and 65535")
    return normalized


def _bounded_int(name: str, value: int, *, minimum: int, maximum: int) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise TypeError(f"{name} must be an integer")
    if value < minimum or value > maximum:
        raise ValueError(f"{name} must be between {minimum} and {maximum}")
    return value


def _optional_bounded_int(
    name: str,
    value: int | None,
    *,
    minimum: int,
    maximum: int,
) -> int | None:
    if value is None:
        return None
    return _bounded_int(name, value, minimum=minimum, maximum=maximum)


def _doc_reader(relative_path: str) -> Callable[[], str]:
    def read_document() -> str:
        return (_docs_root() / relative_path).read_text(encoding="utf-8")

    read_document.__name__ = f"read_{relative_path.replace('/', '_').replace('.', '_')}"
    return read_document


def _docs_root() -> Path:
    bundled = Path(__file__).resolve().with_name("docs")
    if bundled.is_dir():
        return bundled
    source = Path(__file__).resolve().parents[2] / "docs"
    if source.is_dir():
        return source
    raise FileNotFoundError("the bundled eltdx documentation directory is missing")


def _json(value: Any) -> Any:
    return to_jsonable(value)


if __name__ == "__main__":
    raise SystemExit(main())
