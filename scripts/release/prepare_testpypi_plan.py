"""Write a local TestPyPI rehearsal plan without uploading artifacts."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from scripts.verification.verify_release_artifacts import verify_artifacts


def _candidate() -> str:
    return subprocess.run(
        ("git", "rev-parse", "HEAD"),
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()


def _write_json(path: Path, value: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.tmp-{os.getpid()}")
    temporary.write_text(json.dumps(value, ensure_ascii=True, indent=2) + "\n", encoding="utf-8")
    temporary.replace(path)


def build_plan(artifact_dir: Path) -> dict:
    artifact_dir = artifact_dir.resolve()
    candidate = _candidate()
    inventory = verify_artifacts(artifact_dir, candidate=candidate)
    artifact_paths = [str(artifact_dir / record["name"]) for record in inventory["files"]]
    return {
        "schema": 1,
        "kind": "testpypi-rehearsal-plan",
        "candidate": candidate,
        "version": inventory["version"],
        "artifact_dir": str(artifact_dir),
        "artifacts": inventory["files"],
        "artifact_count": len(artifact_paths),
        "testpypi_environment_approval_required": True,
        "testpypi_authorization": "NOT GRANTED",
        "production_pypi_authorization": "NOT GRANTED",
        "github_release_authorization": "NOT GRANTED",
        "upload_performed": False,
        "planned_upload_command": [
            "python",
            "-m",
            "twine",
            "upload",
            "--repository",
            "testpypi",
            *artifact_paths,
        ],
        "planned_install_command": [
            "python",
            "-m",
            "pip",
            "install",
            "--index-url",
            "https://test.pypi.org/simple/",
            "--extra-index-url",
            "https://pypi.org/simple/",
            f"eltdx[mcp]=={inventory['version']}",
        ],
        "notes": [
            "This file is a plan only and performs no upload.",
            "Use the same hashed 5 wheels and 1 sdist; do not rebuild after approval.",
            "TestPyPI approval does not authorize production PyPI or GitHub Release.",
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifact-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    plan = build_plan(args.artifact_dir)
    _write_json(args.output, plan)
    print(json.dumps({"artifacts": plan["artifact_count"], "upload_performed": False}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
