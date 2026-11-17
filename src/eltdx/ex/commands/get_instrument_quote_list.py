"""获取扩展行情商品列表行情（按类别）。"""

from __future__ import annotations

import struct
from collections import OrderedDict
from sys import modules as _MODULES

from eltdx.ex.commands.base import BaseCommand


class GetExInstrumentQuoteListCmd(BaseCommand[list[OrderedDict[str, object]]]):
    """按类别获取商品行情列表（期货/港股等）。"""

    def __init__(
        self,
        market: int,
        category: int,
        start: int = 0,
        count: int = 80,
    ) -> None:
        from eltdx._native_abi import load_native
        load_native().ex_command_init("quotes", self, (market, category, start, count), _MODULES[__name__])

    def build_request(self) -> bytes:
        """构造请求包。"""

        from eltdx._native_abi import load_native
        return load_native().ex_command_build("quotes", self, _MODULES[__name__])

    def parse_response(self, body: bytes) -> list[OrderedDict[str, object]]:
        """解析商品列表行情响应。"""

        from eltdx._native_abi import load_native
        return load_native().ex_response_parse("quotes", self, body, _MODULES[__name__])

    @staticmethod
    def _parse_futures(
        market: int,
        code: str,
        body: bytes,
        pos: int,
        results: list[OrderedDict[str, object]],
    ) -> int:
        from eltdx._native_abi import load_native
        return load_native().ex_quote_list_record("futures", market, code, body, pos, results, _MODULES[__name__])

    @staticmethod
    def _parse_hk_stocks(
        market: int,
        code: str,
        body: bytes,
        pos: int,
        results: list[OrderedDict[str, object]],
    ) -> int:
        from eltdx._native_abi import load_native
        return load_native().ex_quote_list_record("hk", market, code, body, pos, results, _MODULES[__name__])

__all__ = ["GetExInstrumentQuoteListCmd"]
