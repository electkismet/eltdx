"""扩展行情数据模型与已知市场别名表。

每个记录级模型都带一个 ``_raw`` 字段，保存该条记录在响应体中的**原始字节
切片**（与 ``easy_tdx`` 同名同语义）。它不参与比较也不出现在 ``repr`` 中，
仅用于无损留存——当某些字段本库尚未解析，或需要按字节级复核时可以直接取用。
"""

from __future__ import annotations

from dataclasses import dataclass, field

from eltdx.ex.hosts import KNOWN_EX_HOSTS, MAC_EX_HOSTS

# 已知扩展行情市场代码 → 中文名
KNOWN_EX_MARKETS: dict[int, str] = {
    0: "深圳",
    1: "上海",
    28: "郑州商品",
    29: "大连商品",
    30: "上海期货",
    31: "香港主板",
    47: "中金所",
    48: "香港创业板",
    49: "香港基金",
    71: "沪港通",
    74: "外盘",
}


@dataclass(frozen=True, slots=True)
class ExMarketInfo:
    """市场定义（GetMarkets 返回）。"""

    market: int
    category: int
    name: str
    short_name: str
    _raw: bytes = field(default=b"", repr=False, compare=False)


@dataclass(frozen=True, slots=True)
class ExInstrumentInfo:
    """合约/证券信息（GetInstrumentInfo 返回）。"""

    category: int
    market: int
    code: str
    name: str
    desc: str
    _raw: bytes = field(default=b"", repr=False, compare=False)


@dataclass(frozen=True, slots=True)
class ExInstrumentQuote:
    """五档行情（GetInstrumentQuote 返回）。"""

    market: int
    code: str
    pre_close: float
    open: float
    high: float
    low: float
    price: float
    kaicang: int
    zongliang: int
    xianliang: int
    neipan: int
    waipan: int
    chicang: int
    bid1: float
    bid2: float
    bid3: float
    bid4: float
    bid5: float
    bid_vol1: int
    bid_vol2: int
    bid_vol3: int
    bid_vol4: int
    bid_vol5: int
    ask1: float
    ask2: float
    ask3: float
    ask4: float
    ask5: float
    ask_vol1: int
    ask_vol2: int
    ask_vol3: int
    ask_vol4: int
    ask_vol5: int
    _raw: bytes = field(default=b"", repr=False, compare=False)


@dataclass(frozen=True, slots=True)
class ExInstrumentBar:
    """K线数据（GetInstrumentBars / GetHistoryInstrumentBarsRange 返回）。"""

    open: float
    high: float
    low: float
    close: float
    position: int
    trade: int
    amount: float
    year: int
    month: int
    day: int
    hour: int
    minute: int
    _raw: bytes = field(default=b"", repr=False, compare=False)


@dataclass(frozen=True, slots=True)
class ExMinuteBar:
    """分时数据（GetMinuteTimeData / GetHistoryMinuteTimeData 返回）。"""

    hour: int
    minute: int
    price: float
    avg_price: float
    volume: int
    open_interest: int
    _raw: bytes = field(default=b"", repr=False, compare=False)


@dataclass(frozen=True, slots=True)
class ExTransactionRecord:
    """逐笔成交记录（GetTransactionData / GetHistoryTransactionData 返回）。"""

    hour: int
    minute: int
    second: int
    price: int
    volume: int
    zengcang: int
    nature: int
    _raw: bytes = field(default=b"", repr=False, compare=False)


__all__ = [
    "KNOWN_EX_HOSTS",
    "KNOWN_EX_MARKETS",
    "MAC_EX_HOSTS",
    "ExMarketInfo",
    "ExInstrumentInfo",
    "ExInstrumentQuote",
    "ExInstrumentBar",
    "ExMinuteBar",
    "ExTransactionRecord",
]
