"""Contracts for tag-gated release metadata and branch dry-runs."""

from __future__ import annotations

from pathlib import Path

from scripts.verification.check_release_metadata import TARGET_PYTHON_VERSION


ROOT = Path(__file__).resolve().parents[2]
TAG_REF = f"refs/tags/v{TARGET_PYTHON_VERSION}"
BRANCH_REF = "refs/heads/codex/rust-3.0"


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
