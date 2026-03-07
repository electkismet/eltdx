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
    parser = argparse.ArgumentParser(description="Fetch Tongdaxin trade detail data.")
    parser.add_argument("--host", default=None, help="Single host override, example: 124.71.187.122:7709")
    parser.add_argument("--code", default="sz000001", help="Symbol code, example: sz000001")
    parser.add_argument("--date", default=None, help="History trading date, example: 20260305 or 2026-03-05")
    parser.add_argument("--count", type=int, default=20, help="Page size for single-page requests")
    parser.add_argument("--all", action="store_true", help="Fetch the full trading day via internal paging")
    parser.add_argument("--timeout", type=float, default=8.0, help="Socket timeout in seconds")
    args = parser.parse_args()

    with TdxClient(host=args.host, timeout=args.timeout) as client:
        if args.date:
            response = client.get_history_trade_day(args.code, args.date) if args.all else client.get_history_trade(args.code, args.date, count=args.count, include_raw=True)
        else:
            response = client.get_trade_all(args.code) if args.all else client.get_trade(args.code, count=args.count, include_raw=True)

    print(f"count={response.count} trading_date={response.trading_date.isoformat()}")
    if response.raw_payload_hex:
        print("raw_payload_hex:")
        print(response.raw_payload_hex)
    print("time   price    volume  orders  status  side")
    for item in response.items[:20]:
        orders = "-" if item.order_count is None else str(item.order_count)
        side = "-" if item.side is None else item.side
        print(f"{item.time.strftime('%H:%M')}  {item.price:>7.3f}  {item.volume:>6d}  {orders:>6}  {item.status:>6d}  {side}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())