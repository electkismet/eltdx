"""Run or aggregate candidate-bound Windows/Linux stress evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


COMMAND = ["python", "-m", "pytest", "-q", "tests/stress"]


def _now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _canonical_sha256(value: Any) -> str:
    encoded = json.dumps(
        value,
        ensure_ascii=True,
        sort_keys=True,
        separators=(",", ":"),
    ).encode()
    return hashlib.sha256(encoded).hexdigest()


def _write(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=True, indent=2) + "\n", encoding="utf-8")


def run_stress(candidate: str, system: str, runner: str, output: Path) -> int:
    output.parent.mkdir(parents=True, exist_ok=True)
    log = output.with_name("stress.log")
    started_at = _now()
    started = time.monotonic()
    completed = subprocess.run(
        [sys.executable, "-m", "pytest", "-q", "tests/stress"],
        check=False,
        capture_output=True,
        text=True,
    )
    duration = time.monotonic() - started
    log.write_text(completed.stdout + completed.stderr, encoding="utf-8")
    run = {
        "system": system,
        "runner": runner,
        "candidate_sha": candidate,
        "command": COMMAND,
        "started_at_utc": started_at,
        "ended_at_utc": _now(),
        "duration_seconds": duration,
        "exit_code": completed.returncode,
        "status": "passed" if completed.returncode == 0 else "failed",
        "log_sha256": _sha256(log),
    }
    _write(output, run)
    print(completed.stdout + completed.stderr, end="")
    return completed.returncode


def aggregate(candidate: str, inputs: Path, output: Path) -> int:
    paths = sorted(inputs.rglob("stress-run.json"))
    if len(paths) != 2:
        raise ValueError(f"expected two stress run records, found {len(paths)}")
    runs = [json.loads(path.read_text(encoding="utf-8")) for path in paths]
    systems = {run.get("system") for run in runs}
    if systems != {"Linux", "Windows"}:
        raise ValueError(f"unexpected stress systems: {sorted(systems)!r}")
    if any(run.get("candidate_sha") != candidate for run in runs):
        raise ValueError("stress run candidate mismatch")
    evidence = {
        "schema": 1,
        "kind": "stress",
        "candidate_sha": candidate,
        "created_at_utc": _now(),
        "runs": sorted(runs, key=lambda run: run["system"]),
    }
    evidence["evidence_sha256"] = _canonical_sha256(evidence)
    _write(output, evidence)
    return 0 if all(run["status"] == "passed" for run in runs) else 1


def main() -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    run_parser = subparsers.add_parser("run")
    run_parser.add_argument("--candidate", required=True)
    run_parser.add_argument("--system", choices=("Linux", "Windows"), required=True)
    run_parser.add_argument("--runner", required=True)
    run_parser.add_argument("--output", type=Path, required=True)
    aggregate_parser = subparsers.add_parser("aggregate")
    aggregate_parser.add_argument("--candidate", required=True)
    aggregate_parser.add_argument("--inputs", type=Path, required=True)
    aggregate_parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "run":
        return run_stress(args.candidate, args.system, args.runner, args.output)
    return aggregate(args.candidate, args.inputs, args.output)


if __name__ == "__main__":
    raise SystemExit(main())
