"""Copy immutable fixture inputs into an ignored unified-test workspace."""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
EVIDENCE_ROOT = (ROOT / "artifacts" / "release-evidence").resolve()
GENERATED_NAMES = frozenset({"request.bin", "expected.json"})


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _ignore(_directory: str, names: list[str]) -> set[str]:
    ignored = {name for name in names if name in GENERATED_NAMES}
    if "baseline" in names:
        ignored.add("baseline")
    return ignored


def prepare_workspace(source: Path, output: Path) -> dict[str, Any]:
    source = source.resolve()
    output = output.resolve()
    if not source.is_dir():
        raise FileNotFoundError(f"fixture source does not exist: {source}")
    if not output.is_relative_to(EVIDENCE_ROOT):
        raise ValueError(f"fixture workspace must be below {EVIDENCE_ROOT}")
    if output.exists():
        raise FileExistsError(f"fixture workspace already exists: {output}")

    output.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(source, output, ignore=_ignore)
    files = sorted(path for path in output.rglob("*") if path.is_file())
    if any(path.name in GENERATED_NAMES for path in files):
        raise RuntimeError("generated golden output leaked into fixture workspace input")
    manifest = {
        "schema_version": 1,
        "source": str(source),
        "files": [
            {
                "path": path.relative_to(output).as_posix(),
                "sha256": _sha256(path),
                "size": path.stat().st_size,
            }
            for path in files
        ],
    }
    manifest_path = output / "workspace-manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return manifest


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Prepare isolated inputs for 7709 golden export")
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    manifest = prepare_workspace(args.source, args.output)
    print(json.dumps({"input_files": len(manifest["files"]), "output": str(args.output.resolve())}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
