"""Rust-owned bounded pool transport facade."""

from __future__ import annotations

from collections.abc import Iterator, Sequence
from contextlib import contextmanager
from dataclasses import dataclass
from enum import Enum, auto
from typing import Any

from eltdx._native_models import response_from_dto
from eltdx.exceptions import ConnectionClosedError
from eltdx.hosts import (
    DEFAULT_HOSTS,
    DEFAULT_PROBE_TIMEOUT,
    DEFAULT_PROBE_WORKERS,
    sort_hosts_by_latency,
    unique_hosts,
)
from eltdx.transport.native import call_native, native_module

from .actor import ActorSnapshot, RuntimeState, TcpState
from .socket import (
    DEFAULT_HEARTBEAT_INTERVAL,
    DEFAULT_PUSH_QUEUE_BYTES,
    DEFAULT_PUSH_QUEUE_SIZE,
    _push_value,
    _resolved_native_hosts,
)

DEFAULT_MAX_PENDING_REQUESTS = 256
DEFAULT_POOL_SIZE = 1


def validate_pool_size(value: int) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value <= 0:
        raise ValueError("pool_size must be a positive integer")
    return value


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
        pool_size: int = DEFAULT_POOL_SIZE,
        probe_hosts: bool = False,
        probe_timeout: float = DEFAULT_PROBE_TIMEOUT,
        probe_workers: int = DEFAULT_PROBE_WORKERS,
        heartbeat_interval: float | None = DEFAULT_HEARTBEAT_INTERVAL,
        max_pending_requests: int = DEFAULT_MAX_PENDING_REQUESTS,
        push_queue_size: int = DEFAULT_PUSH_QUEUE_SIZE,
        push_queue_bytes: int = DEFAULT_PUSH_QUEUE_BYTES,
    ) -> None:
        values = unique_hosts(list(hosts or DEFAULT_HOSTS))
        if not values:
            raise ValueError("at least one host is required")
        if probe_hosts and len(values) > 1:
            values = sort_hosts_by_latency(
                values,
                timeout=probe_timeout,
                max_workers=probe_workers,
            )
        if timeout <= 0:
            raise ValueError("timeout must be > 0")
        if max_pending_requests <= 0:
            raise ValueError("max_pending_requests must be > 0")
        if push_queue_size <= 0 or push_queue_bytes <= 0:
            raise ValueError("push queue limits must be > 0")
        self._hosts = values
        self._timeout = float(timeout)
        self._pool_size = validate_pool_size(pool_size)
        self._heartbeat_interval = heartbeat_interval
        self._max_pending_requests = int(max_pending_requests)
        self._push_queue_size = int(push_queue_size)
        self._push_queue_bytes = int(push_queue_bytes)
        self._engine: Any = None

    def _native(self) -> Any:
        if self._engine is None:
            self._engine = call_native(
                native_module().NativeEngine,
                _resolved_native_hosts(self._hosts),
                timeout=self._timeout,
                pool_size=self._pool_size,
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
            return PoolDiagnostics(0, PoolState.STOPPED, None, (), 0, 0, 0)
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
