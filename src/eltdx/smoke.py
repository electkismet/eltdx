"""Live smoke command implementation."""

from __future__ import annotations

import argparse
from datetime import date
from typing import Any

from eltdx.client import TdxClient
from eltdx.transport._config import DEFAULT_SERVER_COUNT


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Run a live smoke check against 7709 quote servers.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument("--host", default=None, help="single 7709 host, for example 116.205.183.150:7709")
    parser.add_argument("--code", default="sz000001", help="primary code to test")
    parser.add_argument("--history-date", default=None, help="history trading date, for example 2026-05-20")
    parser.add_argument("--timeout", type=float, default=8.0, help="socket timeout seconds")
    parser.add_argument(
        "--pool-size",
        type=int,
        default=None,
        help="total connection count; overrides the derived default when provided",
    )
    parser.add_argument(
        "--server-count",
        type=int,
        default=DEFAULT_SERVER_COUNT,
        help="number of fastest ranked servers to use",
    )
    parser.add_argument(
        "--connections-per-server",
        type=int,
        default=None,
        help="connections per selected server (default: 4 when pool-size is omitted)",
    )
    parser.add_argument(
        "--runtime-workers",
        type=int,
        default=None,
        help="Tokio worker threads; automatic when omitted",
    )
    parser.add_argument(
        "--max-connections-per-host",
        type=int,
        default=None,
        help="hard active connection cap for one server",
    )
    parser.add_argument(
        "--connect-concurrency",
        type=int,
        default=None,
        help="global concurrent TCP connect and handshake limit",
    )
    parser.add_argument(
        "--connect-concurrency-per-host",
        type=int,
        default=None,
        help="per-server concurrent TCP connect and handshake limit",
    )
    parser.add_argument(
        "--probe-hosts",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="probe, persist, and sort candidate hosts before connecting",
    )
    parser.add_argument("--heartbeat-interval", type=float, default=30.0, help="background heartbeat interval seconds")
    parser.add_argument("--no-heartbeat", action="store_true", help="disable background heartbeat")
    parser.add_argument("--quote-count", type=int, default=120, help="number of symbols for quote batching")
    parser.add_argument("--trade-count", type=int, default=20, help="single-page trade count")
    parser.add_argument("--kline-count", type=int, default=5, help="single-page kline count")
    parser.add_argument("--deep", action="store_true", help="also run heavier all-page and derived-data checks")
    args = parser.parse_args()

    heartbeat_interval = None if args.no_heartbeat else args.heartbeat_interval
    if args.quote_count <= 0 or args.trade_count <= 0 or args.kline_count <= 0:
        raise SystemExit("quote-count, trade-count and kline-count must be positive")

    with TdxClient(
        host=args.host,
        timeout=args.timeout,
        pool_size=args.pool_size,
        server_count=args.server_count,
        connections_per_server=args.connections_per_server,
        runtime_workers=args.runtime_workers,
        max_connections_per_host=args.max_connections_per_host,
        connect_concurrency=args.connect_concurrency,
        connect_concurrency_per_host=args.connect_concurrency_per_host,
        probe_hosts=args.probe_hosts,
        heartbeat_interval=heartbeat_interval,
    ) as client:
        ok("connected")
        smoke_session(client)
        smoke_codes(client)
        smoke_quotes(client, args.quote_count)
        history_date = smoke_klines(client, args.code, args.kline_count, args.history_date)
        smoke_minutes(client, args.code, history_date)
        smoke_trades(client, args.code, history_date, args.trade_count)
        smoke_auctions(client, args.code, history_date)
        smoke_limits(client)
        smoke_corporate(client, args.code)
        if args.deep:
            smoke_deep(client, args.code, history_date)

    ok("live smoke passed")
    return 0


def smoke_session(client: TdxClient) -> None:
    heartbeat = client.session.heartbeat()
    assert_true(heartbeat.server_date is None or isinstance(heartbeat.server_date, date), "heartbeat server date")
    ok(f"heartbeat server_date={heartbeat.server_date}")


def smoke_codes(client: TdxClient) -> None:
    counts = {market: client.codes.count(market) for market in ("sh", "sz", "bj")}
    assert_true(all(value > 0 for value in counts.values()), f"invalid code counts: {counts}")
    page = client.codes.list("sz", limit=5)
    assert_true(len(page) > 0, "empty sz code page")
    ok(f"codes sh={counts['sh']} sz={counts['sz']} bj={counts['bj']} first_sz={page[0].full_code}")


