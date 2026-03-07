from __future__ import annotations

import argparse
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "src"
if str(SRC) not in sys.path:
    sys.path.insert(0, str(SRC))

from eltdx import TdxClient


def main() -> int:
    parser = argparse.ArgumentParser(description="Fetch Tongdaxin adjustment factors and adjusted kline data.")
    parser.add_argument("--host", default=None, help="Single host override, example: 124.71.187.122:7709")
    parser.add_argument("--code", default="sz000001", help="Symbol code, example: sz000001")
    parser.add_argument("--period", default="day", help="Kline period, example: day week month 1m 5m")
    parser.add_argument("--adjust", default="qfq", help="Adjustment mode: qfq or hfq")
    parser.add_argument("--count", type=int, default=10, help="Kline page size")
    parser.add_argument("--factors-only", action="store_true", help="Only print factor data")
    parser.add_argument("--timeout", type=float, default=8.0, help="Socket timeout in seconds")
    args = parser.parse_args()

    with TdxClient(host=args.host, timeout=args.timeout) as client:
        factors = client.get_factors(args.code)
        if args.factors_only:
            print(f"count={factors.count}")
            print("date         last_close  pre_last    qfq_factor    hfq_factor")
            for item in factors.items[:20]:
                last_close = "-" if item.last_close_price is None else f"{item.last_close_price:>10.3f}"
                pre_last = "-" if item.pre_last_close_price is None else f"{item.pre_last_close_price:>9.3f}"
                print(f"{item.time.strftime('%Y-%m-%d')}  {last_close}  {pre_last}  {item.qfq_factor:>11.6f}  {item.hfq_factor:>11.6f}")
            return 0

        response = client.get_adjusted_kline(args.period, args.code, adjust=args.adjust, count=args.count)

    print(f"count={response.count}")
    print("time                 open    high     low   close  last_close")
    for item in response.items[:20]:
        last_close = "-" if item.last_close_price is None else f"{item.last_close_price:>10.3f}"
        print(
            f"{item.time.strftime('%Y-%m-%d %H:%M')}  "
            f"{item.open_price:>6.3f}  {item.high_price:>6.3f}  {item.low_price:>6.3f}  {item.close_price:>6.3f}  {last_close}"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
