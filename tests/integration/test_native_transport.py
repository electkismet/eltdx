from __future__ import annotations

import time
from datetime import date

import pytest

from eltdx.exceptions import ConnectionClosedError
from eltdx.models import HandshakeInfo, HeartbeatAck, QuoteRefreshPage
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


def _heartbeat_payload() -> bytes:
    return b"\x00" * 6 + (20260815).to_bytes(4, "little")


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
