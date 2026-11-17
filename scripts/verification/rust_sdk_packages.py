"""Record package contents and isolated consumption after source completion.

Archive consumption uses explicit local patches for unpublished lower crates.
It never counts as registry acceptance. No mode publishes, tags or pushes.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import tarfile
import time
import tomllib
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
CRATES = ("eltdx-protocol", "eltdx-runtime", "eltdx")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def manifest(path: Path) -> dict:
    return tomllib.loads(path.read_text(encoding="utf-8"))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("archives", "registry-observation", "registry-dry-run"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--toolchain", default="1.89.0")
    parser.add_argument("--implementation-complete", action="store_true")
    args = parser.parse_args()
    if not args.implementation_complete:
        raise RuntimeError("Finish all T01-T30 implementation sources first")
    output = args.output.resolve()
    if output.exists():
        raise FileExistsError("Use a new output directory; preserve failed evidence")
    output.mkdir(parents=True)
    workspace = manifest(ROOT / "Cargo.toml")
    packages = {name: manifest(ROOT / "crates" / name / "Cargo.toml") for name in CRATES}
    versions = {}
    for name, package in packages.items():
        version = package["package"]["version"]
        versions[name] = workspace["workspace"]["package"]["version"] if isinstance(version, dict) else version
    report = {
        "status": "running", "mode": args.mode, "versions": versions, "source_sha": None,
        "commands": [], "artifacts": [], "registry": [],
        "acceptance": "local-patch-simulation" if args.mode == "archives" else args.mode,
    }
    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = str(output / "target")

    def run(arguments: list[str], cwd: Path = ROOT) -> str:
        log = output / ("%03d.log" % len(report["commands"]))
        started = time.monotonic()
        exit_code = None
        timed_out = False
        try:
            with log.open("wb") as stream:
                result = subprocess.run(arguments, cwd=cwd, env=environment, stdout=stream,
                                        stderr=subprocess.STDOUT, check=False, timeout=1800)
                exit_code = result.returncode
        except subprocess.TimeoutExpired:
            timed_out = True
            raise
        finally:
            report["commands"].append({
                "arguments": arguments, "cwd": str(cwd), "exit_code": exit_code, "timed_out": timed_out,
                "seconds": time.monotonic() - started, "log": log.name, "sha256": sha256(log),
            })
        if exit_code:
            raise RuntimeError("Package command failed; see %s" % log)
        return log.read_text(encoding="utf-8", errors="replace")

    def cargo(arguments: list[str], cwd: Path = ROOT) -> str:
        return run(["cargo", "+" + args.toolchain, *arguments], cwd)

    def inspect_archive(archive: Path, name: str, unpack: Path) -> Path:
        expected_root = name + "-" + versions[name]
        files = {}
        with tarfile.open(archive, "r:gz") as source:
            for member in source.getmembers():
                path = PurePosixPath(member.name)
                if (path.is_absolute() or ".." in path.parts or not path.parts or path.parts[0] != expected_root
                        or any("\\" in part or ":" in part for part in path.parts)):
                    raise RuntimeError("Archive contains invalid path: " + member.name)
                if member.issym() or member.islnk() or not (member.isfile() or member.isdir()):
                    raise RuntimeError("Archive contains unsupported member: " + member.name)
                target = unpack.joinpath(*path.parts)
                if member.isdir():
                    target.mkdir(parents=True, exist_ok=True)
                    continue
                target.parent.mkdir(parents=True, exist_ok=True)
                if target.exists():
                    raise RuntimeError("Duplicate archive member: " + member.name)
                stream = source.extractfile(member)
                if stream is None:
                    raise RuntimeError("Cannot read archive member: " + member.name)
                with stream, target.open("xb") as destination:
                    shutil.copyfileobj(stream, destination)
                files[str(path.relative_to(expected_root))] = sha256(target)
        for required in ("Cargo.toml", "README.md", "LICENSE", "src/lib.rs"):
            if required not in files:
                raise RuntimeError("Missing package member: " + required)
        if (unpack / expected_root / "LICENSE").read_bytes() != (ROOT / "LICENSE").read_bytes():
            raise RuntimeError("Package changed the repository license")
        normalized = manifest(unpack / expected_root / "Cargo.toml")
        if normalized["package"]["version"] != versions[name]:
            raise RuntimeError("Normalized package version changed")
        for section in ("dependencies", "build-dependencies"):
            for dependency, value in normalized.get(section, {}).items():
                if isinstance(value, dict) and "path" in value:
                    raise RuntimeError("Normalized package retained a local dependency path")
                if dependency in CRATES and (not isinstance(value, dict) or "version" not in value):
                    raise RuntimeError("Internal package dependency lost its version")
        report["artifacts"].append({"name": name, "version": versions[name], "archive": str(archive),
                                    "sha256": sha256(archive), "files": files})
        return unpack / expected_root

    def registry_observation() -> None:
        for name in CRATES:
            url = "https://crates.io/api/v1/crates/" + name
            request = urllib.request.Request(url, headers={"User-Agent": "eltdx-candidate-verifier/0.1 (https://github.com/electkismet/eltdx)"})
            try:
                with urllib.request.urlopen(request, timeout=30) as response:
                    status, body = response.status, response.read(4 * 1024 * 1024 + 1)
            except urllib.error.HTTPError as error:
                if error.code != 404:
                    raise
                status, body = error.code, error.read(4 * 1024 * 1024 + 1)
            if len(body) > 4 * 1024 * 1024:
                raise RuntimeError("Registry response exceeds evidence bound")
            raw = output / (name + "-registry.json")
            raw.write_bytes(body)
            record = {"name": name, "url": url, "http_status": status, "raw": raw.name,
                      "sha256": sha256(raw), "observed_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
            report["registry"].append(record)
            if status == 404:
                record["state"] = "not-found-at-observation"
            else:
                data = json.loads(body)
                record["state"] = "existing-name"
                record["matching_versions"] = [entry for entry in data["versions"] if entry["num"] == versions[name]]
            record["limitation"] = "Read-only observation; does not prove naming rights, publish authorization or candidate source identity."

    try:
        report["source_sha"] = run(["git", "rev-parse", "HEAD"]).strip()
        report["worktree_status"] = run(["git", "status", "--porcelain"]).splitlines()
        if report["worktree_status"]:
            raise RuntimeError("Package candidate requires a committed clean source checkout")
        cargo(["--version", "--verbose"])
        run(["rustc", "+" + args.toolchain, "-Vv"])
        report["source_lock_sha256"] = sha256(ROOT / "Cargo.lock")
        for name, package in packages.items():
            for dependency, value in package.get("dependencies", {}).items():
                if dependency in CRATES and ("path" not in value or "version" not in value):
                    raise RuntimeError("Every internal dependency needs path and version: " + name)
        if args.mode == "registry-observation":
            registry_observation()
        elif args.mode == "registry-dry-run":
            registry_observation()
            identity = output / "published-lower"
            identity.mkdir()
            for name in CRATES[:-1]:
                record = next(entry for entry in report["registry"] if entry["name"] == name)
                matches = record.get("matching_versions", [])
                if len(matches) != 1 or matches[0]["yanked"]:
                    raise RuntimeError("Lower candidate is not a unique unyanked registry version: " + name)
                request = urllib.request.Request(
                    "https://crates.io/api/v1/crates/" + name + "/" + versions[name] + "/download",
                    headers={"User-Agent": "eltdx-candidate-verifier/0.1 (https://github.com/electkismet/eltdx)"})
                archive = identity / (name + "-" + versions[name] + ".crate")
                with urllib.request.urlopen(request, timeout=30) as response, archive.open("xb") as stream:
                    total = 0
                    while chunk := response.read(1024 * 1024):
                        total += len(chunk)
                        if total > 64 * 1024 * 1024:
                            raise RuntimeError("Published archive exceeds identity inspection bound")
                        stream.write(chunk)
                if sha256(archive) != matches[0]["checksum"]:
                    raise RuntimeError("Published archive checksum differs from registry response")
                published = inspect_archive(archive, name, identity)
                candidate_sources = {path.relative_to(ROOT / "crates" / name).as_posix(): sha256(path)
                                     for path in (ROOT / "crates" / name / "src").rglob("*") if path.is_file()}
                published_sources = {path.relative_to(published).as_posix(): sha256(path)
                                     for path in (published / "src").rglob("*") if path.is_file()}
                if candidate_sources != published_sources:
                    raise RuntimeError("Published lower version contains different source; choose a new version: " + name)
                record["candidate_source_identity"] = "exact-src-file-bytes-and-license"
            for name in CRATES:
                cargo(["package", "-p", name, "--locked"])
                cargo(["publish", "-p", name, "--dry-run", "--locked"])
            report["acceptance"] = "package-and-registry-dry-run-only-no-publication"
        else:
            # A fresh staging workspace is allowed to have its own lock and
            # local patch table. The committed source manifest is never edited.
            staging = output / "staging"
            staging.mkdir()
            for name in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "LICENSE", "README.md"):
                shutil.copyfile(ROOT / name, staging / name)
            shutil.copytree(ROOT / "crates", staging / "crates", ignore=shutil.ignore_patterns("target", "__pycache__"))
            staging_manifest = staging / "Cargo.toml"
            with staging_manifest.open("a", encoding="utf-8") as stream:
                stream.write('\n[patch.crates-io]\n')
                for name in CRATES[:-1]:
                    stream.write(name + ' = { path = "crates/' + name + '" }\n')
            report["local_patch_sources"] = {name: str(staging / "crates" / name) for name in CRATES[:-1]}
            cargo(["generate-lockfile"], staging)
            report["staging_lock_sha256"] = sha256(staging / "Cargo.lock")
            unpack = output / "unpacked"
            unpack.mkdir()
            unpacked = {}
            for name in CRATES:
                cargo(["package", "-p", name, "--list", "--locked"], staging)
                cargo(["package", "-p", name, "--no-verify", "--locked"], staging)
                archive = output / "target/package" / (name + "-" + versions[name] + ".crate")
                unpacked[name] = inspect_archive(archive, name, unpack)
            consumer = output / "consumer"
            consumer.mkdir()
            shutil.copytree(ROOT / "tests/consumers/rust-sdk/src", consumer / "src")
            text = (ROOT / "tests/consumers/rust-sdk/Cargo.toml").read_text(encoding="utf-8")
            # Registry-style version dependency with explicit candidate patches;
            # all three point to unpacked archives, never the checkout/staging.
            text = text.replace('path = "../../../crates/eltdx"', 'version = "=' + versions["eltdx"] + '"')
            text += '\n[patch.crates-io]\n'
            for name in CRATES:
                text += name + ' = { path = ' + json.dumps(unpacked[name].as_posix()) + ' }\n'
            (consumer / "Cargo.toml").write_text(text, encoding="utf-8")
            report["consumer_patch_sources"] = {name: str(path) for name, path in unpacked.items()}
            environment["CARGO_TARGET_DIR"] = str(output / "consumer-target")
            cargo(["generate-lockfile"], consumer)
            cargo(["run", "--locked", "--no-default-features", "--", "lazy"], consumer)
            cargo(["run", "--locked", "--no-default-features", "--", "loopback"], consumer)
            cargo(["run", "--locked", "--", "loopback"], consumer)
            for mode in ("lazy", "loopback", "f10", "offline", "financial", "ex", "mac", "mac-ex"):
                cargo(["run", "--locked", "--no-default-features", "--features", "sdk-full", "--", mode], consumer)
            tree = cargo(["tree", "--locked", "--no-default-features", "--features", "sdk-full", "--edges", "normal,build"], consumer)
            if any(name in tree for name in ("pyo3", "eltdx-python", "maturin")):
                raise RuntimeError("Archive consumer contains Python discovery/bindings")
            report["consumer_lock_sha256"] = sha256(consumer / "Cargo.lock")
            report["limitation"] = "--no-verify packaging plus unpacked local patches; no public registry identity or publish success."
        report["status"] = "passed"
    except BaseException as error:
        report["status"] = "failed"
        report["error"] = "%s: %s" % (type(error).__name__, error)
        raise
    finally:
        (output / "packages.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
