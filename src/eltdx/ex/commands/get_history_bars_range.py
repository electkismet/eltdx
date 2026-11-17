"""获取扩展行情历史K线（按日期范围）。"""

from __future__ import annotations

import struct
from sys import modules as _MODULES

from eltdx.ex.commands.base import BaseCommand
from eltdx.ex.models import ExInstrumentBar


class GetExHistoryInstrumentBarsRangeCmd(BaseCommand[list[ExInstrumentBar]]):
    """按日期范围获取历史K线数据。"""

    #: 请求头里的序号只占 1 字节，所以取值范围是 1..255。0 不用：留作「无序号」的
    #: 哨兵值，回绕时跳过它可以避免和首包撞成同一个值。
    _SEQID_MIN = 1
    _SEQID_MAX = 255

    _seqid: int = _SEQID_MIN

    def __init__(self, market: int, code: str, start_date: int, end_date: int) -> None:
        from eltdx._native_abi import load_native
        load_native().ex_command_init("range", self, (market, code, start_date, end_date), _MODULES[__name__])

    def _next_seqid(self) -> int:
        """取下一个请求序号，到 ``_SEQID_MAX`` 后回绕到 ``_SEQID_MIN``。

        !!! warning "必须回绕"
            序号是**类级**变量且单调递增，早期实现直接 ``struct.pack("<B", self._seqid)``。
            第 257 次构造请求时值变成 256，``<B`` 编码抛
            ``struct.error: 'B' format requires 0 <= number <= 255``。
            由于状态在类上，**新建客户端也恢复不了** —— 一次长会话里的
            第 257 次调用必然失败。回绕后可以无限次调用。
        """

        from eltdx._native_abi import load_native
        return load_native().ex_command_sequence(self, _MODULES[__name__])

    def build_request(self) -> bytes:
        """构造请求包（序号自增并按单字节范围回绕）。"""

        from eltdx._native_abi import load_native
        return load_native().ex_command_build("range", self, _MODULES[__name__])

    @staticmethod
    def _parse_date(num: int) -> tuple[int, int, int]:
        from eltdx._native_abi import load_native
        return load_native().ex_date_parts("range", num)

    @staticmethod
    def _parse_time(num: int) -> tuple[int, int]:
        from eltdx._native_abi import load_native
        return load_native().ex_date_parts("clock", num)

    def parse_response(self, body: bytes) -> list[ExInstrumentBar]:
        """解析历史K线响应。"""

        from eltdx._native_abi import load_native
        return load_native().ex_response_parse("range", self, body, _MODULES[__name__])

__all__ = ["GetExHistoryInstrumentBarsRangeCmd"]
