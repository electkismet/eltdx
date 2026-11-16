"""Authored local quote differential cases; no execution or live provenance."""
from __future__ import annotations

import struct
import _thread
import threading
import time
from types import SimpleNamespace

import pytest

from eltdx.api.quotes import QuoteApi
from eltdx.client import TdxClient
from eltdx.exceptions import ConnectionClosedError, ProtocolError
from eltdx.protocol.commands import parse_command_response
from eltdx.protocol.frame import ResponseFrame
from eltdx.transport import PooledSocketTransport, SocketTransport

from loopback_support import ScriptedServer, answer_handshake, read_request, response_bytes, wait_for_peer_close


CODE = "sh603785"
COMMANDS = {"legacy": 0x053E, "category": 0x054B, "refresh": 0x0547}


def _varint(value):
    magnitude = abs(value)
    first = (magnitude & 0x3F) | (0x40 if value < 0 else 0)
    magnitude >>= 6
    result = bytearray([first | (0x80 if magnitude else 0)])
    while magnitude:
        next_byte = magnitude & 0x7F
        magnitude >>= 7
        result.append(next_byte | (0x80 if magnitude else 0))
    return bytes(result)


def _values(*values):
    return b"".join(_varint(value) for value in values)


def _directory(decimal, code=CODE):
    row = bytearray(37)
    row[:6] = code[2:].encode("ascii")
    row[6:8] = (100).to_bytes(2, "little")
    row[8:12] = b"TEST"
    row[24:28] = struct.pack("<f", 1.0)
    row[28] = decimal
    row[29:33] = struct.pack("<f", 10.0)
    return b"\x01\x00" + row


def _legacy_record(code=CODE, status=0x20):
    row = bytes([1]) + code[2:].encode("ascii") + b"\x01\x00"
    row += _values(12345, -5, 5, 10, -10, 93000, 0, -3, 2)
    row += struct.pack("<I", 0)
    row += _values(1, 2, 0, 3)
    row += _values(*[-1, 1, -2, 4] * 5)
    row += status.to_bytes(2, "little") + _values(0, 0, 0, 0)
    return row + struct.pack("<hH", -12, 7)


def _category_record(code=CODE):
    row = bytes([1]) + code[2:].encode("ascii") + b"\x03\x00"
    row += _values(12345, -5, 5, 10, -10, 93000, -1, -3, 2)
    row += struct.pack("<I", 0) + _values(1, 2, 0, 3, -1, 1, -2, 4)
    tail = bytearray(56)
    tail[:6] = struct.pack("<Hhh", 0x20, -12, 5)
    tail[6:10] = struct.pack("<f", 1.25)
    tail[10:12] = struct.pack("<h", -3)
    tail[12:22] = b"0123456789"
    tail[22:30] = struct.pack("<ff", 2.5, 3.5)
    tail[30:54] = bytes(range(24))
    tail[54:56] = (7).to_bytes(2, "little")
    return row + tail


def _refresh_record(code=CODE):
    row = bytes([1]) + code[2:].encode("ascii") + b"\x04\x00"
    row += _values(12345, -5, 5, 10, -10) + (93000).to_bytes(4, "little")
    row += _values(0, -3, 2) + struct.pack("<I", 0) + _values(1, 2, 0, 3)
    return row + _values(*[-1, 1, -2, 4] * 5) + b"tail"


def _payload(method, codes=(CODE,)):
    if method == "legacy":
        return b"\x05\x00" + len(codes).to_bytes(2, "little") + b"".join(_legacy_record(code) for code in codes)
    if method == "category":
        return b"\x05\x00" + len(codes).to_bytes(2, "little") + b"".join(_category_record(code) for code in codes)
    data = len(codes).to_bytes(2, "little") + b"".join(_refresh_record(code) for code in codes)
    return bytes(byte ^ 0x93 for byte in data)


def _parse(command, data, payload):
    return parse_command_response(command, ResponseFrame(0, 1, command, len(data), len(data), data, b""), payload)


class RecordedQuotes:
    def __init__(self, data):
        self.data = data

    def execute(self, command, payload):
        return _parse(command, self.data, payload)


def _invoke(api, method, codes=(CODE,)):
    if method == "category":
        # The old facade permits zero count and a response larger than it.
        return api.list_by_category("沪深A股", sort_by="涨幅", start=65535, count=0, ascending=True)
    if method == "refresh":
        return api.refresh(list(codes), cursors={codes[0]: 9})
    return api.legacy(list(codes))


