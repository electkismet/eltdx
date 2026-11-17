"""扩展行情二进制解析辅助（带边界检查，失败抛 ProtocolError）。"""

from __future__ import annotations

import struct
from sys import modules as _MODULES
from typing import Any

from eltdx.exceptions import ProtocolError


def require_bytes(data: bytes | bytearray, pos: int, size: int, context: str) -> None:
    """确保从 pos 起至少还能读取 size 字节。"""

    from eltdx._native_abi import load_native
    load_native().extension_require_bytes(data, pos, size, context, _MODULES[__name__])


def unpack_from(fmt: str, data: bytes | bytearray, pos: int, context: str) -> tuple[Any, ...]:
    """带边界检查的 struct.unpack_from。"""

    from eltdx._native_abi import load_native
    load_native().extension_prepare_unpack(fmt, data, pos, context, _MODULES[__name__])
    # Explicit struct/exception adapter: hooks must see Python's active error.
    try:
        return struct.unpack_from(fmt, data, pos)
    except struct.error as e:  # pragma: no cover - require_bytes 已覆盖大部分路径
        raise load_native().extension_unpack_error(context, e, _MODULES[__name__]) from e


def slice_bytes(data: bytes | bytearray, pos: int, size: int, context: str) -> bytes:
    """带边界检查的切片读取。"""

    from eltdx._native_abi import load_native
    return load_native().extension_slice_bytes(data, pos, size, context, _MODULES[__name__])


__all__ = ["require_bytes", "unpack_from", "slice_bytes"]
