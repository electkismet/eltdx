from __future__ import annotations

import _thread
import threading
import time
from datetime import date
from pathlib import Path

import pytest

from eltdx.client import TdxClient
from eltdx.exceptions import ConnectionClosedError, MissingCodesWarning, ProtocolError
from eltdx.models import HandshakeInfo, HeartbeatAck, QuoteRefreshPage
from eltdx.protocol import decode_response
from eltdx.protocol.constants import (
    TYPE_HEARTBEAT,
    TYPE_REFRESH_STREAM,
    TYPE_SECURITY_COUNT,
)
from eltdx.protocol.frame import ResponseFrame
from eltdx.transport import PooledSocketTransport, SocketTransport
from eltdx.transport.actor import RuntimeState, TcpState
from eltdx.transport.pool import PoolState

from loopback_support import (
    ScriptedServer,
    answer_handshake,
    read_request,
    response_bytes,
    wait_for_peer_close,
)

_BRIDGE_CODE = bytes((115, 122, 48, 48, 48, 48, 48, 49)).decode("ascii")


def _heartbeat_payload() -> bytes:
    return b"\x00" * 6 + (20260815).to_bytes(4, "little")


def _quote_bridge_payloads() -> tuple[bytes, bytes]:
    fixtures = Path(__file__).parents[1] / "fixtures" / "7709"
    directory = decode_response(
        (fixtures / "security_list" / "normal" / "response.bin").read_bytes()
    ).data
    snapshot = decode_response(
        (fixtures / "snapshots" / "normal" / "response.bin").read_bytes()
    ).data[4:]
    row = b"\x00" + b"000001" + snapshot[7:]
    return directory, b"\x00\x00\x01\x00" + row


def _quote_bridge_snapshot_payload(codes: tuple[str, ...]) -> bytes:
    _, single = _quote_bridge_payloads()
    row_template = single[4:]
    rows = b"".join(
        b"\x00" + code[2:].encode("ascii") + row_template[7:]
        for code in codes
    )
    return single[:2] + len(codes).to_bytes(2, "little") + rows


def _serve_quote_bridge(connection, *, snapshot_payload: bytes | None, snapshot_seen=None) -> None:
    from eltdx.protocol.constants import TYPE_SECURITY_LIST, TYPE_SNAPSHOTS

    directory, _ = _quote_bridge_payloads()
    answer_handshake(connection)
    message_id, message_type, _ = read_request(connection)
    assert message_type == TYPE_SECURITY_LIST
    connection.sendall(response_bytes(message_id, message_type, directory))
    message_id, message_type, _ = read_request(connection)
    assert message_type == TYPE_SECURITY_LIST
    connection.sendall(response_bytes(message_id, message_type, b"\x00\x00"))
    message_id, message_type, _ = read_request(connection)
    assert message_type == TYPE_SNAPSHOTS
    if snapshot_seen is not None:
        snapshot_seen.set()
    if snapshot_payload is None:
        wait_for_peer_close(connection)
        return
    connection.sendall(response_bytes(message_id, message_type, snapshot_payload))
    wait_for_peer_close(connection)


def _serve_quote_bridge_recovery(connection, snapshot_payloads: tuple[bytes, ...]) -> None:
    from eltdx.protocol.constants import TYPE_SECURITY_LIST, TYPE_SNAPSHOTS

    directory, _ = _quote_bridge_payloads()
    answer_handshake(connection)
    message_id, message_type, _ = read_request(connection)
    assert message_type == TYPE_SECURITY_LIST
    connection.sendall(response_bytes(message_id, message_type, directory))
    message_id, message_type, _ = read_request(connection)
    assert message_type == TYPE_SECURITY_LIST
    connection.sendall(response_bytes(message_id, message_type, b"\x00\x00"))
    for payload in snapshot_payloads:
        message_id, message_type, _ = read_request(connection)
        assert message_type == TYPE_SNAPSHOTS
        connection.sendall(response_bytes(message_id, message_type, payload))
    wait_for_peer_close(connection)


def test_python_quote_facade_uses_shared_rust_snapshot_service() -> None:
    _, snapshot_payload = _quote_bridge_payloads()

    def handler(connection) -> None:
        _serve_quote_bridge(connection, snapshot_payload=snapshot_payload)

    with ScriptedServer([handler]) as server:
        transport = SocketTransport([server.host], timeout=2, heartbeat_interval=None)
        client = TdxClient(transport=transport)
        try:
            rows = client.quotes.get_snapshots(_BRIDGE_CODE)
            assert [row.full_code for row in rows] == [_BRIDGE_CODE]
        finally:
            client.close()
        assert server.wait_for_handlers(1)


