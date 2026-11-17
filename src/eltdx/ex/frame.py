"""扩展行情响应帧编解码（16 字节帧头 + zlib，native 兼容策略）。"""

from __future__ import annotations

import struct
import zlib
from dataclasses import dataclass
from sys import modules as _MODULES

from eltdx.exceptions import ProtocolError
from eltdx.protocol.decompress import (
    MAX_FRAME_UNZIPPED_BYTES,
    DecompressLimitError,
    decompress_limited,
)

HEADER_SIZE: int = 16
_MAGIC: int = 7654321
_HEADER_FMT: str = "<IIIHH"


@dataclass(frozen=True, slots=True)
class FrameHeader:
    """扩展行情响应帧头（16 字节）。"""

    magic: int
    seq_id: int
    method: int
    zipsize: int
    unzipsize: int


def parse_header(buf: bytes) -> FrameHeader:
    """解析 16 字节响应帧头。"""

    from eltdx._native_abi import load_native
    load_native().ex_frame_header_prepare(buf, _MODULES[__name__])
    # Preserve custom struct hooks and Python's active except context.
    try:
        values = load_native().extension_frame_header_values("ex", buf, _MODULES[__name__])
    except struct.error as e:  # pragma: no cover - 长度已校验
        raise load_native().extension_frame_error("header", e, _MODULES[__name__]) from e
    return load_native().extension_frame_header_model("ex", values, _MODULES[__name__])


def decompress_body(header: FrameHeader, raw_body: bytes) -> bytes:
    """按需 zlib 解压响应 body（**带输出上限**）；长度不符或解压失败抛 ProtocolError。

    输出上限来自帧头 ``unzipsize`` 的 2 字节宽度（≤65535）：超过就一定不是合法
    响应。用 :func:`~eltdx.protocol.decompress.decompress_limited` 在产出过程中
    截断，避免异常压缩体造成过量内存分配。
    """

    from eltdx._native_abi import load_native
    if load_native().extension_body_input("ex", header, raw_body, _MODULES[__name__]):
        try:
            body = decompress_limited(raw_body, MAX_FRAME_UNZIPPED_BYTES)
        except DecompressLimitError as exc:
            raise load_native().extension_frame_error("ex_limit", exc, _MODULES[__name__]) from exc
        except zlib.error as e:
            raise load_native().extension_frame_error("ex_zlib", e, _MODULES[__name__]) from e
    else:
        body = raw_body
    return load_native().extension_body_output("ex", header, body, _MODULES[__name__])


def encode_header(*, seq_id: int, method: int, zipsize: int, unzipsize: int | None = None) -> bytes:
    """构造 16 字节响应帧头（测试/工具用）。"""

    from eltdx._native_abi import load_native
    return load_native().ex_frame_encode(seq_id, method, zipsize, unzipsize, _MODULES[__name__])


__all__ = ["HEADER_SIZE", "FrameHeader", "parse_header", "decompress_body", "encode_header"]
