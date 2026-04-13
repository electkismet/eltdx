from __future__ import annotations

import argparse
import csv
import socket
import sys
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from pathlib import Path

ROOT = next(parent for parent in Path(__file__).resolve().parents if (parent / "pyproject.toml").exists())
SRC = ROOT / "src"
if str(SRC) not in sys.path:
    sys.path.insert(0, str(SRC))

from eltdx import TdxClient
from eltdx.exceptions import ConnectionClosedError, ProtocolError

DEFAULT_HOSTS = [
    "110.41.147.114:7709",
    "110.41.2.72:7709",
    "101.33.225.16:7709",
    "175.178.112.197:7709",
    "175.178.128.227:7709",
    "43.139.95.83:7709",
    "124.223.163.242:7709",
    "122.51.120.217:7709",
    "150.158.160.2:7709",
    "123.60.164.122:7709",
    "111.229.247.189:7709",
    "124.70.199.56:7709",
    "62.234.50.143:7709",
    "81.70.151.186:7709",
    "82.156.214.79:7709",
    "159.75.29.111:7709",
    "43.139.18.171:7709",
    "81.71.32.47:7709",
    "122.51.232.182:7709",
    "118.25.98.114:7709",
    "121.36.225.169:7709",
    "123.60.70.228:7709",
    "123.60.73.44:7709",
    "124.70.133.119:7709",
    "124.71.187.72:7709",
    "124.71.187.122:7709",
    "119.97.185.59:7709",
    "129.204.230.128:7709",
    "101.42.240.54:7709",
    "124.71.9.153:7709",
    "123.60.84.66:7709",
    "111.230.186.52:7709",
    "101.43.159.194:7709",
    "120.53.8.251:7709",
    "152.136.191.169:7709",
    "116.205.163.254:7709",
    "116.205.171.132:7709",
    "116.205.183.150:7709",
    "49.232.15.141:7709",
    "82.156.174.84:7709",
    "101.42.164.241:7709",
    "101.35.121.35:7709",
    "111.231.113.208:7709",
]


@dataclass(slots=True)
class ProbeResult:
    host: str
    latency_ms: float | None
    ok: bool


@dataclass(slots=True)
class AuctionRow:
    date: str
    code: str
    has_auction_0925: int
    open_price: float | None
    auction_volume_hand: int | None
    auction_amount_yuan: float | None
    pages_used: int
    source_mode: str
    host: str


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Export 09:25 auction rows to one CSV per trading day.")
    parser.add_argument("--start", required=True, help="Start date, for example: 2026-04-01")
    parser.add_argument("--end", required=True, help="End date, for example: 2026-04-09")
    parser.add_argument("--hosts", default=",".join(DEFAULT_HOSTS), help="Comma-separated candidate hosts")
    parser.add_argument("--host-count", type=int, default=4, help="How many fastest hosts to use")
    parser.add_argument("--workers-per-host", type=int, default=8, help="Worker threads per host")
    parser.add_argument("--pool-size", type=int, default=4, help="Long-lived connection count per host")
    parser.add_argument("--timeout", type=float, default=8.0, help="Socket timeout in seconds")
    parser.add_argument("--probe-timeout", type=float, default=0.8, help="TCP probe timeout in seconds")
    parser.add_argument("--export-dir", required=True, help="Directory for per-day CSV output")
    parser.add_argument("--limit", type=int, default=0, help="Optional code limit for quick tests")
    parser.add_argument(
        "--universe",
        choices=["mainboard_test", "a_share"],
        default="mainboard_test",
        help="Code universe: mainboard_test=SH/SZ main board prefixes only, a_share=main board + ChiNext + STAR",
    )
    return parser.parse_args()


def parse_csv_list(text: str) -> list[str]:
    return [item.strip() for item in text.split(",") if item.strip()]


