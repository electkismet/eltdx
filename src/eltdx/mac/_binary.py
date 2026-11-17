"""MAC 协议二进制解析辅助。

提供带边界检查的 ``unpack_from`` / ``slice_bytes``，异常统一使用
:class:`eltdx.exceptions.ProtocolError`。
"""

from __future__ import annotations

import struct
from sys import modules as _MODULES
from typing import Any

from ..exceptions import ProtocolError


def require_bytes(
    data: bytes | bytearray,
    pos: int,
    size: int,
    context: str,
) -> None:
    """确保从 ``pos`` 起至少还能读取 ``size`` 字节，否则抛出 :class:`ProtocolError`。"""

    from eltdx._native_abi import load_native
    load_native().extension_require_bytes(data, pos, size, context, _MODULES[__name__])


def unpack_from(
    fmt: str,
    data: bytes | bytearray,
    pos: int,
    context: str,
) -> tuple[Any, ...]:
    """带边界检查的 ``struct.unpack_from``。"""

    from eltdx._native_abi import load_native
    load_native().extension_prepare_unpack(fmt, data, pos, context, _MODULES[__name__])
    # Keep the struct buffer hook and active-exception semantics in Python.
    try:
        return struct.unpack_from(fmt, data, pos)
    except struct.error as exc:  # pragma: no cover - require_bytes 已覆盖大部分路径
        raise load_native().extension_unpack_error(context, exc, _MODULES[__name__]) from exc


def slice_bytes(
    data: bytes | bytearray,
    pos: int,
    size: int,
    context: str,
) -> bytes:
    """带边界检查的切片读取。"""

    from eltdx._native_abi import load_native
    return load_native().extension_slice_bytes(data, pos, size, context, _MODULES[__name__])
