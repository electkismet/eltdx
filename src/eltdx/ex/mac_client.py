"""MAC 协议扩展市场高层同步客户端 MacExClient。"""

from __future__ import annotations

from collections.abc import Iterable
from datetime import date
from sys import modules as _MODULES
from types import TracebackType
from typing import Any, TypeVar

from eltdx.ex._hk_transaction import (
    _fetch_all_hk_transactions_sync,
    _fetch_hk_transactions_sync,
    is_hk_stock_market,
)
from eltdx.ex.commands.base import BaseCommand
from eltdx.ex.commands.get_instrument_count import GetExInstrumentCountCmd
from eltdx.ex.commands.get_instrument_info import GetExInstrumentInfoCmd
from eltdx.ex.commands.login import MacExLoginCmd
from eltdx.ex.hosts import DEFAULT_EX_PORT, MAC_EX_HOSTS
from eltdx.mac.enums import SortOrder, SortType
from eltdx.ex.models import ExInstrumentInfo
from eltdx.ex.mac_commands import (
    DEFAULT_QUOTE_FIELDS,
    Adjust,
    ChartSamplingCmd,
    GoodsListCmd,
    Period,
    SymbolBarCmd,
    SymbolQuotesCmd,
    SymbolTickChartCmd,
    SymbolTransactionCmd,
)
from eltdx.ex.mac_models import (
    GoodsItem,
    MacBar,
    MacQuoteField,
    MacTickChart,
    MacTransaction,
)
from eltdx.ex.transport.sync import ExTdxConnection, ping_ex_all
from eltdx.exceptions import ProtocolError, TransportError
from eltdx.ex.enums import ExMarket
from eltdx.mac.bitmap import FieldBit
from eltdx.mac.codec import build_mac_request

_T = TypeVar("_T")


