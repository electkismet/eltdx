"""Shortline indicators composed from live quotes and TDX statistics resources."""

from __future__ import annotations

from collections import Counter
from collections.abc import Iterable, Sequence
from dataclasses import dataclass
from datetime import date, datetime, time
from threading import RLock
from typing import TYPE_CHECKING, Any

from eltdx.exceptions import (
    ResourceFormatError,
    ShortlineIndicatorsNotReadyError,
    TdxStatsDateError,
)
from eltdx.models import TdxStat2Row, TdxStatRow, TdxStatsResource
from eltdx.protocol.unit import MARKET_TO_ID, normalize_code

if TYPE_CHECKING:
    from eltdx.client import TdxClient


AUCTION_READY_TIME = time(9, 25)
MIN_DOMINANT_DATE_COVERAGE = 0.95


@dataclass(frozen=True, slots=True)
class ShortlineIndicator:
    full_code: str
    exchange: str
    market_id: int
    code: str
    target_trade_date: date
    previous_trade_date: date
    stats_date: date | None
    alignment_status: str
    limit_status: str
    beta_60d: float | None
    pe_ttm: float | None
    free_float_shares: float | None
    prev_amount: float | None
    prev_seal_amount: float | None
    prev2_seal_amount: float | None
    prev_open_volume_hand: float | None
    prev_open_amount: float | None
    limit_stat_days: int | None
    limit_up_count_in_stat_days: int | None
    limit_up_streak_days: int | None
    year_limit_up_days: int | None
    free_float_market_value: float | None
    open_turnover_z: float | None
    open_prev_amount_ratio: float | None
    auction_prev_volume_ratio: float | None
    open_prev_seal_ratio: float | None
    seal_to_float_ratio: float | None
    seal_prev_ratio: float | None
    limit_board_text: str | None
    ladder_level: int | None


@dataclass(frozen=True, slots=True)
class ShortlineIndicatorTable:
    codes: tuple[str, ...]
    target_trade_date: date
    previous_trade_date: date
    stats_date: date
    stats_source_path: str
    stats_refreshed: bool
    rows: tuple[ShortlineIndicator, ...]

    @property
    def count(self) -> int:
        return len(self.rows)


@dataclass(frozen=True, slots=True)
class _MarketDateContext:
    target_trade_date: date
    previous_trade_date: date
    ready: bool


class ShortlineIndicatorService:
    def __init__(self, client: TdxClient) -> None:
        self._client = client
        self._stats_cache: dict[str, TdxStatsResource] = {}
        self._stats_lock = RLock()

    def clear_cache(self) -> None:
        with self._stats_lock:
            self._stats_cache.clear()

    def get(
        self,
        codes: str | Sequence[str],
        *,
        stats_path: str = "zhb.zip",
        refresh_stats: bool = False,
    ) -> ShortlineIndicatorTable:
        if not isinstance(refresh_stats, bool):
            raise ValueError("refresh_stats must be a boolean")
        if not isinstance(stats_path, str) or not stats_path.strip():
            raise ValueError("stats_path must be a non-empty string")
        full_codes = _code_list(codes)
        if not full_codes:
            raise ValueError("at least one code is required")

        context = _resolve_market_date_context(self._client)
        if not context.ready:
            raise ShortlineIndicatorsNotReadyError(
                "shortline indicators are not ready before the 09:25 auction completes"
            )
        stats, stats_refreshed = self._stats_resource(
            stats_path,
            refresh=refresh_stats,
            target=context.target_trade_date,
            previous=context.previous_trade_date,
        )
        resource_date = _validate_stats_resource_dates(
            stats,
            target=context.target_trade_date,
            previous=context.previous_trade_date,
        )

        # Shortline metrics use the same complete quote shape as the former
        # flat helper: the snapshot is followed by a best-effort five-level
        # refresh.
        quote_map = _by_full_code(self._client.helpers.full_quotes(full_codes))
        security_map = _security_map(self._client, full_codes)
        rows = tuple(
            _build_indicator(
                full_code,
                quote=quote_map.get(full_code),
                security=security_map.get(full_code),
                stats=stats,
                context=context,
            )
            for full_code in full_codes
        )
        return ShortlineIndicatorTable(
            codes=tuple(full_codes),
            target_trade_date=context.target_trade_date,
            previous_trade_date=context.previous_trade_date,
            stats_date=resource_date,
            stats_source_path=stats.source_path,
            stats_refreshed=stats_refreshed,
            rows=rows,
        )

    def _stats_resource(
        self,
        path: str,
        *,
        refresh: bool,
        target: date,
        previous: date,
    ) -> tuple[TdxStatsResource, bool]:
        with self._stats_lock:
            cached = self._stats_cache.get(path)
            if not refresh and cached is not None and _stats_resource_is_usable(
                cached,
                target=target,
                previous=previous,
            ):
                return cached, False
            resource = self._client.resources.read_stats(path)
            _validate_stats_resource_dates(resource, target=target, previous=previous)
            self._stats_cache[path] = resource
            return resource, True


