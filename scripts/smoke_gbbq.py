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
    parser = argparse.ArgumentParser(description="Fetch Tongdaxin gbbq/xdxr data.")
    parser.add_argument("--host", default=None, help="Single host override, example: 124.71.187.122:7709")
    parser.add_argument("--code", default="sz000001", help="Symbol code, example: sz000001")
    parser.add_argument("--xdxr", action="store_true", help="Only print category-1 xdxr items")
    parser.add_argument("--timeout", type=float, default=8.0, help="Socket timeout in seconds")
    args = parser.parse_args()

    with TdxClient(host=args.host, timeout=args.timeout) as client:
        if args.xdxr:
            items = client.get_xdxr(args.code)
            print(f"count={len(items)}")
            print("date         fenhong  peigujia  songzhuangu  peigu")
            for item in items[:20]:
                print(
                    f"{item.time.strftime('%Y-%m-%d')}  {item.fenhong:>7.2f}  {item.peigujia:>9.2f}  "
                    f"{item.songzhuangu:>11.2f}  {item.peigu:>5.2f}"
                )
            return 0

        response = client.get_gbbq(args.code, include_raw=True)

    print(f"count={response.count}")
    if response.raw_payload_hex:
        print("raw_payload_hex:")
        print(response.raw_payload_hex)
    print("date         category  name        c1             c2             c3             c4")
    for item in response.items[:20]:
        name = "-" if item.category_name is None else item.category_name
        print(
            f"{item.time.strftime('%Y-%m-%d')}  {item.category:>8d}  {name:<8}  "
            f"{item.c1:>13.4f}  {item.c2:>13.4f}  {item.c3:>13.4f}  {item.c4:>13.4f}"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
