"""Contracts for complete, non-overlapping Python test-group ownership."""

from __future__ import annotations

from collections import Counter
from pathlib import Path

from scripts import unified_test
from scripts.verification.run_python_test_group import load_groups


ROOT = Path(__file__).parents[2]
EXPECTED_GROUPS = {
    "round3-golden",
    "round4-local",
    "round5-loopback",
    "round6-stress",
    "round8-real-host",
}


def test_every_python_test_file_has_exactly_one_group() -> None:
    groups = load_groups()
    assert set(groups) == EXPECTED_GROUPS

    assigned = [path for paths in groups.values() for path in paths]
    counts = Counter(assigned)
    assert all(count == 1 for count in counts.values())

    actual = {
        path.relative_to(ROOT).as_posix()
        for path in (ROOT / "tests").rglob("test_*.py")
    }
    assert set(assigned) == actual
    assert all((ROOT / path).is_file() for path in assigned)


def test_unified_plan_runs_each_owned_group_in_its_designated_round() -> None:
    rounds = {round_spec.number: round_spec for round_spec in unified_test.ROUNDS}
    expected = {
        4: "round4-local",
        5: "round5-loopback",
        6: "round6-stress",
        8: "round8-real-host",
    }
    for number, group in expected.items():
        commands = [step.argv for step in rounds[number].steps if step.kind == "command"]
        assert any(
            "scripts/verification/run_python_test_group.py" in command
            and group in command
            for command in commands
        )

    round_three = [
        argument
        for step in rounds[3].steps
        if step.kind == "command"
        for argument in step.argv
    ]
    assert "tests/native/test_differential.py::test_native_request_frames_match_v205_fixture" in round_three


def test_github_python_matrix_uses_the_same_round_four_group() -> None:
    workflow = (ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")
    assert "python scripts/verification/run_python_test_group.py round4-local" in workflow