def probe_host(host: str, timeout: float) -> ProbeResult:
    address, port_text = host.split(":", 1)
    port = int(port_text)
    started = time.perf_counter()
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.settimeout(timeout)
    try:
        sock.connect((address, port))
        latency_ms = round((time.perf_counter() - started) * 1000.0, 2)
        return ProbeResult(host=host, latency_ms=latency_ms, ok=True)
    except OSError:
        return ProbeResult(host=host, latency_ms=None, ok=False)
    finally:
        sock.close()


def pick_hosts(hosts: list[str], probe_timeout: float, host_count: int) -> list[str]:
    results = [probe_host(host, probe_timeout) for host in hosts]
    ok_results = sorted((item for item in results if item.ok), key=lambda item: item.latency_ms or 0.0)
    if len(ok_results) < host_count:
        raise RuntimeError(f"reachable hosts={len(ok_results)} is less than requested host_count={host_count}")
    selected = [item.host for item in ok_results[:host_count]]
    print("Selected hosts:")
    for item in ok_results[:host_count]:
        print(f"  {item.host:<24} {item.latency_ms:>7.2f} ms")
    return selected


def is_mainboard_test(code: str) -> bool:
    full = code.lower()
    number = full[2:]
    if full.startswith("sh"):
        return number.startswith(("600", "601", "603", "605"))
    if full.startswith("sz"):
        return number.startswith(("000", "001", "002", "003"))
    return False


def is_a_share(code: str) -> bool:
    full = code.lower()
    number = full[2:]
    if full.startswith("sh"):
        return number.startswith(("600", "601", "603", "605", "688", "689"))
    if full.startswith("sz"):
        return number.startswith(("000", "001", "002", "003", "300", "301"))
    return False


def load_codes(seed_host: str, timeout: float, universe: str, limit: int) -> list[str]:
    with TdxClient(host=seed_host, pool_size=1, timeout=timeout) as client:
        items = client.get_codes_all("sh") + client.get_codes_all("sz")
    if universe == "a_share":
        codes = sorted(item.full_code for item in items if is_a_share(item.full_code))
    else:
        codes = sorted(item.full_code for item in items if is_mainboard_test(item.full_code))
    if limit > 0:
        codes = codes[:limit]
    return codes


def load_trading_days(seed_host: str, timeout: float, start: str, end: str) -> list[str]:
    with TdxClient(host=seed_host, pool_size=1, timeout=timeout) as client:
        response = client.get_kline_all("day", "sh000001", kind="index")
    days = [item.time.date().isoformat() for item in response.items]
    return [day for day in days if start <= day <= end]


def partition_codes(codes: list[str], hosts: list[str]) -> dict[str, list[str]]:
    partitions = {host: [] for host in hosts}
    for index, code in enumerate(codes):
        partitions[hosts[index % len(hosts)]].append(code)
    return partitions


RETRYABLE_AUCTION_ERRORS = (ConnectionClosedError, ProtocolError, OSError)


def build_auction_row(result, trading_date: str, host: str) -> AuctionRow:
    return AuctionRow(
        date=trading_date,
        code=result.code,
        has_auction_0925=1 if result.has_auction_0925 else 0,
        open_price=result.price,
        auction_volume_hand=result.volume,
        auction_amount_yuan=result.amount,
        pages_used=result.pages_used,
        source_mode=result.source_mode,
        host=host,
    )


def fetch_auction_row(client: TdxClient, code: str, trading_date: str, host: str) -> AuctionRow:
    return build_auction_row(client.get_auction_0925(code, trading_date), trading_date, host)


def retry_fetch_auction_row(code: str, trading_date: str, failed_host: str, selected_hosts: list[str], timeout: float) -> AuctionRow:
    alternate_hosts = [host for host in selected_hosts if host != failed_host]
    last_error: Exception | None = None
    for host in alternate_hosts:
        try:
            with TdxClient(host=host, pool_size=1, timeout=timeout) as client:
                return fetch_auction_row(client, code, trading_date, host)
        except RETRYABLE_AUCTION_ERRORS as exc:
            last_error = exc
    raise RuntimeError(
        f"failed to export {code} on {trading_date} after host failover from {failed_host}"
    ) from last_error


