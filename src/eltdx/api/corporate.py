"""Corporate action and finance API."""

from __future__ import annotations

from collections.abc import Sequence
from typing import Any

from eltdx.equity import build_adjustment_factor_response

from .base import ApiBase


FINANCE_FIELD_ALIASES = {
    "流通股本": "circulating_shares",
    "总股本": "total_shares",
    "总资产": "total_assets_yuan",
    "净利润": "net_profit_yuan",
}


class CorporateApi(ApiBase):
    def capital_changes(self, code: str, *, include_raw: bool = False):
        return self._execute("capital_changes", code=code, include_raw=include_raw)

    def adjustment_factors(self, code: str, anchor_date=None, *, start_date=None):
        changes = self.capital_changes(code)
        if not hasattr(changes, "records"):
            return changes
        return build_adjustment_factor_response(
            changes,
            anchor_date=anchor_date,
            start_date=start_date,
        )

    def finance_batch(self, codes: str | Sequence[str], fields: Sequence[str] | None = None, *, include_raw: bool = False):
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
