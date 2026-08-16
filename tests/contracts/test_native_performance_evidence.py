from __future__ import annotations

import json
from datetime import datetime, timedelta, timezone
from pathlib import Path
from types import SimpleNamespace

import pytest

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
        "single_request": {
            "latency_ns": [100 if is_current else 100],
            "rss_observed_bytes": 1,
        },
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


def test_benchmark_snapshot_record_matches_frozen_fixture() -> None:
    fixture = (
        Path(__file__).parents[1]
        / "fixtures"
        / "7709"
        / "snapshots"
        / "normal"
        / "response.bin"
    )
    response = fixture.read_bytes()

    assert len(response) == 124
    assert len(benchmark_native.SNAPSHOT_RECORD) == 104
    assert benchmark_native.SNAPSHOT_RECORD == response[20:]


def test_benchmark_waits_for_cached_diagnostics_to_publish_idle_state() -> None:
    busy = SimpleNamespace(active_leases=8, waiter_count=176, pin_waiter_count=0)
    idle = SimpleNamespace(active_leases=0, waiter_count=0, pin_waiter_count=0)

    class Transport:
        reads = 0

        @property
        def diagnostics(self):
            self.reads += 1
            return SimpleNamespace(broker=busy if self.reads == 1 else idle)

    transport = Transport()
    diagnostics = benchmark_native._wait_for_idle_diagnostics(transport)

    assert transport.reads == 2
    assert diagnostics.broker is idle


def test_benchmark_campaign_runs_both_release_wheels_in_isolated_environments(
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
) -> None:
    baseline_wheel = tmp_path / "eltdx-2.0.5.whl"
    current_wheel = tmp_path / "eltdx-3.0.0.whl"
    baseline_wheel.write_bytes(b"baseline")
    current_wheel.write_bytes(b"current")
    environments: list[tuple[Path, Path]] = []
    children: list[tuple[Path, str]] = []

    monkeypatch.setattr(
        benchmark_native,
        "_build_current_wheel",
        lambda _output: current_wheel,
    )

    def create_environment(root: Path, wheel: Path) -> Path:
        environments.append((root, wheel))
        return root / "bin" / "python"

    def run_child(python: Path, role: str, _output: Path) -> dict:
        children.append((python, role))
        return {"role": role}

    monkeypatch.setattr(benchmark_native, "_create_wheel_environment", create_environment)
    monkeypatch.setattr(benchmark_native, "_run_child", run_child)
    monkeypatch.setattr(benchmark_native, "_git_head", lambda: "1" * 40)

    result = benchmark_native._run_campaign(baseline_wheel, tmp_path / "benchmark.json")

    assert [wheel for _, wheel in environments] == [baseline_wheel, current_wheel]
    interpreters = {role: python for python, role in children}
    assert interpreters["baseline"] != interpreters["current"]
    assert result["schedule"] == list(benchmark_native.SCHEDULE)


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


def test_benchmark_gate_allows_only_the_editable_native(
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
) -> None:
    relative = check_benchmark_gates.EDITABLE_NATIVE_RELATIVE
    native = tmp_path / relative
    native.parent.mkdir(parents=True)
    native.write_bytes(b"native")
    monkeypatch.setattr(check_benchmark_gates, "ROOT", tmp_path)

    status = f"?? {relative.as_posix()}"
    assert check_benchmark_gates._unexpected_worktree_status(status) == []

    native.unlink()
    assert check_benchmark_gates._unexpected_worktree_status(status) == [status]

    native.write_bytes(b"native")
    unexpected = "?? unexpected.txt"
    assert check_benchmark_gates._unexpected_worktree_status(
        f"{status}\n{unexpected}"
    ) == [unexpected]


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
