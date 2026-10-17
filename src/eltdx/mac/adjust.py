"""本地前复权（QFQ）重算（纯 Python，零第三方依赖）。

通达信 MAC 服务端在 QFQ 模式下，对长期重度除权股票的深层历史页会返回
负价格（上游缺陷）。本模块用 NONE（未复权）K 线 + XDXR（除权除息）记录在
客户端本地重算前复权序列，作为服务端 QFQ 异常时的兜底。

公式::

    复权价 = (原价 - 每股分红 + 每股配股价 x 每股配股比例) /
             (1 + 每股送转股比例 + 每股配股比例)

约定：以除权日**前一交易日**的收盘价（含权价 ``P_cum``）作为基准，前复权
因子把该日及之前的价格乘以::

    f = (P_cum - fenhong + peigujia * peigu) /
        (P_cum * (1 + songzhuangu + peigu))

最新价不动（锚定最新）。``fenhong`` / ``songzhuangu`` / ``peigu`` 为每股
单位，``peigujia`` 为元/股。

与 easy_tdx 的差异：easy_tdx 基于 pandas/numpy DataFrame 实现；本移植在
``list[MacBar]`` 上工作，事件用 :class:`XdxrEvent` 表示。
"""

from __future__ import annotations

import math
from bisect import bisect_left
from collections.abc import Sequence
from dataclasses import dataclass
from datetime import date

from .models import MacBar


@dataclass(frozen=True, slots=True)
class XdxrEvent:
    """单条除权除息（XDXR，``category == 1``）事件。"""

    date: date
    category: int = 1
    fenhong: float = 0.0
    peigujia: float = 0.0
    songzhuangu: float = 0.0
    peigu: float = 0.0


def compute_forward_factor(
    cum_close: float,
    fenhong: float,
    peigujia: float,
    songzhuangu: float,
    peigu: float,
) -> float:
    """计算单次除权除息事件的前复权乘子。

    Args:
        cum_close: 除权日前一交易日的 NONE 收盘价（含权价）。
        fenhong: 每股分红（元）。
        peigujia: 每股配股价（元/股）。
        songzhuangu: 每股送转股比例（如 0.1 = 10 送/转 1）。
        peigu: 每股配股比例。

    Returns:
        前复权因子；输入非法（``cum_close<=0``、分母为 0、结果非有限）时
        返回 ``nan``。
    """

    if cum_close <= 0:
        return float("nan")
    denom = cum_close * (1.0 + songzhuangu + peigu)
    if denom == 0:
        return float("nan")
    factor = (cum_close - fenhong + peigujia * peigu) / denom
    if not math.isfinite(factor):
        return float("nan")
    return float(factor)


def apply_forward_adjust(
    bars: Sequence[MacBar],
    xdxr_events: Sequence[XdxrEvent],
) -> list[MacBar]:
    """对 NONE K 线应用前复权，返回新的 bar 列表（按 datetime 升序）。

    遍历 ``category==1`` 的除权除息事件（按日期升序），把每个事件的因子累乘
    到该除权日**前一交易日及之前**所有 bar 的 OHLC。最新价锚定不动。无事件
    或异常时返回原列表的浅拷贝。
    """

    ordered = sorted(bars, key=lambda bar: bar.datetime)
    if not ordered:
        return list(ordered)

    events = [
        event
        for event in xdxr_events
        if event.category == 1
        and any((event.fenhong, event.peigujia, event.songzhuangu, event.peigu))
    ]
    if not events:
        return list(ordered)

    dates = [bar.datetime.date() for bar in ordered]
    opens = [bar.open for bar in ordered]
    highs = [bar.high for bar in ordered]
    lows = [bar.low for bar in ordered]
    closes = [bar.close for bar in ordered]

    for event in sorted(events, key=lambda item: item.date):
        idx = bisect_left(dates, event.date)
        cum_idx = idx - 1
        if cum_idx < 0 or cum_idx >= len(ordered):
            continue
        factor = compute_forward_factor(
            closes[cum_idx],
            event.fenhong,
            event.peigujia,
            event.songzhuangu,
            event.peigu,
        )
        if not math.isfinite(factor):
            continue
        for i in range(cum_idx + 1):
            opens[i] *= factor
            highs[i] *= factor
            lows[i] *= factor
            closes[i] *= factor

    return [
        MacBar(
            datetime=bar.datetime,
            open=opens[i],
            high=highs[i],
            low=lows[i],
            close=closes[i],
            vol=bar.vol,
            amount=bar.amount,
            float_shares=bar.float_shares,
        )
        for i, bar in enumerate(ordered)
    ]


def has_bad_prices(bars: Sequence[MacBar]) -> bool:
    """检测 QFQ 结果是否含非法价格（``<=0`` 或非有限值）。"""

    for bar in bars:
        for value in (bar.open, bar.high, bar.low, bar.close):
            if not math.isfinite(value) or value <= 0:
                return True
    return False


__all__ = [
    "XdxrEvent",
    "apply_forward_adjust",
    "compute_forward_factor",
    "has_bad_prices",
]