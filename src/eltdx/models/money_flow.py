"""Daily money-flow response models."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import date
from typing import Any


@dataclass(frozen=True, slots=True)
class MoneyFlowDaily:
    date_raw: int
    date: date | None
    total_amount: float
    buckets: tuple[int, ...]
    main_net: float
    main_ratio: float
    raw: tuple[int, ...]
    record_hex: str = ""
    main_buy_net: float = 0.0
    main_buy_ratio: float = 0.0
    main_buy_super_large_net: float = 0.0
    main_buy_large_net: float = 0.0
    main_buy_medium_net: float = 0.0
    main_buy_small_net: float = 0.0
    main_super_large_net: float = 0.0
    main_large_net: float = 0.0
    main_medium_net: float = 0.0
    main_small_net: float = 0.0


@dataclass(frozen=True, slots=True)
class MoneyFlowBlock:
    exchange: str
    market_id: int
    code: str
    records: tuple[MoneyFlowDaily, ...]

    @property
    def full_code(self) -> str:
        return f"{self.exchange}{self.code}"

    @property
    def count(self) -> int:
        return len(self.records)

    @property
    def main_buy_net_total(self) -> float:
        return sum(record.main_buy_net for record in self.records)

    @property
    def main_buy_ratio_total(self) -> float | None:
        total = sum(record.total_amount for record in self.records)
        if total == 0:
            return None
        return self.main_buy_net_total / total * 100.0

    @property
    def main_net_total(self) -> float:
        return sum(record.main_net for record in self.records)

    @property
    def main_ratio_total(self) -> float | None:
        total = sum(record.total_amount for record in self.records)
        if total == 0:
            return None
        return self.main_net_total / total * 100.0


@dataclass(frozen=True, slots=True)
class MoneyFlowBatch:
    blocks: tuple[MoneyFlowBlock, ...]

    @property
    def count(self) -> int:
        return sum(block.count for block in self.blocks)


@dataclass(frozen=True, slots=True)
class FundFlowHistoryRecord:
    """One daily record of the ``0x052d`` category-22 money-flow series.

    The eight tier amounts use the same TDX custom float as k-line volume, so
    they are decoded wire amounts: compare them across records, not against a
    currency-normalised total.
    """

    date_raw: int
    date: date | None
    super_large_in: float
    large_in: float
    medium_in: float
    small_in: float
    super_large_out: float
    large_out: float
    medium_out: float
    small_out: float
    record_hex: str = ""

    @property
    def total_in(self) -> float:
        return self.super_large_in + self.large_in + self.medium_in + self.small_in

    @property
    def total_out(self) -> float:
        return self.super_large_out + self.large_out + self.medium_out + self.small_out

    @property
    def net(self) -> float:
        return self.total_in - self.total_out


@dataclass(frozen=True, slots=True)
class FundFlowHistorySeries:
    """Category-22 response for one security, oldest or newest record first as sent."""

    exchange: str
    market_id: int
    code: str
    start: int
    request_count: int
    records: tuple[FundFlowHistoryRecord, ...]
    raw_payload: bytes = b""

    @property
    def full_code(self) -> str:
        return f"{self.exchange}{self.code}"

    @property
    def count(self) -> int:
        return len(self.records)


@dataclass(frozen=True, slots=True)
class FundFlowRangeSeries:
    """One security's money-flow history from the 7615 TQLEX channel.

    由 ``client.money_flow.history()`` 返回，底层是 7615 网关的
    ``CWServ.tdxf10_gg_sj``（``{"Params": ["zjlx_sj", code, str(days), code]}``）。

    ``rows`` **原样保留**服务端列，不做语义臆测：``N001`` 日期、``N002`` 代码、
    ``N003`` 名称、``N004`` 主力净额、``N005`` 超大单净买入、``N006`` 大单净买入、
    ``N007`` 主买净额、``N009`` 上市日期（其余 ``N00x`` 列语义未确认，原样保留）。
    行序即服务端顺序。
    """

    code: str
    days: int
    codes: tuple[str, ...]
    rows: tuple[dict[str, Any], ...]
    raw_payload: bytes = b""

    @property
    def count(self) -> int:
        """Number of rows after the client-side ``start`` / ``count`` slice."""

        return len(self.rows)
