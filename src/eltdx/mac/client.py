"""MAC 协议高层同步 API：:class:`MacClient`。

移植自 easy_tdx ``mac/client.py`` 的 **同步** 子集。为满足 eltdx 零第三方依赖
的硬性要求，所有返回 pandas DataFrame 的方法一律改为返回 ``list[dataclass]``
（或单个 dataclass）。异步版本 ``AsyncMacClient`` 不在本移植范围内。
"""

from __future__ import annotations

from collections.abc import Callable, Sequence
from datetime import date as _date
from types import TracebackType
from typing import TypeVar

from .._bar_time import (
    BAR_TIME_START,
    align_datetime_bars,
    period_to_minutes,
)
from ..exceptions import TransportError
from .adjust import XdxrEvent, apply_forward_adjust, has_bad_prices
from .bitmap import Fields, PresetField
from .commands import (
    BoardListCmd,
    BoardMembersQuotesCmd,
    ChartSamplingCmd,
    FileDownloadCmd,
    FileListCmd,
    GoodsListCmd,
    KlineOffsetCmd,
    ServerInfoCmd,
    SymbolAuctionCmd,
    SymbolBarCmd,
    SymbolBelongBoardCmd,
    SymbolCapitalFlowCmd,
    SymbolInfoCmd,
    SymbolQuotesCmd,
    SymbolTickChartCmd,
    SymbolTransactionCmd,
    TickChartsCmd,
    UnusualCmd,
)
from .enums import Adjust, BoardType, Category, FilterType, Period, SortOrder, SortType
from .hosts import MAC_HOSTS, MAC_PORT, MAC_TIMEOUT
from .models import (
    AuctionItem,
    BelongBoardInfo,
    BoardInfo,
    CapitalFlowData,
    FileMeta,
    GoodsItem,
    KlineOffsetInfo,
    MacBar,
    MacMultiTickChart,
    MacQuoteField,
    MacSymbolInfo,
    MacTickChart,
    MacTransaction,
    ServerSession,
    UnusualItem,
)
from .transport import MacConnection, SocketFactory, ping_mac_all

_T = TypeVar("_T")

_KLINE_PAGE_SIZE = 700
_BOARD_MEMBERS_PAGE_SIZE = 80
_TRANSACTION_PAGE_SIZE = 1000

XdxrProvider = Callable[[int, str], "Sequence[XdxrEvent] | None"]


def convert_board_code(board_symbol: str) -> int:
    """将用户可见的板块代码转换为服务器协议代码。

    转换规则（来自 opentdx ``exchange_board_code``）::

        US0401   -> 30401   (30000 + N)
        HK0283   -> 20283   (20000 + N)
        000686   -> 31686   (31000 + N)
        399372   -> 30372   (N - 399000 + 30000)
        899050   -> 32050   (N - 899000 + 32000)
        880686   -> 20686   (N - 880000 + 20000)
        其他      -> int(N)
    """

    s = board_symbol.strip()
    if s.startswith("US"):
        return 30000 + int(s[2:])
    if s.startswith("HK"):
        return 20000 + int(s[2:])
    if len(s) == 6:
        if s.startswith("88"):
            return int(s) - 880000 + 20000
        if s.startswith("399"):
            return int(s) - 399000 + 30000
        if s.startswith("899"):
            return int(s) - 899000 + 32000
        if s.startswith("000"):
            return 31000 + int(s)
    return int(s)


