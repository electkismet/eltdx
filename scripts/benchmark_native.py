"""Run a counterbalanced v2.0.5 versus native 3.0 benchmark campaign."""

from __future__ import annotations

import argparse
import gc
import hashlib
import importlib.metadata
import json
import os
import platform
import socket
import statistics
import struct
import subprocess
import sys
import tempfile
import threading
import time
from collections.abc import Callable, Sequence
from concurrent.futures import ThreadPoolExecutor
from contextlib import suppress
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
BASELINE_COMMIT = "6486a1692dd4aca5339001b2de22e88bb29e16ec"
SCHEDULE = ("baseline", "current", "current", "baseline")
TYPE_HANDSHAKE = 0x000D
TYPE_SNAPSHOTS = 0x054C
TYPE_KLINES = 0x052D
TYPE_FILE_CONTENT = 0x06B9
TYPE_TODAY_TICKS = 0x0FC5
BENCHMARK_PATH = "eltdx-native-benchmark.bin"
BENCHMARK_VALUE = struct.Struct("<III")
NETWORK_DELAY_SECONDS = 0.0005
SINGLE_REQUESTS = 2_000
POOL_REQUESTS = 5_000
POOL_SIZES = (1, 4, 8)
PARSE_ITERATIONS = 50
LIFECYCLE_CYCLES = 100
SNAPSHOT_RECORD = bytes.fromhex(
    "00303030303031e61185115b5c005fa4a3cf0ec51187e9aa01bfe40afb44eb0994298cf6800"
    "b8df094100901381c3011614120010004091fc4c000000000000000000000000ca0b9f409ffa84c200"
    "00000000000000000000000000000000000000000000e611"
)


class BenchmarkServer:
    def __init__(self, response_delay: float = NETWORK_DELAY_SECONDS) -> None:
        self.response_delay = response_delay
        self._listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self._listener.settimeout(0.2)
        self._stop = threading.Event()
        self._condition = threading.Condition()
        self._acceptor: threading.Thread | None = None
        self._connections: set[socket.socket] = set()
        self._workers: set[threading.Thread] = set()
        self._active = 0
        self.accepted = 0
        self.requests = 0
        self.errors: list[BaseException] = []
        self.host = ""

    def __enter__(self) -> BenchmarkServer:
        self._listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self._listener.bind(("127.0.0.1", 0))
        self._listener.listen()
        address, port = self._listener.getsockname()
        self.host = f"{address}:{port}"
        self._acceptor = threading.Thread(
            target=self._accept,
            name="eltdx-native-benchmark-acceptor",
            daemon=True,
        )
        self._acceptor.start()
        return self

    def __exit__(self, exc_type, exc, tb) -> None:
        self._stop.set()
        self._listener.close()
        with self._condition:
            connections = tuple(self._connections)
            workers = set(self._workers)
        for connection in connections:
            with suppress(OSError):
                connection.shutdown(socket.SHUT_RDWR)
            connection.close()
        if self._acceptor is not None:
            self._acceptor.join(timeout=5)
        with self._condition:
            workers.update(self._workers)
        for worker in workers:
            worker.join(timeout=5)
        with self._condition:
            stopped = self._condition.wait_for(lambda: not self._workers, timeout=5)
            errors = tuple(self.errors)
        acceptor_alive = self._acceptor is not None and self._acceptor.is_alive()
        if exc_type is None and (not stopped or acceptor_alive):
            raise AssertionError("native benchmark server did not stop")
        if exc_type is None and errors:
            raise AssertionError(f"native benchmark server failed: {errors!r}")

    def _accept(self) -> None:
        while not self._stop.is_set():
            try:
                connection, _ = self._listener.accept()
            except TimeoutError:
                continue
            except OSError:
                return
            connection.settimeout(10)
            with self._condition:
                self.accepted += 1
                connection_id = self.accepted
                self._connections.add(connection)
            worker = threading.Thread(
                target=self._serve,
                args=(connection, connection_id),
                name=f"eltdx-native-benchmark-{connection_id}",
                daemon=True,
            )
            with self._condition:
                self._workers.add(worker)
            worker.start()

    def _serve(self, connection: socket.socket, connection_id: int) -> None:
        current = threading.current_thread()
        try:
            with connection:
                while not self._stop.is_set():
                    message_id, message_type, payload = _read_request(connection)
                    if message_type == TYPE_HANDSHAKE:
                        if payload != b"\x01":
                            raise AssertionError("invalid benchmark handshake")
                        connection.sendall(
                            _response(message_id, message_type, _handshake_payload())
                        )
                        continue
                    if message_type != TYPE_FILE_CONTENT:
                        raise AssertionError(f"unexpected benchmark command: {message_type:#x}")
                    token, requested_size = _file_request(payload)
                    if requested_size != BENCHMARK_VALUE.size:
                        raise AssertionError("benchmark identity size mismatch")
                    with self._condition:
                        self.requests += 1
                        sequence = self.requests
                        self._active += 1
                    try:
                        if self.response_delay:
                            time.sleep(self.response_delay)
                        content = BENCHMARK_VALUE.pack(token, connection_id, sequence)
                        body = len(content).to_bytes(4, "little") + content
                        connection.sendall(_response(message_id, message_type, body))
                    finally:
                        with self._condition:
                            self._active -= 1
                            self._condition.notify_all()
        except (EOFError, OSError, TimeoutError):
            return
        except BaseException as error:
            with self._condition:
                self.errors.append(error)
        finally:
            with self._condition:
                self._connections.discard(connection)
                self._workers.discard(current)
                self._condition.notify_all()


