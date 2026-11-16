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
from eltdx.protocol.commands import command_code
from eltdx.transport import PooledSocketTransport, SocketTransport
from eltdx.transport.native import native_module

from .base import ApiBase
from ._batch import recover_batch, _recover_finance_native
from . import _batch


_NATIVE_CORPORATE_EXECUTE = {
    SocketTransport: SocketTransport.execute,
    PooledSocketTransport: PooledSocketTransport.execute,
}


FINANCE_FIELD_ALIASES = {
    "流通股本": "circulating_shares",
    "总股本": "total_shares",
    "总资产": "total_assets_yuan",
    "净利润": "net_profit_yuan",
}


class CorporateApi(ApiBase):
    def _execute(self, command_name: str, **payload: Any) -> Any:
        execute = self._transport.execute
        if (self._native_services and command_name in ("capital_changes", "finance_batch")
                and type(self._transport) in _NATIVE_CORPORATE_EXECUTE
                and getattr(execute, "__func__", None) is _NATIVE_CORPORATE_EXECUTE[type(self._transport)]):
            execute = self._transport.sdk_corporate
        return execute(command_code(command_name), payload)

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
        chunks = native_module().batch_chunks(codes, batch_size)

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

        return native_module().assemble_capital(responses, CapitalChangeBlock, CapitalChangeBatch)

    def adjustment_factors(
        self,
        code: str | Sequence[str],
        anchor_date=None,
        *,
        start_date=None,
        batch_size: int = DEFAULT_CAPITAL_CHANGE_BATCH_SIZE,
    ):
        changes = self.capital_changes(code, batch_size=batch_size)
        return native_module().adjustment_batch(
            changes, anchor_date, start_date, build_adjustment_factor_response,
            CapitalChangeBlock, CapitalChangeBatch, AdjustmentFactorBatch,
        )

    def _capital_changes_chunk(self, codes: list[str], *, include_raw: bool):
        if self._native_services and type(self) is CorporateApi:
            return native_module().collect_capital(
                codes,
                lambda part: self._execute("capital_changes", codes=part, include_raw=include_raw),
                CapitalChangeBlock, CapitalChangeBatch,
            )
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
        on_missing: str = "raise",
    ):
        code_list = [codes] if isinstance(codes, str) else list(codes)
        recovery = (_recover_finance_native
                    if self._native_services and type(self) is CorporateApi
                    and _batch._RECOVERY_REQUESTS == 32 else recover_batch)
        recovery_kwargs = {} if recovery is _recover_finance_native else {"kind": "finance"}
        batch = recovery(
            code_list,
            lambda part: self._execute("finance_batch", codes=part, include_raw=include_raw),
            on_missing=on_missing, **recovery_kwargs,
        )
        if fields is None:
            return batch
        if not hasattr(batch, "records"):
            return batch
        if self._native_services and _select_finance_fields is _ORIGINAL_FINANCE_SELECTOR:
            return native_module().project_finance_fields(batch.records, fields, FINANCE_FIELD_ALIASES)
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


_ORIGINAL_FINANCE_SELECTOR = _select_finance_fields
