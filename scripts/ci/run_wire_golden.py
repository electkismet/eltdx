"""Verify committed 7709 golden fixtures and run protocol regression tests."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


def _run(*argv: str) -> None:
    subprocess.run(argv, cwd=ROOT, check=True)


def main() -> int:
    _run(sys.executable, "scripts/fixtures/export_current_golden.py", "--check")
    _run(sys.executable, "-m", "pytest", "-q", "tests/native/test_differential.py")
    _run(
        sys.executable,
        "-m",
        "pytest",
        "-q",
        "tests/contracts/test_runtime_surfaces_contract.py",
        "-k",
        "protocol",
    )
    _run(
        sys.executable,
        "-m",
        "pytest",
        "-q",
        "tests/contracts/test_command_contract_manifest.py",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
