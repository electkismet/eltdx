from __future__ import annotations

import json
from datetime import datetime, timedelta, timezone

from scripts import benchmark_native
from scripts.verification import (
    build_stress_evidence,
    check_benchmark_gates,
    check_external_evidence,
)


def _gate_trial(role: str) -> dict:
    is_current = role == "current"
    pool_elapsed = 100 if is_current else 120
    parse_elapsed = 100 if is_current else 120
    return {
        "role": role,
        "single_request": {"latency_ns": [100 if is_current else 100]},
        "pool_throughput": {
            pool: {"requests": 1, "elapsed_ns": pool_elapsed, "rss_observed_bytes": 1}
            for pool in check_benchmark_gates.EXPECTED_POOLS
        },
        "parsing": {
            name: {
                "total_records": count,
                "elapsed_ns": parse_elapsed,
                "rss_observed_bytes": 1,
            }
            for name, count in check_benchmark_gates.EXPECTED_PARSE_COUNTS.items()
        },
        "lifecycle": {"close_latency_ns": [1_000_000]},
        "final_rss_bytes": 1,
    }


def test_benchmark_protocol_freezes_all_plan_workloads() -> None:
    assert benchmark_native.SCHEDULE == ("baseline", "current", "current", "baseline")
    assert benchmark_native.POOL_SIZES == (1, 4, 8)
    assert benchmark_native.SINGLE_REQUESTS >= 2_000
    assert benchmark_native.POOL_REQUESTS >= 5_000
    assert benchmark_native.LIFECYCLE_CYCLES >= 100
    assert check_benchmark_gates.EXPECTED_PARSE_COUNTS == {
        "snapshots_100": 100,
        "snapshots_500": 500,
        "klines_800": 800,
        "ticks_1800": 1_800,
    }


def test_benchmark_gate_thresholds_are_not_report_only() -> None:
    bundle = {
        "candidate_sha": "1" * 40,
        "baseline_wheel": {"size_bytes": 10},
        "current_wheel": {"size_bytes": 20},
        "trials": [
            _gate_trial("baseline"),
            _gate_trial("current"),
            _gate_trial("current"),
            _gate_trial("baseline"),
        ],
    }
    report = check_benchmark_gates._gate_report(bundle, [])

    assert report["passed"]
    required = {gate["gate"] for gate in report["gates"] if not gate.get("report_only")}
    assert required == {
        "single_request_p95",
        "pool_1_throughput",
        "pool_4_throughput",
        "pool_8_throughput",
        "snapshots_100_parse_gain",
        "snapshots_500_parse_gain",
        "klines_800_parse_gain",
        "ticks_1800_parse_gain",
        "close_p99_hard_limit",
        "rss_bounded",
    }

    bundle["trials"][1]["single_request"]["latency_ns"] = [106]
    bundle["trials"][2]["single_request"]["latency_ns"] = [106]
    failed = check_benchmark_gates._gate_report(bundle, [])
    p95 = next(gate for gate in failed["gates"] if gate["gate"] == "single_request_p95")
    assert not p95["passed"]
    assert not failed["passed"]


def test_external_stress_evidence_requires_both_candidate_bound_systems() -> None:
    candidate = "2" * 40
    ended = datetime(2026, 8, 15, 1, 0, tzinfo=timezone.utc)
    runs = []
    for system in ("Linux", "Windows"):
        started = ended - timedelta(seconds=30)
        runs.append(
            {
                "system": system,
                "runner": f"{system.lower()}-runner",
                "candidate_sha": candidate,
                "command": list(check_external_evidence.EXPECTED_STRESS_COMMAND),
                "started_at_utc": started.isoformat().replace("+00:00", "Z"),
                "ended_at_utc": ended.isoformat().replace("+00:00", "Z"),
                "duration_seconds": 30,
                "exit_code": 0,
                "status": "passed",
                "log_sha256": "3" * 64,
            }
        )
    evidence = {
        "schema": 1,
        "kind": "stress",
        "candidate_sha": candidate,
        "created_at_utc": ended.isoformat().replace("+00:00", "Z"),
        "runs": runs,
    }
    evidence["evidence_sha256"] = check_external_evidence._canonical_sha256(evidence)

    assert check_external_evidence._validate_stress(evidence, candidate) == []
    evidence["runs"] = evidence["runs"][:1]
    assert check_external_evidence._validate_stress(evidence, candidate)


def test_stress_evidence_aggregator_emits_the_checker_schema(tmp_path) -> None:
    candidate = "4" * 40
    inputs = tmp_path / "inputs"
    for system in ("Linux", "Windows"):
        run_dir = inputs / system
        run_dir.mkdir(parents=True)
        run = {
            "system": system,
            "runner": system.lower(),
            "candidate_sha": candidate,
            "command": list(build_stress_evidence.COMMAND),
            "started_at_utc": "2026-08-15T01:00:00Z",
            "ended_at_utc": "2026-08-15T01:00:30Z",
            "duration_seconds": 30,
            "exit_code": 0,
            "status": "passed",
            "log_sha256": "5" * 64,
        }
        (run_dir / "stress-run.json").write_text(json.dumps(run), encoding="utf-8")
    output = tmp_path / "stress-evidence.json"

    assert build_stress_evidence.aggregate(candidate, inputs, output) == 0
    evidence = json.loads(output.read_text(encoding="utf-8"))
    assert check_external_evidence._validate_stress(evidence, candidate) == []
