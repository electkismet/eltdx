"""Models for the 7615 TQLEX / F10 HTTP gateway."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from eltdx.protocol.unit import ID_TO_MARKET


@dataclass(frozen=True, slots=True)
class F10Cell:
    """A single value with its raw column name and position."""

    name: str
    value: Any
    index: int


@dataclass(frozen=True, slots=True)
class F10ResultSet:
    """One ResultSet table returned by the TQLEX gateway."""

    key: str | None
    columns: tuple[str, ...]
    rows: tuple[dict[str, Any], ...]
    row_cells: tuple[tuple[F10Cell, ...], ...]
    raw: dict[str, Any] = field(repr=False)

    @property
    def count(self) -> int:
        """Number of rows in this result set."""

        return len(self.rows)

    def first(self) -> dict[str, Any] | None:
        """Return the first row, or None when the table is empty."""

        return self.rows[0] if self.rows else None


@dataclass(frozen=True, slots=True)
class F10Response:
    """Parsed response from a 7615 TQLEX Entry."""

    entry: str
    request_body: Any
    error_code: int | None
    result_sets: tuple[F10ResultSet, ...]
    raw: dict[str, Any] = field(repr=False)

    @property
    def ok(self) -> bool:
        """Whether the gateway reported success."""

        return self.error_code in (None, 0)

    @property
    def tables(self) -> tuple[F10ResultSet, ...]:
        """Alias for result_sets, nicer for product-level examples."""

        return self.result_sets

    @property
    def first_table(self) -> F10ResultSet | None:
        """Return the first table, or None when the response has no table."""

        return self.result_sets[0] if self.result_sets else None

    @property
    def rows(self) -> tuple[dict[str, Any], ...]:
        """Rows from the first table. Empty tuple if no table exists."""

        first = self.first_table
        return first.rows if first is not None else ()

    def first_row(self) -> dict[str, Any] | None:
        """Return the first row in the first table."""

        first = self.first_table
        return first.first() if first is not None else None


@dataclass(frozen=True, slots=True)
class LimitBoardLadderRow:
    """One stock row from ``CWServ.cfg_fx_lbtt``.

    The gateway uses short native names (``ZQDM``, ``lbts`` and so on).
    Descriptive attributes are exposed here while ``raw`` keeps every value
    exactly as returned by the server for fields whose meaning can vary by
    client version.
    """

    trading_date: Any | None
    trading_date_value: Any | None
    board_level: int | None
    ladder_days: int | None
    code: str | None
    market_id: int | None
    limit_reason: Any | None
    seal_amount: float | int | None
    name: Any | None
    limit_reason_extra: Any | None
    limit_time: Any | None
    broken_count: int | None
    industry: Any | None
    limit_type: int | None
    success_rate: Any | None
    highest_board_level: int | None
    raw: dict[str, Any] = field(repr=False)
    market: str | None = field(init=False)
    full_code: str | None = field(init=False)

    def __post_init__(self) -> None:
        """Derive normalized code fields while keeping raw values intact."""

        market = ID_TO_MARKET.get(self.market_id) if self.market_id is not None else None
        object.__setattr__(self, "market", market)
        object.__setattr__(
            self,
            "full_code",
            f"{market}{self.code}" if market and self.code else self.code,
        )

    @property
    def trade_date(self) -> Any | None:
        """Return the canonical date value when the server supplied one."""

        return self.trading_date_value or self.trading_date

    @property
    def board(self) -> int | None:
        """Return this stock's board level, when it is a limit-up row."""

        return self.board_level

    @property
    def consecutive_limit_days(self) -> int | None:
        """Alias for the consecutive limit-up day count."""

        return self.ladder_days

    @property
    def status(self) -> str:
        """Return the normalized row category: ``limit_up``, ``broken`` or ``limit_down``."""

        return {1: "limit_up", 3: "broken", 0: "limit_down"}.get(
            self.limit_type, "unknown"
        )

    @property
    def reason(self) -> Any | None:
        """Return the primary reason shown by the client."""

        return self.limit_reason or self.limit_reason_extra

    # Native aliases are useful when comparing this model with the raw
    # ResultSet returned by ``F10Client.call``.
    @property
    def rq(self) -> Any | None:
        return self.trading_date

    @property
    def rqex(self) -> Any | None:
        return self.trading_date_value

    @property
    def zglb(self) -> int | None:
        return self.highest_board_level

    @property
    def lbts(self) -> int | None:
        return self.ladder_days

    @property
    def ZQDM(self) -> str | None:  # noqa: N802 - native field name
        return self.code

    @property
    def SC(self) -> int | None:  # noqa: N802 - native field name
        return self.market_id

    @property
    def ztyy(self) -> Any | None:
        return self.limit_reason

    @property
    def fde(self) -> float | int | None:
        return self.seal_amount

    @property
    def ZQJC(self) -> Any | None:  # noqa: N802 - native field name
        return self.name

    @property
    def ztyy2(self) -> Any | None:
        return self.limit_reason_extra

    @property
    def ztsj(self) -> Any | None:
        return self.limit_time

    @property
    def kbcs(self) -> int | None:
        return self.broken_count

    @property
    def sshy(self) -> Any | None:
        return self.industry

    @property
    def ztlb(self) -> int | None:
        return self.limit_type

    @property
    def cgl(self) -> Any | None:
        return self.success_rate


