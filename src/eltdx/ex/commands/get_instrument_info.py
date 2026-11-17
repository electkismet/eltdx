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

        if len(body) < 6:
            return []
        (_start, count) = struct.unpack("<IH", body[0:6])
        pos = 6
        results: list[ExInstrumentInfo] = []
        for _ in range(count):
            if pos + 64 > len(body):
                break
            raw = slice_bytes(body, pos, 64, "ex instrument info record")
            (category, market, _unused, raw_code, raw_name, raw_desc) = struct.unpack(
                "<BB3s9s17s9s", raw[:40]
            )
            pos += 64
            code = raw_code.decode("gbk", errors="replace").rstrip("\x00")
            name = raw_name.decode("gbk", errors="replace").rstrip("\x00")
            desc = raw_desc.decode("gbk", errors="replace").rstrip("\x00")
            results.append(
                ExInstrumentInfo(
                    category=category,
                    market=market,
                    code=code,
                    name=name,
                    desc=desc,
                    _raw=raw,
                )
            )
        return results


__all__ = ["GetExInstrumentInfoCmd"]
