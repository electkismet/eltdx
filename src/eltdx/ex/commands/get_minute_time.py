"""获取扩展行情分时数据（当日 + 历史）。"""

from __future__ import annotations

import struct
from sys import modules as _MODULES

from eltdx.ex.commands.base import BaseCommand
from eltdx.ex.models import ExMinuteBar


class GetExMinuteTimeDataCmd(BaseCommand[list[ExMinuteBar]]):
    """获取当日分时行情数据。"""

    def __init__(self, market: int, code: str) -> None:
        from eltdx._native_abi import load_native
        load_native().ex_command_init("minute", self, (market, code), _MODULES[__name__])

    def build_request(self) -> bytes:
        """构造请求包。"""

        from eltdx._native_abi import load_native
        return load_native().ex_command_build("minute", self, _MODULES[__name__])

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
        from eltdx._native_abi import load_native
        load_native().ex_command_init("history_minute", self, (market, code, date), _MODULES[__name__])

    def build_request(self) -> bytes:
        """构造请求包。"""

        from eltdx._native_abi import load_native
        return load_native().ex_command_build("history_minute", self, _MODULES[__name__])

    def parse_response(self, body: bytes) -> list[ExMinuteBar]:
        """解析历史分时响应。"""

        if len(body) < 20:
            return []
        (_market, _code, _unk, num) = struct.unpack("<B9s8sH", body[0:20])
        return GetExMinuteTimeDataCmd._parse_records(body, 20, num)


__all__ = ["GetExMinuteTimeDataCmd", "GetExHistoryMinuteTimeDataCmd"]
