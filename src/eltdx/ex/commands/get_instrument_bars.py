"""获取扩展行情K线数据。"""

from __future__ import annotations

import struct
from sys import modules as _MODULES

from eltdx.ex.commands.base import BaseCommand
from eltdx.ex.datetime_ import get_datetime
from eltdx.ex.models import ExInstrumentBar


class GetExInstrumentBarsCmd(BaseCommand[list[ExInstrumentBar]]):
    """获取K线数据（扩展行情版本，支持期货/港股等）。"""

    def __init__(
        self,
        category: int,
        market: int,
        code: str,
        start: int = 0,
        count: int = 700,
    ) -> None:
        from eltdx._native_abi import load_native
        load_native().ex_command_init("bars", self, (category, market, code, start, count), _MODULES[__name__])

    def build_request(self) -> bytes:
        """构造请求包。"""

        from eltdx._native_abi import load_native
        return load_native().ex_command_build("bars", self, _MODULES[__name__])

    def parse_response(self, body: bytes) -> list[ExInstrumentBar]:
        """解析K线响应。"""

        from eltdx._native_abi import load_native
        return load_native().ex_response_parse("bars", self, body, _MODULES[__name__])

__all__ = ["GetExInstrumentBarsCmd"]
