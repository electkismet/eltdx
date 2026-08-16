from __future__ import annotations

import os
from dataclasses import dataclass


DEFAULT_SERVER_COUNT = 2
DEFAULT_CONNECTIONS_PER_SERVER = 4
DEFAULT_POOL_SIZE = DEFAULT_SERVER_COUNT * DEFAULT_CONNECTIONS_PER_SERVER
DEFAULT_PUSH_QUEUE_BYTES = 64 * 1024 * 1024
DEFAULT_GLOBAL_RAW_CAP_BYTES = 256 * 1024 * 1024
DEFAULT_GLOBAL_DECODED_CAP_BYTES = 2 * 1024 * 1024 * 1024
SLOT_RAW_MAX_BYTES = 256 * 1024 + 16 + 65_535
SLOT_DECODED_MAX_BYTES = 8 * 1024 * 1024


@dataclass(frozen=True, slots=True)
class PoolLayout:
    pool_size: int
    server_count: int
    connections_per_server: int
    max_connections_per_host: int


def available_parallelism() -> int:
    process_cpu_count = getattr(os, "process_cpu_count", None)
    if process_cpu_count is not None:
        try:
            value = process_cpu_count()
        except (OSError, NotImplementedError):
            value = None
        if value:
            return max(1, value)
    affinity = getattr(os, "sched_getaffinity", None)
    if affinity is not None:
        try:
            return max(1, len(affinity(0)))
        except (OSError, NotImplementedError):
            pass
    try:
        value = os.cpu_count()
    except (OSError, NotImplementedError):
        value = None
    return max(1, value or 1)


def automatic_raw_bytes(pool_size: int) -> int:
    return max(
        SLOT_RAW_MAX_BYTES,
        min(pool_size * SLOT_RAW_MAX_BYTES, DEFAULT_GLOBAL_RAW_CAP_BYTES),
    )


def automatic_decoded_bytes(pool_size: int) -> int:
    return max(
        SLOT_DECODED_MAX_BYTES,
        min(pool_size * SLOT_DECODED_MAX_BYTES, DEFAULT_GLOBAL_DECODED_CAP_BYTES),
    )


def positive_int(name: str, value: int) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value <= 0:
        raise ValueError(f"{name} must be a positive integer")
    return value


def optional_positive_int(name: str, value: int | None) -> int | None:
    if value is None:
        return None
    return positive_int(name, value)


def resolve_pool_layout(
    *,
    host_count: int,
    pool_size: int | None,
    server_count: int,
    connections_per_server: int | None,
    max_connections_per_host: int | None,
) -> PoolLayout:
    host_count = positive_int("host_count", host_count)
    requested_servers = positive_int("server_count", server_count)
    pool_size = optional_positive_int("pool_size", pool_size)
    connections_per_server = optional_positive_int(
        "connections_per_server", connections_per_server
    )

    if pool_size is None:
        active_servers = min(requested_servers, host_count)
        per_server = connections_per_server or DEFAULT_CONNECTIONS_PER_SERVER
        total = active_servers * per_server
    else:
        total = pool_size
        active_servers = min(requested_servers, host_count, total)
        if connections_per_server is not None:
            expected = active_servers * connections_per_server
            if total != expected:
                raise ValueError(
                    "pool_size must equal server_count * connections_per_server "
                    "when all three are provided"
                )
        per_server = (total + active_servers - 1) // active_servers

    minimum_host_capacity = (total + active_servers - 1) // active_servers
    host_capacity = optional_positive_int(
        "max_connections_per_host", max_connections_per_host
    )
    if host_capacity is None:
        host_capacity = minimum_host_capacity
    if host_capacity < minimum_host_capacity:
        raise ValueError(
            "max_connections_per_host cannot hold the configured initial Slot distribution"
        )
    return PoolLayout(
        pool_size=total,
        server_count=active_servers,
        connections_per_server=per_server,
        max_connections_per_host=host_capacity,
    )
