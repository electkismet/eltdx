"""MAC 协议数据模型。

所有模型均为 ``@dataclass(frozen=True, slots=True)``。为满足 eltdx 零第三方
依赖的硬性要求，easy_tdx 版本返回的 pandas DataFrame 在本移植中一律改为
``list[dataclass]``。
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import date, datetime, time
from typing import Any


@dataclass(frozen=True, slots=True)
class MacQuoteField:
    """MAC 协议自定义字段报价中的一条记录。"""

    market: int
    code: str
    name: str
    fields: dict[str, Any] = field(default_factory=dict)


@dataclass(frozen=True, slots=True)
class MacSymbolInfo:
    """个股简要特征。"""

    market: int
    code: str
    name: str
    time: datetime | None
    activity: int
    pre_close: float
    open: float
    high: float
    low: float
    close: float
    momentum: float
    vol: int
    amount: float
    inside_volume: int
    outside_volume: int
    turnover: float
    avg: float


@dataclass(frozen=True, slots=True)
class MacBar:
    """MAC 协议 K 线数据。"""

    datetime: datetime
    open: float
    high: float
    low: float
    close: float
    vol: float
    amount: float
    float_shares: float = 0.0


@dataclass(frozen=True, slots=True)
class MacTick:
    """分时数据点。"""

    time: time
    price: float
    avg: float
    vol: int
    momentum: float = 0.0


@dataclass(frozen=True, slots=True)
class MacTickChart:
    """单日分时图。"""

    market: int
    code: str
    name: str
    pre_close: float
    open: float
    high: float
    low: float
    close: float
    vol: int
    amount: float
    turnover: float = 0.0
    avg: float = 0.0
    charts: list[MacTick] = field(default_factory=list)


@dataclass(frozen=True, slots=True)
class MacMultiTickDay:
    """多日分时图中的一天。"""

    date: date
    pre_close: float
    ticks: list[MacTick] = field(default_factory=list)


@dataclass(frozen=True, slots=True)
class MacMultiTickChart:
    """多日分时图。"""

    market: int
    code: str
    name: str
    pre_close: float
    open: float
    high: float
    low: float
    close: float
    vol: int
    amount: float
    turnover: float = 0.0
    avg: float = 0.0
    charts: list[MacMultiTickDay] = field(default_factory=list)


@dataclass(frozen=True, slots=True)
class MacTransaction:
    """逐笔成交。"""

    time: time
    price: float
    vol: int
    trade_count: int
    bs_flag: int  # 0=买入 / 1=卖出 / 2=中性 / 5=盘后


@dataclass(frozen=True, slots=True)
class BoardInfo:
    """板块信息（含领涨股）。"""

    market: int
    code: str
    name: str
    price: float
    rise_speed: float
    pre_close: float
    symbol_market: int
    symbol_code: str
    symbol_name: str
    symbol_price: float
    symbol_rise_speed: float
    symbol_pre_close: float


@dataclass(frozen=True, slots=True)
class AuctionItem:
    """集合竞价数据。"""

    time: time
    price: float
    matched: int
    unmatched: int


@dataclass(frozen=True, slots=True)
class UnusualItem:
    """异动数据。"""

    index: int
    market: int
    code: str
    name: str
    time: time
    desc: str
    value: str
    unusual_type: int


@dataclass(frozen=True, slots=True)
class CapitalFlowData:
    """资金流向数据。"""

    date: str
    main_in: float = 0.0
    main_out: float = 0.0
    main_net: float = 0.0
    small_in: float = 0.0
    small_out: float = 0.0
    small_net: float = 0.0
    mid_in: float = 0.0
    mid_out: float = 0.0
    mid_net: float = 0.0
    large_in: float = 0.0
    large_out: float = 0.0
    large_net: float = 0.0


@dataclass(frozen=True, slots=True)
class BelongBoardInfo:
    """个股所属板块信息。"""

    board_type: int
    market: int
    board_code: str
    board_name: str
    close: float
    pre_close: float


@dataclass(frozen=True, slots=True)
class ServerSession:
    """服务器交易时段信息。"""

    today: str
    last_trading_day: str
    sessions_1: list[dict[str, Any]] = field(default_factory=list)
    sessions_2: list[dict[str, Any]] = field(default_factory=list)
    market_param_1: int = 0
    market_param_2: int = 0


@dataclass(frozen=True, slots=True)
class KlineOffsetInfo:
    """K 线偏移信息。"""

    total: int
    returned: int


@dataclass(frozen=True, slots=True)
class GoodsItem:
    """扩展市场（期货/期权等）商品信息。"""

    name: str
    category: int
    u: int
    index: int
    switch: int
    code: list[float] = field(default_factory=list)
    c1: int = 0
    c2: int = 0


@dataclass(frozen=True, slots=True)
class FileMeta:
    """远程文件元信息。"""

    offset: int
    size: int
    flag: int
    hash: str


__all__ = [
    "AuctionItem",
    "BelongBoardInfo",
    "BoardInfo",
    "CapitalFlowData",
    "FileMeta",
    "GoodsItem",
    "KlineOffsetInfo",
    "MacBar",
    "MacMultiTickChart",
    "MacMultiTickDay",
    "MacQuoteField",
    "MacSymbolInfo",
    "MacTick",
    "MacTickChart",
    "MacTransaction",
    "ServerSession",
    "UnusualItem",
]