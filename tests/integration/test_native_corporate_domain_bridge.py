"""Authored wire/source compatibility obligations; no test has been run."""
from __future__ import annotations

import _thread
import math
import struct
import threading
import time

import pytest

from eltdx.api.corporate import CorporateApi
from eltdx.exceptions import MissingCodesWarning, ProtocolError
from eltdx.models import CapitalChangeBatch, FinanceBatch
from eltdx.protocol.commands import parse_command_response
from eltdx.protocol.frame import ResponseFrame
from eltdx.transport import PooledSocketTransport, SocketTransport

from loopback_support import ScriptedServer, answer_handshake, read_request, response_bytes, wait_for_peer_close


CODE = "sh" + str(600_001)
SECOND = "sh" + str(600_002)


def _capital(code="600001", *, market=1, category=1, declared=7):
    identity = bytes([market]) + code.encode("ascii")
    event = identity + b"\x03" + struct.pack("<IBffff", 20261301, category, 1.0, 2.0, 3.0, 4.0)
    return struct.pack("<H", declared) + identity + struct.pack("<H", 1) + event


def _finance(codes=("600001",), value=-5.0):
    records = []
    for code in codes:
        info = bytearray(136)
        struct.pack_into("<fHHII", info, 0, value, 4, 7, 20261301, 20200229)
        struct.pack_into("<f", info, 16, 9.0)
        records.append(b"\x01" + code.encode("ascii") + info)
    return struct.pack("<H", len(records)) + b"".join(records)


def _parse(command, data, payload):
    return parse_command_response(command, ResponseFrame(0, 1, command, len(data), len(data), data, b""), payload)


def _answer(connection, command, codes, data):
    message_id, actual, body = read_request(connection)
    assert actual == command, "corporate requests must not prefetch security metadata"
    expected = struct.pack("<H", len(codes)) + b"".join(b"\x01" + code[2:].encode("ascii") for code in codes)
    assert body == expected
    connection.sendall(response_bytes(message_id, command, data))


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
@pytest.mark.parametrize("raw", [False, True])
@pytest.mark.parametrize("single", [False, True])
def test_native_corporate_capital_matches_original_dates_values_and_raw(transport_type, raw, single):
    data = _capital()
    payload = dict(code=CODE, include_raw=raw) if single else dict(codes=[CODE], include_raw=raw)
    expected = _parse(0x000F, data, payload)
    if not single:
        from eltdx.models import CapitalChangeBlock
        if isinstance(expected, CapitalChangeBlock):
            expected = CapitalChangeBatch((expected,), (expected.raw_payload,) if expected.raw_payload else ())

    def handler(connection):
        answer_handshake(connection)
        _answer(connection, 0x000F, [CODE], data)
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        transport = transport_type([server.host], timeout=2, heartbeat_interval=None)
        try:
            result = CorporateApi(transport, native_services=True).capital_changes(CODE if single else [CODE], include_raw=raw)
            assert result == expected
            block = result if single else result.blocks[0]
            assert block.records[0].date is None and block.records[0].c4_value == 4.0
        finally:
            transport.close()
        assert server.wait_for_handlers(1)


