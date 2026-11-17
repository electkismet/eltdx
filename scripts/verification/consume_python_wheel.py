"""Install and consume a wheel outside the checkout, without building sources.

Run only after all implementation sources are complete. A no-toolchain image
must assert absence before this script creates the environment. The optional
PATH restriction on hosted runners proves tool unavailability to the process,
not absence from the machine; the evidence records that distinction.
"""
from __future__ import annotations

import argparse
import email
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tempfile
import time
import venv
import zipfile


def sha256(path: Path) -> str:
    with path.open("rb") as stream:
        digest = hashlib.sha256()
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--wheelhouse", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--require-no-toolchain-image", action="store_true")
    parser.add_argument("--implementation-complete", action="store_true")
    args = parser.parse_args()
    if not args.implementation_complete:
        raise RuntimeError("Finish all T01-T30 implementation sources first")
    source = Path(__file__).resolve().parents[2]
    wheelhouse = args.wheelhouse.resolve()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    report_path = output / "installed-wheel.json"
    if report_path.exists():
        raise FileExistsError("Use a new evidence directory for every candidate/run")
    found_tools = {name: shutil.which(name) for name in ("rustc", "cargo", "maturin")}
    report = {
        "status": "running", "source_sha": args.candidate,
        "python": sys.version, "platform": platform.platform(),
        "machine": platform.machine(), "initial_toolchain_lookup": found_tools,
        "isolation": "no-toolchain-image" if args.require_no_toolchain_image else "process-path-only",
        "commands": [], "wheels": [],
    }

    def execute(command: list[str], cwd: Path, environment: dict[str, str]) -> None:
        index = len(report["commands"])
        log = output / ("%02d.log" % index)
        started = time.monotonic()
        exit_code = None
        timed_out = False
        try:
            with log.open("wb") as stream:
                process = subprocess.run(command, cwd=cwd, env=environment, stdout=stream,
                                         stderr=subprocess.STDOUT, check=False, timeout=240)
                exit_code = process.returncode
        except subprocess.TimeoutExpired:
            timed_out = True
            raise
        finally:
            report["commands"].append({
                "arguments": command, "cwd": str(cwd), "exit_code": exit_code, "timed_out": timed_out,
                "seconds": time.monotonic() - started, "log": log.name, "sha256": sha256(log),
            })
        if exit_code:
            raise RuntimeError("Installed-wheel command failed; see %s" % log)

    try:
        if args.require_no_toolchain_image and any(found_tools.values()):
            raise RuntimeError("The required clean image exposes Rust/maturin: %r" % found_tools)
        wheels = sorted(wheelhouse.glob("eltdx-*.whl"))
        if len(wheels) != 1:
            raise RuntimeError("Provide exactly one platform candidate wheel")
        wheel = wheels[0]
        with zipfile.ZipFile(wheel) as archive:
            metadata_files = [name for name in archive.namelist() if name.endswith(".dist-info/METADATA")]
            if len(metadata_files) != 1:
                raise RuntimeError("Wheel has ambiguous METADATA")
            metadata = email.message_from_bytes(archive.read(metadata_files[0]))
            if metadata["Name"] != "eltdx" or not metadata["Version"]:
                raise RuntimeError("Unexpected wheel identity")
            version = metadata["Version"]
            native = {name: hashlib.sha256(archive.read(name)).hexdigest()
                      for name in archive.namelist() if name.endswith((".so", ".pyd", ".dylib"))}
            if not native:
                raise RuntimeError("Candidate wheel contains no native binary")
            wheel_metadata = archive.read(metadata_files[0].rsplit("/", 1)[0] + "/WHEEL").decode("utf-8")
        report["wheels"].append({"name": wheel.name, "sha256": sha256(wheel), "version": version,
                                  "tags": [line[5:] for line in wheel_metadata.splitlines() if line.startswith("Tag: ")],
                                  "native_sha256": native})
        with tempfile.TemporaryDirectory(prefix="eltdx-wheel-consumer-") as directory:
            temporary = Path(directory).resolve()
            if temporary.is_relative_to(source):
                raise RuntimeError("Consumer directory must be outside the checkout")
            environment_root = temporary / "env"
            venv.EnvBuilder(with_pip=True).create(environment_root)
            executable_dir = environment_root / ("Scripts" if os.name == "nt" else "bin")
            python = executable_dir / ("python.exe" if os.name == "nt" else "python")
            consumer = temporary / "consumer.py"
            shutil.copyfile(source / "tests/consumers/python-wheel/consumer.py", consumer)
            environment = os.environ.copy()
            for key in ("PYTHONPATH", "PYTHONHOME", "PIP_INDEX_URL", "PIP_EXTRA_INDEX_URL", "PIP_FIND_LINKS"):
                environment.pop(key, None)
            # Absolute Python executable avoids reliance on this restricted PATH.
            environment["PATH"] = str(executable_dir)
            environment["PIP_CONFIG_FILE"] = os.devnull
            environment["PIP_NO_INDEX"] = "1"
            environment["PIP_DISABLE_PIP_VERSION_CHECK"] = "1"
            execute([str(python), "-I", "-m", "pip", "install", "--only-binary=:all:",
                     "--no-index", "--no-cache-dir", "--find-links", str(wheelhouse),
                     "eltdx==" + version], temporary, environment)
            execute([str(python), "-I", "-m", "pip", "check"], temporary, environment)
            consumer_evidence = output / "public-api.json"
            execute([str(python), "-I", str(consumer), "--forbidden-root", str(source),
                     "--require-no-toolchain", "--evidence", str(consumer_evidence)], temporary, environment)
            consumed = json.loads(consumer_evidence.read_text(encoding="utf-8"))
            if consumed["package_version"] != version or consumed["status"] != "passed":
                raise RuntimeError("Installed consumer did not verify the candidate version")
            installed_native = {entry["path"].replace("\\", "/"): entry["sha256"]
                                for entry in consumed["native_files"]}
            if installed_native != native:
                raise RuntimeError("Installed native binaries differ from candidate wheel bytes")
            report["consumer_sha256"] = sha256(consumer)
            report["public_api_evidence_sha256"] = sha256(consumer_evidence)
        report["status"] = "passed"
    except BaseException as error:
        report["status"] = "failed"
        report["error"] = "%s: %s" % (type(error).__name__, error)
        raise
    finally:
        report_path.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