def smoke_quotes(client: TdxClient, quote_count: int) -> None:
    candidates = ["sz000001", "sh600000"]
    if quote_count > len(candidates):
        candidates.extend(client.codes.all_a_shares())
    codes = list(dict.fromkeys(candidates))[:quote_count]
    assert_true(codes, "no quote test codes")
    quotes = client.helpers.full_quotes(codes)
    assert_true(len(quotes) == len(codes), f"quote count mismatch: {len(quotes)} != {len(codes)}")
    first = quotes[0]
    assert_true(first.full_code == codes[0], "first quote code mismatch")
    assert_true(len(first.buy_levels) == 5 and len(first.sell_levels) == 5, "quote levels should be 5x5")
    assert_true(first.last_price > 0, "first quote should be an active stock with a positive price")
    ok(f"quotes count={len(quotes)} first={first.full_code} price={first.last_price}")


def smoke_klines(client: TdxClient, code: str, count: int, history_date: str | None) -> str:
    day = client.bars.get(code, period="day", count=count, include_raw=True)
    assert_true(day.count > 0, "empty day kline")
    assert_true(day.raw_payload, "day kline raw payload missing")
    assert_true(day.bars[-1].record_hex, "day kline record hex missing")
    qfq = client.bars.get(code, period="day", adjust="qfq", count=count)
    assert_true(qfq.count > 0, "empty qfq kline")
    selected_date = history_date or day.bars[-1].time.date().isoformat()
    ok(f"kline day={day.count} qfq={qfq.count} history_date={selected_date}")
    return selected_date


def smoke_minutes(client: TdxClient, code: str, history_date: str) -> None:
    today = client.minutes.today(code, include_raw=True)
    assert_true(today.raw_payload, "today minute raw payload missing")
    history = client.minutes.history(code, history_date, include_raw=True)
    assert_true(history.raw_payload, "history minute raw payload missing")
    recent = client.minutes.recent(code, history_date)
    assert_true(hasattr(recent, "count"), "recent minute response missing count")
    ok(f"minutes today={today.count} history={history.count} recent={recent.count}")


def smoke_trades(client: TdxClient, code: str, history_date: str, count: int) -> None:
    today = client.trades.today(code, count=count, include_raw=True)
    assert_true(today.raw_payload, "today trade raw payload missing")
    history = client.trades.history(code, history_date, count=count, include_raw=True)
    assert_true(history.raw_payload, "history trade raw payload missing")
    ok(f"trades today={today.count} history={history.count}")


def smoke_auctions(client: TdxClient, code: str, history_date: str) -> None:
    series = client.auctions.series(code, include_raw=True)
    assert_true(series.raw_payload, "auction raw payload missing")
    auction_0925 = client.trades.opening_match_history(code, history_date, max_pages=5)
    ok(f"auction series={series.count} 0925={auction_0925 is not None}")


def smoke_limits(client: TdxClient) -> None:
    page = client.limits.special(start_index=0)
    assert_true(hasattr(page, "count"), "special limits response missing count")
    ok(f"special_limits count={page.count}")


def smoke_corporate(client: TdxClient, code: str) -> None:
    gbbq = client.corporate.capital_changes(code, include_raw=True)
    assert_true(gbbq.raw_payload, "gbbq raw payload missing")
    finance = client.corporate.finance_batch([code])
    assert_true(finance.count == 1, "finance batch should return one record")
    ok(f"corporate gbbq={gbbq.count} finance={finance.count}")


def smoke_deep(client: TdxClient, code: str, history_date: str) -> None:
    all_day = client.bars.get(code, period="day", all_pages=True, max_pages=3)
    assert_true(all_day.count > 0, "empty all day kline")
    all_trades = client.trades.all_history(code, history_date, max_pages=3)
    assert_true(all_trades.count > 0, "empty all history trades")
    a_share_codes = client.codes.all_a_shares()
    etf_codes = client.codes.all_etfs()
    index_codes = client.codes.all_indices()
    factors = client.corporate.adjustment_factors(code)
    sparkline = client.minutes.sparkline(code)
    aux = client.minutes.aux(code)
    assert_true(a_share_codes and etf_codes and index_codes, "code helpers returned empty lists")
    assert_true(factors.count > 0, "empty factors")
    ok(
        "deep kline={} trades={} a_share={} etf={} index={} factors={} sparkline={} aux={}".format(
            all_day.count,
            all_trades.count,
            len(a_share_codes),
            len(etf_codes),
            len(index_codes),
            factors.count,
            sparkline.count,
            aux.count,
        )
    )


def assert_true(condition: Any, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def ok(message: str) -> None:
    print(f"[OK] {message}", flush=True)
