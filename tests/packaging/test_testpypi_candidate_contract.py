"""Contracts for candidate identity across aggregate and TestPyPI evidence."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path

import pytest

from scripts.verification.verify_testpypi import _verify_manifest


ROOT = Path(__file__).resolve().parents[2]
CANDIDATE = "a" * 40
VERSION = "3.0.0a1"


def _manifest(path: Path, wheel: Path, *, candidate: str = CANDIDATE) -> None:
    wheel_record = {
        "name": wheel.name,
        "kind": "wheel",
        "size": wheel.stat().st_size,
        "sha256": hashlib.sha256(wheel.read_bytes()).hexdigest(),
    }
    other_records = [
        {"name": f"other-{index}", "kind": "wheel" if index < 4 else "sdist"}
        for index in range(5)
    ]
    path.write_text(
        json.dumps(
            {
                "schema_version": 1,
                "candidate": candidate,
                "version": VERSION,
                "files": [wheel_record, *other_records],
            }
        ),
        encoding="utf-8",
    )


def test_testpypi_manifest_requires_the_exact_candidate(tmp_path: Path) -> None:
    wheel = tmp_path / "eltdx-3.0.0a1-cp310-abi3-manylinux_2_17_x86_64.whl"
    wheel.write_bytes(b"wheel")
    manifest = tmp_path / "artifacts.json"
    _manifest(manifest, wheel)
    _verify_manifest(manifest, VERSION, CANDIDATE, wheel)
    with pytest.raises(ValueError, match="candidate differs"):
        _verify_manifest(manifest, VERSION, "b" * 40, wheel)


def test_workflows_pass_the_same_github_sha_through_both_gates() -> None:
    native = (ROOT / ".github/workflows/native-wheels.yml").read_text(encoding="utf-8")
    publish = (ROOT / ".github/workflows/publish.yml").read_text(encoding="utf-8")
    native_call = (
        "verify_release_artifacts.py\n"
        "          --artifact-dir dist/packages\n"
        '          --candidate "${{ github.sha }}"'
    )
    testpypi_call = (
        "verify_testpypi.py\n"
        '          --tag "${{ github.ref_name }}"\n'
        '          --candidate "${{ github.sha }}"'
    )
    assert native_call in native
    assert testpypi_call in publish
