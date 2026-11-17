"""Authored directory/filter compatibility cases, deferred until source completion."""
from __future__ import annotations

import struct
from datetime import date

import pytest

from eltdx.api.codes import CodeApi
from eltdx.client import TdxClient
from eltdx.exceptions import ProtocolError
from eltdx.models.security import SecurityCode
from eltdx.protocol.commands import parse_command_response
from eltdx.protocol.frame import ResponseFrame
from eltdx.transport import PooledSocketTransport, SocketTransport

from loopback_support import ScriptedServer, answer_handshake, read_request, response_bytes, wait_for_peer_close


ROWS = [("600777", "TEST", 2), ("900777", "B TEST", 2),
        ("510777", "ETF TEST", 3), ("000777", "INDEX TEST", 2),
        ("600778", "*ST TEST", 2)]


def _directory(rows):
    data = bytearray(len(rows).to_bytes(2, "little"))
    for number, name, decimal in rows:
        record = bytearray(37)
        record[:6] = number.encode("ascii")
        record[6:8] = (100).to_bytes(2, "little")
        encoded = name.encode("gbk")
        record[8:8 + len(encoded)] = encoded
        record[24:28] = struct.pack("<f", 1.25)
        record[28] = decimal
        record[29:33] = struct.pack("<f", 10.5)
        record[33:37] = b"ABCD"
        data.extend(record)
    return bytes(data)


def _parse(command, data, payload):
    return parse_command_response(command, ResponseFrame(0, 1, command, len(data), len(data), data, b""), payload)


class RecordedDirectory:
    def __init__(self, pages):
        self.pages = pages

    def execute(self, command, payload):
        data = self.pages[payload.get("start", 0)]
        return _parse(command, data, payload)


def _client(transport_type, host):
    return TdxClient(transport=transport_type([host], timeout=2, heartbeat_interval=None,
        **({"pool_size": 1} if transport_type is PooledSocketTransport else {})))


