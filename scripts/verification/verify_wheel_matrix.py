"""Install and smoke the one release wheel matching this runner."""

from __future__ import annotations

import argparse
import os
import platform as platform_module
import subprocess
import sys
import tempfile
import venv
from pathlib import Path

if __package__:
    from .verify_release_artifacts import inspect_release_wheel, verify_artifacts
else:
    from verify_release_artifacts import inspect_release_wheel, verify_artifacts


ROOT = Path(__file__).resolve().parents[2]


def _python(venv_root: Path) -> Path:
    return (
        venv_root / "Scripts" / "python.exe"
        if os.name == "nt"
        else venv_root / "bin" / "python"
    )


def verify_wheel(
    artifact_dir: Path,
    *,
    platform: str,
    python_version: str,
) -> None:
    inventory = verify_artifacts(artifact_dir, candidate=None)
    wheel_name = inventory["platforms"].get(platform)
    if wheel_name is None:
        raise ValueError(f"no wheel for platform {platform!r}")
    wheel = artifact_dir.resolve() / wheel_name
    _smoke_wheel(
        wheel,
        expected_version=inventory["version"],
        platform=platform,
        python_version=python_version,
    )


def verify_single_built_wheel(
    artifact_dir: Path,
    *,
    platform: str,
    python_version: str,
) -> None:
    artifact_dir = artifact_dir.resolve()
    if not artifact_dir.is_dir():
        raise FileNotFoundError(artifact_dir)
    files = sorted(path for path in artifact_dir.iterdir() if path.is_file())
    wheels = [path for path in files if path.suffix == ".whl"]
    if len(files) != 1 or len(wheels) != 1:
        raise ValueError(
            "single-wheel smoke requires exactly one wheel and no other files; "
            f"found {[path.name for path in files]!r}"
        )
    inspected = inspect_release_wheel(wheels[0])
    if inspected["platform"] != platform:
        raise ValueError(
            f"built wheel platform {inspected['platform']} does not match requested {platform}"
        )
    _smoke_wheel(
        wheels[0],
        expected_version=inspected["version"],
        platform=platform,
        python_version=python_version,
    )


def _smoke_wheel(
    wheel: Path,
    *,
    expected_version: str,
    platform: str,
    python_version: str,
) -> None:
    actual_python = f"{sys.version_info.major}.{sys.version_info.minor}"
    if actual_python != python_version:
        raise RuntimeError(f"runner Python {actual_python} does not match requested {python_version}")
    actual_platform = _current_platform()
    if actual_platform != platform:
        raise RuntimeError(f"runner platform {actual_platform} does not match requested {platform}")

    with tempfile.TemporaryDirectory(prefix="eltdx-wheel-smoke-") as temporary:
        environment_root = Path(temporary) / "venv"
        venv.EnvBuilder(with_pip=True, clear=True).create(environment_root)
        python = _python(environment_root)
        environment = dict(os.environ)
        environment.pop("PYTHONPATH", None)
        environment["PATH"] = os.pathsep.join(
            item
            for item in (str(python.parent), environment.get("PATH"))
            if item
        )
        subprocess.run(
            (str(python), "-m", "pip", "install", "--disable-pip-version-check", f"{wheel}[mcp]"),
            check=True,
            cwd=temporary,
            env=environment,
        )
        subprocess.run(
            (str(python), "-m", "pip", "check"),
            check=True,
            cwd=temporary,
            env=environment,
        )
        subprocess.run(
            (
                str(python),
                str(ROOT / "scripts" / "verification" / "installed_smoke.py"),
                "--expected-version",
                expected_version,
            ),
            check=True,
            cwd=temporary,
            env=environment,
        )
        scripts_dir = python.parent
        for executable in ("eltdx-smoke", "eltdx-f10-smoke", "eltdx-mcp"):
            suffix = ".exe" if os.name == "nt" else ""
            if not (scripts_dir / f"{executable}{suffix}").is_file():
                raise FileNotFoundError(f"installed console script is missing: {executable}")


def _current_platform() -> str:
    system = platform_module.system().lower()
    machine = platform_module.machine().lower()
    if system == "windows" and machine in {"amd64", "x86_64"}:
        return "windows-x86_64"
    if system == "linux" and machine == "x86_64":
        return "linux-x86_64"
    if system == "linux" and machine in {"aarch64", "arm64"}:
        return "linux-aarch64"
    if system == "darwin" and machine == "x86_64":
        return "macos-x86_64"
    if system == "darwin" and machine == "arm64":
        return "macos-arm64"
    raise RuntimeError(f"unsupported wheel smoke runner: system={system}, machine={machine}")


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Smoke one native eltdx wheel")
    parser.add_argument("--artifact-dir", type=Path, required=True)
    parser.add_argument("--platform")
    parser.add_argument("--python-version")
    parser.add_argument("--single-wheel", action="store_true")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    target_platform = args.platform if args.platform is not None else _current_platform()
    target_python = (
        args.python_version
        if args.python_version is not None
        else f"{sys.version_info.major}.{sys.version_info.minor}"
    )
    verifier = verify_single_built_wheel if args.single_wheel else verify_wheel
    verifier(args.artifact_dir, platform=target_platform, python_version=target_python)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
