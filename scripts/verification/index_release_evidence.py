"""Create a content-addressed local index for one candidate's release evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
INDEX_NAME = "release-evidence-index.json"
EXPECTED_EXTERNAL_EVIDENCE = frozenset({"round6_windows_linux", "round9_artifacts"})
EXPECTED_ROUND_10_STEPS = [
    "mkdocs-strict",
    "pages-links",
    "version-and-docs",
    "release-text",
    "testpypi-plan-only",
    "publish-workflow-dry-run-no-upload",
]
EXCLUDED_EXECUTION_ROOTS = frozenset({"baseline-venv"})
MUTABLE_LOCAL_PATHS = frozenset(
    {
        "state.json",
        "round-10-documentation-and-release/07-evidence-index.log",
    }
)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _write_json(path: Path, value: dict) -> None:
    temporary = path.with_name(f".{path.name}.tmp-{os.getpid()}")
    temporary.write_text(json.dumps(value, ensure_ascii=True, indent=2) + "\n", encoding="utf-8")
    temporary.replace(path)


def _state_evidence(root: Path, candidate: str) -> tuple[dict, list[dict]]:
    state = json.loads((root / "state.json").read_text(encoding="utf-8"))
    if state.get("candidate") != candidate:
        raise ValueError("unified-test state does not match the candidate")
    if state.get("failure") is not None:
        raise ValueError("cannot index failed unified-test state")
    if state.get("completed_rounds") != list(range(1, 10)) or state.get("active_round") != 10:
        raise ValueError("release evidence index must run inside unified round 10")
    completed_steps = state.get("round_progress", {}).get("10", {}).get("completed_steps")
    if completed_steps != EXPECTED_ROUND_10_STEPS:
        raise ValueError("release evidence index is not the final round 10 command")
    external = state.get("external_evidence", {})
    if set(external) != EXPECTED_EXTERNAL_EVIDENCE:
        raise ValueError("release evidence does not contain both required external gates")
    records = []
    for key, value in sorted(external.items()):
        path = Path(value["path"]).resolve()
        if not path.is_file() or path.is_symlink():
            raise ValueError(f"external evidence {key!r} is not a regular file")
        digest = _sha256(path)
        if digest != value["sha256"]:
            raise ValueError(f"external evidence {key!r} changed after it was recorded")
        records.append(
            {
                "key": key,
                "path": str(path),
                "size": path.stat().st_size,
                "sha256": digest,
            }
        )
    state_summary = {
        "completed_rounds": state["completed_rounds"],
        "active_round": state["active_round"],
        "round_10_completed_steps_before_index": completed_steps,
    }
    return state_summary, records


def build_index(candidate: str, evidence_root: Path) -> dict:
    if re.fullmatch(r"[0-9a-f]{40}", candidate) is None:
        raise ValueError("candidate must be a full lowercase commit SHA")
    head = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    if head != candidate:
        raise ValueError(f"candidate {candidate} does not match HEAD {head}")
    root = evidence_root.resolve()
    expected_root = (ROOT / "artifacts" / "release-evidence" / candidate).resolve()
    if root != expected_root or not root.is_dir():
        raise ValueError("evidence root is not the candidate's unified-test directory")
    files = []
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root).as_posix()
        if relative.split("/", 1)[0] in EXCLUDED_EXECUTION_ROOTS:
            continue
        if path.is_symlink():
            raise ValueError(f"release evidence must not contain symlinks: {relative}")
        if not path.is_file():
            continue
        if relative == INDEX_NAME or relative in MUTABLE_LOCAL_PATHS:
            continue
        files.append(
            {
                "path": relative,
                "size": path.stat().st_size,
                "sha256": _sha256(path),
            }
        )
    if not files:
        raise ValueError("release evidence root is empty")
    state_summary, external_evidence = _state_evidence(root, candidate)
    return {
        "schema": 1,
        "kind": "release-evidence-index",
        "candidate": candidate,
        "files": files,
        "file_count": len(files),
        "state_summary": state_summary,
        "external_evidence": external_evidence,
        "excluded_execution_roots": sorted(EXCLUDED_EXECUTION_ROOTS),
        "excluded_mutable_paths": sorted(MUTABLE_LOCAL_PATHS),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--evidence-root", type=Path, required=True)
    args = parser.parse_args()
    index = build_index(args.candidate, args.evidence_root)
    output = args.evidence_root.resolve() / INDEX_NAME
    _write_json(output, index)
    print(json.dumps({"candidate": args.candidate, "files": index["file_count"]}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
