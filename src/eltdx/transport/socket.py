"""Single-slot facade for the Rust-owned 7709 transport engine."""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass
from typing import Any

from eltdx._native_models import push_frame_from_dto, response_from_dto
from eltdx.hosts import DEFAULT_HOSTS, resolve_hosts, unique_hosts
from eltdx.transport.native import call_native, native_module

from .actor import ActorSnapshot, RuntimeState, TcpState

DEFAULT_HEARTBEAT_INTERVAL = 30.0
DEFAULT_PUSH_QUEUE_SIZE = 1024
DEFAULT_PUSH_QUEUE_BYTES = 8 * 1024 * 1024


@dataclass(frozen=True, slots=True)
class TransportDiagnostics:
    epoch: int
    actor: ActorSnapshot | None
    push_frames: int
    push_bytes: int
    push_dropped: int
    push_max_frames: int
    push_max_bytes: int


def _resolved_native_hosts(hosts: Sequence[str]) -> list[str]:
    endpoints = resolve_hosts(list(hosts))
    values: list[str] = []
    for endpoint in endpoints:
        address = endpoint.address
        if ":" in address:
            address = f"[{address}]"
        value = f"{address}:{endpoint.port}"
        if value not in values:
            values.append(value)
    if not values:
        raise ValueError("at least one host is required")
    return values


def _actor(value: tuple[Any, ...] | None) -> ActorSnapshot | None:
    if value is None:
        return None
    return ActorSnapshot(
        runtime_epoch=value[0],
        state=RuntimeState[value[1]],
        tcp_state=TcpState[value[2]],
        tcp_generation=value[3],
        connected_host=value[4],
        actor_alive=value[5],
        pending_depth=value[6],
        reconnect_count=value[7],
        stale_event_count=value[8],
        last_error=value[9],
    )


def _push_value(dto: Any, parse: bool) -> Any:
    _, _, _, _, response, native_parse = push_frame_from_dto(dto)
    if native_parse != parse:
        raise TypeError("native push DTO parse mode does not match the request")
    if not parse:
        return response
    parsed = call_native(
        native_module().parse_command_response,
        response.msg_type,
        response.raw,
        {},
    )
    return response_from_dto(parsed)


class SocketTransport:
    """Synchronous native facade backed by one Rust Engine Slot."""

    def __init__(
        self,
        hosts: Sequence[str] | None = None,
        *,
        timeout: float = 8.0,
        heartbeat_interval: float | None = DEFAULT_HEARTBEAT_INTERVAL,
        push_queue_size: int = DEFAULT_PUSH_QUEUE_SIZE,
        push_queue_bytes: int = DEFAULT_PUSH_QUEUE_BYTES,
    ) -> None:
        self._hosts = unique_hosts(list(hosts or DEFAULT_HOSTS))
        if not self._hosts:
            raise ValueError("at least one host is required")
        if timeout <= 0:
            raise ValueError("timeout must be > 0")
        if push_queue_size <= 0:
            raise ValueError("push_queue_size must be > 0")
        if push_queue_bytes <= 0:
            raise ValueError("push_queue_bytes must be > 0")
        self._timeout = float(timeout)
        self._heartbeat_interval = heartbeat_interval
        self._push_queue_size = int(push_queue_size)
        self._push_queue_bytes = int(push_queue_bytes)
        self._engine: Any = None

    def _native(self) -> Any:
        if self._engine is None:
            self._engine = call_native(
                native_module().NativeEngine,
                _resolved_native_hosts(self._hosts),
                timeout=self._timeout,
                pool_size=1,
                heartbeat_interval=self._heartbeat_interval,
                max_pending_requests=256,
                push_queue_size=self._push_queue_size,
                push_queue_bytes=self._push_queue_bytes,
            )
        return self._engine

    @property
    def connected_host(self) -> str | None:
        actor = self.diagnostics.actor
        return actor.connected_host if actor is not None else None

    @property
    def last_handshake(self) -> Any:
        return self._session_value(0)

    @property
    def last_heartbeat(self) -> Any:
        return self._session_value(1)

    @property
    def pending_push_count(self) -> int:
        return self.diagnostics.push_frames

    def _session_value(self, index: int) -> Any:
        if self._engine is None:
            return None
        dto = call_native(self._engine.session_snapshot)[index]
        return None if dto is None else response_from_dto(dto)

    @property
    def diagnostics(self) -> TransportDiagnostics:
        if self._engine is None:
            return TransportDiagnostics(
                0,
                None,
                0,
                0,
                0,
                self._push_queue_size,
                self._push_queue_bytes,
            )
        dto = call_native(self._engine.transport_diagnostics)
        return TransportDiagnostics(
            epoch=dto[0],
            actor=_actor(dto[1]),
            push_frames=dto[2],
            push_bytes=dto[3],
            push_dropped=dto[4],
            push_max_frames=dto[5],
            push_max_bytes=dto[6],
        )

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
        if timeout is None:
            timeout = self._timeout
        if timeout < 0:
            raise ValueError("timeout must be >= 0")
        if self._engine is None:
            return None
        dto = call_native(self._engine.poll_push, timeout, parse)
        return None if dto is None else _push_value(dto, parse)

    def drain_pushes(self, *, parse: bool = False) -> list[Any]:
        if self._engine is None:
            return []
        return [_push_value(dto, parse) for dto in call_native(self._engine.drain_pushes, parse)]


__all__ = [
    "DEFAULT_HEARTBEAT_INTERVAL",
    "DEFAULT_PUSH_QUEUE_BYTES",
    "DEFAULT_PUSH_QUEUE_SIZE",
    "SocketTransport",
    "TransportDiagnostics",
]
