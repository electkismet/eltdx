from __future__ import annotations

import argparse
import json
import sys
import time
import traceback
from dataclasses import fields, is_dataclass
from datetime import date, datetime
from pathlib import Path
from typing import Any, Callable

ROOT = next(parent for parent in Path(__file__).resolve().parents if (parent / "pyproject.toml").exists())
SRC = ROOT / "src"
if str(SRC) not in sys.path:
    sys.path.insert(0, str(SRC))

from eltdx import TdxClient, __version__  # noqa: E402
from eltdx.protocol.unit import is_a_share_entry  # noqa: E402


def serialize(value: Any) -> Any:
    if is_dataclass(value):
        return {item.name: serialize(getattr(value, item.name)) for item in fields(value)}
    if isinstance(value, datetime):
        return value.isoformat()
    if isinstance(value, date):
        return value.isoformat()
    if isinstance(value, Path):
        return str(value)
    if isinstance(value, dict):
        return {str(key): serialize(item) for key, item in value.items()}
    if isinstance(value, (list, tuple, set)):
        return [serialize(item) for item in value]
    return value


def write_json(path: Path, payload: Any) -> None:
    path.write_text(json.dumps(serialize(payload), ensure_ascii=False, indent=2), encoding="utf-8")


def write_text(path: Path, content: str) -> None:
    path.write_text(content, encoding="utf-8")


def write_lines(path: Path, items: list[str]) -> None:
    path.write_text("\n".join(items) + ("\n" if items else ""), encoding="utf-8")


def summarize(payload: Any) -> dict[str, Any]:
    data = serialize(payload)
    if isinstance(data, dict):
        summary: dict[str, Any] = {"type": "dict", "keys": list(data.keys())[:12]}
        if "count" in data:
            summary["count"] = data["count"]
        if "total" in data:
            summary["total"] = data["total"]
        if "trading_date" in data:
            summary["trading_date"] = data["trading_date"]
        if "items" in data and isinstance(data["items"], list):
            summary["items_len"] = len(data["items"])
            if data["items"]:
                summary["first_item"] = data["items"][0]
                summary["last_item"] = data["items"][-1]
        return summary
    if isinstance(data, list):
        summary = {"type": "list", "len": len(data)}
        if data:
            summary["first_item"] = data[0]
            summary["last_item"] = data[-1]
        return summary
    return {"type": type(data).__name__, "value": data}


class ExportRunner:
    def __init__(self, output_dir: Path) -> None:
        self.output_dir = output_dir
        self.results: list[dict[str, Any]] = []
        self.failures: list[dict[str, Any]] = []

    def run_json(self, name: str, file_name: str, action: Callable[[], Any], *, note: str = "") -> Any | None:
        return self._run(name, action, file_name=file_name, note=note, writer="json")

    def run_text(self, name: str, file_name: str, action: Callable[[], str], *, note: str = "") -> str | None:
        return self._run(name, action, file_name=file_name, note=note, writer="text")

    def run_lines(self, name: str, file_name: str, action: Callable[[], list[str]], *, note: str = "") -> list[str] | None:
        return self._run(name, action, file_name=file_name, note=note, writer="lines")

    def run_check(self, name: str, action: Callable[[], Any], *, note: str = "") -> Any | None:
        return self._run(name, action, note=note, writer=None)

    def _run(self, name: str, action: Callable[[], Any], *, file_name: str | None = None, note: str = "", writer: str | None = None) -> Any | None:
        started = time.perf_counter()
        print(f"[RUN] {name}", flush=True)
        try:
            payload = action()
            output_path: Path | None = None
            if file_name is not None:
                output_path = self.output_dir / file_name
                if writer == "json":
                    write_json(output_path, payload)
                elif writer == "text":
                    write_text(output_path, payload)
                elif writer == "lines":
                    write_lines(output_path, payload)
                else:
                    raise ValueError(f"unsupported writer: {writer}")
            duration = round(time.perf_counter() - started, 3)
            result = {
                "name": name,
                "status": "passed",
                "duration_seconds": duration,
                "file": None if output_path is None else output_path.name,
                "note": note,
                "summary": summarize(payload),
            }
            self.results.append(result)
            print(f"[OK]  {name} ({duration:.3f}s)", flush=True)
            return payload
        except Exception as exc:
            duration = round(time.perf_counter() - started, 3)
            failure = {
                "name": name,
                "status": "failed",
                "duration_seconds": duration,
                "file": file_name,
                "note": note,
                "error_type": type(exc).__name__,
                "error": str(exc),
                "traceback": traceback.format_exc(),
            }
            self.results.append(failure)
            self.failures.append(failure)
            print(f"[FAIL] {name} ({duration:.3f}s) {type(exc).__name__}: {exc}", flush=True)
            return None


