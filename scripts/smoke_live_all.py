from __future__ import annotations

import argparse
import sys
from datetime import date, datetime
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "src"
if str(SRC) not in sys.path:
    sys.path.insert(0, str(SRC))

from eltdx import TdxClient


warnings: list[str] = []


def expect(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def warn(message: str) -> None:
    warnings.append(message)
    print(f"[WARN] {message}", flush=True)


def ok(message: str) -> None:
    print(f"[OK] {message}", flush=True)


def expect_date(value: object, name: str) -> None:
    expect(isinstance(value, date), f"{name} should be date, got {type(value).__name__}")


def expect_datetime(value: object, name: str, *, allow_none: bool = False) -> None:
    if value is None and allow_none:
        return
    expect(isinstance(value, datetime), f"{name} should be datetime, got {type(value).__name__}")
    assert isinstance(value, datetime)
    expect(value.tzinfo is not None, f"{name} should be timezone-aware")


def expect_raw_hex(value: object, name: str) -> None:
    expect(isinstance(value, str), f"{name} should be str, got {type(value).__name__}")
    expect(len(value) > 0, f"{name} should not be empty")


def validate_code_page(page, exchange: str) -> None:
    expect(page.exchange == exchange, f"code page exchange mismatch: {page.exchange} != {exchange}")
    expect(page.count == len(page.items), f"code page count mismatch for {exchange}")
    expect(page.total >= page.count, f"code page total mismatch for {exchange}")
    expect(len(page.items) > 0, f"no codes returned for {exchange}")
    item = page.items[0]
    expect(item.exchange == exchange, f"first code exchange mismatch for {exchange}")
    expect(isinstance(item.name, str) and len(item.name) > 0, f"first code name missing for {exchange}")


def validate_quote(quote) -> None:
    expect(quote.exchange in {"sh", "sz", "bj"}, f"invalid quote exchange: {quote.exchange}")
    expect(isinstance(quote.code, str) and len(quote.code) == 6, f"invalid quote code: {quote.code}")
    expect(len(quote.buy_levels) == 5, f"quote buy level count mismatch for {quote.exchange}{quote.code}")
    expect(len(quote.sell_levels) == 5, f"quote sell level count mismatch for {quote.exchange}{quote.code}")
    expect_datetime(quote.server_time, f"quote server_time {quote.exchange}{quote.code}", allow_none=True)


def validate_kline_response(response, label: str, *, require_raw: bool) -> None:
    expect(response.count == len(response.items), f"{label} count mismatch")
    expect(len(response.items) > 0, f"{label} returned no items")
    item = response.items[-1]
    expect_datetime(item.time, f"{label}.time")
    if require_raw:
        expect_raw_hex(response.raw_frame_hex, f"{label}.raw_frame_hex")
        expect_raw_hex(response.raw_payload_hex, f"{label}.raw_payload_hex")


def validate_minute_response(response, label: str, *, require_raw: bool) -> None:
    expect(response.count == len(response.items), f"{label} count mismatch")
    expect_date(response.trading_date, f"{label}.trading_date")
    if response.items:
        item = response.items[-1]
        expect_datetime(item.time, f"{label}.time")
    else:
        warn(f"{label} returned 0 items")
    if require_raw:
        expect_raw_hex(response.raw_frame_hex, f"{label}.raw_frame_hex")
        expect_raw_hex(response.raw_payload_hex, f"{label}.raw_payload_hex")


def validate_trade_response(response, label: str, *, require_raw: bool) -> None:
    expect(response.count == len(response.items), f"{label} count mismatch")
    expect_date(response.trading_date, f"{label}.trading_date")
    if response.items:
        first = response.items[0]
        last = response.items[-1]
        expect_datetime(first.time, f"{label}.first.time")
        expect_datetime(last.time, f"{label}.last.time")
    else:
        warn(f"{label} returned 0 items")
    if require_raw:
        expect_raw_hex(response.raw_frame_hex, f"{label}.raw_frame_hex")
        expect_raw_hex(response.raw_payload_hex, f"{label}.raw_payload_hex")


def validate_call_auction_response(response, label: str, *, require_raw: bool) -> None:
    expect(response.count == len(response.items), f"{label} count mismatch")
    if response.items:
        item = response.items[0]
        expect_datetime(item.time, f"{label}.time")
        if item.raw_hex is None:
            warn(f"{label} first item raw_hex is None")
        else:
            expect_raw_hex(item.raw_hex, f"{label}.item.raw_hex")
    else:
        warn(f"{label} returned 0 items")
    if require_raw:
        expect_raw_hex(response.raw_frame_hex, f"{label}.raw_frame_hex")
        expect_raw_hex(response.raw_payload_hex, f"{label}.raw_payload_hex")


def build_quote_codes(client: TdxClient, quote_count: int) -> list[str]:
    first_half = max(1, quote_count // 2)
    second_half = max(0, quote_count - first_half)

    sz_page = client.get_codes("sz", limit=first_half)
    validate_code_page(sz_page, "sz")
    ok(f"get_codes(sz) page={len(sz_page.items)} total={sz_page.total}")

    codes = [item.full_code for item in sz_page.items]
    if second_half > 0:
        sh_page = client.get_codes("sh", limit=second_half)
        validate_code_page(sh_page, "sh")
        ok(f"get_codes(sh) page={len(sh_page.items)} total={sh_page.total}")
        codes.extend(item.full_code for item in sh_page.items)

    return codes[:quote_count]


def main() -> int:
    parser = argparse.ArgumentParser(description="Run one live end-to-end smoke pass across the core eltdx APIs.")
    parser.add_argument("--host", default=None, help="Single host override, example: 124.71.187.122:7709")
    parser.add_argument("--code", default="sz000001", help="Primary symbol code, example: sz000001")
    parser.add_argument("--history-date", default=None, help="Optional history trading date, example: 2026-03-06")
    parser.add_argument("--timeout", type=float, default=8.0, help="Socket timeout in seconds")
    parser.add_argument("--quote-count", type=int, default=120, help="How many symbols to request in quote batching smoke")
    parser.add_argument("--trade-count", type=int, default=20, help="How many trades to fetch in single-page trade smoke")
    parser.add_argument("--kline-count", type=int, default=5, help="How many kline rows to fetch in single-page kline smoke")
    parser.add_argument("--deep", action="store_true", help="Also run heavier *_all and full-code-list methods")
    args = parser.parse_args()

    expect(args.quote_count > 0, "quote_count must be > 0")
    expect(args.trade_count > 0, "trade_count must be > 0")
    expect(args.kline_count > 0, "kline_count must be > 0")

    with TdxClient(host=args.host, timeout=args.timeout, pool_size=2) as client:
        ok("connect() via context manager")

        counts = {exchange: client.get_count(exchange) for exchange in ("sh", "sz", "bj")}
        expect(all(value > 0 for value in counts.values()), f"invalid counts: {counts}")
        ok(f"get_count() sh={counts['sh']} sz={counts['sz']} bj={counts['bj']}")

        codes = build_quote_codes(client, args.quote_count)
        expect(len(codes) == args.quote_count, f"requested {args.quote_count} quote codes, got {len(codes)}")

        quotes = client.get_quote(tuple(codes))
        expect(len(quotes) == len(codes), f"quote count mismatch: requested {len(codes)}, got {len(quotes)}")
        returned = {f"{quote.exchange}{quote.code}" for quote in quotes}
        expect(returned == set(codes), "quote code set mismatch after automatic batching")
        for quote in quotes[: min(5, len(quotes))]:
            validate_quote(quote)
        ok(f"get_quote() returned {len(quotes)} quotes with automatic batching")

        day_kline = client.get_kline(args.code, "day", count=args.kline_count, include_raw=True)
        validate_kline_response(day_kline, "get_kline(day)", require_raw=True)
        last_day = day_kline.items[-1]
        history_date = args.history_date or last_day.time.date().isoformat()
        ok(f"get_kline(day) count={day_kline.count} latest={last_day.time.isoformat()} history_date={history_date}")

        minute_kline = client.get_kline(args.code, "1m", count=args.kline_count)
        validate_kline_response(minute_kline, "get_kline(1m)", require_raw=False)
        ok(f"get_kline(1m) count={minute_kline.count} latest={minute_kline.items[-1].time.isoformat()}")

        minute = client.get_minute(args.code, history_date, include_raw=True)
        validate_minute_response(minute, f"get_minute({history_date})", require_raw=True)
        ok(f"get_minute({history_date}) count={minute.count}")

        trades = client.get_trades(args.code, history_date, count=args.trade_count, include_raw=True)
        validate_trade_response(trades, f"get_trades({history_date})", require_raw=True)
        ok(f"get_trades({history_date}) count={trades.count}")

        auction = client.get_call_auction(args.code, include_raw=True)
        validate_call_auction_response(auction, "get_call_auction()", require_raw=True)
        ok(f"get_call_auction() count={auction.count}")

        if args.deep:
            trades_all = client.get_trades_all(args.code, history_date)
            validate_trade_response(trades_all, f"get_trades_all({history_date})", require_raw=False)
            expect(trades_all.count >= trades.count, "get_trades_all() should return at least one page of trades")
            ok(f"get_trades_all({history_date}) count={trades_all.count}")

            day_kline_all = client.get_kline_all(args.code, "day")
            validate_kline_response(day_kline_all, "get_kline_all(day)", require_raw=False)
            expect(day_kline_all.count >= day_kline.count, "get_kline_all() should return at least one page of kline")
            ok(f"get_kline_all(day) count={day_kline_all.count}")

            bj_codes = client.get_codes_all("bj")
            expect(len(bj_codes) > 0, "get_codes_all(bj) returned no items")
            ok(f"get_codes_all(bj) count={len(bj_codes)}")

            stock_codes = client.get_stock_codes_all()
            etf_codes = client.get_etf_codes_all()
            index_codes = client.get_index_codes_all()
            expect(len(stock_codes) > 0, "get_stock_codes_all() returned no items")
            expect(len(etf_codes) > 0, "get_etf_codes_all() returned no items")
            expect(len(index_codes) > 0, "get_index_codes_all() returned no items")
            ok(
                "full-code helpers stock={} etf={} index={}".format(
                    len(stock_codes), len(etf_codes), len(index_codes)
                )
            )

    if warnings:
        print(f"live smoke passed with {len(warnings)} warning(s)", flush=True)
    else:
        print("live smoke passed", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
