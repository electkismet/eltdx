from __future__ import annotations

import argparse
import sys
from pathlib import Path

ROOT = next(parent for parent in Path(__file__).resolve().parents if (parent / "pyproject.toml").exists())
SRC = ROOT / "src"
if str(SRC) not in sys.path:
    sys.path.insert(0, str(SRC))

from eltdx import TdxClient


def main() -> int:
    parser = argparse.ArgumentParser(description="Fetch Tongdaxin minute data.")
    parser.add_argument("--host", default=None, help="Single host override, example: 124.71.187.122:7709")
    parser.add_argument("--code", default="sz000001", help="Symbol code, example: sz000001")
    parser.add_argument("--date", default=None, help="Trading date, example: 20260306 or 2026-03-06")
    parser.add_argument("--timeout", type=float, default=8.0, help="Socket timeout in seconds")
    args = parser.parse_args()

    with TdxClient(host=args.host, timeout=args.timeout) as client:
        if args.date:
            response = client.get_history_minute(args.code, args.date, include_raw=True)
        else:
            response = client.get_minute(args.code, include_raw=True)

    print(f"count={response.count} trading_date={response.trading_date.isoformat()}")
    print("raw_payload_hex:")
    print(response.raw_payload_hex)
    print("time   price    volume")
    for item in response.items[:20]:
        print(f"{item.time.strftime('%H:%M')}  {item.price:>7.3f}  {item.volume:>7d}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())