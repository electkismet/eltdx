"""Compatibility snapshots for the Rust-owned transport runtime.

The executable Actor, socket, and request scheduling implementation lives in
the native Rust engine. These types remain only for documented diagnostics and
import compatibility.
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum, auto


class RuntimeState(Enum):
    STARTING = auto()
    RUNNING = auto()
    CLOSING = auto()
    STOPPED = auto()
    FAILED = auto()
    FAILED_CLOSING = auto()
    FAILED_CLOSED = auto()


class TcpState(Enum):
    DOWN = auto()
    CONNECTING = auto()
    CONNECTED_UNHANDSHAKEN = auto()
    HANDSHAKING = auto()
    READY = auto()
    RETIRING = auto()


@dataclass(frozen=True, slots=True)
class ActorSnapshot:
    runtime_epoch: int
    state: RuntimeState
    tcp_state: TcpState
    tcp_generation: int
    connected_host: str | None
    actor_alive: bool
    pending_depth: int
    reconnect_count: int
    stale_event_count: int
    last_error: str | None


__all__ = ["ActorSnapshot", "RuntimeState", "TcpState"]