def _read_exact(connection: socket.socket, size: int) -> bytes:
    output = bytearray()
    while len(output) < size:
        chunk = connection.recv(size - len(output))
        if not chunk:
            raise EOFError("benchmark connection closed")
        output.extend(chunk)
    return bytes(output)


def _read_request(connection: socket.socket) -> tuple[int, int, bytes]:
    header = _read_exact(connection, 12)
    if header[0] != 0x0C:
        raise AssertionError("invalid benchmark request prefix")
    first_length = int.from_bytes(header[6:8], "little")
    second_length = int.from_bytes(header[8:10], "little")
    if first_length != second_length or first_length < 2:
        raise AssertionError("invalid benchmark request length")
    return (
        int.from_bytes(header[1:5], "little"),
        int.from_bytes(header[10:12], "little"),
        _read_exact(connection, first_length - 2),
    )


def _file_request(payload: bytes) -> tuple[int, int]:
    if len(payload) != 308:
        raise AssertionError("invalid benchmark file request length")
    path = payload[8:].split(b"\x00", 1)[0]
    if path != BENCHMARK_PATH.encode("ascii"):
        raise AssertionError("invalid benchmark file request path")
    return int.from_bytes(payload[:4], "little"), int.from_bytes(payload[4:8], "little")


def _response(message_id: int, message_type: int, payload: bytes) -> bytes:
    size = len(payload).to_bytes(2, "little")
    return (
        b"\xb1\xcb\x74\x00\x00"
        + message_id.to_bytes(4, "little")
        + b"\x00"
        + message_type.to_bytes(2, "little")
        + size
        + size
        + payload
    )


def _handshake_payload() -> bytes:
    payload = bytearray(189)
    payload[1:3] = (2026).to_bytes(2, "little")
    payload[3:9] = bytes((15, 8, 30, 10, 0, 0))
    payload[42:46] = (20260815).to_bytes(4, "little")
    payload[50:54] = (20260815).to_bytes(4, "little")
    payload[68:152] = b"native-benchmark".ljust(84, b"\x00")
    payload[160:189] = b"eltdx-3-benchmark".ljust(29, b"\x00")
    return bytes(payload)