def test_python_quote_bridge_preserves_server_order_on_initial_snapshot() -> None:
    second_code = _BRIDGE_CODE[:-1] + "2"
    response_codes = (second_code, _BRIDGE_CODE)
    snapshot_payload = _quote_bridge_snapshot_payload(response_codes)

    def handler(connection) -> None:
        _serve_quote_bridge(connection, snapshot_payload=snapshot_payload)

    with ScriptedServer([handler]) as server:
        client = TdxClient(
            transport=SocketTransport([server.host], timeout=2, heartbeat_interval=None)
        )
        try:
            rows = client.quotes.get_snapshots([_BRIDGE_CODE, second_code])
            assert [row.full_code for row in rows] == list(response_codes)
        finally:
            client.close()
        assert server.wait_for_handlers(1)


def test_python_quote_bridge_recovery_silently_omits_unmatched_rows() -> None:
    second_code = _BRIDGE_CODE[:-1] + "2"
    other_code_row = _quote_bridge_snapshot_payload((second_code,))
    empty = b"\x00\x00\x00\x00"

    def handler(connection) -> None:
        _serve_quote_bridge_recovery(
            connection, (empty, other_code_row, other_code_row)
        )

    with ScriptedServer([handler]) as server:
        client = TdxClient(
            transport=SocketTransport([server.host], timeout=2, heartbeat_interval=None)
        )
        try:
            rows = client.quotes.get_snapshots([_BRIDGE_CODE, second_code])
            assert [row.full_code for row in rows] == [second_code]
        finally:
            client.close()
        assert server.wait_for_handlers(1)


def test_python_quote_service_keeps_protocol_error_cause_empty() -> None:
    def handler(connection) -> None:
        _serve_quote_bridge(connection, snapshot_payload=b"\x00")

    with ScriptedServer([handler]) as server:
        client = TdxClient(
            transport=SocketTransport([server.host], timeout=2, heartbeat_interval=None)
        )
        try:
            with pytest.raises(ProtocolError) as raised:
                client.quotes.get_snapshots(_BRIDGE_CODE)
            assert raised.value.__cause__ is None
        finally:
            client.close()
        assert server.wait_for_handlers(1)


def test_python_quote_service_keyboard_interrupt_waits_for_cancellation() -> None:
    snapshot_seen = threading.Event()

    def handler(connection) -> None:
        _serve_quote_bridge(
            connection,
            snapshot_payload=None,
            snapshot_seen=snapshot_seen,
        )

    with ScriptedServer([handler]) as server:
        client = TdxClient(
            transport=SocketTransport([server.host], timeout=3, heartbeat_interval=None)
        )

        def interrupt_main() -> None:
            assert snapshot_seen.wait(timeout=3)
            time.sleep(0.05)
            _thread.interrupt_main()

        interrupter = threading.Thread(target=interrupt_main, daemon=True)
        interrupter.start()
        try:
            with pytest.raises(KeyboardInterrupt) as raised:
                client.quotes.get_snapshots(_BRIDGE_CODE)
            assert raised.value.__cause__ is None
            interrupter.join(timeout=1)
        finally:
            client.close()
        assert server.wait_for_handlers(1)


def _answer_count(connection, value: int) -> None:
    message_id, message_type, _ = read_request(connection)
    assert message_type == TYPE_SECURITY_COUNT
    connection.sendall(response_bytes(message_id, message_type, value.to_bytes(2, "little")))


def _wait_for_push_diagnostics(transport: SocketTransport, expected: int) -> None:
    deadline = time.monotonic() + 1.0
    while transport.diagnostics.push_frames != expected:
        if time.monotonic() >= deadline:
            raise AssertionError(
                "push diagnostics did not converge before the deadline: "
                f"expected={expected}, actual={transport.diagnostics.push_frames}"
            )
        time.sleep(0.005)


