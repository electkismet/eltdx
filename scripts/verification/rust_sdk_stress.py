"""Collect and re-read native Rust stress evidence after source completion.

Python observes OS resources; ignored Rust tests perform the workload.
Build finishes before sampling. Requires the verifier's psutil package.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
CASES = {
    "unique": ("stress", "stress_unique_terminals_and_bounded_concurrency", 125_000),
    "inflight": ("stress", "stress_inflight_cancel_then_recovery_never_crosses_identity", 2_000),
    "close": ("stress", "stress_connected_owner_cycles_retire_all_sockets_and_service_handles", None),
    "subscription": ("stress", "stress_subscription_overflow_and_release", 1_000),
    "limits": ("stress", "stress_protocol_errors_and_aggregate_limits", 1_000),
    "retry": ("stress", "stress_disconnect_retries_preserve_operation_identity", 1_000),
    "financial": ("stress", "stress_financial_zip_limits_and_cancel_release", 1_000),
    "f10": ("stress", "stress_f10_connected_owner_worker_lifecycle", 1_000),
    "mac_cache": ("stress_mac", "stress_mac_ex_catalog_macro_cache_lifecycle", 500),
    "budgets": ("eltdx", "client::stress_internal::stress_actual_sdk_budgets_with_slow_server_and_bounded_queue", 32_000),
}
# Fixed before execution. Changes require rationale and retained failed evidence.
POLICY = {
    "sample_interval_seconds": 0.02,
    "timeout_seconds": 1800,
    "peak_rss_bytes": 1024 * 1024 * 1024,
    "retained_rss_bytes": 128 * 1024 * 1024,
    "late_rss_growth_bytes": 32 * 1024 * 1024,
    "late_resource_growth": 32,
    "retained_threads": 4,
    "retained_resources": 32,
    "retained_established_sockets": 0,
    "peak_threads": 128,
    "peak_resources": 4096,
}
OVERRIDES = ("REQUESTS", "WORKERS", "CANCEL_CYCLES", "CLOSE_CYCLES", "SUBSCRIPTIONS",
             "LIMIT_CYCLES", "RETRY_CYCLES", "FINANCIAL_CYCLES", "F10_CYCLES", "MAC_CYCLES")


def digest(path: Path) -> str:
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(block)
    return result.hexdigest()


def now() -> str:
    return datetime.now(timezone.utc).isoformat()


def save(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, ensure_ascii=True, indent=2) + "\n", encoding="utf-8")


def rows(path: Path):
    with path.open(encoding="utf-8") as source:
        for line in source:
            if not line.endswith("\n"):
                raise ValueError("Truncated raw event: " + str(path))
            yield json.loads(line)


def integer(value, minimum: int = 0) -> int:
    if type(value) is not int or value < minimum:
        raise ValueError("Expected nonnegative integer")
    return value


def percentile(values: list[int], ratio: float) -> int:
    values = sorted(values)
    return values[min(len(values) - 1, int((len(values) - 1) * ratio))]


def command(arguments: list[str], output: Path, name: str, environment: dict) -> str:
    with (output / name).open("wb") as log:
        result = subprocess.run(arguments, cwd=ROOT, env=environment, stdout=log,
                                stderr=subprocess.STDOUT, check=False, timeout=1800)
    if result.returncode:
        raise RuntimeError("Command failed; retained " + str(output / name))
    return (output / name).read_text(encoding="utf-8", errors="replace")


def sample(process, psutil) -> dict:
    # A disappearing descendant is a normal race. AccessDenied is an evidence
    # failure: never substitute zero for unobservable live-process resources.
    members = [process, *process.children(recursive=True)]
    totals = dict(rss=0, threads=0, resources=0, established=0, cpu_seconds=0.0)
    pids = []
    tcp_pairs = []
    for member in members:
        try:
            with member.oneshot():
                totals["rss"] += member.memory_info().rss
                totals["threads"] += member.num_threads()
                totals["resources"] += member.num_handles() if os.name == "nt" else member.num_fds()
                cpu = member.cpu_times()
                totals["cpu_seconds"] += cpu.user + cpu.system
            for connection in member.net_connections(kind="tcp"):
                totals["established"] += connection.status == psutil.CONN_ESTABLISHED
                if connection.laddr and connection.raddr:
                    tcp_pairs.append(sorted([list(connection.laddr), list(connection.raddr)]))
            pids.append(member.pid)
        except psutil.NoSuchProcess:
            if member.pid == process.pid:
                raise
    return dict(kind="resource", unix_ns=time.time_ns(), pids=pids, tcp_pairs=tcp_pairs, **totals)


def pair_key(pair) -> tuple:
    return tuple(sorted(tuple(address) for address in pair))


def observe(binary: Path, case: str, test: str, output: Path, environment: dict) -> dict:
    import psutil
    arguments = [str(binary), test, "--exact", "--ignored", "--test-threads=1", "--nocapture"]
    start = time.monotonic()
    record = dict(case=case, command=arguments, started_utc=now(), exit_code=None,
                  timed_out=False, sampler_error=None, binary_sha256=digest(binary))
    with (output / (case + ".log")).open("wb") as stdout, (output / (case + "-resources.ndjson")).open("x", encoding="utf-8") as resources:
        job = subprocess.Popen(arguments, cwd=output, env=environment, stdout=stdout, stderr=subprocess.STDOUT)
        process = psutil.Process(job.pid)
        observed_pairs = set()
        socket_sample_at = 0.0
        try:
            while job.poll() is None:
                if time.monotonic() - start > POLICY["timeout_seconds"]:
                    record["timed_out"] = True
                    raise TimeoutError(case)
                try:
                    reading = sample(process, psutil)
                    observed_pairs.update(pair_key(pair) for pair in reading["tcp_pairs"])
                    reading["time_wait_for_observed_pairs"] = None
                    if time.monotonic() >= socket_sample_at:
                        reading["time_wait_for_observed_pairs"] = sum(
                            bool(connection.status == psutil.CONN_TIME_WAIT and connection.laddr and connection.raddr
                                 and pair_key([connection.laddr, connection.raddr]) in observed_pairs)
                            for connection in psutil.net_connections(kind="tcp"))
                        socket_sample_at = time.monotonic() + .5
                    resources.write(json.dumps(reading) + "\n")
                    resources.flush()
                except psutil.NoSuchProcess:
                    if job.poll() is None:
                        raise
                    break
                time.sleep(POLICY["sample_interval_seconds"])
        except BaseException as error:
            record["sampler_error"] = repr(error)
            try:
                for child in process.children(recursive=True):
                    try:
                        child.kill()
                    except psutil.NoSuchProcess:
                        pass
            except psutil.NoSuchProcess:
                pass
            if job.poll() is None:
                job.kill()
        finally:
            record["exit_code"] = job.wait(timeout=30)
            record["ended_utc"] = now()
            record["elapsed_seconds"] = time.monotonic() - start
    return record


def inspect_case(directory: Path, name: str, record: dict, system: str) -> dict:
    if record["exit_code"] != 0 or record["timed_out"] or record["sampler_error"]:
        raise ValueError("Failed native case: " + name)
    events = list(rows(directory / "events" / (name + ".ndjson")))
    if not events or events[0]["kind"] != "start" or events[-1]["kind"] != "finish":
        raise ValueError("Missing start/finish: " + name)
    if any(event["case"] != name for event in events):
        raise ValueError("Mixed case event log")
    if any(integer(event["elapsed_ns"]) > integer(next_event["elapsed_ns"])
           for event, next_event in zip(events, events[1:])):
        raise ValueError("Non-monotonic raw event order")
    expected = CASES[name][2]
    if name == "close":
        expected = 2_000 if system == "Windows" else 10_000
    if events[0]["expected"] != expected or events[-1]["completed"] != expected:
        raise ValueError("Default workload count changed: " + name)
    terminals = [event for event in events if event["kind"] == "terminal"]
    identities = [integer(event["id"]) for event in terminals]
    if len(terminals) != expected or set(identities) != set(range(expected)):
        raise ValueError("Missing, repeated or foreign terminal identity: " + name)
    for event in terminals:
        outcome = "cancelled" if ((name == "unique" and event["id"] % 5 == 0)
                                  or (name == "inflight" and event["id"] % 2 == 0)) else "completed"
        if event["outcome"] != outcome:
            raise ValueError("Unexpected terminal outcome")
        integer(event["latency_ns"])
    wires = [event for event in events if event["kind"] == "wire"]
    wire_ids = [(integer(event["connection"], 1), integer(event["id"])) for event in wires]
    if len(set(wire_ids)) != len(wire_ids):
        raise ValueError("Reused wire identity within a physical connection")
    associated = [event for event in wires if "operation" in event]
    by_operation = {}
    for event in associated:
        operation = integer(event["operation"])
        by_operation.setdefault(operation, []).append(event)
    if name in ("unique", "inflight", "close", "budgets", "retry"):
        wanted = {i for i in range(expected) if name != "unique" or i % 5 != 0}
        if set(by_operation) != wanted:
            raise ValueError("Wire/terminal operation correspondence failed")
        attempts = 2 if name == "retry" else 1
        if any(len(items) != attempts for items in by_operation.values()):
            raise ValueError("Unexpected actual wire-attempt count")
        if name == "retry" and any(items[0]["connection"] == items[1]["connection"] for items in by_operation.values()):
            raise ValueError("Forced disconnect did not change physical source")
    if name == "unique" and len(associated) < 100_000:
        raise ValueError("Fewer than 100000 actual wire requests")
    budgets = [event for event in events if event["kind"] == "budget"]
    if name == "budgets":
        if len(budgets) < 10:
            raise ValueError("Missing actual SDK budget samples")
        for event in budgets:
            for field in ("raw", "decoded"):
                maximum = integer(event[field + "_max"], 1)
                if integer(event[field]) > maximum or integer(event[field + "_peak"]) > maximum:
                    raise ValueError("Internal budget exceeded")
            if integer(event["queue"]) > integer(event["queue_max"], 1):
                raise ValueError("Internal queue exceeded")
        if any(budgets[-1][field] != 0 for field in ("raw", "decoded")):
            raise ValueError("Internal budget not released after close")
        if any(max(event[field] for event in budgets) == 0 for field in ("raw_peak", "decoded_peak", "queue", "live_actors")):
            raise ValueError("Budget/queue workload did not exercise its claimed path")
    phases = [event for event in events if event["kind"] == "phase"]
    baseline = next(event["unix_ns"] for event in phases if event["phase"] == "baseline")
    measurement = next(event["unix_ns"] for event in phases if event["phase"] == "measurement")
    cooldown = next(event["unix_ns"] for event in phases if event["phase"] == "cooldown")
    # A warmup's final measurement marker supersedes the start marker.
    measurement = max(event["unix_ns"] for event in phases if event["phase"] == "measurement")
    samples = list(rows(directory / (name + "-resources.ndjson")))
    before = [row for row in samples if baseline <= row["unix_ns"] < measurement]
    during = [row for row in samples if measurement <= row["unix_ns"] < cooldown]
    after = [row for row in samples if cooldown <= row["unix_ns"] <= events[-1]["unix_ns"]]
    if min(len(before), len(during), len(after)) < 3:
        raise ValueError("Missing baseline/middle/cooldown resource samples")
    for row in samples:
        if not row["pids"]:
            raise ValueError("Empty live process measurement")
        for field in ("rss", "threads", "resources", "established"):
            integer(row[field])
    peaks = {field: max(row[field] for row in samples) for field in ("rss", "threads", "resources", "established", "cpu_seconds")}
    time_wait = [row["time_wait_for_observed_pairs"] for row in samples if row["time_wait_for_observed_pairs"] is not None]
    if not time_wait:
        raise ValueError("No OS TIME_WAIT observations")
    deltas = {field: statistics.median(row[field] for row in after) - statistics.median(row[field] for row in before)
              for field in ("rss", "threads", "resources", "established")}
    if peaks["rss"] > POLICY["peak_rss_bytes"] or peaks["threads"] > POLICY["peak_threads"] or peaks["resources"] > POLICY["peak_resources"]:
        raise ValueError("OS peak policy exceeded")
    for field, policy in (("rss", "retained_rss_bytes"), ("threads", "retained_threads"),
                          ("resources", "retained_resources"), ("established", "retained_established_sockets")):
        if deltas[field] > POLICY[policy]:
            raise ValueError("Cooldown retained-resource policy exceeded: " + field)
    # Compare late steady workload quartiles as well as final RSS. A peak alone
    # cannot distinguish allocator retention from continuous growth.
    quarter = max(1, len(during) // 4)
    late_growth = {field: statistics.median(row[field] for row in during[-quarter:])
                   - statistics.median(row[field] for row in during[-2 * quarter:-quarter])
                   for field in ("rss", "resources")}
    if late_growth["rss"] > POLICY["late_rss_growth_bytes"] or late_growth["resources"] > POLICY["late_resource_growth"]:
        raise ValueError("Late steady-workload resource growth exceeded")
    latencies = [event["latency_ns"] for event in terminals]
    duration = (cooldown - measurement) / 1e9
    return dict(terminals=len(terminals), actual_wire_requests=len(wires), associated_wire_requests=len(associated),
                time_wait_for_observed_pairs_peak=max(time_wait),
                concurrency=events[0]["concurrency"], peaks=peaks, cooldown_delta=deltas, late_growth=late_growth,
                throughput_per_second=len(terminals) / duration,
                latency_ns={label: percentile(latencies, fraction) for label, fraction in (("p50", .50), ("p95", .95), ("p99", .99))},
                budget_peaks={field: max((event[field] for event in budgets), default=None)
                              for field in ("raw_peak", "decoded_peak", "queue", "live_actors", "configured_workers")})


def git(*arguments: str) -> str:
    return subprocess.check_output(["git", *arguments], cwd=ROOT, text=True, encoding="utf-8").strip()


def file_inventory(directory: Path) -> dict:
    return {str(path.relative_to(directory)).replace("\\", "/"): digest(path)
            for path in sorted(directory.rglob("*")) if path.is_file()
            and path.name not in ("stress-run.json", "stress-recomputed.json")
            and "target" not in path.relative_to(directory).parts}


def run(output: Path, toolchain: str) -> None:
    import psutil
    if platform.system() not in ("Linux", "Windows"):
        raise RuntimeError("Release stress evidence requires Linux and Windows")
    if git("status", "--porcelain", "--untracked-files=normal"):
        raise RuntimeError("Commit the complete candidate first; dirty stress evidence is rejected")
    for suffix in OVERRIDES:
        if "ELTDX_RUST_STRESS_" + suffix in os.environ:
            raise RuntimeError("Release evidence uses default counts; remove workload overrides")
    output.mkdir(parents=True, exist_ok=False)
    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = str(output / "target")
    environment["ELTDX_IMPLEMENTATION_COMPLETE"] = "yes"
    environment["ELTDX_RUST_STRESS_EVENTS"] = str(output / "events")
    report = dict(schema=1, kind="native-rust-sdk-stress", status="running", candidate_sha=git("rev-parse", "HEAD"),
                  source_tree=git("rev-parse", "HEAD^{tree}"), dirty=False, system=platform.system(),
                  platform=platform.platform(), cpu=platform.processor(), cpu_count=os.cpu_count(),
                  psutil=psutil.__version__, toolchain=toolchain, features=["full"], mode="release",
                  policy=POLICY, started_utc=now(), cases={}, artifacts={}, source_files={})
    try:
        tracked = git("ls-files").splitlines()
        for name in tracked:
            if name.startswith(("crates/", "scripts/verification/", "tests/fixtures/", "tests/contracts/")) or name in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml"):
                path = ROOT / name
                if path.is_file():
                    report["source_files"][name] = dict(bytes_sha256=digest(path), git_blob=git("rev-parse", "HEAD:" + name))
        save(output / "source-inputs.json", report["source_files"])
        report["rustc"] = command(["rustc", "+" + toolchain, "-Vv"], output, "rustc.log", environment)
        report["cargo"] = command(["cargo", "+" + toolchain, "-V"], output, "cargo.log", environment)
        build = ["cargo", "+" + toolchain, "test", "-p", "eltdx", "--lib", "--test", "stress", "--test", "stress_mac",
                 "--release", "--no-default-features", "--features", "full", "--locked", "--no-run", "--message-format=json"]
        report["build_command"] = build
        built = command(build, output, "build.log", environment)
        binaries = {}
        for line in built.splitlines():
            try:
                message = json.loads(line)
            except json.JSONDecodeError:
                continue
            if message.get("reason") == "compiler-artifact" and message.get("profile", {}).get("test") and message.get("executable"):
                binaries[message["target"]["name"]] = Path(message["executable"])
        for name, (target, test, _) in CASES.items():
            if target not in binaries:
                raise RuntimeError("Missing compiled stress target: " + target)
            record = observe(binaries[target], name, test, output, environment)
            report["cases"][name] = record
            save(output / "stress-run.json", report)
        if git("rev-parse", "HEAD") != report["candidate_sha"] or git("status", "--porcelain", "--untracked-files=normal"):
            raise RuntimeError("Candidate changed during the run")
        for name, item in report["source_files"].items():
            if digest(ROOT / name) != item["bytes_sha256"]:
                raise RuntimeError("Source input changed during the run: " + name)
        report["recomputed"] = {name: inspect_case(output, name, record, report["system"])
                                for name, record in report["cases"].items()}
        report["status"] = "passed"
    except BaseException as error:
        report["status"] = "failed"
        report["error"] = repr(error)
        raise
    finally:
        report["ended_utc"] = now()
        report["artifacts"] = file_inventory(output)
        save(output / "stress-run.json", report)


def verify(directory: Path, candidate: str) -> dict:
    report = json.loads((directory / "stress-run.json").read_text(encoding="utf-8"))
    if len(candidate) != 40 or any(char not in "0123456789abcdef" for char in candidate):
        raise ValueError("Require the full candidate commit SHA")
    if report.get("schema") != 1 or report.get("kind") != "native-rust-sdk-stress" or report.get("candidate_sha") != candidate:
        raise ValueError("Candidate or evidence kind mismatch")
    if report["dirty"] is not False or report["status"] != "passed" or report["policy"] != POLICY:
        raise ValueError("Dirty, failed or changed-policy evidence")
    if report["mode"] != "release" or report["features"] != ["full"] or report["toolchain"] != "1.89.0":
        raise ValueError("Wrong release measurement configuration")
    if set(report["cases"]) != set(CASES):
        raise ValueError("Missing native stress cases")
    if report["source_tree"] != git("rev-parse", candidate + "^{tree}"):
        raise ValueError("Claimed source tree is not the candidate commit")
    wanted_sources = {name for name in git("ls-tree", "-r", "--name-only", candidate).splitlines()
                      if name.startswith(("crates/", "scripts/verification/", "tests/fixtures/", "tests/contracts/"))
                      or name in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml")}
    if set(report["source_files"]) != wanted_sources:
        raise ValueError("Incomplete candidate source inventory")
    if json.loads((directory / "source-inputs.json").read_text(encoding="utf-8")) != report["source_files"]:
        raise ValueError("Exported source inventory differs from manifest")
    for name, item in report["source_files"].items():
        if item["git_blob"] != git("rev-parse", candidate + ":" + name):
            raise ValueError("Source blob differs from the actual candidate: " + name)
    for name, record in report["cases"].items():
        if record["case"] != name or record["command"][1:] != [CASES[name][1], "--exact", "--ignored", "--test-threads=1", "--nocapture"]:
            raise ValueError("Native case command differs from required target")
    if file_inventory(directory) != report["artifacts"]:
        raise ValueError("Missing or modified raw stress artifact")
    # Treat exported summary as a claim. Re-open raw events/resources and
    # recompute every identity, count, quantile, budget and resource decision.
    recomputed = {name: inspect_case(directory, name, record, report["system"])
                  for name, record in report["cases"].items()}
    if recomputed != report["recomputed"]:
        raise ValueError("Summary does not match the raw evidence")
    save(directory / "stress-recomputed.json", recomputed)
    return report


def aggregate(inputs: Path, output: Path, candidate: str) -> None:
    manifests = sorted(inputs.rglob("stress-run.json"))
    if len(manifests) != 2:
        raise ValueError("Exactly two complete native Linux/Windows runs are required")
    reports = [verify(path.parent, candidate) for path in manifests]
    if {item["system"] for item in reports} != {"Linux", "Windows"}:
        raise ValueError("Missing Linux or Windows native stress evidence")
    if len({item["source_tree"] for item in reports}) != 1:
        raise ValueError("Different source trees")
    blob_maps = [{name: item["git_blob"] for name, item in report["source_files"].items()} for report in reports]
    if blob_maps[0] != blob_maps[1] or "Cargo.lock" not in blob_maps[0]:
        raise ValueError("Different lock/source/script/fixture inputs")
    result = dict(schema=1, kind="native-rust-sdk-stress-aggregate", status="passed", candidate_sha=candidate,
                  created_utc=now(), runs=[dict(system=report["system"], manifest_sha256=digest(path),
                       source_tree=report["source_tree"], cases=report["recomputed"])
                       for path, report in zip(manifests, reports)], policy=POLICY)
    if output.exists():
        raise FileExistsError("Preserve existing aggregate")
    output.parent.mkdir(parents=True, exist_ok=True)
    save(output, result)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("run", "verify", "aggregate"))
    parser.add_argument("--output", type=Path)
    parser.add_argument("--inputs", type=Path)
    parser.add_argument("--candidate")
    parser.add_argument("--toolchain", choices=("1.89.0",), default="1.89.0")
    parser.add_argument("--implementation-complete", action="store_true")
    args = parser.parse_args()
    if not args.implementation_complete:
        raise RuntimeError("Complete ALL T01-T30 implementation sources before execution")
    if args.mode == "run":
        if args.output is None:
            parser.error("run requires --output")
        run(args.output.resolve(), args.toolchain)
    else:
        if args.inputs is None or args.candidate is None:
            parser.error("verify/aggregate require --inputs and --candidate")
        if args.mode == "verify":
            verify(args.inputs.resolve(), args.candidate)
        else:
            if args.output is None:
                parser.error("aggregate requires --output")
            aggregate(args.inputs.resolve(), args.output.resolve(), args.candidate)


if __name__ == "__main__":
    main()
