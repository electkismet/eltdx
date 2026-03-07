from __future__ import annotations

import socket
import struct
import zlib
from dataclasses import dataclass

from ..exceptions import ConnectionClosedError, ProtocolError
from .constants import CONTROL_DEFAULT, PREFIX, PREFIX_RESP


@dataclass(slots=True)
class RequestFrame:
    msg_id: int
    msg_type: int
    data: bytes = b""
    control: int = CONTROL_DEFAULT

    def to_bytes(self) -> bytes:
        length = len(self.data) + 2
        return struct.pack("<BIBHHH", PREFIX, self.msg_id, self.control, length, length, self.msg_type) + self.data


@dataclass(slots=True)
class ResponseFrame:
    control: int
    msg_id: int
    msg_type: int
    zip_length: int
    length: int
    data: bytes
    raw: bytes


def read_exact(sock: socket.socket, size: int) -> bytes:
    chunks = bytearray()
    while len(chunks) < size:
        piece = sock.recv(size - len(chunks))
        if not piece:
            raise ConnectionClosedError("socket closed by remote peer")
        chunks.extend(piece)
    return bytes(chunks)


def read_response_frame(sock: socket.socket) -> bytes:
    window = bytearray(read_exact(sock, 4))
    while bytes(window) != PREFIX_RESP:
        window = window[1:] + read_exact(sock, 1)

    header = read_exact(sock, 12)
    zip_length = int.from_bytes(header[8:10], "little", signed=False)
    payload = read_exact(sock, zip_length)
    return bytes(window) + header + payload


def decode_response(raw: bytes) -> ResponseFrame:
    if len(raw) < 16:
        raise ProtocolError(f"invalid response length: {len(raw)}")

    control = raw[4]
    msg_id = int.from_bytes(raw[5:9], "little", signed=False)
    msg_type = int.from_bytes(raw[10:12], "little", signed=False)
    zip_length = int.from_bytes(raw[12:14], "little", signed=False)
    length = int.from_bytes(raw[14:16], "little", signed=False)
    data = raw[16:]

    if len(data) != zip_length:
        raise ProtocolError(f"zip length mismatch: expected {zip_length}, got {len(data)}")

    if zip_length != length:
        data = zlib.decompress(data)

    if len(data) != length:
        raise ProtocolError(f"decoded length mismatch: expected {length}, got {len(data)}")

    return ResponseFrame(
        control=control,
        msg_id=msg_id,
        msg_type=msg_type,
        zip_length=zip_length,
        length=length,
        data=data,
        raw=raw,
    )

