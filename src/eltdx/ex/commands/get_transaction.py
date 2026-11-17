"""获取扩展行情成交数据（当日 + 历史）。"""

from __future__ import annotations

import struct
from sys import modules as _MODULES

from eltdx.ex.commands.base import BaseCommand
from eltdx.ex.models import ExTransactionRecord


class GetExTransactionDataCmd(BaseCommand[list[ExTransactionRecord]]):
    """获取当日分笔成交数据。"""

    def __init__(self, market: int, code: str, start: int = 0, count: int = 1800) -> None:
        from eltdx._native_abi import load_native
        load_native().ex_command_init("transactions", self, (market, code, start, count), _MODULES[__name__])

    def build_request(self) -> bytes:
        """构造请求包。"""

        from eltdx._native_abi import load_native
        return load_native().ex_command_build("transactions", self, _MODULES[__name__])

    def parse_response(self, body: bytes) -> list[ExTransactionRecord]:
        """解析当日分笔成交响应。"""

        from eltdx._native_abi import load_native
        return load_native().ex_response_parse("transactions", self, body, _MODULES[__name__])

    @staticmethod
    def _parse_records(body: bytes, pos: int, num: int) -> list[ExTransactionRecord]:
        from eltdx._native_abi import load_native
        return load_native().ex_records_parse("transactions", body, pos, num, _MODULES[__name__])


class GetExHistoryTransactionDataCmd(BaseCommand[list[ExTransactionRecord]]):
    """获取历史某日分笔成交数据。"""

    def __init__(
        self,
        market: int,
        code: str,
        date: int,
        start: int = 0,
        count: int = 1800,
    ) -> None:
        from eltdx._native_abi import load_native
        load_native().ex_command_init("history_transactions", self, (market, code, date, start, count), _MODULES[__name__])

    def build_request(self) -> bytes:
        """构造请求包。"""

        from eltdx._native_abi import load_native
        return load_native().ex_command_build("history_transactions", self, _MODULES[__name__])

    def parse_response(self, body: bytes) -> list[ExTransactionRecord]:
        """解析历史分笔成交响应。"""

        from eltdx._native_abi import load_native
        return load_native().ex_response_parse("history_transactions", self, body, _MODULES[__name__])


__all__ = ["GetExTransactionDataCmd", "GetExHistoryTransactionDataCmd"]
