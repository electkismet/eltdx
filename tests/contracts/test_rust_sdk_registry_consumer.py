"""Publication evidence must come from the public registry and its Cargo lock."""

from __future__ import annotations

import io
import json
import sys
import tarfile
import urllib.error
from pathlib import Path

import pytest

tomllib = pytest.importorskip("tomllib", reason="release verification requires Python 3.11+")
rust_sdk_packages = pytest.importorskip("scripts.verification.rust_sdk_packages")


def test_unpublished_crates_leave_failure_evidence(tmp_path, monkeypatch) -> None:
    result = tmp_path / "unpublished"
    monkeypatch.setattr(sys, "argv", ["packages", "registry-consumer", "--implementation-complete", "--output", str(result)])

    def run(arguments, *, stdout, **kwargs):
        if arguments == ["git", "rev-parse", "HEAD"]:
            stdout.write(b"0123456789012345678901234567890123456789\n")
        return rust_sdk_packages.subprocess.CompletedProcess(arguments, 0)

    def not_published(request, *, timeout):
        raise urllib.error.HTTPError(request.full_url, 404, "Not found", {}, io.BytesIO(b"{}"))

    monkeypatch.setattr(rust_sdk_packages.subprocess, "run", run)
    monkeypatch.setattr(rust_sdk_packages.urllib.request, "urlopen", not_published)
    with pytest.raises(RuntimeError, match="not a unique unyanked registry version: eltdx-protocol"):
        rust_sdk_packages.main()
    report = json.loads((result / "packages.json").read_text(encoding="utf-8"))
    assert report["status"] == "failed"
    assert [entry["http_status"] for entry in report["registry"]] == [404, 404, 404]
    assert not any("generate-lockfile" in item["arguments"] for item in report["commands"])


@pytest.mark.parametrize(
    ("modified_crate", "field", "actual", "error"),
    [
        ("eltdx", "source", None, "origin"),
        ("eltdx-runtime", "source", "registry+https://mirror.test/index", "origin"),
        ("eltdx-protocol", "checksum", "0" * 64, "checksum"),
        ("eltdx-protocol", "version", "9.9.9", "version"),
    ],
)
def test_registry_lock_rejects_a_different_resolved_package(
    tmp_path: Path, modified_crate: str, field: str, actual: str | None, error: str
) -> None:
    versions = {"eltdx": "0.1.0", "eltdx-runtime": "3.3.5", "eltdx-protocol": "3.3.5"}
    checksums = dict(zip(versions, ("1" * 64, "2" * 64, "3" * 64), strict=True))
    lock = tmp_path / "Cargo.lock"
    contents = ['[[package]]\nname = "eltdx-independent-consumer"\nversion = "0.0.0"']
    for name, version in versions.items():
        record = {
            "name": name, "version": version, "checksum": checksums[name],
            "source": "registry+https://github.com/rust-lang/crates.io-index",
        }
        if name == modified_crate:
            if actual is None:
                del record[field]
            else:
                record[field] = actual
        contents.append("[[package]]\n" + "\n".join(f"{key} = {json.dumps(value)}" for key, value in record.items()))
    lock.write_text("\n\n".join(contents) + "\n", encoding="utf-8")
    with pytest.raises(RuntimeError, match=error):
        rust_sdk_packages.verify_registry_lock(lock, versions, checksums)


def test_registry_lock_accepts_verified_versions_and_checksums(tmp_path: Path) -> None:
    checksum = "a" * 64
    lock = tmp_path / "Cargo.lock"
    lock.write_text(
        '[[package]]\nname = "eltdx-independent-consumer"\nversion = "0.0.0"\n'
        '[[package]]\nname = "eltdx"\nversion = "0.1.0"\n'
        'source = "registry+https://github.com/rust-lang/crates.io-index"\n'
        f'checksum = "{checksum}"\n',
        encoding="utf-8",
    )
    resolved = rust_sdk_packages.verify_registry_lock(lock, {"eltdx": "0.1.0"}, {"eltdx": checksum})
    assert resolved["eltdx"]["checksum"] == checksum


