"""Rust-owned bounded pool transport facade."""

from __future__ import annotations

from collections.abc import Iterator, Sequence
from contextlib import contextmanager
from dataclasses import dataclass
from enum import Enum, auto
from threading import Lock
from typing import Any

from eltdx._native_models import response_from_dto
from eltdx.exceptions import ConnectionClosedError
from eltdx.hosts import (
    DEFAULT_HOSTS,
    DEFAULT_PROBE_HOSTS,
    DEFAULT_PROBE_TIMEOUT,
    DEFAULT_PROBE_WORKERS,
    rank_hosts_from_cache,
    sort_hosts_by_latency,
    unique_hosts,
)
from eltdx.transport.native import call_native, native_module

from .actor import ActorSnapshot, RuntimeState, TcpState
from ._config import (
    DEFAULT_POOL_SIZE,
    DEFAULT_PUSH_QUEUE_BYTES,
    DEFAULT_SERVER_COUNT,
    SLOT_DECODED_MAX_BYTES,
    SLOT_RAW_MAX_BYTES,
    automatic_decoded_bytes as _automatic_decoded_bytes,
    automatic_raw_bytes as _automatic_raw_bytes,
    available_parallelism as _available_parallelism,
    optional_positive_int,
    positive_int,
    resolve_pool_layout,
)
from .socket import (
    DEFAULT_HEARTBEAT_INTERVAL,
    DEFAULT_PUSH_QUEUE_SIZE,
    _push_value,
    _resolved_native_hosts,
)

DEFAULT_MAX_PENDING_REQUESTS = 256


def validate_pool_size(value: int) -> int:
    return positive_int("pool_size", value)


class PoolState(Enum):
    STOPPED = auto()
    STARTING = auto()
    RUNNING = auto()
    CLOSING = auto()
    FAILED = auto()
    FAILED_CLOSING = auto()
    FAILED_CLOSED = auto()


@dataclass(frozen=True, slots=True)
class BrokerSnapshot:
    pool_epoch: int
    idle_slots: int
    waiter_count: int
    pin_waiter_count: int
    active_leases: int
    closed: bool


@dataclass(frozen=True, slots=True)
class PoolDiagnostics:
    epoch: int
    state: PoolState
    broker: BrokerSnapshot | None
    actors: tuple[ActorSnapshot, ...]
    push_frames: int
    push_bytes: int
    push_dropped: int
    runtime_workers: int = 0
    pool_size: int = 0
    server_count: int = 0
    max_connections_per_host: int = 0
    connect_concurrency: int = 0
    connect_concurrency_per_host: int = 0
    raw_bytes: int = 0
    raw_max_bytes: int = 0
    raw_peak_bytes: int = 0
    decoded_bytes: int = 0
    decoded_max_bytes: int = 0
    decoded_peak_bytes: int = 0
    push_max_bytes: int = 0


def _pool_diagnostics(dto: tuple[Any, ...]) -> PoolDiagnostics:
    broker = None if dto[2] is None else BrokerSnapshot(*dto[2])
    actors = tuple(
        ActorSnapshot(
            runtime_epoch=item[0],
            state=RuntimeState[item[1]],
            tcp_state=TcpState[item[2]],
            tcp_generation=item[3],
            connected_host=item[4],
            actor_alive=item[5],
            pending_depth=item[6],
            reconnect_count=item[7],
            stale_event_count=item[8],
            last_error=item[9],
        )
        for item in dto[3]
    )
    return PoolDiagnostics(
        dto[0],
        PoolState[dto[1]],
        broker,
        actors,
        dto[4],
        dto[5],
        dto[6],
        dto[7],
        dto[8],
        dto[9],
        dto[10],
        dto[11],
        dto[12],
        dto[13],
        dto[14],
        dto[15],
        dto[16],
        dto[17],
        dto[18],
        dto[19],
    )


class PinnedTransportProxy:
    """Epoch-bound view that does not own the shared Engine lifecycle."""

    def __init__(self, pool: PooledSocketTransport, native_pin: Any, epoch: int) -> None:
        self._pool = pool
        self._native_pin = native_pin
        self._epoch = epoch
        self._closed = False

    def _require_open(self) -> PooledSocketTransport:
        if self._closed or self._pool.diagnostics.epoch != self._epoch:
            raise ConnectionClosedError("pinned proxy is no longer valid")
        return self._pool

    @property
    def connected_host(self) -> str | None:
        self._require_open()
        return self._native_pin.connected_host

    @property
    def last_handshake(self) -> Any:
        return self._session_value(0)

    @property
    def last_heartbeat(self) -> Any:
        return self._session_value(1)

    @property
    def pending_push_count(self) -> int:
        return self._require_open().pending_push_count

    def _session_value(self, index: int) -> Any:
        self._require_open()
        dto = call_native(self._native_pin.session_snapshot)[index]
        return None if dto is None else response_from_dto(dto)

    def connect(self) -> None:
        self._require_open()

    def close(self) -> None:
        if self._closed:
            return
        diagnostics = self._pool.diagnostics
        if diagnostics.epoch == self._epoch and diagnostics.state is PoolState.RUNNING:
            call_native(self._native_pin.close)
        self._closed = True

    def execute(self, command: int, payload: dict[str, Any] | None = None) -> Any:
        self._require_open()
        dto = call_native(self._native_pin.execute, command, payload or {})
        return response_from_dto(dto)

    def request(self, command: str) -> str:
        return self._require_open().request(command)

    def poll_push(self, timeout: float | None = 0.0, *, parse: bool = False) -> Any:
        return self._require_open().poll_push(timeout, parse=parse)

    def drain_pushes(self, *, parse: bool = False) -> list[Any]:
        return self._require_open().drain_pushes(parse=parse)


