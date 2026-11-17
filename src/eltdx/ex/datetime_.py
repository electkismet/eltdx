"""扩展行情日期时间解码（分钟级/日线两种格式）。"""

from __future__ import annotations

from sys import modules as _MODULES

from eltdx.ex._binary import unpack_from


def get_datetime_minute(data: bytes | bytearray, pos: int) -> tuple[int, int, int, int, int, int]:
    """解析分钟级时间戳（4 字节压缩日期 + 分钟数）。"""

    from eltdx._native_abi import load_native
    return load_native().ex_datetime_decode("minute", None, data, pos, _MODULES[__name__])


def get_datetime_day(data: bytes | bytearray, pos: int) -> tuple[int, int, int, int]:
    """解析日线日期（4 字节 YYYYMMDD 整数）。"""

    from eltdx._native_abi import load_native
    return load_native().ex_datetime_decode("day", None, data, pos, _MODULES[__name__])


def get_datetime(
    category: int, data: bytes | bytearray, pos: int
) -> tuple[int, int, int, int, int, int]:
    """按 K 线周期解析日期时间，返回 (year, month, day, hour, minute, new_pos)。"""

    from eltdx._native_abi import load_native
    return load_native().ex_datetime_decode("auto", category, data, pos, _MODULES[__name__])


__all__ = ["get_datetime", "get_datetime_minute", "get_datetime_day"]
