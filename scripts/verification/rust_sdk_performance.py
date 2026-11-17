"""Build release-wheel provenance or measure paired isolated SDK workloads.

No benchmark is run before the all-source-complete gate. build-wheel does not
publish. run requires actual baseline/candidate wheel build records and installs
them separately without an index or source fallback. All raw failures survive.
"""
from __future__ import annotations
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import statistics
import subprocess
import sys
import time
import venv
import zipfile
from performance_fixture import Peer, dat

ROOT = Path(__file__).resolve().parents[2]
PROFILE = {"CARGO_PROFILE_RELEASE_OPT_LEVEL": "3", "CARGO_PROFILE_RELEASE_DEBUG": "0",
           "CARGO_PROFILE_RELEASE_LTO": "false", "CARGO_PROFILE_RELEASE_CODEGEN_UNITS": "16",
           "CARGO_PROFILE_RELEASE_INCREMENTAL": "false", "CARGO_PROFILE_RELEASE_PANIC": "unwind"}
SAMPLES, WARMUP, TRIALS = 500, 50, 7


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def save(path, value):
    Path(path).write_text(json.dumps(value, ensure_ascii=True, indent=2) + "\n", encoding="utf-8")


def utc():
    return datetime.now(timezone.utc).isoformat()


def git(root, *arguments):
    return subprocess.check_output(["git", *arguments], cwd=root, encoding="utf-8").strip()


def source_sha256(revision, name):
    content = subprocess.check_output(["git", "cat-file", "blob", revision + ":" + name], cwd=ROOT)
    return hashlib.sha256(content).hexdigest()


