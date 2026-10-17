"""获取扩展行情历史K线（按日期范围）。"""

from __future__ import annotations

import struct

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
        if not code:
            raise ValueError("code 不能为空")
        self.market = market
        self.code = code.encode("utf-8")
        self.start_date = start_date
        self.end_date = end_date

    def _next_seqid(self) -> int:
        """取下一个请求序号，到 ``_SEQID_MAX`` 后回绕到 ``_SEQID_MIN``。

        !!! warning "必须回绕"
            序号是**类级**变量且单调递增，早期实现直接 ``struct.pack("<B", self._seqid)``。
            第 257 次构造请求时值变成 256，``<B`` 编码抛
            ``struct.error: 'B' format requires 0 <= number <= 255``。
            由于状态在类上，**新建客户端也恢复不了** —— 一次长会话里的
            第 257 次调用必然失败。回绕后可以无限次调用。
        """

        seqid = type(self)._seqid
        type(self)._seqid = seqid + 1 if seqid < self._SEQID_MAX else self._SEQID_MIN
        return seqid

    def build_request(self) -> bytes:
        """构造请求包（序号自增并按单字节范围回绕）。"""

        pkg = bytearray.fromhex("01")
        pkg.extend(struct.pack("<B", self._next_seqid()))
        pkg.extend(bytearray.fromhex("38 92 00 01 16 00 16 00 0D 24"))
        pkg.extend(struct.pack("<B9s", self.market, self.code))
        pkg.extend(bytearray.fromhex("07 00"))
        pkg.extend(struct.pack("<II", self.start_date, self.end_date))
        return bytes(pkg)

    @staticmethod
    def _parse_date(num: int) -> tuple[int, int, int]:
        year = num // 2048 + 2004
        month = (num % 2048) // 100
        day = (num % 2048) % 100
        return year, month, day

    @staticmethod
    def _parse_time(num: int) -> tuple[int, int]:
        return num // 60, num % 60

    def parse_response(self, body: bytes) -> list[ExInstrumentBar]:
        """解析历史K线响应。"""

        pos = 12  # skip 12-byte header
        if pos + 2 > len(body):
            return []
        (ret_count,) = struct.unpack("<H", body[pos : pos + 2])
        pos += 2
        results: list[ExInstrumentBar] = []
        for _ in range(ret_count):
            record_start = pos
            if pos + 32 > len(body):
                break
            (d1, d2, open_p, high, low, close_p, position, trade, settlement) = struct.unpack(
                "<HHffffIIf",
                body[pos : pos + 32],
            )
            pos += 32
            year, month, day = self._parse_date(d1)
            hour, minute = self._parse_time(d2)
            results.append(
                ExInstrumentBar(
                    open=open_p,
                    high=high,
                    low=low,
                    close=close_p,
                    position=position,
                    trade=trade,
                    amount=settlement,
                    year=year,
                    month=month,
                    day=day,
                    hour=hour,
                    minute=minute,
                    _raw=body[record_start:pos],
                )
            )
        return results


__all__ = ["GetExHistoryInstrumentBarsRangeCmd"]