class MacExClient:
    """同步 MAC 协议扩展市场客户端（期货/港股/美股，端口 7727）。

    使用示例::

        with MacExClient() as c:
            bars = c.goods_kline(ExMarket.CFFEX_FUTURES, "IFL0", Period.DAILY)
            quotes = c.goods_quotes([(ExMarket.HK_MAIN_BOARD, "00700")])

    每次建立连接后都必须先执行 MAC 登录（0x2454），因此 ``connect`` 会同时完成
    TCP 连接与登录；未显式指定 ``host`` 时按 ``hosts``（默认
    :data:`eltdx.ex.hosts.MAC_EX_HOSTS`）顺序故障转移。
    """

    def __init__(
        self,
        host: str | None = None,
        port: int = DEFAULT_EX_PORT,
        timeout: float = 15.0,
        *,
        hosts: list[str] | None = None,
    ) -> None:
        candidates = list(hosts) if hosts else list(MAC_EX_HOSTS)
        if host is not None:
            candidates = [host] + [h for h in candidates if h != host]
        if not candidates:
            raise ValueError("hosts 不能为空")
        self._hosts = candidates
        self._host = host if host is not None else candidates[0]
        self._port = port
        self._timeout = timeout
        self._conn = ExTdxConnection(self._host, port, timeout, mac_ex_mode=True)
        self._connected = False
        self._bounds_cache: dict[int, tuple[int, int]] = {}
        # 宏观指标（市场 38）代码缓存：name -> GoodsItem
        self._macro_cache: dict[str, GoodsItem] = {}

    @classmethod
    def from_best_host(
        cls,
        hosts: list[str] | None = None,
        port: int = DEFAULT_EX_PORT,
        timeout: float = 15.0,
        ping_timeout: float = 5.0,
    ) -> "MacExClient":
        """测量所有 MAC 扩展行情服务器延迟，选最低延迟建立连接。"""

        candidates = list(hosts) if hosts else list(MAC_EX_HOSTS)
        # MAC-EX 会话探活必须用 mac_ex_mode=True + 登录命令，与 connect() 一致；
        # 否则以 EX 模式发 92B 握手 + markets，会把 MAC-EX 主站误判为不可用。
        ranked = ping_ex_all(
            candidates, port, ping_timeout, mac_ex_mode=True, command_factory=MacExLoginCmd
        )
        best = ranked[0][0] if ranked else candidates[0]
        return cls(best, port, timeout, hosts=candidates)

    @staticmethod
    def ping_all(
        hosts: list[str] | None = None,
        port: int = DEFAULT_EX_PORT,
        timeout: float = 5.0,
    ) -> list[tuple[str, float]]:
        """并发测量多台 MAC 扩展行情服务器延迟，按延迟升序返回。

        与 :meth:`connect` 一致，按 MAC-EX 会话探活：``mac_ex_mode=True`` 跳过
        EX 92B 握手，并用登录命令（0x2454）验证主机是否真正可用。
        """

        return ping_ex_all(
            hosts or list(MAC_EX_HOSTS),
            port,
            timeout,
            mac_ex_mode=True,
            command_factory=MacExLoginCmd,
        )

    # ------------------------------------------------------------------ #
    # 连接管理
    # ------------------------------------------------------------------ #

    def connect(self) -> None:
        """建立 TCP 连接并执行 MAC 登录；按主机顺序故障转移。"""

        last_exc: TransportError | None = None
        for host in self._ordered_hosts():
            try:
                self._conn = ExTdxConnection(host, self._port, self._timeout, mac_ex_mode=True)
                self._conn.connect()
                self._conn.execute(MacExLoginCmd())
            except TransportError as e:
                last_exc = e
                continue
            self._host = host
            self._connected = True
            return
        raise last_exc if last_exc is not None else TransportError("没有可用主机")

    def close(self) -> None:
        """关闭连接。"""

        self._conn.close()
        self._connected = False

    def disconnect(self) -> None:
        """关闭连接（别名）。"""

        self.close()

    def ensure_connected(self) -> None:
        """验证连接存活，断线则重建并重新登录。"""

        if not self._connected:
            self.connect()

    def __enter__(self) -> "MacExClient":
        self.connect()
        return self

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        exc_tb: TracebackType | None,
    ) -> None:
        self.close()

    def _ordered_hosts(self) -> list[str]:
        return [self._host] + [h for h in self._hosts if h != self._host]

    def _execute(self, cmd: "BaseCommand[_T]") -> _T:
        """执行命令；连接断开时重建连接 + 重新登录并故障转移一次。"""

        if not self._connected:
            self.connect()
        try:
            return self._conn.execute(cmd)
        except TransportError:
            self._conn.close()
            self._connected = False
            self.connect()
            return self._conn.execute(cmd)

    # ------------------------------------------------------------------ #
    # 商品列表
    # ------------------------------------------------------------------ #

    # ------------------------------------------------------------------ #
    # 宏观指标（市场 38 = ExMarket.MACRO_INDICATOR）
    #
    # 通达信「宏观数据」页（T0002/cloud_dax/HGSJ.sp，GPType=88）的数据源：
    # 指标是市场 38 的特殊证券，代码格式 ``{类别}_{指标}``（如 ``1_GDP``）。
    # 该页面是**客户端原生页面**（非 web），因此 JS 缓存里没有它，
    # 只能走扩展行情协议（7727）。
    # ------------------------------------------------------------------ #

    #: 宏观指标类别 → 中文名（对齐通达信「宏观数据」页左侧分组）。
    MACRO_CATEGORIES: dict[str, str] = {
        "1": "国民经济",
        "2": "物价",
        "3": "景气指数",
        "4": "对外经济",
        "5": "货币金融",
        "6": "房地产与其他",
        "7": "证券市场",
        "8": "美国经济",
        "9": "利率与回购",
    }

    def _macro_code(self, code: str) -> str:
        """规范化宏观指标代码 → ``{类别}_{指标}``（如 ``1_GDP``）。

        已带类别前缀（含 ``_``）时原样返回；裸代码（``GDP``）时在全部指标
        中查找后缀匹配：唯一命中直接用，多个命中抛 ``ValueError`` 列出候选
        （如 ``XAU`` 同时属于类别 5 与 8），无命中抛 ``ProtocolError``。
        """

        from eltdx._native_abi import load_native

        return load_native().mac_ex_macro_code(self, code, _MODULES[__name__])

    def macro_indicators(self) -> list[GoodsItem]:
        """列出全部宏观指标（市场 38，实测 217 个，9 个类别）。

        名称在 ``GoodsItem.name``（``年度国内生产总值`` 等，随报价/列表下发）。
        结果按 ``index`` 排序；重复调用会命中实例缓存（``_macro_cache``）。
        """

        from eltdx._native_abi import load_native

        return load_native().mac_ex_macro_indicators(self, _MODULES[__name__])

    def macro_quotes(
        self,
        codes: "str | Iterable[str]",
        fields: Any = None,
    ) -> list[MacQuoteField]:
        """批量获取宏观指标最新值。

        ``codes`` 支持完整代码（``1_GDP``）或裸代码（``GDP``，自动补类别）。
        返回的 ``MacQuoteField.fields`` 含 ``close``（最新值）、``pre_close``
        （前值）等；宏观指标无成交量（``vol``/``amount`` 恒 0）。

        注意：宏观指标按报告期更新（月/季/年），非交易时段推送，
        ``pre_close`` 通常是上一个报告期的值。
        """

        from eltdx._native_abi import load_native

        return load_native().mac_ex_macro_quotes(self, codes, fields, _MODULES[__name__])

    def macro_kline(
        self,
        code: str,
        period: Period = Period.MONTHLY,
        start: int = 0,
        count: int = 800,
        adjust: Adjust = Adjust.NONE,
    ) -> list[MacBar]:
        """获取宏观指标历史序列（K 线）。

        ``period`` 应与指标发布频率匹配：GDP 用 ``YEARLY``/``QUARTERLY``，
        CPI/PPI/PMI/M2 等用 ``MONTHLY``，利率（SHIBOR/R007）用 ``DAILY``。
        ``MacBar.close`` 即该报告期的指标值；``vol``/``amount`` 恒 0。
        """

        from eltdx._native_abi import load_native

        return load_native().mac_ex_macro_kline(
            self, (code, period, start, count, adjust), _MODULES[__name__],
        )

    def goods_count(self, market: int | None = None) -> int:
        """获取扩展市场商品总数；``market=None`` 返回全市场总数。

        Note:
            有 ``market`` 时优先用 MAC 商品列表命令（0x2562）返回的 ``total``；
            **该命令在部分扩展行情主站上恒返回 0**，此时自动退回 EX 元数据
            （``GetExInstrumentInfoCmd``）二分定位市场边界后相减计数。
        """
        from eltdx._native_abi import load_native

        return load_native().mac_ex_goods_count(self, market, _MODULES[__name__])

    def goods_list(self, market: int, start: int = 0, count: int = 600) -> list[GoodsItem]:
        """获取扩展市场商品列表（自动翻页）。

        Note:
            优先走 MAC 商品列表命令（0x2562）。该命令在部分扩展行情主站上
            **恒返回空列表**（``total=0``），此时自动退回 :meth:`instruments`
            的 EX 元数据路径。回退结果里 ``name`` 字段存的是**代码**（与
            0x2562 协议一致），代码字符串由 ``_first_code`` 取出；需要
            「代码 + 名称」成对数据请直接用 :meth:`instruments`。
        """

        from eltdx._native_abi import load_native

        return load_native().mac_ex_goods_list(self, market, start, count, _MODULES[__name__])

    # ------------------------------------------------------------------ #
    # EX 元数据路径（服务器不支持 0x2562 时的可靠替代）
    # ------------------------------------------------------------------ #

    def _info_page(self, start: int, count: int) -> list[ExInstrumentInfo]:
        from eltdx._native_abi import load_native

        return load_native().extension_single(self, "mac_ex", "info", (start, count), None, _MODULES[__name__])

    def _market_bounds(self, market: int) -> tuple[int, int]:
        """二分定位 ``market`` 在全量扩展行情标的中的半开区间 ``[lo, hi)``。

        扩展行情标的全表按 market 升序排列，因此用「首个 ``>= market``」与
        「首个 ``> market``」两次二分即可得到边界；结果按市场号缓存。
        """

        from eltdx._native_abi import load_native

        return load_native().mac_ex_market_bounds(self, market, _MODULES[__name__])

    def instruments(
        self,
        market: int,
        start: int = 0,
        count: int = 600,
    ) -> list[ExInstrumentInfo]:
        """列出某扩展市场的标的（代码 + 名称），走 EX 元数据协议。

        与 :meth:`goods_list` 的 0x2562 不同，本方法用 ``GetExInstrumentInfoCmd``
        分页并在市场内偏移，因此在**只服务元数据的扩展行情主站**上也能工作，
        且返回真正的 ``code``/``name``/``category``/``desc`` 四个字段。

        典型用途：枚举开放式基金（``market=33``）、货币基金（``34``）等
        无法通过 7709 交易所行情获取的场外品种。

        Args:
            market: 扩展市场编号（``ExMarket``，如 ``33`` 为开放式基金）。
            start: **市场内**起始偏移。
            count: 最多返回条数。
        """

        from eltdx._native_abi import load_native

        return load_native().mac_ex_instruments(self, market, start, count, _MODULES[__name__])

    # ------------------------------------------------------------------ #
    # 行情
    # ------------------------------------------------------------------ #

    def goods_quotes(
        self,
        stocks: list[tuple[int, str]],
        fields: Any = None,
    ) -> list[MacQuoteField]:
        """批量获取扩展市场自定义字段报价。

        ``fields`` 省略时使用 :data:`~eltdx.ex.mac_commands.DEFAULT_QUOTE_FIELDS`
        （7 个基础字段：前收/开/高/低/收/量/额）。
        """

        from eltdx._native_abi import load_native

        return load_native().extension_single(self, "mac_ex", "quotes", (stocks, fields), None, _MODULES[__name__])

    def goods_quotes_list(
        self,
        market: int,
        start: int = 0,
        count: int = 100,
        sort_type: SortType = SortType.CODE,
        sort_order: SortOrder = SortOrder.NONE,
    ) -> list[MacQuoteField]:
        """获取扩展市场排序报价列表（GoodsList + Quotes 组合）。

        ``sort_type`` / ``sort_order`` 允许**位置传参**（与 ``easy_tdx`` 一致），
        例如 ``goods_quotes_list(33, 0, 5, SortType.CHANGE_PCT, SortOrder.DESC)``；
        传普通 ``int`` 同样可用（枚举是 ``IntEnum``）。

        Note:
            ``sort_type`` / ``sort_order`` 目前**仅作预留、不生效**——底层
            ``GoodsListCmd``（0x2562）的请求体只有 ``market/start/count``，
            协议上无法排序。``easy_tdx`` 同样标注为「暂未实现排序，预留接口」。
        """

        from eltdx._native_abi import load_native

        return load_native().mac_ex_quotes_list(self, market, start, count, _MODULES[__name__])

    # ------------------------------------------------------------------ #
    # K 线
    # ------------------------------------------------------------------ #

    def goods_kline(
        self,
        market: int,
        code: str,
        period: Period = Period.DAILY,
        start: int = 0,
        count: int = 800,
        adjust: Adjust = Adjust.NONE,
    ) -> list[MacBar]:
        """获取扩展市场 K 线数据（支持复权）。"""

        from eltdx._native_abi import load_native

        return load_native().extension_single(
            self, "mac_ex", "bars", (market, code, period, start, count, adjust), None, _MODULES[__name__],
        )

    # ------------------------------------------------------------------ #
    # 分时
    # ------------------------------------------------------------------ #

    def goods_tick_chart(
        self,
        market: int,
        code: str,
        query_date: date | None = None,
    ) -> MacTickChart:
        """获取单日分时图。"""

        from eltdx._native_abi import load_native

        return load_native().extension_single(self, "mac_ex", "tick", (market, code, query_date), None, _MODULES[__name__])

    def goods_chart_sampling(self, market: int, code: str) -> list[float]:
        """获取分时缩略采样价格点（约 240 个点）。"""

        from eltdx._native_abi import load_native

        return load_native().extension_single(self, "mac_ex", "sampling", (market, code), None, _MODULES[__name__])

    # ------------------------------------------------------------------ #
    # 成交
    # ------------------------------------------------------------------ #

    def goods_transaction(
        self,
        market: int,
        code: str,
        query_date: date | None = None,
        start: int = 0,
        count: int = 2000,
    ) -> list[MacTransaction]:
        """获取逐笔成交数据。

        Note:
            港股股票类市场（见 :data:`eltdx.ex._hk_transaction.HK_STOCK_MARKETS`）
            走 ex 协议（当日 0x23FC / 历史 0x2406，价格单位港元）；其余市场走 MAC
            协议 0x122F。``start=0`` 指向最新一笔（协议为倒序）。
        """

        from eltdx._native_abi import load_native

        return load_native().mac_ex_transactions(
            self, (market, code, query_date, start, count), False, _MODULES[__name__],
        )

    def goods_transaction_all(
        self,
        market: int,
        code: str,
        query_date: date | None = None,
    ) -> list[MacTransaction]:
        """获取港股某日**全部**逐笔成交（仅港股股票类市场，自动翻页）。"""

        from eltdx._native_abi import load_native

        return load_native().mac_ex_transactions(self, (market, code, query_date), True, _MODULES[__name__])


def _to_goods_items(infos: Iterable[ExInstrumentInfo]) -> list[GoodsItem]:
    """把 EX 元数据项转成 ``GoodsItem``（0x2562 不可用时的回退表示）。

    ``GoodsItem`` 没有字符串 ``code`` 字段，因此把代码写入 ``name``——这与
    0x2562 协议本身的语义一致（该命令的 23 字节字段就是代码，如 ``IFL0``）。
    """

    from eltdx._native_abi import load_native

    return load_native().mac_ex_goods_items(infos, _MODULES[__name__])


def _first_code(item: GoodsItem) -> str:
    """从 GoodsItem 复原代码字符串。

    0x2562 协议把代码放在 23 字节字符串字段（``name``，如 ``IFL0``），
    ``code`` 只是 3 个 float 占位；元数据回退路径同样把代码写进 ``name``。
    只有在 ``name`` 为空时才退回 float 占位里取数字。
    """

    from eltdx._native_abi import load_native

    return load_native().mac_ex_first_code(item, _MODULES[__name__])


__all__ = ["MacExClient", "Period", "Adjust", "FieldBit", "build_mac_request"]
