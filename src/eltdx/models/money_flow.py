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


@dataclass(frozen=True, slots=True)
class MoneyFlowBatch:
    blocks: tuple[MoneyFlowBlock, ...]

    @property
    def count(self) -> int:
        return sum(block.count for block in self.blocks)