def test_published_consumer_uses_only_verified_registry_packages(tmp_path, monkeypatch) -> None:
    from hashlib import sha256

    versions = {"eltdx-protocol": "3.3.5", "eltdx-runtime": "3.3.5", "eltdx": "0.1.0"}
    root = tmp_path / "checkout"
    root.mkdir()
    (root / "Cargo.toml").write_text("[workspace]\n", encoding="utf-8")
    (root / "Cargo.lock").touch()
    (root / "LICENSE").write_bytes(b"test license\n")
    consumer = root / "tests/consumers/rust-sdk"
    (consumer / "src").mkdir(parents=True)
    (consumer / "Cargo.toml").write_bytes((rust_sdk_packages.ROOT / "tests/consumers/rust-sdk/Cargo.toml").read_bytes())
    (consumer / "src/main.rs").write_bytes(b"fn main() {}\n")
    responses = {}
    checksums = {}
    for name, version in versions.items():
        crate = root / "crates" / name
        (crate / "src").mkdir(parents=True)
        package_manifest = f'[package]\nname = "{name}"\nversion = "{version}"\n'.encode()
        (crate / "Cargo.toml").write_bytes(package_manifest)
        source_code = b"pub struct Example;\n"
        (crate / "src/lib.rs").write_bytes(source_code)
        archive = io.BytesIO()
        with tarfile.open(fileobj=archive, mode="w:gz") as archive_file:
            for path, data in {
                "Cargo.toml": package_manifest, "LICENSE": b"test license\n",
                "README.md": b"test crate\n", "src/lib.rs": source_code,
            }.items():
                info = tarfile.TarInfo(f"{name}-{version}/{path}")
                info.size = len(data)
                archive_file.addfile(info, io.BytesIO(data))
        digest = sha256(archive.getvalue()).hexdigest()
        checksums[name] = digest
        base = f"https://crates.io/api/v1/crates/{name}"
        responses[base] = json.dumps({"versions": [{"num": version, "yanked": False, "checksum": digest}]}).encode()
        responses[f"{base}/{version}/download"] = archive.getvalue()

    def urlopen(request, *, timeout):
        response = io.BytesIO(responses[request.full_url])
        response.status = 200
        return response

    def run(arguments, *, stdout, cwd, env, **kwargs):
        if arguments == ["git", "rev-parse", "HEAD"]:
            stdout.write(b"0123456789012345678901234567890123456789\n")
        if "generate-lockfile" in arguments:
            manifest = tomllib.loads((cwd / "Cargo.toml").read_text(encoding="utf-8"))
            assert not cwd.is_relative_to(root)
            assert manifest["dependencies"] == {"eltdx": {"version": "=0.1.0", "default-features": False}}
            assert "patch" not in manifest and not (cwd / ".cargo").exists()
            assert Path(env["CARGO_HOME"]).parent == cwd.parent
            assert not Path(env["CARGO_HOME"]).exists()
            assert "CARGO_NET_OFFLINE" not in env
            records = ['[[package]]\nname = "eltdx-independent-consumer"\nversion = "0.0.0"']
            for name, version in versions.items():
                records.append(
                    f'[[package]]\nname = "{name}"\nversion = "{version}"\n'
                    'source = "registry+https://github.com/rust-lang/crates.io-index"\n'
                    f'checksum = "{checksums[name]}"'
                )
            (cwd / "Cargo.lock").write_text("\n\n".join(records), encoding="utf-8")
        return rust_sdk_packages.subprocess.CompletedProcess(arguments, 0)

    result = tmp_path / "published"
    monkeypatch.setattr(sys, "argv", ["packages", "registry-consumer", "--implementation-complete", "--output", str(result)])
    monkeypatch.setattr(rust_sdk_packages, "ROOT", root)
    monkeypatch.setattr(rust_sdk_packages.subprocess, "run", run)
    monkeypatch.setattr(rust_sdk_packages.urllib.request, "urlopen", urlopen)
    monkeypatch.setenv("CARGO_HOME", str(root))
    monkeypatch.setenv("CARGO_NET_OFFLINE", "true")

    rust_sdk_packages.main()

    report = json.loads((result / "packages.json").read_text(encoding="utf-8"))
    assert report["status"] == "passed"
    assert {entry["sha256"] for entry in report["artifacts"]} == set(checksums.values())
    assert set(report["consumer_registry_packages"]) == set(versions)
    assert (result / "consumer/Cargo.lock").is_file()
    runs = [entry["arguments"] for entry in report["commands"] if entry["arguments"][2:3] == ["run"]]
    assert [run[-1] for run in runs] == [
        "lazy", "loopback", "loopback", "lazy", "loopback", "f10", "offline", "financial", "ex", "mac", "mac-ex",
    ]
    assert all("--locked" in run for run in runs)
