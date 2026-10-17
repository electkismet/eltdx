"""扩展行情高层同步客户端 ExTdxClient。"""

from __future__ import annotations

from collections import OrderedDict
from types import TracebackType
from typing import TypeVar

from eltdx._bar_time import (
    BAR_TIME_END,
    BAR_TIME_START,
    align_hour_minute_bars,
    category_to_minutes,
    check_bar_time,
)
from eltdx.ex.commands.base import BaseCommand
from eltdx.ex.commands.get_history_bars_range import GetExHistoryInstrumentBarsRangeCmd
from eltdx.ex.commands.get_instrument_bars import GetExInstrumentBarsCmd
from eltdx.ex.commands.get_instrument_count import GetExInstrumentCountCmd
from eltdx.ex.commands.get_instrument_info import GetExInstrumentInfoCmd
from eltdx.ex.commands.get_instrument_quote import GetExInstrumentQuoteCmd
from eltdx.ex.commands.get_instrument_quote_list import GetExInstrumentQuoteListCmd
from eltdx.ex.commands.get_markets import GetExMarketsCmd
from eltdx.ex.commands.get_minute_time import (
    GetExHistoryMinuteTimeDataCmd,
    GetExMinuteTimeDataCmd,
)
from eltdx.ex.commands.get_transaction import (
    GetExHistoryTransactionDataCmd,
    GetExTransactionDataCmd,
)
from eltdx.ex.hosts import DEFAULT_EX_PORT, load_ex_hosts
from eltdx.ex.models import (
    ExInstrumentBar,
    ExInstrumentInfo,
    ExInstrumentQuote,
    ExMarketInfo,
    ExMinuteBar,
    ExTransactionRecord,
)
from eltdx.ex.transport.sync import (
    DEFAULT_PROBE_TIMEOUT,
    DEFAULT_PROBE_WORKERS,
    ExHostProbe,
    ExTdxConnection,
    ping_ex_all,
    probe_ex_hosts,
    rank_ex_hosts,
)
from eltdx.exceptions import TransportError

_T = TypeVar("_T")


