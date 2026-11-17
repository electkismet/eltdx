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

        from eltdx._native_abi import load_native
        return load_native().ex_response_parse("minute", self, body, _MODULES[__name__])

    @staticmethod
    def _parse_records(body: bytes, pos: int, num: int) -> list[ExMinuteBar]:
        from eltdx._native_abi import load_native
        return load_native().ex_records_parse("minute", body, pos, num, _MODULES[__name__])


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

        from eltdx._native_abi import load_native
        return load_native().ex_response_parse("history_minute", self, body, _MODULES[__name__])


__all__ = ["GetExMinuteTimeDataCmd", "GetExHistoryMinuteTimeDataCmd"]
