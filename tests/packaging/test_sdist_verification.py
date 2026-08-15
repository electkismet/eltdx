"""Contracts for the native source distribution archive."""

from __future__ import annotations

import io
import tarfile
from pathlib import Path

import pytest

from scripts.verification.verify_sdist import REQUIRED_FILES, REQUIRED_PREFIXES, inspect_sdist


def _write_sdist(
    path: Path,
    *,
    mirror_matches: bool = True,
    unsafe: bool = False,
    bytecode: bool = False,
) -> None:
    files = {name: name.encode("utf-8") for name in REQUIRED_FILES}
    for prefix in REQUIRED_PREFIXES:
        files[f"{prefix}sentinel.txt"] = prefix.encode("utf-8")
    files["docs/index.md"] = b"# docs\n"
    for name, content in tuple(files.items()):
        if name.startswith("docs/"):
            files[f"src/eltdx/{name}"] = content
    if not mirror_matches:
        files["src/eltdx/docs/index.md"] = b"different\n"
    if unsafe:
        files["../escape.txt"] = b"escape"
    if bytecode:
        files["src/eltdx/__pycache__/client.cpython-314.pyc"] = b"bytecode"

    with tarfile.open(path, "w:gz") as archive:
        for relative, content in sorted(files.items()):
            info = tarfile.TarInfo(f"eltdx-3.0.0a1/{relative}")
            info.size = len(content)
            archive.addfile(info, io.BytesIO(content))


def test_sdist_inventory_requires_all_trees_and_identical_docs(tmp_path: Path) -> None:
    sdist = tmp_path / "eltdx-3.0.0a1.tar.gz"
    _write_sdist(sdist)
    inventory = inspect_sdist(sdist)
    assert inventory["root"] == "eltdx-3.0.0a1"
    assert inventory["docs"] >= 1


def test_sdist_rejects_docs_mirror_drift(tmp_path: Path) -> None:
    sdist = tmp_path / "eltdx-3.0.0a1.tar.gz"
    _write_sdist(sdist, mirror_matches=False)
    with pytest.raises(ValueError, match="mirror differs"):
        inspect_sdist(sdist)


def test_sdist_rejects_path_traversal(tmp_path: Path) -> None:
    sdist = tmp_path / "eltdx-3.0.0a1.tar.gz"
    _write_sdist(sdist, unsafe=True)
    with pytest.raises(ValueError, match="unsafe sdist path"):
        inspect_sdist(sdist)


def test_sdist_rejects_python_bytecode(tmp_path: Path) -> None:
    sdist = tmp_path / "eltdx-3.0.0a1.tar.gz"
    _write_sdist(sdist, bytecode=True)
    with pytest.raises(ValueError, match="contains Python bytecode"):
        inspect_sdist(sdist)