def _answer_directory(connection, *, offset, limit, data):
    message_id, kind, body = read_request(connection)
    assert kind == 0x044D
    assert int.from_bytes(body[:2], "little") == 1
    assert int.from_bytes(body[2:6], "little") == offset
    assert int.from_bytes(body[6:10], "little") == limit
    connection.sendall(response_bytes(message_id, kind, data))


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
def test_native_code_count_keeps_original_binding_date_and_single_request(transport_type) -> None:
    observed = []

    def handler(connection):
        answer_handshake(connection)
        message_id, kind, body = read_request(connection)
        assert kind == 0x044E and int.from_bytes(body[:2], "little") == 1
        observed.append(int.from_bytes(body[2:6], "little"))
        connection.sendall(response_bytes(message_id, kind, (123).to_bytes(2, "little")))
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        client = _client(transport_type, server.host)
        try:
            before = int(date.today().strftime("%Y%m%d"))
            assert client.codes.count("sh") == 123
            after = int(date.today().strftime("%Y%m%d"))
            assert observed[0] in (before, after)
        finally:
            client.close()
        assert server.wait_for_handlers(1)


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
@pytest.mark.parametrize("limit", [0, 1])
def test_native_code_list_keeps_zero_limit_duplicate_rows_and_raw_fields(transport_type, limit) -> None:
    data = _directory([ROWS[0], ROWS[0]]) + b"original ignored trailing bytes"
    expected = CodeApi(RecordedDirectory({7: data})).list("sh", start=7, limit=limit)

    def handler(connection):
        answer_handshake(connection)
        _answer_directory(connection, offset=7, limit=limit, data=data)
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        client = _client(transport_type, server.host)
        try:
            actual = client.codes.list("sh", start=7, limit=limit)
            assert actual == expected and len(actual) == 2
            assert actual[0].unknown3_raw == b"ABCD"
            assert client._decimal_cache[actual[0].full_code] == 2
        finally:
            client.close()
        assert server.wait_for_handlers(1)


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
@pytest.mark.parametrize("method", ["stocks", "a_shares", "etfs", "indices", "stock_count", "a_share_count", "latest_st"])
def test_native_directory_selection_matches_existing_rows_order_and_counts(transport_type, method) -> None:
    data = _directory(ROWS)
    rows = CodeApi(RecordedDirectory({0: data})).list("sh")
    reference = CodeApi(RecordedDirectory({0: data, len(ROWS): b"\x00\x00"}))
    expected = getattr(reference, method)("sh")

    def handler(connection):
        answer_handshake(connection)
        _answer_directory(connection, offset=0, limit=1600, data=data)
        _answer_directory(connection, offset=len(ROWS), limit=1600, data=b"\x00\x00")
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        client = _client(transport_type, server.host)
        try:
            actual = getattr(client.codes, method)("sh")
            assert actual == expected
            if method == "stocks":
                assert [row.full_code for row in actual] == [rows[index].full_code for index in (0, 1, 4)]
        finally:
            client.close()
        assert server.wait_for_handlers(1)


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
@pytest.mark.parametrize("collect", [False, True])
def test_native_directory_retains_old_precision_registry_for_unmigrated_commands(transport_type, collect) -> None:
    number = "603889"
    old = _directory([(number, "TEST", 2)])
    current = _directory([(number, "TEST", 3)])
    _parse(0x044D, old, {"market": "sh"})
    minute = b"\x01\x00\x00\x00\x0a\x0c\x03"

    def handler(connection):
        answer_handshake(connection)
        _answer_directory(connection, offset=0, limit=1600, data=current)
        if collect:
            _answer_directory(connection, offset=1, limit=1600, data=b"\x00\x00")
        message_id, kind, _ = read_request(connection)
        assert kind == 0x0537
        connection.sendall(response_bytes(message_id, kind, minute))
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        client = _client(transport_type, server.host)
        try:
            getattr(client.codes, "all" if collect else "list")("sh")
            actual = client.transport.execute(0x0537, {"code": "sh" + number})
            assert actual.points[0].price == 0.01
        finally:
            client.close()
        assert server.wait_for_handlers(1)


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
@pytest.mark.parametrize("command", [0x044D, 0x044E])
def test_native_directory_protocol_errors_keep_original_text_and_cause(transport_type, command) -> None:
    with pytest.raises(ProtocolError) as reference:
        _parse(command, b"\x00", {"market": "sh"})

    def handler(connection):
        answer_handshake(connection)
        message_id, kind, _ = read_request(connection)
        assert kind == command
        connection.sendall(response_bytes(message_id, kind, b"\x00"))
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        client = _client(transport_type, server.host)
        try:
            with pytest.raises(ProtocolError) as actual:
                getattr(client.codes, "list" if command == 0x044D else "count")("sh")
            assert type(actual.value) is type(reference.value)
            assert str(actual.value) == str(reference.value)
            assert actual.value.__cause__ is reference.value.__cause__ is None
        finally:
            client.close()
        assert server.wait_for_handlers(1)


def test_native_st_selection_handles_python_unicode_rules_without_starting_engine() -> None:
    client = TdxClient(transport=SocketTransport(["127.0.0.1:7709"], heartbeat_interval=None))
    rows = [SecurityCode("sh", 1, "600001", "\x1cſt股份", 100, 2, 1.0, 1.0,
                         b"0000", b"0000", b"0000", "a_share", "source", "none", "source")]
    client.codes.latest_stock_list = lambda market=None: rows
    assert client.transport._engine is None
    assert client.codes.latest_st() == rows
    assert client.transport._engine is None


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
def test_native_directory_retains_prior_row_precision_when_later_row_fails(transport_type) -> None:
    number = "603887"
    _parse(0x044D, _directory([(number, "TEST", 2)]), {"market": "sh"})
    data = bytearray(_directory([(number, "TEST", 3), ROWS[0]]))
    data[2 + 37] = 0xFF

    def handler(connection):
        answer_handshake(connection)
        _answer_directory(connection, offset=0, limit=1600, data=data)
        message_id, kind, _ = read_request(connection)
        assert kind == 0x0537
        connection.sendall(response_bytes(message_id, kind, b"\x01\x00\x00\x00\x0a\x0c\x03"))
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        client = _client(transport_type, server.host)
        try:
            with pytest.raises(ProtocolError, match="invalid security code"):
                client.codes.list("sh")
            assert client.transport.execute(0x0537, {"code": "sh" + number}).points[0].price == 0.01
        finally:
            client.close()
        assert server.wait_for_handlers(1)
