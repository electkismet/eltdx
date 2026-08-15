from __future__ import annotations

import threading
import time
from collections.abc import Callable

import pytest

from eltdx import _native

from loopback_support import (
    ScriptedServer,
    answer_handshake,
    handshake_payload,
    read_request,
    response_bytes,
    wait_for_peer_close,
)


def native_engine(host: str, *, pool_size: int = 2, timeout: float = 1.0):
    return _native.NativeEngine(
        [host],
        timeout=timeout,
        pool_size=pool_size,
        heartbeat_interval=None,
        max_pending_requests=8,
        push_queue_size=16,
        push_queue_bytes=64 * 1024,
    )


def call_in_thread(call: Callable[[], None], output: list[BaseException | None]) -> None:
    try:
        call()
    except BaseException as error:
        output.append(error)
    else:
        output.append(None)


def test_concurrent_explicit_connect_callers_share_one_attempt_and_result() -> None:
    sockets_closed = [threading.Event(), threading.Event()]

    def handler(index: int):
        def run(connection) -> None:
            answer_handshake(connection)
            wait_for_peer_close(connection)
            sockets_closed[index].set()

        return run

    with ScriptedServer([handler(0), handler(1)]) as server:
        engine = native_engine(server.host)
        results: list[BaseException | None] = []
        callers = [
            threading.Thread(
                target=call_in_thread,
                args=(engine.connect, results),
                daemon=True,
            )
            for _ in range(2)
        ]
        for caller in callers:
            caller.start()
        for caller in callers:
            caller.join(timeout=3)
        assert all(not caller.is_alive() for caller in callers)
        assert results == [None, None]
        assert server.accepted_count == 2
        engine.close()
        assert all(closed.wait(timeout=1) for closed in sockets_closed)


def test_public_runtime_gate_stays_unpublished_until_all_handshakes_succeed() -> None:
    both_waiting = threading.Barrier(2)
    release = threading.Event()

    def delayed(connection) -> None:
        message_id, message_type, payload = read_request(connection)
        assert message_id != 0
        assert message_type == 0x000D and payload == b"\x01"
        both_waiting.wait(timeout=3)
        assert release.wait(timeout=3)
        connection.sendall(response_bytes(message_id, message_type, handshake_payload()))
        wait_for_peer_close(connection)

    with ScriptedServer([delayed, delayed]) as server:
        engine = native_engine(server.host)
        results: list[BaseException | None] = []
        caller = threading.Thread(
            target=call_in_thread,
            args=(engine.connect, results),
            daemon=True,
        )
        caller.start()
        assert server.wait_for_connections(2)
        with pytest.raises(Exception, match="connect.*progress|not running"):
            engine.drain_pushes()
        release.set()
        caller.join(timeout=3)
        assert results == [None]
        engine.close()


def test_concurrent_close_waits_for_the_same_connect_rollback() -> None:
    both_started = threading.Barrier(2)
    failure_released = threading.Event()
    successful_socket_closed = threading.Event()

    def successful(connection) -> None:
        answer_handshake(connection)
        both_started.wait(timeout=3)
        wait_for_peer_close(connection)
        successful_socket_closed.set()

    def failed(connection) -> None:
        message_id, message_type, payload = read_request(connection)
        assert message_id != 0
        assert message_type == 0x000D and payload == b"\x01"
        both_started.wait(timeout=3)
        failure_released.set()

    with ScriptedServer([successful, failed]) as server:
        engine = native_engine(server.host, timeout=3)
        connect_results: list[BaseException | None] = []
        close_results: list[BaseException | None] = []
        connect_thread = threading.Thread(
            target=call_in_thread,
            args=(engine.connect, connect_results),
            daemon=True,
        )
        connect_thread.start()
        assert failure_released.wait(timeout=3)
        close_started = time.monotonic()
        close_thread = threading.Thread(
            target=call_in_thread,
            args=(engine.close, close_results),
            daemon=True,
        )
        close_thread.start()
        connect_thread.join(timeout=3)
        close_thread.join(timeout=3)
        close_elapsed = time.monotonic() - close_started
        assert not connect_thread.is_alive() and not close_thread.is_alive()
        assert len(connect_results) == 1 and isinstance(connect_results[0], Exception)
        assert close_results == [None]
        assert successful_socket_closed.wait(timeout=1)
        assert close_elapsed < 1.25
