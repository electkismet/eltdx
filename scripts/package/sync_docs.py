"""Synchronize the authoritative root docs into the packaged mirror."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "docs"
DESTINATION = ROOT / "src" / "eltdx" / "docs"


@dataclass(frozen=True, slots=True)
class SyncResult:
    changed: tuple[str, ...]
    remaining: tuple[str, ...]


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _files(root: Path) -> dict[str, Path]:
    if not root.exists():
        return {}
    return {
        path.relative_to(root).as_posix(): path
        for path in root.rglob("*")
        if path.is_file()
    }


def differences(source: Path, destination: Path) -> tuple[str, ...]:
    source_files = _files(source)
    destination_files = _files(destination)
    paths = sorted(set(source_files) | set(destination_files))
    return tuple(
        relative
        for relative in paths
        if relative not in source_files
        or relative not in destination_files
        or _sha256(source_files[relative]) != _sha256(destination_files[relative])
    )


def sync_docs(
    source: Path,
    destination: Path,
    *,
    check: bool,
    max_changes: int | None,
) -> SyncResult:
    source = source.resolve()
    destination = destination.resolve()
    if not source.is_dir():
        raise FileNotFoundError(f"documentation source does not exist: {source}")
    if source == destination or source in destination.parents:
        raise ValueError("documentation destination must not be inside the source tree")
    if max_changes is not None and max_changes <= 0:
        raise ValueError("max_changes must be > 0")

    pending = differences(source, destination)
    if check:
        return SyncResult(changed=(), remaining=pending)

    selected = pending[:max_changes] if max_changes is not None else pending
    source_files = _files(source)
    destination.mkdir(parents=True, exist_ok=True)
    changed = []
    for relative in selected:
        target = destination / relative
        source_path = source_files.get(relative)
        if source_path is None:
            target.unlink()
        else:
            target.parent.mkdir(parents=True, exist_ok=True)
            temporary = target.with_name(f".{target.name}.tmp-{os.getpid()}")
            shutil.copy2(source_path, temporary)
            temporary.replace(target)
        changed.append(relative)

    for directory in sorted(
        (path for path in destination.rglob("*") if path.is_dir()),
        reverse=True,
    ):
        if not any(directory.iterdir()):
            directory.rmdir()
    return SyncResult(changed=tuple(changed), remaining=differences(source, destination))


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Synchronize packaged eltdx documentation")
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--max-changes", type=int)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    result = sync_docs(
        SOURCE,
        DESTINATION,
        check=args.check,
        max_changes=args.max_changes,
    )
    print(json.dumps({"changed": result.changed, "remaining": result.remaining}, ensure_ascii=False))
    return 1 if args.check and result.remaining else 0


if __name__ == "__main__":
    raise SystemExit(main())
