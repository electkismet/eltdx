"""Check candidate version, docs mirror, catalog, and banner metadata."""

from __future__ import annotations

import argparse
import re
import struct
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from scripts.verification.verify_release_artifacts import verify_artifacts


TARGET_CARGO_VERSION = "3.0.6"
TARGET_PYTHON_VERSION = "3.0.6"
# The v3.0.5 banner is intentionally reused for this patch release.
BANNER_KEYWORD = b"eltdx_release\x00" + b"3.0.5"


def _cargo_version() -> str:
    text = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(r'^version = "([^"]+)"$', text, re.MULTILINE)
    if match is None:
        raise AssertionError("Cargo workspace version is missing")
    return match.group(1)


def _normalize_cargo(value: str) -> str:
    match = re.fullmatch(r"(\d+\.\d+\.\d+)-alpha\.(\d+)", value)
    return f"{match.group(1)}a{match.group(2)}" if match else value


def _png_text_chunks(path: Path) -> list[bytes]:
    raw = path.read_bytes()
    if not raw.startswith(b"\x89PNG\r\n\x1a\n"):
        raise AssertionError("README banner is not a PNG")
    offset = 8
    chunks = []
    while offset + 12 <= len(raw):
        length = struct.unpack(">I", raw[offset : offset + 4])[0]
        kind = raw[offset + 4 : offset + 8]
        end = offset + 12 + length
        if end > len(raw):
            raise AssertionError("README banner has a truncated PNG chunk")
        if kind == b"tEXt":
            chunks.append(raw[offset + 8 : offset + 8 + length])
        offset = end
        if kind == b"IEND":
            break
    return chunks


def _mirror_errors() -> list[str]:
    source = ROOT / "docs"
    mirror = ROOT / "src" / "eltdx" / "docs"
    source_files = {path.relative_to(source): path for path in source.rglob("*") if path.is_file()}
    mirror_files = {path.relative_to(mirror): path for path in mirror.rglob("*") if path.is_file()}
    errors = []
    if set(source_files) != set(mirror_files):
        errors.append("package docs mirror file inventory differs from root docs")
        return errors
    for relative, path in source_files.items():
        if path.read_bytes() != mirror_files[relative].read_bytes():
            errors.append(f"package docs mirror differs: {relative}")
    return errors


def check(
    *,
    artifact_dir: Path | None = None,
    ref: str | None = None,
    sha: str | None = None,
    allow_dry_run_ref: bool = False,
) -> list[str]:
    errors: list[str] = []
    cargo = _cargo_version()
    if cargo != TARGET_CARGO_VERSION or _normalize_cargo(cargo) != TARGET_PYTHON_VERSION:
        errors.append(f"Cargo/Python target version mismatch: {cargo}")
    pyproject = (ROOT / "pyproject.toml").read_text(encoding="utf-8")
    if 'dynamic = ["version"]' not in pyproject or 'module-name = "eltdx._native"' not in pyproject:
        errors.append("pyproject does not use dynamic native package versioning")
    if 'Development Status :: 5 - Production/Stable' not in pyproject:
        errors.append("pyproject does not classify the release as production/stable")
    required = {
        "README.md": TARGET_PYTHON_VERSION,
        "docs/CHANGELOG.md": f"v{TARGET_PYTHON_VERSION}",
        "docs/releases/v3.0.6.md": f"v{TARGET_PYTHON_VERSION}",
        "mkdocs.yml": "releases/v3.0.6.md",
        "docs/assets/interface-catalog-data.js": f'"version": "{TARGET_PYTHON_VERSION}"',
    }
    for relative, needle in required.items():
        if needle not in (ROOT / relative).read_text(encoding="utf-8"):
            errors.append(f"{relative} does not contain {needle!r}")
    banner = ROOT / ".github" / "assets" / "eltdx-readme-banner-v3.0.5.png"
    if BANNER_KEYWORD not in _png_text_chunks(banner):
        errors.append(f"README banner lacks eltdx_release={TARGET_PYTHON_VERSION} PNG metadata")
    errors.extend(_mirror_errors())
    release_values = (artifact_dir, ref, sha)
    if any(value is not None for value in release_values):
        if artifact_dir is None or ref is None or sha is None:
            errors.append("artifact-dir, ref, and sha must be supplied together")
        else:
            expected_ref = f"refs/tags/v{TARGET_PYTHON_VERSION}"
            if ref != expected_ref and not allow_dry_run_ref:
                errors.append(f"release ref differs: expected {expected_ref!r}, got {ref!r}")
            try:
                inventory = verify_artifacts(artifact_dir, candidate=sha)
            except (FileNotFoundError, RuntimeError, ValueError) as error:
                errors.append(f"release artifact verification failed: {error}")
            else:
                if inventory["version"] != TARGET_PYTHON_VERSION:
                    errors.append(f"release artifact version differs: {inventory['version']!r}")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description="Check frozen release metadata")
    parser.add_argument("--artifact-dir", type=Path)
    parser.add_argument("--ref")
    parser.add_argument("--sha")
    parser.add_argument(
        "--allow-dry-run-ref",
        action="store_true",
        help="allow a non-tag ref for workflow_dispatch verification only",
    )
    args = parser.parse_args()
    errors = check(
        artifact_dir=args.artifact_dir,
        ref=args.ref,
        sha=args.sha,
        allow_dry_run_ref=args.allow_dry_run_ref,
    )
    for error in errors:
        print(error)
    return 1 if errors else 0


if __name__ == "__main__":
    raise SystemExit(main())