@dataclass(frozen=True, slots=True)
class LimitBoardLadder:
    """Parsed stock-level limit-up ladder query."""

    entry: str
    request_body: dict[str, Any]
    error_code: int | None
    start_date: str
    end_date: str
    rows: tuple[LimitBoardLadderRow, ...]
    summary: tuple[dict[str, Any], ...]
    raw: dict[str, Any] = field(repr=False)

    @property
    def ok(self) -> bool:
        """Whether the gateway reported success."""

        return self.error_code in (None, 0)

    @property
    def count(self) -> int:
        return len(self.rows)

    @property
    def trade_date(self) -> str | None:
        """Return the queried date for a single-day request."""

        return self.start_date if self.start_date == self.end_date else None

    def first_row(self) -> LimitBoardLadderRow | None:
        """Return the first detail row, or ``None`` when no rows exist."""

        return self.rows[0] if self.rows else None

    @property
    def market_overview(self) -> tuple[dict[str, Any], ...]:
        """Return the optional market totals with descriptive field names.

        The gateway's ``N002``..``N014`` fields are the page-level overview:
        advancing, declining, flat, limit-up, limit-down, turnover amounts,
        and the three leading topics.  The original row is retained as
        ``raw`` in each mapped item.
        """

        result: list[dict[str, Any]] = []
        for row in self.summary:
            result.append(
                {
                    "trade_date": row.get("N001") or row.get("t001"),
                    "advancing_count": row.get("N002"),
                    "declining_count": row.get("N003"),
                    "flat_count": row.get("N004"),
                    "limit_up_count": row.get("N005"),
                    "limit_down_count": row.get("N006"),
                    "today_amount": row.get("N007"),
                    "previous_amount": row.get("N008"),
                    "hot_topics": tuple(
                        {"name": row.get(name), "count": row.get(count)}
                        for name, count in (("N009", "N010"), ("N011", "N012"), ("N013", "N014"))
                        if row.get(name) not in (None, "")
                    ),
                    "raw": dict(row),
                }
            )
        return tuple(result)

    @property
    def overview(self) -> tuple[dict[str, Any], ...]:
        """Alias for :attr:`market_overview`."""

        return self.market_overview

    @property
    def ladder_counts(self) -> dict[int, int]:
        """Return limit-up counts grouped by ``lbts`` for a single-day query.

        For a date range use :attr:`ladder_counts_by_date` to avoid merging
        separate trading days.
        """

        counts: dict[int, int] = {}
        for row in self.rows:
            if row.status == "limit_up" and row.board_level is not None:
                counts[row.board_level] = counts.get(row.board_level, 0) + 1
        return dict(sorted(counts.items(), reverse=True))

    @property
    def ladder_counts_by_date(self) -> dict[str, dict[int, int]]:
        """Return actual board counts separately for every trading date."""

        result: dict[str, dict[int, int]] = {}
        for row in self.rows:
            if row.status != "limit_up" or row.board_level is None:
                continue
            day = str(row.trading_date_value or row.trading_date or "")
            counts = result.setdefault(day, {})
            counts[row.board_level] = counts.get(row.board_level, 0) + 1
        return {day: dict(sorted(counts.items(), reverse=True)) for day, counts in result.items()}

    @property
    def promotion_rates(self) -> dict[int, float]:
        """Return each board's promotion rate against the immediately lower board."""

        counts = self.ladder_counts
        return {
            level: counts[level] / counts[level - 1]
            for level in counts
            if counts.get(level - 1)
        }

    @property
    def promotion_rates_by_date(self) -> dict[str, dict[int, float]]:
        """Return board promotion rates separately for every trading date."""

        result: dict[str, dict[int, float]] = {}
        for day, counts in self.ladder_counts_by_date.items():
            result[day] = {
                level: counts[level] / counts[level - 1]
                for level in counts
                if counts.get(level - 1)
            }
        return result
