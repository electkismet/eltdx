"""MAC 协议扩展市场高层同步客户端 MacExClient。"""

from __future__ import annotations

from collections.abc import Iterable
from datetime import date
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
from eltdx.exceptions import TransportError
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
        ranked = ping_ex_all(candidates, port, ping_timeout)
        best = ranked[0][0] if ranked else candidates[0]
        return cls(best, port, timeout, hosts=candidates)

    @staticmethod
    def ping_all(
        hosts: list[str] | None = None,
        port: int = DEFAULT_EX_PORT,
        timeout: float = 5.0,
    ) -> list[tuple[str, float]]:
        """并发测量多台 MAC 扩展行情服务器延迟，按延迟升序返回。"""

        return ping_ex_all(hosts or list(MAC_EX_HOSTS), port, timeout)

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

    def goods_count(self, market: int | None = None) -> int:
        """获取扩展市场商品总数；``market=None`` 返回全市场总数。

        Note:
            有 ``market`` 时优先用 MAC 商品列表命令（0x2562）返回的 ``total``；
            **该命令在部分扩展行情主站上恒返回 0**，此时自动退回 EX 元数据
            （``GetExInstrumentInfoCmd``）二分定位市场边界后相减计数。
        """

        if market is None:
            return self._execute(GetExInstrumentCountCmd())
        cmd = GoodsListCmd(market, 0, 1)
        self._execute(cmd)
        if cmd.total:
            return int(cmd.total)
        lo, hi = self._market_bounds(market)
        return hi - lo

    def goods_list(self, market: int, start: int = 0, count: int = 600) -> list[GoodsItem]:
        """获取扩展市场商品列表（自动翻页）。

        Note:
            优先走 MAC 商品列表命令（0x2562）。该命令在部分扩展行情主站上
            **恒返回空列表**（``total=0``），此时自动退回 :meth:`instruments`
            的 EX 元数据路径。回退结果里 ``name`` 字段存的是**代码**（与
            0x2562 协议一致），代码字符串由 ``_first_code`` 取出；需要
            「代码 + 名称」成对数据请直接用 :meth:`instruments`。
        """

        page_size = 600
        collected: list[GoodsItem] = []
        offset = start
        while len(collected) < count:
            batch = self._execute(GoodsListCmd(market, offset, min(page_size, 1000)))
            if not batch:
                break
            collected.extend(batch)
            offset += len(batch)
            if len(batch) < page_size:
                break
        if collected:
            return collected[:count]
        return _to_goods_items(self.instruments(market, start=start, count=count))

    # ------------------------------------------------------------------ #
    # EX 元数据路径（服务器不支持 0x2562 时的可靠替代）
    # ------------------------------------------------------------------ #

    def _info_page(self, start: int, count: int) -> list[ExInstrumentInfo]:
        return self._execute(GetExInstrumentInfoCmd(start, count))

    def _market_bounds(self, market: int) -> tuple[int, int]:
        """二分定位 ``market`` 在全量扩展行情标的中的半开区间 ``[lo, hi)``。

        扩展行情标的全表按 market 升序排列，因此用「首个 ``>= market``」与
        「首个 ``> market``」两次二分即可得到边界；结果按市场号缓存。
        """

        cached = self._bounds_cache.get(market)
        if cached is not None:
            return cached
        total = self._execute(GetExInstrumentCountCmd())
        if total <= 0:
            return (0, 0)

        def lower_bound(predicate: Any) -> int:
            lo, hi = 0, total
            while lo < hi:
                mid = (lo + hi) // 2
                items = self._info_page(mid, 1)
                if not items:
                    hi = mid
                    continue
                if predicate(items[0].market):
                    hi = mid
                else:
                    lo = mid + 1
            return lo

        bounds = (lower_bound(lambda m: m >= market), lower_bound(lambda m: m > market))
        self._bounds_cache[market] = bounds
        return bounds

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

        if count <= 0:
            return []
        offset, _ = self._market_bounds(market)
        total = self._execute(GetExInstrumentCountCmd())
        page = min(max(count, 1), 1000)
        collected: list[ExInstrumentInfo] = []
        skipped = 0
        pos = offset
        while pos < total and len(collected) < count:
            batch = self._info_page(pos, page)
            if not batch:
                break
            for item in batch:
                if item.market == market:
                    if skipped < start:
                        skipped += 1
                        continue
                    collected.append(item)
                    if len(collected) >= count:
                        break
                elif item.market > market:
                    return collected
            pos += len(batch)
        return collected

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

        selected = DEFAULT_QUOTE_FIELDS if fields is None else fields
        return self._execute(SymbolQuotesCmd(stocks, selected))

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

        page_size = min(count, 80)
        items = self.goods_list(market, start=start, count=page_size)
        if not items:
            return []
        codes = [(market, _first_code(item)) for item in items]
        codes = [(m, c) for m, c in codes if c]
        if not codes:
            return []
        return self._execute(SymbolQuotesCmd(codes, DEFAULT_QUOTE_FIELDS))

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

        return self._execute(
            SymbolBarCmd(market, code, period=period, start=start, count=count, fq=adjust)
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

        return self._execute(SymbolTickChartCmd(market, code, query_date))

    def goods_chart_sampling(self, market: int, code: str) -> list[float]:
        """获取分时缩略采样价格点（约 240 个点）。"""

        return self._execute(ChartSamplingCmd(market, code))

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

        if is_hk_stock_market(market):
            return _fetch_hk_transactions_sync(self._execute, market, code, query_date, start, count)
        return self._execute(SymbolTransactionCmd(market, code, query_date, start, count))

    def goods_transaction_all(
        self,
        market: int,
        code: str,
        query_date: date | None = None,
    ) -> list[MacTransaction]:
        """获取港股某日**全部**逐笔成交（仅港股股票类市场，自动翻页）。"""

        if not is_hk_stock_market(market):
            raise ValueError(
                "goods_transaction_all 仅支持港股股票类市场（HK_STOCK_MARKETS），"
                f"收到 market={market}；其他市场请用 goods_transaction。"
            )
        return _fetch_all_hk_transactions_sync(self._execute, market, code, query_date)


def _to_goods_items(infos: Iterable[ExInstrumentInfo]) -> list[GoodsItem]:
    """把 EX 元数据项转成 ``GoodsItem``（0x2562 不可用时的回退表示）。

    ``GoodsItem`` 没有字符串 ``code`` 字段，因此把代码写入 ``name``——这与
    0x2562 协议本身的语义一致（该命令的 23 字节字段就是代码，如 ``IFL0``）。
    """

    items: list[GoodsItem] = []
    for index, info in enumerate(infos):
        try:
            numeric = [float(info.code)]
        except (TypeError, ValueError):
            numeric = []
        items.append(
            GoodsItem(
                name=info.code or info.name,
                category=info.category,
                u=0,
                index=index,
                switch=0,
                code=numeric,
                c1=0,
                c2=0,
            )
        )
    return items


def _first_code(item: GoodsItem) -> str:
    """从 GoodsItem 复原代码字符串。

    0x2562 协议把代码放在 23 字节字符串字段（``name``，如 ``IFL0``），
    ``code`` 只是 3 个 float 占位；元数据回退路径同样把代码写进 ``name``。
    只有在 ``name`` 为空时才退回 float 占位里取数字。
    """

    label = (item.name or "").strip()
    if label:
        return label
    parts: Iterable[float] = item.code
    values = [int(v) for v in parts]
    if not values or values[0] <= 0:
        return ""
    return str(values[0])


__all__ = ["MacExClient", "Period", "Adjust", "FieldBit", "build_mac_request"]
