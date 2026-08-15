"""Inspect the release sdist and prove it can build a native wheel."""

from __future__ import annotations

import argparse
import os
import subprocess
import tarfile
import tempfile
import venv
from pathlib import Path, PurePosixPath
from typing import Any


REQUIRED_FILES = frozenset(
    {
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        "pyproject.toml",
        "README.md",
        "LICENSE",
    }
)
REQUIRED_PREFIXES = (
    "crates/eltdx-protocol/",
    "crates/eltdx-runtime/",
    "crates/eltdx-python/",
    "src/eltdx/",
    "src/eltdx/docs/",
    "docs/",
    "tests/",
    "scripts/",
)


def _python(venv_root: Path) -> Path:
    return (
        venv_root / "Scripts" / "python.exe"
        if os.name == "nt"
        else venv_root / "bin" / "python"
    )


def _relative_members(archive: tarfile.TarFile) -> tuple[str, dict[str, tarfile.TarInfo]]:
    members = archive.getmembers()
    if not members:
        raise ValueError("sdist is empty")
    roots = set()
    relative: dict[str, tarfile.TarInfo] = {}
    for member in members:
        path = PurePosixPath(member.name)
        if path.is_absolute() or ".." in path.parts or len(path.parts) < 1:
            raise ValueError(f"unsafe sdist path: {member.name}")
        if member.issym() or member.islnk():
            raise ValueError(f"sdist links are not allowed: {member.name}")
        if not member.isfile() and not member.isdir():
            raise ValueError(f"unsupported sdist member type: {member.name}")
        roots.add(path.parts[0])
        if len(path.parts) > 1:
            relative_name = PurePosixPath(*path.parts[1:]).as_posix()
            if relative_name in relative:
                raise ValueError(f"duplicate sdist member: {relative_name}")
            relative[relative_name] = member
    if len(roots) != 1:
        raise ValueError(f"sdist must contain one top-level directory: {sorted(roots)!r}")
    return roots.pop(), relative


def inspect_sdist(path: Path) -> dict[str, Any]:
    path = path.resolve()
    if not path.is_file() or not path.name.endswith(".tar.gz"):
        raise FileNotFoundError(f"sdist does not exist: {path}")
    with tarfile.open(path, "r:gz") as archive:
        root, members = _relative_members(archive)
        regular_files = {name for name, member in members.items() if member.isfile()}
        bytecode = sorted(name for name in regular_files if _is_python_bytecode(name))
        if bytecode:
            raise ValueError(f"sdist contains Python bytecode: {bytecode!r}")
        missing_files = sorted(REQUIRED_FILES - regular_files)
        if missing_files:
            raise ValueError(f"sdist is missing required files: {missing_files!r}")
        missing_prefixes = [
            prefix for prefix in REQUIRED_PREFIXES if not any(name.startswith(prefix) for name in regular_files)
        ]
        if missing_prefixes:
            raise ValueError(f"sdist is missing required trees: {missing_prefixes!r}")

        docs = sorted(name for name in regular_files if name.startswith("docs/"))
        if not docs:
            raise ValueError("sdist root docs are empty")
        for root_doc in docs:
            packaged_doc = f"src/eltdx/{root_doc}"
            if packaged_doc not in regular_files:
                raise ValueError(f"packaged docs mirror is missing: {packaged_doc}")
            source_bytes = _read_member(archive, members[root_doc])
            mirror_bytes = _read_member(archive, members[packaged_doc])
            if source_bytes != mirror_bytes:
                raise ValueError(f"packaged docs mirror differs: {root_doc}")
    return {
        "root": root,
        "files": len(regular_files),
        "docs": len(docs),
    }


def _is_python_bytecode(name: str) -> bool:
    path = PurePosixPath(name)
    return "__pycache__" in path.parts or path.suffix in {".pyc", ".pyo"}


def _read_member(archive: tarfile.TarFile, member: tarfile.TarInfo) -> bytes:
    stream = archive.extractfile(member)
    if stream is None:
        raise ValueError(f"cannot read sdist member: {member.name}")
    return stream.read()


def build_from_sdist(path: Path) -> str:
    inventory = inspect_sdist(path)
    with tempfile.TemporaryDirectory(prefix="eltdx-sdist-") as temporary:
        temporary_root = Path(temporary)
        source_root = temporary_root / "source"
        wheel_root = temporary_root / "wheel"
        environment_root = temporary_root / "venv"
        source_root.mkdir()
        wheel_root.mkdir()
        with tarfile.open(path, "r:gz") as archive:
            try:
                archive.extractall(source_root, filter="data")
            except TypeError:
                archive.extractall(source_root)
        venv.EnvBuilder(with_pip=True, clear=True).create(environment_root)
        python = _python(environment_root)
        environment = dict(os.environ)
        environment.pop("PYTHONPATH", None)
        subprocess.run(
            (
                str(python),
                "-m",
                "pip",
                "wheel",
                "--no-binary",
                ":all:",
                "--no-deps",
                "--wheel-dir",
                str(wheel_root),
                str(source_root / inventory["root"]),
            ),
            check=True,
            env=environment,
        )
        wheels = list(wheel_root.glob("eltdx-*-cp310-abi3-*.whl"))
        if len(wheels) != 1:
            raise RuntimeError(f"sdist build did not produce one cp310-abi3 wheel: {wheels!r}")
        return wheels[0].name


def _find_sdist(artifact_dir: Path) -> Path:
    sdists = sorted(artifact_dir.resolve().glob("eltdx-*.tar.gz"))
    if len(sdists) != 1:
        raise ValueError(f"expected one eltdx sdist in {artifact_dir}, got {len(sdists)}")
    return sdists[0]


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Verify and build the eltdx release sdist")
    parser.add_argument("--artifact-dir", type=Path, required=True)
    parser.add_argument("--inspect-only", action="store_true")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    sdist = _find_sdist(args.artifact_dir)
    inventory = inspect_sdist(sdist)
    rebuilt = None
    if not args.inspect_only:
        rebuilt = build_from_sdist(sdist)
    print(
        f"sdist root={inventory['root']} files={inventory['files']} "
        f"docs={inventory['docs']} rebuilt={rebuilt}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
