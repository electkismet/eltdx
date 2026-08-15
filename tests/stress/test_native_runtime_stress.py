from __future__ import annotations

import multiprocessing
import os
import subprocess
import sys
import textwrap
import time
from collections.abc import Callable
from concurrent.futures import ThreadPoolExecutor
from typing import Any

import pytest

from eltdx.exceptions import PoolBusyError, PushOverflowError, TransportError
from eltdx.models import FileContentChunk
from eltdx.protocol.constants import TYPE_FILE_CONTENT
from eltdx.transport import PooledSocketTransport, SocketTransport
from eltdx.transport.actor import RuntimeState
from eltdx.transport.pool import PoolState

from runtime_support import CONTENT, PATH, NativeStressServer


def _count(name: str, default: int) -> int:
    value = int(os.environ.get(name, str(default)))
    if value <= 0:
        raise ValueError(f"{name} must be positive")
    return value


def _execute_identity(transport, token: int) -> tuple[int, int, int]:
    result = transport.execute(
        TYPE_FILE_CONTENT,
        {"path": PATH, "offset": token, "size": CONTENT.size},
    )
    if not isinstance(result, FileContentChunk):
        raise AssertionError(f"stress response is not FileContentChunk: {result!r}")
    echoed, connection_id, request_sequence, checksum = CONTENT.unpack(result.content)
    if result.offset != token or echoed != token:
        raise AssertionError(f"cross-request completion: requested={token}, echoed={echoed}")
    if checksum != echoed ^ connection_id ^ request_sequence:
        raise AssertionError("stress response identity checksum mismatch")
    return echoed, connection_id, request_sequence


def _wait_until(predicate: Callable[[], bool], timeout: float = 5.0) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.005)
    return predicate()


def _ordinary_waiters(transport: PooledSocketTransport) -> int:
    broker = transport.diagnostics.broker
    return -1 if broker is None else broker.waiter_count


def _pin_waiters(transport: PooledSocketTransport) -> int:
    broker = transport.diagnostics.broker
    return -1 if broker is None else broker.pin_waiter_count


def _acquire_pin(transport: PooledSocketTransport) -> str | None:
    with transport.pin() as pinned:
        return pinned.connected_host


def _fresh_process(host: str, token: int, result_queue: Any) -> None:
    try:
        transport = SocketTransport([host], timeout=10, heartbeat_interval=None)
        try:
            echoed = _execute_identity(transport, token)[0]
        finally:
            transport.close()
        result_queue.put(("ok", echoed))
    except BaseException as error:
        result_queue.put(("error", type(error).__name__, str(error)))


def _fork_process(
    host: str,
    inherited: SocketTransport,
    token: int,
    result_queue: Any,
) -> None:
    try:
        try:
            _execute_identity(inherited, token)
        except TransportError as error:
            inherited_result = (type(error).__name__, str(error))
        else:
            inherited_result = ("accepted", "")
        fresh = SocketTransport([host], timeout=10, heartbeat_interval=None)
        try:
            echoed = _execute_identity(fresh, token)[0]
        finally:
            fresh.close()
        result_queue.put(("ok", inherited_result, echoed))
    except BaseException as error:
        result_queue.put(("error", type(error).__name__, str(error)))


def _run_process(context: Any, target: Callable[..., None], *args: Any) -> tuple[Any, ...]:
    result_queue = context.Queue()
    process = context.Process(target=target, args=(*args, result_queue))
    process.start()
    process.join(timeout=30)
    if process.is_alive():
        process.terminate()
        process.join(timeout=5)
        raise AssertionError(f"stress child did not exit: pid={process.pid}")
    result = result_queue.get(timeout=5)
    result_queue.close()
    result_queue.join_thread()
    if process.exitcode != 0:
        raise AssertionError(f"stress child exit code: {process.exitcode}")
    return result


def test_one_hundred_thousand_requests_are_unique_and_resource_bounded() -> None:
    request_count = _count("ELTDX_STRESS_REQUESTS", 100_000)
    concurrency = _count("ELTDX_STRESS_THREADS", 100)
    with NativeStressServer(response_delay=0.0001) as server:
        transport = PooledSocketTransport(
            [server.host],
            timeout=30,
            pool_size=4,
            heartbeat_interval=None,
            max_pending_requests=concurrency,
        )
        try:
            transport.connect()
            with ThreadPoolExecutor(max_workers=concurrency) as executor:
                values = list(
                    executor.map(
                        lambda token: _execute_identity(transport, token),
                        range(request_count),
                    )
                )
            assert len({value[0] for value in values}) == request_count
            assert len({value[2] for value in values}) == request_count
            assert server.wait_for_idle()
            diagnostics = transport.diagnostics
            assert diagnostics.state is PoolState.RUNNING
            assert diagnostics.broker is not None
            assert diagnostics.broker.waiter_count == 0
            assert diagnostics.broker.pin_waiter_count == 0
            assert diagnostics.broker.active_leases == 0
            assert len(diagnostics.actors) == 4
            assert all(actor.pending_depth == 0 for actor in diagnostics.actors)
        finally:
            transport.close()
        assert transport.diagnostics.state is PoolState.STOPPED
        assert transport.diagnostics.actors == ()
        assert server.wait_for_no_workers()


