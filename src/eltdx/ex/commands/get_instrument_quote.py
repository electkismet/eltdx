"""获取扩展行情实时五档报价。"""

from __future__ import annotations

import struct
from sys import modules as _MODULES

from eltdx.ex.commands.base import BaseCommand
from eltdx.ex.models import ExInstrumentQuote


class GetExInstrumentQuoteCmd(BaseCommand[ExInstrumentQuote | None]):
    """获取单个商品的五档实时行情。"""

    def __init__(self, market: int, code: str) -> None:
        from eltdx._native_abi import load_native
        load_native().ex_command_init("quote", self, (market, code), _MODULES[__name__])

    def build_request(self) -> bytes:
        """构造请求包。"""

        from eltdx._native_abi import load_native
        return load_native().ex_command_build("quote", self, _MODULES[__name__])

    def parse_response(self, body: bytes) -> ExInstrumentQuote | None:
        """解析五档报价响应。"""

        from eltdx._native_abi import load_native
        return load_native().ex_response_parse("quote", self, body, _MODULES[__name__])

__all__ = ["GetExInstrumentQuoteCmd"]
