"""Shared API helpers."""

from __future__ import annotations

from typing import Any

from eltdx.protocol.commands import command_code
from eltdx.transport import Transport


class ApiBase:
    """Base class for capability-specific APIs."""

    def __init__(self, transport: Transport, *, price_resolver=None, metadata_sink=None) -> None:
        self._transport = transport
        self._price_resolver = price_resolver
        self._metadata_sink = metadata_sink

    def _execute(self, command_name: str, **payload: Any) -> Any:
        return self._transport.execute(command_code(command_name), payload)

    def _execute_priced(self, command_name: str, *, price_codes=None, **payload: Any) -> Any:
        if self._price_resolver is not None:
            self._price_resolver(command_name, None, price_codes)
        result = self._execute(command_name, **payload)
        if self._price_resolver is None:
            return result
        return self._price_resolver(command_name, result, price_codes)
