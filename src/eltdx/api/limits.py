"""Reference limit-price API."""

from __future__ import annotations

from .base import ApiBase
from eltdx.transport import SocketTransport, PooledSocketTransport
from eltdx.transport.native import native_module

_NATIVE_LIMIT_EXECUTE = {kind: kind.execute for kind in (SocketTransport, PooledSocketTransport)}


class LimitApi(ApiBase):
    def special(self, *, start_index: int = 0):
        if self._native_ready():
            return self._transport.sdk_special_limits({"start_index": start_index})
        return self._execute("special_limits", start_index=start_index)

    def scan_special(self, *, start_index: int = 0, max_rows: int = 10000):
        if (self._native_ready() and type(self) is LimitApi
                and getattr(self.special, "__func__", None) is _ORIGINAL_SPECIAL
                and type(start_index) is int and 0 <= start_index <= 0xFFFF
                and type(max_rows) is int and 0 <= max_rows <= 0x7FFFFFFF):
            return native_module().scan_special(self, start_index, max_rows)
        records = []
        index = start_index
        while index < start_index + max_rows:
            page = self.special(start_index=index)
            if not page.records:
                break
            records.extend(page.records)
            index += page.count
        return records

    def _native_ready(self):
        return (self._native_services and type(self._transport) in _NATIVE_LIMIT_EXECUTE
                and getattr(self._transport.execute, "__func__", None) is _NATIVE_LIMIT_EXECUTE[type(self._transport)])


_ORIGINAL_SPECIAL = LimitApi.special