def _client(transport_type, host):
    return TdxClient(transport=transport_type([host], timeout=2, heartbeat_interval=None,
        **({"pool_size": 1} if transport_type is PooledSocketTransport else {})))


def _answer(connection, command, data):
    message_id, kind, body = read_request(connection)
    assert kind == command
    connection.sendall(response_bytes(message_id, kind, data))
    return body


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
@pytest.mark.parametrize("method", ["legacy", "category", "refresh"])
@pytest.mark.parametrize("decimal", [2, 3, 9])
def test_native_quotes_match_original_scalars_negative_values_order_and_raw(transport_type, method, decimal) -> None:
    directory = _directory(decimal)
    _parse(0x044D, directory, {"market": "sh"})
    codes = (CODE, CODE)
    data = _payload(method, codes)
    expected = _invoke(QuoteApi(RecordedQuotes(data)), method, codes)
    observed = []

    def handler(connection):
        answer_handshake(connection)
        markets = ("bj", "sh", "sz") if method == "category" else ("sh",)
        for market in markets:
            page = directory if market == "sh" else b"\x00\x00"
            body = _answer(connection, 0x044D, page)
            assert int.from_bytes(body[:2], "little") == {"sz": 0, "sh": 1, "bj": 2}[market]
            if market == "sh":
                body = _answer(connection, 0x044D, b"\x00\x00")
                assert int.from_bytes(body[2:6], "little") == 1
        observed.append(_answer(connection, COMMANDS[method], data))
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        client = _client(transport_type, server.host)
        try:
            actual = _invoke(client.quotes, method, codes)
            assert actual == expected
            records = actual if method == "legacy" else actual.records
            assert len(records) == 2 and records[0].record_hex
            assert records[0].total_hand == -3
            if method == "category":
                assert actual.request_count == 0 and actual.start == 65535
                assert actual.raw_payload == data
            if method == "refresh":
                assert actual.raw_payload == data
                assert actual.decoded_payload == bytes(byte ^ 0x93 for byte in data)
        finally:
            client.close()
        assert server.wait_for_handlers(1)


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
@pytest.mark.parametrize("method", ["legacy", "category", "refresh"])
def test_native_quote_api_without_resolver_does_not_prefetch_metadata(transport_type, method) -> None:
    _parse(0x044D, _directory(3), {"market": "sh"})
    data = _payload(method)
    expected = _invoke(QuoteApi(RecordedQuotes(data)), method)

    def handler(connection):
        answer_handshake(connection)
        _answer(connection, COMMANDS[method], data)
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        transport = transport_type([server.host], timeout=2, heartbeat_interval=None,
            **({"pool_size": 1} if transport_type is PooledSocketTransport else {}))
        try:
            assert _invoke(QuoteApi(transport, native_services=True), method) == expected
        finally:
            transport.close()
        assert server.wait_for_handlers(1)


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
@pytest.mark.parametrize("method", ["legacy", "category", "refresh"])
def test_native_quote_protocol_errors_keep_original_type_text_and_cause(transport_type, method) -> None:
    with pytest.raises(ProtocolError) as reference:
        _invoke(QuoteApi(RecordedQuotes(b"\x00")), method)

    def handler(connection):
        answer_handshake(connection)
        _answer(connection, COMMANDS[method], b"\x00")
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        transport = transport_type([server.host], timeout=2, heartbeat_interval=None,
            **({"pool_size": 1} if transport_type is PooledSocketTransport else {}))
        try:
            with pytest.raises(ProtocolError) as actual:
                _invoke(QuoteApi(transport, native_services=True), method)
            assert type(actual.value) is type(reference.value)
            assert str(actual.value) == str(reference.value)
            assert actual.value.__cause__ is reference.value.__cause__ is None
        finally:
            transport.close()
        assert server.wait_for_handlers(1)


def test_native_pinned_refresh_keeps_cursors_raw_and_closed_proxy_guard() -> None:
    _parse(0x044D, _directory(3), {"market": "sh"})
    data = _payload("refresh")
    expected = QuoteApi(RecordedQuotes(data)).refresh(CODE, cursors={CODE: 42})
    observed = []

    def handler(connection):
        answer_handshake(connection)
        observed.append(_answer(connection, 0x0547, data))
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        pool = PooledSocketTransport([server.host], pool_size=1, timeout=2, heartbeat_interval=None)
        try:
            with pool.pin() as pinned:
                api = QuoteApi(pinned, native_services=True)
                assert api.refresh(CODE, cursors={CODE: 42}) == expected
                assert int.from_bytes(observed[0][-4:], "little") == 42
            with pytest.raises(ConnectionClosedError, match="no longer valid"):
                api.refresh(CODE)
        finally:
            pool.close()
        assert server.wait_for_handlers(1)


