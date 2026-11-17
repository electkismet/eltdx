"""Auction API."""

from __future__ import annotations

from collections.abc import Callable
from threading import Lock

from eltdx.protocol.commands import command_code
from eltdx.transport import PooledSocketTransport, SocketTransport, Transport

from .base import ApiBase, _weak_callback


_NATIVE_AUCTION_EXECUTE = {
    SocketTransport: SocketTransport.execute,
    PooledSocketTransport: PooledSocketTransport.execute,
}


class AuctionApi(ApiBase):
    def __init__(
        self,
        transport: Transport,
        *,
        transport_factory: Callable[[], Transport] | None = None,
        native_services: bool = False,
    ) -> None:
        super().__init__(transport, native_services=native_services)
        # 弱引用保存 bound method，避免子服务与 TdxClient 互持形成引用环。
        self._transport_factory = _weak_callback(transport_factory)
        self._dedicated_transport: Transport | None = None
        self._transport_lock = Lock()

    def _active_transport(self) -> Transport:
        factory = self._transport_factory() if self._transport_factory is not None else None
        if factory is None:
            return self._transport
        with self._transport_lock:
            if self._dedicated_transport is None:
                self._dedicated_transport = factory()
            return self._dedicated_transport

    def _close(self) -> None:
        with self._transport_lock:
            if self._dedicated_transport is not None:
                self._dedicated_transport.close()
                self._dedicated_transport = None

    def series(self, code: str, date=None, *, include_raw: bool = False):
        """Read current or historical ``0x056a`` snapshots from the dedicated pool."""

        transport = self._active_transport()
        execute = transport.execute
        # A dedicated pool still belongs to the original Python lifetime and
        # host-selection policy. Overrides/factories keep their execute hook.
        if (self._native_services and type(transport) in _NATIVE_AUCTION_EXECUTE
                and getattr(execute, "__func__", None) is _NATIVE_AUCTION_EXECUTE[type(transport)]):
            execute = transport.sdk_auction
        return execute(
            command_code("auction_series"),
            {"code": code, "trading_date": date, "include_raw": include_raw},
        )
