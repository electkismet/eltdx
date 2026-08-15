"""Static and state-machine contracts for the ten-round unified test runner."""

from __future__ import annotations

import inspect
import os
import re
from pathlib import Path

import pytest

from scripts import unified_test
from scripts.fixtures import prepare_fixture_workspace


def test_unified_test_has_exact_ten_round_order() -> None:
    assert [(round_spec.number, round_spec.name) for round_spec in unified_test.ROUNDS] == [
        (1, "source-and-compile"),
        (2, "rust-unit-and-property"),
        (3, "protocol-golden"),
        (4, "python-public-contract"),
        (5, "loopback-fault-injection"),
        (6, "stress-and-resources"),
        (7, "performance"),
        (8, "real-host"),
        (9, "wheel-and-install"),
        (10, "documentation-and-release"),
    ]


def test_first_compile_and_first_import_exist_only_in_round_one() -> None:
    commands = {
        round_spec.number: [step.argv for step in round_spec.steps if step.kind == "command"]
        for round_spec in unified_test.ROUNDS
    }
    assert commands[1][0] == ("cargo", "fmt", "--check")
    assert commands[1][1] == ("cargo", "check", "--workspace", "--all-targets")
    assert any("maturin" in command for command in commands[1])
    assert ("{python}", "scripts/verification/check_native_abi.py") in commands[1]
    for number in range(2, 11):
        assert all("cargo check" not in " ".join(command) for command in commands[number])
        assert all("maturin develop" not in " ".join(command) for command in commands[number])


def test_each_round_preserves_plan_step_order() -> None:
    names = {
        round_spec.number: [step.name for step in round_spec.steps]
        for round_spec in unified_test.ROUNDS
    }
    assert names[1][:3] == ["cargo-fmt", "cargo-check", "cargo-clippy"]
    assert names[2] == [
        "cargo-test-workspace",
        "frame-fragmentation",
        "parser-properties",
        "runtime-properties",
        "loom",
        "parser-fuzz-corpus",
        "panic-and-limits",
    ]
    assert names[3][:4] == [
        "baseline-venv",
        "baseline-install",
        "fixture-workspace",
        "baseline-export",
    ]
    assert names[9][0] == "five-wheel-and-sdist-evidence"
    assert names[10][-2:] == ["publish-workflow-dry-run-no-upload", "evidence-index"]


def test_round_two_filters_have_static_rust_test_targets() -> None:
    root = Path(unified_test.ROOT)
    round_two = next(round_spec for round_spec in unified_test.ROUNDS if round_spec.number == 2)
    steps = {step.name: step for step in round_two.steps}
    expected = {
        "frame-fragmentation": ("crates/eltdx-protocol/src", "frame_decoder"),
        "parser-properties": ("crates/eltdx-protocol/src", "parser_properties"),
        "runtime-properties": ("crates/eltdx-runtime/src", "arbitrary_"),
        "loom": ("crates/eltdx-runtime/src", "loom"),
        "parser-fuzz-corpus": ("crates/eltdx-protocol/src", "fuzz_corpus"),
        "panic-and-limits": ("crates/eltdx-protocol/src", "protocol_limits"),
    }
    for step_name, (source_root, test_filter) in expected.items():
        step = steps[step_name]
        assert step.argv[-1] == test_filter
        test_name = re.compile(rf"\bfn\s+[A-Za-z0-9_]*{re.escape(test_filter)}[A-Za-z0-9_]*\b")
        matches = [
            path
            for path in (root / source_root).rglob("*.rs")
            if test_name.search(path.read_text(encoding="utf-8"))
        ]
        assert matches, (step_name, test_filter)

    loom = steps["loom"]
    assert loom.argv[4:6] == ("--features", "loom")
    runtime_lib = (root / "crates/eltdx-runtime/src/lib.rs").read_text(encoding="utf-8")
    assert '#[cfg(all(test, feature = "loom"))]\nmod loom_tests;' in runtime_lib


def test_remote_requirements_are_evidence_gates_not_actions() -> None:
    gates = {
        step.evidence_key
        for round_spec in unified_test.ROUNDS
        for step in round_spec.steps
        if step.kind == "external_evidence"
    }
    assert gates == {"round6_windows_linux", "round9_artifacts"}
    for round_spec in unified_test.ROUNDS:
        names = [step.name for step in round_spec.steps]
        assert len(names) == len(set(names))


def test_runner_contains_no_release_or_remote_mutation_command() -> None:
    forbidden = (
        ("git", "push"),
        ("git", "tag"),
        ("gh", "release"),
        ("gh", "workflow"),
        ("twine", "upload"),
    )
    for round_spec in unified_test.ROUNDS:
        for step in round_spec.steps:
            if step.kind != "command":
                continue
            for prefix in forbidden:
                assert step.argv[: len(prefix)] != prefix, (round_spec.number, step.name)
    source = inspect.getsource(unified_test)
    assert "shell=True" not in source
    assert "workflow_dispatch" not in source


def test_all_unified_command_targets_exist() -> None:
    root = Path(unified_test.ROOT)
    for round_spec in unified_test.ROUNDS:
        for step in round_spec.steps:
            if step.kind != "command":
                continue
            for argument in step.argv:
                target = argument.split("::", 1)[0]
                if target.endswith(".py") or target.startswith("tests/"):
                    assert (root / target).is_file() or (root / target).is_dir(), (
                        round_spec.number,
                        step.name,
                        target,
                    )


def test_candidate_check_requires_exact_head_and_clean_worktree(monkeypatch: pytest.MonkeyPatch) -> None:
    answers = iter(["abc", ""])
    monkeypatch.setattr(unified_test, "_git", lambda *_args: next(answers))
    unified_test._assert_candidate("abc")

    answers = iter(["other"])
    monkeypatch.setattr(unified_test, "_git", lambda *_args: next(answers))
    with pytest.raises(RuntimeError, match="candidate mismatch"):
        unified_test._assert_candidate("abc")


