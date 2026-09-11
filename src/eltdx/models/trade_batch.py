"""Opt-in bulk trade results without eager per-record Python model creation."""

from __future__ import annotations

from bisect import bisect_right
from collections.abc import Iterable
from dataclasses import dataclass, field, fields, replace
from datetime import date, datetime
from itertools import accumulate
from operator import index as integer_index
from typing import Any, ClassVar

from .trade import TradePage, TradeTick


_COLUMNS = tuple(item.name for item in fields(TradeTick))
_STRIDE = 19
_COLUMN_INDEX = {name: offset for offset, name in enumerate(_COLUMNS)}


def _as_tick(block: tuple[Any, ...], offset: int) -> TradeTick:
    from eltdx._native_models import _trade_tick_at

    # Custom transports may supply datetimes with microseconds or custom tzinfo.
    # Keep those exact values rather than reducing them to the wire date parts.
    dt = block[offset + 4]
    if isinstance(dt, datetime):
        row = block[offset:offset + _STRIDE]
        return replace(_trade_tick_at(row[:4] + (None,) + row[5:], 0), trade_datetime=dt)
    return _trade_tick_at(block, offset)


@dataclass(frozen=True, slots=True)
class TradeBatch:
    """A page or complete history stored in immutable, flat field blocks.

    ``column()`` and ``select()`` do not construct TradeTick objects. ``tick()``
    creates one object; ``to_page()`` explicitly materializes every record.
    The private block layout is not a public serialization format.
    """

    exchange: str
    market_id: int
    code: str
    start: int
    request_count: int
    _blocks: tuple[tuple[Any, ...], ...] = field(repr=False)
    trading_date: date | None = None
    price_base_raw_f32: float | None = None
    raw_payload: bytes = b""
    _ends: tuple[int, ...] = field(init=False, repr=False, compare=False)

    columns: ClassVar[tuple[str, ...]] = _COLUMNS

    def __post_init__(self) -> None:
        if not isinstance(self._blocks, tuple) or any(
            not isinstance(block, tuple) or len(block) % _STRIDE
            for block in self._blocks
        ):
            raise ValueError("trade batch blocks must be tuples of complete 19-field records")
        object.__setattr__(
            self, "_ends", tuple(accumulate(len(block) // _STRIDE for block in self._blocks))
        )

    @property
    def full_code(self) -> str:
        return f"{self.exchange}{self.code}"

    @property
    def count(self) -> int:
        return self._ends[-1] if self._ends else 0

    def __len__(self) -> int:
        return self.count

    @property
    def has_more(self) -> bool:
        return self.count > 0

    def column(self, name: str) -> tuple[Any, ...]:
        """Return one named field for all rows, using TradeTick value types.

        Only the requested column is built. Requesting ``trade_datetime``
        converts that column to datetime objects, without constructing ticks.
        """
        from eltdx._native_models import _datetime

        try:
            offset = _COLUMN_INDEX[name]
        except KeyError:
            raise KeyError(f"unknown trade column: {name!r}") from None
        values: list[Any] = []
        for block in self._blocks:
            values.extend(block[offset::_STRIDE])
        if name == "trade_datetime":
            return tuple(value if isinstance(value, datetime) else _datetime(value) for value in values)
        return tuple(values)

    def to_columns(self, names: Iterable[str] | None = None) -> dict[str, tuple[Any, ...]]:
        """Export selected columns, or all fields when names is omitted."""
        return {name: self.column(name) for name in (self.columns if names is None else names)}

    def _position(self, index: int) -> tuple[tuple[Any, ...], int]:
        index = integer_index(index)
        if index < 0:
            index += self.count
        if index < 0 or index >= self.count:
            raise IndexError("trade batch index out of range")
        block_index = bisect_right(self._ends, index)
        previous = self._ends[block_index - 1] if block_index else 0
        return self._blocks[block_index], (index - previous) * _STRIDE

    def tick(self, index: int) -> TradeTick:
        """Materialize one row. Negative indices count from the end."""
        block, offset = self._position(index)
        return _as_tick(block, offset)

    def select(self, indices: Iterable[int]) -> TradeBatch:
        """Copy selected fields in the supplied order, without creating ticks.

        Repeated/negative indices are supported. The result has start=0 and
        request_count=count. Its raw payload is cleared because the original
        page bytes no longer describe this subset; per-record fields remain.
        """
        values: list[Any] = []
        for index in indices:
            block, offset = self._position(index)
            values.extend(block[offset:offset + _STRIDE])
        return replace(
            self, start=0, request_count=len(values) // _STRIDE,
            _blocks=(tuple(values),), raw_payload=b"",
        )

    def to_page(self) -> TradePage:
        """Materialize a regular TradePage, including all TradeTick objects."""
        ticks = tuple(
            _as_tick(block, offset)
            for block in self._blocks
            for offset in range(0, len(block), _STRIDE)
        )
        return TradePage(
            self.exchange, self.market_id, self.code, self.start,
            self.request_count, ticks, self.trading_date,
            self.price_base_raw_f32, self.raw_payload,
        )

    @classmethod
    def from_page(cls, page: TradePage) -> TradeBatch:
        """Convert an existing page; this cannot undo its object creation cost.

        Also provides compatibility for custom transports that only implement
        the original ``execute()`` method.
        """
        values: list[Any] = []
        for tick in page.ticks:
            values.extend(getattr(tick, name) for name in _COLUMNS)
        return cls(
            page.exchange, page.market_id, page.code, page.start,
            page.request_count, (tuple(values),), page.trading_date,
            page.price_base_raw_f32, page.raw_payload,
        )