def test_native_subscription_preloads_before_pin_and_uses_existing_refresh_path() -> None:
    data = _payload("refresh")

    def handler(connection):
        answer_handshake(connection)
        _answer(connection, 0x044D, _directory(3))
        _answer(connection, 0x044D, b"\x00\x00")
        _answer(connection, 0x0547, data)
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        client = _client(PooledSocketTransport, server.host)
        subscription = client.quotes.subscribe(CODE, lambda _row: None)
        try:
            page = subscription.activate()
            assert page.records[0].full_code == CODE
            assert page.records[0].record_hex
            assert subscription._group_apis[0]._native_services
        finally:
            subscription.close()
            client.close()
        assert server.wait_for_handlers(1)


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
def test_native_suspended_does_not_add_metadata_request_or_identity_reordering(transport_type) -> None:
    other = "sh603786"
    # An override supplies one requested code, while the old wire parser returns
    # an unrelated suspended identity. Preserve that observable legacy result.
    data = b"\x05\x00\x02\x00" + _legacy_record(other) + _legacy_record(CODE, status=0)

    def handler(connection):
        answer_handshake(connection)
        _answer(connection, 0x053E, data)
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        client = _client(transport_type, server.host)
        client.codes.latest_stock_list = lambda market=None: [SimpleNamespace(full_code=CODE)]
        try:
            assert client.codes.latest_suspended() == [other]
        finally:
            client.close()
        assert server.wait_for_handlers(1)


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
def test_native_empty_refresh_keeps_wire_request_and_complete_raw_views(transport_type) -> None:
    observed = []

    def handler(connection):
        answer_handshake(connection)
        observed.append(_answer(connection, 0x0547, b"\x93\x93"))
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        transport = transport_type([server.host], timeout=2, heartbeat_interval=None,
            **({"pool_size": 1} if transport_type is PooledSocketTransport else {}))
        try:
            page = QuoteApi(transport, native_services=True).refresh()
            assert observed == [b"\x00\x00"]
            assert page.records == () and page.requested_codes == ()
            assert page.decoded_payload == b"\x00\x00" and page.raw_payload == b"\x93\x93"
        finally:
            transport.close()
        assert server.wait_for_handlers(1)


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
def test_native_refresh_preserves_cursor_only_keys_and_wire_order(transport_type) -> None:
    other = "sh603786"
    _parse(0x044D, _directory(3), {"market": "sh"})
    data = _payload("refresh", (CODE, other))
    expected = QuoteApi(RecordedQuotes(data)).refresh([CODE], cursors={CODE: 9, other: 7})
    observed = []

    def handler(connection):
        answer_handshake(connection)
        observed.append(_answer(connection, 0x0547, data))
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        transport = transport_type([server.host], timeout=2, heartbeat_interval=None,
            **({"pool_size": 1} if transport_type is PooledSocketTransport else {}))
        try:
            actual = QuoteApi(transport, native_services=True).refresh([CODE], cursors={CODE: 9, other: 7})
            assert actual == expected and actual.requested_codes == (CODE, other)
            assert int.from_bytes(observed[0][:2], "little") == 2
            assert int.from_bytes(observed[0][9:13], "little") == 9
            assert int.from_bytes(observed[0][20:24], "little") == 7
        finally:
            transport.close()
        assert server.wait_for_handlers(1)


def test_native_pinned_refresh_interrupt_confirms_request_cleanup() -> None:
    requested = threading.Event()

    def handler(connection):
        answer_handshake(connection)
        _, kind, _ = read_request(connection)
        assert kind == 0x0547
        requested.set()
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        pool = PooledSocketTransport([server.host], pool_size=1, timeout=2, heartbeat_interval=None)

        def interrupt():
            assert requested.wait(timeout=3)
            time.sleep(0.05)
            _thread.interrupt_main()

        interrupter = threading.Thread(target=interrupt, daemon=True)
        try:
            with pool.pin() as pinned:
                interrupter.start()
                with pytest.raises(KeyboardInterrupt) as caught:
                    QuoteApi(pinned, native_services=True).refresh(CODE)
                assert caught.value.__cause__ is None
        finally:
            interrupter.join(timeout=1)
            pool.close()
        assert not interrupter.is_alive()
        assert server.wait_for_handlers(1)
