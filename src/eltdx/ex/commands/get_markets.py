"""获取扩展行情市场列表。"""

from __future__ import annotations

import struct

from eltdx.ex._binary import slice_bytes
from eltdx.ex.commands.base import BaseCommand
from eltdx.ex.models import ExMarketInfo


class GetExMarketsCmd(BaseCommand[list[ExMarketInfo]]):
    """获取扩展行情支持的市场列表。"""

    def build_request(self) -> bytes:
        """构造请求包。"""

        return bytes.fromhex("01 02 48 69 00 01 02 00 02 00 f4 23")

    def parse_response(self, body: bytes) -> list[ExMarketInfo]:
        """解析市场列表响应。"""

        if len(body) < 2:
            return []
        (count,) = struct.unpack_from("<H", body, 0)
        pos = 2
        results: list[ExMarketInfo] = []
        for _ in range(count):
            raw = slice_bytes(body, pos, 64, "ex markets record") if pos + 64 <= len(body) else None
            if raw is None:
                break
            (category, raw_name, market, raw_short_name) = struct.unpack("<B32sB2s", raw[:36])
            pos += 64
            if category == 0 and market == 0:
                continue
            name = raw_name.decode("gbk", errors="replace").rstrip("\x00")
            short_name = raw_short_name.decode("gbk", errors="replace").rstrip("\x00")
            results.append(
                ExMarketInfo(
                    market=market,
                    category=category,
                    name=name,
                    short_name=short_name,
                    _raw=raw,
                )
            )
        return results


__all__ = ["GetExMarketsCmd"]
