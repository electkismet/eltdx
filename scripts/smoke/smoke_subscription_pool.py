"""Exercise the public subscription API with 1,000 stocks on ten slots."""
from __future__ import annotations

import argparse
from collections import Counter
from dataclasses import asdict
import json
from pathlib import Path
import time

import eltdx
from eltdx import TdxClient
from eltdx.api.quotes import QuoteApi


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--duration", type=float, default=60)
    parser.add_argument("--output", type=Path, default=Path("output/subscription_pool.json"))
    args = parser.parse_args()
    refreshes = []
    pushes = Counter()
    push_frames = [0]
    last_push = {}
    started = [0.0]

    class RecordingQuoteApi(QuoteApi):
        def refresh(self, codes=None, cursors=None):
            page = super().refresh(codes, cursors)
            refreshes.append({"elapsed": round(time.monotonic()-started[0], 3),
                              "codes": list(codes), "cursors": dict(cursors or {}), "records": page.count})
            return page

        def poll_push(self, *, timeout=0.0, parse=False):
            page = super().poll_push(timeout=timeout, parse=parse)
            if page is not None:
                push_frames[0] += 1
                for record in page.records:
                    pushes[record.full_code] += 1
                    last_push[record.full_code] = time.monotonic()-started[0]
            return page

    callbacks = Counter()
    with TdxClient(timeout=10, pool_size=10) as client:
        sh, sz = client.codes.a_shares("sh"), client.codes.a_shares("sz")
        codes = [r.full_code for pair in zip(sh, sz) for r in pair][:1000]
        assert len(set(codes)) == 1000
        api = RecordingQuoteApi(client.transport, price_resolver=client._normalize_prices)
        sub = api.subscribe(codes, lambda record: callbacks.update([record.full_code]))
        started[0] = time.monotonic()
        sub.run(duration=args.duration)
        diagnostics = asdict(client.transport.diagnostics)
        # Diagnostics are a periodically refreshed runtime snapshot. Allow
        # that cache to observe released leases before asserting cleanup.
        until = time.monotonic() + 2
        while diagnostics["broker"]["active_leases"] and time.monotonic() < until:
            time.sleep(0.02)
            diagnostics = asdict(client.transport.diagnostics)
    groups = []
    for offset in range(0, 1000, 100):
        group = codes[offset:offset+100]
        requests = [r for r in refreshes if r["codes"] == group]
        groups.append({"slot": offset//100, "initial_records": requests[0]["records"],
                       "renewals": len(requests)-1, "renewal_response_records": sum(r["records"] for r in requests[1:]),
                       "push_records": sum(pushes[c] for c in group),
                       "push_coverage": sum(pushes[c] > 0 for c in group),
                       "last_push_seconds": round(max(last_push.get(c, 0) for c in group), 3),
                       "request_times": [r["elapsed"] for r in requests]})
    report = {"version": eltdx.__version__, "duration": args.duration, "groups": groups,
              "push_frames": push_frames[0], "push_records": sum(pushes.values()),
              "callback_records": sum(callbacks.values()), "push_coverage": len(pushes),
              "refreshes": refreshes, "diagnostics_after_close": diagnostics}
    report["pass"] = (all(g["initial_records"] == 100 and g["renewals"] >= 2
                          and g["last_push_seconds"] >= args.duration-10 for g in groups)
                      and diagnostics["push_dropped"] == 0
                      and diagnostics["broker"]["active_leases"] == 0
                      and set(callbacks).issubset(set(codes)))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2, default=str), encoding="utf-8")
    print(json.dumps({k:v for k,v in report.items() if k not in {"refreshes", "diagnostics_after_close"}}, indent=2), flush=True)
    return 0 if report["pass"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