def _resolve_market_date_context(client: TdxClient) -> _MarketDateContext:
    handshake = _client_handshake_info(client)
    if handshake is None:
        raise TdxStatsDateError(
            "unable to resolve the target trading day without a TDX handshake"
        )
    server_datetime = getattr(handshake, "server_datetime", None)
    if not isinstance(server_datetime, datetime):
        raise TdxStatsDateError(
            "TDX handshake does not contain a usable server datetime"
        )
    handshake_dates = sorted(
        value
        for value in (
            getattr(handshake, "server_date_1", None),
            getattr(handshake, "server_date_2", None),
        )
        if isinstance(value, date)
    )
    if not handshake_dates:
        raise TdxStatsDateError(
            "TDX handshake does not contain a usable target trading day"
        )
    target = handshake_dates[-1]

    previous = client.workdays.previous_workday(target)
    if previous is None:
        raise TdxStatsDateError(
            f"unable to resolve the previous trading day for {target.isoformat()}"
        )

    ready = True
    if server_datetime.date() == target and server_datetime.time() < AUCTION_READY_TIME:
        ready = False
    return _MarketDateContext(
        target_trade_date=target,
        previous_trade_date=previous,
        ready=ready,
    )


def _client_handshake_info(client: TdxClient) -> Any | None:
    transport = getattr(client, "transport", None)
    candidates = list(getattr(transport, "_transports", ()) or ())
    if transport is not None and not candidates:
        candidates = [transport]
    for candidate in candidates:
        handshake = getattr(candidate, "last_handshake", None)
        if handshake is not None:
            return handshake
    request_handshake = getattr(getattr(client, "session", None), "handshake", None)
    if callable(request_handshake):
        return request_handshake()
    return None


def _stats_resource_is_usable(
    resource: TdxStatsResource,
    *,
    target: date,
    previous: date,
) -> bool:
    try:
        _validate_stats_resource_dates(resource, target=target, previous=previous)
    except ResourceFormatError:
        return False
    return True


def _validate_stats_resource_dates(
    resource: TdxStatsResource,
    *,
    target: date,
    previous: date,
) -> date:
    stat_date, stat_coverage = _dominant_date_and_coverage(resource.stat.values())
    stat2_date, stat2_coverage = _dominant_date_and_coverage(resource.stat2.values())
    if stat_date is None or stat2_date is None:
        raise ResourceFormatError(
            "TDX statistics resource has no dominant date in tdxstat.cfg or tdxstat2.cfg"
        )
    if stat_date != stat2_date:
        raise ResourceFormatError(
            "TDX statistics resource dates disagree: "
            f"tdxstat.cfg={stat_date}, tdxstat2.cfg={stat2_date}"
        )
    if (
        stat_coverage < MIN_DOMINANT_DATE_COVERAGE
        or stat2_coverage < MIN_DOMINANT_DATE_COVERAGE
    ):
        raise ResourceFormatError(
            "TDX statistics resource dominant-date coverage is too low: "
            f"tdxstat.cfg={stat_coverage:.2%}, tdxstat2.cfg={stat2_coverage:.2%}"
        )
    parsed = _parse_stats_date(stat_date)
    if parsed not in {target, previous}:
        raise TdxStatsDateError(
            "TDX statistics resource is not usable for the target session: "
            f"stats_date={parsed.isoformat()}, target={target.isoformat()}, "
            f"previous={previous.isoformat()}"
        )
    return parsed


