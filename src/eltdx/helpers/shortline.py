"""Shortline indicators composed from live quotes and TDX statistics resources."""

from __future__ import annotations

import weakref
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
    open_price: float | None = None
    pre_close: float | None = None
    open_change_pct: float | None = None
    open_amount: float | None = None
    open_volume_hand: float | None = None
    open_volume_ratio: float | None = None
    opening_rush: float | None = None
    float_shares: float | None = None
    float_market_value: float | None = None
    seal_amount: float | None = None
    seal_to_amount_ratio: float | None = None


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
        # 只存弱引用：本服务由 HelperApi/client 持有，强引用 client 会构成
        # 引用环，让 client（以及底层 ~5 条 worker 线程）只能等 GC 回收。
        self._client_ref = weakref.ref(client)
        self._stats_cache: dict[str, TdxStatsResource] = {}
        self._stats_lock = RLock()

    @property
    def _client(self) -> TdxClient:
        """解析 TdxClient 弱引用；client 已回收时明确报错而不是静默降级。"""

        client = self._client_ref()
        if client is None:
            raise ReferenceError("TdxClient 已被回收")
        return client

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

        from eltdx.transport.native import native_module

        return native_module().helper_shortline_get(
            self, full_codes, stats_path, refresh_stats, globals(),
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
            from eltdx.transport.native import native_module

            return native_module().helper_stats_resource(
                self, path, refresh, target, previous,
                _stats_resource_is_usable, _validate_stats_resource_dates,
            )


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
    from eltdx.transport.native import native_module

    return native_module().helper_validate_stats(
        resource, target, previous, _dominant_date_and_coverage, _parse_stats_date,
        MIN_DOMINANT_DATE_COVERAGE, ResourceFormatError, TdxStatsDateError,
    )


def _dominant_date_and_coverage(rows: Iterable[Any]) -> tuple[str | None, float]:
    from eltdx.transport.native import native_module

    return native_module().helper_dominant_date(rows)


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
    recent_daily_bars: Sequence[Any],
    finance: Any | None,
    opening_rush: float | None,
    stats: TdxStatsResource,
    context: _MarketDateContext,
) -> ShortlineIndicator:
    from eltdx.transport.native import native_module

    return native_module().shortline_indicator(
        full_code, quote, security, recent_daily_bars, finance,
        opening_rush, stats, context, globals(),
    )


def _aligned_stat2(
    row: TdxStat2Row | None,
    *,
    target: date,
    previous: date,
) -> dict[str, Any]:
    from eltdx.transport.native import native_module

    return native_module().helper_aligned_stat2(row, target, previous, _empty_alignment)


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
    from eltdx.transport.native import native_module

    return native_module().helper_shortline_security(client, full_codes)


def _recent_daily_bars_map(
    client: TdxClient,
    full_codes: Sequence[str],
    *,
    before: date | None = None,
) -> dict[str, tuple[Any, ...]]:
    from eltdx.transport.native import native_module

    return native_module().helper_shortline_bars(client, full_codes, before)


def _finance_map(client: TdxClient, full_codes: Sequence[str]) -> dict[str, Any]:
    from eltdx.transport.native import native_module

    return native_module().helper_shortline_finance(client, full_codes)


def _opening_rush_map(client: TdxClient, full_codes: Sequence[str]) -> dict[str, float]:
    from eltdx.transport.native import native_module

    return native_module().helper_shortline_rush(client, full_codes)


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
    from eltdx.transport.native import native_module

    return native_module().shortline_scalar("round", value, None)


def _change_pct(value: Any, base: Any) -> float | None:
    from eltdx.transport.native import native_module

    return native_module().shortline_scalar("change", value, base)


def _open_volume_ratio(open_volume_hand: Any, bars: Sequence[Any]) -> float | None:
    from eltdx.transport.native import native_module

    return native_module().shortline_volume_ratio(open_volume_hand, bars, _safe_ratio)


def _tenk(value: Any) -> float | None:
    from eltdx.transport.native import native_module

    return native_module().shortline_scalar("tenk", value, None)


def _multiply(left: Any, right: Any) -> float | None:
    from eltdx.transport.native import native_module

    return native_module().shortline_scalar("multiply", left, right)


def _safe_ratio(numerator: Any, denominator: Any) -> float | None:
    from eltdx.transport.native import native_module

    return native_module().shortline_scalar("ratio", numerator, denominator)


def _safe_ratio_pct(numerator: Any, denominator: Any) -> float | None:
    from eltdx.transport.native import native_module

    return native_module().shortline_scalar("percent", numerator, denominator)


def _locked_amount(quote: Any | None) -> float | None:
    from eltdx.transport.native import native_module

    return native_module().shortline_locked(quote)


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
    if full_code.startswith("sz") and symbol.startswith("30"):
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
