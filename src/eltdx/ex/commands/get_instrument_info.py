"""获取扩展行情商品信息。"""

from __future__ import annotations

import struct
from sys import modules as _MODULES

from eltdx.ex._binary import slice_bytes
from eltdx.ex.commands.base import BaseCommand
from eltdx.ex.models import ExInstrumentInfo


class GetExInstrumentInfoCmd(BaseCommand[list[ExInstrumentInfo]]):
    """获取扩展行情市场中的商品信息列表（分页）。"""

    def __init__(self, start: int, count: int = 100) -> None:
        from eltdx._native_abi import load_native
        load_native().ex_command_init("info", self, (start, count), _MODULES[__name__])

    def build_request(self) -> bytes:
        """构造请求包。"""

        from eltdx._native_abi import load_native
        return load_native().ex_command_build("info", self, _MODULES[__name__])

    def parse_response(self, body: bytes) -> list[ExInstrumentInfo]:
        """解析商品信息列表响应。"""

        from eltdx._native_abi import load_native
        return load_native().ex_response_parse("info", self, body, _MODULES[__name__])

__all__ = ["GetExInstrumentInfoCmd"]