def _dominant_date_and_coverage(rows: Iterable[Any]) -> tuple[str | None, float]:
    materialized = list(rows)
    counts = Counter(str(row.stats_date) for row in materialized if row.stats_date)
    if not counts:
        return None, 0.0
    dominant = max(counts, key=lambda value: (counts[value], value))
    return dominant, counts[dominant] / max(1, len(materialized))


def _parse_stats_date(value: str) -> date:
    try:
        parsed = datetime.strptime(value, "%Y%m%d").date()
    except ValueError as exc:
        raise ResourceFormatError(
            f"TDX statistics resource contains an invalid date: {value!r}"
        ) from exc
    return parsed


def _build_indicator(
    full_code: str,
    *,
    quote: Any | None,
    security: Any | None,
    stats: TdxStatsResource,
    context: _MarketDateContext,
) -> ShortlineIndicator:
    exchange = full_code[:2]
    code = full_code[2:]
    market_id = MARKET_TO_ID[exchange]
    stat_row, stat2_row = stats.row(market_id, code)
    acceptable_dates = {
        context.target_trade_date.strftime("%Y%m%d"),
        context.previous_trade_date.strftime("%Y%m%d"),
    }
    if stat_row is not None and stat_row.stats_date not in acceptable_dates:
        stat_row = None
    aligned = _aligned_stat2(
        stat2_row,
        target=context.target_trade_date,
        previous=context.previous_trade_date,
    )

    last_price = _number(quote, "last_price")
    open_price = _number(quote, "open_price")
    open_amount = _number(quote, "open_amount_yuan")
    open_volume_hand = _safe_ratio(open_amount, open_price * 100.0 if open_price else None)
    free_float_shares = _tenk(getattr(stat_row, "free_float_shares_10k", None))
    free_float_market_value = _multiply(free_float_shares, last_price)
    locked_amount = _locked_amount(quote)
    prev_amount = _tenk(aligned["prev_amount_10k"])
    prev_seal_amount = _tenk(aligned["prev_seal_amount_10k"])
    prev2_seal_amount = _tenk(aligned["prev2_seal_amount_10k"])
    prev_open_amount = _tenk(aligned["prev_open_amount_10k"])
    prev_open_volume_hand = _round(aligned["prev_open_volume_hand"])
    limit_status = _limit_status(full_code, quote, getattr(security, "name", None))

    days = getattr(stat_row, "limit_stat_days", None)
    count = getattr(stat_row, "limit_up_count_in_stat_days", None)
    stat_date = _row_stats_date(stat_row, stat2_row)
    if (
        limit_status == "sealed"
        and stat_date == context.previous_trade_date
    ):
        days = (days or 0) + 1
        count = (count or 0) + 1

    return ShortlineIndicator(
        full_code=full_code,
        exchange=exchange,
        market_id=market_id,
        code=code,
        target_trade_date=context.target_trade_date,
        previous_trade_date=context.previous_trade_date,
        stats_date=stat_date,
        alignment_status=str(aligned["status"]),
        limit_status=limit_status,
        beta_60d=_round(getattr(stat_row, "beta_60d", None)),
        pe_ttm=_round(getattr(stat_row, "pe_ttm", None)),
        free_float_shares=free_float_shares,
        prev_amount=prev_amount,
        prev_seal_amount=prev_seal_amount,
        prev2_seal_amount=prev2_seal_amount,
        prev_open_volume_hand=prev_open_volume_hand,
        prev_open_amount=prev_open_amount,
        limit_stat_days=getattr(stat_row, "limit_stat_days", None),
        limit_up_count_in_stat_days=getattr(
            stat_row, "limit_up_count_in_stat_days", None
        ),
        limit_up_streak_days=getattr(stat_row, "limit_up_streak_days", None),
        year_limit_up_days=getattr(stat_row, "year_limit_up_days", None),
        free_float_market_value=free_float_market_value,
        open_turnover_z=_safe_ratio_pct(
            open_volume_hand,
            free_float_shares / 100.0 if free_float_shares else None,
        ),
        open_prev_amount_ratio=_safe_ratio_pct(open_amount, prev_amount),
        auction_prev_volume_ratio=_safe_ratio(
            open_volume_hand, prev_open_volume_hand
        ),
        open_prev_seal_ratio=_safe_ratio_pct(open_amount, prev_seal_amount),
        seal_to_float_ratio=_safe_ratio_pct(
            locked_amount, free_float_market_value
        ),
        seal_prev_ratio=_safe_ratio(locked_amount, prev_seal_amount),
        limit_board_text=_limit_board_text(days, count),
        ladder_level=_ladder_level(
            stat_row,
            stats_date=stat_date,
            target=context.target_trade_date,
            previous=context.previous_trade_date,
            limit_status=limit_status,
        ),
    )


