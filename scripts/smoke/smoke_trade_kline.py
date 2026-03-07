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
    parser = argparse.ArgumentParser(description="Aggregate Tongdaxin trade detail into 241 one-minute bars.")
    parser.add_argument("--host", default=None, help="Single host override, example: 124.71.187.122:7709")
    parser.add_argument("--code", default="sz000001", help="Symbol code, example: sz000001")
    parser.add_argument("--date", default=None, help="History trading date, example: 20260305 or 2026-03-05")
    parser.add_argument("--timeout", type=float, default=8.0, help="Socket timeout in seconds")
    args = parser.parse_args()

    with TdxClient(host=args.host, timeout=args.timeout) as client:
        response = client.get_history_trade_minute_kline(args.code, args.date) if args.date else client.get_trade_minute_kline(args.code)

    print(f"count={response.count}")
    print("time   open    high    low   close  volume    amount  orders")
    for item in response.items:
        if item.volume <= 0:
            continue
        orders = "-" if item.order_count is None else str(item.order_count)
        print(
            f"{item.time.strftime('%H:%M')}  {item.open_price:>6.3f}  {item.high_price:>6.3f}  "
            f"{item.low_price:>6.3f}  {item.close_price:>6.3f}  {item.volume:>6d}  {item.amount:>8.3f}  {orders:>6}"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())