def environment(output, toolchain):
    if any(os.environ.get(name) for name in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS")):
        raise ValueError("Remove custom Rust flags for comparable release measurements")
    result = os.environ.copy()
    result.update(PROFILE)
    result.update(ELTDX_IMPLEMENTATION_COMPLETE="yes", CARGO_TARGET_DIR=str(output / "target"),
                  RUSTUP_TOOLCHAIN=toolchain)
    result.pop("PYTHONPATH", None)
    return result


def execute(arguments, cwd, output, name, env, report):
    path = output / name
    started = time.monotonic()
    record = dict(command=arguments, cwd=str(cwd), started_utc=utc(), exit_code=None)
    try:
        with path.open("wb") as log:
            record["exit_code"] = subprocess.run(arguments, cwd=cwd, env=env, stdout=log,
                stderr=subprocess.STDOUT, check=False, timeout=1800).returncode
    finally:
        record.update(ended_utc=utc(), elapsed_seconds=time.monotonic() - started,
                      log=name, log_sha256=sha(path))
        report["commands"].append(record)
    if record["exit_code"] != 0:
        raise RuntimeError("Measurement preparation failed; see " + str(path))
    return path.read_text(encoding="utf-8", errors="replace")


def build_wheel(root, output, python, toolchain):
    if git(root, "status", "--porcelain", "--untracked-files=normal"):
        raise RuntimeError("Commit the wheel source candidate before building")
    output.mkdir(parents=True, exist_ok=False)
    env = environment(output, toolchain)
    report = dict(schema=1, kind="performance-wheel-build", status="running", source_sha=git(root, "rev-parse", "HEAD"),
                  profile=PROFILE, toolchain=toolchain, lock_sha256=sha(root / "Cargo.lock"), commands=[], started_utc=utc())
    try:
        report["rustc"] = execute(["rustc", "-Vv"], root, output, "rustc.log", env, report)
        report["cargo"] = execute(["cargo", "-V"], root, output, "cargo.log", env, report)
        report["python"] = execute([python, "-I", "-c", "import sys;print(sys.version)"], root, output, "python.log", env, report)
        execute(["maturin", "build", "--release", "--locked", "--interpreter", python, "--out", str(output / "wheels")],
                root, output, "build-wheel.log", env, report)
        wheels = list((output / "wheels").glob("*.whl"))
        if len(wheels) != 1:
            raise ValueError("Expected exactly one release wheel")
        report.update(wheel=str(wheels[0]), wheel_sha256=sha(wheels[0]), status="passed")
        if git(root, "rev-parse", "HEAD") != report["source_sha"] or git(root, "status", "--porcelain", "--untracked-files=normal"):
            raise RuntimeError("Wheel source changed during build")
    except BaseException as error:
        report.update(status="failed", error=repr(error))
        raise
    finally:
        report["ended_utc"] = utc()
        save(output / "wheel-build.json", report)


def wheel_record(path):
    record = json.loads(path.read_text(encoding="utf-8"))
    if record.get("kind") != "performance-wheel-build" or record.get("status") != "passed" or record.get("profile") != PROFILE:
        raise ValueError("A real comparable release wheel build record is required")
    portable = path.parent / "wheels" / Path(record["wheel"]).name
    if portable.is_file():
        record["wheel"] = str(portable.resolve())
    if len(record["source_sha"]) != 40 or sha(record["wheel"]) != record["wheel_sha256"]:
        raise ValueError("Wheel candidate/provenance mismatch")
    build = [item for item in record["commands"] if item["command"][:2] == ["maturin", "build"]]
    if len(build) != 1 or "--release" not in build[0]["command"] or "--locked" not in build[0]["command"]:
        raise ValueError("Missing actual release/locked wheel build")
    for item in record["commands"]:
        if item["exit_code"] != 0 or sha(path.parent / item["log"]) != item["log_sha256"]:
            raise ValueError("Failed or altered wheel preparation evidence")
    with zipfile.ZipFile(record["wheel"]) as archive:
        native = [name for name in archive.namelist() if name.startswith("eltdx/_native") and name.endswith((".pyd", ".so"))]
        if len(native) != 1:
            raise ValueError("Require exactly one candidate native extension")
        record["native_sha256"] = hashlib.sha256(archive.read(native[0])).hexdigest()
    return record


def copy_build(path, directory):
    built = wheel_record(path)
    directory.mkdir(parents=True)
    shutil.copyfile(path, directory / "wheel-build.json")
    for item in built["commands"]:
        shutil.copyfile(path.parent / item["log"], directory / item["log"])
    (directory / "wheels").mkdir()
    shutil.copyfile(built["wheel"], directory / "wheels" / Path(built["wheel"]).name)
    return wheel_record(directory / "wheel-build.json")


def observe(arguments, cwd, output, name, env):
    import psutil
    raw, resources = output / (name + ".ndjson"), output / (name + "-resources.ndjson")
    start = time.monotonic()
    record = dict(command=arguments, cwd=str(cwd), started_utc=utc(), exit_code=None, error=None,
                  raw=raw.name, resources=resources.name)
    job = None
    with raw.open("wb") as stdout, resources.open("x", encoding="utf-8") as samples:
        try:
            job = subprocess.Popen(arguments, cwd=cwd, env=env, stdout=stdout, stderr=subprocess.STDOUT)
            process = psutil.Process(job.pid)
            record["pid"] = job.pid
            while job.poll() is None:
                if time.monotonic() - start > 300:
                    raise TimeoutError(name)
                try:
                    with process.oneshot():
                        cpu = process.cpu_times()
                        point = dict(unix_ns=time.time_ns(), rss=process.memory_info().rss,
                                     threads=process.num_threads(), cpu_seconds=cpu.user + cpu.system)
                    samples.write(json.dumps(point) + "\n")
                except psutil.NoSuchProcess:
                    if job.poll() is None:
                        raise
                    break
                time.sleep(.01)
        except BaseException as error:
            record["error"] = repr(error)
            if job is not None and job.poll() is None:
                job.kill()
        finally:
            if job is not None:
                try:
                    record["exit_code"] = job.wait(timeout=30)
                except BaseException as error:
                    record["error"] = record["error"] or repr(error)
    # Hash after both streams are closed: the final buffered resource samples
    # must be part of the immutable artifact, including failed runs.
    record.update(ended_utc=utc(), seconds=time.monotonic() - start,
                  raw_sha256=sha(raw), resources_sha256=sha(resources))
    return record


def summarize(output, record, native_hash=None, import_root=None):
    if record["exit_code"] != 0 or record["error"]:
        raise ValueError("Worker failed; keep raw stdout: " + record["raw"])
    if sha(output / record["raw"]) != record["raw_sha256"] or sha(output / record["resources"]) != record["resources_sha256"]:
        raise ValueError("Altered benchmark raw evidence")
    events = [json.loads(line) for line in (output / record["raw"]).read_text(encoding="utf-8").splitlines()]
    starts = [item for item in events if item["kind"] == "start"]
    finishes = [item for item in events if item["kind"] == "finish"]
    values = [item for item in events if item["kind"] == "sample"]
    if len(starts) != 1 or len(finishes) != 1 or starts[0]["samples"] != SAMPLES or starts[0]["warmup"] != WARMUP or starts[0]["concurrency"] != 1:
        raise ValueError("Changed benchmark workload")
    if events[0]["kind"] not in ("environment", "start") or events[-1]["kind"] != "finish" or starts[0]["mode"] != record["mode"]:
        raise ValueError("Worker mode or event boundaries changed")
    if finishes[0]["completed"] != SAMPLES or [item["id"] for item in values] != list(range(SAMPLES)):
        raise ValueError("Missing or repeated measured samples")
    if native_hash is not None:
        imported = [item for item in events if item["kind"] == "environment"]
        if len(imported) != 1 or imported[0]["native_sha256"] != native_hash:
            raise ValueError("Worker did not import the built wheel's actual native binary")
        expected_root = str(import_root or output).replace("\\", "/").rstrip("/").casefold() + "/python-"
        if not imported[0]["import_path"].replace("\\", "/").casefold().startswith(expected_root):
            raise ValueError("Python worker imported outside its isolated measurement environment")
    latencies = sorted(item["latency_ns"] for item in values)
    if any(type(value) is not int or value <= 0 for value in latencies):
        raise ValueError("Invalid raw latency")
    resources = [json.loads(line) for line in (output / record["resources"]).read_text(encoding="utf-8").splitlines()]
    if not resources:
        raise ValueError("Missing actual measured-process resources")
    return dict(samples=len(values), concurrency=1, p50_ns=latencies[int((SAMPLES - 1) * .50)],
                p95_ns=latencies[int((SAMPLES - 1) * .95)], p99_ns=latencies[int((SAMPLES - 1) * .99)],
                timed_operations_per_second=SAMPLES * 1e9 / sum(latencies),
                worker_wall_operations_per_second=SAMPLES / record["seconds"],
                sampled_process_cpu_seconds=max(item["cpu_seconds"] for item in resources),
                sampled_process_rss_peak=max(item["rss"] for item in resources),
                sampled_process_threads_peak=max(item["threads"] for item in resources))


def inspect_wire(output, record):
    if record["mode"] not in ("loopback", "close"):
        if "wire" in record:
            raise ValueError("Unexpected loopback traffic for a different mode")
        return
    path = output / record["wire"]
    errors_path = output / record["peer_errors"]
    if (sha(path) != record["wire_sha256"] or sha(errors_path) != record["peer_errors_sha256"]
            or json.loads(errors_path.read_text(encoding="utf-8"))):
        raise ValueError("Failed or altered actual server traffic")
    traffic = json.loads(path.read_text(encoding="utf-8"))
    if any(item["command"] not in (0x000d, 0x0004, 0x044d) for item in traffic):
        raise ValueError("Unexpected benchmark command")
    calls = [item for item in traffic if item["command"] == 0x044d]
    payloads = [bytes.fromhex(item["payload_hex"]) for item in calls]
    offsets = [int.from_bytes(payload[2:6], "little") for payload in payloads]
    if (any(len(payload) != 14 or payload[:2] != b"\x01\x00"
            or payload[6:] != b"\x01\x00\x00\x00\x00\x00\x00\x00" for payload in payloads)
            or offsets != [i % 1000 for i in range(SAMPLES + WARMUP)]):
        raise ValueError("Server workload differs across measurements")
    physical = {item["connection"] for item in calls}
    if len(physical) != (SAMPLES + WARMUP if record["mode"] == "close" else 1):
        raise ValueError("Physical connection reuse/close workload changed")


def run(output, baseline_path, candidate_path, toolchain, external_endpoint):
    baseline, candidate = wheel_record(baseline_path), wheel_record(candidate_path)
    if any(baseline[key] != candidate[key] for key in ("profile", "toolchain", "rustc", "cargo", "python")):
        raise ValueError("Baseline/candidate toolchain, Python or release profile differs")
    if toolchain != candidate["toolchain"] or candidate["source_sha"] != git(ROOT, "rev-parse", "HEAD"):
        raise ValueError("Candidate wheel is not the active source candidate")
    if baseline["source_sha"] == candidate["source_sha"]:
        raise ValueError("Require an actual pre-migration baseline, not two copies of the new implementation")
    if git(ROOT, "status", "--porcelain", "--untracked-files=normal"):
        raise ValueError("Commit the measurement candidate first")
    output.mkdir(parents=True, exist_ok=False)
    env = environment(output, toolchain)
    report = dict(schema=1, kind="rust-python-paired-performance", status="running", candidate_sha=candidate["source_sha"],
                  baseline_sha=baseline["source_sha"], candidate_build_sha256=sha(candidate_path), baseline_build_sha256=sha(baseline_path),
                  wheel_records={"baseline": baseline, "candidate": candidate}, profile=PROFILE, toolchain=toolchain,
                  samples=SAMPLES, warmup=WARMUP, trials=TRIALS, concurrency=1, seed=0,
                  platform=platform.platform(), cpu=platform.processor(), cpu_count=os.cpu_count(), started_utc=utc(),
                  commands=[], runs=[], scripts={}, external_endpoint=external_endpoint, external_status="unrun" if not external_endpoint else "requested")
    try:
        report["measurement_root"] = str(output)
        baseline = copy_build(baseline_path, output / "provenance/baseline")
        candidate = copy_build(candidate_path, output / "provenance/candidate")
        report["wheel_records"] = {"baseline": baseline, "candidate": candidate}
        (output / "scripts").mkdir()
        for name in ("rust_sdk_performance.py", "performance_fixture.py", "performance_python_worker.py"):
            report["scripts"][name] = sha(Path(__file__).with_name(name))
            shutil.copyfile(Path(__file__).with_name(name), output / "scripts" / name)
        report["script_blobs"] = {name: git(ROOT, "rev-parse", "HEAD:scripts/verification/" + name) for name in report["scripts"]}
        report["rust_worker_source_sha256"] = sha(ROOT / "crates/eltdx/examples/measure_candidate.rs")
        report["rust_worker_source_blob"] = git(ROOT, "rev-parse", "HEAD:crates/eltdx/examples/measure_candidate.rs")
        shutil.copyfile(ROOT / "crates/eltdx/examples/measure_candidate.rs", output / "measure_candidate.rs")
        worker = output / "worker.py"
        shutil.copyfile(Path(__file__).with_name("performance_python_worker.py"), worker)
        fixture = output / "fixture.dat"
        fixture.write_bytes(dat())
        report["fixture_sha256"] = sha(fixture)
        report["fixture_bytes"] = fixture.stat().st_size
        interpreters = {}
        for label, built in (("baseline", baseline), ("candidate", candidate)):
            directory = output / ("python-" + label)
            venv.EnvBuilder(with_pip=True).create(directory)
            python = directory / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
            execute([str(python), "-I", "-m", "pip", "install", "--no-index", "--only-binary=:all:", built["wheel"]],
                    output, output, label + "-install.log", env, report)
            interpreters[label] = str(python)
        build = execute(["cargo", "+" + toolchain, "build", "-p", "eltdx", "--example", "measure_candidate", "--locked",
                         "--release", "--no-default-features", "--features", "offline", "--message-format=json"],
                        ROOT, output, "build-rust.log", env, report)
        binaries = []
        for line in build.splitlines():
            try:
                item = json.loads(line)
            except json.JSONDecodeError:
                continue
            if item.get("reason") == "compiler-artifact" and item.get("target", {}).get("name") == "measure_candidate" and item.get("executable"):
                binaries.append(Path(item["executable"]))
        if len(binaries) != 1:
            raise ValueError("Could not identify actual release measurement executable")
        binary = output / ("measurement-bin.exe" if os.name == "nt" else "measurement-bin")
        shutil.copy2(binaries[0], binary)
        report["rust_binary"] = binary.name
        report["rust_binary_sha256"] = sha(binary)
        report["lock_sha256"] = sha(ROOT / "Cargo.lock")
        if report["lock_sha256"] != candidate["lock_sha256"]:
            raise ValueError("Rust measurement and candidate wheel used different Cargo.lock bytes")
        shutil.copyfile(ROOT / "Cargo.lock", output / "Cargo.lock")
        modes = [("rust", "core"), ("rust", "cancel"), ("rust", "loopback"), ("rust", "close")]
        modes += [(label, mode) for label in ("baseline", "candidate") for mode in ("parse", "projection", "loopback", "close")]
        if external_endpoint:
            # Explicit opt-in only. These observations have uncontrolled server
            # load/network conditions and never support a core speedup claim.
            modes += [(label, "external") for label in ("rust", "baseline", "candidate")]
        for trial in range(TRIALS):
            # Alternate paired order to reduce systematic warm-cache/thermal
            # bias; preserve actual execution order and each raw trial.
            ordered = modes if trial % 2 == 0 else list(reversed(modes))
            for label, mode in ordered:
                name = "%02d-%s-%s" % (trial, label, mode)

                def measure(source):
                    if label == "rust":
                        arguments = [str(binary), mode, source, str(SAMPLES), str(WARMUP)]
                    else:
                        arguments = [interpreters[label], "-I", str(worker), mode, source, str(SAMPLES), str(WARMUP)]
                    return observe(arguments, output, output, name, env)

                wire = []
                if mode in ("loopback", "close"):
                    peer = Peer()
                    record = None
                    try:
                        with peer:
                            record = measure(peer.endpoint)
                    except BaseException as error:
                        if record is None:
                            raise
                        record["error"] = record["error"] or repr(error)
                    finally:
                        wire = peer.observed
                        wire_path = output / (name + "-wire.json")
                        save(wire_path, wire)
                        save(output / (name + "-peer-errors.json"), peer.errors)
                    record.update(wire=wire_path.name, wire_sha256=sha(wire_path),
                                  peer_errors=name + "-peer-errors.json",
                                  peer_errors_sha256=sha(output / (name + "-peer-errors.json")))
                elif mode == "external":
                    record = measure(external_endpoint)
                else:
                    record = measure(str(fixture))
                record.update(trial=trial, label=label, mode=mode)
                # Persist a failed record before any summary validation raises.
                report["runs"].append(record)
                save(output / "performance-run.json", report)
                inspect_wire(output, record)
                record["summary"] = summarize(output, record, None if label == "rust" else report["wheel_records"][label]["native_sha256"])
        report["comparison"] = comparisons(report)
        if git(ROOT, "rev-parse", "HEAD") != candidate["source_sha"] or git(ROOT, "status", "--porcelain", "--untracked-files=normal"):
            raise ValueError("Candidate changed during measurements")
        if external_endpoint:
            report["external_status"] = "observed-not-controlled"
        report["status"] = "measured"
    except BaseException as error:
        report.update(status="failed", error=repr(error))
        raise
    finally:
        report["ended_utc"] = utc()
        save(output / "performance-run.json", report)


def comparisons(report):
    grouped = {}
    for record in report["runs"]:
        grouped.setdefault(record["label"] + ":" + record["mode"], []).append(record["summary"])
    result = {}
    for name, summaries in grouped.items():
        if len(summaries) != TRIALS:
            raise ValueError("Missing paired measurement trials")
        result[name] = {field: dict(median=statistics.median(item[field] for item in summaries),
                                   minimum=min(item[field] for item in summaries), maximum=max(item[field] for item in summaries))
                        for field in summaries[0] if field not in ("samples", "concurrency")}
    # Deliberately no (Python time - Rust time) "conversion" inference: strict
    # Rust policy and legacy Python result/projection profiles differ.
    for mode in ("parse", "projection", "loopback", "close"):
        old = result["baseline:" + mode]["p50_ns"]["median"]
        new = result["candidate:" + mode]["p50_ns"]["median"]
        result["candidate_over_baseline:" + mode] = dict(p50_latency_ratio=new / old,
             meaning="measured ratio only; >1 is slower; preserve all trials and profiles")
    return result


def verify(output):
    report = json.loads((output / "performance-run.json").read_text(encoding="utf-8"))
    if report.get("kind") != "rust-python-paired-performance" or report.get("status") != "measured" or report.get("profile") != PROFILE:
        raise ValueError("Incomplete measurement report")
    if (report["samples"], report["warmup"], report["trials"], report["concurrency"]) != (SAMPLES, WARMUP, TRIALS, 1):
        raise ValueError("Changed workload")
    if sha(output / "fixture.dat") != report["fixture_sha256"] or (output / "fixture.dat").read_bytes() != dat():
        raise ValueError("Different fixture")
    if sha(output / report["rust_binary"]) != report["rust_binary_sha256"] or sha(output / "Cargo.lock") != report["lock_sha256"]:
        raise ValueError("Different measured binary or lockfile")
    if sha(output / "measure_candidate.rs") != report["rust_worker_source_sha256"]:
        raise ValueError("Different Rust worker source snapshot")
    source_name = "crates/eltdx/examples/measure_candidate.rs"
    if (report["rust_worker_source_blob"] != git(ROOT, "rev-parse", report["candidate_sha"] + ":" + source_name)
            or sha(output / "measure_candidate.rs") != source_sha256(report["candidate_sha"], source_name)):
        raise ValueError("Rust worker source does not match the actual candidate blob")
    for name, expected_hash in report["scripts"].items():
        source_name = "scripts/verification/" + name
        if (sha(output / "scripts" / name) != expected_hash
                or expected_hash != source_sha256(report["candidate_sha"], source_name)
                or report["script_blobs"][name] != git(ROOT, "rev-parse", report["candidate_sha"] + ":" + source_name)):
            raise ValueError("Different measurement script/candidate provenance")
    if sha(output / "worker.py") != report["scripts"]["performance_python_worker.py"]:
        raise ValueError("Executed Python worker differs from its candidate source snapshot")
    for label in ("baseline", "candidate"):
        record_path = output / "provenance" / label / "wheel-build.json"
        if sha(record_path) != report[label + "_build_sha256"]:
            raise ValueError("Changed original wheel build record")
        reloaded = wheel_record(record_path)
        for field in ("source_sha", "wheel_sha256", "native_sha256", "profile", "toolchain", "rustc", "cargo", "python"):
            if reloaded[field] != report["wheel_records"][label][field]:
                raise ValueError("Recomputed build provenance differs")
    for command in report["commands"]:
        if command["exit_code"] != 0 or sha(output / command["log"]) != command["log_sha256"]:
            raise ValueError("Failed or altered preparation command")
    expected = {(trial, label, mode) for trial in range(TRIALS)
                for label, modes in (("rust", ("core", "cancel", "loopback", "close")),
                                     ("baseline", ("parse", "projection", "loopback", "close")),
                                     ("candidate", ("parse", "projection", "loopback", "close"))) for mode in modes}
    if report["external_endpoint"]:
        expected |= {(trial, label, "external") for trial in range(TRIALS) for label in ("rust", "baseline", "candidate")}
    actual = [(item["trial"], item["label"], item["mode"]) for item in report["runs"]]
    if len(actual) != len(expected) or set(actual) != expected:
        raise ValueError("Missing, duplicate or foreign performance run")
    for record in report["runs"]:
        native = None if record["label"] == "rust" else report["wheel_records"][record["label"]]["native_sha256"]
        if summarize(output, record, native, report["measurement_root"]) != record["summary"]:
            raise ValueError("Performance summary differs from actual samples")
        inspect_wire(output, record)
    if comparisons(report) != report["comparison"]:
        raise ValueError("Comparison differs from raw trials")
    save(output / "performance-recomputed.json", report["comparison"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("build-wheel", "run", "verify"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--source-root", type=Path, default=ROOT)
    parser.add_argument("--python", default=sys.executable)
    parser.add_argument("--toolchain", default="1.89.0")
    parser.add_argument("--baseline-build", type=Path)
    parser.add_argument("--candidate-build", type=Path)
    parser.add_argument("--external-endpoint")
    parser.add_argument("--implementation-complete", action="store_true")
    args = parser.parse_args()
    if not args.implementation_complete:
        raise RuntimeError("Complete ALL T01-T30 implementation sources first")
    if args.mode == "build-wheel":
        build_wheel(args.source_root.resolve(), args.output.resolve(), args.python, args.toolchain)
    elif args.mode == "run":
        if not args.baseline_build or not args.candidate_build:
            parser.error("run requires --baseline-build and --candidate-build")
        run(args.output.resolve(), args.baseline_build.resolve(), args.candidate_build.resolve(), args.toolchain, args.external_endpoint)
    else:
        verify(args.output.resolve())


if __name__ == "__main__":
    main()
