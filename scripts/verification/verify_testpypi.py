"""Install one exact TestPyPI wheel and run the installed-distribution smoke."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
TAG_PATTERN = re.compile(r"v(?P<version>[0-9]+\.[0-9]+\.[0-9]+(?:(?:a|b|rc)[0-9]+)?)")
DOWNLOAD_ATTEMPTS = 12
DOWNLOAD_RETRY_SECONDS = 10


def _venv_python(venv: Path) -> Path:
    return venv / ("Scripts/python.exe" if os.name == "nt" else "bin/python")


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _verify_manifest(manifest_path: Path, version: str, candidate: str, wheel: Path) -> None:
    document = json.loads(manifest_path.read_text(encoding="utf-8"))
    records = document.get("files")
    if document.get("schema_version") != 1 or document.get("version") != version:
        raise ValueError("release artifact manifest schema/version differs from the TestPyPI tag")
    if not re.fullmatch(r"[0-9a-f]{40}", candidate):
        raise ValueError("candidate must be a full lowercase commit SHA")
    if document.get("candidate") != candidate:
        raise ValueError("release artifact manifest candidate differs from the TestPyPI candidate")
    if not isinstance(records, list) or len(records) != 6:
        raise ValueError("release artifact manifest must contain exact 5+1 records")
    if any(not isinstance(record, dict) for record in records):
        raise ValueError("release artifact manifest records must be objects")
    names = [record.get("name") for record in records]
    if any(not isinstance(name, str) for name in names):
        raise ValueError("release artifact manifest filenames must be strings")
    if len(set(names)) != len(names):
        raise ValueError("release artifact manifest contains duplicate filenames")
    if sum(record.get("kind") == "wheel" for record in records) != 5:
        raise ValueError("release artifact manifest does not contain five wheels")
    if sum(record.get("kind") == "sdist" for record in records) != 1:
        raise ValueError("release artifact manifest does not contain one sdist")
    matches = [record for record in records if record.get("name") == wheel.name]
    if len(matches) != 1:
        raise ValueError(f"downloaded TestPyPI wheel is absent from the manifest: {wheel.name}")
    record = matches[0]
    if record.get("size") != wheel.stat().st_size or record.get("sha256") != _sha256(wheel):
        raise ValueError("downloaded TestPyPI wheel differs from the same-run release artifact")


def _download(version: str, destination: Path, environment: dict[str, str]) -> Path:
    command = (
        sys.executable,
        "-m",
        "pip",
        "download",
        "--disable-pip-version-check",
        "--no-cache-dir",
        "--no-deps",
        "--only-binary=:all:",
        "--index-url",
        "https://test.pypi.org/simple/",
        "--dest",
        str(destination),
        f"eltdx=={version}",
    )
    for attempt in range(1, DOWNLOAD_ATTEMPTS + 1):
        result = subprocess.run(command, cwd=ROOT, env=environment, check=False)
        if result.returncode == 0:
            break
        if attempt == DOWNLOAD_ATTEMPTS:
            raise RuntimeError(f"TestPyPI wheel remained unavailable after {attempt} attempts")
        time.sleep(DOWNLOAD_RETRY_SECONDS)
    wheels = sorted(destination.glob("eltdx-*.whl"))
    expected = re.compile(rf"eltdx-{re.escape(version)}-cp310-abi3-.+\.whl")
    if len(wheels) != 1 or expected.fullmatch(wheels[0].name) is None:
        raise RuntimeError(f"TestPyPI did not provide one exact ABI3 wheel: {wheels!r}")
    return wheels[0]


def main() -> int:
    parser = argparse.ArgumentParser(description="Verify the exact approved TestPyPI release")
    parser.add_argument("--tag", required=True)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--artifact-manifest", type=Path, required=True)
    args = parser.parse_args()
    match = TAG_PATTERN.fullmatch(args.tag)
    if match is None:
        raise ValueError("tag must be a normalized v-prefixed PEP 440 release")
    version = match.group("version")

    environment = dict(os.environ)
    environment.pop("PYTHONPATH", None)
    environment["PYTHONNOUSERSITE"] = "1"
    with tempfile.TemporaryDirectory(prefix="eltdx-testpypi-") as temporary:
        workspace = Path(temporary)
        downloads = workspace / "downloads"
        downloads.mkdir()
        wheel = _download(version, downloads, environment)
        _verify_manifest(args.artifact_manifest.resolve(), version, args.candidate, wheel)
        venv = workspace / "venv"
        subprocess.run(
            (sys.executable, "-m", "venv", str(venv)),
            cwd=workspace,
            env=environment,
            check=True,
        )
        python = _venv_python(venv)
        subprocess.run(
            (str(python), "-m", "pip", "install", f"{wheel}[mcp]"),
            cwd=workspace,
            env=environment,
            check=True,
        )
        subprocess.run(
            (str(python), "-m", "pip", "check"),
            cwd=workspace,
            env=environment,
            check=True,
        )
        subprocess.run(
            (
                str(python),
                str(ROOT / "scripts" / "verification" / "installed_smoke.py"),
                "--expected-version",
                version,
            ),
            cwd=workspace,
            env=environment,
            check=True,
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
