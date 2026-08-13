"""Deterministic loopback stress, soak, and resource measurements."""

from __future__ import annotations

import argparse
import ctypes
import gc
from ctypes import wintypes
import hashlib
import json
import os
import platform
import socket
import statistics
import struct
import subprocess
import sys
import threading
import time
from concurrent.futures import ThreadPoolExecutor, TimeoutError as FutureTimeoutError
from contextlib import ExitStack
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from eltdx.exceptions import PushOverflowError, TransportError  # noqa: E402
from eltdx.models import FileContentChunk  # noqa: E402
from eltdx.protocol.constants import (  # noqa: E402
    TYPE_FILE_CONTENT,
    TYPE_HANDSHAKE,
    TYPE_HEARTBEAT,
    TYPE_REFRESH_STREAM,
    TYPE_SECURITY_COUNT,
)
from eltdx.transport import PooledSocketTransport, SocketTransport  # noqa: E402
from eltdx.transport.actor import TcpState  # noqa: E402


_STRESS_VALUE = struct.Struct("<IHIQ")
_STRESS_PATH = "actor-stress.bin"


class StressLedger:
    """Wire-side provenance shared by both real loopback servers."""

    def __init__(self) -> None:
        self._lock = threading.Lock()
        self._attempt_sequence = 0
        self._attempts: dict[int, list[tuple[int, int, int]]] = {}
        self._active_business = 0
        self._max_active_business = 0

    def enter_business(self) -> None:
        with self._lock:
            self._active_business += 1
            self._max_active_business = max(self._max_active_business, self._active_business)

    def leave_business(self) -> None:
        with self._lock:
            self._active_business -= 1

    def record_attempt(self, token: int, server_id: int, connection_id: int) -> int:
        with self._lock:
            self._attempt_sequence += 1
            sequence = self._attempt_sequence
            self._attempts.setdefault(token, []).append((sequence, server_id, connection_id))
            return sequence

    def expected_identity(self, token: int) -> tuple[int, int] | None:
        with self._lock:
            attempts = self._attempts.get(token)
            if not attempts:
                return None
            _, server_id, connection_id = attempts[-1]
            return server_id, connection_id

    def expected_provenance(self, token: int) -> tuple[int, int, int] | None:
        with self._lock:
            attempts = self._attempts.get(token)
            if not attempts:
                return None
            sequence, server_id, connection_id = attempts[-1]
            return server_id, connection_id, sequence

    def attempt_count(self, token: int) -> int:
        with self._lock:
            return len(self._attempts.get(token, ()))

    def snapshot(self) -> dict[str, Any]:
        with self._lock:
            retried = [items for items in self._attempts.values() if len(items) > 1]
            cross_endpoint = sum(len({item[1] for item in items}) > 1 for items in retried)
            return {
                "attempts": self._attempt_sequence,
                "logical_requests": len(self._attempts),
                "retried_requests": len(retried),
                "cross_endpoint_retried_requests": cross_endpoint,
                "same_endpoint_retried_requests": len(retried) - cross_endpoint,
                "max_business_active": self._max_active_business,
            }