def test_candidate_check_allows_only_completed_editable_native(
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
) -> None:
    relative = unified_test.EDITABLE_NATIVE_RELATIVE
    native = tmp_path / relative
    native.parent.mkdir(parents=True)
    native.write_bytes(b"native")
    monkeypatch.setattr(unified_test, "ROOT", tmp_path)

    status = f"?? {relative.as_posix()}"
    answers = iter(["abc", status])
    monkeypatch.setattr(unified_test, "_git", lambda *_args: next(answers))
    unified_test._assert_candidate("abc", allow_editable_native=True)

    answers = iter(["abc", status])
    monkeypatch.setattr(unified_test, "_git", lambda *_args: next(answers))
    with pytest.raises(RuntimeError, match="candidate worktree is not clean"):
        unified_test._assert_candidate("abc")

    answers = iter(["abc", status + "\n?? unexpected.txt"])
    monkeypatch.setattr(unified_test, "_git", lambda *_args: next(answers))
    with pytest.raises(RuntimeError, match="unexpected.txt"):
        unified_test._assert_candidate("abc", allow_editable_native=True)


def test_editable_native_allowance_requires_completed_maturin() -> None:
    state = {"round_progress": {"1": {"completed_steps": ["python-mypy"]}}}
    assert not unified_test._state_allows_editable_native(state)
    state["round_progress"]["1"]["completed_steps"].append("maturin-develop")
    assert unified_test._state_allows_editable_native(state)


def test_command_environment_follows_explicit_python_virtualenv(
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
) -> None:
    virtualenv = tmp_path / "runner-venv"
    executable_dir = virtualenv / ("Scripts" if os.name == "nt" else "bin")
    executable = executable_dir / ("python.exe" if os.name == "nt" else "python")
    executable_dir.mkdir(parents=True)
    (virtualenv / "pyvenv.cfg").write_text("version = test\n", encoding="utf-8")
    monkeypatch.setenv("VIRTUAL_ENV", str(tmp_path / "wrong-venv"))
    monkeypatch.setenv("PATH", os.pathsep.join(("first", "second")))

    environment = unified_test._command_environment(
        (str(executable), "-m", "maturin"),
        working_fixtures="fixtures",
    )

    assert environment["VIRTUAL_ENV"] == str(virtualenv)
    assert environment["PATH"].split(os.pathsep)[0] == str(executable_dir)
    assert environment["ELTDX_FIXTURES_ROOT"] == "fixtures"


def test_step_cleanliness_failure_is_recorded(
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
) -> None:
    state = {
        "next_round": 1,
        "completed_rounds": [],
        "failure": None,
        "external_evidence": {},
        "round_progress": {},
    }
    checks = 0

    def assert_candidate(_candidate: str, *, allow_editable_native: bool = False) -> None:
        nonlocal checks
        del allow_editable_native
        checks += 1
        if checks == 2:
            raise RuntimeError("candidate worktree is not clean: unexpected.txt")

    monkeypatch.setattr(unified_test, "EVIDENCE_ROOT", tmp_path)
    monkeypatch.setattr(unified_test, "_load_state", lambda _candidate: state)
    monkeypatch.setattr(unified_test, "_assert_candidate", assert_candidate)
    monkeypatch.setattr(unified_test, "_write_json", lambda _path, _value: None)

    with pytest.raises(RuntimeError, match="unexpected.txt"):
        unified_test.run_next_round("abc", baseline_wheel=None, artifact_dir=None)

    assert state["active_round"] is None
    assert state["failure"]["round"] == 1
    assert state["failure"]["step"] == "cargo-fmt"


def test_external_evidence_is_content_addressed(tmp_path: Path) -> None:
    evidence = tmp_path / "evidence.json"
    evidence.write_text("{}", encoding="utf-8")
    state = {
        "external_evidence": {
            "round6_windows_linux": {
                "path": str(evidence),
                "sha256": unified_test._sha256(evidence),
            }
        }
    }
    step = unified_test.gate("stress", "round6_windows_linux")
    unified_test._check_gate(step, state)
    evidence.write_text('{"changed": true}', encoding="utf-8")
    with pytest.raises(unified_test.ExternalEvidenceRequired, match="changed or is missing"):
        unified_test._check_gate(step, state)


def test_fixture_workspace_excludes_generated_goldens(
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
) -> None:
    source = tmp_path / "source"
    case = source / "heartbeat" / "normal"
    case.mkdir(parents=True)
    (case / "request.json").write_text("{}", encoding="utf-8")
    (case / "response.bin").write_bytes(b"response")
    (case / "request.bin").write_bytes(b"generated")
    (case / "expected.json").write_text("{}", encoding="utf-8")
    (source / "baseline").mkdir()
    (source / "baseline" / "old.json").write_text("{}", encoding="utf-8")

    evidence_root = tmp_path / "evidence"
    output = evidence_root / "candidate" / "round-03" / "fixtures"
    monkeypatch.setattr(prepare_fixture_workspace, "EVIDENCE_ROOT", evidence_root)
    manifest = prepare_fixture_workspace.prepare_workspace(source, output)

    assert (output / "heartbeat" / "normal" / "request.json").is_file()
    assert (output / "heartbeat" / "normal" / "response.bin").is_file()
    assert not (output / "heartbeat" / "normal" / "request.bin").exists()
    assert not (output / "heartbeat" / "normal" / "expected.json").exists()
    assert not (output / "baseline").exists()
    assert [item["path"] for item in manifest["files"]] == [
        "heartbeat/normal/request.json",
        "heartbeat/normal/response.bin",
    ]
