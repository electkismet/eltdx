"""获取扩展行情K线数据。"""

from __future__ import annotations

import struct

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
        if not code:
            raise ValueError("code 不能为空")
        if start < 0:
            raise ValueError(f"start 必须 >= 0，当前: {start}")
        if count <= 0:
            raise ValueError(f"count 必须 > 0，当前: {count}")
        self.category = category
        self.market = market
        self.code = code.encode("utf-8")
        self.start = start
        self.count = count

    def build_request(self) -> bytes:
        """构造请求包。"""

        header = bytes.fromhex("01 01 08 6a 01 01 16 00 16 00 ff 23")
        return header + struct.pack(
            "<B9sHHIH",
            self.market,
            self.code,
            self.category,
            1,
            self.start,
            self.count,
        )

    def parse_response(self, body: bytes) -> list[ExInstrumentBar]:
        """解析K线响应。"""

        pos = 18  # skip 18-byte header
        if pos + 2 > len(body):
            return []
        (ret_count,) = struct.unpack("<H", body[pos : pos + 2])
        pos += 2
        results: list[ExInstrumentBar] = []
        for _ in range(ret_count):
            record_start = pos
            year, month, day, hour, minute, pos = get_datetime(self.category, body, pos)
            if pos + 28 > len(body):
                break
            (open_p, high, low, close_p, position, trade, _price) = struct.unpack(
                "<ffffIIf",
                body[pos : pos + 28],
            )
            (amount,) = struct.unpack("<f", body[pos + 16 : pos + 20])
            pos += 28
            results.append(
                ExInstrumentBar(
                    open=open_p,
                    high=high,
                    low=low,
                    close=close_p,
                    position=position,
                    trade=trade,
                    amount=amount,
                    year=year,
                    month=month,
                    day=day,
                    hour=hour,
                    minute=minute,
                    _raw=body[record_start:pos],
                )
            )
        return results


__all__ = ["GetExInstrumentBarsCmd"]
