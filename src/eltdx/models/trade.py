"""Trade tick models."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import date, datetime


@dataclass(frozen=True, slots=True)
class TradeTick:
    index: int
    absolute_index: int
    time_minutes: int
    time_label: str
    trade_datetime: datetime | None
    price: float
    price_milli: int
    volume: int
    order_count: int
    status_raw: int
    side: str
    price_delta_raw: int
    price_acc_raw: int
    unknown_tail_raw: int | None = None
    reserved_zero: int | None = None
    record_hex: str = ""
    # ``0x0fc5``/``0x0fc6`` share one wire record shape for auction snapshots
    # and real trades.  Keep the raw-compatible fields above and expose the
    # semantic interpretation separately.
    event_kind: str = "trade"
    auction_matched_volume: int | None = None
    auction_unmatched_signed_volume: int | None = None

    @property
    def is_auction_snapshot(self) -> bool:
        return self.event_kind == "auction_snapshot"

    @property
    def is_opening_match(self) -> bool:
        return self.event_kind == "opening_match"

    @property
    def is_trade(self) -> bool:
        return self.event_kind == "trade"

    @property
    def auction_unmatched_volume(self) -> int | None:
        if self.auction_unmatched_signed_volume is None:
            return None
        return abs(self.auction_unmatched_signed_volume)

    @property
    def trade_amount_yuan(self) -> float:
        return self.price * self.volume * 100.0


@dataclass(frozen=True, slots=True)
class TradePage:
    exchange: str
    market_id: int
    code: str
    start: int
    request_count: int
    ticks: tuple[TradeTick, ...]
    trading_date: date | None = None
    price_base_raw_f32: float | None = None
    raw_payload: bytes = b""

    @property
    def full_code(self) -> str:
        return f"{self.exchange}{self.code}"

    @property
    def count(self) -> int:
        return len(self.ticks)

    @property
    def has_more(self) -> bool:
        return self.count > 0

    @property
    def auction_snapshots(self) -> tuple[TradeTick, ...]:
        """Return embedded ``status=8`` auction snapshots from this page."""

        return tuple(tick for tick in self.ticks if tick.is_auction_snapshot)

    @property
    def opening_matches(self) -> tuple[TradeTick, ...]:
        """Return formal 09:25 opening matches from this page."""

        return tuple(tick for tick in self.ticks if tick.is_opening_match)