class ExTdxClient:
    """同步扩展行情客户端（期货、港股、外股等，端口 7727）。

    使用示例::

        with ExTdxClient() as c:
            print(c.get_instrument_count())
            rows = c.get_instrument_info(0, 100)

    注意：**不要用 ``get_markets()`` 做首条调用**。实测（含与 easy_tdx 1.20.7 的
    差分对照）所有内置主站都不回应 ``0x23f4``，该调用只会阻塞到超时；需要市场
    清单请直接用 :class:`eltdx.models.enums.ExMarket` 枚举。详见 docs/EXTENDED_MARKET.md。

    主机选择与失败自动负载均衡
    --------------------------

    扩展行情的可用性比 7709 差得多：内置的 16 台主机 **TCP 全部可连**，但只有极少数
    会应答业务命令（实测只有 ``0x23f5`` 枚举时有且仅有 1 台应答）。因此「按 TCP 顺序
    故障转移」是没有意义的——连得上不等于用得了。

    本客户端的做法是：

    1. **并发探活**：``connect()`` 时会用真实业务命令（默认枚举命令 ``0x23f5``）并发
       探测全部候选主机，把**真正应答**的主机按延迟排在前面（见
       :func:`eltdx.ex.transport.sync.probe_ex_hosts`）。16 台并发约 1 个往返，
       而不是串行超时 16 次。
    2. **命令级故障转移**：失败判定不看 TCP，而看**命令是否拿到应答**。某台主机命令
       失败后立刻记为不健康并换到次优主机重试，不会像以前那样「重连后仍回到同一台」
       （旧实现的 ``_ordered_hosts()`` 把当前主机排在首位，导致故障转移形同虚设）。
    3. **失败自动负载均衡**：一次会话内失败的宿主机会被排除在候选之外，请求自动落到
       其余健康主机上；成功一次即自动恢复该主机的健康标记。全部主机都失败时清空标记
       重来一轮，避免会话被永久锁死。

    单主机（``hosts`` 只有一台，或显式只给 ``host``）时**不做探活**，行为与旧版一致，
    零额外开销。
    """

    def __init__(
        self,
        host: str | None = None,
        port: int = DEFAULT_EX_PORT,
        timeout: float = 15.0,
        *,
        hosts: list[str] | None = None,
        auto_failover: bool = True,
        probe_on_connect: bool = True,
        probe_timeout: float | None = None,
        max_probe_workers: int = DEFAULT_PROBE_WORKERS,
    ) -> None:
        candidates = load_ex_hosts() if hosts is None else list(hosts)
        if host is not None:
            candidates = [host] + [h for h in candidates if h != host]
        if not candidates:
            raise ValueError("hosts 不能为空")
        self._hosts = candidates
        self._port = port
        self._timeout = timeout
        self._auto_failover = auto_failover
        self._probe_on_connect = probe_on_connect
        self._probe_timeout = (
            min(DEFAULT_PROBE_TIMEOUT, timeout) if probe_timeout is None else probe_timeout
        )
        self._max_probe_workers = max_probe_workers

        # 显式指定的主机优先；探活只用于在它失败后挑备份。
        self._preferred_host = host
        self._host = host if host is not None else candidates[0]
        self._conn = ExTdxConnection(self._host, port, timeout)
        self._connected = False

        self._ordered = list(candidates)
        self._healthy: set[str] = set()
        self._unhealthy: dict[str, str] = {}
        self._probed = False

    @classmethod
    def from_best_host(
        cls,
        hosts: list[str] | None = None,
        port: int = DEFAULT_EX_PORT,
        timeout: float = 15.0,
        ping_timeout: float = DEFAULT_PROBE_TIMEOUT,
    ) -> "ExTdxClient":
        """并发探活全部候选主机，选延迟最低的**可用**主机建立连接。

        与旧版的区别：旧版只测 TCP（16 台全通，等于随机挑一台）；现在要求主机
        真的应答业务命令，所以选出来的主机是可用的。
        """

        candidates = load_ex_hosts() if hosts is None else list(hosts)
        results = probe_ex_hosts(candidates, port, ping_timeout)
        ordered = rank_ex_hosts(results, candidates=candidates)
        client = cls(
            ordered[0] if ordered else None,
            port,
            timeout,
            hosts=candidates,
            probe_on_connect=False,
        )
        client._seed_from_probe(results)
        return client

    @staticmethod
    def ping_all(
        hosts: list[str] | None = None,
        port: int = DEFAULT_EX_PORT,
        timeout: float = DEFAULT_PROBE_TIMEOUT,
    ) -> list[tuple[str, float]]:
        """并发探活多台主机，按延迟升序返回**应答业务命令**的主机。

        语义已修正：旧版把「TCP 能连上」当成健康，于是 16 台内置主机全部返回，
        结论毫无参考价值；现在只返回真正应答业务命令的主机。
        """

        return ping_ex_all(hosts, port, timeout)

    # ------------------------------------------------------------------ #
    # 主机健康度
    # ------------------------------------------------------------------ #

    @property
    def host(self) -> str:
        """当前（或下一个将要）使用的主机地址。"""

        return self._host

    @property
    def healthy_hosts(self) -> list[str]:
        """当前已知可用、且近期未失败的主机，按优先级排序。

        未做过探活（例如单主机模式）时返回空列表——空表示「没有信息」，
        而不是「都不可用」。
        """

        return [h for h in self._ordered if h in self._healthy and h not in self._unhealthy]

    @property
    def unhealthy_hosts(self) -> dict[str, str]:
        """近期失败的主机到失败原因的映射（副本）。"""

        return dict(self._unhealthy)

    def refresh_hosts(
        self,
        *,
        timeout: float | None = None,
        max_workers: int | None = None,
    ) -> list[ExHostProbe]:
        """并发重新探活全部候选主机并重排优先级；返回探活明细。

        会清空此前累积的失败标记（探活结果是最新事实）。这里是**完整**探活，会等满
        ``probe_timeout`` 以取得可靠的延迟排名；只求「尽快找到一台能用的」时走内部
        早退路径（``connect()`` 与失败换台）。
        """

        return self._probe_and_seed(timeout=timeout, max_workers=max_workers)

    def _probe_and_seed(
        self,
        *,
        timeout: float | None = None,
        max_workers: int | None = None,
        stop_after_first: bool = False,
    ) -> list[ExHostProbe]:
        results = probe_ex_hosts(
            self._hosts,
            self._port,
            self._probe_timeout if timeout is None else timeout,
            max_workers=max_workers if max_workers is not None else self._max_probe_workers,
            stop_after_first=stop_after_first,
        )
        self._seed_from_probe(results)
        return results

    def _seed_from_probe(self, results: list[ExHostProbe]) -> None:
        self._ordered = rank_ex_hosts(results, candidates=self._hosts)
        self._healthy = {result.host for result in results if result.ok}
        self._unhealthy.clear()
        self._probed = True

    def _reprobe_remaining(self) -> None:
        """失败后并发重探「尚未判坏」的主机，避免失败链退化成逐台串行超时。

        必须重探而不是照单顺序硬试：真实环境里 15/16 台主机 TCP 通但不应答，
        照单硬试会让一次失败最多拖 ``15 × timeout`` 秒。重探是并发的，代价约一个
        ``probe_timeout``，并且只会把真正应答的主机排到前面。
        """

        remaining = [host for host in self._hosts if host not in self._unhealthy]
        if len(remaining) <= 1:
            return
        results = probe_ex_hosts(
            remaining,
            self._port,
            self._probe_timeout,
            max_workers=self._max_probe_workers,
            stop_after_first=True,
        )
        # candidates 传全量：已判坏的主机仍留在队尾，全部失败时还能被回退机制捡回来。
        self._ordered = rank_ex_hosts(results, candidates=self._hosts)
        self._healthy = {result.host for result in results if result.ok}

    # ------------------------------------------------------------------ #
    # 连接管理
    # ------------------------------------------------------------------ #

    def connect(self) -> None:
        """建立 TCP 连接。

        多主机且启用探活时，会先并发探活、再连优先级最高的主机；
        单主机模式不做探活，直接连。
        """

        if (
            self._auto_failover
            and self._probe_on_connect
            and not self._probed
            and len(self._hosts) > 1
        ):
            # 早退：可用主机通常几十毫秒就回，没必要等 15 台静默主机把超时耗满。
            self._probe_and_seed(stop_after_first=True)

        last_exc: TransportError | None = None
        for host in self._candidate_order():
            try:
                conn = ExTdxConnection(host, self._port, self._timeout)
                conn.connect()
            except TransportError as exc:
                last_exc = exc
                self._unhealthy[host] = f"{type(exc).__name__}: {exc}"
                continue
            self._conn = conn
            self._host = host
            self._connected = True
            return
        raise last_exc if last_exc is not None else TransportError("没有可用主机")

    def close(self) -> None:
        """关闭连接。"""

        self._conn.close()
        self._connected = False

    def __enter__(self) -> "ExTdxClient":
        self.connect()
        return self

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        exc_tb: TracebackType | None,
    ) -> None:
        self.close()

    def _candidate_order(self) -> list[str]:
        """连接优先级：探活排序 - 近期失败者；失败者清空后回退到全量顺序。"""

        ordered = [host for host in self._ordered if host not in self._unhealthy]
        if not ordered:
            # 全部判坏：清空标记重来一轮，避免会话被永久锁死。
            self._unhealthy.clear()
            ordered = list(self._ordered)
        if (
            self._preferred_host is not None
            and self._preferred_host in ordered
            and ordered[0] != self._preferred_host
        ):
            ordered = [self._preferred_host] + [h for h in ordered if h != self._preferred_host]
        return ordered

    def _execute(self, cmd: "BaseCommand[_T]") -> _T:
        """执行命令；命令级失败时标记该主机并换到下一台重试。"""

        if not self._connected:
            self.connect()

        if not self._auto_failover:
            return self._conn.execute(cmd)

        last_exc: TransportError | None = None
        limit = max(1, len(self._hosts))
        for _ in range(limit):
            try:
                result = self._conn.execute(cmd)
            except TransportError as exc:
                last_exc = exc
                self._healthy.discard(self._host)
                self._unhealthy[self._host] = f"{type(exc).__name__}: {exc}"
                self._conn.close()
                self._connected = False
                try:
                    self._reprobe_remaining()
                    self.connect()
                except TransportError as connect_exc:
                    last_exc = connect_exc
                    break
                continue
            self._healthy.add(self._host)
            self._unhealthy.pop(self._host, None)
            return result
        raise last_exc if last_exc is not None else TransportError("没有可用主机")

    # ------------------------------------------------------------------ #
    # 市场信息
    # ------------------------------------------------------------------ #

    def get_markets(self) -> list[ExMarketInfo]:
        """获取扩展行情支持的市场列表。"""

        return self._execute(GetExMarketsCmd())

    def get_instrument_count(self) -> int:
        """获取扩展行情商品总数。"""

        return self._execute(GetExInstrumentCountCmd())

    def get_instrument_info(self, start: int, count: int = 100) -> list[ExInstrumentInfo]:
        """获取商品信息列表（分页）。"""

        return self._execute(GetExInstrumentInfoCmd(start, count))

    # ------------------------------------------------------------------ #
    # 行情
    # ------------------------------------------------------------------ #

    def get_instrument_quote(self, market: int, code: str) -> ExInstrumentQuote | None:
        """获取单个商品五档实时行情。"""

        return self._execute(GetExInstrumentQuoteCmd(market, code))

    def get_instrument_quote_list(
        self,
        market: int,
        category: int,
        start: int = 0,
        count: int = 80,
    ) -> list[OrderedDict[str, object]]:
        """按类别获取商品行情列表。"""

        return self._execute(GetExInstrumentQuoteListCmd(market, category, start, count))

    # ------------------------------------------------------------------ #
    # K线
    # ------------------------------------------------------------------ #

    def get_instrument_bars(
        self,
        category: int,
        market: int,
        code: str,
        start: int = 0,
        count: int = 700,
        *,
        bar_time: str = BAR_TIME_START,
    ) -> list[ExInstrumentBar]:
        """获取K线数据。

        Args:
            bar_time: 时间戳语义。``"start"``（默认）= bar 开始时间（通达信原始，
                上午最后一根 5min 标 ``11:25``、下午第一根标 ``13:00``）；``"end"`` =
                bar 右端点（= 开始 + 周期时长，与 Tushare/同花顺对齐，上午最后一根标
                ``11:30``）。仅对分钟级周期生效；日线及以上不受影响。
        """

        bars = self._execute(GetExInstrumentBarsCmd(category, market, code, start, count))
        return align_hour_minute_bars(
            bars,
            delta_minutes=category_to_minutes(category),
            bar_time=bar_time,
        )

    def get_history_instrument_bars_range(
        self,
        market: int,
        code: str,
        start_date: int,
        end_date: int,
        *,
        bar_time: str = BAR_TIME_START,
    ) -> list[ExInstrumentBar]:
        """按日期范围获取历史K线。

        Args:
            bar_time: 见 :meth:`get_instrument_bars`。本接口**不支持** ``"end"``：
            它只按日期范围查询、不知道周期时长，无法算出右端点，会抛
            :class:`ValueError`（与 easy_tdx 一致）。
        """

        if bar_time == BAR_TIME_END:
            raise ValueError(
                "get_history_instrument_bars_range 不支持 bar_time='end'（缺少周期信息），"
                f"请改用 {BAR_TIME_START!r}"
            )
        check_bar_time(bar_time)
        return self._execute(GetExHistoryInstrumentBarsRangeCmd(market, code, start_date, end_date))

    # ------------------------------------------------------------------ #
    # 分时
    # ------------------------------------------------------------------ #

    def get_minute_time_data(self, market: int, code: str) -> list[ExMinuteBar]:
        """获取当日分时行情数据。"""

        return self._execute(GetExMinuteTimeDataCmd(market, code))

    def get_history_minute_time_data(self, market: int, code: str, date: int) -> list[ExMinuteBar]:
        """获取历史某日分时行情数据（date: YYYYMMDD）。"""

        return self._execute(GetExHistoryMinuteTimeDataCmd(market, code, date))

    # ------------------------------------------------------------------ #
    # 成交
    # ------------------------------------------------------------------ #

    def get_transaction_data(
        self,
        market: int,
        code: str,
        start: int = 0,
        count: int = 1800,
    ) -> list[ExTransactionRecord]:
        """获取当日分笔成交数据。"""

        return self._execute(GetExTransactionDataCmd(market, code, start, count))

    def get_history_transaction_data(
        self,
        market: int,
        code: str,
        date: int,
        start: int = 0,
        count: int = 1800,
    ) -> list[ExTransactionRecord]:
        """获取历史某日分笔成交数据（date: YYYYMMDD）。"""

        return self._execute(GetExHistoryTransactionDataCmd(market, code, date, start, count))


__all__ = ["ExTdxClient"]
