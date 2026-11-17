"""获取扩展行情市场列表。"""

from __future__ import annotations

import struct
from sys import modules as _MODULES

from eltdx.ex._binary import slice_bytes
from eltdx.ex.commands.base import BaseCommand
from eltdx.ex.models import ExMarketInfo


class GetExMarketsCmd(BaseCommand[list[ExMarketInfo]]):
    """获取扩展行情支持的市场列表。"""

    def build_request(self) -> bytes:
        """构造请求包。"""

        from eltdx._native_abi import load_native
        return load_native().ex_command_build("markets", self, _MODULES[__name__])

    def parse_response(self, body: bytes) -> list[ExMarketInfo]:
        """解析市场列表响应。"""

        from eltdx._native_abi import load_native
        return load_native().ex_response_parse("markets", self, body, _MODULES[__name__])

__all__ = ["GetExMarketsCmd"]