class StressServer:
    def __init__(
        self,
        *,
        server_id: int = 0,
        ledger: StressLedger | None = None,
        push_every: int = 0,
        poison_every: int = 0,
        close_every: int = 0,
        keep_open_token: int | None = None,
        drop_before_response_every: int = 0,
        fail_before_response_every: int = 0,
        response_delay: float = 0.0,
    ) -> None:
        self.server_id = server_id
        self.ledger = ledger
        self.push_every = push_every
        self.poison_every = poison_every
        self.close_every = close_every
        self.keep_open_token = keep_open_token
        self.drop_before_response_every = drop_before_response_every
        self.fail_before_response_every = fail_before_response_every
        self.response_delay = response_delay
        self._listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self._stop = threading.Event()
        self._condition = threading.Condition()
        self._thread: threading.Thread | None = None
        self._workers: list[threading.Thread] = []
        self._connections: set[socket.socket] = set()
        self.accept_count = 0
        self.business_requests = 0
        self.push_frames = 0
        self.heartbeat_requests = 0
        self.heartbeat_responses = 0
        self.heartbeat_during_business = 0
        self.heartbeat_by_connection: dict[int, int] = {}
        self.heartbeat_responses_by_connection: dict[int, int] = {}
        self._heartbeat_business_phases: dict[str, dict[str, Any]] = {}
        self._active_heartbeat_business_phase_id: str | None = None
        self._heartbeat_phase_wire_lock = threading.Lock()
        self.active_business = 0
        self.max_business_active = 0
        self.errors: list[str] = []
        self.host = ""

    def __enter__(self) -> StressServer:
        self._listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self._listener.bind(("127.0.0.1", 0))
        self._listener.listen()
        address, port = self._listener.getsockname()
        self.host = f"{address}:{port}"
        self._thread = threading.Thread(target=self._accept, name="eltdx-stress-server", daemon=True)
        self._thread.start()
        return self

    def __exit__(self, exc_type, exc, tb) -> None:
        self._stop.set()
        self._listener.close()
        with self._condition:
            connections = tuple(self._connections)
        for conn in connections:
            try:
                conn.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            try:
                conn.close()
            except OSError:
                pass
        if self._thread is not None:
            self._thread.join(timeout=2)
        for worker in self._workers:
            worker.join(timeout=2)
        alive = [worker.name for worker in self._workers if worker.is_alive()]
        if exc_type is None and alive:
            raise RuntimeError(f"stress server workers did not stop: {alive!r}")
        if exc_type is None and self.errors:
            raise RuntimeError(f"stress server errors: {self.errors!r}")
        self._workers.clear()
        self._connections.clear()
        self._thread = None

    def wait_for_business(self, count: int, timeout: float = 2.0) -> bool:
        with self._condition:
            return self._condition.wait_for(lambda: self.business_requests >= count, timeout=timeout)

    def begin_heartbeat_business_phase(
        self,
        phase_id: str,
        *,
        start_business_requests: int,
        target_business_requests: int,
    ) -> None:
        if not phase_id or target_business_requests <= 0:
            raise ValueError("heartbeat business phase requires an ID and positive target")
        with self._condition:
            if phase_id in self._heartbeat_business_phases:
                raise AssertionError(f"duplicate heartbeat business phase ID: {phase_id}")
            if start_business_requests != self.business_requests:
                raise AssertionError(
                    "heartbeat business phase start count changed before publication"
                )
            if self._active_heartbeat_business_phase_id is not None:
                previous = self._heartbeat_business_phases[
                    self._active_heartbeat_business_phase_id
                ]
                if (
                    previous["business_window_open"]
                    or previous["business_responses_sent"]
                    != previous["target_business_requests"]
                ):
                    raise AssertionError("previous heartbeat business phase is still open")
            self._heartbeat_business_phases[phase_id] = {
                "phase_id": phase_id,
                "start_business_requests": start_business_requests,
                "target_business_requests": target_business_requests,
                "business_requests_started": 0,
                "business_responses_sent": 0,
                "business_window_open": False,
                "heartbeat_requests": 0,
            }
            self._active_heartbeat_business_phase_id = phase_id

    def heartbeat_business_phase_snapshot(self, phase_id: str) -> dict[str, Any]:
        with self._heartbeat_phase_wire_lock:
            with self._condition:
                try:
                    phase = self._heartbeat_business_phases[phase_id]
                except KeyError as exc:
                    raise AssertionError(
                        f"unknown heartbeat business phase ID: {phase_id}"
                    ) from exc
                return dict(phase)

    def _record_heartbeat_request(self, connection_id: int) -> None:
        with self._heartbeat_phase_wire_lock:
            with self._condition:
                self.heartbeat_requests += 1
                self.heartbeat_by_connection[connection_id] = (
                    self.heartbeat_by_connection.get(connection_id, 0) + 1
                )
                if self.active_business:
                    self.heartbeat_during_business += 1
                if self._active_heartbeat_business_phase_id is not None:
                    phase = self._heartbeat_business_phases[
                        self._active_heartbeat_business_phase_id
                    ]
                    if phase["business_window_open"]:
                        phase["heartbeat_requests"] += 1
                self._condition.notify_all()

    def _is_heartbeat_business_phase_response(self, sequence: int) -> bool:
        with self._condition:
            phase_id = self._active_heartbeat_business_phase_id
            if phase_id is None:
                return False
            phase = self._heartbeat_business_phases[phase_id]
            phase_start = phase["start_business_requests"]
            phase_end = phase_start + phase["target_business_requests"]
            return phase_start < sequence <= phase_end

    def _record_business_request_started(self) -> int:
        with self._condition:
            self.business_requests += 1
            sequence = self.business_requests
            self.active_business += 1
            self.max_business_active = max(self.max_business_active, self.active_business)
            if self._active_heartbeat_business_phase_id is not None:
                phase = self._heartbeat_business_phases[
                    self._active_heartbeat_business_phase_id
                ]
                phase_start = phase["start_business_requests"]
                phase_end = phase_start + phase["target_business_requests"]
                if phase_start < sequence <= phase_end:
                    phase["business_requests_started"] += 1
                    if phase["business_requests_started"] == 1:
                        phase["business_window_open"] = True
            self._condition.notify_all()
            return sequence

    def _record_business_request_finished(self, sequence: int, *, response_sent: bool) -> None:
        with self._condition:
            self.active_business -= 1
            if self._active_heartbeat_business_phase_id is not None and response_sent:
                phase = self._heartbeat_business_phases[
                    self._active_heartbeat_business_phase_id
                ]
                phase_start = phase["start_business_requests"]
                phase_end = phase_start + phase["target_business_requests"]
                if phase_start < sequence <= phase_end:
                    phase["business_responses_sent"] += 1
                    if (
                        phase["business_responses_sent"]
                        == phase["target_business_requests"]
                    ):
                        phase["business_window_open"] = False
            self._condition.notify_all()

    def wait_for_heartbeat(self, count: int, timeout: float = 2.0) -> bool:
        with self._condition:
            return self._condition.wait_for(lambda: self.heartbeat_requests >= count, timeout=timeout)

    def wait_for_heartbeat_connections(self, count: int, timeout: float = 2.0) -> bool:
        with self._condition:
            return self._condition.wait_for(lambda: len(self.heartbeat_by_connection) >= count, timeout=timeout)

    def wait_for_heartbeat_response_connections(self, count: int, timeout: float = 2.0) -> bool:
        with self._condition:
            return self._condition.wait_for(
                lambda: len(self.heartbeat_responses_by_connection) >= count,
                timeout=timeout,
            )

    def heartbeat_response_snapshot(self) -> dict[int, int]:
        with self._condition:
            return dict(self.heartbeat_responses_by_connection)

    def wait_for_heartbeat_response_round(self, baseline: dict[int, int], timeout: float = 2.0) -> bool:
        with self._condition:
            return self._condition.wait_for(
                lambda: baseline and all(
                    self.heartbeat_responses_by_connection.get(connection_id, 0) > count
                    for connection_id, count in baseline.items()
                ),
                timeout=timeout,
            )

    def _accept(self) -> None:
        while not self._stop.is_set():
            try:
                conn, _ = self._listener.accept()
            except OSError:
                return
            conn.settimeout(5)
            with self._condition:
                self.accept_count += 1
                self._connections.add(conn)
                index = self.accept_count
                self._condition.notify_all()
            worker = threading.Thread(
                target=self._serve,
                args=(conn, index),
                name=f"eltdx-stress-{self.server_id}-conn-{index}",
                daemon=True,
            )
            self._workers.append(worker)
            worker.start()

    def _serve(self, conn: socket.socket, connection_id: int) -> None:
        try:
            with conn:
                while not self._stop.is_set():
                    msg_id, msg_type, request = _read_request(conn)
                    token: int | None = None
                    if msg_type == TYPE_HANDSHAKE:
                        conn.sendall(_response(msg_id, msg_type, _handshake_payload()))
                        continue
                    if msg_type == TYPE_HEARTBEAT:
                        self._record_heartbeat_request(connection_id)
                        if self.response_delay:
                            time.sleep(self.response_delay)
                        conn.sendall(_response(msg_id, msg_type, bytes.fromhex("0000000000008f173501")))
                        with self._condition:
                            self.heartbeat_responses += 1
                            self.heartbeat_responses_by_connection[connection_id] = (
                                self.heartbeat_responses_by_connection.get(connection_id, 0) + 1
                            )
                            self._condition.notify_all()
                        continue
                    if msg_type not in (TYPE_FILE_CONTENT, TYPE_SECURITY_COUNT):
                        raise RuntimeError(f"unexpected stress command: {msg_type:#x}")
                    sequence = self._record_business_request_started()
                    heartbeat_business_phase_response = self._is_heartbeat_business_phase_response(
                        sequence
                    )
                    if self.ledger is not None:
                        self.ledger.enter_business()
                    response_sent = False
                    business_finished = False
                    try:
                        if self.response_delay:
                            time.sleep(self.response_delay)
                        if msg_type == TYPE_FILE_CONTENT:
                            if len(request) < 8:
                                raise RuntimeError("truncated stress file request")
                            token = int.from_bytes(request[:4], "little")
                            requested_size = int.from_bytes(request[4:8], "little")
                            if requested_size < _STRESS_VALUE.size:
                                raise RuntimeError("stress file request is too small")
                            attempt_sequence = (
                                self.ledger.record_attempt(token, self.server_id, connection_id)
                                if self.ledger is not None
                                else sequence
                            )
                            content = _STRESS_VALUE.pack(token, self.server_id, connection_id, attempt_sequence)
                            payload = len(content).to_bytes(4, "little") + content
                            if self.drop_before_response_every and sequence % self.drop_before_response_every == 0:
                                return
                            if self.fail_before_response_every and sequence % self.fail_before_response_every == 0:
                                wire = _response(msg_id, msg_type, payload)
                                conn.sendall(wire[: max(1, len(wire) // 2)])
                                return
                        else:
                            payload = (23285).to_bytes(2, "little")
                        if self.push_every and sequence % self.push_every == 0:
                            conn.sendall(_response(0xF0000000 | (sequence & 0x0FFFFFFF), TYPE_REFRESH_STREAM, b"\x93\x93"))
                            with self._condition:
                                self.push_frames += 1
                        wire = _response(msg_id, msg_type, payload)
                        if msg_type == TYPE_FILE_CONTENT and self.poison_every and sequence % self.poison_every == 0:
                            wire += _response(msg_id, msg_type, payload)
                            with self._condition:
                                self.push_frames += 1
                        if heartbeat_business_phase_response:
                            with self._heartbeat_phase_wire_lock:
                                try:
                                    conn.sendall(wire)
                                    response_sent = True
                                finally:
                                    self._record_business_request_finished(
                                        sequence,
                                        response_sent=response_sent,
                                    )
                                    business_finished = True
                        else:
                            conn.sendall(wire)
                            response_sent = True
                    finally:
                        if not business_finished:
                            self._record_business_request_finished(
                                sequence,
                                response_sent=response_sent,
                            )
                        if self.ledger is not None:
                            self.ledger.leave_business()
                    if (
                        self.close_every
                        and sequence % self.close_every == 0
                        and token != self.keep_open_token
                    ):
                        return
        except (EOFError, OSError, TimeoutError):
            return
        except BaseException as exc:
            self.errors.append(f"{type(exc).__name__}: {exc}")
        finally:
            with self._condition:
                self._connections.discard(conn)


def _execute_unique(transport: Any, token: int) -> dict[str, int]:
    chunk = transport.execute(
        TYPE_FILE_CONTENT,
        {"path": _STRESS_PATH, "offset": token, "size": _STRESS_VALUE.size},
    )
    if not isinstance(chunk, FileContentChunk) or len(chunk.content) != _STRESS_VALUE.size:
        raise AssertionError("stress response is not an exact FileContentChunk")
    echoed_token, server_id, connection_id, attempt_sequence = _STRESS_VALUE.unpack(chunk.content)
    return {
        "requested_token": token,
        "snapshot_token": chunk.offset,
        "echoed_token": echoed_token,
        "server_id": server_id,
        "connection_id": connection_id,
        "attempt_sequence": attempt_sequence,
    }


def _unique_completion_summary(values: list[dict[str, int]], ledger: StressLedger) -> dict[str, int]:
    echoed = [item["echoed_token"] for item in values]
    response_identities = {
        (item["echoed_token"], item["server_id"], item["connection_id"], item["attempt_sequence"])
        for item in values
    }
    expected_tokens = {item["requested_token"] for item in values}
    returned_tokens = set(echoed)
    cross_request = sum(
        item["echoed_token"] != item["requested_token"] or item["snapshot_token"] != item["requested_token"]
        for item in values
    )
    cross_generation = 0
    for item in values:
        expected = ledger.expected_provenance(item["requested_token"])
        if expected != (item["server_id"], item["connection_id"], item["attempt_sequence"]):
            cross_generation += 1
    return {
        "unique_responses": len(response_identities),
        "duplicate_responses": len(values) - len(response_identities),
        "missing_responses": len(expected_tokens - returned_tokens),
        "unexpected_responses": len(returned_tokens - expected_tokens),
        "cross_request_completions": cross_request,
        "cross_generation_completions": cross_generation,
    }


def _capture_runtime_resources(runtime: Any) -> tuple[Any, Any, Any, Any, Any]:
    with runtime.control_lock:
        generation = runtime.generation
        return (
            runtime.selector,
            runtime.wake_reader,
            runtime.wake_writer,
            generation,
            generation.sock if generation is not None else None,
        )


def _socket_closed(sock: Any) -> bool:
    return sock is not None and sock.fileno() == -1


def _selector_closed(selector: Any) -> bool:
    if selector is None:
        return True
    try:
        return selector.get_map() is None
    except (OSError, RuntimeError, ValueError):
        return True


def _runtime_cleanup_snapshot(runtime: Any, owned: tuple[Any, Any, Any, Any, Any]) -> dict[str, Any]:
    selector, wake_reader, wake_writer, generation, tcp_socket = owned
    with runtime.control_lock:
        thread = runtime.actor_thread
        snapshot = {
            "state": runtime.state.name,
            "actor_alive": thread is not None and thread.is_alive(),
            "generation_present": runtime.generation is not None,
            "selector_present": runtime.selector is not None,
            "wake_reader_present": runtime.wake_reader is not None,
            "wake_writer_present": runtime.wake_writer is not None,
            "pending_ticket_present": runtime.pending_task is not None,
            "active_ticket_present": runtime.active_task is not None,
            "cancel_count": len(runtime.cancel_requests),
        }
    snapshot.update(
        saved_selector_present=selector is not None,
        saved_wake_reader_present=wake_reader is not None,
        saved_wake_writer_present=wake_writer is not None,
        saved_generation_present=generation is not None,
        saved_tcp_present=tcp_socket is not None,
        saved_selector_closed=_selector_closed(selector),
        saved_wake_reader_closed=_socket_closed(wake_reader),
        saved_wake_writer_closed=_socket_closed(wake_writer),
        saved_tcp_closed=_socket_closed(tcp_socket),
    )
    snapshot["all_owned_resources_closed"] = (
        not snapshot["actor_alive"]
        and not snapshot["generation_present"]
        and not snapshot["selector_present"]
        and not snapshot["wake_reader_present"]
        and not snapshot["wake_writer_present"]
        and not snapshot["pending_ticket_present"]
        and not snapshot["active_ticket_present"]
        and snapshot["cancel_count"] == 0
        and snapshot["saved_selector_present"]
        and snapshot["saved_wake_reader_present"]
        and snapshot["saved_wake_writer_present"]
        and snapshot["saved_generation_present"]
        and snapshot["saved_tcp_present"]
        and snapshot["saved_selector_closed"]
        and snapshot["saved_wake_reader_closed"]
        and snapshot["saved_wake_writer_closed"]
        and snapshot["saved_tcp_closed"]
    )
    return snapshot


def run_generation_stress(count: int) -> dict[str, Any]:
    before_threads = _actor_threads()
    ledger = StressLedger()
    with (
        StressServer(
            server_id=1,
            ledger=ledger,
            close_every=0,
            keep_open_token=count - 1,
            poison_every=31,
            drop_before_response_every=2,
        ) as first,
        StressServer(
            server_id=2,
            ledger=ledger,
            close_every=1,
            keep_open_token=count - 1,
            poison_every=29,
        ) as second,
    ):
        transport = SocketTransport(hosts=[first.host, second.host], timeout=5, heartbeat_interval=None)
        runtime_ref = None
        thread_ref = None
        closed = False
        try:
            started = time.perf_counter()
            values = []
            for token in range(count):
                values.append(_execute_unique(transport, token))
                runtime = transport._runtime
                if runtime is None or runtime.actor_thread is None:
                    raise AssertionError("Actor runtime disappeared")
                if runtime_ref is None:
                    runtime_ref = runtime
                    thread_ref = runtime.actor_thread
                elif runtime is not runtime_ref or runtime.actor_thread is not thread_ref:
                    raise AssertionError("Actor object identity changed across generations")
            elapsed = time.perf_counter() - started
            transport.connect()
            diagnostics = transport.diagnostics
            generation_counter = diagnostics.actor.tcp_generation if diagnostics.actor is not None else 0
            stale_events = diagnostics.actor.stale_event_count if diagnostics.actor is not None else -1
            if runtime_ref is None or thread_ref is None:
                raise AssertionError("generation stress did not create an Actor")
            owned = _capture_runtime_resources(runtime_ref)
            transport.close()
            closed = True
            cleanup = _runtime_cleanup_snapshot(runtime_ref, owned)
            result = {
                "requests": count,
                "seconds": round(elapsed, 6),
                "throughput_rps": round(count / elapsed, 3),
                "runtime_identity": id(runtime_ref),
                "actor_object_identity": id(thread_ref),
                "actor_thread_ident": thread_ref.ident,
                "generation_counter": generation_counter,
                "server_accepts": [first.accept_count, second.accept_count],
                "server_requests": [first.business_requests, second.business_requests],
                "servers_used": sum(item > 0 for item in (first.business_requests, second.business_requests)),
                "stale_events": stale_events,
                "ledger": ledger.snapshot(),
                "cleanup": cleanup,
                **_unique_completion_summary(values, ledger),
            }
        finally:
            if not closed:
                transport.close()
    del first, second
    result.update(
        actor_threads_before=len(before_threads),
        actor_threads_after=len(_actor_threads()),
    )
    return result


def run_mixed_stress(
    requests: int,
    *,
    pool_size: int = 4,
    concurrency: int = 100,
    push_every: int = 97,
    close_every: int = 1000,
    response_delay: float = 0.0005,
) -> dict[str, Any]:
    ledger = StressLedger()
    first_fail_every = max(2, close_every // 2) if close_every else 0
    with (
        StressServer(
            server_id=1,
            ledger=ledger,
            push_every=push_every,
            poison_every=push_every,
            close_every=close_every,
            fail_before_response_every=first_fail_every,
            response_delay=response_delay,
        ) as first,
        StressServer(
            server_id=2,
            ledger=ledger,
            push_every=push_every,
            poison_every=push_every + 2 if push_every else 0,
            close_every=close_every,
            response_delay=response_delay,
        ) as second,
    ):
        pool = PooledSocketTransport(
            hosts=[first.host, second.host],
            timeout=10,
            pool_size=pool_size,
            heartbeat_interval=None,
            max_pending_requests=max(concurrency * 2, 256),
            push_queue_size=1024,
            push_queue_bytes=8 * 1024 * 1024,
        )

        def execute(token: int) -> dict[str, int]:
            return _execute_unique(pool, token)

        closed = False
        try:
            started = time.perf_counter()
            with ThreadPoolExecutor(max_workers=concurrency) as executor:
                values = list(executor.map(execute, range(requests)))
            elapsed = time.perf_counter() - started
            del executor
            gap_reported = False
            try:
                pool.drain_pushes()
            except PushOverflowError:
                gap_reported = True
                pool.drain_pushes()
            pool.connect()
            diagnostics = pool.diagnostics
            actor_generations = [actor.tcp_generation for actor in diagnostics.actors]
            stale_events = sum(actor.stale_event_count for actor in diagnostics.actors)
            broker = pool._broker
            push_buffer = pool._push_buffer
            runtimes = [transport._runtime for transport in pool._transports]
            if broker is None or push_buffer is None or any(runtime is None for runtime in runtimes):
                raise AssertionError("mixed stress pool ownership disappeared")
            owned = [_capture_runtime_resources(runtime) for runtime in runtimes]
            pool.close()
            closed = True
            broker_after = broker.snapshot()
            push_after = push_buffer.snapshot()
            cleanup = [
                _runtime_cleanup_snapshot(runtime, resources)
                for runtime, resources in zip(runtimes, owned)
            ]
            result = {
                "requests": requests,
                "pool_size": pool_size,
                "concurrency": concurrency,
                "seconds": round(elapsed, 6),
                "throughput_rps": round(requests / elapsed, 3),
                "server_requests": [first.business_requests, second.business_requests],
                "server_accepts": [first.accept_count, second.accept_count],
                "servers_used": sum(item > 0 for item in (first.business_requests, second.business_requests)),
                "server_max_active": ledger.snapshot()["max_business_active"],
                "push_frames_sent": first.push_frames + second.push_frames,
                "push_dropped": diagnostics.push_dropped,
                "push_gap_reported": gap_reported,
                "actor_generations": actor_generations,
                "stale_events": stale_events,
                "ledger": ledger.snapshot(),
                "broker_after_close": {
                    "idle_slots": broker_after.idle_slots,
                    "waiters": broker_after.waiter_count,
                    "pin_waiters": broker_after.pin_waiter_count,
                    "leases": broker_after.active_leases,
                    "closed": broker_after.closed,
                },
                "push_after_close": {
                    "frames": push_after.frame_count,
                    "bytes": push_after.byte_count,
                    "configured_max_frames": push_buffer.max_frames,
                    "configured_max_bytes": push_buffer.max_bytes,
                    "max_frames_observed": push_after.max_frames_observed,
                    "max_bytes_observed": push_after.max_bytes_observed,
                    "dropped_total": push_after.dropped_total,
                    "gap_pending": push_after.gap_pending,
                    "closed": push_after.closed,
                },
                "cleanup": cleanup,
                **_unique_completion_summary(values, ledger),
            }
        finally:
            if not closed:
                pool.close()
    del first, second
    result["actor_threads_after"] = len(_actor_threads())
    return result


def run_warmed_resource_stress(
    *,
    warmup_rounds: int = 3,
    measured_rounds: int = 6,
    generations_per_round: int = 20,
) -> dict[str, Any]:
    if warmup_rounds < 1 or measured_rounds < 3 or generations_per_round < 1:
        raise ValueError("resource stress requires warmup >= 1, measured >= 3, and generations >= 1")
    rounds = []
    for index in range(warmup_rounds + measured_rounds):
        result = run_generation_stress(generations_per_round)
        rounds.append(
            {
                "round": index,
                "phase": "warmup" if index < warmup_rounds else "measured",
                "resources": _resource_after_gc(),
                "actor_threads": result["actor_threads_after"],
                "cross_request_completions": result["cross_request_completions"],
                "cross_generation_completions": result["cross_generation_completions"],
                "owned_resources_closed": result["cleanup"]["all_owned_resources_closed"],
            }
        )
    measured = rounds[warmup_rounds:]
    counts = [item["resources"] for item in measured]
    supported = all(value is not None for value in counts)
    plateau = supported and len(set(counts)) == 1
    monotonic_growth = supported and all(
        later >= earlier for earlier, later in zip(counts, counts[1:])
    ) and counts[-1] > counts[0]
    return {
        "warmup_rounds": warmup_rounds,
        "measured_rounds": measured_rounds,
        "generations_per_round": generations_per_round,
        "samples": rounds,
        "measured_resources": counts,
        "resource_counter_supported": supported,
        "exact_plateau": plateau,
        "monotonic_growth": monotonic_growth,
        "all_actor_threads_closed": all(item["actor_threads"] == 0 for item in measured),
        "all_owned_resources_closed": all(item["owned_resources_closed"] for item in measured),
        "cross_request_completions": sum(item["cross_request_completions"] for item in measured),
        "cross_generation_completions": sum(item["cross_generation_completions"] for item in measured),
    }


def run_close_samples(samples: int, *, pool_size: int = 4, loaded: bool = False) -> dict[str, Any]:
    latencies: list[float] = []
    sample_evidence: list[dict[str, Any]] = []
    completed_results = 0
    expected_errors: dict[str, int] = {}
    with StressServer(response_delay=0.05 if loaded else 0.0) as server:
        handled = 0
        for _ in range(samples):
            pool = PooledSocketTransport(hosts=[server.host], timeout=2, pool_size=pool_size, heartbeat_interval=None)
            pool.connect()
            runtimes = [transport._runtime for transport in pool._transports]
            if any(runtime is None for runtime in runtimes):
                raise AssertionError("close sample pool lost an Actor runtime")
            owned = [_capture_runtime_resources(runtime) for runtime in runtimes]
            futures = []
            executor = None
            if loaded:
                executor = ThreadPoolExecutor(max_workers=pool_size)
                futures = [executor.submit(pool.execute, TYPE_SECURITY_COUNT, {"market": "sz"}) for _ in range(pool_size)]
                handled += pool_size
                if not server.wait_for_business(handled, timeout=2):
                    raise AssertionError("loaded close requests did not reach server")
            started = time.perf_counter()
            pool.close()
            latencies.append((time.perf_counter() - started) * 1000)
            cleanup = [
                _runtime_cleanup_snapshot(runtime, resources)
                for runtime, resources in zip(runtimes, owned)
            ]
            if not all(item["all_owned_resources_closed"] for item in cleanup):
                raise AssertionError("close sample retained Actor-owned resources")
            settle_started = time.perf_counter()
            if executor is not None:
                for future in futures:
                    try:
                        future.result(timeout=2)
                    except FutureTimeoutError as exc:
                        raise AssertionError("caller future did not settle after loaded close") from exc
                    except TransportError as exc:
                        name = type(exc).__name__
                        expected_errors[name] = expected_errors.get(name, 0) + 1
                    else:
                        completed_results += 1
                executor.shutdown()
            settle_ms = (time.perf_counter() - settle_started) * 1000
            sample_evidence.append(
                {
                    "futures": len(futures),
                    "futures_terminal_within_timeout": all(future.done() for future in futures),
                    "caller_settle_ms": round(settle_ms, 4),
                    "cleanup": cleanup,
                }
            )
    ordered = sorted(latencies)
    return {
        "samples": samples,
        "loaded": loaded,
        "p50_ms": round(statistics.median(ordered), 4),
        "p95_ms": round(_percentile(ordered, 0.95), 4),
        "p99_ms": round(_percentile(ordered, 0.99), 4),
        "max_ms": round(max(ordered), 4),
        "completed_results": completed_results,
        "expected_request_errors": expected_errors,
        "all_futures_terminal_within_timeout": all(
            item["futures_terminal_within_timeout"] for item in sample_evidence
        ),
        "max_caller_settle_ms": round(max(item["caller_settle_ms"] for item in sample_evidence), 4),
        "all_tickets_terminal_at_close": all(
            not cleanup["pending_ticket_present"] and not cleanup["active_ticket_present"]
            for item in sample_evidence
            for cleanup in item["cleanup"]
        ),
        "all_owned_resources_closed": all(
            cleanup["all_owned_resources_closed"]
            for item in sample_evidence
            for cleanup in item["cleanup"]
        ),
        "sample_evidence": sample_evidence,
    }


def _set_heartbeat_pool_interval(pool: PooledSocketTransport, interval: float | None) -> None:
    runtimes = []
    for transport in pool._transports:
        runtime = transport._runtime
        if runtime is None:
            raise AssertionError("heartbeat pool lost an Actor runtime")
        runtimes.append(runtime)
    with ExitStack() as stack:
        for runtime in runtimes:
            stack.enter_context(runtime.control_lock)
        now = time.monotonic()
        for runtime in runtimes:
            generation = runtime.generation
            if generation is not None:
                generation.last_activity_at = now
        for runtime in runtimes:
            runtime.heartbeat_interval = interval


def _heartbeat_runtime_is_quiescent(runtime: Any) -> bool:
    if runtime.active_task is not None or runtime.pending_task is not None or runtime.cancel_requests:
        return False
    generation = runtime.generation
    if generation is None:
        return False
    return (
        generation.state is TcpState.READY
        and generation.active_exchange is None
        and generation.tx_bytes == b""
        and generation.tx_offset == 0
        and not generation.decoded_frames
        and generation.decoder.buffered_bytes == 0
        and generation.receive_drained
    )


def _heartbeat_fence_snapshot(
    pool: PooledSocketTransport,
    pool_size: int,
) -> tuple[int, ...] | None:
    runtimes = []
    for transport in pool._transports:
        runtime = transport._runtime
        if runtime is None:
            raise AssertionError("heartbeat pool lost an Actor runtime")
        runtimes.append(runtime)
    if len(runtimes) != pool_size:
        raise AssertionError("heartbeat pool size changed during configuration")
    with ExitStack() as stack:
        for runtime in runtimes:
            stack.enter_context(runtime.control_lock)
        if not all(_heartbeat_runtime_is_quiescent(runtime) for runtime in runtimes):
            return None
        return tuple(runtime.generation.generation_id for runtime in runtimes)


def _synchronize_heartbeat_pool(
    pool: PooledSocketTransport,
    pool_size: int,
    interval: float | None,
    *,
    timeout: float = 2.0,
) -> int:
    _set_heartbeat_pool_interval(pool, None)
    # Every exact ConnectTicket runs on its owning Actor after any heartbeat
    # that raced with disable, so this is the phase's Actor-thread fence.
    pool.connect()
    deadline = time.monotonic() + timeout
    while True:
        generation_ids = _heartbeat_fence_snapshot(pool, pool_size)
        if generation_ids is not None:
            # Target publication holds all runtime locks and starts every
            # interval from one timestamp after the disabled Actor fence.
            _set_heartbeat_pool_interval(pool, interval)
            return len(generation_ids)
        if time.monotonic() >= deadline:
            raise AssertionError("heartbeat configuration did not reach wire quiescence")
        threading.Event().wait(0.001)


def _synchronize_disabled_heartbeat_pool(
    pool: PooledSocketTransport,
    pool_size: int,
    *,
    timeout: float = 2.0,
) -> int:
    return _synchronize_heartbeat_pool(pool, pool_size, None, timeout=timeout)


def _cleanup_heartbeat_pool(pool: PooledSocketTransport) -> BaseException | None:
    failures: list[tuple[str, BaseException]] = []
    try:
        _set_heartbeat_pool_interval(pool, None)
    except BaseException as exc:
        failures.append(("interval reset", exc))
    try:
        pool.close()
    except BaseException as exc:
        failures.append(("pool close", exc))
    if not failures:
        return None
    details = "; ".join(f"{stage}: {type(exc).__name__}: {exc}" for stage, exc in failures)
    cleanup_error = RuntimeError(f"heartbeat pool cleanup failed ({details})")
    cleanup_error.__cause__ = failures[0][1]
    return cleanup_error


def _raise_primary_with_cleanup(primary: BaseException, cleanup: BaseException) -> None:
    if cleanup is primary:
        _break_exception_chain_cycle(primary)
        raise primary
    previous = primary.__cause__
    if _exception_chain_contains(cleanup, primary):
        _detach_exception_chain_target(cleanup, primary)
    _break_exception_chain_cycle(cleanup)
    if (
        previous is not None
        and previous is not primary
        and not _exception_chain_contains(previous, primary)
        and _exception_chain_ids(cleanup).isdisjoint(_exception_chain_ids(previous))
    ):
        _break_exception_chain_cycle(previous)
        tail = cleanup
        seen = {id(tail)}
        while tail.__cause__ is not None and id(tail.__cause__) not in seen:
            tail = tail.__cause__
            seen.add(id(tail))
        if tail.__cause__ is None:
            tail.__cause__ = previous
    raise primary from cleanup


def _exception_chain_contains(error: BaseException, target: BaseException) -> bool:
    seen = set()
    current: BaseException | None = error
    while current is not None and id(current) not in seen:
        if current is target:
            return True
        seen.add(id(current))
        current = current.__cause__
    return False


def _exception_chain_ids(error: BaseException) -> set[int]:
    seen: set[int] = set()
    current: BaseException | None = error
    while current is not None and id(current) not in seen:
        seen.add(id(current))
        current = current.__cause__
    return seen


def _break_exception_chain_cycle(error: BaseException) -> None:
    seen: set[int] = set()
    current: BaseException | None = error
    while current is not None:
        seen.add(id(current))
        following = current.__cause__
        if following is None:
            return
        if id(following) in seen:
            current.__cause__ = None
            return
        current = following


def _detach_exception_chain_target(error: BaseException, target: BaseException) -> None:
    seen = set()
    current: BaseException | None = error
    while current is not None and id(current) not in seen:
        seen.add(id(current))
        if current.__cause__ is target:
            current.__cause__ = None
            return
        current = current.__cause__


def _publish_heartbeat_phase(
    pool: PooledSocketTransport,
    server: Any,
    interval: float | None,
) -> int:
    heartbeat_before = server.heartbeat_requests
    _set_heartbeat_pool_interval(pool, interval)
    return heartbeat_before


def run_heartbeat_impact(requests: int, *, blocks: int = 4) -> dict[str, Any]:
    if requests < 100 or blocks < 3:
        raise ValueError("heartbeat impact requires at least 100 requests and three balanced blocks")

    pool_size = 4
    concurrency = pool_size + 1
    heartbeat_interval = 0.02

    next_token = 0
    all_values: list[dict[str, int]] = []

    def run_workers(
        executor: ThreadPoolExecutor,
        pool: PooledSocketTransport,
        total: int,
        *,
        before_release: Any = None,
    ) -> float:
        nonlocal next_token
        tokens = list(range(next_token, next_token + total))
        next_token += total
        quotient, remainder = divmod(total, concurrency)
        token_groups = []
        offset = 0
        for index in range(concurrency):
            count = quotient + int(index < remainder)
            token_groups.append(tokens[offset : offset + count])
            offset += count
        started_at: list[float] = []

        def release_workers() -> None:
            if before_release is not None:
                before_release()
            started_at.append(time.perf_counter())

        barrier = threading.Barrier(concurrency + 1, action=release_workers)

        def worker(worker_tokens: list[int]) -> list[dict[str, int]]:
            barrier.wait()
            return [_execute_unique(pool, token) for token in worker_tokens]

        futures = [executor.submit(worker, worker_tokens) for worker_tokens in token_groups]
        barrier.wait()
        values = [value for future in futures for value in future.result()]
        if len(started_at) != 1:
            raise AssertionError("heartbeat workload did not publish one launch boundary")
        elapsed = time.perf_counter() - started_at[0]
        if len(values) != total:
            raise AssertionError("heartbeat workload response mismatch")
        all_values.extend(values)
        return elapsed

    samples: dict[str, list[dict[str, Any]]] = {"without_heartbeat": [], "with_heartbeat": []}
    raw_elapsed: dict[str, list[float]] = {"without_heartbeat": [], "with_heartbeat": []}
    block_ratios = []
    phase_schedule: list[dict[str, Any]] = []
    configuration_slot_barriers = 0
    ledger = StressLedger()
    with StressServer(server_id=1, ledger=ledger, response_delay=0.005) as server:
        pool = PooledSocketTransport(
            hosts=[server.host],
            timeout=10,
            pool_size=pool_size,
            heartbeat_interval=heartbeat_interval,
            max_pending_requests=256,
        )
        idle_heartbeat_before = server.heartbeat_responses
        primary_error: BaseException | None = None
        try:
            pool.connect()
            if not server.wait_for_heartbeat_response_connections(pool_size, timeout=2.0):
                raise AssertionError("automatic heartbeat did not reach every idle Actor connection")
            idle_probe_heartbeats = server.heartbeat_responses - idle_heartbeat_before
            idle_probe_connections = len(server.heartbeat_responses_by_connection)
            warmup_count = min(100, requests)
            with ThreadPoolExecutor(max_workers=concurrency) as executor:
                paced_heartbeat_before = server.heartbeat_responses
                paced_business_requests = 0
                for _ in range(8):
                    heartbeat_baseline = server.heartbeat_response_snapshot()
                    if not server.wait_for_heartbeat_response_round(heartbeat_baseline, timeout=2.0):
                        raise AssertionError("paced automatic heartbeat round did not complete")
                    run_workers(executor, pool, pool_size)
                    paced_business_requests += pool_size
                paced_heartbeat_requests = server.heartbeat_responses - paced_heartbeat_before
                configuration_slot_barriers += _synchronize_disabled_heartbeat_pool(pool, pool_size)
                base_order = (
                    None,
                    heartbeat_interval,
                    heartbeat_interval,
                    None,
                    heartbeat_interval,
                    None,
                    None,
                    heartbeat_interval,
                )
                for block in range(blocks):
                    order = base_order if block % 2 == 0 else tuple(reversed(base_order))
                    block_elapsed = {"without_heartbeat": [], "with_heartbeat": []}
                    for phase, interval in enumerate(order):
                        phase_id = f"block-{block}-phase-{phase}"
                        try:
                            _set_heartbeat_pool_interval(pool, None)
                            gc.collect()
                            _set_heartbeat_pool_interval(pool, interval)
                            run_workers(executor, pool, warmup_count)
                            configuration_slot_barriers += _synchronize_disabled_heartbeat_pool(pool, pool_size)
                            generation_ids_before = _heartbeat_fence_snapshot(pool, pool_size)
                            if generation_ids_before is None:
                                raise AssertionError("heartbeat phase lost its configured generation fence")
                            accept_count_before = server.accept_count
                            gc_enabled = gc.isenabled()
                            gc.disable()
                            phase_error: BaseException | None = None
                            barrier_error: BaseException | None = None
                            try:
                                heartbeat_baseline: list[int] = []

                                def start_timed_phase() -> None:
                                    server.begin_heartbeat_business_phase(
                                        phase_id,
                                        start_business_requests=server.business_requests,
                                        target_business_requests=requests,
                                    )
                                    heartbeat_baseline.append(
                                        _publish_heartbeat_phase(pool, server, interval)
                                    )

                                elapsed = run_workers(
                                    executor,
                                    pool,
                                    requests,
                                    before_release=start_timed_phase,
                                )
                                if len(heartbeat_baseline) != 1:
                                    raise AssertionError("heartbeat phase did not publish one counter baseline")
                                heartbeat_before = heartbeat_baseline[0]
                                business_phase = server.heartbeat_business_phase_snapshot(
                                    phase_id
                                )
                                if (
                                    business_phase["business_requests_started"] != requests
                                    or business_phase["business_responses_sent"] != requests
                                    or business_phase["business_window_open"]
                                ):
                                    raise AssertionError(
                                        "heartbeat business phase did not close at its final response: "
                                        f"{business_phase!r}"
                                    )
                            except BaseException as exc:
                                phase_error = exc
                            finally:
                                try:
                                    configuration_slot_barriers += _synchronize_disabled_heartbeat_pool(
                                        pool, pool_size
                                    )
                                    generation_ids_after = _heartbeat_fence_snapshot(pool, pool_size)
                                    if generation_ids_after is None:
                                        raise AssertionError("heartbeat phase lost its terminal generation fence")
                                    accept_count_after = server.accept_count
                                except BaseException as exc:
                                    barrier_error = exc
                                finally:
                                    if gc_enabled:
                                        gc.enable()
                            if phase_error is not None:
                                if barrier_error is not None:
                                    _raise_primary_with_cleanup(phase_error, barrier_error)
                                raise phase_error
                            if barrier_error is not None:
                                raise barrier_error
                            if generation_ids_after != generation_ids_before:
                                raise AssertionError("heartbeat phase changed Actor TCP generation")
                            if accept_count_after != accept_count_before:
                                raise AssertionError("heartbeat phase opened a replacement TCP connection")
                            key = "without_heartbeat" if interval is None else "with_heartbeat"
                            sample = {
                                "block": block,
                                "phase": phase,
                                "phase_id": phase_id,
                                "seconds": round(elapsed, 6),
                                "throughput_rps": round(requests / elapsed, 3),
                                "heartbeat_requests": server.heartbeat_requests - heartbeat_before,
                                "business_window_heartbeat_requests": business_phase[
                                    "heartbeat_requests"
                                ],
                                "start_business_requests": business_phase[
                                    "start_business_requests"
                                ],
                                "target_business_requests": business_phase[
                                    "target_business_requests"
                                ],
                                "generation_ids_before": list(generation_ids_before),
                                "generation_ids_after": list(generation_ids_after),
                                "accept_count_before": accept_count_before,
                                "accept_count_after": accept_count_after,
                                "launch_boundary": "worker barrier action: phase, counter, interval, timer, release",
                            }
                            phase_schedule.append(
                                {
                                    "block": block,
                                    "phase": phase,
                                    "phase_id": phase_id,
                                    "condition": key,
                                }
                            )
                            samples[key].append(sample)
                            raw_elapsed[key].append(elapsed)
                            block_elapsed[key].append(elapsed)
                        except BaseException as exc:
                            try:
                                _set_heartbeat_pool_interval(pool, None)
                            except BaseException as reset_error:
                                _raise_primary_with_cleanup(exc, reset_error)
                            raise
                        else:
                            _set_heartbeat_pool_interval(pool, None)
                    block_ratios.append(
                        round(
                            sum(block_elapsed["without_heartbeat"]) / sum(block_elapsed["with_heartbeat"]),
                            6,
                        )
                    )
        except BaseException as exc:
            primary_error = exc
        cleanup_error = _cleanup_heartbeat_pool(pool)
        if primary_error is not None:
            if cleanup_error is not None:
                _raise_primary_with_cleanup(primary_error, cleanup_error)
            raise primary_error
        if cleanup_error is not None:
            raise cleanup_error

    def summarize(values: list[dict[str, Any]]) -> dict[str, Any]:
        return {
            "seconds": round(statistics.median(item["seconds"] for item in values), 6),
            "throughput_rps": round(statistics.median(item["throughput_rps"] for item in values), 3),
            "heartbeat_requests": max(item["heartbeat_requests"] for item in values),
            "heartbeat_requests_total": sum(item["heartbeat_requests"] for item in values),
            "business_window_heartbeat_requests": max(
                item["business_window_heartbeat_requests"] for item in values
            ),
            "business_window_heartbeat_requests_total": sum(
                item["business_window_heartbeat_requests"] for item in values
            ),
            "samples": values,
        }

    baseline = summarize(samples["without_heartbeat"])
    active = summarize(samples["with_heartbeat"])
    completion_summary = _unique_completion_summary(all_values, ledger)
    return {
        "requests_per_phase": requests,
        "blocks": blocks,
        "phases": blocks * 8,
        "pool_size": pool_size,
        "concurrency": concurrency,
        "response_delay_ms": 5.0,
        "heartbeat_interval_ms": heartbeat_interval * 1000,
        "idle_probe_heartbeats": idle_probe_heartbeats,
        "idle_probe_connections": idle_probe_connections,
        "paced_heartbeat_requests": paced_heartbeat_requests,
        "paced_business_requests": paced_business_requests,
        "configuration_slot_barriers": configuration_slot_barriers,
        "configuration_quiescence": "disabled all-slot Actor ConnectTicket fence plus per-phase TCP generation identity",
        "phase_schedule": phase_schedule,
        "heartbeat_during_business": server.heartbeat_during_business,
        "business_requests": len(all_values),
        "without_heartbeat": baseline,
        "with_heartbeat": active,
        "block_throughput_ratios": block_ratios,
        "median_block_throughput_ratio": round(statistics.median(block_ratios), 6),
        "throughput_estimator": "aggregate_elapsed_ratio",
        "throughput_ratio": round(
            sum(raw_elapsed["without_heartbeat"]) / sum(raw_elapsed["with_heartbeat"]),
            6,
        ),
        **completion_summary,
    }


def run_idle_cpu_sample(seconds: float = 0.5) -> dict[str, Any]:
    transport = SocketTransport(hosts=["127.0.0.1:9"], timeout=1, heartbeat_interval=None)
    transport._ensure_runtime()
    started_cpu = time.process_time()
    started = time.perf_counter()
    threading.Event().wait(seconds)
    elapsed = time.perf_counter() - started
    cpu = time.process_time() - started_cpu
    transport.close()
    return {"seconds": round(elapsed, 6), "process_cpu_seconds": round(cpu, 6), "cpu_ratio": round(cpu / elapsed, 6)}


def _read_exact(conn: socket.socket, size: int) -> bytes:
    output = bytearray()
    while len(output) < size:
        chunk = conn.recv(size - len(output))
        if not chunk:
            raise EOFError
        output.extend(chunk)
    return bytes(output)


def _read_request(conn: socket.socket) -> tuple[int, int, bytes]:
    header = _read_exact(conn, 12)
    length = int.from_bytes(header[6:8], "little")
    return int.from_bytes(header[1:5], "little"), int.from_bytes(header[10:12], "little"), _read_exact(conn, length - 2)


def _response(msg_id: int, msg_type: int, payload: bytes) -> bytes:
    return b"\xb1\xcb\x74\x00\x00" + msg_id.to_bytes(4, "little") + b"\x00" + msg_type.to_bytes(2, "little") + len(payload).to_bytes(2, "little") * 2 + payload


def _handshake_payload() -> bytes:
    payload = bytearray(189)
    payload[1:3] = (2026).to_bytes(2, "little")
    payload[3:9] = bytes((27, 5, 30, 10, 0, 0))
    payload[42:46] = (20260527).to_bytes(4, "little")
    payload[50:54] = (20260527).to_bytes(4, "little")
    return bytes(payload)


def _actor_threads() -> list[threading.Thread]:
    return [thread for thread in threading.enumerate() if thread.name.startswith("eltdx-7709-actor-")]


def _resource_count() -> int | None:
    if os.name == "nt":
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        get_count = kernel32.GetProcessHandleCount
        get_count.argtypes = (wintypes.HANDLE, ctypes.POINTER(wintypes.DWORD))
        get_count.restype = wintypes.BOOL
        count = wintypes.DWORD()
        if get_count(kernel32.GetCurrentProcess(), ctypes.byref(count)):
            return int(count.value)
        return None
    # Linux exposes descriptors through /proc; macOS exposes the same view at /dev/fd.
    for fd_path in (Path("/proc/self/fd"), Path("/dev/fd")):
        if fd_path.exists():
            try:
                return len(tuple(fd_path.iterdir()))
            except OSError:
                continue
    return None


def _resource_after_gc() -> int | None:
    gc.collect()
    return _resource_count()


def _percentile(values: list[float], fraction: float) -> float:
    return values[min(len(values) - 1, int((len(values) - 1) * fraction))]


def _git_identity() -> dict[str, Any]:
    try:
        sha = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        dirty = bool(
            subprocess.run(
                ["git", "status", "--porcelain"],
                cwd=ROOT,
                check=True,
                capture_output=True,
                text=True,
            ).stdout.strip()
        )
    except (OSError, subprocess.CalledProcessError):
        return {"implementation_sha": None, "worktree_dirty": None}
    return {"implementation_sha": sha, "worktree_dirty": dirty}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--generations", type=int, default=0)
    parser.add_argument("--requests", type=int, default=0)
    parser.add_argument("--pool-size", type=int, default=4)
    parser.add_argument("--concurrency", type=int, default=100)
    parser.add_argument("--close-samples", type=int, default=0)
    parser.add_argument("--heartbeat-requests", type=int, default=0)
    parser.add_argument("--idle-seconds", type=float, default=0.0)
    parser.add_argument("--resource-rounds", type=int, default=0)
    parser.add_argument("--resource-warmup", type=int, default=3)
    parser.add_argument("--resource-generations", type=int, default=20)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    source_hash = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    result: dict[str, Any] = {
        "schema": 2,
        "workload_sha256": source_hash,
        "platform": platform.platform(),
        "python": platform.python_version(),
        **_git_identity(),
    }
    if args.generations:
        result["generation_stress"] = run_generation_stress(args.generations)
    if args.requests:
        result["mixed_stress"] = run_mixed_stress(
            args.requests, pool_size=args.pool_size, concurrency=args.concurrency
        )
    if args.close_samples:
        result["idle_close"] = run_close_samples(args.close_samples, pool_size=args.pool_size)
        result["loaded_close"] = run_close_samples(args.close_samples, pool_size=args.pool_size, loaded=True)
    if args.heartbeat_requests:
        result["heartbeat_impact"] = run_heartbeat_impact(args.heartbeat_requests)
    if args.idle_seconds:
        result["idle_cpu"] = run_idle_cpu_sample(args.idle_seconds)
    if args.resource_rounds:
        result["warmed_resources"] = run_warmed_resource_stress(
            warmup_rounds=args.resource_warmup,
            measured_rounds=args.resource_rounds,
            generations_per_round=args.resource_generations,
        )
    encoded = json.dumps(result, ensure_ascii=True, indent=2) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(encoded, encoding="utf-8")
    print(encoded, end="")


if __name__ == "__main__":
    main()