@pytest.mark.parametrize("pool_size", (1, 4, 8))
def test_pool_sizes_preserve_exact_completion_and_cleanup(pool_size: int) -> None:
    request_count = _count("ELTDX_STRESS_POOL_REQUESTS", 2_048)
    with NativeStressServer() as server:
        transport = PooledSocketTransport(
            [server.host],
            timeout=10,
            pool_size=pool_size,
            heartbeat_interval=None,
            max_pending_requests=128,
        )
        try:
            with ThreadPoolExecutor(max_workers=100) as executor:
                values = list(
                    executor.map(
                        lambda token: _execute_identity(transport, token),
                        range(request_count),
                    )
                )
            assert {value[0] for value in values} == set(range(request_count))
            assert len(transport.diagnostics.actors) == pool_size
        finally:
            transport.close()
        assert transport.diagnostics.state is PoolState.STOPPED
        assert server.wait_for_no_workers()


def test_ten_thousand_tcp_generations_keep_one_engine_and_no_live_actor() -> None:
    generation_count = _count("ELTDX_STRESS_GENERATIONS", 10_000)
    with NativeStressServer(close_after_response=True) as server:
        transport = SocketTransport([server.host], timeout=10, heartbeat_interval=None)
        try:
            for token in range(generation_count):
                assert _execute_identity(transport, token)[0] == token
            diagnostics = transport.diagnostics
            assert diagnostics.actor is not None
            assert diagnostics.actor.reconnect_count >= generation_count - 1
            assert server.accepted >= generation_count
        finally:
            transport.close()
        diagnostics = transport.diagnostics
        assert diagnostics.actor is None
        assert server.wait_for_no_workers()


def test_ten_thousand_normal_close_reopen_cycles_end_stopped() -> None:
    cycle_count = _count("ELTDX_STRESS_CLOSE_CYCLES", 10_000)
    with NativeStressServer() as server:
        transport = SocketTransport([server.host], timeout=10, heartbeat_interval=None)
        last_epoch = 0
        for _ in range(cycle_count):
            transport.connect()
            diagnostics = transport.diagnostics
            assert diagnostics.epoch > last_epoch
            assert diagnostics.actor is not None
            assert diagnostics.actor.state is RuntimeState.RUNNING
            last_epoch = diagnostics.epoch
            transport.close()
            assert transport.diagnostics.actor is None
        assert transport.diagnostics.epoch == last_epoch
        assert server.wait_for_no_workers()


def test_waiting_capacity_rejects_without_losing_admitted_requests() -> None:
    with NativeStressServer(response_delay=1.0) as server:
        transport = PooledSocketTransport(
            [server.host],
            timeout=10,
            pool_size=1,
            heartbeat_interval=None,
            max_pending_requests=1,
        )
        try:
            transport.connect()
            with ThreadPoolExecutor(max_workers=2) as executor:
                active = executor.submit(_execute_identity, transport, 1)
                assert server.wait_for_active(1)
                waiting = executor.submit(_execute_identity, transport, 2)
                assert _wait_until(lambda: _ordinary_waiters(transport) == 1)
                with pytest.raises(PoolBusyError):
                    _execute_identity(transport, 3)
                assert active.result(timeout=5)[0] == 1
                assert waiting.result(timeout=5)[0] == 2
        finally:
            transport.close()
        assert transport.diagnostics.state is PoolState.STOPPED
        assert server.wait_for_no_workers()


def test_pin_waiting_capacity_is_shared_and_exactly_released() -> None:
    with NativeStressServer() as server:
        transport = PooledSocketTransport(
            [server.host],
            timeout=10,
            pool_size=1,
            heartbeat_interval=None,
            max_pending_requests=1,
        )
        try:
            transport.connect()
            with ThreadPoolExecutor(max_workers=1) as executor:
                with transport.pin():
                    waiting = executor.submit(_acquire_pin, transport)
                    assert _wait_until(lambda: _ordinary_waiters(transport) == 1)
                    with pytest.raises(PoolBusyError):
                        with transport.pin():
                            raise AssertionError("full pin queue admitted another owner")
                assert waiting.result(timeout=5) == server.host
            broker = transport.diagnostics.broker
            assert broker is not None
            assert broker.waiter_count == 0
            assert broker.pin_waiter_count == 0
            assert broker.active_leases == 0
        finally:
            transport.close()
        assert server.wait_for_no_workers()