class MacClient:
    """同步 MAC 协议客户端，支持多主机故障转移。

    使用示例::

        with MacClient(hosts=["121.36.248.138", "123.60.47.136"]) as client:
            bars = client.get_stock_kline(0, "600000", Period.DAILY, count=100)

    Args:
        host: 单个服务器地址（与 ``hosts`` 二选一）。
        port: 服务器端口，默认 :data:`MAC_PORT`。
        timeout: 读写超时（秒），默认 :data:`MAC_TIMEOUT`。
        hosts: 候选服务器地址列表，连接失败时逐个故障转移。
        socket_factory: 测试用 socket 工厂（默认 :func:`socket.socket`）。
        xdxr_provider: 可选 XDXR（除权除息）数据提供回调
            ``(market, code) -> Sequence[XdxrEvent] | None``，用于 QFQ 负价兜底。
    """

    def __init__(
        self,
        host: str | None = None,
        port: int | None = None,
        timeout: float | None = None,
        hosts: Sequence[str] | None = None,
        *,
        socket_factory: SocketFactory | None = None,
        xdxr_provider: XdxrProvider | None = None,
    ) -> None:
        if hosts is not None:
            self._hosts = [str(h) for h in hosts]
        elif host is not None:
            self._hosts = [host]
        else:
            self._hosts = list(MAC_HOSTS)
        if not self._hosts:
            raise ValueError("至少需要一个 host")
        self._port = port if port is not None else MAC_PORT
        self._timeout = timeout if timeout is not None else MAC_TIMEOUT
        self._socket_factory = socket_factory
        self._xdxr_provider = xdxr_provider
        self._host_index = 0
        self._conn: MacConnection | None = None

    # ------------------------------------------------------------------ #
    # 连接管理
    # ------------------------------------------------------------------ #

    @property
    def host(self) -> str:
        """当前选中的服务器地址。"""

        return self._hosts[self._host_index]

    @property
    def hosts(self) -> list[str]:
        """候选服务器地址列表（只读副本）。"""

        return list(self._hosts)

    @staticmethod
    def ping_all(
        hosts: Sequence[str] | None = None,
        port: int | None = None,
        timeout: float = 5.0,
    ) -> list[tuple[str, float]]:
        """并发测量多台 MAC 服务器延迟，返回按延迟升序的 ``(host, seconds)``。

        与 :meth:`from_best_host` 一样，要求握手后能真正拿到一条可解析的
        服务器信息响应，因此端口开放但非 MAC 行情的主机不会被计入。
        """

        return ping_mac_all(hosts, port if port is not None else MAC_PORT, timeout)

    @classmethod
    def from_best_host(
        cls,
        hosts: Sequence[str] | None = None,
        port: int | None = None,
        timeout: float | None = None,
        ping_timeout: float = 5.0,
    ) -> MacClient:
        """测量所有 MAC 服务器延迟，选最低延迟的建立客户端。

        最佳主机被放在候选列表**首位**（``MacClient`` 的 ``hosts`` 参数优先于
        ``host``，只传 ``host`` 不会改变当前主机），其余候选仍保留用于故障转移。

        注意：``easy_tdx`` 的同名方法还接受 ``auto_reconnect`` /
        ``heartbeat_interval``；本客户端的 :class:`~eltdx.mac.transport.MacConnection`
        没有自动重连与心跳特性，故未提供这两个参数——给出会被静默忽略的
        参数比缺少它更容易误导。
        """

        ranked = ping_mac_all(hosts, port if port is not None else MAC_PORT, ping_timeout)
        candidates = [str(h) for h in hosts] if hosts else list(MAC_HOSTS)
        best = ranked[0][0] if ranked else (candidates[0] if candidates else None)
        if best is None:
            raise ValueError("hosts 不能为空")
        ordered = [best] + [h for h in candidates if h != best]
        return cls(best, port, timeout, hosts=ordered)

    def connect(self) -> None:
        """连接服务器：按候选列表逐个尝试，成功即记录为当前主机。

        重复调用是安全的：新连接建立成功后才关闭旧连接（见下），所以既不会
        泄漏上一条 socket，也不会出现「重连失败反而把可用连接关掉」的窗口。
        """

        last_exc: TransportError | None = None
        for offset in range(len(self._hosts)):
            idx = (self._host_index + offset) % len(self._hosts)
            conn = MacConnection(
                self._hosts[idx],
                self._port,
                self._timeout,
                socket_factory=self._socket_factory,
            )
            try:
                conn.connect()
            except TransportError as exc:
                last_exc = exc
                conn.close()
                continue
            # 接管新连接，并释放被替换掉的旧连接。
            # 顺序很关键：先让 self._conn 指向新连接，再关旧的 —— 旧连接必须在
            # 新连接确认可用之后才关，否则重连失败会把还能用的连接弄丢。
            previous = self._conn
            self._host_index = idx
            self._conn = conn
            if previous is not None and previous is not conn:
                previous.close()
            return
        raise last_exc or TransportError("无可用 MAC 服务器")

    def close(self) -> None:
        """关闭当前连接（幂等）。"""

        if self._conn is not None:
            self._conn.close()
            self._conn = None

    def disconnect(self) -> None:
        """``close()`` 的别名。"""

        self.close()

    def ensure_connected(self) -> None:
        """验证连接存活，断线则自动重建。

        与 ``easy_tdx`` 一致，用一条极廉价的 :class:`KlineOffsetCmd` 探测而不是
        只看标志位——标志位无法发现「TCP 还在但服务端已经不理人」的半开连接。
        未连接时 :meth:`_execute` 会先建立连接；传输失败时它会把当前主机切到
        候选列表的下一台并重试一次，因此这里不需要单独的 ``_reconnect``。
        """

        self._execute(KlineOffsetCmd(0, 1))

    def __enter__(self) -> "MacClient":
        self.connect()
        return self

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        exc_tb: TracebackType | None,
    ) -> None:
        self.close()

    def _execute(self, cmd) -> object:
        """执行命令；传输失败时切换到下一台主机重试一次。"""

        if self._conn is None:
            self.connect()
        assert self._conn is not None
        try:
            return self._conn.execute(cmd)
        except TransportError:
            self.close()
            self._host_index = (self._host_index + 1) % len(self._hosts)
            self.connect()
            assert self._conn is not None
            return self._conn.execute(cmd)

    # ------------------------------------------------------------------ #
    # 报价
    # ------------------------------------------------------------------ #

    def get_stock_quotes(
        self,
        stocks: list[tuple[int, str]],
        fields: Fields | None = None,
    ) -> list[MacQuoteField]:
        """批量获取自定义字段报价（最多 80 只/次）。

        Args:
            stocks: ``[(market, code), ...]`` 列表。
            fields: 字段选择，默认 :attr:`PresetField.COMMON`。
        """

        return self._execute(SymbolQuotesCmd(stocks, fields))  # type: ignore[return-value]

    def get_stock_quotes_list(
        self,
        category: Category,
        start: int = 0,
        count: int = 80,
        sort_type: SortType = SortType.CHANGE_PCT,
        sort_order: SortOrder = SortOrder.DESC,
        exclude_flags: list[FilterType] | None = None,
        fields: Fields | None = None,
    ) -> list[MacQuoteField]:
        """获取市场分类报价列表（自动分页）。

        Args:
            category: 市场分类（如 ``Category.A`` / ``Category.SH``）。
            start: 起始偏移。
            count: 请求总数。
            sort_type: 排序字段。
            sort_order: 排序方向。
            exclude_flags: 过滤标志列表。
            fields: 请求字段集合，默认 ``PresetField.BASIC + PresetField.VOLUME``。
        """

        if fields is None:
            fields = PresetField.BASIC + PresetField.VOLUME
        all_quotes: list[MacQuoteField] = []
        fetched = 0
        offset = start

        while fetched < count:
            # 每页按**剩余条数**请求：固定用 _BOARD_MEMBERS_PAGE_SIZE 会让最后一页
            # 超量（count=100 → 两页各 80 → 返回 160 条）。
            page_size = min(count - fetched, _BOARD_MEMBERS_PAGE_SIZE)
            batch = self._execute(
                BoardMembersQuotesCmd(
                    board_code=int(category),
                    sort_type=sort_type,
                    start=offset,
                    page_size=page_size,
                    sort_order=sort_order,
                    fields=fields,
                    exclude_flags=exclude_flags,
                )
            )
            if not batch:
                break
            # 服务端已按 sort_type/sort_order 排好序，跨页直接顺序拼接。
            # 注意不要像 _fetch_kline_pages 那样前插 —— 那是为 K 线倒序返回准备的。
            all_quotes.extend(batch)
            fetched += len(batch)
            offset += len(batch)
            if len(batch) < page_size:
                break

        return all_quotes

    # ------------------------------------------------------------------ #
    # K 线（支持复权）
    # ------------------------------------------------------------------ #

    def _fetch_kline_pages(
        self,
        market: int,
        code: str,
        period: Period,
        start: int,
        count: int,
        times: int,
        fq: Adjust,
    ) -> list[MacBar]:
        """分页拉取指定复权类型的 K 线（返回 oldest -> newest 的 MacBar 列表）。"""

        all_bars: list[MacBar] = []
        fetched = 0
        offset = start
        while fetched < count:
            page_size = min(count - fetched, _KLINE_PAGE_SIZE)
            bars = self._execute(
                SymbolBarCmd(
                    market=market,
                    code=code,
                    period=period,
                    times=times,
                    start=offset,
                    count=page_size,
                    fq=fq,
                )
            )
            if not bars:
                break
            all_bars = bars + all_bars
            fetched += len(bars)
            offset += len(bars)
            if len(bars) < page_size:
                break
        return all_bars

    def _local_recompute_qfq(
        self,
        bars: list[MacBar],
        market: int,
        code: str,
    ) -> list[MacBar] | None:
        """用 NONE + XDXR 本地重算前复权（服务端 QFQ 负价时的兜底）。

        XDXR 由 :attr:`_xdxr_provider` 提供；未配置、无事件或重算后仍含非法
        价格时返回 ``None``，调用方降级返回服务端原始结果。
        """

        if self._xdxr_provider is None:
            return None
        events = self._xdxr_provider(market, code)
        if not events:
            return None
        out = apply_forward_adjust(bars, events)
        if has_bad_prices(out):
            return None
        return out

    def get_stock_kline(
        self,
        market: int,
        code: str,
        period: Period = Period.DAILY,
        start: int = 0,
        count: int = 800,
        times: int = 1,
        adjust: Adjust = Adjust.NONE,
        *,
        bar_time: str = BAR_TIME_START,
    ) -> list[MacBar]:
        """获取 K 线数据（自动分页，每页最多 700 条）。

        Args:
            market: 市场代码。
            code: 股票代码。
            period: K 线周期。
            start: 起始偏移（0 = 最新）。
            count: 总请求条数。
            times: 周期倍数（``Period.MINS`` / ``Period.DAYS`` 时有效）。
            adjust: 复权方式。当 QFQ 结果出现负价/零价且配置了
                :attr:`_xdxr_provider` 时，用 NONE + XDXR 本地重算兜底。
            bar_time: 时间戳语义。``"start"``（默认）= bar 开始时间（通达信原始，
                上午最后一根 5min 标 ``11:25``、下午第一根标 ``13:00``）；``"end"`` =
                bar 右端点（= 开始 + 周期时长，与 Tushare/同花顺对齐，上午最后一根标
                ``11:30``）。仅对分钟级周期生效；日线及以上不受影响。
        """

        bars = self._fetch_kline_pages(market, code, period, start, count, times, adjust)

        if adjust == Adjust.QFQ and bars and has_bad_prices(bars):
            none_bars = self._fetch_kline_pages(
                market, code, period, start, count, times, Adjust.NONE
            )
            if none_bars:
                recomputed = self._local_recompute_qfq(none_bars, market, code)
                if recomputed is not None:
                    bars = recomputed

        return align_datetime_bars(
            bars,
            delta_minutes=period_to_minutes(period, times),
            bar_time=bar_time,
        )

    # ------------------------------------------------------------------ #
    # 分时
    # ------------------------------------------------------------------ #

    def get_tick_chart(
        self,
        market: int,
        code: str,
        date: int | None = None,
    ) -> MacTickChart:
        """获取单日分时图。

        Args:
            market: 市场代码。
            code: 股票代码。
            date: 查询日期（``YYYYMMDD`` 整数），``None`` 表示今天。
        """

        return self._execute(SymbolTickChartCmd(market, code, _ymd_to_date(date)))

    def get_tick_charts(
        self,
        market: int,
        code: str,
        date: int | None = None,
        days: int = 5,
    ) -> MacMultiTickChart:
        """获取多日分时图（最多 5 天）。

        Args:
            market: 市场代码。
            code: 股票代码。
            date: 起始日期（``YYYYMMDD``），``None`` 表示从最新交易日开始。
            days: 天数。
        """

        return self._execute(TickChartsCmd(market, code, _ymd_to_date(date), days))

    def get_chart_sampling(self, market: int, code: str) -> list[float]:
        """获取分时缩略采样价格点（240 个点）。"""

        return self._execute(ChartSamplingCmd(market, code))

    # ------------------------------------------------------------------ #
    # 逐笔成交
    # ------------------------------------------------------------------ #

    def get_transactions(
        self,
        market: int,
        code: str,
        count: int = 2000,
        start: int = 0,
        date: int | None = None,
    ) -> list[MacTransaction]:
        """获取逐笔成交数据（自动分页）。

        Args:
            market: 市场代码。
            code: 股票代码。
            count: 请求总数。
            start: 起始偏移。
            date: 查询日期（``YYYYMMDD``），``None`` 表示今天。
        """

        query_date = _ymd_to_date(date)
        all_items = self._execute(
            SymbolTransactionCmd(
                market, code, query_date, start, min(count, _TRANSACTION_PAGE_SIZE)
            )
        )
        fetched = len(all_items)
        offset = start + fetched
        while fetched < count:
            page_size = min(count - fetched, _TRANSACTION_PAGE_SIZE)
            batch = self._execute(
                SymbolTransactionCmd(market, code, query_date, offset, page_size)
            )
            if not batch:
                break
            all_items.extend(batch)
            fetched += len(batch)
            offset += len(batch)
            if len(batch) < page_size:
                break
        return all_items

    # ------------------------------------------------------------------ #
    # 个股信息
    # ------------------------------------------------------------------ #

    def get_symbol_info(self, market: int, code: str) -> MacSymbolInfo:
        """获取个股简要特征快照。"""

        return self._execute(SymbolInfoCmd(market, code))

    # ------------------------------------------------------------------ #
    # 板块
    # ------------------------------------------------------------------ #

    def get_board_list(
        self,
        board_type: BoardType = BoardType.ALL,
        count: int = 10000,
    ) -> list[BoardInfo]:
        """获取板块列表（自动分页）。

        Args:
            board_type: 板块类型。
            count: 请求总数。
        """

        all_items = self._execute(BoardListCmd(board_type, 0, min(count, 150)))
        fetched = len(all_items)
        offset = fetched
        while fetched < count:
            page_size = min(count - fetched, 150)
            batch = self._execute(BoardListCmd(board_type, offset, page_size))
            if not batch:
                break
            all_items.extend(batch)
            fetched += len(batch)
            offset += len(batch)
            if len(batch) < page_size:
                break
        return all_items

    def get_board_members(
        self,
        board_symbol: str,
        count: int = 100000,
        sort_type: SortType = SortType.CHANGE_PCT,
        sort_order: SortOrder = SortOrder.DESC,
        fields: Fields = PresetField.COMMON,
        exclude_flags: list[FilterType] | None = None,
    ) -> list[MacQuoteField]:
        """获取板块成分股报价（自动分页）。

        Args:
            board_symbol: 板块代码（如 ``"881001"``）。
            count: 请求总数。
            sort_type: 排序字段。
            sort_order: 排序方向。
            fields: 字段选择。
            exclude_flags: 过滤标志列表。
        """

        board_code = convert_board_code(board_symbol)
        all_quotes: list[MacQuoteField] = []
        fetched = 0
        offset = 0
        while fetched < count:
            page_size = min(count - fetched, _BOARD_MEMBERS_PAGE_SIZE)
            batch = self._execute(
                BoardMembersQuotesCmd(
                    board_code=board_code,
                    sort_type=sort_type,
                    start=offset,
                    page_size=page_size,
                    sort_order=sort_order,
                    fields=fields,
                    exclude_flags=exclude_flags,
                )
            )
            if not batch:
                break
            # 顺序拼接，保留服务端的跨页排序（整页前插会把第 2 页排到最前）。
            all_quotes.extend(batch)
            fetched += len(batch)
            offset += len(batch)
            if len(batch) < page_size:
                break
        return all_quotes

    def get_belong_board(self, market: int, code: str) -> list[BelongBoardInfo]:
        """获取个股所属板块列表。"""

        return self._execute(SymbolBelongBoardCmd(market, code))

    def get_board_summary(
        self,
        board_symbol: str,
        sort_type: SortType = SortType.CHANGE_PCT,
        sort_order: SortOrder = SortOrder.DESC,
    ) -> dict[str, object]:
        """获取板块汇总（聚合成分股数据）。

        基于 :meth:`get_board_members` 获取全部成分股报价，对成交额与资金流
        字段求和。返回字典键：``member_count`` / ``amount`` / ``vol`` /
        ``main_net_amount`` / ``main_net_3d`` / ``main_net_5d`` / ``up_count`` /
        ``down_count`` / ``members``（成分股 :class:`MacQuoteField` 列表）。
        """

        from .bitmap import FieldBit

        fields = (
            PresetField.BASIC
            + FieldBit.AMOUNT
            + FieldBit.MAIN_NET_AMOUNT
            + FieldBit.MAIN_NET_3D_AMOUNT
            + FieldBit.MAIN_NET_5D_AMOUNT
        )
        members = self.get_board_members(
            board_symbol,
            sort_type=sort_type,
            sort_order=sort_order,
            fields=fields,
        )

        def _f(field: str) -> float:
            return float(sum(float(q.fields.get(field, 0.0) or 0.0) for q in members))

        amount = _f("amount")
        main_net_amount = _f("main_net_amount")
        main_net_3d = _f("main_net_3d_amount")
        main_net_5d = _f("main_net_5d_amount")
        vol = int(sum(int(q.fields.get("vol", 0) or 0) for q in members))
        up_count = sum(
            1
            for q in members
            if float(q.fields.get("close", 0.0) or 0.0)
            > float(q.fields.get("pre_close", 0.0) or 0.0)
        )
        down_count = sum(
            1
            for q in members
            if float(q.fields.get("close", 0.0) or 0.0)
            < float(q.fields.get("pre_close", 0.0) or 0.0)
        )

        return {
            "member_count": len(members),
            "amount": amount,
            "vol": vol,
            "main_net_amount": main_net_amount,
            "main_net_3d": main_net_3d,
            "main_net_5d": main_net_5d,
            "up_count": up_count,
            "down_count": down_count,
            "members": members,
        }

    def get_board_ranking(
        self,
        board_type: BoardType = BoardType.HY,
        top_n: int = 50,
        sort_by: str = "change_pct",
        ascending: bool = False,
    ) -> list[dict[str, object]]:
        """获取板块排行榜（含成交额、成交量、资金净流入、涨跌家数）。

        先通过 :meth:`get_board_list` 获取全部板块，再按**用户指定的指标**排序，
        取前 ``top_n`` 后逐个调用 :meth:`get_board_summary` 聚合成分股数据。

        !!! warning "必须先按目标指标排序再截断"
            早期实现先用 ``change_pct`` 排一遍、截取 ``top_n``，之后才按
            ``sort_by`` 重排。于是「成交额最高但涨幅较低」的板块在第一轮就被丢掉，
            永远进不了 ``sort_by="amount"`` 的结果 —— 排行给出的不是目标指标的
            前 N 名。

        Args:
            board_type: 板块类型。
            top_n: 返回条数。
            sort_by: 排序指标（``change_pct`` / ``amount`` / ``vol`` /
                ``main_net_amount``）。
            ascending: 是否升序。

        Note:
            ``sort_by="change_pct"`` 时涨跌幅直接来自 :meth:`get_board_list`，
            可以先截断再聚合（少调 ``N - top_n`` 次 summary）；
            其余指标必须**先聚合全部板块**才能排序，请求数会明显增加 ——
            这是拿到正确排名的必要代价，不是可以优化掉的。
        """

        valid_sort = {"change_pct", "amount", "main_net_amount", "vol"}
        if sort_by not in valid_sort:
            raise ValueError(f"sort_by 必须是 {valid_sort} 之一， got {sort_by!r}")

        boards = self.get_board_list(board_type)
        if not boards:
            return []

        change_by_code: dict[str, float] = {}
        for board in boards:
            pre = board.pre_close if board.pre_close else float("nan")
            change_by_code[board.code] = (
                (board.price - board.pre_close) / pre * 100 if pre == pre else 0.0
            )

        rows: list[dict[str, object]] = []
        if sort_by == "change_pct":
            # 涨跌幅不需要额外请求即可排序：先截断再聚合，避免无谓的网络开销。
            ordered = sorted(
                boards,
                key=lambda b: change_by_code[b.code],
                reverse=not ascending,
            )[:top_n]
            rows = [self._board_row(board, change_by_code[board.code]) for board in ordered]
            return rows

        # 其余指标只有聚合成分股后才知道，必须对**全部**板块求值再排序。
        rows = [self._board_row(board, change_by_code[board.code]) for board in boards]
        rows.sort(key=lambda row: float(row.get(sort_by, 0.0) or 0.0), reverse=not ascending)
        return rows[:top_n]

    def _board_row(self, board: BoardInfo, change_pct: float) -> dict[str, object]:
        """聚合单个板块的成分股数据，产出排行榜的一行。"""

        summary = self.get_board_summary(board.code)
        return {
            "code": board.code,
            "name": board.name,
            "change_pct": round(change_pct, 2),
            "amount": summary["amount"],
            "vol": summary["vol"],
            "main_net_amount": summary["main_net_amount"],
            "up_count": summary["up_count"],
            "down_count": summary["down_count"],
            "member_count": summary["member_count"],
        }

    def get_board_change_ranking(
        self,
        board_type: BoardType = BoardType.HY,
        target_date: int | None = None,
        days: int = 20,
        top_n: int | None = None,
        ascending: bool = False,
    ) -> list[dict[str, object]]:
        """获取板块 N 日涨跌幅排行榜。

        对每个板块获取日 K 线，计算指定日期前 N 个交易日的涨跌幅并排行。
        """

        if days < 1:
            raise ValueError(f"days 必须 >= 1，got {days}")

        boards = self.get_board_list(board_type)
        if not boards:
            return []

        fetch_count = days + 10  # 缓冲节假日
        target = _ymd_to_date(target_date)

        rows: list[dict[str, object]] = []
        for board in boards:
            try:
                bars = self.get_stock_kline(
                    market=board.market,
                    code=board.code,
                    period=Period.DAILY,
                    count=fetch_count,
                    adjust=Adjust.NONE,
                )
            except TransportError:
                continue
            bars = sorted(bars, key=lambda bar: bar.datetime)
            if len(bars) < 2:
                continue

            if target is not None:
                eligible = [i for i, bar in enumerate(bars) if bar.datetime.date() <= target]
                if not eligible:
                    continue
                end_pos = eligible[-1]
            else:
                end_pos = len(bars) - 1

            start_pos = max(0, end_pos - days)
            close_end = bars[end_pos].close
            close_start = bars[start_pos].close
            if close_start == 0:
                continue

            rows.append(
                {
                    "code": board.code,
                    "name": board.name,
                    "close_end": close_end,
                    "close_start": close_start,
                    "change_pct": round((close_end - close_start) / close_start * 100, 2),
                }
            )

        rows.sort(key=lambda row: float(row["change_pct"]), reverse=not ascending)
        if top_n is not None:
            rows = rows[:top_n]
        return rows

    # ------------------------------------------------------------------ #
    # 资金流向 / 集合竞价 / 异动
    # ------------------------------------------------------------------ #

    def get_capital_flow(self, market: int, code: str) -> CapitalFlowData | None:
        """获取个股资金流向（无数据时返回 ``None``）。"""

        return self._execute(SymbolCapitalFlowCmd(market, code))

    def get_auction(self, market: int, code: str) -> list[AuctionItem]:
        """获取集合竞价数据。"""

        return self._execute(SymbolAuctionCmd(market, code))

    def get_unusual(
        self,
        market: int,
        start: int = 0,
        count: int = 0,
    ) -> list[UnusualItem]:
        """获取市场异动数据（``count=0`` 时使用默认值 600）。"""

        return self._execute(UnusualCmd(market, start, count or 600))

    # ------------------------------------------------------------------ #
    # 服务器信息 / K 线偏移
    # ------------------------------------------------------------------ #

    def get_server_info(self) -> ServerSession:
        """获取服务器交易时段信息。"""

        return self._execute(ServerInfoCmd())

    def get_kline_offset(
        self,
        offset: int = 0,
        count: int = 128000,
    ) -> KlineOffsetInfo:
        """获取 K 线数据偏移信息。"""

        return self._execute(KlineOffsetCmd(offset, count))

    # ------------------------------------------------------------------ #
    # 文件操作
    # ------------------------------------------------------------------ #

    def get_file_meta(self, filename: str) -> FileMeta:
        """查询远程文件元信息。"""

        return self._execute(FileListCmd(filename))

    def download_file_chunk(
        self,
        filename: str,
        index: int,
        offset: int,
        size: int,
    ) -> bytes:
        """下载远程文件的一个分片。"""

        return self._execute(FileDownloadCmd(filename, index, offset, size))

    def download_file(self, filename: str, filesize: int = 0) -> bytearray:
        """下载完整远程文件（``filesize=0`` 时自动探测大小）。"""

        if filesize <= 0:
            filesize = self._execute(FileListCmd(filename)).size

        full_data = bytearray()
        chunk_size = 30000
        pos = 0
        idx = 1
        while pos < filesize:
            chunk = self._execute(FileDownloadCmd(filename, idx, pos, chunk_size))
            if not chunk:
                break
            full_data.extend(chunk)
            pos += len(chunk)
            idx += 1
            if len(chunk) < chunk_size:
                break
        return full_data

    # ------------------------------------------------------------------ #
    # 扩展市场
    # ------------------------------------------------------------------ #

    def get_goods_list(
        self,
        market: int,
        start: int = 0,
        count: int = 600,
    ) -> list[GoodsItem]:
        """获取扩展市场（期货/期权等）商品列表。"""

        return self._execute(GoodsListCmd(market, start, count))

    # ------------------------------------------------------------------ #
    # easy_tdx 语义别名
    # ------------------------------------------------------------------ #

    get_stock_minute_time = get_tick_chart
    get_stock_minute_time_hist = get_tick_charts
    get_stock_transaction = get_transactions


def _ymd_to_date(value: int | None) -> _date | None:
    """将 ``YYYYMMDD`` 整数转换为 :class:`datetime.date`（``None`` 透传）。"""

    if value is None:
        return None
    return _date(value // 10000, (value % 10000) // 100, value % 100)


__all__ = ["MacClient", "convert_board_code"]