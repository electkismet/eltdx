"""Connection/session API."""

from __future__ import annotations

from eltdx.protocol.commands import command_code

from .base import ApiBase


class SessionApi(ApiBase):
    def ping(self) -> str:
        return self._transport.request("ping")

    def handshake(self):
        if self._native_services:
            return self._transport.sdk_session(command_code("handshake"), {})
        return self._execute("handshake")

    def heartbeat(self):
        if self._native_services:
            return self._transport.sdk_session(command_code("heartbeat"), {})
        return self._execute("heartbeat")
