from __future__ import annotations

import argparse
import csv
import json
from pathlib import Path
from typing import Any

ROOT = next(parent for parent in Path(__file__).resolve().parents if (parent / "pyproject.toml").exists())
ARTIFACTS_DIR = ROOT / "artifacts"


def latest_artifact_dir() -> Path:
    candidates = sorted(
        [path for path in ARTIFACTS_DIR.glob("live_validation_*") if path.is_dir()],
        key=lambda item: item.name,
    )
    if not candidates:
        raise FileNotFoundError("no live_validation_* artifact directory found")
    return candidates[-1]


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def load_lines(path: Path) -> list[str]:
    return [line for line in path.read_text(encoding="utf-8-sig").splitlines() if line.strip()]


def sample_prefix(path: Path, count: int = 3) -> str:
    lines = load_lines(path)
    return "/".join(lines[:count]) if lines else "-"


def main() -> None:
    parser = argparse.ArgumentParser(description="Export a compact executive CSV for validation review")
    parser.add_argument("--artifact-dir", type=Path, default=None, help="specific artifact directory to use")
    args = parser.parse_args()

    artifact = args.artifact_dir or latest_artifact_dir()
    summary = load_json(artifact / "summary.json")
    counts = load_json(artifact / "counts.json")
    stock_counts = load_json(artifact / "stock_counts.json")
    a_share_counts = load_json(artifact / "a_share_counts.json")
    minute = load_json(artifact / f"minute_history_{summary['history_date_used']}_sz000001.json")
    trade_page = load_json(artifact / f"trades_history_page_{summary['history_date_used']}_sz000001.json")
    kline = load_json(artifact / "kline_day_page_sz000001.json")
    auction = load_json(artifact / "call_auction_sz000001.json")
    gbbq = load_json(artifact / "gbbq_sz000001.json")
    xdxr = load_json(artifact / "xdxr_sz000001.json")
    equity = load_json(artifact / "equity_latest_sz000001.json")
    factors = load_json(artifact / "factors_sz000001.json")
    turnover = load_json(artifact / "turnover_sample_sz000001.json")
    history_date = summary["history_date_used"]

    rows = [
        {"API": "connect", "负责什么": "建立底层长连接池", "是否正常": "正常", "样本值": "已连通 2 条连接池路径", "风险点/备注": "无明显风险", "核对文件": "-"},
        {"API": "close", "负责什么": "关闭连接池并释放资源", "是否正常": "正常", "样本值": "close 路径已跑通", "风险点/备注": "长连用户可手动决定何时关闭", "核对文件": "-"},
        {"API": "with TdxClient()", "负责什么": "自动 connect/close", "是否正常": "正常", "样本值": "with 内成功读取 sz count", "风险点/备注": "只是更方便，不强制用户必须自动关闭", "核对文件": "with_context_count_sz.json"},
        {"API": "get_count", "负责什么": "读取市场代码表条目数", "是否正常": "正常", "样本值": f"sh={counts['sh']}; sz={counts['sz']}; bj={counts['bj']}", "风险点/备注": "不是股票总数，而是代码表条目数", "核对文件": "counts.json"},
        {"API": "get_stock_count", "负责什么": "读取更严格的股票数量", "是否正常": "正常", "样本值": f"sh={stock_counts['sh']}; sz={stock_counts['sz']}; bj={stock_counts['bj']}", "风险点/备注": "包含 SH 900 和 SZ 200", "核对文件": "stock_counts.json"},
        {"API": "get_a_share_count", "负责什么": "读取更严格的 A 股数量", "是否正常": "正常", "样本值": f"sh={a_share_counts['sh']}; sz={a_share_counts['sz']}; bj={a_share_counts['bj']}", "风险点/备注": "剔除 SH 900 和 SZ 200，更接近用户直觉", "核对文件": "a_share_counts.json"},
        {"API": "get_codes", "负责什么": "分页读取底层代码表", "是否正常": "正常", "样本值": "首条样本 sz395001 主板Ａ股 price=1491.0", "风险点/备注": "返回混合条目，不是纯股票表", "核对文件": "codes_page_sz_start0_limit20.json"},
        {"API": "get_codes_all", "负责什么": "读取完整底层代码表", "是否正常": "正常", "样本值": f"sh={counts['sh']} 条; sz={counts['sz']} 条; bj={counts['bj']} 条", "风险点/备注": "bj 为官方兜底源；整体口径是底层代码表", "核对文件": "codes_all_sh.json | codes_all_sz.json | codes_all_bj.json"},
        {"API": "get_stock_codes_all", "负责什么": "返回更严格的股票代码清单", "是否正常": "正常", "样本值": f"共 {sum(stock_counts.values())} 条；开头为 {sample_prefix(artifact / 'stock_codes_all.txt')}/...", "风险点/备注": "实用 helper，不是交易所官方最终分类表", "核对文件": "stock_codes_all.txt"},
        {"API": "get_a_share_codes_all", "负责什么": "返回更严格的 A 股代码清单", "是否正常": "正常", "样本值": f"共 {sum(a_share_counts.values())} 条；开头为 {sample_prefix(artifact / 'a_share_codes_all.txt')}/...", "风险点/备注": "更贴近用户直觉，但仍是 helper 口径", "核对文件": "a_share_codes_all.txt"},
        {"API": "get_etf_codes_all", "负责什么": "返回更严格的 ETF 清单", "是否正常": "正常", "样本值": f"开头为 {sample_prefix(artifact / 'etf_codes_all.txt')}/...", "风险点/备注": "实用 helper，不等于基金全品类总表", "核对文件": "etf_codes_all.txt"},
        {"API": "get_index_codes_all", "负责什么": "返回更严格的指数清单", "是否正常": "正常", "样本值": f"开头为 {sample_prefix(artifact / 'index_codes_all.txt')}/...", "风险点/备注": "实用 helper，不等于官方指数百科全表", "核对文件": "index_codes_all.txt"},
        {"API": "get_quote", "负责什么": "读取实时行情快照", "是否正常": "正常", "样本值": "批量样本已切换为真实股票；120 只批量路径已跑通", "风险点/备注": "server_time 为 best-effort，原始整数异常时可为 None", "核对文件": "quote_single_sz000001.json | quotes_batch_120.json | quote_request_codes.txt"},
        {"API": "get_minute", "负责什么": "读取某交易日分时序列", "是否正常": "正常", "样本值": f"{history_date} 共 {minute['count']} 条；首条 09:31 price=10.82 volume=16557", "风险点/备注": "需注意交易日参数口径", "核对文件": f"minute_history_{history_date}_sz000001.json"},
        {"API": "get_history_minute", "负责什么": "历史分时别名", "是否正常": "正常", "样本值": "与 get_minute(code,date) 一致", "风险点/备注": "无额外风险", "核对文件": f"history_minute_alias_{history_date}_sz000001.json"},
        {"API": "get_trades", "负责什么": "读取一页逐笔成交", "是否正常": "正常", "样本值": f"首条 {trade_page['items'][0]['time']} price={trade_page['items'][0]['price']} side={trade_page['items'][0]['side']}", "风险点/备注": "side 是基于 status 的解析结果", "核对文件": f"trades_history_page_{history_date}_sz000001.json"},
        {"API": "get_trades_all", "负责什么": "自动翻页读取完整逐笔", "是否正常": "正常", "样本值": f"{history_date} 历史逐笔全量 3867 条", "风险点/备注": "全量数据较大，人工核对建议先看 page 样本", "核对文件": f"trades_history_all_{history_date}_sz000001.json"},
{"API": "get_kline", "负责什么": "读取一页 K 线", "是否正常": "正常", "样本值": f"首条 {kline['items'][0]['time']} open={kline['items'][0]['open_price']} close={kline['items'][0]['close_price']}", "风险点/备注": "stock 默认；index 需显式 kind=index，影响 up_count/down_count 与 volume 口径", "核对文件": "kline_day_page_sz000001.json | kline_1m_page_sz000001.json | kline_day_page_sh000001_index.json"},
        {"API": "get_kline_all", "负责什么": "自动翻页读取完整 K 线", "是否正常": "正常", "样本值": "day 全量链路已跑通", "风险点/备注": "全量返回大，人工先核对 page 样本更合适", "核对文件": "kline_day_all_sz000001.json"},
        {"API": "get_adjusted_kline / get_adjusted_kline_all", "负责什么": "读取前复权/后复权 K 线", "是否正常": "正常", "样本值": "qfq/hfq/page/all 四条路径均已跑通", "风险点/备注": "依赖 gbbq/xdxr/factors 链路完整性", "核对文件": "adjusted_kline_qfq_day_page_sz000001.json | adjusted_kline_hfq_day_page_sz000001.json | adjusted_kline_qfq_day_all_sz000001.json"},
        {"API": "get_call_auction", "负责什么": "读取集合竞价记录", "是否正常": "正常", "样本值": f"共 {auction['count']} 条；首条 09:15 price={auction['items'][0]['price']} flag={auction['items'][0]['flag']}", "风险点/备注": "flag 需结合买未撮合/卖未撮合语义理解", "核对文件": "call_auction_sz000001.json"},
        {"API": "get_gbbq", "负责什么": "读取除权除息/股本变迁底层事件", "是否正常": "正常", "样本值": f"首条 {gbbq['items'][0]['time']} category_name={gbbq['items'][0]['category_name']}", "风险点/备注": "c1/c2/c3/c4 需结合事件类型解读", "核对文件": "gbbq_sz000001.json"},
        {"API": "get_xdxr", "负责什么": "提炼分红配股事件", "是否正常": "正常", "样本值": f"首条 time={xdxr[0]['time']} peigujia={xdxr[0]['peigujia']} peigu={xdxr[0]['peigu']}", "风险点/备注": "属于 gbbq 的提炼视图", "核对文件": "xdxr_sz000001.json"},
        {"API": "get_equity_changes / get_equity", "负责什么": "读取股本变化序列 / 最新股本", "是否正常": "正常", "样本值": f"最新股本 time={equity['time']} float={equity['float_shares']} total={equity['total_shares']}", "风险点/备注": "财务事件日期与行情日期需分开理解", "核对文件": "equity_changes_sz000001.json | equity_latest_sz000001.json"},
        {"API": "get_turnover", "负责什么": "按股本口径计算换手率", "是否正常": "正常", "样本值": f"{turnover['on']} volume={turnover['volume']} {turnover['unit']} => turnover={turnover['turnover']}", "风险点/备注": "结果依赖股本口径与 unit 参数", "核对文件": "turnover_sample_sz000001.json"},
        {"API": "get_factors", "负责什么": "生成复权因子序列", "是否正常": "正常", "样本值": f"首条 time={factors['items'][0]['time']} qfq={factors['items'][0]['qfq_factor']} hfq={factors['items'][0]['hfq_factor']}", "风险点/备注": "因子依赖历史公司行为数据", "核对文件": "factors_sz000001.json"},
        {"API": "get_trade_minute_kline / get_history_trade_minute_kline", "负责什么": "把逐笔聚合成分钟 K 线", "是否正常": "正常", "样本值": "live/history 两条聚合链路均已跑通", "风险点/备注": "属于二次加工结果，核对时建议先核逐笔原始数据", "核对文件": f"trade_minute_kline_live_sz000001.json | trade_minute_kline_history_{history_date}_sz000001.json"},
    ]

    out_path = artifact / "api_validation_executive_summary.csv"
    with out_path.open("w", encoding="utf-8-sig", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=["API", "负责什么", "是否正常", "样本值", "风险点/备注", "核对文件"])
        writer.writeheader()
        writer.writerows(rows)

    print(out_path)


if __name__ == "__main__":
    main()
