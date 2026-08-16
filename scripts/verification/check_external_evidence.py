"""Validate candidate-bound external evidence without contacting remote systems."""

from __future__ import annotations

import argparse
import hashlib
import json
from datetime import datetime, timedelta
from pathlib import Path
from typing import Any


EXPECTED_STRESS_SYSTEMS = {"Linux", "Windows"}
EXPECTED_STRESS_COMMAND = [
    "python",
    "scripts/verification/run_python_test_group.py",
    "round6-stress",
]


def _parse_utc(value: Any) -> datetime:
    if not isinstance(value, str):
        raise ValueError("timestamp is not a string")
    parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    if parsed.tzinfo is None or parsed.utcoffset() != timedelta(0):
        raise ValueError("timestamp must use UTC")
    return parsed


def _is_sha256(value: Any) -> bool:
    return (
        isinstance(value, str)
        and len(value) == 64
        and value == value.lower()
        and value != "0" * 64
        and all(character in "0123456789abcdef" for character in value)
    )


def _canonical_sha256(value: Any) -> str:
    encoded = json.dumps(
        value,
        ensure_ascii=True,
        sort_keys=True,
        separators=(",", ":"),
    ).encode()
    return hashlib.sha256(encoded).hexdigest()


def _validate_stress(bundle: Any, candidate: str) -> list[str]:
    errors: list[str] = []
    if not isinstance(bundle, dict):
        return ["evidence must be a JSON object"]
    expected_keys = {
        "schema",
        "kind",
        "candidate_sha",
        "created_at_utc",
        "runs",
        "evidence_sha256",
    }
    if set(bundle) != expected_keys:
        errors.append("evidence keys differ from the frozen stress schema")
    if bundle.get("schema") != 1 or bundle.get("kind") != "stress":
        errors.append("unexpected stress evidence schema or kind")
    if bundle.get("candidate_sha") != candidate:
        errors.append("stress evidence candidate SHA mismatch")
    try:
        created_at = _parse_utc(bundle.get("created_at_utc"))
    except ValueError as error:
        errors.append(f"created_at_utc: {error}")
        created_at = None
    hash_payload = dict(bundle)
    declared_hash = hash_payload.pop("evidence_sha256", None)
    if not _is_sha256(declared_hash) or declared_hash != _canonical_sha256(hash_payload):
        errors.append("stress evidence SHA256 mismatch")
    runs = bundle.get("runs")
    if not isinstance(runs, list):
        errors.append("stress evidence runs must be a list")
        return errors
    systems: set[str] = set()
    for index, run in enumerate(runs):
        location = f"runs[{index}]"
        if not isinstance(run, dict):
            errors.append(f"{location}: expected an object")
            continue
        expected_run_keys = {
            "system",
            "runner",
            "candidate_sha",
            "command",
            "started_at_utc",
            "ended_at_utc",
            "duration_seconds",
            "exit_code",
            "status",
            "log_sha256",
        }
        if set(run) != expected_run_keys:
            errors.append(f"{location}: keys differ from the frozen run schema")
        system = run.get("system")
        if not isinstance(system, str):
            errors.append(f"{location}: system is missing")
        elif system in systems:
            errors.append(f"{location}: duplicate system {system}")
        else:
            systems.add(system)
        if run.get("candidate_sha") != candidate:
            errors.append(f"{location}: candidate SHA mismatch")
        if run.get("command") != EXPECTED_STRESS_COMMAND:
            errors.append(f"{location}: command differs from the frozen stress command")
        if run.get("status") != "passed" or run.get("exit_code") != 0:
            errors.append(f"{location}: stress run did not pass")
        if not isinstance(run.get("runner"), str) or not run["runner"]:
            errors.append(f"{location}: runner identity is missing")
        duration = run.get("duration_seconds")
        if (
            isinstance(duration, bool)
            or not isinstance(duration, (int, float))
            or duration <= 0
        ):
            errors.append(f"{location}: duration_seconds must be positive")
        if not _is_sha256(run.get("log_sha256")):
            errors.append(f"{location}: log_sha256 is invalid")
        try:
            started = _parse_utc(run.get("started_at_utc"))
            ended = _parse_utc(run.get("ended_at_utc"))
            if ended < started:
                errors.append(f"{location}: timestamps are reversed")
            if created_at is not None and ended > created_at:
                errors.append(f"{location}: run ends after evidence creation")
            if isinstance(duration, (int, float)) and not isinstance(duration, bool):
                actual_duration = (ended - started).total_seconds()
                if abs(float(duration) - actual_duration) > 1.0:
                    errors.append(f"{location}: duration_seconds differs from timestamps")
        except ValueError as error:
            errors.append(f"{location}: {error}")
    if systems != EXPECTED_STRESS_SYSTEMS:
        errors.append(
            f"stress systems={sorted(systems)!r}, "
            f"expected {sorted(EXPECTED_STRESS_SYSTEMS)!r}"
        )
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--kind", choices=("stress",), required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    args = parser.parse_args()
    if not args.evidence.is_file():
        raise FileNotFoundError(f"external evidence does not exist: {args.evidence}")
    bundle = json.loads(args.evidence.read_text(encoding="utf-8"))
    errors = _validate_stress(bundle, args.candidate)
    result = {
        "schema": 1,
        "kind": args.kind,
        "candidate_sha": args.candidate,
        "passed": not errors,
        "errors": errors,
    }
    print(json.dumps(result, ensure_ascii=True, indent=2))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
