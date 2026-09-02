"""Default 7709 quote server hosts."""

from __future__ import annotations

import ipaddress
import json
import os
import socket
import sys
import time
import warnings
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from datetime import datetime, timezone
from functools import lru_cache
from importlib import resources
from pathlib import Path
from threading import Lock, get_ident
from typing import Any

SERVER_FILE = "tdx_server.json"
MONEY_FLOW_SERVER_FILE = "money_flow_servers.json"
SERVER_RANKING_FILE = "tdx_server_ranking.json"
SERVER_RANKING_SCHEMA_VERSION = 1
DEFAULT_PROBE_TIMEOUT = 1.2
DEFAULT_PROBE_WORKERS = 32
DEFAULT_PROBE_HOSTS = True

_RANKING_WRITE_LOCK = Lock()


@dataclass(frozen=True, slots=True)
class HostProbeResult:
    host: str
    ok: bool
    latency_ms: float | None = None
    error: str | None = None


@dataclass(frozen=True, slots=True)
class ResolvedEndpoint:
    host: str
    address: str
    port: int
    family: int
    socktype: int
    proto: int
    sockaddr: tuple[Any, ...]


FALLBACK_HOSTS: tuple[str, ...] = (
    "116.205.183.150:7709",
    "116.205.171.132:7709",
    "111.230.186.52:7709",
    "129.204.230.128:7709",
    "116.205.163.254:7709",
    "110.41.2.72:7709",
    "159.75.29.111:7709",
    "43.139.95.83:7709",
    "175.178.128.227:7709",
    "110.41.147.114:7709",
    "124.71.9.153:7709",
    "81.71.32.47:7709",
    "43.139.18.171:7709",
    "119.97.185.59:7709",
    "123.60.70.228:7709",
    "123.60.73.44:7709",
    "124.70.199.56:7709",
    "175.178.112.197:7709",
    "101.33.225.16:7709",
    "124.71.187.122:7709",
    "124.71.187.72:7709",
    "111.229.247.189:7709",
    "121.36.225.169:7709",
    "150.158.160.2:7709",
    "123.60.164.122:7709",
    "49.232.15.141:7709",
    "122.51.120.217:7709",
    "111.231.113.208:7709",
    "124.223.163.242:7709",
    "62.234.50.143:7709",
    "101.35.121.35:7709",
    "101.42.240.54:7709",
    "101.43.159.194:7709",
    "81.70.151.186:7709",
    "82.156.174.84:7709",
    "123.60.84.66:7709",
    "120.53.8.251:7709",
    "124.70.133.119:7709",
    "118.25.98.114:7709",
    "122.51.232.182:7709",
    "101.42.164.241:7709",
    "152.136.191.169:7709",
    "82.156.214.79:7709",
)


def normalize_host(value: Any) -> str | None:
    if not isinstance(value, str):
        return None
    host = value.strip()
    if not host or ":" not in host:
        return None
    address, port = host.rsplit(":", 1)
    address = address.strip()
    port = port.strip()
    if not address or not port.isdigit():
        return None
    try:
        port_number = int(port)
    except ValueError:
        return None
    if not 1 <= port_number <= 65535:
        return None
    return f"{address}:{port_number}"


def unique_hosts(values: list[Any] | tuple[Any, ...]) -> list[str]:
    hosts: list[str] = []
    for value in values:
        host = normalize_host(value)
        if host is not None and host not in hosts:
            hosts.append(host)
    return hosts


@lru_cache(maxsize=256)
def resolve_host(host: str) -> tuple[ResolvedEndpoint, ...]:
    """Resolve one normalized host outside the Actor thread."""

    normalized = normalize_host(host)
    if normalized is None:
        raise ValueError(f"invalid host: {host!r}")
    address, port_text = normalized.rsplit(":", 1)
    address = address.removeprefix("[").removesuffix("]")
    port = int(port_text)
    try:
        numeric = ipaddress.ip_address(address)
    except ValueError:
        records = socket.getaddrinfo(address, port, type=socket.SOCK_STREAM)
    else:
        family = socket.AF_INET6 if numeric.version == 6 else socket.AF_INET
        sockaddr: tuple[Any, ...] = (address, port, 0, 0) if family == socket.AF_INET6 else (address, port)
        records = [(family, socket.SOCK_STREAM, socket.IPPROTO_TCP, "", sockaddr)]

    endpoints: list[ResolvedEndpoint] = []
    seen: set[tuple[int, int, int, tuple[Any, ...]]] = set()
    for family, socktype, proto, _, sockaddr in records:
        key = (family, socktype, proto, tuple(sockaddr))
        if key in seen:
            continue
        seen.add(key)
        endpoints.append(
            ResolvedEndpoint(
                host=normalized,
                address=str(sockaddr[0]),
                port=port,
                family=family,
                socktype=socktype,
                proto=proto,
                sockaddr=tuple(sockaddr),
            )
        )
    if not endpoints:
        raise OSError(f"unable to resolve host: {normalized}")
    return tuple(endpoints)


