"""7709 daily money-flow API."""

from __future__ import annotations

from collections.abc import Sequence
from concurrent.futures import ThreadPoolExecutor

from eltdx.models import MoneyFlowBatch, MoneyFlowBlock
from eltdx.protocol.constants import TYPE_MONEY_FLOW
from eltdx.protocol.unit import normalize_code

from .base import ApiBase


class MoneyFlowApi(ApiBase):
    """Read the latest daily money-flow records."""

    def __init__(self, transport, *, transport_factory=None) -> None:
        super().__init__(transport)
        self._transport_factory = transport_factory
        self._dedicated_transport = None

    def _active_transport(self):
        if self._transport_factory is not None and self._dedicated_transport is None:
            self._dedicated_transport = self._transport_factory()
        return self._dedicated_transport or self._transport

    def close(self) -> None:
        if self._dedicated_transport is not None:
            self._dedicated_transport.close()
            self._dedicated_transport = None

    def daily(
        self,
        code: str | Sequence[str],
        *,
        include_raw: bool = False,
        batch_size: int = 75,
    ):
        _validate_batch_size(batch_size)
        if isinstance(code, str):
            return self._single(code, include_raw=include_raw)

        codes = [normalize_code(item) for item in code]
        if not codes:
            raise ValueError("codes must not be empty")
        workers = _batch_workers(self._transport, len(codes), batch_size)
        blocks: list[MoneyFlowBlock] = []
        with ThreadPoolExecutor(max_workers=workers) as executor:
            for start in range(0, len(codes), workers):
                chunk = codes[start : start + workers]
                for response in executor.map(
                    lambda item: self._single(item, include_raw=include_raw), chunk
                ):
                    if isinstance(response, MoneyFlowBlock):
                        blocks.append(response)
                    elif isinstance(response, MoneyFlowBatch):
                        blocks.extend(response.blocks)
                    else:
                        return response
        return MoneyFlowBatch(tuple(blocks))

    def _single(self, code: str, *, include_raw: bool):
        response = self._active_transport().execute(
            TYPE_MONEY_FLOW, {"code": code, "include_raw": include_raw}
        )
        blocks = getattr(response, "blocks", ())
        if len(blocks) == 1:
            return blocks[0]
        return response


def _validate_batch_size(value: int) -> None:
    if isinstance(value, bool) or not isinstance(value, int) or value <= 0:
        raise ValueError("batch_size must be a positive integer")


def _batch_workers(transport, code_count: int, batch_size: int) -> int:
    capacity = getattr(transport, "pool_size", 1)
    if isinstance(capacity, bool) or not isinstance(capacity, int) or capacity <= 0:
        capacity = 1
    return min(code_count, batch_size, capacity)
