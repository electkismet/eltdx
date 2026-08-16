"""Run the frozen eltdx 3.0 candidate through the mandatory ten rounds.

This orchestrator never pushes, tags, uploads, publishes, or triggers remote
workflows. Cross-platform results are explicit evidence gates supplied by the
operator after separate authorization.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Literal


PLAN_SHA256 = "d74a81850c90d9e781a47a5350203f9357e4a218a287f075acca6120b5e85e73"
BASELINE_COMMIT = "6486a1692dd4aca5339001b2de22e88bb29e16ec"
ROOT = Path(__file__).resolve().parents[1]
EVIDENCE_ROOT = ROOT / "artifacts" / "release-evidence"
EDITABLE_NATIVE_RELATIVE = Path(
    "src/eltdx/_native.pyd" if os.name == "nt" else "src/eltdx/_native.abi3.so"
)


@dataclass(frozen=True, slots=True)
class Step:
    name: str
    kind: Literal["command", "external_evidence"]
    argv: tuple[str, ...] = ()
    evidence_key: str | None = None


@dataclass(frozen=True, slots=True)
class Round:
    number: int
    name: str
    steps: tuple[Step, ...]


class ExternalEvidenceRequired(RuntimeError):
    """The candidate is intact but an authorized external result is absent."""


def command(name: str, *argv: str) -> Step:
    return Step(name=name, kind="command", argv=argv)


def gate(name: str, evidence_key: str) -> Step:
    return Step(name=name, kind="external_evidence", evidence_key=evidence_key)


ROUNDS = (
    Round(
        1,
        "source-and-compile",
        (
            command("cargo-fmt", "cargo", "fmt", "--check"),
            command("cargo-check", "cargo", "check", "--workspace", "--all-targets"),
            command(
                "cargo-clippy",
                "cargo",
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ),
            command("python-compileall", "{python}", "-m", "compileall", "-q", "src", "tests", "scripts"),
            command("python-ruff", "{python}", "-m", "ruff", "check", "."),
            command("python-mypy", "{python}", "-m", "mypy", "src/eltdx", "scripts", "tests"),
            command("maturin-develop", "{python}", "-m", "maturin", "develop", "--locked"),
            command("native-import-and-abi", "{python}", "scripts/verification/check_native_abi.py"),
        ),
    ),
    Round(
        2,
        "rust-unit-and-property",
        (
            command("cargo-test-workspace", "cargo", "test", "--workspace"),
            command("frame-fragmentation", "cargo", "test", "-p", "eltdx-protocol", "frame_decoder"),
            command("parser-properties", "cargo", "test", "-p", "eltdx-protocol", "parser_properties"),
            command("runtime-properties", "cargo", "test", "-p", "eltdx-runtime", "arbitrary_"),
            command("loom", "cargo", "test", "-p", "eltdx-runtime", "--features", "loom", "loom"),
            command("parser-fuzz-corpus", "cargo", "test", "-p", "eltdx-protocol", "fuzz_corpus"),
            command("panic-and-limits", "cargo", "test", "-p", "eltdx-protocol", "protocol_limits"),
        ),
    ),
    Round(
        3,
        "protocol-golden",
        (
            command("baseline-venv", "{python}", "-m", "venv", "{baseline_venv}"),
            command(
                "baseline-install",
                "{baseline_python}",
                "-m",
                "pip",
                "install",
                "{baseline_wheel_extra}",
            ),
            command(
                "fixture-workspace",
                "{python}",
                "scripts/fixtures/prepare_fixture_workspace.py",
                "--source",
                "tests/fixtures/7709",
                "--output",
                "{working_fixtures}",
            ),
            command(
                "baseline-export",
                "{baseline_python}",
                "scripts/fixtures/export_v205_baseline.py",
                "--wheel",
                "{baseline_wheel}",
                "--fixtures-root",
                "{working_fixtures}",
                "--contract-output",
                "{round_dir}/contracts.json",
            ),
            command(
                "golden-request-frames",
                "{python}",
                "-m",
                "pytest",
                "-q",
                "tests/native/test_differential.py::test_native_request_frames_match_v205_fixture",
            ),
            command(
                "golden-parse-results",
                "{python}",
                "-m",
                "pytest",
                "-q",
                "tests/native/test_differential.py::test_native_parse_results_match_v205_fixture",
            ),
            command(
                "golden-errors",
                "{python}",
                "-m",
                "pytest",
                "-q",
                "tests/native/test_differential.py::test_native_errors_match_v205_fixture",
            ),
            command(
                "golden-raw-and-precision",
                "{python}",
                "-m",
                "pytest",
                "-q",
                "tests/native/test_differential.py::test_native_raw_and_precision_match_v205_fixture",
            ),
            command(
                "protocol-facade",
                "{python}",
                "-m",
                "pytest",
                "-q",
                "tests/contracts/test_runtime_surfaces_contract.py",
                "-k",
                "protocol",
            ),
            command(
                "command-contracts",
                "{python}",
                "-m",
                "pytest",
                "-q",
                "tests/contracts/test_command_contract_manifest.py",
                "tests/native/test_differential.py::test_differential_matrix_covers_all_21_commands",
                "tests/native/test_differential.py::test_declared_tick_default_override_is_narrow_and_exact",
            ),
        ),
    ),
    Round(
        4,
        "python-public-contract",
        (
            command(
                "python-local-suite",
                "{python}",
                "scripts/verification/run_python_test_group.py",
                "round4-local",
            ),
        ),
    ),
    Round(
        5,
        "loopback-fault-injection",
        (
            command(
                "loopback-fault-injection",
                "{python}",
                "scripts/verification/run_python_test_group.py",
                "round5-loopback",
            ),
        ),
    ),
    Round(
        6,
        "stress-and-resources",
        (
            command(
                "local-stress",
                "{python}",
                "scripts/verification/run_python_test_group.py",
                "round6-stress",
            ),
            gate("windows-linux-long-running-evidence", "round6_windows_linux"),
            command(
                "stress-evidence-check",
                "{python}",
                "scripts/verification/check_external_evidence.py",
                "--candidate",
                "{candidate}",
                "--kind",
                "stress",
                "--evidence",
                "{evidence:round6_windows_linux}",
            ),
        ),
    ),
    Round(
        7,
        "performance",
        (
            command(
                "benchmark-v205-vs-native",
                "{python}",
                "scripts/benchmark_native.py",
                "--baseline-wheel",
                "{baseline_wheel}",
                "--output",
                "{round_dir}/benchmark.json",
            ),
            command(
                "benchmark-gates",
                "{python}",
                "scripts/verification/check_benchmark_gates.py",
                "{round_dir}/benchmark.json",
            ),
        ),
    ),
    Round(
        8,
        "real-host",
        (
            command(
                "real-host-21-commands",
                "{python}",
                "scripts/verification/run_python_test_group.py",
                "round8-real-host",
            ),
            command(
                "real-host-differential",
                "{python}",
                "scripts/validation/export_live_validation.py",
                "--baseline-wheel",
                "{baseline_wheel}",
                "--output",
                "{round_dir}/live-validation.json",
            ),
            command(
                "real-f10-7615",
                "{python}",
                "-m",
                "eltdx.f10_smoke",
                "--code",
                "000034",
                "--timeout",
                "8",
            ),
        ),
    ),
    Round(
        9,
        "wheel-and-install",
        (
            gate("five-wheel-and-sdist-evidence", "round9_artifacts"),
            command(
                "artifact-count-tags-and-hashes",
                "{python}",
                "scripts/verification/verify_release_artifacts.py",
                "--candidate",
                "{candidate}",
                "--artifact-dir",
                "{artifact_dir}",
                "--output",
                "{round_dir}/artifacts.json",
            ),
            command(
                "clean-install-matrix",
                "{python}",
                "scripts/verification/verify_wheel_matrix.py",
                "--artifact-dir",
                "{artifact_dir}",
            ),
            command(
                "sdist-build",
                "{python}",
                "scripts/verification/verify_sdist.py",
                "--artifact-dir",
                "{artifact_dir}",
            ),
        ),
    ),
    Round(
        10,
        "documentation-and-release",
        (
            command("mkdocs-strict", "{python}", "-m", "mkdocs", "build", "--strict"),
            command("pages-links", "{python}", "scripts/verification/check_pages_links.py", "site"),
            command("version-and-docs", "{python}", "scripts/verification/check_release_metadata.py"),
            command("release-text", "{python}", "scripts/verification/check_release_text.py"),
            command(
                "publish-workflow-dry-run-no-upload",
                "{python}",
                "scripts/verification/dry_run_publish_workflow.py",
                "--disable-uploads",
                "--disable-release",
            ),
            command(
                "evidence-index",
                "{python}",
                "scripts/verification/index_release_evidence.py",
                "--candidate",
                "{candidate}",
                "--evidence-root",
                "{evidence_root}",
            ),
        ),
    ),
)


def _now() -> str:
    return datetime.now(timezone.utc).isoformat()


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _git(*args: str) -> str:
    result = subprocess.run(
        ("git", *args),
        cwd=ROOT,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return result.stdout.strip()


def _state_allows_editable_native(state: dict[str, Any]) -> bool:
    completed = state.get("round_progress", {}).get("1", {}).get("completed_steps", ())
    return "maturin-develop" in completed


def _assert_candidate(candidate: str, *, allow_editable_native: bool = False) -> None:
    head = _git("rev-parse", "HEAD")
    if head != candidate:
        raise RuntimeError(f"candidate mismatch: state={candidate}, HEAD={head}")
    status = _git("status", "--porcelain", "--untracked-files=all")
    status_lines = status.splitlines()
    if allow_editable_native:
        editable_status = f"?? {EDITABLE_NATIVE_RELATIVE.as_posix()}"
        editable_path = ROOT / EDITABLE_NATIVE_RELATIVE
        status_lines = [
            line
            for line in status_lines
            if not (
                line == editable_status
                and editable_path.is_file()
                and not editable_path.is_symlink()
            )
        ]
    if status_lines:
        raise RuntimeError(f"candidate worktree is not clean:\n{os.linesep.join(status_lines)}")


def _candidate_dir(candidate: str) -> Path:
    return EVIDENCE_ROOT / candidate


def _state_path(candidate: str) -> Path:
    return _candidate_dir(candidate) / "state.json"


def _write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.tmp-{os.getpid()}")
    temporary.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    temporary.replace(path)


def _load_state(candidate: str) -> dict[str, Any]:
    path = _state_path(candidate)
    if not path.is_file():
        raise RuntimeError(f"unified-test state is not initialized: {path}")
    state = json.loads(path.read_text(encoding="utf-8"))
    if state["candidate"] != candidate:
        raise RuntimeError("unified-test state candidate does not match its directory")
    if state["plan_sha256"] != PLAN_SHA256:
        raise RuntimeError("unified-test state uses a different execution plan")
    return state


def initialize(candidate: str, *, attested_never_tested: bool) -> Path:
    _assert_candidate(candidate)
    if not attested_never_tested:
        raise RuntimeError("initialization requires the never-compiled-or-tested attestation")
    path = _state_path(candidate)
    if path.exists():
        raise FileExistsError(f"candidate already has unified-test state: {path}")
    state = {
        "schema_version": 1,
        "candidate": candidate,
        "baseline_commit": BASELINE_COMMIT,
        "plan_sha256": PLAN_SHA256,
        "attestation": "candidate had never been compiled or tested before unified round 1",
        "created_at": _now(),
        "updated_at": _now(),
        "next_round": 1,
        "completed_rounds": [],
        "active_round": None,
        "failure": None,
        "external_evidence": {},
        "round_progress": {},
    }
    _write_json(path, state)
    return path


def record_external_evidence(candidate: str, key: str, evidence: Path) -> None:
    state = _load_state(candidate)
    _assert_candidate(
        candidate,
        allow_editable_native=_state_allows_editable_native(state),
    )
    if state["failure"] is not None:
        raise RuntimeError("cannot add evidence to a failed candidate")
    allowed_keys = {
        step.evidence_key
        for round_spec in ROUNDS
        for step in round_spec.steps
        if step.kind == "external_evidence"
    }
    if key not in allowed_keys:
        raise ValueError(f"unknown external evidence key: {key}")
    evidence = evidence.resolve()
    if not evidence.is_file():
        raise FileNotFoundError(evidence)
    state["external_evidence"][key] = {
        "path": str(evidence),
        "sha256": _sha256(evidence),
        "recorded_at": _now(),
    }
    state["updated_at"] = _now()
    _write_json(_state_path(candidate), state)


def _context(
    *,
    candidate: str,
    round_dir: Path,
    state: dict[str, Any],
    baseline_wheel: Path | None,
    artifact_dir: Path | None,
) -> dict[str, str]:
    baseline_venv = _candidate_dir(candidate) / "baseline-venv"
    baseline_python = (
        baseline_venv / "Scripts" / "python.exe"
        if os.name == "nt"
        else baseline_venv / "bin" / "python"
    )
    values = {
        "python": sys.executable,
        "candidate": candidate,
        "round_dir": str(round_dir),
        "evidence_root": str(_candidate_dir(candidate)),
        "baseline_venv": str(baseline_venv),
        "baseline_python": str(baseline_python),
        "baseline_wheel": str(baseline_wheel) if baseline_wheel is not None else "",
        "baseline_wheel_extra": f"{baseline_wheel}[mcp]" if baseline_wheel is not None else "",
        "artifact_dir": str(artifact_dir) if artifact_dir is not None else "",
        "working_fixtures": str(round_dir / "fixtures"),
    }
    for key, evidence in state["external_evidence"].items():
        values[f"evidence:{key}"] = evidence["path"]
    return values


def _render(argv: tuple[str, ...], values: dict[str, str]) -> tuple[str, ...]:
    rendered = []
    for argument in argv:
        value = argument
        for key, replacement in values.items():
            value = value.replace("{" + key + "}", replacement)
        if "{" in value or "}" in value:
            raise RuntimeError(f"unresolved unified-test command placeholder: {value}")
        if value == "":
            raise RuntimeError(f"missing required unified-test command value in {argv!r}")
        rendered.append(value)
    return tuple(rendered)


def _check_gate(step: Step, state: dict[str, Any]) -> None:
    key = step.evidence_key
    if key is None or key not in state["external_evidence"]:
        raise ExternalEvidenceRequired(
            f"external evidence gate {step.name!r} is not satisfied; "
            f"record key {key!r} only after separate authorization and execution"
        )
    record = state["external_evidence"][key]
    path = Path(record["path"])
    if not path.is_file() or _sha256(path) != record["sha256"]:
        raise ExternalEvidenceRequired(
            f"external evidence changed or is missing for gate {step.name!r}"
        )


def _virtualenv_for_executable(executable: str) -> Path | None:
    path = Path(executable)
    if not path.is_absolute():
        return None
    virtualenv = path.parent.parent
    return virtualenv if (virtualenv / "pyvenv.cfg").is_file() else None


def _command_environment(argv: tuple[str, ...], *, working_fixtures: str) -> dict[str, str]:
    environment = dict(os.environ)
    environment.update(
        {
            "PYTHONHASHSEED": "0",
            "CARGO_TERM_COLOR": "never",
            "ELTDX_FIXTURES_ROOT": working_fixtures,
        }
    )
    virtualenv = _virtualenv_for_executable(argv[0])
    if virtualenv is not None:
        executable_dir = str(Path(argv[0]).parent)
        path_entries = [
            entry
            for entry in environment.get("PATH", "").split(os.pathsep)
            if entry and entry != executable_dir
        ]
        environment["VIRTUAL_ENV"] = str(virtualenv)
        environment["PATH"] = os.pathsep.join((executable_dir, *path_entries))
    return environment


def _run_command(argv: tuple[str, ...], log: Path, *, working_fixtures: str) -> int:
    environment = _command_environment(argv, working_fixtures=working_fixtures)
    with log.open("ab") as stream:
        stream.write(("$ " + " ".join(argv) + "\n").encode("utf-8"))
        stream.flush()
        result = subprocess.run(
            argv,
            cwd=ROOT,
            env=environment,
            stdout=stream,
            stderr=subprocess.STDOUT,
            check=False,
        )
        stream.write(f"[exit {result.returncode}]\n".encode("utf-8"))
    return result.returncode


def run_next_round(
    candidate: str,
    *,
    baseline_wheel: Path | None,
    artifact_dir: Path | None,
) -> int:
    state = _load_state(candidate)
    _assert_candidate(
        candidate,
        allow_editable_native=_state_allows_editable_native(state),
    )
    if state["failure"] is not None:
        raise RuntimeError("candidate has failed; fix, freeze, commit, and restart from round 1 with a new SHA")
    number = int(state["next_round"])
    if number > len(ROUNDS):
        return 0
    round_spec = ROUNDS[number - 1]
    if round_spec.number != number or state["completed_rounds"] != list(range(1, number)):
        raise RuntimeError("unified-test round state is not contiguous")
    serialized_commands = "\n".join(
        "\0".join(step.argv) for step in round_spec.steps if step.kind == "command"
    )
    if "{baseline_wheel}" in serialized_commands:
        if baseline_wheel is None or not baseline_wheel.is_file():
            raise RuntimeError(f"round {number} requires --baseline-wheel pointing to a local wheel")
    if "{artifact_dir}" in serialized_commands:
        if artifact_dir is None or not artifact_dir.is_dir():
            raise RuntimeError(f"round {number} requires --artifact-dir pointing to local artifacts")

    round_dir = _candidate_dir(candidate) / f"round-{number:02d}-{round_spec.name}"
    round_dir.mkdir(parents=True, exist_ok=True)
    context = _context(
        candidate=candidate,
        round_dir=round_dir,
        state=state,
        baseline_wheel=baseline_wheel.resolve() if baseline_wheel is not None else None,
        artifact_dir=artifact_dir.resolve() if artifact_dir is not None else None,
    )
    state["active_round"] = number
    progress = state["round_progress"].setdefault(str(number), {"completed_steps": []})
    state["updated_at"] = _now()
    _write_json(_state_path(candidate), state)

    for index, step in enumerate(round_spec.steps, start=1):
        if step.name in progress["completed_steps"]:
            continue
        try:
            _assert_candidate(
                candidate,
                allow_editable_native=_state_allows_editable_native(state),
            )
            if step.kind == "external_evidence":
                _check_gate(step, state)
            else:
                argv = _render(step.argv, context)
                log = round_dir / f"{index:02d}-{step.name}.log"
                returncode = _run_command(
                    argv,
                    log,
                    working_fixtures=context["working_fixtures"],
                )
                if returncode != 0:
                    raise RuntimeError(f"command failed with exit code {returncode}: {argv!r}")
        except ExternalEvidenceRequired:
            state["active_round"] = None
            state["updated_at"] = _now()
            _write_json(_state_path(candidate), state)
            raise
        except Exception as error:
            state["failure"] = {
                "round": number,
                "step": step.name,
                "message": str(error),
                "recorded_at": _now(),
            }
            state["active_round"] = None
            state["updated_at"] = _now()
            _write_json(_state_path(candidate), state)
            raise
        progress["completed_steps"].append(step.name)
        state["updated_at"] = _now()
        _write_json(_state_path(candidate), state)

    state["completed_rounds"].append(number)
    state["next_round"] = number + 1
    state["active_round"] = None
    state["updated_at"] = _now()
    _write_json(_state_path(candidate), state)
    return number


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Run frozen eltdx 3.0 unified-test rounds")
    subparsers = parser.add_subparsers(dest="action", required=True)

    initialize_parser = subparsers.add_parser("init")
    initialize_parser.add_argument("--candidate", required=True)
    initialize_parser.add_argument("--attest-never-tested", action="store_true")

    run_parser = subparsers.add_parser("run-next")
    run_parser.add_argument("--candidate", required=True)
    run_parser.add_argument("--baseline-wheel", type=Path)
    run_parser.add_argument("--artifact-dir", type=Path)

    evidence_parser = subparsers.add_parser("record-evidence")
    evidence_parser.add_argument("--candidate", required=True)
    evidence_parser.add_argument("--key", required=True)
    evidence_parser.add_argument("--file", type=Path, required=True)

    status_parser = subparsers.add_parser("status")
    status_parser.add_argument("--candidate", required=True)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    if args.action == "init":
        path = initialize(args.candidate, attested_never_tested=args.attest_never_tested)
        print(path)
        return 0
    if args.action == "record-evidence":
        record_external_evidence(args.candidate, args.key, args.file)
        return 0
    if args.action == "status":
        print(json.dumps(_load_state(args.candidate), ensure_ascii=False, indent=2))
        return 0
    completed = run_next_round(
        args.candidate,
        baseline_wheel=args.baseline_wheel,
        artifact_dir=args.artifact_dir,
    )
    print(json.dumps({"completed_round": completed, "candidate": args.candidate}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
