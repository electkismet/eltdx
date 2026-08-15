"""Run the complete 21-command wire differential in an isolated CI workspace."""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
BASELINE_VERSION = "2.0.5"


def _run(argv: tuple[str, ...], *, env: dict[str, str] | None = None) -> None:
    subprocess.run(argv, cwd=ROOT, env=env, check=True)


def _venv_python(venv: Path) -> Path:
    return venv / ("Scripts/python.exe" if os.name == "nt" else "bin/python")


def main() -> int:
    environment = dict(os.environ)
    environment.pop("PYTHONPATH", None)
    environment["PYTHONNOUSERSITE"] = "1"
    evidence_root = ROOT / "artifacts" / "release-evidence"
    evidence_root.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(
        prefix="wire-golden-ci-",
        dir=evidence_root,
    ) as temporary:
        workspace = Path(temporary)
        downloads = workspace / "downloads"
        downloads.mkdir()
        _run(
            (
                sys.executable,
                "-m",
                "pip",
                "download",
                "--disable-pip-version-check",
                "--no-cache-dir",
                "--no-deps",
                "--only-binary=:all:",
                "--index-url",
                "https://pypi.org/simple",
                "--dest",
                str(downloads),
                f"eltdx=={BASELINE_VERSION}",
            ),
            env=environment,
        )
        wheels = sorted(downloads.glob("*.whl"))
        expected_wheel = f"eltdx-{BASELINE_VERSION}-py3-none-any.whl"
        if len(wheels) != 1 or wheels[0].name != expected_wheel:
            raise RuntimeError(f"expected one v2.0.5 baseline wheel, found {wheels!r}")

        baseline_venv = workspace / "baseline-venv"
        _run((sys.executable, "-m", "venv", str(baseline_venv)), env=environment)
        baseline_python = _venv_python(baseline_venv)
        _run(
            (str(baseline_python), "-m", "pip", "install", f"{wheels[0]}[mcp]"),
            env=environment,
        )

        fixtures = workspace / "fixtures"
        _run(
            (
                sys.executable,
                "scripts/fixtures/prepare_fixture_workspace.py",
                "--source",
                "tests/fixtures/7709",
                "--output",
                str(fixtures),
            ),
            env=environment,
        )
        _run(
            (
                str(baseline_python),
                "scripts/fixtures/export_v205_baseline.py",
                "--wheel",
                str(wheels[0]),
                "--fixtures-root",
                str(fixtures),
                "--contract-output",
                str(workspace / "contracts.json"),
            ),
            env=environment,
        )

        environment["ELTDX_FIXTURES_ROOT"] = str(fixtures)
        _run(
            (sys.executable, "-m", "pytest", "-q", "tests/native/test_differential.py"),
            env=environment,
        )
        _run(
            (
                sys.executable,
                "-m",
                "pytest",
                "-q",
                "tests/contracts/test_runtime_surfaces_contract.py",
                "-k",
                "protocol",
            ),
            env=environment,
        )
        _run(
            (
                sys.executable,
                "-m",
                "pytest",
                "-q",
                "tests/contracts/test_command_contract_manifest.py",
            ),
            env=environment,
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
