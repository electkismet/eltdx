"""Market-wide statistics and intraday fund flow helpers.

Both helpers compose existing public APIs; numeric classification is shared
with the independent Rust SDK through the native compatibility boundary:
``0x053e`` legacy quotes for the market-statistic pseudo indices
(880005/880001/880006) and ``0x0fc5`` today ticks for intraday fund flow.
"""

from __future__ import annotations

from dataclasses import dataclass

from eltdx.models.trade import TradeTick

# Statistic pseudo-indices served by the quote server.  880005 carries the
# up/down/neutral/total counts, 880001 the total market cap and 880006 the
# limit-up/limit-down counts.  Each counter field is reported as counts on the
# wire, and counts are recovered with ``round(value)``.
_MARKET_STAT_CODES = ("sh880005", "sh880001", "sh880006")

# Tick-amount thresholds (yuan) classifying one trade into the four size
# buckets, mirroring the 超大单/大单/中单/小单 convention.
_LARGE_THRESHOLD = 200_000.0
_SUPER_THRESHOLD = 1_000_000.0


@dataclass(frozen=True, slots=True)
class MarketStat:
    """Whole-market breadth snapshot built from the statistic indices."""

    up_count: int  # 上涨家数
    down_count: int  # 下跌家数
    neutral_count: int  # 平盘家数
    suspended_count: int  # 由 total-(up+down+neutral) 得到的剩余项，近似表示停牌/未参与统计家数
    total_count: int  # 总家数（包含停牌）
    total_amount: float  # 总成交额
    total_volume: float  # 总成交量
    total_market_cap: float  # 总市值（元），来自 880001 收盘价，乘 1e10 还原
    limit_up_count: int  # 涨停家数，来自 880006
    limit_down_count: int  # 跌停家数，来自 880006


@dataclass(frozen=True, slots=True)
class FundFlow:
    """Per-stock intraday fund flow aggregated from L1 ticks."""

    super_in: float  # 超大单流入 (>100万)
    large_in: float  # 大单流入 (>20万 且 <=100万)
    medium_in: float  # 中单流入 (>4万 且 <=20万)
    small_in: float  # 小单流入 (<=4万)
    super_out: float
    large_out: float
    medium_out: float
    small_out: float

    @property
    def main_net_inflow(self) -> float:
        """主力净流入（超大单 + 大单）。"""
        return (self.super_in + self.large_in) - (self.super_out + self.large_out)

    @property
    def total_net_inflow(self) -> float:
        """全单净流入。"""
        return (self.super_in + self.large_in + self.medium_in + self.small_in) - (
            self.super_out + self.large_out + self.medium_out + self.small_out
        )


def fetch_market_stat(client) -> MarketStat:
    """Fetch the breadth snapshot via one batched ``0x053e`` request."""

    from eltdx.transport.native import native_module

    return native_module().helper_market_stat(client, _MARKET_STAT_CODES, MarketStat)


def classify_fund_flow(ticks: tuple[TradeTick, ...] | list[TradeTick]) -> FundFlow:
    """Aggregate one stock's today ticks into the four-bucket fund flow."""

    from eltdx.transport.native import native_module

    values = native_module().helper_fund_flow(ticks, _SUPER_THRESHOLD, _LARGE_THRESHOLD)
    names = ("super_in", "large_in", "medium_in", "small_in",
             "super_out", "large_out", "medium_out", "small_out")
    return FundFlow(**dict(zip(names, values)))


def fetch_today_fund_flow(client, code: str) -> FundFlow:
    """Read all today ticks of one stock and classify them into fund flow."""

    from eltdx.transport.native import native_module

    return native_module().helper_today_flow(client, code, classify_fund_flow)