def resolve_hosts(hosts: list[str] | tuple[str, ...]) -> tuple[ResolvedEndpoint, ...]:
    endpoints: list[ResolvedEndpoint] = []
    last_error: OSError | None = None
    for host in unique_hosts(list(hosts)):
        try:
            endpoints.extend(resolve_host(host))
        except OSError as exc:
            last_error = exc
    if not endpoints and last_error is not None:
        raise OSError("unable to resolve any configured host") from last_error
    return tuple(endpoints)


def load_server_config() -> dict[str, Any]:
    """Load packaged 7709 host configuration.

    The file is deliberately optional at runtime. If a downstream package strips
    it out, the client still falls back to the built-in host list below.
    """

    try:
        content = resources.files("eltdx").joinpath(SERVER_FILE).read_text(encoding="utf-8")
        data = json.loads(content)
    except (FileNotFoundError, json.JSONDecodeError, OSError):
        return {}
    return data if isinstance(data, dict) else {}


def load_server_hosts() -> list[str]:
    """Return normalized hosts from the packaged server config."""

    data = load_server_config()
    hosts = data.get("hosts")
    if isinstance(hosts, list):
        return unique_hosts(hosts)

    values: list[Any] = [data.get("current_host")]
    for key in ("manual_hosts", "imported_hosts"):
        item = data.get(key, [])
        if isinstance(item, list):
            values.extend(item)
    return unique_hosts(values)


def load_money_flow_hosts() -> list[str]:
    """Return hosts reserved for the 0x0FFC money-flow service."""

    try:
        content = resources.files("eltdx").joinpath(MONEY_FLOW_SERVER_FILE).read_text(
            encoding="utf-8"
        )
        data = json.loads(content)
    except (FileNotFoundError, json.JSONDecodeError, OSError):
        return []
    if not isinstance(data, dict):
        return []
    hosts = data.get("hosts")
    return unique_hosts(hosts) if isinstance(hosts, list) else []


def server_ranking_path() -> Path:
    """Return the writable per-user server ranking path."""

    override = os.environ.get("ELTDX_DATA_DIR")
    if override:
        data_dir = Path(override).expanduser()
    elif os.name == "nt":
        windows_data = os.environ.get("LOCALAPPDATA") or os.environ.get("APPDATA")
        data_dir = Path(windows_data or Path.home()) / "eltdx"
    elif sys.platform == "darwin":
        data_dir = Path.home() / "Library" / "Application Support" / "eltdx"
    else:
        xdg_data = os.environ.get("XDG_DATA_HOME")
        data_dir = Path(xdg_data or Path.home() / ".local" / "share") / "eltdx"
    return data_dir / SERVER_RANKING_FILE


def load_server_ranking() -> dict[str, Any]:
    """Load the last complete per-user probe ranking, if available."""

    try:
        data = json.loads(server_ranking_path().read_text(encoding="utf-8"))
    except (FileNotFoundError, json.JSONDecodeError, OSError):
        return {}
    if not isinstance(data, dict) or data.get("schema_version") != SERVER_RANKING_SCHEMA_VERSION:
        return {}
    return data


def rank_hosts_from_cache(hosts: list[str] | tuple[str, ...]) -> list[str]:
    """Order candidates by the last persisted ranking without removing hosts."""

    candidates = unique_hosts(list(hosts))
    candidate_set = set(candidates)
    records = load_server_ranking().get("hosts", [])
    if not isinstance(records, list):
        return candidates
    ranked = unique_hosts(
        [record.get("host") for record in records if isinstance(record, dict)]
    )
    cached = [host for host in ranked if host in candidate_set]
    cached_set = set(cached)
    return cached + [host for host in candidates if host not in cached_set]


