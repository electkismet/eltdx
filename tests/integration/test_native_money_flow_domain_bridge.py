"""Authored local money-flow wire parity and recovery-boundary cases."""
from __future__ import annotations

import _thread
import math
import struct
import threading
import time
from types import SimpleNamespace

import pytest

from eltdx.api.money_flow import MoneyFlowApi
from eltdx.client import TdxClient
from eltdx.exceptions import ProtocolError
from eltdx.models import MoneyFlowBatch, MoneyFlowBlock
from eltdx.protocol.commands import parse_command_response
from eltdx.protocol.frame import ResponseFrame
from eltdx.transport import PooledSocketTransport, SocketTransport

from loopback_support import ScriptedServer, answer_handshake, read_request, response_bytes, wait_for_peer_close


CODE = "sh" + str(600_001)


def _block(code="600001", market=1, amount=-100000.0, *, count=1):
    header = bytearray(40)
    header[0] = market
    header[2:8] = code.encode("ascii")
    header[38:40] = count.to_bytes(2, "little")
    record = bytearray(88)
    record[:4] = (20261301).to_bytes(4, "little")
    record[8:12] = struct.pack("<f", amount)
    for index, pair in enumerate([(100, 20), (300, 100), (50, 10), (200, 100),
                                  (60, 10), (30, 10), (20, 70), (70, 20)]):
        record[44 + index * 4:48 + index * 4] = struct.pack("<HH", *pair)
    return bytes(header) + bytes(record) * count


def _parse(data, raw):
    return parse_command_response(0x0FFC, ResponseFrame(0, 1, 0x0FFC, len(data), len(data), data, b""),
                                  dict(code=CODE, include_raw=raw))


def _answer(connection, data):
    message_id, command, body = read_request(connection)
    assert command == 0x0FFC, "money flow must not prefetch security metadata"
    assert message_id & 0xFFFF00 == 0x7E2D00
    assert body == b"\x01\x00" + b"600001" + b"\x00" * 30
    connection.sendall(response_bytes(message_id, command, data))


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
@pytest.mark.parametrize("raw", [False, True])
@pytest.mark.parametrize("multiple", [False, True])
def test_native_money_flow_matches_old_identity_order_invalid_date_and_raw(transport_type, raw, multiple):
    data = _block(market=255) + (_block("600002", amount=50000.0) if multiple else b"")
    parsed = _parse(data, raw)
    expected = parsed if multiple else parsed.blocks[0]

    def handler(connection):
        answer_handshake(connection)
        _answer(connection, data)
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        transport = transport_type([server.host], timeout=2, heartbeat_interval=None,
            **({"pool_size": 1} if transport_type is PooledSocketTransport else {}))
        try:
            result = MoneyFlowApi(transport, native_services=True).daily(CODE, include_raw=raw)
            assert result == expected
            block = result.blocks[0] if multiple else result
            assert block.market_id == 255 and block.records[0].date is None
            assert block.records[0].total_amount == -100000.0
            assert block.records[0].record_hex
        finally:
            transport.close()
        assert server.wait_for_handlers(1)


@pytest.mark.parametrize("data", [b"", b"\x00" * 39, _block()[:-1], b"\xff" * 40])
def test_native_money_flow_protocol_errors_keep_original_class_text_and_cause(data):
    with pytest.raises(ProtocolError) as expected:
        _parse(data, True)

    def handler(connection):
        answer_handshake(connection)
        _answer(connection, data)
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        transport = SocketTransport([server.host], timeout=2, heartbeat_interval=None)
        try:
            with pytest.raises(ProtocolError) as actual:
                MoneyFlowApi(transport, native_services=True).daily(CODE, include_raw=True)
            assert type(actual.value) is type(expected.value)
            assert str(actual.value) == str(expected.value)
            assert actual.value.__cause__ is expected.value.__cause__ is None
        finally:
            transport.close()
        assert server.wait_for_handlers(1)


def test_native_money_flow_preserves_nonfinite_legacy_record_value():
    data = _block(amount=float("nan"))

    def handler(connection):
        answer_handshake(connection)
        _answer(connection, data)
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        transport = SocketTransport([server.host], timeout=2, heartbeat_interval=None)
        try:
            block = MoneyFlowApi(transport, native_services=True).daily(CODE)
            assert math.isnan(block.records[0].total_amount)
            assert math.isnan(block.records[0].main_net)
        finally:
            transport.close()
        assert server.wait_for_handlers(1)


def test_native_money_flow_recovery_reuses_real_probe_and_closes_unadopted_connection():
    empty = MoneyFlowBatch(())
    data = _block()
    created = []

    def handler(connection):
        answer_handshake(connection)
        _answer(connection, data)
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        primary = SimpleNamespace(execute=lambda *_args: empty)

        def factory(host=None):
            if host is None:
                return primary
            pool = SocketTransport([host], timeout=2, heartbeat_interval=None)
            created.append(pool)
            return pool

        api = MoneyFlowApi(primary, transport_factory=factory, candidate_hosts=(server.host,), native_services=True)
        # An existing preferred owner prevents adoption of the newly successful
        # source. Its result is still returned and its actual socket is closed.
        api._preferred_transport = SimpleNamespace(execute=lambda *_args: empty, close=lambda: None)
        api._preferred_host = "existing"
        try:
            assert isinstance(api.daily(CODE), MoneyFlowBlock)
            assert api._preferred_host == "existing" and len(created) == 1
        finally:
            api.close()
        assert server.wait_for_handlers(1)


def test_native_money_flow_dedicated_pool_and_close_recreate_are_unchanged():
    data = _block()

    def handler(connection):
        answer_handshake(connection)
        _answer(connection, data)
        wait_for_peer_close(connection)

    with ScriptedServer([handler, handler]) as server:
        primary = PooledSocketTransport(["127.0.0.1:1"], pool_size=1, timeout=2, heartbeat_interval=None)
        client = TdxClient(transport=primary, pool_size=1, timeout=2, heartbeat_interval=None, probe_hosts=False)
        client._dedicated_hosts = (server.host,)
        try:
            assert client.money_flow.daily(CODE).count == 1
            first = client.money_flow._dedicated_transport
            assert first is not primary and primary._engine is None
            client.close()
            assert client.money_flow._dedicated_transport is None
            assert client.money_flow.daily(CODE).count == 1
            assert client.money_flow._dedicated_transport is not first
        finally:
            client.close()
        assert server.wait_for_handlers(2)


def test_native_money_flow_interrupt_confirms_request_retirement():
    requested = threading.Event()

    def handler(connection):
        answer_handshake(connection)
        _, command, _ = read_request(connection)
        assert command == 0x0FFC
        requested.set()
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        transport = SocketTransport([server.host], timeout=2, heartbeat_interval=None)

        def interrupt():
            assert requested.wait(timeout=3)
            time.sleep(0.05)
            _thread.interrupt_main()

        interrupter = threading.Thread(target=interrupt, daemon=True)
        interrupter.start()
        try:
            with pytest.raises(KeyboardInterrupt):
                MoneyFlowApi(transport, native_services=True).daily(CODE)
        finally:
            transport.close()
            interrupter.join(timeout=3)
        assert not interrupter.is_alive() and server.wait_for_handlers(1)
