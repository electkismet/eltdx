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
    parser = argparse.ArgumentParser(description="Fetch Tongdaxin security count and code list.")
    parser.add_argument("--host", default=None, help="Single host override, example: 124.71.187.122:7709")
    parser.add_argument("--exchange", default="sz", help="Market code: sz, sh, bj")
    parser.add_argument("--start", type=int, default=0, help="Start offset")
    parser.add_argument("--limit", type=int, default=10, help="How many codes to print")
    parser.add_argument("--timeout", type=float, default=8.0, help="Socket timeout in seconds")
    args = parser.parse_args()

    with TdxClient(host=args.host, timeout=args.timeout) as client:
        count = client.get_count(args.exchange)
        items = client.get_codes(args.exchange, start=args.start, limit=args.limit)

    print(f"exchange={args.exchange} count={count}")
    print("code     name      last_price  decimal  multiple")
    for item in items:
        print(f"{item.code:<6}  {item.name:<8}  {item.last_price:>10.3f}  {item.decimal:>7d}  {item.multiple:>8d}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())