def _persist_probe_results(
    candidates: list[str],
    results: list[HostProbeResult],
) -> None:
    previous = load_server_ranking()
    previous_records = previous.get("hosts", [])
    if not isinstance(previous_records, list):
        previous_records = []
    previous_by_host = {
        record["host"]: record
        for record in previous_records
        if isinstance(record, dict) and normalize_host(record.get("host")) is not None
    }
    previous_order = unique_hosts(
        [record.get("host") for record in previous_records if isinstance(record, dict)]
    )
    result_by_host = {result.host: result for result in results}
    reachable = sorted(
        (result for result in results if result.ok),
        key=lambda result: (
            result.latency_ms if result.latency_ms is not None else float("inf"),
            candidates.index(result.host),
        ),
    )
    reachable_hosts = [result.host for result in reachable]
    known_hosts = unique_hosts(previous_order + candidates)
    reachable_set = set(reachable_hosts)
    ranked_hosts = reachable_hosts + [host for host in known_hosts if host not in reachable_set]
    probed_at = datetime.now(timezone.utc).isoformat()

    records: list[dict[str, Any]] = []
    for rank, host in enumerate(ranked_hosts, start=1):
        prior = previous_by_host.get(host, {})
        result = result_by_host.get(host)
        if result is None:
            record = dict(prior) if prior else {"host": host}
        elif result.ok:
            record = {
                "host": host,
                "ok": True,
                "latency_ms": result.latency_ms,
                "error": None,
                "last_probe_at": probed_at,
                "last_success_at": probed_at,
                "last_success_latency_ms": result.latency_ms,
                "consecutive_failures": 0,
            }
        else:
            prior_failures = prior.get("consecutive_failures", 0)
            if (
                isinstance(prior_failures, bool)
                or not isinstance(prior_failures, int)
                or prior_failures < 0
            ):
                prior_failures = 0
            record = {
                "host": host,
                "ok": False,
                "latency_ms": None,
                "error": result.error,
                "last_probe_at": probed_at,
                "last_success_at": prior.get("last_success_at"),
                "last_success_latency_ms": prior.get("last_success_latency_ms"),
                "consecutive_failures": prior_failures + 1,
            }
        record["host"] = host
        record["rank"] = rank
        records.append(record)

    payload = {
        "schema_version": SERVER_RANKING_SCHEMA_VERSION,
        "updated_at": probed_at,
        "hosts": records,
    }
    path = server_ranking_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.{os.getpid()}.{get_ident()}.tmp")
    try:
        with temporary.open("w", encoding="utf-8") as stream:
            json.dump(payload, stream, ensure_ascii=False, indent=2)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        try:
            temporary.chmod(0o600)
        except OSError:
            pass
        temporary.replace(path)
    finally:
        try:
            temporary.unlink()
        except FileNotFoundError:
            pass


def probe_host(host: str, *, timeout: float = DEFAULT_PROBE_TIMEOUT) -> HostProbeResult:
    """Measure whether a 7709 host accepts TCP connections."""

    normalized = normalize_host(host)
    if normalized is None:
        return HostProbeResult(host=str(host), ok=False, error="invalid host")

    address, port_text = normalized.rsplit(":", 1)
    address = address.removeprefix("[").removesuffix("]")
    started = time.perf_counter()
    try:
        with socket.create_connection((address, int(port_text)), timeout=timeout):
            latency_ms = round((time.perf_counter() - started) * 1000.0, 2)
            return HostProbeResult(host=normalized, ok=True, latency_ms=latency_ms)
    except OSError as exc:
        return HostProbeResult(host=normalized, ok=False, error=type(exc).__name__)


def probe_hosts(
    hosts: list[str] | tuple[str, ...],
    *,
    timeout: float = DEFAULT_PROBE_TIMEOUT,
    max_workers: int = DEFAULT_PROBE_WORKERS,
    persist: bool = True,
) -> list[HostProbeResult]:
    """Probe many hosts concurrently and persist their ranking by default."""

    candidates = unique_hosts(list(hosts))
    if not candidates:
        return []

    worker_count = min(max(1, int(max_workers)), len(candidates))
    results: list[HostProbeResult] = []
    with ThreadPoolExecutor(max_workers=worker_count, thread_name_prefix="eltdx-probe") as executor:
        futures = [executor.submit(probe_host, host, timeout=timeout) for host in candidates]
        for future in as_completed(futures):
            results.append(future.result())
    if persist:
        try:
            with _RANKING_WRITE_LOCK:
                _persist_probe_results(candidates, results)
        except OSError as exc:
            warnings.warn(
                f"unable to persist eltdx server ranking: {exc}",
                RuntimeWarning,
                stacklevel=2,
            )
    return results


def refresh_server_ranking(
    hosts: list[str] | tuple[str, ...] | None = None,
    *,
    timeout: float = DEFAULT_PROBE_TIMEOUT,
    max_workers: int = DEFAULT_PROBE_WORKERS,
) -> list[HostProbeResult]:
    """Manually probe and persist all packaged servers, or explicit candidates."""

    candidates = unique_hosts(list(hosts or load_server_hosts() or FALLBACK_HOSTS))
    results = probe_hosts(candidates, timeout=timeout, max_workers=max_workers, persist=True)
    by_host = {result.host: result for result in results}
    return [by_host[host] for host in rank_hosts_from_cache(candidates) if host in by_host]


def sort_hosts_by_latency(
    hosts: list[str] | tuple[str, ...],
    *,
    timeout: float = DEFAULT_PROBE_TIMEOUT,
    max_workers: int = DEFAULT_PROBE_WORKERS,
) -> list[str]:
    """Return reachable hosts first, ordered by TCP latency."""

    candidates = unique_hosts(list(hosts))
    previous_order = rank_hosts_from_cache(candidates)
    results = probe_hosts(candidates, timeout=timeout, max_workers=max_workers, persist=True)
    reachable = sorted(
        (result for result in results if result.ok),
        key=lambda result: (
            result.latency_ms if result.latency_ms is not None else float("inf"),
            candidates.index(result.host),
        ),
    )
    reachable_hosts = {result.host for result in reachable}
    unreachable = [host for host in previous_order if host not in reachable_hosts]
    return [result.host for result in reachable] + unreachable


DEFAULT_HOSTS: tuple[str, ...] = tuple(load_server_hosts() or FALLBACK_HOSTS)