def run_host_for_day(
    host: str,
    codes: list[str],
    trading_date: str,
    timeout: float,
    workers: int,
    pool_size: int,
    selected_hosts: list[str],
) -> tuple[list[AuctionRow], float, int]:
    started = time.perf_counter()
    retried = 0
    with TdxClient(host=host, pool_size=pool_size, timeout=timeout) as client:
        with ThreadPoolExecutor(max_workers=workers) as executor:
            future_map = {
                executor.submit(fetch_auction_row, client, code, trading_date, host): code
                for code in codes
            }
            rows: list[AuctionRow] = []
            for future in as_completed(future_map):
                code = future_map[future]
                try:
                    rows.append(future.result())
                except RETRYABLE_AUCTION_ERRORS:
                    rows.append(retry_fetch_auction_row(code, trading_date, host, selected_hosts, timeout))
                    retried += 1
    elapsed = time.perf_counter() - started
    rows.sort(key=lambda item: item.code)
    return rows, elapsed, retried


def write_day_csv(export_dir: Path, trading_date: str, rows: list[AuctionRow]) -> Path:
    path = export_dir / f"{trading_date}.csv"
    with path.open("w", newline="", encoding="utf-8-sig") as handle:
        writer = csv.writer(handle)
        writer.writerow(["date", "code", "has_auction_0925", "open_price", "auction_volume_hand", "auction_amount_yuan"])
        for row in rows:
            writer.writerow(
                [
                    row.date,
                    row.code,
                    row.has_auction_0925,
                    "" if row.open_price is None else f"{row.open_price:.3f}",
                    "" if row.auction_volume_hand is None else row.auction_volume_hand,
                    "" if row.auction_amount_yuan is None else f"{row.auction_amount_yuan:.2f}",
                ]
            )
    return path


def main() -> int:
    args = parse_args()
    export_dir = Path(args.export_dir).resolve()
    export_dir.mkdir(parents=True, exist_ok=True)

    selected_hosts = pick_hosts(parse_csv_list(args.hosts), args.probe_timeout, args.host_count)
    codes = load_codes(selected_hosts[0], args.timeout, args.universe, args.limit)
    trading_days = load_trading_days(selected_hosts[0], args.timeout, args.start, args.end)

    print(f"universe={args.universe} codes={len(codes)}")
    print(f"trading_days={len(trading_days)} {trading_days}")
    print(f"config hosts={len(selected_hosts)} workers_per_host={args.workers_per_host} pool_size={args.pool_size}")
    print()

    overall_started = time.perf_counter()
    for trading_date in trading_days:
        day_started = time.perf_counter()
        partitions = partition_codes(codes, selected_hosts)
        all_rows: list[AuctionRow] = []
        with ThreadPoolExecutor(max_workers=len(selected_hosts)) as executor:
            future_map = {
                executor.submit(
                    run_host_for_day,
                    host,
                    partitions[host],
                    trading_date,
                    args.timeout,
                    args.workers_per_host,
                    args.pool_size,
                    selected_hosts,
                ): host
                for host in selected_hosts
            }
            for future in as_completed(future_map):
                rows, elapsed, retried = future.result()
                host = future_map[future]
                host_missing = sum(1 for row in rows if row.has_auction_0925 == 0)
                host_found = len(rows) - host_missing
                print(
                    f"  {trading_date} host={host} codes={len(partitions[host])} elapsed={elapsed:.3f}s "
                    f"rows={host_found} missing={host_missing} retried={retried}"
                )
                all_rows.extend(rows)

        all_rows.sort(key=lambda item: item.code)
        day_csv = write_day_csv(export_dir, trading_date, all_rows)
        day_elapsed = time.perf_counter() - day_started
        missing_count = sum(1 for row in all_rows if row.has_auction_0925 == 0)
        found_count = len(all_rows) - missing_count
        print(
            f"{trading_date} finished in {day_elapsed:.3f}s  rows={found_count} missing={missing_count}  "
            f"csv={day_csv.name}"
        )
        print()

    overall_elapsed = time.perf_counter() - overall_started
    print(f"all done in {overall_elapsed:.3f}s  output={export_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
