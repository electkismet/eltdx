"""Inspect publish workflow gates while explicitly disabling external mutations."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
TAG_GATE = "if: github.event_name == 'push' && startsWith(github.ref, 'refs/tags/v')"


def _job(text: str, name: str) -> str:
    match = re.search(
        rf"^  {re.escape(name)}:\n(?P<body>.*?)(?=^  [A-Za-z0-9_-]+:\n|\Z)",
        text,
        flags=re.MULTILINE | re.DOTALL,
    )
    if match is None:
        raise AssertionError(f"publish workflow is missing job {name!r}")
    return match.group("body")


def inspect_workflow(*, disable_uploads: bool, disable_release: bool) -> dict:
    if not disable_uploads or not disable_release:
        raise ValueError("dry-run requires both --disable-uploads and --disable-release")
    path = ROOT / ".github" / "workflows" / "publish.yml"
    text = path.read_text(encoding="utf-8")
    native = _job(text, "native-dist")
    testpypi = _job(text, "testpypi")
    testpypi_smoke = _job(text, "testpypi-smoke")
    pypi = _job(text, "pypi")
    release = _job(text, "release")
    checks = {
        "manual_dispatch_present": "workflow_dispatch:" in text,
        "same_run_native_workflow_present": "uses: ./.github/workflows/native-wheels.yml" in native,
        "testpypi_tag_gate_present": TAG_GATE in testpypi,
        "testpypi_environment_present": "name: testpypi" in testpypi,
        "testpypi_trusted_publish_present": "pypa/gh-action-pypi-publish" in testpypi,
        "testpypi_smoke_tag_gate_present": TAG_GATE in testpypi_smoke,
        "testpypi_smoke_depends_on_upload": "needs: [verify-release, testpypi]" in testpypi_smoke,
        "pypi_tag_gate_present": TAG_GATE in pypi,
        "pypi_environment_present": "name: pypi" in pypi,
        "pypi_depends_on_testpypi_smoke": (
            "needs: [native-dist, verify-release, testpypi-smoke]" in pypi
        ),
        "pypi_trusted_publish_present": "pypa/gh-action-pypi-publish" in pypi,
        "release_tag_gate_present": TAG_GATE in release,
        "release_depends_on_pypi": "needs: [native-dist, verify-release, pypi]" in release,
        "github_release_command_present": "gh release create" in release,
    }
    if not all(checks.values()):
        missing = sorted(name for name, passed in checks.items() if not passed)
        raise AssertionError(f"publish workflow dry-run checks failed: {missing!r}")
    return {
        "schema": 1,
        "kind": "publish-workflow-dry-run",
        "workflow": str(path.relative_to(ROOT)),
        "checks": checks,
        "uploads_disabled": True,
        "github_release_disabled": True,
        "disabled_jobs": ["testpypi", "testpypi-smoke", "pypi", "release"],
        "commands_executed": [],
        "network_calls": 0,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--disable-uploads", action="store_true")
    parser.add_argument("--disable-release", action="store_true")
    args = parser.parse_args()
    report = inspect_workflow(
        disable_uploads=args.disable_uploads,
        disable_release=args.disable_release,
    )
    print(json.dumps(report, ensure_ascii=True, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