def main() -> int:
    parser = argparse.ArgumentParser(description="Run all public eltdx APIs online once and export raw results for manual verification.")
    parser.add_argument("--host", default=None, help="Single host override, example: 124.71.187.122:7709")
    parser.add_argument("--code", default="sz000001", help="Primary symbol code used for per-symbol exports")
    parser.add_argument("--index-code", default="sh000001", help="Index symbol used for explicit kind=index kline validation")
    parser.add_argument("--quote-count", type=int, default=120, help="How many symbols to request for quote batching")
    parser.add_argument("--timeout", type=float, default=8.0, help="Socket timeout in seconds")
    parser.add_argument("--output-dir", default=None, help="Optional export directory override")
    args = parser.parse_args()

    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    output_dir = Path(args.output_dir).resolve() if args.output_dir else ROOT / "artifacts" / f"live_validation_{timestamp}"
    output_dir.mkdir(parents=True, exist_ok=True)

    runner = ExportRunner(output_dir)
    metadata = {
        "generated_at": datetime.now().astimezone().isoformat(),
        "package_version": __version__,
        "host_override": args.host,
        "code": args.code,
        "quote_count": args.quote_count,
        "timeout": args.timeout,
        "output_dir": str(output_dir),
    }
    write_json(output_dir / "meta.json", metadata)

    client = TdxClient(host=args.host, timeout=args.timeout, pool_size=2)

    runner.run_check("connect", client.connect, note="Manual lifecycle connect() path")
    runner.run_json(
        "with_context_count_sz",
        "with_context_count_sz.json",
        lambda: _with_context_count(args.host, args.timeout),
        note="Context manager path with TdxClient().__enter__/__exit__",
    )

    counts = runner.run_json(
        "get_count_all",
        "counts.json",
        lambda: {exchange: client.get_count(exchange) for exchange in ("sh", "sz", "bj")},
        note="Underlying code-table counts for all three supported exchanges",
    ) or {}
    stock_counts = runner.run_json(
        "get_stock_count_all",
        "stock_counts.json",
        lambda: {exchange: client.get_stock_count(exchange) for exchange in ("sh", "sz", "bj")},
        note="Derived stock counts across all three exchanges (includes B shares)",
    ) or {}
    a_share_counts = runner.run_json(
        "get_a_share_count_all",
        "a_share_counts.json",
        lambda: {exchange: client.get_a_share_count(exchange) for exchange in ("sh", "sz", "bj")},
        note="Derived A-share-like counts across all three exchanges (excludes SH900/SZ200)",
    ) or {}

    codes_page_sz = runner.run_json(
        "get_codes_sz_page",
        "codes_page_sz_start0_limit20.json",
        lambda: client.get_codes("sz", start=0, limit=20),
        note="One logical page from sz for manual spot checks",
    )
    runner.run_json(
        "get_codes_all_sh",
        "codes_all_sh.json",
        lambda: client.get_codes_all("sh"),
        note="Full code table for sh",
    )
    runner.run_json(
        "get_codes_all_sz",
        "codes_all_sz.json",
        lambda: client.get_codes_all("sz"),
        note="Full code table for sz",
    )
    runner.run_json(
        "get_codes_all_bj",
        "codes_all_bj.json",
        lambda: client.get_codes_all("bj"),
        note="Full code table for bj",
    )
    runner.run_lines(
        "get_stock_codes_all",
        "stock_codes_all.txt",
        client.get_stock_codes_all,
        note="All stock codes across exchanges",
    )
    runner.run_lines(
        "get_a_share_codes_all",
        "a_share_codes_all.txt",
        client.get_a_share_codes_all,
        note="All A-share-like stock codes across exchanges",
    )
    runner.run_lines(
        "get_etf_codes_all",
        "etf_codes_all.txt",
        client.get_etf_codes_all,
        note="All ETF codes across exchanges",
    )
    runner.run_lines(
        "get_index_codes_all",
        "index_codes_all.txt",
        client.get_index_codes_all,
        note="All index codes across exchanges",
    )

    quote_codes = _build_quote_codes(client, args.quote_count)
    write_lines(output_dir / "quote_request_codes.txt", quote_codes)
    runner.run_json(
        "get_quote_single",
        f"quote_single_{args.code}.json",
        lambda: client.get_quote(args.code),
        note="Single-code quote path",
    )
    runner.run_json(
        "get_quote_batch",
        f"quotes_batch_{len(quote_codes)}.json",
        lambda: client.get_quote(tuple(quote_codes)),
        note="Multi-code quote path with automatic batching",
    )

    day_kline = runner.run_json(
        "get_kline_day_page",
        f"kline_day_page_{args.code}.json",
        lambda: client.get_kline(args.code, "day", count=10, include_raw=True),
        note="Canonical get_kline(code, freq) path",
    )
    runner.run_json(
        "get_kline_day_page_index",
        f"kline_day_page_{args.index_code}_index.json",
        lambda: client.get_kline(args.index_code, "day", count=10, kind="index", include_raw=True),
        note="Index day kline page with explicit kind=index",
    )

    history_date = _pick_history_date(day_kline)
    if history_date is None:
        history_date = datetime.now().date().isoformat()

    runner.run_json(
        "get_kline_1m_page",
        f"kline_1m_page_{args.code}.json",
        lambda: client.get_kline(args.code, "1m", count=10),
        note="Minute-frequency kline page",
    )
    runner.run_json(
        "get_kline_all_day",
        f"kline_day_all_{args.code}.json",
        lambda: client.get_kline_all(args.code, "day"),
        note="Canonical get_kline_all(code, freq) path",
    )
    runner.run_json(
        "get_adjusted_kline_qfq",
        f"adjusted_kline_qfq_day_page_{args.code}.json",
        lambda: client.get_adjusted_kline("day", args.code, adjust="qfq", count=10, include_raw=True),
        note="QFQ adjusted day kline page",
    )
    runner.run_json(
        "get_adjusted_kline_hfq",
        f"adjusted_kline_hfq_day_page_{args.code}.json",
        lambda: client.get_adjusted_kline("day", args.code, adjust="hfq", count=10),
        note="HFQ adjusted day kline page",
    )
    runner.run_json(
        "get_adjusted_kline_all_qfq",
        f"adjusted_kline_qfq_day_all_{args.code}.json",
        lambda: client.get_adjusted_kline_all("day", args.code, adjust="qfq"),
        note="QFQ adjusted full day kline series",
    )

    runner.run_json(
        "get_minute_live",
        f"minute_live_{args.code}.json",
        lambda: client.get_minute(args.code, include_raw=True),
        note="No-date minute path",
    )
    minute_history = runner.run_json(
        "get_minute_history",
        f"minute_history_{history_date}_{args.code}.json",
        lambda: client.get_minute(args.code, history_date, include_raw=True),
        note="Historical minute path",
    )
    runner.run_json(
        "get_history_minute_alias",
        f"history_minute_alias_{history_date}_{args.code}.json",
        lambda: client.get_history_minute(args.code, history_date, include_raw=True),
        note="Compatibility alias around historical minute",
    )

    runner.run_json(
        "get_trades_live_page",
        f"trades_live_page_{args.code}.json",
        lambda: client.get_trades(args.code, count=50, include_raw=True),
        note="No-date trade page path",
    )
    trades_history_page = runner.run_json(
        "get_trades_history_page",
        f"trades_history_page_{history_date}_{args.code}.json",
        lambda: client.get_trades(args.code, history_date, count=50, include_raw=True),
        note="Historical trade page path",
    )
    runner.run_json(
        "get_trades_all_live",
        f"trades_live_all_{args.code}.json",
        lambda: client.get_trades_all(args.code),
        note="No-date trade all path",
    )
    trades_history_all = runner.run_json(
        "get_trades_all_history",
        f"trades_history_all_{history_date}_{args.code}.json",
        lambda: client.get_trades_all(args.code, history_date),
        note="Historical trade all path",
    )
    runner.run_json(
        "get_trade_alias",
        f"trade_alias_live_page_{args.code}.json",
        lambda: client.get_trade(args.code, count=50, include_raw=True),
        note="Compatibility alias for live trade page",
    )
    runner.run_json(
        "get_history_trade_alias",
        f"history_trade_alias_page_{history_date}_{args.code}.json",
        lambda: client.get_history_trade(args.code, history_date, count=50, include_raw=True),
        note="Compatibility alias for historical trade page",
    )
    runner.run_json(
        "get_trade_all_alias",
        f"trade_all_alias_live_{args.code}.json",
        lambda: client.get_trade_all(args.code),
        note="Compatibility alias for live trade all",
    )
    runner.run_json(
        "get_history_trade_day_alias",
        f"history_trade_day_alias_{history_date}_{args.code}.json",
        lambda: client.get_history_trade_day(args.code, history_date),
        note="Compatibility alias for historical trade all",
    )
    runner.run_json(
        "get_trade_minute_kline",
        f"trade_minute_kline_live_{args.code}.json",
        lambda: client.get_trade_minute_kline(args.code),
        note="Trade-detail to minute-kline aggregation, live path",
    )
    runner.run_json(
        "get_history_trade_minute_kline",
        f"trade_minute_kline_history_{history_date}_{args.code}.json",
        lambda: client.get_history_trade_minute_kline(args.code, history_date),
        note="Trade-detail to minute-kline aggregation, historical path",
    )

    gbbq = runner.run_json(
        "get_gbbq",
        f"gbbq_{args.code}.json",
        lambda: client.get_gbbq(args.code, include_raw=True),
        note="Raw gbbq corporate-action feed",
    )
    runner.run_json(
        "get_xdxr",
        f"xdxr_{args.code}.json",
        lambda: client.get_xdxr(args.code),
        note="Filtered xdxr items from gbbq",
    )
    runner.run_json(
        "get_equity_changes",
        f"equity_changes_{args.code}.json",
        lambda: client.get_equity_changes(args.code),
        note="Derived equity-change series",
    )
    runner.run_json(
        "get_equity_latest",
        f"equity_latest_{args.code}.json",
        lambda: client.get_equity(args.code),
        note="Latest derived equity snapshot",
    )
    runner.run_json(
        "get_turnover_sample",
        f"turnover_sample_{args.code}.json",
        lambda: {"code": args.code, "volume": 1000, "unit": "hand", "on": history_date, "turnover": client.get_turnover(args.code, 1000, on=history_date, unit="hand")},
        note="Sample turnover calculation using 1000 hand",
    )
    runner.run_json(
        "get_factors",
        f"factors_{args.code}.json",
        lambda: client.get_factors(args.code),
        note="Derived qfq/hfq factor series",
    )

    runner.run_json(
        "get_call_auction",
        f"call_auction_{args.code}.json",
        lambda: client.get_call_auction(args.code, include_raw=True),
        note="Call-auction records with raw hex",
    )

    alias_checks = _build_alias_checks(
        code=args.code,
        history_date=history_date,
        canonical_day_kline=day_kline,
        canonical_minute_history=minute_history,
        canonical_trades_history_page=trades_history_page,
        canonical_trades_history_all=trades_history_all,
        client=client,
    )
    write_json(output_dir / "alias_checks.json", alias_checks)

    runner.run_check("close", client.close, note="Manual lifecycle close() path")

    summary = {
        "meta": metadata,
        "counts": counts,
        "stock_counts": stock_counts,
        "a_share_counts": a_share_counts,
        "history_date_used": history_date,
        "results": runner.results,
        "failures": runner.failures,
        "alias_checks": alias_checks,
    }
    write_json(output_dir / "summary.json", summary)
    write_text(output_dir / "REPORT.md", _build_report(output_dir, summary))

    print(f"\n导出目录: {output_dir}", flush=True)
    print(f"报告文件: {output_dir / 'REPORT.md'}", flush=True)
    print(f"汇总文件: {output_dir / 'summary.json'}", flush=True)
    print(f"失败数量: {len(runner.failures)}", flush=True)

    return 1 if runner.failures else 0


