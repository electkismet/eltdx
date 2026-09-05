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

    The gateway uses short native names (``ZQDM``, ``zglb`` and so on).
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
        """Alias for the native board-level field."""

        return self.board_level

    @property
    def consecutive_limit_days(self) -> int | None:
        """Alias for the consecutive limit-up day count."""

        return self.ladder_days

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
        return self.board_level

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
