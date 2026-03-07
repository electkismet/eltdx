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
    parser = argparse.ArgumentParser(description="Fetch Tongdaxin equity snapshot and turnover data.")
    parser.add_argument("--host", default=None, help="Single host override, example: 124.71.187.122:7709")
    parser.add_argument("--code", default="sz000001", help="Symbol code, example: sz000001")
    parser.add_argument("--on", default=None, help="Effective date, example: 2025-03-01")
    parser.add_argument("--volume", type=float, default=None, help="Volume for turnover calculation")
    parser.add_argument("--unit", default="hand", help="Volume unit: hand or share")
    parser.add_argument("--changes", action="store_true", help="Print equity change events instead of one snapshot")
    parser.add_argument("--timeout", type=float, default=8.0, help="Socket timeout in seconds")
    args = parser.parse_args()

    with TdxClient(host=args.host, timeout=args.timeout) as client:
        if args.changes:
            response = client.get_equity_changes(args.code)
            print(f"count={response.count}")
            print("date         category  name        float_shares    total_shares")
            for item in response.items[:20]:
                name = "-" if item.category_name is None else item.category_name
                print(f"{item.time.strftime('%Y-%m-%d')}  {item.category:>8d}  {name:<8}  {item.float_shares:>12d}  {item.total_shares:>12d}")
            return 0

        equity = client.get_equity(args.code, args.on)
        if equity is None:
            print("no equity snapshot found")
            return 1

        print(f"code={equity.code} date={equity.time.strftime('%Y-%m-%d')} category={equity.category} name={equity.category_name}")
        print(f"float_shares={equity.float_shares} total_shares={equity.total_shares}")
        if args.volume is not None:
            turnover = client.get_turnover(args.code, args.volume, on=args.on, unit=args.unit)
            print(f"turnover={turnover:.6f}%")
        return 0


if __name__ == "__main__":
    raise SystemExit(main())
