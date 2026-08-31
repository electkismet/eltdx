"""Daily money-flow response models."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import date


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