def _aligned_stat2(
    row: TdxStat2Row | None,
    *,
    target: date,
    previous: date,
) -> dict[str, Any]:
    if row is None or not row.stats_date:
        return _empty_alignment("stats_row_missing")
    target_text = target.strftime("%Y%m%d")
    previous_text = previous.strftime("%Y%m%d")
    if row.stats_date == target_text:
        return {
            "status": "same_day",
            "prev_amount_10k": row.prev_amount_10k,
            "prev_seal_amount_10k": row.prev_seal_amount_10k,
            "prev2_seal_amount_10k": row.prev2_seal_amount_10k,
            "prev_open_volume_hand": row.prev_open_volume_hand,
            "prev_open_amount_10k": row.prev_open_amount_10k,
        }
    if row.stats_date == previous_text:
        return {
            "status": "previous_trading_day",
            "prev_amount_10k": row.amount_10k,
            "prev_seal_amount_10k": row.seal_amount_10k,
            "prev2_seal_amount_10k": row.prev_seal_amount_10k,
            "prev_open_volume_hand": row.open_volume_hand,
            "prev_open_amount_10k": row.open_amount_10k,
        }
    return _empty_alignment("stats_date_unaligned")


def _empty_alignment(status: str) -> dict[str, Any]:
    return {
        "status": status,
        "prev_amount_10k": None,
        "prev_seal_amount_10k": None,
        "prev2_seal_amount_10k": None,
        "prev_open_volume_hand": None,
        "prev_open_amount_10k": None,
    }


