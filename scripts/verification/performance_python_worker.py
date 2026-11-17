"""Isolated installed-wheel measurement worker; standard library + public API.

The projection case times FinancialRecord construction from already decoded
rows. It does not claim to isolate all PyO3 boxing or subtract core timings.
"""
from __future__ import annotations
import argparse
import hashlib
import importlib.metadata
import json
import os
from pathlib import Path
import sys
import time


def main() -> None:
    if os.environ.get("ELTDX_IMPLEMENTATION_COMPLETE") != "yes":
        raise RuntimeError("Complete implementation before measurement")
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=("parse", "projection", "loopback", "close", "external"))
    parser.add_argument("source")
    parser.add_argument("samples", type=int)
    parser.add_argument("warmup", type=int)
    args = parser.parse_args()
    if not 1 <= args.samples <= 100_000 or not 1 <= args.warmup <= 100_000:
        raise ValueError("Finite samples and warmup required")
    import eltdx
    from eltdx.offline.history_financial import parse_financial_dat
    from eltdx.offline.models import FinancialRecord
    from eltdx import TdxClient
    from eltdx.transport import SocketTransport
    native = Path(__import__("eltdx._native", fromlist=["__file__"]).__file__).resolve()
    print(json.dumps(dict(kind="environment", python=sys.version, version=importlib.metadata.version("eltdx"),
                          import_path=eltdx.__file__, native_sha256=hashlib.sha256(native.read_bytes()).hexdigest())))

    def make():
        return TdxClient(transport=SocketTransport([args.source], timeout=5, heartbeat_interval=None), probe_hosts=False)

    def query(client, index):
        result = client.codes.list("sh", start=index, limit=1)
        if args.mode == "external":
            if not result:
                raise ValueError("Empty external observation cannot count as a successful query")
        elif len(result) != 1 or result[0].code != "%06d" % (600_000 + index):
            raise ValueError("Loopback business identity mismatch")
        return result

    data = Path(args.source).read_bytes() if args.mode in ("parse", "projection") else None
    decoded = parse_financial_dat(data) if args.mode == "projection" else None
    shared = make() if args.mode in ("loopback", "external") else None
    print(json.dumps(dict(kind="start", mode=args.mode, samples=args.samples, warmup=args.warmup, concurrency=1)), flush=True)
    try:
        for iteration in range(args.warmup + args.samples):
            index = iteration % 1000
            prepared = make() if args.mode == "close" else None
            try:
                if prepared is not None:
                    query(prepared, index)
                started = time.perf_counter_ns()
                if args.mode == "parse":
                    result = parse_financial_dat(data)
                elif args.mode == "projection":
                    result = [FinancialRecord(code=code, market_id=market, report_date=date, fields=tuple(fields))
                              for code, market, date, fields in decoded]
                elif args.mode == "close":
                    prepared.close()
                    result = None
                else:
                    result = query(shared, index)
                elapsed = time.perf_counter_ns() - started
                if args.mode in ("parse", "projection") and (len(result) != 256 or any(len(row[3] if args.mode == "parse" else row.fields) != 64 for row in result)):
                    raise ValueError("Fixed fixture dimensions changed")
                if iteration >= args.warmup:
                    print(json.dumps(dict(kind="sample", id=iteration - args.warmup, latency_ns=elapsed)))
                result = None
            finally:
                if prepared is not None:
                    prepared.close()
    finally:
        if shared is not None:
            shared.close()
    print(json.dumps(dict(kind="finish", completed=args.samples)), flush=True)


if __name__ == "__main__":
    main()
