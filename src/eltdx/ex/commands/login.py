"""MAC EX 扩展行情登录命令（msg_id=0x2454）。"""

from __future__ import annotations

import struct

from eltdx.ex.commands.base import BaseCommand

_MSG_ID = 0x2454
_HEAD_FLAG = 0x01

# 80 字节 Login body（来自 opentdx 参考实现，已通过实际测试验证）。
_LOGIN_BODY = bytes(
    bytearray.fromhex(
        "e5bb1c2fafe52594"
        "1f32c6e5d53dfb41"
        "5b734cc9cdbf0ac9"
        "2021bfdd1eb06d22"
        "d008884c1611cb13"
        "78f6abd824d899d2"
        "1f32c6e5d53dfb41"
        "1f32c6e5d53dfb41"
        "a9325ac935dc0837"
        "335a16e4ce17c1bb"
    )
)

_HEADER_FMT = "<BIBHH"


class MacExLoginCmd(BaseCommand[bool]):
    """MAC EX 扩展行情登录命令。"""

    def build_request(self) -> bytes:
        """构造登录请求包（head_flag=0x01）。"""

        inner = struct.pack("<H", _MSG_ID) + _LOGIN_BODY
        header = struct.pack(_HEADER_FMT, _HEAD_FLAG, 0, 1, len(inner), len(inner))
        return header + inner

    def parse_response(self, body: bytes) -> bool:
        """登录响应 body 非空即视为成功。"""

        return len(body) >= 2


__all__ = ["MacExLoginCmd"]
