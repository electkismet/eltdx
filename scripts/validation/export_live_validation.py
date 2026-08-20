"""Export same-session v2.0.5 and native 3.0 real-host evidence."""

from __future__ import annotations

import argparse
import hashlib
import importlib.metadata
import json
import os
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
NAMES = (
    "heartbeat", "handshake", "security_count", "security_list", "special_limits",
    "intraday_aux", "klines", "today_intraday", "legacy_quotes", "refresh_stream",
    "category_quotes", "snapshots", "auction_series", "file_content",
    "historical_intraday", "today_ticks", "historical_ticks", "sparkline",
    "recent_intraday", "capital_changes", "finance_batch",
)
STABLE = {
    "security_count", "security_list", "klines", "file_content",
    "historical_intraday", "historical_ticks", "recent_intraday",
    "capital_changes", "finance_batch",
}


def _now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def _identity(value: Any) -> Any:
    if isinstance(value, (list, tuple)):
        return [_identity(item) for item in value]
    if isinstance(value, (bytes, str, int, float, bool)) or value is None:
        return None
    result = {}
    for name in ("full_code", "code", "market_id", "path", "trading_date"):
        item = getattr(value, name, None)
        if item is not None:
            result[name] = str(item)
    for name in ("records", "bars", "points", "ticks"):
        items = getattr(value, name, None)
        if isinstance(items, (list, tuple)):
            result[f"{name}_count"] = len(items)
    return result


def _summary(value: Any) -> dict[str, Any]:
    from eltdx.serialization import to_jsonable

    canonical = json.dumps(
        to_jsonable(value), ensure_ascii=True, sort_keys=True, separators=(",", ":"),
        default=str,
    ).encode()
    return {
        "type": f"{type(value).__module__}.{type(value).__qualname__}",
        "identity": _identity(value),
        "canonical_sha256": hashlib.sha256(canonical).hexdigest(),
        "canonical_bytes": len(canonical),
    }


def _identity_key(value: Any) -> Any:
    if isinstance(value, list):
        return [_identity_key(item) for item in value]
    if isinstance(value, dict):
        return {
            key: _identity_key(item)
            for key, item in value.items()
            if not key.endswith("_count") and key != "count"
        }
    return value


def _client() -> Any:
    from eltdx import TdxClient

    raw = os.environ.get("ELTDX_REAL_HOSTS")
    hosts = None if raw is None else [item.strip() for item in raw.split(",") if item.strip()]
    if raw is not None and not hosts:
        raise ValueError("ELTDX_REAL_HOSTS does not contain a host")
    if hosts is None:
        return TdxClient(timeout=8, pool_size=1, heartbeat_interval=None)
    return TdxClient.from_hosts(hosts, timeout=8, pool_size=1, heartbeat_interval=None)


def _child_result(role: str) -> dict[str, Any]:
    from eltdx.exceptions import ConnectionClosedError, ResponseTimeoutError

    codes = tuple(code.strip() for code in os.environ.get(
        "ELTDX_REAL_CODES", "sz000001,sh600000,bj920001"
    ).split(",") if code.strip())
    if len(codes) < 3:
        raise ValueError("ELTDX_REAL_CODES must contain three market codes")
    stock = codes[0]
    day = os.environ.get("ELTDX_REAL_HISTORY_DATE", "2026-08-14")
    path = os.environ.get("ELTDX_REAL_RESOURCE_PATH", "T0002/hq_cache.dat")
    results: dict[str, Any] = {}
    external: list[str] = []
    client_errors: list[str] = []
    started = _now()
    with _client() as client:
        calls = (
            client.session.heartbeat, client.session.handshake,
            lambda: client.codes.count("sz"), lambda: client.codes.list("bj", limit=5),
            lambda: client.limits.special(start_index=0), lambda: client.minutes.aux(stock),
            lambda: client.bars.get(stock, start=1, count=20),
            lambda: client.minutes.today(stock),
            lambda: client.quotes.legacy(codes), lambda: client.quotes.refresh(codes, cursors={}),
            lambda: client.quotes.list_by_category(6, count=5),
            lambda: client.quotes.get_snapshots(codes), lambda: client.auctions.series(stock),
            lambda: client.resources.read(path, size=64),
            lambda: client.minutes.history(stock, day),
            lambda: client.trades.today(stock, count=20),
            lambda: client.trades.history(stock, day, count=20),
            lambda: client.minutes.sparkline(stock), lambda: client.minutes.recent(stock, day),
            lambda: client.corporate.capital_changes(stock),
            lambda: client.corporate.finance_batch(codes),
        )
        if len(calls) != len(NAMES):
            raise AssertionError("live validation command names and calls differ")
        for name, call in zip(NAMES, calls):
            try:
                results[name] = _summary(call())
            except (ConnectionClosedError, ResponseTimeoutError) as error:
                external.append(f"{name}: {type(error).__name__}: {error}")
            except BaseException as error:
                client_errors.append(f"{name}: {type(error).__name__}: {error}")
    return {
        "schema": 1, "role": role, "package_version": importlib.metadata.version("eltdx"),
        "started_at_utc": started, "ended_at_utc": _now(), "commands": results,
        "external_failures": external, "client_failures": client_errors,
    }


