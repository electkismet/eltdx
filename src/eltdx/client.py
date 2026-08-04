"""eltdx 对外客户端入口。"""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass, field, replace
from typing import Any

from .api.auctions import AuctionApi
from .api.bars import BarApi
from .api.codes import CodeApi
from .api.corporate import CorporateApi
from .api.limits import LimitApi
from .api.minutes import MinuteApi
from .api.quotes import QuoteApi
from .api.resources import ResourceApi
from .api.session import SessionApi
from .api.trades import TradeApi
from .equity import (
    apply_factors_to_kline,
    build_factor_response,
    compute_turnover,
    filter_equity_records,
    filter_xdxr_records,
    pick_equity,
)
from .f10 import F10Client
from .helpers import HelperApi
from .models import Auction0925Result, QuoteRefreshRecord, QuoteSnapshot, SecurityCode
from .hosts import DEFAULT_PROBE_TIMEOUT, DEFAULT_PROBE_WORKERS
from .protocol.commands.klines import normalize_period
from .protocol.constants import DEFAULT_CODE_PAGE_SIZE, DEFAULT_FILE_CHUNK_SIZE
from .protocol.unit import normalize_code, normalize_market
from .transport import InMemoryTransport, PooledSocketTransport, SocketTransport, Transport
from .transport.pool import DEFAULT_POOL_SIZE, validate_pool_size
from .workday import WorkdayService

DEFAULT_QUOTE_BATCH_SIZE = 80
DEFAULT_KLINE_PAGE_SIZE = 800
DEFAULT_TODAY_TRADE_PAGE_SIZE = 1800
DEFAULT_HISTORY_TRADE_PAGE_SIZE = 2000


