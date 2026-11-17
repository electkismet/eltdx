"""获取扩展行情商品数量。"""

from __future__ import annotations

from sys import modules as _MODULES

from eltdx.ex._binary import unpack_from
from eltdx.ex.commands.base import BaseCommand


class GetExInstrumentCountCmd(BaseCommand[int]):
    """获取扩展行情市场中商品总数。"""

    def build_request(self) -> bytes:
        """构造请求包。"""

        from eltdx._native_abi import load_native
        return load_native().ex_command_build("count", self, _MODULES[__name__])

    def parse_response(self, body: bytes) -> int:
        """解析商品总数响应。"""

        from eltdx._native_abi import load_native
        return load_native().ex_response_parse("count", self, body, _MODULES[__name__])

__all__ = ["GetExInstrumentCountCmd"]
