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
    parser = argparse.ArgumentParser(description="Fetch Tongdaxin call auction data.")
    parser.add_argument("--host", default=None, help="Single host override, example: 124.71.187.122:7709")
    parser.add_argument("--code", default="sz000001", help="Symbol code, example: sz000001")
    parser.add_argument("--timeout", type=float, default=8.0, help="Socket timeout in seconds")
    args = parser.parse_args()

    with TdxClient(host=args.host, timeout=args.timeout) as client:
        response = client.get_call_auction(args.code, include_raw=True)

    print(f"count={response.count}")
    print("raw_frame_hex:")
    print(response.raw_frame_hex)
    print("raw_payload_hex:")
    print(response.raw_payload_hex)
    print("time      price    match  unmatched  flag")
    for item in response.items:
        print(
            f"{item.time.strftime('%H:%M:%S')}  {item.price:>7.3f}  {item.match:>5d}  {item.unmatched:>9d}  {item.flag:>4d}"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
