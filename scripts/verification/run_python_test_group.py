"""Run one candidate-bound Python test group from the shared inventory."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).parents[2]
MANIFEST = ROOT / "tests" / "contracts" / "manifests" / "python_test_groups.json"


def load_groups() -> dict[str, tuple[str, ...]]:
    raw: Any = json.loads(MANIFEST.read_text(encoding="utf-8"))
    if not isinstance(raw, dict) or raw.get("schema_version") != 1:
        raise ValueError("unsupported Python test-group manifest")
    groups = raw.get("groups")
    if not isinstance(groups, dict) or not groups:
        raise ValueError("Python test-group manifest has no groups")

    result: dict[str, tuple[str, ...]] = {}
    for name, paths in groups.items():
        if not isinstance(name, str) or not isinstance(paths, list) or not paths:
            raise ValueError(f"invalid Python test group: {name!r}")
        if not all(isinstance(path, str) and path for path in paths):
            raise ValueError(f"invalid Python test path in group: {name}")
        result[name] = tuple(paths)
    return result


def run_group(name: str) -> int:
    groups = load_groups()
    if name not in groups:
        raise ValueError(f"unknown Python test group: {name}")
    return subprocess.call(
        [sys.executable, "-m", "pytest", "-q", *groups[name]],
        cwd=ROOT,
    )


def main(argv: list[str] | None = None) -> int:
    groups = load_groups()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("group", choices=sorted(groups))
    args = parser.parse_args(argv)
    return run_group(args.group)


if __name__ == "__main__":
    raise SystemExit(main())
