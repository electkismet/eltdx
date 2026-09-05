"""eltdx 对外客户端入口。"""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass, field
from threading import RLock

from .api.auctions import AuctionApi
from .api.bars import BarApi
from .api.codes import CodeApi
from .api.corporate import CorporateApi
from .api.limits import LimitApi
from .api.minutes import MinuteApi
from .api.money_flow import MoneyFlowApi
from .api.quotes import QuoteApi
from .api.resources import ResourceApi
from .api.session import SessionApi
from .api.trades import TradeApi
from .f10 import F10Client
from .helpers import HelperApi
from .hosts import (
    DEFAULT_HOSTS,
    DEFAULT_PROBE_HOSTS,
    DEFAULT_PROBE_TIMEOUT,
    DEFAULT_PROBE_WORKERS,
    load_money_flow_hosts,
    rank_hosts_from_cache,
    sort_hosts_by_latency,
    unique_hosts,
)
from .transport import InMemoryTransport, PooledSocketTransport, SocketTransport, Transport
from .transport._config import (
    DEFAULT_CONNECTIONS_PER_SERVER,
    DEFAULT_POOL_SIZE,
    DEFAULT_PUSH_QUEUE_BYTES,
    DEFAULT_SERVER_COUNT,
    optional_positive_int,
    positive_int,
)
from .workday import WorkdayService

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
    pool_size: int | None = None
    probe_hosts: bool = DEFAULT_PROBE_HOSTS
    probe_timeout: float = DEFAULT_PROBE_TIMEOUT
    probe_workers: int = DEFAULT_PROBE_WORKERS
    heartbeat_interval: float | None = 30.0
    max_pending_requests: int = 256
    push_queue_size: int = 1024
    push_queue_bytes: int = DEFAULT_PUSH_QUEUE_BYTES
    server_count: int = DEFAULT_SERVER_COUNT
    connections_per_server: int | None = None
    runtime_workers: int | None = None
    max_connections_per_host: int | None = None
    connect_concurrency: int | None = None
    connect_concurrency_per_host: int | None = None
    global_raw_bytes: int | None = None
    global_decoded_bytes: int | None = None
    session: SessionApi = field(init=False)
    codes: CodeApi = field(init=False)
    quotes: QuoteApi = field(init=False)
    resources: ResourceApi = field(init=False)
    bars: BarApi = field(init=False)
    minutes: MinuteApi = field(init=False)
    money_flow: MoneyFlowApi = field(init=False)
    trades: TradeApi = field(init=False)
    auctions: AuctionApi = field(init=False)
    corporate: CorporateApi = field(init=False)
    limits: LimitApi = field(init=False)
    workdays: WorkdayService = field(init=False)
    f10: F10Client = field(init=False)
    helpers: HelperApi = field(init=False)
    _dedicated_hosts: tuple[str, ...] = field(init=False, default=(), repr=False, compare=False)
    _decimal_cache: dict[str, int] = field(init=False, default_factory=dict, repr=False, compare=False)
    _decimal_markets: set[str] = field(init=False, default_factory=set, repr=False, compare=False)
    _decimal_lock: RLock = field(init=False, default_factory=RLock, repr=False, compare=False)

    @classmethod
    def from_hosts(
        cls,
        hosts: list[str] | tuple[str, ...] | None = None,
        *,
        timeout: float = 8.0,
        pool_size: int | None = None,
        server_count: int = DEFAULT_SERVER_COUNT,
        connections_per_server: int | None = None,
        runtime_workers: int | None = None,
        max_connections_per_host: int | None = None,
        connect_concurrency: int | None = None,
        connect_concurrency_per_host: int | None = None,
        global_raw_bytes: int | None = None,
        global_decoded_bytes: int | None = None,
        probe_hosts: bool = DEFAULT_PROBE_HOSTS,
        probe_timeout: float = DEFAULT_PROBE_TIMEOUT,
        probe_workers: int = DEFAULT_PROBE_WORKERS,
        heartbeat_interval: float | None = 30.0,
        max_pending_requests: int = 256,
        push_queue_size: int = 1024,
        push_queue_bytes: int = DEFAULT_PUSH_QUEUE_BYTES,
    ) -> TdxClient:
        """创建连接真实 7709 行情主站的客户端。"""

        return cls(
            transport=PooledSocketTransport(
                hosts=hosts,
                timeout=timeout,
                pool_size=pool_size,
                server_count=server_count,
                connections_per_server=connections_per_server,
                runtime_workers=runtime_workers,
                max_connections_per_host=max_connections_per_host,
                connect_concurrency=connect_concurrency,
                connect_concurrency_per_host=connect_concurrency_per_host,
                global_raw_bytes=global_raw_bytes,
                global_decoded_bytes=global_decoded_bytes,
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
            server_count=server_count,
            connections_per_server=connections_per_server,
            runtime_workers=runtime_workers,
            max_connections_per_host=max_connections_per_host,
            connect_concurrency=connect_concurrency,
            connect_concurrency_per_host=connect_concurrency_per_host,
            global_raw_bytes=global_raw_bytes,
            global_decoded_bytes=global_decoded_bytes,
            probe_hosts=probe_hosts,
            probe_timeout=probe_timeout,
            probe_workers=probe_workers,
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
        self.pool_size = optional_positive_int("pool_size", self.pool_size)
        self.server_count = positive_int("server_count", self.server_count)
        self.connections_per_server = optional_positive_int(
            "connections_per_server", self.connections_per_server
        )
        self.runtime_workers = optional_positive_int("runtime_workers", self.runtime_workers)
        self.max_connections_per_host = optional_positive_int(
            "max_connections_per_host", self.max_connections_per_host
        )
        self.connect_concurrency = optional_positive_int(
            "connect_concurrency", self.connect_concurrency
        )
        self.connect_concurrency_per_host = optional_positive_int(
            "connect_concurrency_per_host", self.connect_concurrency_per_host
        )
        self.global_raw_bytes = optional_positive_int(
            "global_raw_bytes", self.global_raw_bytes
        )
        self.global_decoded_bytes = optional_positive_int(
            "global_decoded_bytes", self.global_decoded_bytes
        )
        if self.transport is None:
            resolved_hosts = _resolve_hosts(self.host, self.hosts)
            self.transport = PooledSocketTransport(
                hosts=resolved_hosts or None,
                timeout=self.timeout,
                pool_size=self.pool_size,
                server_count=self.server_count,
                connections_per_server=self.connections_per_server,
                runtime_workers=self.runtime_workers,
                max_connections_per_host=self.max_connections_per_host,
                connect_concurrency=self.connect_concurrency,
                connect_concurrency_per_host=self.connect_concurrency_per_host,
                global_raw_bytes=self.global_raw_bytes,
                global_decoded_bytes=self.global_decoded_bytes,
                probe_hosts=self.probe_hosts,
                probe_timeout=self.probe_timeout,
                probe_workers=self.probe_workers,
                heartbeat_interval=self.heartbeat_interval,
                max_pending_requests=self.max_pending_requests,
                push_queue_size=self.push_queue_size,
                push_queue_bytes=self.push_queue_bytes,
            )
        if isinstance(self.transport, PooledSocketTransport):
            self.pool_size = self.transport.pool_size
            self.server_count = self.transport.server_count
            self.connections_per_server = self.transport.connections_per_server
        elif self.pool_size is None:
            self.pool_size = DEFAULT_POOL_SIZE
        if self.connections_per_server is None:
            self.connections_per_server = DEFAULT_CONNECTIONS_PER_SERVER
        self.session = SessionApi(self.transport)
        self.codes = CodeApi(self.transport, metadata_sink=self._cache_security_rows)
        self.quotes = QuoteApi(self.transport, price_resolver=self._normalize_prices)
        self.resources = ResourceApi(self.transport)
        self.bars = BarApi(self.transport)
        self.minutes = MinuteApi(self.transport, price_resolver=self._normalize_prices)
        self.money_flow = MoneyFlowApi(
            self.transport,
            transport_factory=(
                self._build_money_flow_transport
                if isinstance(self.transport, PooledSocketTransport)
                else None
            ),
        )
        self.trades = TradeApi(self.transport)
        self.auctions = AuctionApi(
            self.transport,
            transport_factory=(
                self._build_dedicated_market_transport
                if isinstance(self.transport, PooledSocketTransport)
                else None
            ),
        )
        self.corporate = CorporateApi(self.transport)
        self.limits = LimitApi(self.transport)
        self.workdays = WorkdayService(self)
        self.f10 = F10Client(timeout=self.timeout)
        self.helpers = HelperApi(self)
        self._prepare_host_rankings()

    def _prepare_host_rankings(self) -> None:
        """Rank both host groups once without opening business connections."""
        if not isinstance(self.transport, PooledSocketTransport):
            return
        ordinary = self.transport.hosts
        dedicated = rank_hosts_from_cache(load_money_flow_hosts() or DEFAULT_HOSTS)
        if self.probe_hosts:
            candidates = unique_hosts(list(ordinary) + dedicated)
            ranked = sort_hosts_by_latency(
                candidates, timeout=self.probe_timeout, max_workers=self.probe_workers
            )
            ordinary_set, dedicated_set = set(ordinary), set(dedicated)
            ordinary = tuple(host for host in ranked if host in ordinary_set)
            dedicated = [host for host in ranked if host in dedicated_set]
        self.transport._set_ranked_hosts(ordinary)
        self._dedicated_hosts = tuple(dedicated)

    def connect(self) -> None:
        """打开底层连接。"""

        assert self.transport is not None
        self.transport.connect()

    def close(self) -> None:
        """关闭底层连接。"""

        assert self.transport is not None
        self.transport.close()
        self.money_flow.close()
        self.auctions._close()

    def _build_dedicated_market_transport(self) -> PooledSocketTransport:
        """Create a pool using the money-flow and auction-capable host list."""
        transport = PooledSocketTransport(
            hosts=self._dedicated_hosts,
            timeout=self.timeout,
            pool_size=self.pool_size,
            server_count=self.server_count,
            connections_per_server=self.connections_per_server,
            runtime_workers=self.runtime_workers,
            max_connections_per_host=self.max_connections_per_host,
            connect_concurrency=self.connect_concurrency,
            connect_concurrency_per_host=self.connect_concurrency_per_host,
            global_raw_bytes=self.global_raw_bytes,
            global_decoded_bytes=self.global_decoded_bytes,
            probe_hosts=self.probe_hosts,
            probe_timeout=self.probe_timeout,
            probe_workers=self.probe_workers,
            heartbeat_interval=self.heartbeat_interval,
            max_pending_requests=self.max_pending_requests,
            push_queue_size=self.push_queue_size,
            push_queue_bytes=self.push_queue_bytes,
        )
        transport._set_ranked_hosts(self._dedicated_hosts)
        return transport

    def _build_money_flow_transport(self) -> PooledSocketTransport:
        """Backward-compatible name for the dedicated 7709 market pool."""
        return self._build_dedicated_market_transport()

    def __enter__(self) -> TdxClient:
        self.connect()
        return self

    def __exit__(self, exc_type, exc, tb) -> None:
        self.close()

    def ping(self) -> str:
        """客户端可用性检查。"""

        return self.session.ping()

    def clear_cache(self) -> None:
        """清空 Helpers 与行情精度元数据的内存缓存。"""

        self.helpers.clear_cache()
        with self._decimal_lock:
            self._decimal_cache.clear()
            self._decimal_markets.clear()

    def _ensure_decimals(self, codes) -> dict[str, int]:
        markets = set()
        for value in (codes or []):
            if not isinstance(value, str):
                continue
            code = value.strip().lower()
            if code[:2] in {"sh", "sz", "bj"}:
                markets.add(code[:2])
            elif code[:1] in {"6", "9"}:
                markets.add("sh")
            elif code[:1] in {"0", "1", "2", "3", "4"}:
                markets.add("sz")
            elif code[:1] in {"8"}:
                markets.add("bj")
        if not markets:
            markets = {"sh", "sz", "bj"}
        with self._decimal_lock:
            for market in sorted(markets):
                if market in self._decimal_markets:
                    continue
                self.codes.all(market)
                self._decimal_markets.add(market)
            return dict(self._decimal_cache)

    def _cache_security_rows(self, rows) -> None:
        with self._decimal_lock:
            # Code-list requests normally return ``SecurityRow`` objects, but
            # transports used by tests (and diagnostic/in-memory transports)
            # may return an envelope or other iterable payload.  Ignore those
            # values here; precision caching is best-effort and must not alter
            # the public response or make a metadata request fail.
            if not isinstance(rows, (list, tuple)):
                return
            for row in rows:
                full_code = getattr(row, "full_code", None)
                decimal = getattr(row, "decimal", None)
                if isinstance(full_code, str) and decimal is not None:
                    self._decimal_cache[full_code] = int(decimal)

    def _normalize_prices(self, command_name: str, result, codes=None):
        if not isinstance(self.transport, (PooledSocketTransport, SocketTransport)):
            return result
        self._ensure_decimals(codes)
        if result is None:
            return None
        # The native parser reads the shared 0x044d precision registry.  The
        # metadata load above is intentionally performed before the request.
        return result

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


Client = TdxClient
