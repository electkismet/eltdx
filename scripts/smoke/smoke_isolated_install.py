from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys
import tempfile
import venv
from pathlib import Path

ROOT = next(parent for parent in Path(__file__).resolve().parents if (parent / "pyproject.toml").exists())
DIST = ROOT / "dist"
ABOUT = ROOT / "src" / "eltdx" / "__about__.py"


def read_version() -> str:
    text = ABOUT.read_text(encoding="utf-8")
    match = re.search(r'__version__\s*=\s*"([^"]+)"', text)
    if match is None:
        raise RuntimeError(f"cannot parse version from {ABOUT}")
    return match.group(1)


def run(command: list[str], *, cwd: Path | None = None) -> None:
    print("+", " ".join(command), flush=True)
    subprocess.run(command, check=True, cwd=cwd)


def find_wheel(version: str) -> Path:
    matches = sorted(DIST.glob(f"eltdx-{version}-*.whl"))
    if not matches:
        raise FileNotFoundError(f"wheel for version {version} not found in {DIST}")
    return matches[-1]


def python_in_venv(venv_dir: Path) -> Path:
    if os.name == "nt":
        return venv_dir / "Scripts" / "python.exe"
    return venv_dir / "bin" / "python"


def ensure_wheel(version: str, *, force_build: bool) -> Path:
    if force_build:
        run([sys.executable, "-m", "build", "--wheel"], cwd=ROOT)
        return find_wheel(version)
    try:
        return find_wheel(version)
    except FileNotFoundError:
        run([sys.executable, "-m", "build", "--wheel"], cwd=ROOT)
        return find_wheel(version)


def verify_install(python_executable: Path) -> None:
    check = (
        "from eltdx import TdxClient, __version__, CodePage, MinuteSeries, TradePage, KlinePage; "
        "from eltdx.services import CodesService, WorkdayService, GbbqService; "
        "client = TdxClient(); "
        "print('version=', __version__); "
        "print('client=', TdxClient.__name__); "
        "print('models=', CodePage.__name__, MinuteSeries.__name__, TradePage.__name__, KlinePage.__name__); "
        "print('services=', CodesService.__name__, WorkdayService.__name__, GbbqService.__name__); "
        "print('pool_size=', client._pool_size); "
        "client.close()"
    )
    run([str(python_executable), "-c", check])


def main() -> int:
    parser = argparse.ArgumentParser(description="Create an isolated venv, install the built wheel, and verify basic imports.")
    parser.add_argument("--wheel", default=None, help="Optional wheel path override")
    parser.add_argument("--venv-dir", default=None, help="Optional virtualenv path override")
    parser.add_argument("--build", action="store_true", help="Rebuild wheel before running the smoke check")
    parser.add_argument("--keep-venv", action="store_true", help="Keep the temporary virtualenv after success")
    args = parser.parse_args()

    version = read_version()
    wheel = Path(args.wheel).resolve() if args.wheel else ensure_wheel(version, force_build=args.build)
    if not wheel.exists():
        raise FileNotFoundError(f"wheel not found: {wheel}")

    temp_dir: Path | None = None
    if args.venv_dir:
        venv_dir = Path(args.venv_dir).resolve()
        keep_venv = True
    else:
        temp_dir = Path(tempfile.mkdtemp(prefix="eltdx-smoke-venv-"))
        venv_dir = temp_dir
        keep_venv = args.keep_venv

    print(f"using wheel: {wheel}", flush=True)
    print(f"using venv:  {venv_dir}", flush=True)

    try:
        builder = venv.EnvBuilder(with_pip=True, clear=True)
        builder.create(venv_dir)
        python_executable = python_in_venv(venv_dir)

        run([str(python_executable), "-m", "pip", "install", "--force-reinstall", "--no-deps", str(wheel)])
        verify_install(python_executable)
        print("smoke check passed", flush=True)
        return 0
    finally:
        if temp_dir is not None and not keep_venv:
            shutil.rmtree(temp_dir, ignore_errors=True)


if __name__ == "__main__":
    raise SystemExit(main())

