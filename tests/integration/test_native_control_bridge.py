"""Unexecuted local differential sources for session and HQ company services."""
from __future__ import annotations

import pytest

from eltdx._native_models import response_from_dto
from eltdx.api.company_info import CompanyInfoApi
from eltdx.api.session import SessionApi
from eltdx.client import TdxClient
from eltdx.exceptions import ProtocolError
from eltdx.protocol.commands import parse_command_response
from eltdx.protocol.frame import ResponseFrame
from eltdx.transport import PooledSocketTransport, SocketTransport
from eltdx.transport.native import call_native

from loopback_support import (
    ScriptedServer, answer_handshake, handshake_payload, read_request,
    response_bytes, wait_for_peer_close,
)


def _parse(command, data, payload):
    frame = ResponseFrame(0, 1, command, len(data), len(data), data, b"")
    return parse_command_response(command, frame, payload)


class RecordedControl:
    def __init__(self, data):
        self.data = data

    def execute(self, command, payload):
        return _parse(command, self.data, payload)


def _client(transport_type, host):
    return TdxClient(transport=transport_type([host], timeout=2, heartbeat_interval=None,
        **({"pool_size": 1} if transport_type is PooledSocketTransport else {})))


def _serve(command, data, inspect=None):
    def handler(connection):
        answer_handshake(connection)
        message_id, kind, body = read_request(connection)
        assert kind == command, "unpriced control queries must not load a directory"
        if inspect is not None:
            inspect(body)
        connection.sendall(response_bytes(message_id, kind, data))
        wait_for_peer_close(connection)
    return handler


def _session_data(method):
    if method == "heartbeat":
        # Invalid server date is a retained scalar with a None date projection.
        return b"ABCDEF" + (20260231).to_bytes(4, "little") + b"tail"
    data = bytearray(handshake_payload())
    data[58:68] = bytes(range(10))
    data[152:160] = b"12345678"
    data[68:152] = b"explicit handshake".ljust(84, b"\x00")
    return bytes(data) + b"tail"


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
@pytest.mark.parametrize("method,command", [("handshake", 0x000D), ("heartbeat", 0x0004)])
def test_native_session_matches_original_fields_raw_and_transport_snapshot(transport_type, method, command) -> None:
    data = _session_data(method)
    expected = getattr(SessionApi(RecordedControl(data)), method)()
    with ScriptedServer([_serve(command, data)]) as server:
        client = _client(transport_type, server.host)
        try:
            actual = getattr(client.session, method)()
            assert actual == expected
            assert actual.raw_payload == data
            if transport_type is SocketTransport:
                assert getattr(client.transport, f"last_{method}") == actual
            else:
                index = 0 if method == "handshake" else 1
                dto = call_native(client.transport._engine.session_snapshot)[index]
                assert response_from_dto(dto) == actual
            if method == "heartbeat":
                assert actual.server_date is None and actual.server_date_raw == 20260231
            else:
                assert actual.server_datetime is not None
                assert actual.server_datetime.tzinfo is None
                assert actual.flags_raw == bytes(range(10))
        finally:
            client.close()
        assert server.wait_for_handlers(1)


def _catalog_data():
    records = []
    for name, filename, offset, length in (
        (b" A\x00B ", b" report.txt ", 0xFFFFFFFF, 3),
        (b"\xc9\xee\xff\xca\xd0", b"second.txt", 17, 8),
    ):
        record = bytearray(152)
        record[:len(name)] = name
        record[64:64 + len(filename)] = filename
        record[144:148] = offset.to_bytes(4, "little")
        record[148:152] = length.to_bytes(4, "little")
        records.append(bytes(record))
    return b"\x02\x00" + b"".join(records) + b"unused tail"


def _content_data():
    text = b" \xc9\xee\xff\xca\xd0\x00 \r\n "
    return b"\x00" * 10 + len(text).to_bytes(2, "little") + text + b"unused tail"


def _company(api, method):
    if method == "category":
        # Explicit market wins over a disagreeing prefixed request code.
        return api.category("sh600000", "sz")
    # The old constructor allows overflowing offset+length and its decoder does
    # not reject content longer than the requested length. Preserve both.
    return api.content("sh600000", "sz", " 报告.txt ", 0xFFFFFFFF, 1)


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
@pytest.mark.parametrize("method,command", [("category", 0x02CF), ("content", 0x02D0)])
def test_native_company_matches_original_gbk_byte_ranges_and_explicit_market(transport_type, method, command) -> None:
    data = _catalog_data() if method == "category" else _content_data()
    expected = _company(CompanyInfoApi(RecordedControl(data)), method)

    def inspect(body):
        assert int.from_bytes(body[:2], "little") == 0
        assert body[2:8] == b"600000"
        if method == "content":
            assert body[10:90].rstrip(b"\x00") == "报告.txt".encode("gbk")
            assert int.from_bytes(body[90:94], "little") == 0xFFFFFFFF
            assert int.from_bytes(body[94:98], "little") == 1

    with ScriptedServer([_serve(command, data, inspect)]) as server:
        client = _client(transport_type, server.host)
        try:
            actual = _company(client.company_info, method)
            assert actual == expected
            if method == "content":
                assert actual == "深市"
            else:
                assert actual[0].name == "AB"
                assert actual[0].start == 0xFFFFFFFF and actual[0].length == 3
                assert actual[1].name == "深市"
        finally:
            client.close()
        assert server.wait_for_handlers(1)


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
@pytest.mark.parametrize("domain,method,command", [
    ("session", "handshake", 0x000D), ("session", "heartbeat", 0x0004),
    ("company", "category", 0x02CF), ("company", "content", 0x02D0),
])
def test_native_control_protocol_failures_keep_original_text_and_cause(transport_type, domain, method, command) -> None:
    reference = RecordedControl(b"\x00")
    with pytest.raises(ProtocolError) as expected:
        if domain == "session":
            getattr(SessionApi(reference), method)()
        else:
            _company(CompanyInfoApi(reference), method)
    with ScriptedServer([_serve(command, b"\x00")]) as server:
        client = _client(transport_type, server.host)
        try:
            with pytest.raises(ProtocolError) as actual:
                if domain == "session":
                    getattr(client.session, method)()
                else:
                    _company(client.company_info, method)
            assert type(actual.value) is type(expected.value)
            assert str(actual.value) == str(expected.value)
            assert actual.value.__cause__ is expected.value.__cause__ is None
        finally:
            client.close()
        assert server.wait_for_handlers(1)


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
def test_native_session_preserves_python_close_then_reopen_generation(transport_type) -> None:
    data = _session_data("heartbeat")
    handler = _serve(0x0004, data)
    with ScriptedServer([handler, handler]) as server:
        client = _client(transport_type, server.host)
        try:
            previous_epoch = 0
            for _ in range(2):
                assert client.session.heartbeat().raw_payload == data
                epoch = client.transport.diagnostics.epoch
                assert epoch > previous_epoch
                previous_epoch = epoch
                engine = client.transport._engine
                client.close()
                assert client.transport._engine is engine
                assert client.session.ping() == "pong"
        finally:
            client.close()
        assert server.wait_for_handlers(2)
        assert server.accepted_count == 2
