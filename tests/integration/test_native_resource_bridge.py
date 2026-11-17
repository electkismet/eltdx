"""Authored local wire cases for shared resource requests and pinned paging."""
from __future__ import annotations

import struct

import pytest

from eltdx.api.resources import ResourceApi
from eltdx.exceptions import ProtocolError
from eltdx.protocol.commands import parse_command_response
from eltdx.protocol.frame import ResponseFrame
from eltdx.transport import PooledSocketTransport, SocketTransport

from loopback_support import ScriptedServer, answer_handshake, read_request, response_bytes, wait_for_peer_close


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
@pytest.mark.parametrize("trailing", [b"", b"unknown-trailer"])
def test_native_block_metadata_keeps_size_hash_and_permitted_trailer(transport_type, trailing):
    digest = b"0123456789abcdef0123456789abcdef"
    data = struct.pack("<I", 12345) + b"\x00" + digest + b"\x00" + trailing
    expected = parse_command_response(
        0x02C5, ResponseFrame(0, 1, 0x02C5, len(data), len(data), data, b""),
        dict(filename="block_zs.dat"),
    )

    def handler(connection):
        answer_handshake(connection)
        message_id, command, body = read_request(connection)
        assert command == 0x02C5
        assert body == b"block_zs.dat".ljust(40, b"\x00")
        connection.sendall(response_bytes(message_id, command, data))
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        transport = transport_type([server.host], timeout=2, heartbeat_interval=None)
        try:
            actual = ResourceApi(transport, native_services=True).block_meta("block_zs.dat")
            assert actual == expected
            assert actual.size == 12345 and actual.md5 == digest.decode("ascii")
        finally:
            transport.close()
        assert server.wait_for_handlers(1)


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
def test_native_block_metadata_short_payload_preserves_protocol_error(transport_type):
    data = bytes(37)
    with pytest.raises(ProtocolError) as reference:
        parse_command_response(
            0x02C5, ResponseFrame(0, 1, 0x02C5, len(data), len(data), data, b""),
            dict(filename="block_zs.dat"),
        )

    def handler(connection):
        answer_handshake(connection)
        message_id, command, body = read_request(connection)
        assert command == 0x02C5 and body == b"block_zs.dat".ljust(40, b"\x00")
        connection.sendall(response_bytes(message_id, command, data))
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        transport = transport_type([server.host], timeout=2, heartbeat_interval=None)
        try:
            with pytest.raises(ProtocolError) as actual:
                ResourceApi(transport, native_services=True).block_meta("block_zs.dat")
            assert type(actual.value) is type(reference.value)
            assert str(actual.value) == str(reference.value)
            assert type(actual.value.__cause__) is type(reference.value.__cause__)
        finally:
            transport.close()
        assert server.wait_for_handlers(1)


def _answer(connection, data, *, offset=0, size=4):
    message_id, command, body = read_request(connection)
    assert command == 0x06B9 and len(body) == 308
    assert struct.unpack("<II", body[:8]) == (offset, size)
    assert body[8:].split(b"\x00", 1)[0] == b"sample.bin"
    connection.sendall(response_bytes(message_id, command, data))


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
def test_native_resource_single_read_keeps_raw_and_trailing_bytes(transport_type):
    data = struct.pack("<I", 3) + b"abc" + b"trailing"
    expected = parse_command_response(0x06B9, ResponseFrame(0, 1, 0x06B9, len(data), len(data), data, b""),
                                      dict(path="sample.bin", offset=0, size=4))

    def handler(connection):
        answer_handshake(connection)
        _answer(connection, data)
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        transport = transport_type([server.host], timeout=2, heartbeat_interval=None)
        try:
            assert ResourceApi(transport, native_services=True).read("sample.bin", size=4) == expected
        finally:
            transport.close()
        assert server.wait_for_handlers(1)


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
@pytest.mark.parametrize("cap", [None, 6, 8])
def test_native_resource_download_uses_one_connection_and_original_eof_policy(transport_type, cap):
    source = b"abcdefgh"
    wanted = [(0, 4), (4, 2)] if cap == 6 else [(0, 4), (4, 4)] + ([(8, 4)] if cap is None else [])

    def handler(connection):
        answer_handshake(connection)
        for offset, size in wanted:
            content = source[offset:offset + size]
            _answer(connection, struct.pack("<I", len(content)) + content, offset=offset, size=size)
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        transport = transport_type([server.host], timeout=2, heartbeat_interval=None,
                                   **({"pool_size": 1} if transport_type is PooledSocketTransport else {}))
        try:
            assert ResourceApi(transport, native_services=True).download_file("sample.bin", chunk_size=4, max_bytes=cap) == source[:cap]
        finally:
            transport.close()
        assert server.wait_for_handlers(1) and server.accepted_count == 1
