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
    parser = argparse.ArgumentParser(description="Fetch Tongdaxin kline data.")
    parser.add_argument("--host", default=None, help="Single host override, example: 124.71.187.122:7709")
    parser.add_argument("--code", default="sz000001", help="Symbol code, example: sz000001 or sh000001")
    parser.add_argument("--period", default="day", help="Kline period, example: 1m 5m 15m 30m 60m day week month quarter year")
    parser.add_argument("--kind", choices=["stock", "index"], default="stock", help="Decode mode: stock for stocks/ETF, index for indices")
    parser.add_argument("--start", type=int, default=0, help="Server offset, 0 means the latest page")
    parser.add_argument("--count", type=int, default=20, help="Page size for single-page requests")
    parser.add_argument("--all", action="store_true", help="Fetch the full available history via internal paging")
    parser.add_argument("--timeout", type=float, default=8.0, help="Socket timeout in seconds")
    args = parser.parse_args()

    with TdxClient(host=args.host, timeout=args.timeout) as client:
        response = (
            client.get_kline_all(args.period, args.code, kind=args.kind)
            if args.all
            else client.get_kline(args.period, args.code, start=args.start, count=args.count, kind=args.kind, include_raw=True)
        )

    print(f"count={response.count}")
    if response.raw_payload_hex:
        print("raw_payload_hex:")
        print(response.raw_payload_hex)
    print("time                 open    high     low   close  last_close   volume        amount  up  down")
    for item in response.items[:20]:
        last_close = "-" if item.last_close_price is None else f"{item.last_close_price:>10.3f}"
        up_count = "-" if item.up_count is None else str(item.up_count)
        down_count = "-" if item.down_count is None else str(item.down_count)
        print(
            f"{item.time.strftime('%Y-%m-%d %H:%M')}  "
            f"{item.open_price:>6.3f}  {item.high_price:>6.3f}  {item.low_price:>6.3f}  {item.close_price:>6.3f}  "
            f"{last_close}  {item.volume:>7d}  {item.amount:>12.3f}  {up_count:>3}  {down_count:>4}"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
