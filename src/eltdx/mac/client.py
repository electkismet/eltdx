"""MAC 协议高层同步 API：:class:`MacClient`。

移植自 easy_tdx ``mac/client.py`` 的 **同步** 子集。为满足 eltdx 零第三方依赖
的硬性要求，所有返回 pandas DataFrame 的方法一律改为返回 ``list[dataclass]``
（或单个 dataclass）。异步版本 ``AsyncMacClient`` 不在本移植范围内。
"""

from __future__ import annotations

from collections.abc import Callable, Sequence
from datetime import date as _date
from sys import modules as _MODULES
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


def _native_connect_attempt(client, domain, target):
    """Only connect failures are caught; the caller constructs the connection."""
    from eltdx._native_abi import load_native

    native = load_native()
    try:
        conn = native.extension_client_connect_one(domain, client, target, _MODULES[__name__])
    except TransportError as exc:
        native.extension_client_connect_failed(domain, client, target, exc, _MODULES[__name__])
        return False, exc
    return True, conn


def _native_execute_attempt(client, domain, command):
    from eltdx._native_abi import load_native

    try:
        result = client._conn.execute(command)
    except TransportError as exc:
        return load_native().extension_client_recover(
            client=client, domain=domain, command=command, cause=exc,
            module=_MODULES[__name__], assert_enabled=__debug__,
        )
    return True, result, False


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

    from eltdx._native_abi import load_native

    return load_native().mac_board_code(board_symbol, _MODULES[__name__])


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
        from eltdx._native_abi import load_native

        load_native().extension_client_init(
            "mac", self, (host, port, timeout, hosts, socket_factory, xdxr_provider), _MODULES[__name__]
        )

    # ------------------------------------------------------------------ #
    # 连接管理
    # ------------------------------------------------------------------ #

    @property
    def host(self) -> str:
        """当前选中的服务器地址。"""

        from eltdx._native_abi import load_native

        return load_native().extension_client_state("mac", "host", self, _MODULES[__name__])

    @property
    def hosts(self) -> list[str]:
        """候选服务器地址列表（只读副本）。"""

        from eltdx._native_abi import load_native

        return load_native().extension_client_state("mac", "hosts", self, _MODULES[__name__])

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

        from eltdx._native_abi import load_native

        return load_native().extension_client_ping("mac", (hosts, port, timeout), _MODULES[__name__])

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

        from eltdx._native_abi import load_native

        return load_native().extension_client_best(
            "mac", cls, (hosts, port, timeout, ping_timeout), _MODULES[__name__]
        )

    def connect(self) -> None:
        """连接服务器：按候选列表逐个尝试，成功即记录为当前主机。

        重复调用是安全的：新连接建立成功后才关闭旧连接（见下），所以既不会
        泄漏上一条 socket，也不会出现「重连失败反而把可用连接关掉」的窗口。
        """

        from eltdx._native_abi import load_native

        load_native().extension_client_connect("mac", self, _MODULES[__name__])

    def close(self) -> None:
        """关闭当前连接（幂等）。"""

        from eltdx._native_abi import load_native

        load_native().extension_client_state("mac", "close", self, _MODULES[__name__])

    def disconnect(self) -> None:
        """``close()`` 的别名。"""

        from eltdx._native_abi import load_native

        load_native().extension_client_state("mac", "disconnect", self, _MODULES[__name__])

    def ensure_connected(self) -> None:
        """验证连接存活，断线则自动重建。

        与 ``easy_tdx`` 一致，用一条极廉价的 :class:`KlineOffsetCmd` 探测而不是
        只看标志位——标志位无法发现「TCP 还在但服务端已经不理人」的半开连接。
        未连接时 :meth:`_execute` 会先建立连接；传输失败时它会把当前主机切到
        候选列表的下一台并重试一次，因此这里不需要单独的 ``_reconnect``。
        """

        from eltdx._native_abi import load_native

        load_native().extension_client_state("mac", "ensure", self, _MODULES[__name__])

    def __enter__(self) -> "MacClient":
        from eltdx._native_abi import load_native

        return load_native().extension_client_state("mac", "enter", self, _MODULES[__name__])

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        exc_tb: TracebackType | None,
    ) -> None:
        from eltdx._native_abi import load_native

        load_native().extension_client_state("mac", "exit", self, _MODULES[__name__])

    def _execute(self, cmd) -> object:
        """执行命令；传输失败时切换到下一台主机重试一次。"""

        from eltdx._native_abi import load_native

        return load_native().extension_client_execute("mac", self, cmd, _MODULES[__name__], __debug__)

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

        from eltdx._native_abi import load_native

        return load_native().extension_single(self, "mac", "quotes", (stocks, fields), None, _MODULES[__name__])

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

        from eltdx._native_abi import load_native

        return load_native().mac_page_query(
            self, "quotes", (category, start, count, sort_type, sort_order, exclude_flags, fields),
            _MODULES[__name__],
        )

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

        from eltdx._native_abi import load_native

        return load_native().mac_page_query(
            self, "bars", (market, code, period, start, count, times, fq), _MODULES[__name__]
        )

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

        from eltdx._native_abi import load_native

        return load_native().mac_local_recompute(self, bars, market, code, _MODULES[__name__])

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

        from eltdx._native_abi import load_native

        return load_native().mac_stock_kline(
            self, (market, code, period, start, count, times, adjust), bar_time, _MODULES[__name__]
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

        from eltdx._native_abi import load_native

        return load_native().extension_single(self, "mac", "tick", (market, code, date), None, _MODULES[__name__])

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

        from eltdx._native_abi import load_native

        return load_native().extension_single(self, "mac", "ticks", (market, code, date, days), None, _MODULES[__name__])

    def get_chart_sampling(self, market: int, code: str) -> list[float]:
        """获取分时缩略采样价格点（240 个点）。"""

        from eltdx._native_abi import load_native

        return load_native().extension_single(self, "mac", "sampling", (market, code), None, _MODULES[__name__])

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

        from eltdx._native_abi import load_native

        return load_native().mac_page_query(
            self, "transactions", (market, code, count, start, date), _MODULES[__name__]
        )

    # ------------------------------------------------------------------ #
    # 个股信息
    # ------------------------------------------------------------------ #

    def get_symbol_info(self, market: int, code: str) -> MacSymbolInfo:
        """获取个股简要特征快照。"""

        from eltdx._native_abi import load_native

        return load_native().extension_single(self, "mac", "symbol", (market, code), None, _MODULES[__name__])

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

        from eltdx._native_abi import load_native

        return load_native().mac_page_query(self, "boards", (board_type, count), _MODULES[__name__])

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

        from eltdx._native_abi import load_native

        return load_native().mac_page_query(
            self, "members", (board_symbol, count, sort_type, sort_order, fields, exclude_flags),
            _MODULES[__name__],
        )

    def get_belong_board(self, market: int, code: str) -> list[BelongBoardInfo]:
        """获取个股所属板块列表。"""

        from eltdx._native_abi import load_native

        return load_native().extension_single(self, "mac", "belong", (market, code), None, _MODULES[__name__])

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
        from eltdx._native_abi import load_native

        return load_native().mac_board_summary(
            self, board_symbol, sort_type, sort_order, FieldBit,
            lambda values: (value for value in values), _MODULES[__name__],
        )

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

        from eltdx._native_abi import load_native

        return load_native().mac_board_ranking(
            self, (board_type, top_n, sort_by, ascending),
            lambda changes: lambda board: changes[board.code],
            lambda project: lambda value: project(value), _MODULES[__name__],
        )

    def _board_row(self, board: BoardInfo, change_pct: float) -> dict[str, object]:
        """聚合单个板块的成分股数据，产出排行榜的一行。"""

        from eltdx._native_abi import load_native

        return load_native().mac_board_row(self, board, change_pct, _MODULES[__name__])

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

        from eltdx._native_abi import load_native

        return load_native().mac_board_change_ranking(
            self, (board_type, target_date, days, top_n, ascending),
            lambda bar: bar.datetime, lambda project: lambda value: project(value),
            _MODULES[__name__],
        )

    # ------------------------------------------------------------------ #
    # 资金流向 / 集合竞价 / 异动
    # ------------------------------------------------------------------ #

    def get_capital_flow(self, market: int, code: str) -> CapitalFlowData | None:
        """获取个股资金流向（无数据时返回 ``None``）。"""

        from eltdx._native_abi import load_native

        return load_native().extension_single(self, "mac", "capital", (market, code), None, _MODULES[__name__])

    def get_auction(self, market: int, code: str) -> list[AuctionItem]:
        """获取集合竞价数据。"""

        from eltdx._native_abi import load_native

        return load_native().extension_single(self, "mac", "auction", (market, code), None, _MODULES[__name__])

    def get_unusual(
        self,
        market: int,
        start: int = 0,
        count: int = 0,
    ) -> list[UnusualItem]:
        """获取市场异动数据（``count=0`` 时使用默认值 600）。"""

        from eltdx._native_abi import load_native

        return load_native().extension_single(self, "mac", "unusual", (market, start, count), None, _MODULES[__name__])

    # ------------------------------------------------------------------ #
    # 服务器信息 / K 线偏移
    # ------------------------------------------------------------------ #

    def get_server_info(self) -> ServerSession:
        """获取服务器交易时段信息。"""

        from eltdx._native_abi import load_native

        return load_native().extension_single(self, "mac", "server", (), None, _MODULES[__name__])

    def get_kline_offset(
        self,
        offset: int = 0,
        count: int = 128000,
    ) -> KlineOffsetInfo:
        """获取 K 线数据偏移信息。"""

        from eltdx._native_abi import load_native

        return load_native().extension_single(self, "mac", "offset", (offset, count), None, _MODULES[__name__])

    # ------------------------------------------------------------------ #
    # 文件操作
    # ------------------------------------------------------------------ #

    def get_file_meta(self, filename: str) -> FileMeta:
        """查询远程文件元信息。"""

        from eltdx._native_abi import load_native

        return load_native().extension_single(self, "mac", "file_meta", (filename,), None, _MODULES[__name__])

    def download_file_chunk(
        self,
        filename: str,
        index: int,
        offset: int,
        size: int,
    ) -> bytes:
        """下载远程文件的一个分片。"""

        from eltdx._native_abi import load_native

        return load_native().extension_single(self, "mac", "file_chunk", (filename, index, offset, size), None, _MODULES[__name__])

    def download_file(self, filename: str, filesize: int = 0) -> bytearray:
        """下载完整远程文件（``filesize=0`` 时自动探测大小）。"""

        from eltdx._native_abi import load_native

        return load_native().mac_download_file(self, filename, filesize, _MODULES[__name__])

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

        from eltdx._native_abi import load_native

        return load_native().extension_single(self, "mac", "goods", (market, start, count), None, _MODULES[__name__])

    # ------------------------------------------------------------------ #
    # easy_tdx 语义别名
    # ------------------------------------------------------------------ #

    get_stock_minute_time = get_tick_chart
    get_stock_minute_time_hist = get_tick_charts
    get_stock_transaction = get_transactions


def _ymd_to_date(value: int | None) -> _date | None:
    """将 ``YYYYMMDD`` 整数转换为 :class:`datetime.date`（``None`` 透传）。"""

    from eltdx._native_abi import load_native

    return load_native().mac_ymd_date(value, _MODULES[__name__])


__all__ = ["MacClient", "convert_board_code"]
