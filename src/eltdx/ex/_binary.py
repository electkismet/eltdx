"""扩展行情二进制解析辅助（带边界检查，失败抛 ProtocolError）。"""

from __future__ import annotations

import struct
from typing import Any

from eltdx.exceptions import ProtocolError


def require_bytes(data: bytes | bytearray, pos: int, size: int, context: str) -> None:
    """确保从 pos 起至少还能读取 size 字节。"""

    if pos < 0:
        raise ProtocolError(f"{context}: 非法偏移 {pos}")
    if pos + size > len(data):
        remaining = max(len(data) - pos, 0)
        raise ProtocolError(f"{context}: 数据不足，需要 {size} 字节，偏移 {pos}，实际剩余 {remaining} 字节")


def unpack_from(fmt: str, data: bytes | bytearray, pos: int, context: str) -> tuple[Any, ...]:
    """带边界检查的 struct.unpack_from。"""

    require_bytes(data, pos, struct.calcsize(fmt), context)
    try:
        return struct.unpack_from(fmt, data, pos)
    except struct.error as e:  # pragma: no cover - require_bytes 已覆盖大部分路径
        raise ProtocolError(f"{context}: 解析失败: {e}") from e


def slice_bytes(data: bytes | bytearray, pos: int, size: int, context: str) -> bytes:
    """带边界检查的切片读取。"""

    require_bytes(data, pos, size, context)
    return bytes(data[pos : pos + size])


__all__ = ["require_bytes", "unpack_from", "slice_bytes"]