def _security_map(client: TdxClient, full_codes: Sequence[str]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for market in sorted({code[:2] for code in full_codes}):
        for item in client.codes.all(market):
            item_code = getattr(item, "full_code", None)
            if item_code in full_codes:
                result[str(item_code)] = item
    return result


def _by_full_code(items: Any) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for item in items or ():
        full_code = getattr(item, "full_code", None)
        if full_code is not None:
            result[str(full_code)] = item
    return result


def _code_list(codes: str | Sequence[str]) -> list[str]:
    values = [codes] if isinstance(codes, str) else list(codes)
    return list(dict.fromkeys(normalize_code(code) for code in values))


def _row_stats_date(
    stat_row: TdxStatRow | None,
    stat2_row: TdxStat2Row | None,
) -> date | None:
    value = getattr(stat2_row, "stats_date", None) or getattr(
        stat_row, "stats_date", None
    )
    return _parse_stats_date(value) if value else None


def _number(item: Any | None, name: str) -> float | None:
    value = getattr(item, name, None)
    return _round(value)


def _round(value: Any) -> float | None:
    if value is None:
        return None
    return round(float(value), 6)


def _tenk(value: Any) -> float | None:
    return None if value is None else round(float(value) * 10000.0, 6)


def _multiply(left: Any, right: Any) -> float | None:
    if left is None or right is None:
        return None
    return round(float(left) * float(right), 6)


def _safe_ratio(numerator: Any, denominator: Any) -> float | None:
    if numerator is None or denominator is None or float(denominator) == 0:
        return None
    return round(float(numerator) / float(denominator), 6)


def _safe_ratio_pct(numerator: Any, denominator: Any) -> float | None:
    if numerator is None or denominator is None or float(denominator) == 0:
        return None
    return round(float(numerator) / float(denominator) * 100.0, 6)


def _locked_amount(quote: Any | None) -> float | None:
    levels = tuple(getattr(quote, "buy_levels", ()) or ())
    if not levels:
        return None
    first = levels[0]
    return round(float(first.price) * float(first.volume) * 100.0, 6)


def _limit_board_text(days: Any, count: Any) -> str | None:
    if days is None or count is None or int(days) <= 0 or int(count) <= 0:
        return None
    return f"{int(days)}天{int(count)}板"


def _ladder_level(
    stat_row: TdxStatRow | None,
    *,
    stats_date: date | None,
    target: date,
    previous: date,
    limit_status: str,
) -> int | None:
    if limit_status != "sealed" or stat_row is None:
        return None
    prior = int(stat_row.limit_up_streak_days or 0)
    if stats_date == target:
        return max(1, prior)
    if stats_date == previous:
        return max(1, prior + 1)
    return None


def _limit_status(full_code: str, quote: Any | None, name: str | None) -> str:
    if quote is None:
        return "unknown"
    ratio = _price_limit_ratio(full_code, name)
    if ratio is None:
        return "none"
    pre_close = _number(quote, "pre_close_price")
    if not pre_close:
        return "none"
    limit_up = round(pre_close * (1.0 + ratio / 100.0) + 1e-9, 2)
    last_price = _number(quote, "last_price")
    high_price = _number(quote, "high_price")
    levels = tuple(getattr(quote, "buy_levels", ()) or ())
    bid1 = float(levels[0].price) if levels else None
    locked = _locked_amount(quote)
    if _price_close(last_price, limit_up) and (
        _price_close(bid1, limit_up) or (locked is not None and locked > 0)
    ):
        return "sealed"
    if _price_at_or_above(high_price, limit_up) or _price_at_or_above(
        last_price, limit_up
    ):
        return "touched"
    return "none"


def _price_limit_ratio(full_code: str, name: str | None) -> float | None:
    upper_name = str(name or "").strip().upper()
    if upper_name.startswith(("N", "C")):
        return None
    if upper_name.startswith(("ST", "*ST", "SST", "S*ST")):
        return 5.0
    symbol = full_code[2:]
    if full_code.startswith("bj"):
        return 30.0
    if full_code.startswith("sh688"):
        return 20.0
    if full_code.startswith("sz") and symbol.startswith(("300", "301")):
        return 20.0
    return 10.0


def _price_close(left: Any, right: Any, *, tolerance: float = 0.0051) -> bool:
    if left is None or right is None:
        return False
    return abs(float(left) - float(right)) <= tolerance


def _price_at_or_above(left: Any, right: Any, *, tolerance: float = 0.0051) -> bool:
    if left is None or right is None:
        return False
    return float(left) + tolerance >= float(right)


__all__ = [
    "ShortlineIndicator",
    "ShortlineIndicatorService",
    "ShortlineIndicatorTable",
]