@pytest.mark.parametrize("raw", [False, True])
def test_native_corporate_capital_prefix_collects_original_pages(raw):
    first, second = _capital(), _capital("600002", category=5)
    pages = [_parse(0x000F, data, dict(codes=codes, include_raw=raw))
             for data, codes in [(first, [CODE, SECOND]), (second, [SECOND])]]

    def handler(connection):
        answer_handshake(connection)
        _answer(connection, 0x000F, [CODE, SECOND], first)
        _answer(connection, 0x000F, [SECOND], second)
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        transport = SocketTransport([server.host], timeout=2, heartbeat_interval=None)
        try:
            result = CorporateApi(transport, native_services=True).capital_changes([CODE, SECOND], include_raw=raw)
            expected_blocks = tuple(block for page in pages for block in
                                    (page.blocks if isinstance(page, CapitalChangeBatch) else (page,)))
            assert result.blocks == expected_blocks
            assert result.raw_payloads == ((first, second) if raw else ())
        finally:
            transport.close()
        assert server.wait_for_handlers(1)


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
@pytest.mark.parametrize("raw", [False, True])
def test_native_corporate_finance_preserves_server_order_dates_and_raw(transport_type, raw):
    data = _finance(("600002", "600001"))
    expected = _parse(0x0010, data, dict(codes=[CODE, SECOND], include_raw=raw))

    def handler(connection):
        answer_handshake(connection)
        _answer(connection, 0x0010, [CODE, SECOND], data)
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        transport = transport_type([server.host], timeout=2, heartbeat_interval=None)
        try:
            result = CorporateApi(transport, native_services=True).finance_batch([CODE, SECOND], include_raw=raw)
            assert result == expected
            assert [row.full_code for row in result.records] == [SECOND, CODE]
            assert result.records[0].updated_date is None
            assert result.records[0].liu_tong_gu_ben_raw_float == -5.0
        finally:
            transport.close()
        assert server.wait_for_handlers(1)


def test_native_corporate_finance_recovery_keeps_missing_warning_and_aggregate_raw():
    valid = _finance()

    def handler(connection):
        answer_handshake(connection)
        _answer(connection, 0x0010, [CODE, SECOND], valid)
        _answer(connection, 0x0010, [CODE], valid)
        _answer(connection, 0x0010, [SECOND], b"\x00\x00")
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        transport = SocketTransport([server.host], timeout=2, heartbeat_interval=None)
        try:
            with pytest.warns(MissingCodesWarning, match=SECOND):
                result = CorporateApi(transport, native_services=True).finance_batch([CODE, SECOND], include_raw=True, on_missing="skip")
            assert result.raw_payload == b"" and [row.full_code for row in result.records] == [CODE]
        finally:
            transport.close()
        assert server.wait_for_handlers(1)


def test_native_corporate_finance_keeps_nonfinite_legacy_value():
    data = _finance(value=float("nan"))

    def handler(connection):
        answer_handshake(connection)
        _answer(connection, 0x0010, [CODE], data)
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        transport = SocketTransport([server.host], timeout=2, heartbeat_interval=None)
        try:
            result = CorporateApi(transport, native_services=True).finance_batch(CODE)
            assert math.isnan(result.records[0].liu_tong_gu_ben_raw_float)
        finally:
            transport.close()
        assert server.wait_for_handlers(1)


def test_native_corporate_capital_protocol_error_keeps_original_class_text_and_cause():
    data = b"\x01"
    with pytest.raises(ProtocolError) as expected:
        _parse(0x000F, data, dict(code=CODE))

    def handler(connection):
        answer_handshake(connection)
        _answer(connection, 0x000F, [CODE], data)
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        transport = SocketTransport([server.host], timeout=2, heartbeat_interval=None)
        try:
            with pytest.raises(ProtocolError) as actual:
                CorporateApi(transport, native_services=True).capital_changes(CODE)
            assert type(actual.value) is type(expected.value)
            assert str(actual.value) == str(expected.value)
            assert actual.value.__cause__ is expected.value.__cause__ is None
        finally:
            transport.close()
        assert server.wait_for_handlers(1)


def test_native_corporate_interrupt_confirms_request_retirement():
    requested = threading.Event()

    def handler(connection):
        answer_handshake(connection)
        _, command, _ = read_request(connection)
        assert command == 0x0010
        requested.set()
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        transport = SocketTransport([server.host], timeout=2, heartbeat_interval=None)

        def interrupt():
            assert requested.wait(timeout=3)
            time.sleep(0.05)
            _thread.interrupt_main()

        worker = threading.Thread(target=interrupt, daemon=True)
        worker.start()
        try:
            with pytest.raises(KeyboardInterrupt):
                CorporateApi(transport, native_services=True).finance_batch(CODE)
        finally:
            transport.close()
            worker.join(timeout=3)
        assert not worker.is_alive() and server.wait_for_handlers(1)