def _venv_python(root: Path, wheel: Path) -> Path:
    subprocess.run([sys.executable, "-m", "venv", str(root)], check=True, timeout=300)
    python = root / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
    subprocess.run(
        [str(python), "-m", "pip", "install", "--no-deps", str(wheel)],
        check=True, timeout=300,
    )
    return python


def _run(python: Path, role: str, output: Path) -> dict[str, Any]:
    env = os.environ.copy()
    env.pop("PYTHONHOME", None)
    env.pop("PYTHONPATH", None)
    subprocess.run(
        [str(python), str(Path(__file__).resolve()), "--role", role,
         "--child-output", str(output)],
        cwd=ROOT, env=env, check=True, timeout=600,
    )
    return json.loads(output.read_text(encoding="utf-8"))


def _compare(baseline: dict[str, Any], current: dict[str, Any]) -> list[str]:
    errors = []
    if baseline["package_version"] != "2.0.5" or current["package_version"] != "3.0.3":
        errors.append("baseline/current package version mismatch")
    for role, result in (("baseline", baseline), ("current", current)):
        if result["external_failures"] or result["client_failures"]:
            failures = result["external_failures"] + result["client_failures"]
            errors.append(f"{role} failures: {failures!r}")
        if set(result["commands"]) != set(NAMES):
            errors.append(f"{role} command coverage differs from all 21 commands")
    for name in sorted(set(baseline["commands"]) & set(current["commands"])):
        before, after = baseline["commands"][name], current["commands"][name]
        identities_match = _identity_key(before["identity"]) == _identity_key(
            after["identity"]
        )
        if before["type"] != after["type"] or not identities_match:
            errors.append(f"{name}: response type or identity mismatch")
        if name in STABLE and before["canonical_sha256"] != after["canonical_sha256"]:
            errors.append(f"{name}: stable canonical digest mismatch")
    return errors


def _campaign(wheel: Path) -> dict[str, Any]:
    if not wheel.is_file():
        raise FileNotFoundError(f"baseline wheel does not exist: {wheel}")
    with tempfile.TemporaryDirectory(prefix="eltdx-live-") as temporary:
        root = Path(temporary)
        baseline = _run(_venv_python(root / "venv", wheel), "baseline", root / "b.json")
        current = _run(Path(sys.executable), "current", root / "c.json")
    errors = _compare(baseline, current)
    return {
        "schema": 1, "kind": "eltdx-live-validation", "created_at_utc": _now(),
        "baseline_wheel_sha256": hashlib.sha256(wheel.read_bytes()).hexdigest(),
        "baseline": baseline, "current": current, "stable_digest_commands": sorted(STABLE),
        "passed": not errors, "errors": errors,
    }


def _write(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=True, indent=2) + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--baseline-wheel", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--role", choices=("baseline", "current"))
    parser.add_argument("--child-output", type=Path)
    args = parser.parse_args()
    if args.role:
        if args.child_output is None:
            parser.error("--role requires --child-output")
        _write(args.child_output, _child_result(args.role))
        return 0
    if args.baseline_wheel is None or args.output is None:
        parser.error("campaign mode requires --baseline-wheel and --output")
    evidence = _campaign(args.baseline_wheel.resolve())
    _write(args.output.resolve(), evidence)
    return 0 if evidence["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