def _with_context_count(host: str | None, timeout: float) -> dict[str, Any]:
    with TdxClient(host=host, timeout=timeout, pool_size=2) as client:
        return {"sz": client.get_count("sz")}


def _pick_history_date(day_kline: Any) -> str | None:
    if day_kline is None or getattr(day_kline, "items", None) is None:
        return None
    if not day_kline.items:
        return None
    return day_kline.items[-1].time.date().isoformat()


def _build_quote_codes(client: TdxClient, quote_count: int) -> list[str]:
    first_half = max(1, quote_count // 2)
    second_half = max(0, quote_count - first_half)

    sz_codes = [item.full_code for item in client.get_codes_all("sz") if is_a_share_entry(item.full_code)]
    sh_codes = [item.full_code for item in client.get_codes_all("sh") if is_a_share_entry(item.full_code)]
    bj_codes = [item.full_code for item in client.get_codes_all("bj") if is_a_share_entry(item.full_code)]

    codes: list[str] = []
    codes.extend(sz_codes[:first_half])
    codes.extend(sh_codes[:second_half])

    if len(codes) < quote_count:
        codes.extend(bj_codes[: quote_count - len(codes)])
    if len(codes) < quote_count:
        codes.extend(sh_codes[second_half : second_half + (quote_count - len(codes))])
    if len(codes) < quote_count:
        codes.extend(sz_codes[first_half : first_half + (quote_count - len(codes))])

    return codes[:quote_count]


def _normalize_for_compare(value: Any) -> Any:
    data = serialize(value)
    if isinstance(data, dict):
        return {
            key: _normalize_for_compare(item)
            for key, item in data.items()
            if key not in {"raw_frame_hex", "raw_payload_hex", "raw_hex"}
        }
    if isinstance(data, list):
        return [_normalize_for_compare(item) for item in data]
    return data


def _build_alias_checks(
    *,
    code: str,
    history_date: str,
    canonical_day_kline: Any,
    canonical_minute_history: Any,
    canonical_trades_history_page: Any,
    canonical_trades_history_all: Any,
    client: TdxClient,
) -> dict[str, Any]:
    checks: dict[str, Any] = {
        "history_date": history_date,
        "checks": [],
    }

    def add(name: str, left: Any, right: Any) -> None:
        checks["checks"].append(
            {
                "name": name,
                "match": _normalize_for_compare(left) == _normalize_for_compare(right),
            }
        )

    try:
        add("get_kline(code,freq)==get_kline(freq,code)", canonical_day_kline, client.get_kline("day", code, count=10, include_raw=True))
    except Exception as exc:
        checks["checks"].append({"name": "get_kline(code,freq)==get_kline(freq,code)", "match": False, "error": str(exc)})

    try:
        add("get_kline_all(code,freq)==get_kline_all(freq,code)", client.get_kline_all(code, "day"), client.get_kline_all("day", code))
    except Exception as exc:
        checks["checks"].append({"name": "get_kline_all(code,freq)==get_kline_all(freq,code)", "match": False, "error": str(exc)})

    try:
        add("get_minute(code,date)==get_history_minute(code,date)", canonical_minute_history, client.get_history_minute(code, history_date, include_raw=True))
    except Exception as exc:
        checks["checks"].append({"name": "get_minute(code,date)==get_history_minute(code,date)", "match": False, "error": str(exc)})

    try:
        add("get_trades(code,date)==get_history_trade(code,date)", canonical_trades_history_page, client.get_history_trade(code, history_date, count=50, include_raw=True))
    except Exception as exc:
        checks["checks"].append({"name": "get_trades(code,date)==get_history_trade(code,date)", "match": False, "error": str(exc)})

    try:
        add("get_trades_all(code,date)==get_history_trade_day(code,date)", canonical_trades_history_all, client.get_history_trade_day(code, history_date))
    except Exception as exc:
        checks["checks"].append({"name": "get_trades_all(code,date)==get_history_trade_day(code,date)", "match": False, "error": str(exc)})

    return checks


def _build_report(output_dir: Path, summary: dict[str, Any]) -> str:
    meta = summary["meta"]
    failures = summary["failures"]
    results = summary["results"]
    alias_checks = summary["alias_checks"]["checks"]

    passed = sum(1 for item in results if item["status"] == "passed")
    failed = sum(1 for item in results if item["status"] == "failed")

    lines = [
        "# eltdx 在线全量核对报告",
        "",
        f"- 生成时间：{meta['generated_at']}",
        f"- 包版本：{meta['package_version']}",
        f"- 主核对代码：{meta['code']}",
        f"- 历史日期：{summary['history_date_used']}",
        f"- 输出目录：{output_dir}",
        f"- 通过步骤：{passed}",
        f"- 失败步骤：{failed}",
        "",
        "## 你先看哪些文件",
        "",
        "- `summary.json`：所有步骤的总状态、耗时、失败信息",
        "- `counts.json`：三大市场代码表条目数，适合先做总量核对",
        "- `codes_all_sh.json` / `codes_all_sz.json` / `codes_all_bj.json`：完整底层代码表",
        "- `quotes_batch_120.json`：120 只股票批量行情，顺带覆盖自动分批",
        f"- `minute_history_{summary['history_date_used']}_{meta['code']}.json`：分时历史数据",
        f"- `trades_history_all_{summary['history_date_used']}_{meta['code']}.json`：逐笔历史全量数据",
        f"- `kline_day_all_{meta['code']}.json`：日 K 全量数据",
        f"- `call_auction_{meta['code']}.json`：集合竞价原始解析结果",
        f"- `gbbq_{meta['code']}.json` / `xdxr_{meta['code']}.json` / `factors_{meta['code']}.json`：复权相关核对文件",
        "",
        "## 步骤结果",
        "",
    ]

    for item in results:
        if item["status"] == "passed":
            file_text = "无输出文件" if item.get("file") is None else item["file"]
            lines.append(f"- PASS `{item['name']}` | 文件：`{file_text}` | 耗时：{item['duration_seconds']}s")
        else:
            lines.append(f"- FAIL `{item['name']}` | 错误：{item['error_type']}: {item['error']}")

    lines.extend([
        "",
        "## 兼容别名核对",
        "",
    ])
    for item in alias_checks:
        if item.get("match"):
            lines.append(f"- MATCH `{item['name']}`")
        else:
            error = item.get("error")
            if error:
                lines.append(f"- FAIL `{item['name']}` | 错误：{error}")
            else:
                lines.append(f"- FAIL `{item['name']}` | 结果不一致")

    if failures:
        lines.extend([
            "",
            "## 失败详情",
            "",
        ])
        for item in failures:
            lines.append(f"- `{item['name']}` -> {item['error_type']}: {item['error']}")

    lines.extend([
        "",
        "## 核对建议",
        "",
        "- 先看 `counts.json` 是否和你手头的代码表总量一致。",
        "- 再看 `codes_all_*.json` 是否有缺市场、缺代码、名称错位。",
        "- 再看 `quotes_batch_120.json` 的时间、买卖五档、价格字段。",
        "- 对单只股票，优先核对 `minute_history`、`trades_history_all`、`kline_day_all` 三份文件。",
        "- 如果要查复权链路，再看 `gbbq`、`xdxr`、`factors`、`adjusted_kline_*`。",
    ])

    return "\n".join(lines) + "\n"


if __name__ == "__main__":
    raise SystemExit(main())
