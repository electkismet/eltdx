"""Corporate action and finance API."""

from __future__ import annotations

from collections.abc import Sequence
from concurrent.futures import ThreadPoolExecutor
from typing import Any

from eltdx.equity import build_adjustment_factor_response
from eltdx.models import AdjustmentFactorBatch, CapitalChangeBatch, CapitalChangeBlock
from eltdx.protocol.constants import (
    DEFAULT_CAPITAL_CHANGE_BATCH_SIZE,
    MAX_CAPITAL_CHANGE_CODES,
)
from eltdx.protocol.unit import normalize_code

from .base import ApiBase


FINANCE_FIELD_ALIASES = {
    "流通股本": "circulating_shares",
    "总股本": "total_shares",
    "总资产": "total_assets_yuan",
    "净利润": "net_profit_yuan",
}


class CorporateApi(ApiBase):
    def capital_changes(
        self,
        code: str | Sequence[str],
        *,
        include_raw: bool = False,
        batch_size: int = DEFAULT_CAPITAL_CHANGE_BATCH_SIZE,
    ):
        _validate_capital_change_batch_size(batch_size)
        if isinstance(code, str):
            return self._execute("capital_changes", code=code, include_raw=include_raw)

        codes = [normalize_code(item) for item in code]
        if not codes:
            raise ValueError("codes must not be empty")
        chunks = [
            codes[index : index + batch_size]
            for index in range(0, len(codes), batch_size)
        ]

        if len(chunks) == 1:
            responses = [
                self._capital_changes_chunk(chunks[0], include_raw=include_raw)
            ]
        else:
            with ThreadPoolExecutor(
                max_workers=self._batch_workers(len(chunks))
            ) as executor:
                responses = list(
                    executor.map(
                        lambda chunk: self._capital_changes_chunk(
                            chunk, include_raw=include_raw
                        ),
                        chunks,
                    )
                )

        blocks = []
        raw_payloads = []
        for response in responses:
            if isinstance(response, CapitalChangeBlock):
                blocks.append(response)
                if response.raw_payload:
                    raw_payloads.append(response.raw_payload)
            elif isinstance(response, CapitalChangeBatch):
                blocks.extend(response.blocks)
                raw_payloads.extend(response.raw_payloads)
            else:
                return response
        return CapitalChangeBatch(tuple(blocks), tuple(raw_payloads))

    def adjustment_factors(
        self,
        code: str | Sequence[str],
        anchor_date=None,
        *,
        start_date=None,
        batch_size: int = DEFAULT_CAPITAL_CHANGE_BATCH_SIZE,
    ):
        changes = self.capital_changes(code, batch_size=batch_size)
        if isinstance(changes, CapitalChangeBatch):
            return AdjustmentFactorBatch(
                tuple(
                    build_adjustment_factor_response(
                        block,
                        anchor_date=anchor_date,
                        start_date=start_date,
                    )
                    for block in changes.blocks
                )
            )
        if not isinstance(changes, CapitalChangeBlock):
            return changes
        return build_adjustment_factor_response(
            changes,
            anchor_date=anchor_date,
            start_date=start_date,
        )

    def _capital_changes_chunk(self, codes: list[str], *, include_raw: bool):
        remaining = codes
        blocks = []
        raw_payloads = []
        while remaining:
            response = self._execute(
                "capital_changes", codes=remaining, include_raw=include_raw
            )
            if isinstance(response, CapitalChangeBlock):
                returned = (response,)
                payloads = (response.raw_payload,) if response.raw_payload else ()
            elif isinstance(response, CapitalChangeBatch):
                returned = response.blocks
                payloads = response.raw_payloads
            else:
                return response

            if not returned:
                raise RuntimeError(
                    "capital_changes returned no blocks for a non-empty request"
                )
            expected_prefix = remaining[: len(returned)]
            actual_prefix = [block.full_code for block in returned]
            if actual_prefix != expected_prefix:
                raise RuntimeError(
                    "capital_changes returned blocks outside the requested prefix"
                )

            blocks.extend(returned)
            raw_payloads.extend(payloads)
            remaining = remaining[len(returned) :]
        return CapitalChangeBatch(tuple(blocks), tuple(raw_payloads))

    def _batch_workers(self, batch_count: int) -> int:
        pool_size = getattr(self._transport, "pool_size", 1)
        capacity = pool_size if isinstance(pool_size, int) and pool_size > 0 else 1
        return min(batch_count, capacity)

    def finance_batch(
        self,
        codes: str | Sequence[str],
        fields: Sequence[str] | None = None,
        *,
        include_raw: bool = False,
    ):
        code_list = [codes] if isinstance(codes, str) else list(codes)
        batch = self._execute("finance_batch", codes=code_list, include_raw=include_raw)
        if fields is None:
            return batch
        if not hasattr(batch, "records"):
            return batch
        return [_select_finance_fields(record, fields) for record in batch.records]


def _select_finance_fields(record: Any, fields: Sequence[str]) -> dict[str, Any]:
    selected: dict[str, Any] = {"full_code": record.full_code}
    for field in fields:
        attr = FINANCE_FIELD_ALIASES.get(field, field)
        if not hasattr(record, attr):
            raise ValueError(f"unknown finance field: {field}")
        selected[str(field)] = getattr(record, attr)
    return selected


def _validate_capital_change_batch_size(value: int) -> None:
    if not isinstance(value, int) or isinstance(value, bool):
        raise ValueError("batch_size must be an integer")
    if value <= 0 or value > MAX_CAPITAL_CHANGE_CODES:
        raise ValueError(f"batch_size must be between 1 and {MAX_CAPITAL_CHANGE_CODES}")
