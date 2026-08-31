"""7709 daily money-flow API."""

from __future__ import annotations

from eltdx.protocol.constants import TYPE_MONEY_FLOW

from .base import ApiBase


class MoneyFlowApi(ApiBase):
    """Read the latest daily money-flow records for one security."""

    def daily(self, code: str, *, include_raw: bool = False):
        # 0x0FFC is registered in the native protocol layer but intentionally
        # remains outside the legacy 21-command public registry until live
        # session initialization is fully reproduced.
        response = self._transport.execute(
            TYPE_MONEY_FLOW, {"code": code, "include_raw": include_raw}
        )
        blocks = getattr(response, "blocks", ())
        if len(blocks) == 1:
            return blocks[0]
        return response
