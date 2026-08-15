"""Validate native benchmark evidence and enforce the 3.0 performance gates."""

from __future__ import annotations

import argparse
import hashlib
import json
import statistics
import subprocess
from collections.abc import Mapping, Sequence
from datetime import datetime, timedelta
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
EXPECTED_SCHEDULE = ["baseline", "current", "current", "baseline"]
EXPECTED_BASELINE_COMMIT = "6486a1692dd4aca5339001b2de22e88bb29e16ec"
EXPECTED_POOLS = {"1", "4", "8"}
EXPECTED_PARSERS = {"snapshots_100", "snapshots_500", "klines_800", "ticks_1800"}
EXPECTED_PARSE_COUNTS = {
    "snapshots_100": 100,
    "snapshots_500": 500,
    "klines_800": 800,
    "ticks_1800": 1_800,
}
EXPECTED_TRIAL_KEYS = {
    "schema",
    "kind",
    "role",
    "package_version",
    "native_abi",
    "system",
    "platform",
    "python",
    "started_at_utc",
    "ended_at_utc",
    "single_request",
    "pool_throughput",
    "parsing",
    "lifecycle",
    "final_rss_bytes",
}


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _git_output(*arguments: str) -> str:
    return subprocess.run(
        ["git", *arguments],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()


def _is_positive_int(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool) and value > 0


def _percentile(values: Sequence[int], numerator: int, denominator: int) -> int:
    ordered = sorted(values)
    return ordered[((len(ordered) - 1) * numerator) // denominator]


def _parse_utc(value: Any) -> datetime:
    if not isinstance(value, str):
        raise ValueError("timestamp is not a string")
    parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    if parsed.tzinfo is None or parsed.utcoffset() != timedelta(0):
        raise ValueError("timestamp must use UTC")
    return parsed


def _validate_samples(
    value: Any,
    *,
    expected_count: int | None,
    location: str,
    errors: list[str],
) -> list[int]:
    if not isinstance(value, Mapping):
        errors.append(f"{location}: expected an object")
        return []
    samples = value.get("latency_ns")
    if not isinstance(samples, list) or not samples:
        errors.append(f"{location}: latency_ns must be a non-empty list")
        return []
    if expected_count is not None and len(samples) != expected_count:
        errors.append(
            f"{location}: sample count={len(samples)}, expected {expected_count}"
        )
    if any(not _is_positive_int(sample) for sample in samples):
        errors.append(f"{location}: latency_ns contains an invalid sample")
        return []
    expected = {
        "samples": len(samples),
        "p50_ns": int(statistics.median(samples)),
        "p95_ns": _percentile(samples, 95, 100),
        "p99_ns": _percentile(samples, 99, 100),
        "min_ns": min(samples),
        "max_ns": max(samples),
    }
    for field, expected_value in expected.items():
        if value.get(field) != expected_value:
            errors.append(
                f"{location}: {field}={value.get(field)!r}, expected {expected_value}"
            )
    for field in (
        "elapsed_ns",
        "cpu_ns",
        "rss_before_bytes",
        "rss_after_bytes",
        "rss_observed_bytes",
    ):
        if not _is_positive_int(value.get(field)):
            errors.append(f"{location}: {field} must be a positive integer")
    rss_values = (value.get("rss_before_bytes"), value.get("rss_after_bytes"))
    if all(_is_positive_int(item) for item in rss_values):
        if value.get("rss_observed_bytes") != max(rss_values):
            errors.append(f"{location}: rss_observed_bytes is not the observed maximum")
    return samples


def _validate_transport_case(
    case: Any,
    *,
    pool_size: int,
    requests: int,
    location: str,
    errors: list[str],
) -> None:
    samples = _validate_samples(
        case,
        expected_count=requests,
        location=location,
        errors=errors,
    )
    if not isinstance(case, Mapping):
        return
    expected = {
        "pool_size": pool_size,
        "requests": requests,
        "server_requests": requests + 200,
    }
    for field, expected_value in expected.items():
        if case.get(field) != expected_value:
            errors.append(
                f"{location}: {field}={case.get(field)!r}, expected {expected_value}"
            )
    concurrency = case.get("concurrency")
    if not _is_positive_int(concurrency):
        errors.append(f"{location}: concurrency must be positive")
    connections = case.get("server_connections")
    if not _is_positive_int(connections) or connections != pool_size:
        errors.append(f"{location}: server_connections must equal pool_size")
    elapsed = case.get("elapsed_ns")
    throughput = case.get("throughput_rps")
    if _is_positive_int(elapsed) and isinstance(throughput, (int, float)):
        expected_throughput = requests * 1_000_000_000 / elapsed
        if abs(float(throughput) - expected_throughput) > expected_throughput * 1e-12:
            errors.append(f"{location}: throughput_rps is not derived from elapsed_ns")
    if samples and _is_positive_int(elapsed) and max(samples) > elapsed:
        errors.append(f"{location}: a latency sample exceeds the measured interval")


def _validate_parse_case(
    case: Any,
    *,
    name: str,
    location: str,
    errors: list[str],
) -> None:
    if not isinstance(case, Mapping):
        errors.append(f"{location}: expected an object")
        return
    iterations = case.get("iterations")
    records = EXPECTED_PARSE_COUNTS[name]
    if not _is_positive_int(iterations):
        errors.append(f"{location}: iterations must be positive")
        return
    _validate_samples(
        case,
        expected_count=iterations,
        location=location,
        errors=errors,
    )
    total_records = records * iterations
    expected = {
        "name": name,
        "records_per_parse": records,
        "total_records": total_records,
    }
    for field, expected_value in expected.items():
        if case.get(field) != expected_value:
            errors.append(
                f"{location}: {field}={case.get(field)!r}, expected {expected_value}"
            )
    elapsed = case.get("elapsed_ns")
    rate = case.get("records_per_second")
    if _is_positive_int(elapsed) and isinstance(rate, (int, float)):
        expected_rate = total_records * 1_000_000_000 / elapsed
        if abs(float(rate) - expected_rate) > expected_rate * 1e-12:
            errors.append(f"{location}: records_per_second is not derived from elapsed_ns")


def _validate_lifecycle(value: Any, location: str, errors: list[str]) -> None:
    if not isinstance(value, Mapping):
        errors.append(f"{location}: expected an object")
        return
    cycles = value.get("cycles")
    if not _is_positive_int(cycles):
        errors.append(f"{location}: cycles must be positive")
        return
    for sample_field, summary_field in (
        ("start_latency_ns", "start"),
        ("close_latency_ns", "close"),
    ):
        samples = value.get(sample_field)
        summary = value.get(summary_field)
        if not isinstance(samples, list) or len(samples) != cycles:
            errors.append(f"{location}: {sample_field} count must equal cycles")
            continue
        if any(not _is_positive_int(sample) for sample in samples):
            errors.append(f"{location}: {sample_field} contains an invalid sample")
            continue
        if not isinstance(summary, Mapping):
            errors.append(f"{location}: {summary_field} summary is missing")
            continue
        expected = {
            "samples": cycles,
            "p50_ns": int(statistics.median(samples)),
            "p95_ns": _percentile(samples, 95, 100),
            "p99_ns": _percentile(samples, 99, 100),
            "min_ns": min(samples),
            "max_ns": max(samples),
        }
        for field, expected_value in expected.items():
            if summary.get(field) != expected_value:
                errors.append(
                    f"{location}.{summary_field}: {field} does not match samples"
                )


def _validate_artifact(value: Any, location: str, errors: list[str]) -> None:
    if not isinstance(value, Mapping):
        errors.append(f"{location}: expected an object")
        return
    path_value = value.get("path")
    if not isinstance(path_value, str):
        errors.append(f"{location}: path is missing")
        return
    path = Path(path_value)
    if not path.is_file():
        errors.append(f"{location}: artifact does not exist: {path}")
        return
    if value.get("filename") != path.name:
        errors.append(f"{location}: filename mismatch")
    if value.get("size_bytes") != path.stat().st_size:
        errors.append(f"{location}: size mismatch")
    if value.get("sha256") != _sha256(path):
        errors.append(f"{location}: SHA256 mismatch")
    if location == "current_wheel" and "-cp310-abi3-" not in path.name:
        errors.append("current_wheel: filename is not a cp310-abi3 wheel")


def _validate_campaign(bundle: Any) -> list[str]:
    errors: list[str] = []
    if not isinstance(bundle, Mapping):
        return ["bundle: expected an object"]
    if bundle.get("schema") != 1 or bundle.get("kind") != "eltdx-native-performance-campaign":
        errors.append("bundle: unexpected schema or kind")
    if bundle.get("schedule") != EXPECTED_SCHEDULE:
        errors.append("bundle: schedule differs from the frozen order")
    candidate = bundle.get("candidate_sha")
    if (
        not isinstance(candidate, str)
        or len(candidate) != 40
        or any(character not in "0123456789abcdef" for character in candidate)
    ):
        errors.append("bundle: candidate SHA is invalid")
    if candidate != _git_output("rev-parse", "HEAD"):
        errors.append("bundle: candidate SHA does not match HEAD")
    if bundle.get("baseline_commit") != EXPECTED_BASELINE_COMMIT:
        errors.append("bundle: baseline commit mismatch")
    if _git_output("status", "--porcelain"):
        errors.append("bundle: candidate worktree is dirty")
    benchmark = ROOT / "scripts" / "benchmark_native.py"
    if bundle.get("workload_sha256") != _sha256(benchmark):
        errors.append("bundle: workload SHA256 does not match benchmark_native.py")
    _validate_artifact(bundle.get("baseline_wheel"), "baseline_wheel", errors)
    _validate_artifact(bundle.get("current_wheel"), "current_wheel", errors)
    trials = bundle.get("trials")
    if not isinstance(trials, list) or len(trials) != len(EXPECTED_SCHEDULE):
        errors.append("bundle: trial count differs from the schedule")
        return errors
    for index, (trial, role) in enumerate(zip(trials, EXPECTED_SCHEDULE)):
        location = f"trial[{index}]"
        if not isinstance(trial, Mapping):
            errors.append(f"{location}: expected an object")
            continue
        if set(trial) != EXPECTED_TRIAL_KEYS:
            errors.append(f"{location}: keys differ from the frozen trial schema")
        if trial.get("schema") != 1 or trial.get("kind") != "eltdx-native-performance-trial":
            errors.append(f"{location}: unexpected schema or kind")
        if trial.get("role") != role:
            errors.append(f"{location}: role differs from schedule")
        expected_version = "2.0.5" if role == "baseline" else "3.0.0a1"
        if trial.get("package_version") != expected_version:
            errors.append(f"{location}: package version must be {expected_version}")
        expected_abi = None if role == "baseline" else 1
        if trial.get("native_abi") != expected_abi:
            errors.append(f"{location}: native ABI mismatch")
        for field in ("system", "platform", "python"):
            if trial.get(field) != bundle.get(field):
                errors.append(f"{location}: {field} differs from campaign host")
        try:
            started = _parse_utc(trial.get("started_at_utc"))
            ended = _parse_utc(trial.get("ended_at_utc"))
            if ended < started:
                errors.append(f"{location}: timestamps are reversed")
        except ValueError as error:
            errors.append(f"{location}: {error}")
        _validate_transport_case(
            trial.get("single_request"),
            pool_size=1,
            requests=2_000,
            location=f"{location}.single_request",
            errors=errors,
        )
        pools = trial.get("pool_throughput")
        if not isinstance(pools, Mapping) or set(pools) != EXPECTED_POOLS:
            errors.append(f"{location}: pool case names differ")
        else:
            for pool_name, case in pools.items():
                _validate_transport_case(
                    case,
                    pool_size=int(pool_name),
                    requests=5_000,
                    location=f"{location}.pool_throughput.{pool_name}",
                    errors=errors,
                )
        parsing = trial.get("parsing")
        if not isinstance(parsing, Mapping) or set(parsing) != EXPECTED_PARSERS:
            errors.append(f"{location}: parser case names differ")
        else:
            for name, case in parsing.items():
                _validate_parse_case(
                    case,
                    name=name,
                    location=f"{location}.parsing.{name}",
                    errors=errors,
                )
        _validate_lifecycle(trial.get("lifecycle"), f"{location}.lifecycle", errors)
        if not _is_positive_int(trial.get("final_rss_bytes")):
            errors.append(f"{location}: final_rss_bytes must be positive")
    return errors


def _role_trials(bundle: Mapping[str, Any], role: str) -> list[Mapping[str, Any]]:
    return [trial for trial in bundle["trials"] if trial["role"] == role]


def _combined_samples(
    trials: Sequence[Mapping[str, Any]],
    *path: str,
) -> list[int]:
    output: list[int] = []
    for trial in trials:
        value: Any = trial
        for key in path:
            value = value[key]
        output.extend(value)
    return output


def _aggregate_rate(
    trials: Sequence[Mapping[str, Any]],
    *path: str,
    count_field: str,
) -> float:
    count = 0
    elapsed = 0
    for trial in trials:
        value: Any = trial
        for key in path:
            value = value[key]
        count += value[count_field]
        elapsed += value["elapsed_ns"]
    return count * 1_000_000_000 / elapsed


def _rss_peak(trials: Sequence[Mapping[str, Any]]) -> int:
    values = []
    for trial in trials:
        values.append(trial["final_rss_bytes"])
        values.append(trial["single_request"]["rss_observed_bytes"])
        values.extend(case["rss_observed_bytes"] for case in trial["pool_throughput"].values())
        values.extend(case["rss_observed_bytes"] for case in trial["parsing"].values())
    return max(values)


def _gate_report(bundle: Mapping[str, Any], errors: list[str]) -> dict[str, Any]:
    gates: list[dict[str, Any]] = []
    if not errors:
        baseline = _role_trials(bundle, "baseline")
        current = _role_trials(bundle, "current")
        baseline_p95 = _percentile(
            _combined_samples(baseline, "single_request", "latency_ns"),
            95,
            100,
        )
        current_p95 = _percentile(
            _combined_samples(current, "single_request", "latency_ns"),
            95,
            100,
        )
        gates.append(
            {
                "gate": "single_request_p95",
                "passed": current_p95 <= baseline_p95 * 1.05,
                "baseline_ns": baseline_p95,
                "current_ns": current_p95,
                "maximum_ratio": 1.05,
            }
        )
        for pool_size in sorted(EXPECTED_POOLS):
            baseline_rate = _aggregate_rate(
                baseline,
                "pool_throughput",
                pool_size,
                count_field="requests",
            )
            current_rate = _aggregate_rate(
                current,
                "pool_throughput",
                pool_size,
                count_field="requests",
            )
            gates.append(
                {
                    "gate": f"pool_{pool_size}_throughput",
                    "passed": current_rate >= baseline_rate,
                    "baseline_rps": baseline_rate,
                    "current_rps": current_rate,
                    "minimum_ratio": 1.0,
                }
            )
        for name in sorted(EXPECTED_PARSERS):
            baseline_rate = _aggregate_rate(
                baseline,
                "parsing",
                name,
                count_field="total_records",
            )
            current_rate = _aggregate_rate(
                current,
                "parsing",
                name,
                count_field="total_records",
            )
            minimum_ratio = 1.05 if name == "snapshots_100" else 1.10
            gates.append(
                {
                    "gate": f"{name}_parse_gain",
                    "passed": current_rate >= baseline_rate * minimum_ratio,
                    "baseline_records_per_second": baseline_rate,
                    "current_records_per_second": current_rate,
                    "minimum_ratio": minimum_ratio,
                }
            )
        close_values = _combined_samples(current, "lifecycle", "close_latency_ns")
        close_p99 = _percentile(close_values, 99, 100)
        gates.append(
            {
                "gate": "close_p99_hard_limit",
                "passed": close_p99 <= 1_000_000_000,
                "current_ns": close_p99,
                "maximum_ns": 1_000_000_000,
            }
        )
        baseline_rss = _rss_peak(baseline)
        current_rss = _rss_peak(current)
        rss_limit = max(baseline_rss + 64 * 1024 * 1024, int(baseline_rss * 1.5))
        gates.append(
            {
                "gate": "rss_bounded",
                "passed": current_rss <= rss_limit,
                "baseline_bytes": baseline_rss,
                "current_bytes": current_rss,
                "maximum_bytes": rss_limit,
            }
        )
        gates.append(
            {
                "gate": "wheel_size_report",
                "passed": True,
                "report_only": True,
                "baseline_bytes": bundle["baseline_wheel"]["size_bytes"],
                "current_bytes": bundle["current_wheel"]["size_bytes"],
            }
        )
    return {
        "schema": 1,
        "kind": "eltdx-native-performance-gates",
        "candidate_sha": bundle.get("candidate_sha"),
        "passed": not errors and all(gate["passed"] for gate in gates),
        "errors": errors,
        "gates": gates,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("benchmark", type=Path)
    args = parser.parse_args()
    bundle = json.loads(args.benchmark.read_text(encoding="utf-8"))
    errors = _validate_campaign(bundle)
    report = _gate_report(bundle, errors)
    report_path = args.benchmark.with_name("benchmark-gates.json")
    report_path.write_text(
        json.dumps(report, ensure_ascii=True, indent=2) + "\n",
        encoding="utf-8",
    )
    print(json.dumps(report, ensure_ascii=True, indent=2))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
