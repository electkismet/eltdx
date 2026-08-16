"""Verify and inventory the exact eltdx 5-wheel plus 1-sdist set."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import zipfile
from pathlib import Path, PurePosixPath
from typing import Any


WHEEL_PATTERN = re.compile(
    r"^eltdx-(?P<version>[^-]+)-cp310-abi3-(?P<platform>[^-]+)\.whl$"
)
SDIST_PATTERN = re.compile(r"^eltdx-(?P<version>[^-]+)\.tar\.gz$")
EXPECTED_PLATFORMS = frozenset(
    {
        "windows-x86_64",
        "linux-x86_64",
        "linux-aarch64",
        "macos-x86_64",
        "macos-arm64",
    }
)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def classify_platform(tag: str) -> str:
    if tag == "win_amd64":
        return "windows-x86_64"
    if "musllinux" in tag:
        raise ValueError(f"musllinux is not a 3.0 release target: {tag}")
    if tag.endswith("_x86_64") and "manylinux" in tag:
        if "manylinux_2_17" not in tag and "manylinux2014" not in tag:
            raise ValueError(f"Linux x86_64 wheel does not declare manylinux_2_17/2014: {tag}")
        return "linux-x86_64"
    if tag.endswith("_aarch64") and "manylinux" in tag:
        if "manylinux_2_17" not in tag and "manylinux2014" not in tag:
            raise ValueError(f"Linux aarch64 wheel does not declare manylinux_2_17/2014: {tag}")
        return "linux-aarch64"
    if "universal2" in tag:
        raise ValueError("universal2 wheels are forbidden; release separate macOS architectures")
    if tag == "macosx_10_12_x86_64":
        return "macos-x86_64"
    if tag == "macosx_11_0_arm64":
        return "macos-arm64"
    raise ValueError(f"unsupported release wheel platform tag: {tag}")


def inspect_release_wheel(wheel: Path) -> dict[str, str]:
    match = WHEEL_PATTERN.fullmatch(wheel.name)
    if match is None:
        raise ValueError(f"wheel is not eltdx cp310-abi3: {wheel.name}")
    _verify_wheel_archive(wheel)
    return {
        "name": wheel.name,
        "version": match.group("version"),
        "platform": classify_platform(match.group("platform")),
    }


def verify_artifacts(artifact_dir: Path, *, candidate: str | None) -> dict[str, Any]:
    artifact_dir = artifact_dir.resolve()
    if not artifact_dir.is_dir():
        raise FileNotFoundError(artifact_dir)
    files = sorted(path for path in artifact_dir.iterdir() if path.is_file())
    wheels = [path for path in files if path.suffix == ".whl"]
    sdists = [path for path in files if path.name.endswith(".tar.gz")]
    unexpected = [path.name for path in files if path not in wheels and path not in sdists]
    if unexpected:
        raise ValueError(f"unexpected files in release package directory: {unexpected!r}")
    if len(wheels) != 5 or len(sdists) != 1:
        raise ValueError(f"release requires exactly 5 wheels and 1 sdist; got {len(wheels)}+{len(sdists)}")

    platforms: dict[str, str] = {}
    versions = set()
    records = []
    for wheel in wheels:
        inspected = inspect_release_wheel(wheel)
        platform = inspected["platform"]
        if platform in platforms:
            raise ValueError(f"duplicate release wheel platform {platform}: {wheel.name}")
        platforms[platform] = wheel.name
        versions.add(inspected["version"])
        records.append(_record(wheel, kind="wheel", platform=platform))
    if set(platforms) != EXPECTED_PLATFORMS:
        raise ValueError(
            f"release wheel platforms differ: missing={sorted(EXPECTED_PLATFORMS - set(platforms))!r}, "
            f"extra={sorted(set(platforms) - EXPECTED_PLATFORMS)!r}"
        )

    sdist_match = SDIST_PATTERN.fullmatch(sdists[0].name)
    if sdist_match is None:
        raise ValueError(f"invalid eltdx sdist filename: {sdists[0].name}")
    versions.add(sdist_match.group("version"))
    records.append(_record(sdists[0], kind="sdist", platform=None))
    if len(versions) != 1:
        raise ValueError(f"release artifacts contain different versions: {sorted(versions)!r}")

    if candidate is not None:
        if not re.fullmatch(r"[0-9a-f]{40}", candidate):
            raise ValueError("candidate must be a full lowercase commit SHA")
        head = subprocess.run(
            ("git", "rev-parse", "HEAD"),
            check=True,
            text=True,
            stdout=subprocess.PIPE,
        ).stdout.strip()
        if head != candidate:
            raise ValueError(f"artifact candidate {candidate} does not match HEAD {head}")
    return {
        "schema_version": 1,
        "candidate": candidate,
        "version": versions.pop(),
        "platforms": platforms,
        "files": sorted(records, key=lambda item: item["name"]),
    }


def _record(path: Path, *, kind: str, platform: str | None) -> dict[str, Any]:
    return {
        "name": path.name,
        "kind": kind,
        "platform": platform,
        "size": path.stat().st_size,
        "sha256": _sha256(path),
    }


def _verify_wheel_archive(path: Path) -> None:
    try:
        with zipfile.ZipFile(path) as archive:
            bytecode = sorted(
                member.filename
                for member in archive.infolist()
                if _is_python_bytecode(member.filename)
            )
    except zipfile.BadZipFile as error:
        raise ValueError(f"invalid wheel archive: {path.name}") from error
    if bytecode:
        raise ValueError(f"wheel contains Python bytecode: {path.name}: {bytecode!r}")


def _is_python_bytecode(name: str) -> bool:
    member = PurePosixPath(name)
    return "__pycache__" in member.parts or member.suffix in {".pyc", ".pyo"}


def _write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.tmp-{os.getpid()}")
    temporary.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    temporary.replace(path)


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Verify exact eltdx native release artifacts")
    parser.add_argument("--artifact-dir", type=Path, required=True)
    parser.add_argument("--candidate")
    parser.add_argument("--output", type=Path, required=True)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    inventory = verify_artifacts(args.artifact_dir, candidate=args.candidate)
    _write_json(args.output, inventory)
    print(json.dumps({"version": inventory["version"], "files": len(inventory["files"])}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
