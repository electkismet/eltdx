from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
from itertools import cycle
from threading import Lock

from .adjustment import apply_factors_to_kline, build_factor_response
from .bse import fetch_bj_codes
from .equity import compute_turnover, filter_equity_items, pick_equity
from .exceptions import ProtocolError
from .hosts import DEFAULT_HOSTS
from .models import CodePage, EquityResponse, FactorResponse, KlineResponse, SecurityCode, TradeResponse
from .protocol.constants import CODE_PAGE_SIZE, HISTORY_TRADE_PAGE_SIZE, KLINE_PAGE_SIZE, TRADE_PAGE_SIZE
from .protocol.unit import (
    is_a_share_entry,
    is_etf_entry,
    is_index,
    is_stock_entry,
    normalize_exchange,
    normalize_kline_period,
    normalize_trading_date,
)
from .trade_kline import build_trade_minute_kline
from .transport import TdxConnection


class TdxClient:
    def __init__(
        self,
        host: str | None = None,
        hosts: list[str] | tuple[str, ...] | None = None,
        *,
        timeout: float = 8.0,
        pool_size: int = 2,
        batch_size: int = 80,
    ) -> None:
        resolved_hosts = list(hosts or ([host] if host else DEFAULT_HOSTS))
        if not resolved_hosts:
            raise ValueError("at least one host is required")

        self._hosts = resolved_hosts
        self._timeout = timeout
        self._pool_size = max(1, pool_size)
        self._batch_size = min(80, max(1, batch_size))
        self._connections = [TdxConnection(self._hosts, timeout=self._timeout) for _ in range(self._pool_size)]
        self._executor: ThreadPoolExecutor | None = None
        self._round_robin = cycle(range(len(self._connections)))
        self._round_robin_lock = Lock()
        self._code_cache: dict[str, list[SecurityCode]] = {}
        self._connected = False

    def __enter__(self) -> TdxClient:
        self.connect()
        return self

    def __exit__(self, exc_type, exc, tb) -> None:
        self.close()

    def connect(self) -> None:
        if self._connected:
            return
        for connection in self._connections:
            connection.connect()
        self._executor = ThreadPoolExecutor(max_workers=len(self._connections), thread_name_prefix="eltdx-pool")
        self._connected = True

    def close(self) -> None:
        if self._executor is not None:
            self._executor.shutdown(wait=True)
            self._executor = None
        for connection in self._connections:
            connection.close()
        self._connected = False

    def get_count(self, exchange) -> int:
        resolved_exchange = normalize_exchange(exchange).value
        if resolved_exchange == "bj":
            return len(self._get_bj_codes())
        self.connect()
        return self._pick_connection().request_count(resolved_exchange)

    def get_codes(self, exchange, *, start: int = 0, limit: int | None = CODE_PAGE_SIZE) -> CodePage:
        total, items = self._collect_codes(exchange, start=start, limit=limit)
        return CodePage(exchange=normalize_exchange(exchange).value, start=start, count=len(items), total=total, items=items)

    def get_codes_all(self, exchange) -> list[SecurityCode]:
        _, items = self._collect_codes(exchange, start=0, limit=None)
        return items

    def get_stock_count(self, exchange) -> int:
        return sum(1 for item in self.get_codes_all(exchange) if is_stock_entry(item.full_code))

    def get_a_share_count(self, exchange) -> int:
        return sum(1 for item in self.get_codes_all(exchange) if is_a_share_entry(item.full_code))

    def get_stock_codes_all(self) -> list[str]:
        return [item.full_code for item in self._get_all_codes() if is_stock_entry(item.full_code)]

    def get_a_share_codes_all(self) -> list[str]:
        return [item.full_code for item in self._get_all_codes() if is_a_share_entry(item.full_code)]

    def get_etf_codes_all(self) -> list[str]:
        return [item.full_code for item in self._get_all_codes() if is_etf_entry(item.full_code, item.name)]

    def get_index_codes_all(self) -> list[str]:
        return [item.full_code for item in self._get_all_codes() if is_index(item.full_code)]

    def _collect_codes(self, exchange, *, start: int, limit: int | None) -> tuple[int, list[SecurityCode]]:
        if start < 0:
            raise ValueError("start must be >= 0")
        if limit is not None and limit < 0:
            raise ValueError("limit must be >= 0")

        resolved_exchange = normalize_exchange(exchange).value
        if resolved_exchange == "bj":
            items = self._get_bj_codes()
            total = len(items)
            if limit == 0:
                return total, []
            if start >= total:
                return total, []
            end = total if limit is None else min(start + limit, total)
            return total, items[start:end]

        self.connect()
        total = self.get_count(resolved_exchange)
        if limit == 0:
            return total, []
        if start >= total:
            return total, []

        remaining = total - start if limit is None else min(limit, total - start)
        current = start
        items: list[SecurityCode] = []

        while remaining > 0:
            page = self._pick_connection().request_codes(resolved_exchange, current)
            if not page:
                break

            take = min(len(page), remaining)
            items.extend(page[:take])
            current += len(page)
            remaining -= take

            if len(page) < CODE_PAGE_SIZE:
                break

        return total, items

    def _get_all_codes(self) -> list[SecurityCode]:
        items: list[SecurityCode] = []
        for exchange in ("sh", "sz", "bj"):
            items.extend(self.get_codes_all(exchange))
        return items

    def _get_bj_codes(self) -> list[SecurityCode]:
        cached = self._code_cache.get("bj")
        if cached is None:
            cached = fetch_bj_codes(timeout=self._timeout)
            self._code_cache["bj"] = cached
        return cached

    def get_gbbq(self, code: str, *, include_raw: bool = False):
        self.connect()
        return self._pick_connection().request_gbbq(code, include_raw=include_raw)

    def get_xdxr(self, code: str):
        self.connect()
        return self._pick_connection().request_xdxr(code)

    def get_equity_changes(self, code: str) -> EquityResponse:
        return filter_equity_items(self.get_gbbq(code).items)

    def get_equity(self, code: str, on=None):
        return pick_equity(self.get_equity_changes(code).items, on)

    def get_turnover(self, code: str, volume: int | float, *, on=None, unit: str = "hand") -> float:
        return compute_turnover(self.get_equity(code, on), volume, unit=unit)

    def get_factors(self, code: str) -> FactorResponse:
        day_kline = self.get_kline_all("day", code)
        xdxr_items = self.get_xdxr(code)
        return build_factor_response(day_kline, xdxr_items)

    def get_minute(self, code: str, date=None, *, include_raw: bool = False):
        self.connect()
        connection = self._pick_connection()
        if date is None:
            return connection.request_minute(code, include_raw=include_raw)
        return connection.request_history_minute(code, date, include_raw=include_raw)

    def get_history_minute(self, code: str, date, *, include_raw: bool = False):
        return self.get_minute(code, date, include_raw=include_raw)

    def get_kline(self, arg1, arg2, *, start: int = 0, count: int = KLINE_PAGE_SIZE, kind: str = "stock", include_raw: bool = False):
        if start < 0:
            raise ValueError("start must be >= 0")
        if count <= 0:
            raise ValueError("count must be > 0")
        if kind not in {"stock", "index"}:
            raise ValueError("kind must be 'stock' or 'index'")

        period, code = self._resolve_kline_args(arg1, arg2)
        page_count = min(KLINE_PAGE_SIZE, count)
        self.connect()
        return self._pick_connection().request_kline(period, code, start, page_count, kind=kind, include_raw=include_raw)

    def get_kline_all(self, arg1, arg2, *, kind: str = "stock") -> KlineResponse:
        if kind not in {"stock", "index"}:
            raise ValueError("kind must be 'stock' or 'index'")

        period, code = self._resolve_kline_args(arg1, arg2)
        self.connect()
        return self._collect_kline_pages(
            lambda start, count: self._pick_connection().request_kline(period, code, start, count, kind=kind),
            KLINE_PAGE_SIZE,
        )

    def get_adjusted_kline(self, period, code: str, *, adjust="qfq", start: int = 0, count: int = KLINE_PAGE_SIZE, include_raw: bool = False) -> KlineResponse:
        factors = self.get_factors(code)
        response = self.get_kline(period, code, start=start, count=count, include_raw=include_raw)
        return apply_factors_to_kline(response, factors, adjust)

    def get_adjusted_kline_all(self, period, code: str, *, adjust="qfq") -> KlineResponse:
        factors = self.get_factors(code)
        response = self.get_kline_all(period, code)
        return apply_factors_to_kline(response, factors, adjust)

    def get_trades(self, code: str, date=None, *, start: int = 0, count: int = TRADE_PAGE_SIZE, include_raw: bool = False):
        if start < 0:
            raise ValueError("start must be >= 0")
        if count <= 0:
            raise ValueError("count must be > 0")

        page_size = HISTORY_TRADE_PAGE_SIZE if date is not None else TRADE_PAGE_SIZE
        page_count = min(page_size, count)
        self.connect()
        if date is None:
            return self._pick_connection().request_trade(code, start, page_count, include_raw=include_raw)
        return self._pick_connection().request_history_trade(code, date, start, page_count, include_raw=include_raw)

    def get_trades_all(self, code: str, date=None):
        self.connect()
        if date is None:
            fetch_page = lambda start, count: self._pick_connection().request_trade(code, start, count)
            trading_date = None
            page_size = TRADE_PAGE_SIZE
        else:
            fetch_page = lambda start, count: self._pick_connection().request_history_trade(code, date, start, count)
            trading_date = date
            page_size = HISTORY_TRADE_PAGE_SIZE

        return self._collect_trade_pages(
            fetch_page,
            trading_date,
            page_size,
        )

    def get_trade(self, code: str, *, start: int = 0, count: int = TRADE_PAGE_SIZE, include_raw: bool = False):
        return self.get_trades(code, start=start, count=count, include_raw=include_raw)

    def get_history_trade(self, code: str, date, *, start: int = 0, count: int = HISTORY_TRADE_PAGE_SIZE, include_raw: bool = False):
        return self.get_trades(code, date, start=start, count=count, include_raw=include_raw)

    def get_trade_all(self, code: str):
        return self.get_trades_all(code)

    def get_history_trade_day(self, code: str, date):
        return self.get_trades_all(code, date)

    def get_trade_minute_kline(self, code: str) -> KlineResponse:
        return build_trade_minute_kline(self.get_trade_all(code))

    def get_history_trade_minute_kline(self, code: str, date) -> KlineResponse:
        return build_trade_minute_kline(self.get_history_trade_day(code, date))

    def get_call_auction(self, code: str, include_raw: bool = False):
        return self._pick_connection().request_call_auction(code, include_raw=include_raw)

    def get_quote(self, codes: str | list[str] | tuple[str, ...]):
        code_list = [codes] if isinstance(codes, str) else list(codes)
        if not code_list:
            return []

        self.connect()
        batches = [code_list[index : index + self._batch_size] for index in range(0, len(code_list), self._batch_size)]
        if len(batches) == 1:
            return self._pick_connection().request_quotes(batches[0])

        assert self._executor is not None
        futures = []
        for index, batch in enumerate(batches):
            connection = self._connections[index % len(self._connections)]
            futures.append(self._executor.submit(connection.request_quotes, batch))

        quotes = []
        for future in futures:
            quotes.extend(future.result())
        return quotes

    def _collect_kline_pages(self, fetch_page, page_size: int) -> KlineResponse:
        pages = []
        total = 0
        previous_page_first = None

        for start in range(0, 65536, page_size):
            page = fetch_page(start, page_size)
            if previous_page_first is not None and page.items:
                previous_page_first.last_close_price_milli = page.items[-1].close_price_milli
                previous_page_first.last_close_price = page.items[-1].close_price
            if page.items:
                previous_page_first = page.items[0]
            pages.append(page.items)
            total += len(page.items)
            if page.count < page_size:
                break

        items = []
        for page_items in reversed(pages):
            items.extend(page_items)
        return KlineResponse(count=total, items=items)

    def _collect_trade_pages(self, fetch_page, trading_date, page_size: int) -> TradeResponse:
        pages: list[list] = []
        total = 0
        response_date = None

        for start in range(0, 65536, page_size):
            page = fetch_page(start, page_size)
            pages.append(page.items)
            total += len(page.items)
            response_date = page.trading_date
            if page.count < page_size:
                break

        items = []
        for page_items in reversed(pages):
            items.extend(page_items)

        if response_date is None:
            _, response_date = normalize_trading_date(trading_date)
        return TradeResponse(count=total, trading_date=response_date, items=items)

    def _pick_connection(self) -> TdxConnection:
        with self._round_robin_lock:
            index = next(self._round_robin)
        return self._connections[index]

    def _resolve_kline_args(self, arg1, arg2) -> tuple[object, str]:
        first_is_period = self._is_kline_period(arg1)
        second_is_period = self._is_kline_period(arg2)

        if first_is_period and not second_is_period:
            return arg1, arg2
        if second_is_period and not first_is_period:
            return arg2, arg1
        if first_is_period and second_is_period:
            return arg1, arg2
        raise ValueError("one of the first two positional arguments must be a valid kline frequency")

    def _is_kline_period(self, value) -> bool:
        try:
            normalize_kline_period(value)
        except (ProtocolError, TypeError, ValueError):
            return False
        return True