def test_pin_local_waiting_capacity_rejects_without_a_second_active_lease() -> None:
    with NativeStressServer(response_delay=1.0) as server:
        transport = PooledSocketTransport(
            [server.host],
            timeout=10,
            pool_size=1,
            heartbeat_interval=None,
            max_pending_requests=1,
        )
        try:
            transport.connect()
            with transport.pin() as pinned:
                with ThreadPoolExecutor(max_workers=2) as executor:
                    active = executor.submit(_execute_identity, pinned, 4)
                    assert server.wait_for_active(1)
                    waiting = executor.submit(_execute_identity, pinned, 5)
                    assert _wait_until(lambda: _pin_waiters(transport) == 1)
                    broker = transport.diagnostics.broker
                    assert broker is not None
                    assert broker.active_leases == 1
                    with pytest.raises(PoolBusyError):
                        _execute_identity(pinned, 6)
                    assert active.result(timeout=5)[0] == 4
                    assert waiting.result(timeout=5)[0] == 5
            broker = transport.diagnostics.broker
            assert broker is not None
            assert broker.pin_waiter_count == 0
            assert broker.active_leases == 0
        finally:
            transport.close()
        assert server.wait_for_no_workers()


def test_push_capacity_reports_one_gap_and_retains_newest_frames() -> None:
    with NativeStressServer(push_frames_per_response=5) as server:
        transport = SocketTransport(
            [server.host],
            timeout=10,
            heartbeat_interval=None,
            push_queue_size=2,
            push_queue_bytes=1_024,
        )
        try:
            assert _execute_identity(transport, 7)[0] == 7
            diagnostics = transport.diagnostics
            assert diagnostics.push_frames == 2
            assert diagnostics.push_dropped == 3
            with pytest.raises(PushOverflowError):
                transport.poll_push(timeout=0, parse=False)
            retained = transport.drain_pushes(parse=False)
            assert len(retained) == 2
            assert len({frame.message_id for frame in retained}) == 2
            assert transport.drain_pushes(parse=False) == []
        finally:
            transport.close()
        assert server.wait_for_no_workers()


def test_low_priority_heartbeat_recovers_after_request_pressure() -> None:
    request_count = _count("ELTDX_STRESS_HEARTBEAT_REQUESTS", 2_048)
    with NativeStressServer(response_delay=0.0001) as server:
        transport = SocketTransport(
            [server.host],
            timeout=10,
            heartbeat_interval=0.01,
            max_pending_requests=128,
        )
        try:
            with ThreadPoolExecutor(max_workers=100) as executor:
                values = list(
                    executor.map(
                        lambda token: _execute_identity(transport, token),
                        range(request_count),
                    )
                )
            assert {value[0] for value in values} == set(range(request_count))
            assert _wait_until(lambda: transport.last_heartbeat is not None, timeout=5)
            assert transport.last_heartbeat.server_date_raw == 20260815
            diagnostics = transport.diagnostics
            assert diagnostics.actor is not None
            assert diagnostics.actor.pending_depth == 0
        finally:
            transport.close()
        assert server.wait_for_no_workers()


def test_interpreter_exit_drops_an_unclosed_engine_without_hanging() -> None:
    script = textwrap.dedent(
        """
        import sys
        from eltdx.protocol.constants import TYPE_FILE_CONTENT
        from eltdx.transport import SocketTransport

        transport = SocketTransport([sys.argv[1]], timeout=10, heartbeat_interval=None)
        value = transport.execute(
            TYPE_FILE_CONTENT,
            {"path": "eltdx-native-stress.bin", "offset": 17, "size": 16},
        )
        assert value.offset == 17 and len(value.content) == 16
        print("unclosed-engine-exit-ok")
        """
    )
    with NativeStressServer() as server:
        completed = subprocess.run(
            [sys.executable, "-c", script, server.host],
            check=False,
            capture_output=True,
            text=True,
            timeout=30,
        )
        assert completed.returncode == 0, completed.stderr
        assert "unclosed-engine-exit-ok" in completed.stdout
        assert server.wait_for_no_workers()


def test_spawn_child_uses_a_fresh_engine_and_cleans_up() -> None:
    context = multiprocessing.get_context("spawn")
    with NativeStressServer() as server:
        assert _run_process(context, _fresh_process, server.host, 23) == ("ok", 23)
        assert server.wait_for_no_workers()


@pytest.mark.skipif(
    "fork" not in multiprocessing.get_all_start_methods(),
    reason="fork is unavailable on this platform",
)
def test_fork_rejects_inherited_engine_before_allowing_a_fresh_one() -> None:
    context = multiprocessing.get_context("fork")
    with NativeStressServer() as server:
        inherited = SocketTransport([server.host], timeout=10, heartbeat_interval=None)
        try:
            inherited.connect()
            result = _run_process(context, _fork_process, server.host, inherited, 29)
            assert result[0] == "ok"
            assert result[1][0] != "accepted"
            assert "after fork" in result[1][1]
            assert result[2] == 29
            assert _execute_identity(inherited, 31)[0] == 31
        finally:
            inherited.close()
        assert server.wait_for_no_workers()