@dataclass(slots=True)
class TdxClient:
    """面向业务能力组织的客户端总入口。

    协议命令号保留在底层 registry 里。使用者调用
    ``client.quotes.get_snapshots(...)`` 这类业务方法，不需要直接关心
    ``0x054c`` 这类命令号。
    """

    transport: Transport | None = None
    host: str | None = None
    hosts: Sequence[str] | None = None
    timeout: float = 8.0
    pool_size: int = DEFAULT_POOL_SIZE
    batch_size: int = DEFAULT_QUOTE_BATCH_SIZE
    probe_hosts: bool = False
    probe_timeout: float = DEFAULT_PROBE_TIMEOUT
    probe_workers: int = DEFAULT_PROBE_WORKERS
    heartbeat_interval: float | None = 30.0
    max_pending_requests: int = 256
    push_queue_size: int = 1024
    push_queue_bytes: int = 8 * 1024 * 1024
    session: SessionApi = field(init=False)
    codes: CodeApi = field(init=False)
    quotes: QuoteApi = field(init=False)
    resources: ResourceApi = field(init=False)
    bars: BarApi = field(init=False)
    minutes: MinuteApi = field(init=False)
    trades: TradeApi = field(init=False)
    auctions: AuctionApi = field(init=False)
    corporate: CorporateApi = field(init=False)
    limits: LimitApi = field(init=False)
    workdays: WorkdayService = field(init=False)
    f10: F10Client = field(init=False)
    helpers: HelperApi = field(init=False)
    _code_count_cache: dict[str, int] = field(init=False, repr=False)
    _codes_all_cache: dict[str, list[SecurityCode]] = field(init=False, repr=False)
    _gbbq_cache: dict[str, Any] = field(init=False, repr=False)
    _finance_cache: dict[tuple[str, ...], Any] = field(init=False, repr=False)

    @classmethod
    def from_hosts(
        cls,
        hosts: list[str] | tuple[str, ...] | None = None,
        *,
        timeout: float = 8.0,
        pool_size: int = DEFAULT_POOL_SIZE,
        batch_size: int = DEFAULT_QUOTE_BATCH_SIZE,
        probe_hosts: bool = False,
        probe_timeout: float = DEFAULT_PROBE_TIMEOUT,
        probe_workers: int = DEFAULT_PROBE_WORKERS,
        heartbeat_interval: float | None = 30.0,
        max_pending_requests: int = 256,
        push_queue_size: int = 1024,
        push_queue_bytes: int = 8 * 1024 * 1024,
    ) -> TdxClient:
        """创建连接真实 7709 行情主站的客户端。"""

        return cls(
            transport=PooledSocketTransport(
                hosts=hosts,
                timeout=timeout,
                pool_size=pool_size,
                probe_hosts=probe_hosts,
                probe_timeout=probe_timeout,
                probe_workers=probe_workers,
                heartbeat_interval=heartbeat_interval,
                max_pending_requests=max_pending_requests,
                push_queue_size=push_queue_size,
                push_queue_bytes=push_queue_bytes,
            ),
            hosts=hosts,
            timeout=timeout,
            pool_size=pool_size,
            probe_hosts=probe_hosts,
            probe_timeout=probe_timeout,
            probe_workers=probe_workers,
            batch_size=batch_size,
            heartbeat_interval=heartbeat_interval,
            max_pending_requests=max_pending_requests,
            push_queue_size=push_queue_size,
            push_queue_bytes=push_queue_bytes,
        )

    @classmethod
    def in_memory(cls) -> TdxClient:
        """创建用于测试和示例的内存客户端。"""

        return cls(transport=InMemoryTransport())

    def __post_init__(self) -> None:
        self.pool_size = validate_pool_size(self.pool_size)
        if self.transport is None:
            resolved_hosts = _resolve_hosts(self.host, self.hosts)
            self.transport = PooledSocketTransport(
                hosts=resolved_hosts or None,
                timeout=self.timeout,
                pool_size=self.pool_size,
                probe_hosts=self.probe_hosts,
                probe_timeout=self.probe_timeout,
                probe_workers=self.probe_workers,
                heartbeat_interval=self.heartbeat_interval,
                max_pending_requests=self.max_pending_requests,
                push_queue_size=self.push_queue_size,
                push_queue_bytes=self.push_queue_bytes,
            )
        self.batch_size = min(DEFAULT_QUOTE_BATCH_SIZE, max(1, int(self.batch_size)))
        self.session = SessionApi(self.transport)
        self.codes = CodeApi(self.transport)
        self.quotes = QuoteApi(self.transport)
        self.resources = ResourceApi(self.transport)
        self.bars = BarApi(self.transport)
        self.minutes = MinuteApi(self.transport)
        self.trades = TradeApi(self.transport)
        self.auctions = AuctionApi(self.transport)
        self.corporate = CorporateApi(self.transport)
        self.limits = LimitApi(self.transport)
        self.workdays = WorkdayService(self)
        self.f10 = F10Client(timeout=self.timeout)
        self.helpers = HelperApi(self)
        self._code_count_cache = {}
        self._codes_all_cache = {}
        self._gbbq_cache = {}
        self._finance_cache = {}

    def connect(self) -> None:
        """打开底层连接。"""

        self.transport.connect()

    def close(self) -> None:
        """关闭底层连接。"""

        self.transport.close()

    def __enter__(self) -> TdxClient:
        self.connect()
        return self

    def __exit__(self, exc_type, exc, tb) -> None:
        self.close()

    def ping(self) -> str:
        """客户端可用性检查。"""

        return self.session.ping()

    def clear_cache(self) -> None:
        """清空代码表、股本变迁和财务信息等低频数据缓存。"""

        self._code_count_cache.clear()
        self._codes_all_cache.clear()
        self._gbbq_cache.clear()
        self._finance_cache.clear()
        self.helpers.clear_cache()

    def get_quote(self, codes: str | Sequence[str]):
        """兼容旧版：按代码列表批量查询行情快照，自动按 80 个一批拆分。

        ``0x054c`` 当前实盘响应只稳定包含一档盘口；这里用首次
        ``0x0547`` 刷新补齐买一到买五，避免返回伪造的深度档位。
        """

        code_list = _as_code_list(codes)
        if not code_list:
            return []

        results: list[Any] = []
        for batch in _chunks(code_list, self.batch_size):
            page = self.quotes.get_snapshots(batch)
            if isinstance(page, list):
                results.extend(self._merge_quote_depths(page, batch))
            elif isinstance(page, tuple):
                results.extend(self._merge_quote_depths(list(page), batch))
            else:
                return page
        return results

    def get_quote_depth(self, codes: str | Sequence[str]):
        """按代码列表查询五档盘口，0x0547 单次最多接受 100 个代码。"""

        return self.quotes.get_depth(codes)

    def get_legacy_quotes(self, codes: str | Sequence[str]):
        """按代码列表查询 0x053E 旧版批量行情，自动按 80 个一批拆分。"""

        code_list = _as_code_list(codes)
        if not code_list:
            return []

        results: list[Any] = []
        for batch in _chunks(code_list, self.batch_size):
            page = self.quotes.legacy(batch)
            if isinstance(page, list):
                results.extend(page)
            elif isinstance(page, tuple):
                results.extend(page)
            else:
                return page
        return results

    def read_server_file(
        self,
        path: str,
        *,
        offset: int = 0,
        size: int = DEFAULT_FILE_CHUNK_SIZE,
    ):
        """通过 0x06B9 读取一个服务器文件块。"""

        return self.resources.read(path, offset=offset, size=size)

    def _merge_quote_depths(self, snapshots: list[Any], codes: Sequence[str]) -> list[Any]:
        if not snapshots or not all(isinstance(item, QuoteSnapshot) for item in snapshots):
            return snapshots

        try:
            refresh = self.quotes.get_depth(codes)
        except Exception:
            return snapshots

        records = getattr(refresh, "records", ())
        if not records:
            return snapshots
        depth_by_code = {
            record.full_code: record
            for record in records
            if isinstance(record, QuoteRefreshRecord)
            and len(record.buy_levels) >= 5
            and len(record.sell_levels) >= 5
        }
        if not depth_by_code:
            return snapshots

        merged: list[Any] = []
        for snapshot in snapshots:
            depth = depth_by_code.get(snapshot.full_code)
            if depth is None:
                merged.append(snapshot)
                continue
            merged.append(
                replace(
                    snapshot,
                    buy_levels=depth.buy_levels,
                    sell_levels=depth.sell_levels,
                    open_amount_raw=depth.open_amount_raw,
                    open_amount_yuan=depth.open_amount_yuan,
                )
            )
        return merged

    def get_count(self, exchange, *, refresh: bool = False) -> int:
        """兼容旧版：查询某市场代码数量。"""

        market = normalize_market(exchange)
        if not refresh and market in self._code_count_cache:
            return self._code_count_cache[market]
        count = self.codes.count(market)
        self._code_count_cache[market] = count
        return count

    def get_gbbq(self, code: str, *, include_raw: bool = False, refresh: bool = False):
        """兼容旧版：查询股本变迁 / 除权相关事件。"""

        full_code = normalize_code(code)
        if not include_raw and not refresh and full_code in self._gbbq_cache:
            return self._gbbq_cache[full_code]
        block = self.corporate.capital_changes(full_code, include_raw=include_raw)
        if not include_raw:
            self._gbbq_cache[full_code] = block
        return block

    def get_xdxr(self, code: str, *, refresh: bool = False):
        """从股本变迁中整理除权除息记录。"""

        return filter_xdxr_records(self.get_gbbq(code, refresh=refresh))

    def get_equity_changes(self, code: str, *, refresh: bool = False):
        """从股本变迁中整理股本变化记录。"""

        return filter_equity_records(self.get_gbbq(code, refresh=refresh))

    def get_equity(self, code: str, on=None, *, refresh: bool = False):
        """取某日之前最近一条股本记录。"""

        return pick_equity(self.get_equity_changes(code, refresh=refresh).items, on)

    def get_turnover(self, code: str, volume: int | float, *, on=None, unit: str = "hand", refresh: bool = False) -> float:
        """用成交量和流通股本计算换手率。"""

        return compute_turnover(self.get_equity(code, on=on, refresh=refresh), volume, unit=unit)

    def get_factors(self, code: str, *, refresh: bool = False):
        """基于不复权日 K 和除权除息记录计算本地复权因子。"""

        return build_factor_response(self.bars.all(code, period="day", adjust="none"), self.get_xdxr(code, refresh=refresh))

    def get_codes(self, exchange, *, start: int = 0, limit: int | None = DEFAULT_CODE_PAGE_SIZE):
        """兼容旧版：分页查询代码表。"""

        market = normalize_market(exchange)
        if start < 0:
            raise ValueError("start must be >= 0")
        if limit is None:
            return self.codes.all(market)[start:]
        if limit < 0:
            raise ValueError("limit must be >= 0")
        return self.codes.list(market, start=start, limit=limit)

    def get_codes_all(self, exchange, *, refresh: bool = False) -> list:
        """兼容旧版：拉取某市场全量代码表。"""

        market = normalize_market(exchange)
        if not refresh and market in self._codes_all_cache:
            return list(self._codes_all_cache[market])
        items = list(self.codes.all(market))
        self._codes_all_cache[market] = items
        self._code_count_cache[market] = len(items)
        return list(items)

    def get_stock_count(self, exchange) -> int:
        return len([item for item in self.get_codes_all(exchange) if _is_stock(item)])

    def get_a_share_count(self, exchange) -> int:
        return len([item for item in self.get_codes_all(exchange) if _is_a_share(item)])

    def get_stock_codes_all(self) -> list[str]:
        return [item.full_code for item in self._all_security_codes() if _is_stock(item)]

    def get_a_share_codes_all(self) -> list[str]:
        return [item.full_code for item in self._all_security_codes() if _is_a_share(item)]

    def get_etf_codes_all(self) -> list[str]:
        return [item.full_code for item in self._all_security_codes() if _is_etf(item)]

    def get_index_codes_all(self) -> list[str]:
        return [item.full_code for item in self._all_security_codes() if _is_index(item)]

    def get_finance_batch(self, codes: str | Sequence[str], *, refresh: bool = False):
        """批量查询财务基础信息，并缓存完整返回。"""

        code_list = _as_code_list(codes)
        full_codes = tuple(normalize_code(code) for code in code_list)
        if not refresh and full_codes in self._finance_cache:
            return self._finance_cache[full_codes]
        batch = self.corporate.finance_batch(full_codes)
        self._finance_cache[full_codes] = batch
        return batch

    def get_minute(self, code: str, date=None, *, include_raw: bool = False):
        """兼容旧版：不传日期取当日分时，传日期取历史分时。"""

        if date is None:
            return self.minutes.today(code, include_raw=include_raw)
        return self.minutes.history(code, date, include_raw=include_raw)

    def get_history_minute(self, code: str, date, *, include_raw: bool = False):
        return self.get_minute(code, date, include_raw=include_raw)

    def get_kline(
        self,
        arg1,
        arg2=None,
        *,
        start: int = 0,
        count: int = DEFAULT_KLINE_PAGE_SIZE,
        kind: str = "stock",
        adjust: str | None = None,
        anchor_date=None,
        include_raw: bool = False,
    ):
        """兼容旧版：查询一页 K 线，支持 ``(period, code)`` 或 ``(code, period)``。"""

        period, code = self._resolve_kline_args(arg1, arg2)
        return self.bars.get(
            code,
            period=period,
            start=start,
            count=count,
            adjust=adjust,
            anchor_date=anchor_date,
            kind=kind,
            include_raw=include_raw,
        )

    def get_kline_all(
        self,
        arg1,
        arg2=None,
        *,
        kind: str = "stock",
        adjust: str | None = None,
        anchor_date=None,
        page_size: int = DEFAULT_KLINE_PAGE_SIZE,
        max_pages: int | None = 200,
        include_raw: bool = False,
    ):
        """兼容旧版：分页拉取完整 K 线。"""

        period, code = self._resolve_kline_args(arg1, arg2)
        return self.bars.all(
            code,
            period=period,
            adjust=adjust,
            anchor_date=anchor_date,
            kind=kind,
            page_size=page_size,
            max_pages=max_pages,
            include_raw=include_raw,
        )

    def get_trades(
        self,
        code: str,
        date=None,
        *,
        start: int = 0,
        count: int | None = None,
        include_raw: bool = False,
    ):
        """兼容旧版：不传日期取当日成交明细，传日期取历史成交明细。"""

        if date is None:
            return self.trades.today(code, start=start, count=count or DEFAULT_TODAY_TRADE_PAGE_SIZE, include_raw=include_raw)
        return self.trades.history(
            code,
            date,
            start=start,
            count=count or DEFAULT_HISTORY_TRADE_PAGE_SIZE,
            include_raw=include_raw,
        )

    def get_trades_all(
        self,
        code: str,
        date=None,
        *,
        page_size: int | None = None,
        max_pages: int | None = 100,
        include_raw: bool = False,
    ):
        """兼容旧版：分页拉取成交明细，直到服务端返回短页。"""

        page_size = page_size or (DEFAULT_HISTORY_TRADE_PAGE_SIZE if date is not None else DEFAULT_TODAY_TRADE_PAGE_SIZE)
        if page_size <= 0 or page_size > 0xFFFF:
            raise ValueError("page_size must be between 1 and 65535")
        if max_pages is not None and max_pages <= 0:
            raise ValueError("max_pages must be positive or None")

        start = 0
        pages = 0
        first_page = None
        ticks = []
        while True:
            page = self.get_trades(code, date, start=start, count=page_size, include_raw=include_raw)
            if not hasattr(page, "ticks") or not hasattr(page, "count"):
                return page
            if first_page is None:
                first_page = page
            ticks.extend(page.ticks)
            pages += 1
            if page.count < page_size:
                return replace(first_page, start=0, request_count=len(ticks), ticks=tuple(ticks))
            if max_pages is not None and pages >= max_pages:
                raise RuntimeError("get_trades_all reached max_pages before the server returned a short page")
            start += page_size

    def get_trade(self, code: str, *, start: int = 0, count: int = DEFAULT_TODAY_TRADE_PAGE_SIZE, include_raw: bool = False):
        return self.get_trades(code, start=start, count=count, include_raw=include_raw)

    def get_trade_all(self, code: str, *, include_raw: bool = False):
        return self.get_trades_all(code, include_raw=include_raw)

    def get_history_trade(
        self,
        code: str,
        date,
        *,
        start: int = 0,
        count: int = DEFAULT_HISTORY_TRADE_PAGE_SIZE,
        include_raw: bool = False,
    ):
        return self.get_trades(code, date, start=start, count=count, include_raw=include_raw)

    def get_history_trade_day(self, code: str, date, *, include_raw: bool = False):
        return self.get_trades_all(code, date, include_raw=include_raw)

    def get_auction_0925(
        self,
        code: str,
        date,
        *,
        page_size: int = DEFAULT_HISTORY_TRADE_PAGE_SIZE,
        max_pages: int | None = 100,
    ) -> Auction0925Result:
        """兼容旧版：从历史成交明细里获取 09:25 竞价成交快照。"""

        if page_size <= 0 or page_size > 0xFFFF:
            raise ValueError("page_size must be between 1 and 65535")
        if max_pages is not None and max_pages <= 0:
            raise ValueError("max_pages must be positive or None")

        full_code = normalize_code(code)
        pages_used = 0
        for start in range(0, 0x10000, page_size):
            page = self.trades.history(full_code, date, start=start, count=page_size)
            pages_used += 1
            tick = next((item for item in getattr(page, "ticks", ()) if item.time_minutes == 9 * 60 + 25), None)
            if tick is not None:
                return Auction0925Result(
                    code=full_code,
                    trading_date=getattr(page, "trading_date", None),
                    has_auction_0925=True,
                    price=tick.price,
                    price_milli=tick.price_milli,
                    volume=tick.volume,
                    amount=round(tick.trade_amount_yuan, 2),
                    status=tick.status_raw,
                    side=tick.side,
                    pages_used=pages_used,
                    source_mode="history_ticks_scan",
                )
            if not hasattr(page, "count") or page.count < page_size:
                return Auction0925Result(
                    code=full_code,
                    trading_date=getattr(page, "trading_date", None),
                    has_auction_0925=False,
                    price=None,
                    price_milli=None,
                    volume=None,
                    amount=None,
                    status=None,
                    side=None,
                    pages_used=pages_used,
                    source_mode="history_ticks_no_0925",
                )
            if max_pages is not None and pages_used >= max_pages:
                raise RuntimeError("get_auction_0925 reached max_pages before the server returned a short page")
        raise RuntimeError("get_auction_0925 exceeded protocol page limit")

    def get_call_auction(self, code: str, *, include_raw: bool = False):
        return self.auctions.series(code, include_raw=include_raw)

    def get_adjusted_kline(
        self,
        period,
        code: str,
        *,
        adjust="qfq",
        anchor_date=None,
        start: int = 0,
        count: int = DEFAULT_KLINE_PAGE_SIZE,
        include_raw: bool = False,
    ):
        """兼容旧版名称：当前直接使用 0x052d 的服务端复权参数。"""

        return self.bars.get(
            code,
            period=period,
            adjust=adjust,
            anchor_date=anchor_date,
            start=start,
            count=count,
            include_raw=include_raw,
        )

    def get_adjusted_kline_all(
        self,
        period,
        code: str,
        *,
        adjust="qfq",
        anchor_date=None,
        page_size: int = DEFAULT_KLINE_PAGE_SIZE,
        max_pages: int | None = 200,
        include_raw: bool = False,
    ):
        """兼容旧版名称：当前直接使用 0x052d 的服务端复权参数。"""

        return self.bars.all(
            code,
            period=period,
            adjust=adjust,
            anchor_date=anchor_date,
            page_size=page_size,
            max_pages=max_pages,
            include_raw=include_raw,
        )

    def get_local_adjusted_kline_all(self, period, code: str, *, adjust="qfq"):
        """高级工具：用本地复权因子调整不复权 K 线。"""

        base = self.bars.all(code, period=period, adjust="none")
        return apply_factors_to_kline(base, self.get_factors(code), adjust=adjust)

    def _all_security_codes(self) -> list[SecurityCode]:
        items: list[SecurityCode] = []
        for market in ("sh", "sz", "bj"):
            items.extend(self.get_codes_all(market))
        return items

    def _resolve_kline_args(self, arg1, arg2) -> tuple[Any, str]:
        if arg2 is None:
            raise ValueError("get_kline requires both period and code")
        first_is_period = _is_kline_period(arg1)
        second_is_period = _is_kline_period(arg2)
        if first_is_period and not second_is_period:
            return arg1, str(arg2)
        if second_is_period and not first_is_period:
            return arg2, str(arg1)
        if first_is_period and second_is_period:
            return arg1, str(arg2)
        raise ValueError("one of the first two positional arguments must be a valid kline period")


def _as_code_list(codes: str | Sequence[str]) -> list[str]:
    if isinstance(codes, str):
        return [codes]
    return list(codes)


def _resolve_hosts(host: str | None, hosts: Sequence[str] | None) -> list[str]:
    if hosts is None:
        resolved_hosts = []
    elif isinstance(hosts, str):
        resolved_hosts = [hosts]
    else:
        resolved_hosts = list(hosts)
    if host is not None:
        resolved_hosts.insert(0, host)
    return resolved_hosts


def _chunks(items: Sequence[str], size: int):
    for index in range(0, len(items), size):
        yield list(items[index : index + size])


def _is_kline_period(value) -> bool:
    try:
        normalize_period(value)
    except Exception:
        return False
    return True


def _is_a_share(item: SecurityCode) -> bool:
    return item.category == "a_share"


def _is_stock(item: SecurityCode) -> bool:
    return item.category in {"a_share", "b_share"}


def _is_etf(item: SecurityCode) -> bool:
    return item.category == "etf"


def _is_index(item: SecurityCode) -> bool:
    return item.category == "index"


Client = TdxClient
