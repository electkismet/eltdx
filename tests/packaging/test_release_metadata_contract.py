"""Contracts for tag-gated release metadata and branch dry-runs."""

from __future__ import annotations

from pathlib import Path

import pytest

from scripts.verification.check_release_metadata import TARGET_CARGO_VERSION


ROOT = Path(__file__).resolve().parents[2]
TAG_REF = f"refs/tags/v{TARGET_CARGO_VERSION}"
BRANCH_REF = "refs/heads/codex/rust-3.0"


@pytest.mark.parametrize(
    ("cargo", "python"),
    [("3.4.0-beta.1", "3.4.0b1"), ("3.4.0-rc.2", "3.4.0rc2"), ("3.4.0", "3.4.0")],
)
def test_cargo_release_version_has_canonical_python_equivalent(cargo: str, python: str) -> None:
    from scripts.verification.check_release_metadata import _normalize_cargo

    assert _normalize_cargo(cargo) == python


def test_current_release_metadata_is_consistent() -> None:
    from scripts.verification.check_release_metadata import check

    assert check() == []


def test_current_release_notes_are_complete() -> None:
    from scripts.verification.check_release_text import check

    assert check() == []


def test_publish_workflow_separates_tag_and_manual_metadata_checks() -> None:
    workflow = (ROOT / ".github/workflows/publish.yml").read_text(encoding="utf-8")
    assert "if: github.event_name == 'push'" in workflow
    assert "if: github.event_name == 'workflow_dispatch'" in workflow
    assert "--allow-dry-run-ref" in workflow
    assert workflow.count("check_release_metadata.py") == 2


def test_manual_ref_override_is_explicit_and_tag_validation_stays_strict() -> None:
    source = (ROOT / "scripts/verification/check_release_metadata.py").read_text(
        encoding="utf-8"
    )
    assert "allow_dry_run_ref: bool = False" in source
    assert "ref != expected_ref and not allow_dry_run_ref" in source
    assert TAG_REF != BRANCH_REF
