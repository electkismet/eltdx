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
    from eltdx.transport.native import native_module

    return native_module().helper_market_context(client, globals())


def _client_handshake_info(client: TdxClient) -> Any | None:
    from eltdx.transport.native import native_module

    return native_module().helper_handshake(client)


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
    from eltdx.transport.native import native_module

    return native_module().helper_shortline_by_code(items)


def _code_list(codes: str | Sequence[str]) -> list[str]:
    values = [codes] if isinstance(codes, str) else list(codes)
    return list(dict.fromkeys(normalize_code(code) for code in values))


def _row_stats_date(
    stat_row: TdxStatRow | None,
    stat2_row: TdxStat2Row | None,
) -> date | None:
    from eltdx.transport.native import native_module

    return native_module().helper_shortline_row_date(stat_row, stat2_row, _parse_stats_date)


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
    from eltdx.transport.native import native_module

    return native_module().shortline_board_text(days, count)


def _ladder_level(
    stat_row: TdxStatRow | None,
    *,
    stats_date: date | None,
    target: date,
    previous: date,
    limit_status: str,
) -> int | None:
    from eltdx.transport.native import native_module

    return native_module().shortline_ladder_level(stat_row, stats_date, target, previous, limit_status)


def _limit_status(full_code: str, quote: Any | None, name: str | None) -> str:
    from eltdx.transport.native import native_module

    return native_module().shortline_limit_status(full_code, quote, name, globals())


def _price_limit_ratio(full_code: str, name: str | None) -> float | None:
    from eltdx.transport.native import native_module

    return native_module().shortline_limit_ratio(full_code, name)


def _price_close(left: Any, right: Any, *, tolerance: float = 0.0051) -> bool:
    from eltdx.transport.native import native_module

    return native_module().shortline_price_compare(left, right, tolerance, False)


def _price_at_or_above(left: Any, right: Any, *, tolerance: float = 0.0051) -> bool:
    from eltdx.transport.native import native_module

    return native_module().shortline_price_compare(left, right, tolerance, True)


__all__ = [
    "ShortlineIndicator",
    "ShortlineIndicatorService",
    "ShortlineIndicatorTable",
]
