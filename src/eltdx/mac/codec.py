"""MAC 协议帧编解码。

MAC 请求帧格式（10 字节头 + body），``struct "<BIBHH"``::

    偏移 0: B (1 字节)  — head_flag（MAC=0x1c，标准=0x0c）
    偏移 1: I (4 字节)  — customize（通常为 0）
    偏移 5: B (1 字节)  — version（通常为 1）
    偏移 6: H (2 字节)  — zipsize（body 长度）
    偏移 8: H (2 字节)  — unzipsize（同 zipsize，MAC 不压缩请求）

MAC 响应复用标准 16 字节帧头（``<IIIHH``），用 :func:`parse_mac_header` /
:func:`decompress_mac_body` 解析。

本模块导出 :func:`build_mac_request`（另一任务
``eltdx/ex/mac_client.py`` 依赖的稳定公开接口）。
"""

from __future__ import annotations

import struct
import zlib
from dataclasses import dataclass

from ..exceptions import ProtocolError
from ..protocol.decompress import (
    MAX_FRAME_UNZIPPED_BYTES,
    DecompressLimitError,
    decompress_limited,
)
from ._binary import unpack_from

_MAC_HEADER_FMT = "<BIBHH"
MAC_HEADER_SIZE = 10
MAC_HEAD_FLAG = 0x1C
_MAC_CUSTOMIZE = 0
_MAC_VERSION = 1

HEADER_SIZE = 16
_RESP_HEADER_FMT = "<IIIHH"


def build_mac_request(
    msg_id: int,
    body: bytes,
    *,
    head_flag: int = MAC_HEAD_FLAG,
) -> bytes:
    """构建 MAC 协议请求帧。

    Parameters
    ----------
    msg_id : int
        MAC 命令 ID（如 ``0x122B``）。
    body : bytes
        命令特有的请求体（不含 ``msg_id`` 前缀）。
    head_flag : int
        帧头标识字节，默认 ``0x1C``（标准 MAC）。部分命令（如 ``0x1218``）
        使用不同的 ``head_flag`` 区分子协议。

    Returns
    -------
    bytes
        完整请求帧（10 字节头 + 2 字节 msg_id + body）。
    """

    inner = struct.pack("<H", msg_id) + body
    header = struct.pack(
        _MAC_HEADER_FMT,
        head_flag,
        _MAC_CUSTOMIZE,
        _MAC_VERSION,
        len(inner),
        len(inner),
    )
    return header + inner


@dataclass(frozen=True, slots=True)
class MacFrameHeader:
    """MAC 响应帧头（标准 16 字节 ``<IIIHH``）。"""

    magic: int
    seq_id: int
    method: int
    zipsize: int
    unzipsize: int


def parse_mac_header(buf: bytes | bytearray) -> MacFrameHeader:
    """解析 16 字节响应帧头。"""

    magic, seq_id, method, zipsize, unzipsize = unpack_from(
        _RESP_HEADER_FMT, buf, 0, "mac frame header"
    )
    return MacFrameHeader(magic, seq_id, method, zipsize, unzipsize)


def decompress_mac_body(header: MacFrameHeader, raw_body: bytes) -> bytes:
    """按需 zlib 解压响应 body，**带输出上限**。

    ``zipsize == unzipsize`` 时直接返回原始字节，否则用有上限的方式解压
    （``decompress_limited``）；长度不符时抛 :class:`ProtocolError`。

    !!! warning "为什么不能直接用 ``zlib.decompress``"
        它会先把全部输出分配出来，之后才比对 ``unzipsize``。一个 8 KB 的异常
        压缩体可以先解出 8 MiB（内存放大约 1000×）才报错，异常响应因此能造成
        过量内存分配。加上限后在产出过程中就截断。
    """

    if len(raw_body) != header.zipsize:
        raise ProtocolError(
            f"mac frame body 长度不符: header={header.zipsize}, actual={len(raw_body)}"
        )
    if header.zipsize == header.unzipsize:
        body = raw_body
    else:
        try:
            body = decompress_limited(raw_body, MAX_FRAME_UNZIPPED_BYTES)
        except DecompressLimitError as exc:
            raise ProtocolError(f"mac frame body 解压超限: {exc}") from exc
        except zlib.error as exc:
            raise ProtocolError(f"mac frame body zlib 解压失败: {exc}") from exc

    if len(body) != header.unzipsize:
        raise ProtocolError(
            f"mac frame body 解压长度不符: header={header.unzipsize}, actual={len(body)}"
        )
    return body


__all__ = [
    "HEADER_SIZE",
    "MAC_HEADER_SIZE",
    "MAC_HEAD_FLAG",
    "MacFrameHeader",
    "build_mac_request",
    "decompress_mac_body",
    "parse_mac_header",
]
