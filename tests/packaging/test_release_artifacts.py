"""Contracts for native release artifact selection and inventory."""

from __future__ import annotations

import zipfile
from pathlib import Path

import pytest

from scripts.verification import verify_wheel_matrix
from scripts.verification.verify_release_artifacts import (
    EXPECTED_PLATFORMS,
    classify_platform,
    verify_artifacts,
)


WHEELS = (
    "eltdx-3.0.0a1-cp310-abi3-win_amd64.whl",
    "eltdx-3.0.0a1-cp310-abi3-manylinux_2_17_x86_64.manylinux2014_x86_64.whl",
    "eltdx-3.0.0a1-cp310-abi3-manylinux_2_17_aarch64.manylinux2014_aarch64.whl",
    "eltdx-3.0.0a1-cp310-abi3-macosx_10_12_x86_64.whl",
    "eltdx-3.0.0a1-cp310-abi3-macosx_11_0_arm64.whl",
)


def _artifacts(root: Path) -> None:
    for name in WHEELS:
        with zipfile.ZipFile(root / name, "w") as archive:
            archive.writestr("eltdx/__init__.py", b"")
    (root / "eltdx-3.0.0a1.tar.gz").write_bytes(b"sdist")


def test_exact_release_artifact_inventory(tmp_path: Path) -> None:
    _artifacts(tmp_path)
    inventory = verify_artifacts(tmp_path, candidate=None)
    assert inventory["version"] == "3.0.0a1"
    assert set(inventory["platforms"]) == EXPECTED_PLATFORMS
    assert len(inventory["files"]) == 6
    assert all(len(item["sha256"]) == 64 for item in inventory["files"])


@pytest.mark.parametrize(
    ("tag", "expected"),
    [
        ("win_amd64", "windows-x86_64"),
        ("manylinux_2_17_x86_64.manylinux2014_x86_64", "linux-x86_64"),
        ("manylinux_2_17_aarch64.manylinux2014_aarch64", "linux-aarch64"),
        ("macosx_10_12_x86_64", "macos-x86_64"),
        ("macosx_11_0_arm64", "macos-arm64"),
    ],
)
def test_platform_classification(tag: str, expected: str) -> None:
    assert classify_platform(tag) == expected


@pytest.mark.parametrize(
    "tag",
    [
        "musllinux_1_2_x86_64",
        "macosx_11_0_universal2",
        "macosx_10_9_x86_64",
        "macosx_10_15_arm64",
        "manylinux_2_28_x86_64",
    ],
)
def test_unsupported_platform_tags_are_rejected(tag: str) -> None:
    with pytest.raises(ValueError):
        classify_platform(tag)


def test_inventory_rejects_extra_or_missing_distributions(tmp_path: Path) -> None:
    _artifacts(tmp_path)
    (tmp_path / "notes.txt").write_text("unexpected", encoding="utf-8")
    with pytest.raises(ValueError, match="unexpected files"):
        verify_artifacts(tmp_path, candidate=None)
    (tmp_path / "notes.txt").unlink()
    (tmp_path / WHEELS[0]).unlink()
    with pytest.raises(ValueError, match="exactly 5 wheels"):
        verify_artifacts(tmp_path, candidate=None)


def test_inventory_rejects_wheel_bytecode(tmp_path: Path) -> None:
    _artifacts(tmp_path)
    with zipfile.ZipFile(tmp_path / WHEELS[0], "a") as archive:
        archive.writestr("eltdx/__pycache__/client.cpython-314.pyc", b"bytecode")
    with pytest.raises(ValueError, match="wheel contains Python bytecode"):
        verify_artifacts(tmp_path, candidate=None)


def test_wheel_matrix_cli_defaults_to_current_runner(
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
) -> None:
    calls: list[tuple[Path, str, str]] = []
    monkeypatch.setattr(
        verify_wheel_matrix,
        "verify_wheel",
        lambda artifact_dir, *, platform, python_version: calls.append(
            (artifact_dir, platform, python_version)
        ),
    )

    assert verify_wheel_matrix.main(["--artifact-dir", str(tmp_path)]) == 0
    version_info = verify_wheel_matrix.sys.version_info
    assert calls == [
        (
            tmp_path,
            verify_wheel_matrix._current_platform(),
            f"{version_info.major}.{version_info.minor}",
        )
    ]

    calls.clear()
    assert (
        verify_wheel_matrix.main(
            [
                "--artifact-dir",
                str(tmp_path),
                "--platform",
                "linux-x86_64",
                "--python-version",
                "3.14",
            ]
        )
        == 0
    )
    assert calls == [(tmp_path, "linux-x86_64", "3.14")]