class PooledSocketTransport:
    def __init__(
        self,
        hosts: Sequence[str] | None = None,
        *,
        timeout: float = 8.0,
        pool_size: int | None = None,
        server_count: int = DEFAULT_SERVER_COUNT,
        connections_per_server: int | None = None,
        runtime_workers: int | None = None,
        max_connections_per_host: int | None = None,
        connect_concurrency: int | None = None,
        connect_concurrency_per_host: int | None = None,
        global_raw_bytes: int | None = None,
        global_decoded_bytes: int | None = None,
        probe_hosts: bool = DEFAULT_PROBE_HOSTS,
        probe_timeout: float = DEFAULT_PROBE_TIMEOUT,
        probe_workers: int = DEFAULT_PROBE_WORKERS,
        heartbeat_interval: float | None = DEFAULT_HEARTBEAT_INTERVAL,
        max_pending_requests: int = DEFAULT_MAX_PENDING_REQUESTS,
        push_queue_size: int = DEFAULT_PUSH_QUEUE_SIZE,
        push_queue_bytes: int = DEFAULT_PUSH_QUEUE_BYTES,
    ) -> None:
        values = rank_hosts_from_cache(unique_hosts(list(hosts or DEFAULT_HOSTS)))
        if not values:
            raise ValueError("at least one host is required")
        if timeout <= 0:
            raise ValueError("timeout must be > 0")
        max_pending_requests = positive_int(
            "max_pending_requests", max_pending_requests
        )
        push_queue_size = positive_int("push_queue_size", push_queue_size)
        push_queue_bytes = positive_int("push_queue_bytes", push_queue_bytes)
        if probe_timeout <= 0:
            raise ValueError("probe_timeout must be > 0")
        probe_workers = positive_int("probe_workers", probe_workers)
        layout = resolve_pool_layout(
            host_count=len(values),
            pool_size=pool_size,
            server_count=server_count,
            connections_per_server=connections_per_server,
            max_connections_per_host=max_connections_per_host,
        )
        self._hosts = values
        self._timeout = float(timeout)
        self._pool_size = layout.pool_size
        self._server_count = layout.server_count
        self._connections_per_server = layout.connections_per_server
        self._runtime_workers = optional_positive_int("runtime_workers", runtime_workers)
        if self._runtime_workers is not None and self._runtime_workers > self._pool_size:
            raise ValueError("runtime_workers cannot exceed pool_size")
        self._max_connections_per_host = layout.max_connections_per_host
        self._connect_concurrency = optional_positive_int(
            "connect_concurrency", connect_concurrency
        )
        if (
            self._connect_concurrency is not None
            and self._connect_concurrency > self._pool_size
        ):
            raise ValueError("connect_concurrency cannot exceed pool_size")
        self._connect_concurrency_per_host = optional_positive_int(
            "connect_concurrency_per_host", connect_concurrency_per_host
        )
        effective_connect_concurrency = self._connect_concurrency or min(
            self._pool_size,
            max(4, min(_available_parallelism() * 2, 32)),
        )
        if self._connect_concurrency_per_host is not None and (
            self._connect_concurrency_per_host > effective_connect_concurrency
            or self._connect_concurrency_per_host > self._max_connections_per_host
        ):
            raise ValueError(
                "connect_concurrency_per_host cannot exceed connect_concurrency "
                "or max_connections_per_host"
            )
        self._global_raw_bytes = optional_positive_int("global_raw_bytes", global_raw_bytes)
        if self._global_raw_bytes is not None and self._global_raw_bytes < SLOT_RAW_MAX_BYTES:
            raise ValueError(
                "global_raw_bytes must allow at least one complete Slot staging buffer"
            )
        self._global_decoded_bytes = optional_positive_int(
            "global_decoded_bytes", global_decoded_bytes
        )
        if (
            self._global_decoded_bytes is not None
            and self._global_decoded_bytes < SLOT_DECODED_MAX_BYTES
        ):
            raise ValueError(
                "global_decoded_bytes must allow at least one complete Slot decoded queue"
            )
        self._heartbeat_interval = heartbeat_interval
        self._max_pending_requests = max_pending_requests
        self._push_queue_size = push_queue_size
        self._push_queue_bytes = push_queue_bytes
        self._probe_hosts = bool(probe_hosts)
        self._probe_timeout = float(probe_timeout)
        self._probe_workers = probe_workers
        self._hosts_probed = not self._probe_hosts or len(values) <= 1
        self._engine_lock = Lock()
        self._engine: Any = None

    def _native(self) -> Any:
        if self._engine is not None:
            return self._engine
        with self._engine_lock:
            if self._engine is not None:
                return self._engine
            if not self._hosts_probed:
                self._hosts = sort_hosts_by_latency(
                    self._hosts,
                    timeout=self._probe_timeout,
                    max_workers=self._probe_workers,
                )
                self._hosts_probed = True
            self._engine = call_native(
                native_module().NativeEngine,
                _resolved_native_hosts(self._hosts),
                timeout=self._timeout,
                pool_size=self._pool_size,
                runtime_workers=self._runtime_workers,
                server_count=self._server_count,
                max_connections_per_host=self._max_connections_per_host,
                connect_concurrency=self._connect_concurrency,
                connect_concurrency_per_host=self._connect_concurrency_per_host,
                global_raw_bytes=self._global_raw_bytes,
                global_decoded_bytes=self._global_decoded_bytes,
                heartbeat_interval=self._heartbeat_interval,
                max_pending_requests=self._max_pending_requests,
                push_queue_size=self._push_queue_size,
                push_queue_bytes=self._push_queue_bytes,
            )
        return self._engine

    @property
    def hosts(self) -> tuple[str, ...]:
        return tuple(self._hosts)

    @property
    def pool_size(self) -> int:
        return self._pool_size

    @property
    def server_count(self) -> int:
        return self._server_count

    @property
    def connections_per_server(self) -> int:
        return self._connections_per_server

    @property
    def heartbeat_interval(self) -> float | None:
        return self._heartbeat_interval

    @property
    def max_pending_requests(self) -> int:
        return self._max_pending_requests

    @property
    def push_queue_size(self) -> int:
        return self._push_queue_size

    @property
    def push_queue_bytes(self) -> int:
        return self._push_queue_bytes

    @property
    def diagnostics(self) -> PoolDiagnostics:
        if self._engine is None:
            return PoolDiagnostics(
                0,
                PoolState.STOPPED,
                None,
                (),
                0,
                0,
                0,
                self._runtime_workers or min(self._pool_size, _available_parallelism()),
                self._pool_size,
                self._server_count,
                self._max_connections_per_host,
                self._connect_concurrency
                or min(
                    self._pool_size,
                    max(4, min(_available_parallelism() * 2, 32)),
                ),
                self._connect_concurrency_per_host
                or min(2, self._max_connections_per_host),
                0,
                self._global_raw_bytes or _automatic_raw_bytes(self._pool_size),
                0,
                0,
                self._global_decoded_bytes or _automatic_decoded_bytes(self._pool_size),
                0,
                self._push_queue_bytes,
            )
        return _pool_diagnostics(call_native(self._engine.pool_diagnostics))

    @property
    def connected_hosts(self) -> tuple[str | None, ...]:
        return tuple(actor.connected_host for actor in self.diagnostics.actors)

    @property
    def connected_host(self) -> str | None:
        return next((host for host in self.connected_hosts if host is not None), None)

    @property
    def pending_push_count(self) -> int:
        return self.diagnostics.push_frames

    def connect(self) -> None:
        call_native(self._native().connect)

    def close(self) -> None:
        if self._engine is not None:
            call_native(self._engine.close)

    def execute(self, command: int, payload: dict[str, Any] | None = None) -> Any:
        dto = call_native(self._native().execute, command, payload or {})
        return response_from_dto(dto)

    def request(self, command: str) -> str:
        if command == "ping":
            return "pong"
        raise ValueError(f"unsupported command: {command}")

    def poll_push(self, timeout: float | None = 0.0, *, parse: bool = False) -> Any:
        if self._engine is None:
            return None
        wait = self._timeout if timeout is None else timeout
        dto = call_native(self._engine.poll_push, wait, parse)
        return None if dto is None else _push_value(dto, parse)

    def drain_pushes(self, *, parse: bool = False) -> list[Any]:
        if self._engine is None:
            return []
        return [_push_value(dto, parse) for dto in call_native(self._engine.drain_pushes, parse)]

    @contextmanager
    def pin(self) -> Iterator[PinnedTransportProxy]:
        native_pin = call_native(self._native().pin)
        proxy = PinnedTransportProxy(self, native_pin, self.diagnostics.epoch)
        try:
            yield proxy
        finally:
            proxy.close()


__all__ = [
    "BrokerSnapshot",
    "DEFAULT_HEARTBEAT_INTERVAL",
    "DEFAULT_MAX_PENDING_REQUESTS",
    "DEFAULT_POOL_SIZE",
    "DEFAULT_PUSH_QUEUE_BYTES",
    "DEFAULT_PUSH_QUEUE_SIZE",
    "PoolDiagnostics",
    "PoolState",
    "PinnedTransportProxy",
    "PooledSocketTransport",
    "validate_pool_size",
]
