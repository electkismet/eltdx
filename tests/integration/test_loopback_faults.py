from __future__ import annotations

import _thread
import threading
import time

import pytest

from eltdx import _native

from loopback_support import (
    ScriptedServer,
    answer_handshake,
    read_request,
    response_bytes,
    wait_for_peer_close,
)


def native_engine(host: str, *, pool_size: int = 1, timeout: float = 1.0):
    return _native.NativeEngine(
        [host],
        timeout=timeout,
        pool_size=pool_size,
        heartbeat_interval=None,
        max_pending_requests=8,
        push_queue_size=16,
        push_queue_bytes=64 * 1024,
    )


def test_explicit_connect_failure_joins_every_started_slot_before_publication() -> None:
    both_started = threading.Barrier(2)
    successful_socket_closed = threading.Event()

    def successful(connection) -> None:
        answer_handshake(connection)
        both_started.wait(timeout=3)
        wait_for_peer_close(connection)
        successful_socket_closed.set()

    def failed(connection) -> None:
        message_id, message_type, payload = read_request(connection)
        assert message_type == 0x000D and payload == b"\x01"
        assert message_id != 0
        both_started.wait(timeout=3)

    with ScriptedServer([successful, failed]) as server:
        engine = native_engine(server.host, pool_size=2)
        with pytest.raises(Exception) as raised:
            engine.connect()
        assert successful_socket_closed.wait(timeout=1)
        assert "connect" in str(raised.value).lower() or "closed" in str(raised.value).lower()
        engine.close()


def test_malformed_handshake_rolls_back_the_complete_unpublished_epoch() -> None:
    socket_closed = threading.Event()

    def malformed(connection) -> None:
        message_id, message_type, payload = read_request(connection)
        assert message_type == 0x000D and payload == b"\x01"
        connection.sendall(response_bytes(message_id, message_type, b"too short"))
        wait_for_peer_close(connection)
        socket_closed.set()

    with ScriptedServer([malformed]) as server:
        engine = native_engine(server.host)
        with pytest.raises(Exception, match="handshake"):
            engine.connect()
        assert socket_closed.wait(timeout=1)
        engine.close()


def test_connect_keyboard_interrupt_preserves_the_original_exception() -> None:
    handshake_seen = threading.Event()
    socket_closed = threading.Event()

    def blocked(connection) -> None:
        message_id, message_type, payload = read_request(connection)
        assert message_id != 0
        assert message_type == 0x000D and payload == b"\x01"
        handshake_seen.set()
        wait_for_peer_close(connection)
        socket_closed.set()

    def interrupt_main() -> None:
        assert handshake_seen.wait(timeout=3)
        time.sleep(0.05)
        _thread.interrupt_main()

    with ScriptedServer([blocked]) as server:
        engine = native_engine(server.host, timeout=3)
        interrupter = threading.Thread(target=interrupt_main, daemon=True)
        interrupter.start()
        started = time.monotonic()
        with pytest.raises(KeyboardInterrupt) as raised:
            engine.connect()
        elapsed = time.monotonic() - started
        interrupter.join(timeout=1)
        assert raised.value.__cause__ is None
        assert elapsed < 1.5
        assert socket_closed.wait(timeout=1)
        engine.close()
