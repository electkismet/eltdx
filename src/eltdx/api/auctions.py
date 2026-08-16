"""Auction API."""

from __future__ import annotations

from .base import ApiBase


class AuctionApi(ApiBase):
    def series(self, code: str, date=None, *, include_raw: bool = False):
        """Return all call-auction process snapshots supplied by ``0x056a``."""

        return self._execute(
            "auction_series",
            code=code,
            trading_date=date,
            include_raw=include_raw,
        )
