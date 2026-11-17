"""Authored raw special-limit and scan compatibility; no execution yet."""
from __future__ import annotations

import math
import struct

import pytest

from eltdx.api.limits import LimitApi
from eltdx.exceptions import ProtocolError
from eltdx.protocol.commands import parse_command_response
from eltdx.protocol.frame import ResponseFrame
from eltdx.transport import PooledSocketTransport, SocketTransport

from loopback_support import ScriptedServer, answer_handshake, read_request, response_bytes, wait_for_peer_close


def _data(value=12.0):
    return struct.pack("<HBIff", 1, 255, 1234567, value, -8.0)


def _reply(connection, index, data):
    message, command, body = read_request(connection)
    assert command == 0x0452 and body == struct.pack("<H", index) + bytes(12)
    connection.sendall(response_bytes(message, command, data))


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
def test_native_special_limits_preserve_unknown_market_code_and_raw(transport_type):
    data = _data()
    expected = parse_command_response(0x0452, ResponseFrame(0, 1, 0x0452, len(data), len(data), data, b""), dict(start_index=4))

    def handler(connection):
        answer_handshake(connection)
        _reply(connection, 4, data)
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        transport = transport_type([server.host], timeout=2, heartbeat_interval=None)
        try:
            assert LimitApi(transport, native_services=True).special(start_index=4) == expected
        finally:
            transport.close()
        assert server.wait_for_handlers(1)


def test_native_special_limits_preserve_nonfinite_legacy_price():
    def handler(connection):
        answer_handshake(connection)
        _reply(connection, 0, _data(float("nan")))
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        transport = SocketTransport([server.host], timeout=2, heartbeat_interval=None)
        try:
            assert math.isnan(LimitApi(transport, native_services=True).special().records[0].limit_up_price)
        finally:
            transport.close()
        assert server.wait_for_handlers(1)


def test_native_special_limits_scan_does_not_pin_or_require_full_target():
    def handler(connection):
        answer_handshake(connection)
        _reply(connection, 3, _data())
        _reply(connection, 4, b"\x00\x00")
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        transport = SocketTransport([server.host], timeout=2, heartbeat_interval=None)
        try:
            result = LimitApi(transport, native_services=True).scan_special(start_index=3, max_rows=10)
            assert len(result) == 1 and result[0].code_num == 1234567
        finally:
            transport.close()
        assert server.wait_for_handlers(1)


def test_native_special_limits_offset_overflow_keeps_next_request_error():
    def handler(connection):
        answer_handshake(connection)
        _reply(connection, 65535, _data())
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        transport = SocketTransport([server.host], timeout=2, heartbeat_interval=None)
        try:
            with pytest.raises((ValueError, OverflowError)):
                LimitApi(transport, native_services=True).scan_special(start_index=65535, max_rows=2)
        finally:
            transport.close()
        assert server.wait_for_handlers(1)