def _percentile(values: Sequence[int], numerator: int, denominator: int) -> int:
    if not values:
        raise ValueError("benchmark sample is empty")
    ordered = sorted(values)
    return ordered[((len(ordered) - 1) * numerator) // denominator]


def _sample_summary(values: Sequence[int]) -> dict[str, Any]:
    return {
        "samples": len(values),
        "p50_ns": int(statistics.median(values)),
        "p95_ns": _percentile(values, 95, 100),
        "p99_ns": _percentile(values, 99, 100),
        "min_ns": min(values),
        "max_ns": max(values),
    }


def _rss_bytes() -> int:
    if platform.system() == "Windows":
        import ctypes
        from ctypes import wintypes

        class ProcessMemoryCounters(ctypes.Structure):
            _fields_ = [
                ("cb", wintypes.DWORD),
                ("PageFaultCount", wintypes.DWORD),
                ("PeakWorkingSetSize", ctypes.c_size_t),
                ("WorkingSetSize", ctypes.c_size_t),
                ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
                ("QuotaPagedPoolUsage", ctypes.c_size_t),
                ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
                ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
                ("PagefileUsage", ctypes.c_size_t),
                ("PeakPagefileUsage", ctypes.c_size_t),
            ]

        counters = ProcessMemoryCounters()
        counters.cb = ctypes.sizeof(counters)
        process = ctypes.windll.kernel32.GetCurrentProcess()
        if not ctypes.windll.psapi.GetProcessMemoryInfo(
            process,
            ctypes.byref(counters),
            counters.cb,
        ):
            raise OSError("GetProcessMemoryInfo failed")
        return int(counters.PeakWorkingSetSize)
    import resource

    maximum = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return int(maximum if platform.system() == "Darwin" else maximum * 1_024)


def _execute_timed(transport: Any, token: int) -> tuple[int, int, int]:
    from eltdx.models import FileContentChunk

    started = time.perf_counter_ns()
    result = transport.execute(
        TYPE_FILE_CONTENT,
        {"path": BENCHMARK_PATH, "offset": token, "size": BENCHMARK_VALUE.size},
    )
    elapsed = time.perf_counter_ns() - started
    if not isinstance(result, FileContentChunk):
        raise AssertionError(f"unexpected benchmark result: {result!r}")
    echoed, connection_id, sequence = BENCHMARK_VALUE.unpack(result.content)
    if result.offset != token or echoed != token:
        raise AssertionError(f"cross-request benchmark completion: {token} != {echoed}")
    return elapsed, connection_id, sequence


def _transport_case(pool_size: int, concurrency: int, requests: int) -> dict[str, Any]:
    from eltdx.transport import PooledSocketTransport
    from eltdx.transport.pool import PoolState

    with BenchmarkServer() as server:
        transport = PooledSocketTransport(
            [server.host],
            timeout=10,
            pool_size=pool_size,
            heartbeat_interval=None,
            max_pending_requests=max(concurrency, 256),
        )
        try:
            transport.connect()
            for token in range(200):
                _execute_timed(transport, 0xE000_0000 + token)
            rss_before = _rss_bytes()
            cpu_started = time.process_time_ns()
            wall_started = time.perf_counter_ns()
            if concurrency == 1:
                results = [_execute_timed(transport, token) for token in range(requests)]
            else:
                with ThreadPoolExecutor(max_workers=concurrency) as executor:
                    results = list(
                        executor.map(
                            lambda token: _execute_timed(transport, token),
                            range(requests),
                        )
                    )
            wall_elapsed = time.perf_counter_ns() - wall_started
            cpu_elapsed = time.process_time_ns() - cpu_started
            rss_after = _rss_bytes()
            diagnostics = transport.diagnostics
            if diagnostics.broker is None or diagnostics.broker.active_leases != 0:
                raise AssertionError("benchmark transport retained an active lease")
            if diagnostics.broker.waiter_count or diagnostics.broker.pin_waiter_count:
                raise AssertionError("benchmark transport retained a waiter")
            latencies = [result[0] for result in results]
            sequences = [result[2] for result in results]
            if len(set(sequences)) != requests:
                raise AssertionError("benchmark server sequence identity is not unique")
        finally:
            transport.close()
        if transport.diagnostics.state is not PoolState.STOPPED:
            raise AssertionError("benchmark transport did not stop")
    return {
        "pool_size": pool_size,
        "concurrency": concurrency,
        "requests": requests,
        "elapsed_ns": wall_elapsed,
        "cpu_ns": cpu_elapsed,
        "rss_before_bytes": rss_before,
        "rss_after_bytes": rss_after,
        "rss_observed_bytes": max(rss_before, rss_after),
        "throughput_rps": requests * 1_000_000_000 / wall_elapsed,
        "server_connections": server.accepted,
        "server_requests": server.requests,
        "latency_ns": latencies,
        **_sample_summary(latencies),
    }


def _response_raw(message_type: int, payload: bytes) -> bytes:
    return _response(0x1234_5678, message_type, payload)


def _snapshot_fixture(count: int) -> tuple[bytes, dict[str, Any], Callable[[Any], int]]:
    codes = [f"sz{index:06d}" for index in range(1, count + 1)]
    records = []
    for code in codes:
        record = bytearray(SNAPSHOT_RECORD)
        record[0] = 0
        record[1:7] = code[2:].encode("ascii")
        records.append(bytes(record))
    payload = b"\x00\x00" + count.to_bytes(2, "little") + b"".join(records)
    return _response_raw(TYPE_SNAPSHOTS, payload), {"codes": codes}, len


def _kline_fixture(count: int) -> tuple[bytes, dict[str, Any], Callable[[Any], int]]:
    record = (20260814).to_bytes(4, "little") + b"\x00" * 12
    payload = count.to_bytes(2, "little") + record * count
    request = {"code": "sz000001", "period": "day", "start": 0, "count": count}
    return _response_raw(TYPE_KLINES, payload), request, lambda value: len(value.bars)


def _ticks_fixture(count: int) -> tuple[bytes, dict[str, Any], Callable[[Any], int]]:
    record = bytes.fromhex("50030a14030000")
    payload = count.to_bytes(2, "little") + record * count
    request = {"code": "sz000001", "start": 0, "count": count}
    return _response_raw(TYPE_TODAY_TICKS, payload), request, lambda value: len(value.ticks)


def _parse_case(
    name: str,
    message_type: int,
    fixture: tuple[bytes, dict[str, Any], Callable[[Any], int]],
    records: int,
) -> dict[str, Any]:
    from eltdx.protocol import decode_response, parse_command_response

    raw, request, count_result = fixture

    def parse_once() -> int:
        frame = decode_response(raw)
        return count_result(parse_command_response(message_type, frame, request))

    for _ in range(5):
        if parse_once() != records:
            raise AssertionError(f"{name} warmup record count mismatch")
    gc.collect()
    rss_before = _rss_bytes()
    cpu_started = time.process_time_ns()
    wall_started = time.perf_counter_ns()
    latencies = []
    for _ in range(PARSE_ITERATIONS):
        started = time.perf_counter_ns()
        if parse_once() != records:
            raise AssertionError(f"{name} record count mismatch")
        latencies.append(time.perf_counter_ns() - started)
    wall_elapsed = time.perf_counter_ns() - wall_started
    cpu_elapsed = time.process_time_ns() - cpu_started
    rss_after = _rss_bytes()
    total_records = records * PARSE_ITERATIONS
    return {
        "name": name,
        "records_per_parse": records,
        "iterations": PARSE_ITERATIONS,
        "total_records": total_records,
        "elapsed_ns": wall_elapsed,
        "cpu_ns": cpu_elapsed,
        "rss_before_bytes": rss_before,
        "rss_after_bytes": rss_after,
        "rss_observed_bytes": max(rss_before, rss_after),
        "records_per_second": total_records * 1_000_000_000 / wall_elapsed,
        "latency_ns": latencies,
        **_sample_summary(latencies),
    }


def _lifecycle_case() -> dict[str, Any]:
    from eltdx.transport import SocketTransport

    start_latencies = []
    close_latencies = []
    with BenchmarkServer(response_delay=0) as server:
        for _ in range(LIFECYCLE_CYCLES):
            transport = SocketTransport([server.host], timeout=10, heartbeat_interval=None)
            started = time.perf_counter_ns()
            transport.connect()
            start_latencies.append(time.perf_counter_ns() - started)
            started = time.perf_counter_ns()
            transport.close()
            close_latencies.append(time.perf_counter_ns() - started)
    return {
        "cycles": LIFECYCLE_CYCLES,
        "start_latency_ns": start_latencies,
        "close_latency_ns": close_latencies,
        "start": _sample_summary(start_latencies),
        "close": _sample_summary(close_latencies),
    }


def _run_trial(role: str) -> dict[str, Any]:
    version = importlib.metadata.version("eltdx")
    native_abi = None
    if role == "current":
        from eltdx import _native
        from eltdx._native_abi import EXPECTED_NATIVE_ABI_VERSION

        native_abi = _native.ABI_VERSION
        if native_abi != EXPECTED_NATIVE_ABI_VERSION:
            raise AssertionError("benchmark current native ABI mismatch")
    started_at = _utc_now()
    single = _transport_case(1, 1, SINGLE_REQUESTS)
    pools = {
        str(pool_size): _transport_case(
            pool_size,
            pool_size * 25,
            POOL_REQUESTS,
        )
        for pool_size in POOL_SIZES
    }
    parsing = {
        "snapshots_100": _parse_case(
            "snapshots_100",
            TYPE_SNAPSHOTS,
            _snapshot_fixture(100),
            100,
        ),
        "snapshots_500": _parse_case(
            "snapshots_500",
            TYPE_SNAPSHOTS,
            _snapshot_fixture(500),
            500,
        ),
        "klines_800": _parse_case(
            "klines_800",
            TYPE_KLINES,
            _kline_fixture(800),
            800,
        ),
        "ticks_1800": _parse_case(
            "ticks_1800",
            TYPE_TODAY_TICKS,
            _ticks_fixture(1_800),
            1_800,
        ),
    }
    return {
        "schema": 1,
        "kind": "eltdx-native-performance-trial",
        "role": role,
        "package_version": version,
        "native_abi": native_abi,
        "system": platform.system(),
        "platform": platform.platform(),
        "python": platform.python_version(),
        "started_at_utc": started_at,
        "ended_at_utc": _utc_now(),
        "single_request": single,
        "pool_throughput": pools,
        "parsing": parsing,
        "lifecycle": _lifecycle_case(),
        "final_rss_bytes": _rss_bytes(),
    }


def _utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _git_head() -> str:
    return subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()


def _build_current_wheel(output_dir: Path) -> Path:
    output_dir.mkdir(parents=True, exist_ok=False)
    subprocess.run(
        [
            sys.executable,
            "-m",
            "maturin",
            "build",
            "--locked",
            "--release",
            "--out",
            str(output_dir),
        ],
        cwd=ROOT,
        check=True,
        timeout=1_800,
    )
    wheels = sorted(output_dir.glob("*.whl"))
    if len(wheels) != 1:
        raise AssertionError(f"benchmark build produced {len(wheels)} wheels")
    return wheels[0]


def _create_baseline_environment(root: Path, wheel: Path) -> Path:
    subprocess.run(
        [sys.executable, "-m", "venv", str(root)],
        check=True,
        timeout=300,
    )
    python = root / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
    subprocess.run(
        [str(python), "-m", "pip", "install", "--no-deps", str(wheel)],
        check=True,
        timeout=300,
    )
    return python


def _run_child(python: Path, role: str, output: Path) -> dict[str, Any]:
    environment = os.environ.copy()
    environment.pop("PYTHONHOME", None)
    environment.pop("PYTHONPATH", None)
    subprocess.run(
        [
            str(python),
            str(Path(__file__).resolve()),
            "--role",
            role,
            "--trial-output",
            str(output),
        ],
        cwd=ROOT,
        env=environment,
        check=True,
        timeout=1_800,
    )
    return json.loads(output.read_text(encoding="utf-8"))


def _artifact(path: Path) -> dict[str, Any]:
    return {
        "path": str(path.resolve()),
        "filename": path.name,
        "size_bytes": path.stat().st_size,
        "sha256": _sha256(path),
    }


def _run_campaign(baseline_wheel: Path, output: Path) -> dict[str, Any]:
    if not baseline_wheel.is_file():
        raise FileNotFoundError(f"baseline wheel does not exist: {baseline_wheel}")
    output.parent.mkdir(parents=True, exist_ok=True)
    current_wheel = _build_current_wheel(output.parent / "benchmark-current-wheel")
    with tempfile.TemporaryDirectory(prefix="eltdx-benchmark-") as temporary:
        temporary_root = Path(temporary)
        baseline_python = _create_baseline_environment(
            temporary_root / "baseline-venv",
            baseline_wheel,
        )
        trials = []
        for index, role in enumerate(SCHEDULE):
            python = baseline_python if role == "baseline" else Path(sys.executable)
            trials.append(
                _run_child(
                    python,
                    role,
                    temporary_root / f"trial-{index:02d}-{role}.json",
                )
            )
    return {
        "schema": 1,
        "kind": "eltdx-native-performance-campaign",
        "candidate_sha": _git_head(),
        "baseline_commit": BASELINE_COMMIT,
        "workload_sha256": _sha256(Path(__file__).resolve()),
        "system": platform.system(),
        "platform": platform.platform(),
        "python": platform.python_version(),
        "schedule": list(SCHEDULE),
        "baseline_wheel": _artifact(baseline_wheel),
        "current_wheel": _artifact(current_wheel),
        "trials": trials,
    }


def _write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=True, indent=2) + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--baseline-wheel", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--role", choices=("baseline", "current"))
    parser.add_argument("--trial-output", type=Path)
    args = parser.parse_args()
    if args.role is not None:
        if args.trial_output is None:
            parser.error("--role requires --trial-output")
        _write_json(args.trial_output, _run_trial(args.role))
        return 0
    if args.baseline_wheel is None or args.output is None:
        parser.error("campaign mode requires --baseline-wheel and --output")
    result = _run_campaign(args.baseline_wheel.resolve(), args.output.resolve())
    _write_json(args.output, result)
    print(f"native benchmark campaign written to {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
