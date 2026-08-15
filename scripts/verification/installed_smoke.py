"""Smoke an installed eltdx wheel without using the source tree."""

from __future__ import annotations

import argparse
import importlib.metadata
import importlib.resources
import os
import subprocess
import sys
from pathlib import Path


def installed_smoke(expected_version: str) -> None:
    import eltdx
    from eltdx import _native
    from eltdx._native_abi import EXPECTED_NATIVE_ABI_VERSION

    installed_version = importlib.metadata.version("eltdx")
    if installed_version != expected_version or eltdx.__version__ != expected_version:
        raise AssertionError(
            f"installed version mismatch: metadata={installed_version!r}, "
            f"module={eltdx.__version__!r}, expected={expected_version!r}"
        )
    if _native.ABI_VERSION != EXPECTED_NATIVE_ABI_VERSION:
        raise AssertionError(
            f"native ABI mismatch: native={_native.ABI_VERSION}, expected={EXPECTED_NATIVE_ABI_VERSION}"
        )

    package = importlib.resources.files("eltdx")
    for relative in (
        "py.typed",
        "tdx_server.json",
        "docs/index.md",
        "docs/API_REFERENCE.md",
        "docs/MCP.md",
    ):
        if not package.joinpath(relative).is_file():
            raise AssertionError(f"installed package resource is missing: {relative}")

    scripts_dir = Path(sys.executable).parent
    suffix = ".exe" if os.name == "nt" else ""
    environment = dict(os.environ)
    environment.pop("PYTHONPATH", None)
    for name in ("eltdx-smoke", "eltdx-f10-smoke"):
        subprocess.run(
            (str(scripts_dir / f"{name}{suffix}"), "--help"),
            check=True,
            timeout=20,
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
        )
    subprocess.run(
        (
            sys.executable,
            str(Path(__file__).resolve().parents[1] / "smoke" / "mcp_stdio_check.py"),
        ),
        check=True,
        timeout=30,
        env=environment,
    )


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Smoke an installed eltdx distribution")
    parser.add_argument("--expected-version", required=True)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    installed_smoke(args.expected_version)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
