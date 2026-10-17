"""扩展行情日期时间解码（分钟级/日线两种格式）。"""

from __future__ import annotations

from eltdx.ex._binary import unpack_from


def get_datetime_minute(data: bytes | bytearray, pos: int) -> tuple[int, int, int, int, int, int]:
    """解析分钟级时间戳（4 字节压缩日期 + 分钟数）。"""

    zipday, tminutes = unpack_from("<HH", data, pos, "minute datetime")
    year = (zipday >> 11) + 2004
    month = (zipday % 2048) // 100
    day = (zipday % 2048) % 100
    hour = tminutes // 60
    minute = tminutes % 60
    return year, month, day, hour, minute, pos + 4


def get_datetime_day(data: bytes | bytearray, pos: int) -> tuple[int, int, int, int]:
    """解析日线日期（4 字节 YYYYMMDD 整数）。"""

    (zipday,) = unpack_from("<I", data, pos, "day datetime")
    year = zipday // 10000
    month = (zipday % 10000) // 100
    day = zipday % 100
    return year, month, day, pos + 4


def get_datetime(
    category: int, data: bytes | bytearray, pos: int
) -> tuple[int, int, int, int, int, int]:
    """按 K 线周期解析日期时间，返回 (year, month, day, hour, minute, new_pos)。"""

    if category < 4 or category in (7, 8):
        return get_datetime_minute(data, pos)
    year, month, day, new_pos = get_datetime_day(data, pos)
    return year, month, day, 15, 0, new_pos


__all__ = ["get_datetime", "get_datetime_minute", "get_datetime_day"]
