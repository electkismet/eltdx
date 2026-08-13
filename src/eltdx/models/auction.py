"""Call auction models."""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True, slots=True)
class AuctionPoint:
    index: int
    minute_of_day_raw: int
    second_raw: int
    time_label: str
    time_seconds: int
    price: float
    price_milli: int
    matched_volume: int
    unmatched_signed_raw: int
    unmatched_volume: int
    unmatched_direction_raw: int
    reserved_zero_0e: int
    record_hex: str = ""

    @property
    def matched_amount_estimated(self) -> float:
        return self.price * self.matched_volume * 100.0


@dataclass(frozen=True, slots=True)
class AuctionSeries:
    exchange: str
    market_id: int
    code: str
    mode_or_selector_raw: int
    start_raw: int
    limit_or_count_raw: int
    points: tuple[AuctionPoint, ...]
    raw_payload: bytes = b""

    @property
    def full_code(self) -> str:
        return f"{self.exchange}{self.code}"

    @property
    def count(self) -> int:
        return len(self.points)
