from __future__ import annotations

from .models import EquityItem, EquityResponse, GbbqItem
from .protocol.unit import normalize_trading_date

EQUITY_CATEGORIES = {2, 3, 5, 7, 8, 9, 10}
VOLUME_UNIT_MULTIPLIERS = {
    "share": 1,
    "shares": 1,
    "stock": 1,
    "hand": 100,
    "hands": 100,
    "lot": 100,
    "lots": 100,
}


def filter_equity_items(items: list[GbbqItem]) -> EquityResponse:
    filtered = [
        EquityItem(
            code=item.code,
            time=item.time,
            category=item.category,
            category_name=item.category_name,
            float_shares=int(item.c3),
            total_shares=int(item.c4),
        )
        for item in items
        if item.category in EQUITY_CATEGORIES
    ]
    return EquityResponse(count=len(filtered), items=filtered)


def pick_equity(items: list[EquityItem], on=None) -> EquityItem | None:
    _, target_date = normalize_trading_date(on)
    ordered = sorted(items, key=lambda item: item.time)
    for item in reversed(ordered):
        if item.time.date() <= target_date:
            return item
    return None


def normalize_volume_unit(unit: str) -> int:
    key = str(unit).strip().lower()
    try:
        return VOLUME_UNIT_MULTIPLIERS[key]
    except KeyError as exc:
        raise ValueError(f"invalid volume unit: {unit!r}") from exc


def compute_turnover(equity: EquityItem | None, volume: int | float, *, unit: str = "hand") -> float:
    if equity is None or equity.float_shares <= 0:
        return 0.0
    shares = float(volume) * normalize_volume_unit(unit)
    return shares / float(equity.float_shares) * 100.0
