"""Auction API."""

from __future__ import annotations

from collections.abc import Callable
from threading import Lock

from eltdx.protocol.commands import command_code
from eltdx.transport import Transport

from .base import ApiBase


class AuctionApi(ApiBase):
    def __init__(
        self,
        transport: Transport,
        *,
        transport_factory: Callable[[], Transport] | None = None,
    ) -> None:
        super().__init__(transport)
        self._transport_factory = transport_factory
        self._dedicated_transport: Transport | None = None
        self._transport_lock = Lock()

    def _active_transport(self) -> Transport:
        if self._transport_factory is None:
            return self._transport
        with self._transport_lock:
            if self._dedicated_transport is None:
                self._dedicated_transport = self._transport_factory()
            return self._dedicated_transport

    def _close(self) -> None:
        with self._transport_lock:
            if self._dedicated_transport is not None:
                self._dedicated_transport.close()
                self._dedicated_transport = None

    def series(self, code: str, date=None, *, include_raw: bool = False):
        """Read current or historical ``0x056a`` snapshots from the dedicated pool."""

        return self._active_transport().execute(
            command_code("auction_series"),
            {"code": code, "trading_date": date, "include_raw": include_raw},
        )