def test_socket_execute_push_diagnostics_and_session_models() -> None:
    def handler(connection) -> None:
        answer_handshake(connection)
        message_id, message_type, _ = read_request(connection)
        assert message_type == TYPE_SECURITY_COUNT
        connection.sendall(response_bytes(0x290001, TYPE_REFRESH_STREAM, b"\x93\x93"))
        connection.sendall(response_bytes(0x290003, TYPE_REFRESH_STREAM, b"\x93\x93"))
        connection.sendall(response_bytes(message_id, message_type, (321).to_bytes(2, "little")))

        message_id, message_type, payload = read_request(connection)
        assert message_type == TYPE_HEARTBEAT and payload == b""
        connection.sendall(response_bytes(message_id, message_type, _heartbeat_payload()))
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        transport = SocketTransport(
            [server.host],
            timeout=1,
            heartbeat_interval=None,
            push_queue_size=8,
        )
        try:
            transport.connect()
            assert transport.execute(TYPE_SECURITY_COUNT, {"market": "sz"}) == 321
            heartbeat = transport.execute(TYPE_HEARTBEAT)
            assert isinstance(heartbeat, HeartbeatAck)
            assert heartbeat.server_date == date(2026, 8, 15)

            raw_push = transport.poll_push(parse=False)
            parsed_push = transport.poll_push(parse=True)
            assert isinstance(raw_push, ResponseFrame)
            assert raw_push.msg_type == TYPE_REFRESH_STREAM
            assert isinstance(parsed_push, QuoteRefreshPage)
            assert parsed_push.count == 0

            handshake = transport.last_handshake
            assert isinstance(handshake, HandshakeInfo)
            assert handshake.server_name == "native-loopback"
            assert transport.last_heartbeat == heartbeat
            _wait_for_push_diagnostics(transport, 0)
            diagnostics = transport.diagnostics
            assert diagnostics.epoch > 0
            assert diagnostics.actor is not None
            assert diagnostics.actor.state is RuntimeState.RUNNING
            assert diagnostics.actor.tcp_state is TcpState.READY
            assert diagnostics.actor.connected_host == server.host
            assert diagnostics.push_frames == 0
            assert diagnostics.push_dropped == 0
            assert diagnostics.runtime_workers == 1
            assert diagnostics.raw_bytes <= diagnostics.raw_max_bytes
            assert diagnostics.decoded_bytes <= diagnostics.decoded_max_bytes
        finally:
            transport.close()
        assert transport.diagnostics.actor is None
        assert transport.diagnostics.raw_bytes == 0
        assert transport.diagnostics.decoded_bytes == 0


def test_pool_reuses_slots_and_pin_keeps_connection_affinity() -> None:
    def handler(connection) -> None:
        answer_handshake(connection)
        for value in (10, 11, 12, 13):
            _answer_count(connection, value)
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        pool = PooledSocketTransport(
            [server.host],
            timeout=1,
            pool_size=1,
            heartbeat_interval=None,
            max_pending_requests=4,
        )
        try:
            pool.connect()
            assert pool.execute(TYPE_SECURITY_COUNT, {"market": "sz"}) == 10
            assert pool.execute(TYPE_SECURITY_COUNT, {"market": "sz"}) == 11
            with pool.pin() as pinned:
                assert pinned.connected_host == server.host
                assert pinned.execute(TYPE_SECURITY_COUNT, {"market": "sz"}) == 12
                assert pinned.connected_host == server.host
            assert pool.execute(TYPE_SECURITY_COUNT, {"market": "sz"}) == 13

            diagnostics = pool.diagnostics
            assert diagnostics.state is PoolState.RUNNING
            assert diagnostics.broker is not None
            assert diagnostics.broker.active_leases == 0
            assert diagnostics.broker.waiter_count == 0
            assert diagnostics.broker.pin_waiter_count == 0
            assert len(diagnostics.actors) == 1
            assert diagnostics.actors[0].connected_host == server.host
            assert diagnostics.pool_size == 1
            assert diagnostics.server_count == 1
            assert diagnostics.max_connections_per_host == 1
            assert server.accepted_count == 1
        finally:
            pool.close()
        assert pool.diagnostics.raw_bytes == 0
        assert pool.diagnostics.decoded_bytes == 0


def test_close_reopen_invalidates_the_old_pinned_proxy() -> None:
    def first(connection) -> None:
        answer_handshake(connection)
        wait_for_peer_close(connection)

    def second(connection) -> None:
        answer_handshake(connection)
        _answer_count(connection, 77)
        wait_for_peer_close(connection)

    with ScriptedServer([first, second]) as server:
        pool = PooledSocketTransport(
            [server.host],
            timeout=1,
            pool_size=1,
            heartbeat_interval=None,
        )
        pin_context = pool.pin()
        pinned = pin_context.__enter__()
        first_epoch = pool.diagnostics.epoch
        try:
            pool.close()
            pool.connect()
            assert pool.diagnostics.epoch > first_epoch
            with pytest.raises(ConnectionClosedError, match="no longer valid"):
                _ = pinned.connected_host
            with pytest.raises(ConnectionClosedError, match="no longer valid"):
                pinned.execute(TYPE_SECURITY_COUNT, {"market": "sz"})
            assert pool.execute(TYPE_SECURITY_COUNT, {"market": "sz"}) == 77
        finally:
            pin_context.__exit__(None, None, None)
            pool.close()
