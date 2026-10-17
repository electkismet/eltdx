"""获取扩展行情分时数据（当日 + 历史）。"""

from __future__ import annotations

import struct

from eltdx.ex.commands.base import BaseCommand
from eltdx.ex.models import ExMinuteBar


class GetExMinuteTimeDataCmd(BaseCommand[list[ExMinuteBar]]):
    """获取当日分时行情数据。"""

    def __init__(self, market: int, code: str) -> None:
        if not code:
            raise ValueError("code 不能为空")
        self.market = market
        self.code = code.encode("utf-8")

    def build_request(self) -> bytes:
        """构造请求包。"""

        header = bytes.fromhex("01 07 08 00 01 01 0c 00 0c 00 0b 24")
        return header + struct.pack("<B9s", self.market, self.code)

    def parse_response(self, body: bytes) -> list[ExMinuteBar]:
        """解析当日分时响应。"""

        if len(body) < 12:
            return []
        (_market, _code, num) = struct.unpack("<B9sH", body[0:12])
        return self._parse_records(body, 12, num)

    @staticmethod
    def _parse_records(body: bytes, pos: int, num: int) -> list[ExMinuteBar]:
        results: list[ExMinuteBar] = []
        for _ in range(num):
            if pos + 18 > len(body):
                break
            record_start = pos
            (raw_time, price, avg_price, volume, amount) = struct.unpack("<HffII", body[pos : pos + 18])
            pos += 18
            results.append(
                ExMinuteBar(
                    hour=raw_time // 60,
                    minute=raw_time % 60,
                    price=price,
                    avg_price=avg_price,
                    volume=volume,
                    open_interest=amount,
                    _raw=body[record_start:pos],
                )
            )
        return results


class GetExHistoryMinuteTimeDataCmd(BaseCommand[list[ExMinuteBar]]):
    """获取历史某日分时行情数据。"""

    def __init__(self, market: int, code: str, date: int) -> None:
        if not code:
            raise ValueError("code 不能为空")
        self.market = market
        self.code = code.encode("utf-8")
        self.date = date

    def build_request(self) -> bytes:
        """构造请求包。"""

        header = bytes.fromhex("01 01 30 00 01 01 10 00 10 00 0c 24")
        return header + struct.pack("<IB9s", self.date, self.market, self.code)

    def parse_response(self, body: bytes) -> list[ExMinuteBar]:
        """解析历史分时响应。"""

        if len(body) < 20:
            return []
        (_market, _code, _unk, num) = struct.unpack("<B9s8sH", body[0:20])
        return GetExMinuteTimeDataCmd._parse_records(body, 20, num)


__all__ = ["GetExMinuteTimeDataCmd", "GetExHistoryMinuteTimeDataCmd"]
