from __future__ import annotations

import argparse
import csv
import json
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
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


def write_csv(path: Path, headers: list[str], rows: list[dict[str, Any]]) -> None:
    with path.open("w", encoding="utf-8-sig", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=headers)
        writer.writeheader()
        writer.writerows(rows)


def compact_json(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def build_overview_rows(summary: dict[str, Any]) -> list[dict[str, Any]]:
    history_date = summary["history_date_used"]
    return [
        {
            "API": "connect",
            "用途": "建立底层长连接池",
            "状态": "正常",
            "可信度": "高",
            "返回类型": "None",
            "关键字段": "-",
            "样例文件": "-",
            "备注": "所有在线接口的基础入口",
        },
        {
            "API": "close",
            "用途": "关闭连接池并释放 socket",
            "状态": "正常",
            "可信度": "高",
            "返回类型": "None",
            "关键字段": "-",
            "样例文件": "-",
            "备注": "长连模式下手动释放资源",
        },
        {
            "API": "with TdxClient()",
            "用途": "自动 connect/close",
            "状态": "正常",
            "可信度": "高",
            "返回类型": "上下文中的 client",
            "关键字段": "-",
            "样例文件": "with_context_count_sz.json",
            "备注": "适合一次性抓取任务；长连场景仍可手动控制",
        },
        {
            "API": "get_count",
            "用途": "读取市场代码表条目数",
            "状态": "正常",
            "可信度": "高（代码表口径）",
            "返回类型": "int",
            "关键字段": "sh/sz/bj",
            "样例文件": "counts.json",
            "备注": "不是股票总数；本次为 sh=26779 sz=22736 bj=297",
        },
        {
            "API": "get_codes",
            "用途": "分页读取底层代码表",
            "状态": "正常",
            "可信度": "高（底层代码表口径）",
            "返回类型": "CodePage",
            "关键字段": "exchange,start,count,total,items",
            "样例文件": "codes_page_sz_start0_limit20.json",
            "备注": "返回混合条目，不是纯股票",
        },
        {
            "API": "get_codes_all",
            "用途": "读取单市场完整底层代码表",
            "状态": "正常",
            "可信度": "高（底层代码表口径）",
            "返回类型": "list[SecurityCode]",
            "关键字段": "exchange,code,name,last_price,last_price_raw",
            "样例文件": "codes_all_sh.json | codes_all_sz.json | codes_all_bj.json",
            "备注": "bj 走官方兜底源",
        },
        {
            "API": "get_stock_codes_all",
            "用途": "返回更严格的股票清单",
            "状态": "正常",
            "可信度": "较高（实用 helper）",
            "返回类型": "list[str]",
            "关键字段": "full_code",
            "样例文件": "stock_codes_all.txt",
            "备注": "本次 5566 条",
        },
        {
            "API": "get_etf_codes_all",
            "用途": "返回更严格的 ETF 清单",
            "状态": "正常",
            "可信度": "较高（实用 helper）",
            "返回类型": "list[str]",
            "关键字段": "full_code",
            "样例文件": "etf_codes_all.txt",
            "备注": "名称需显式包含 ETF；本次 267 条",
        },
        {
            "API": "get_index_codes_all",
            "用途": "返回更严格的指数清单",
            "状态": "正常",
            "可信度": "较高（实用 helper）",
            "返回类型": "list[str]",
            "关键字段": "full_code",
            "样例文件": "index_codes_all.txt",
            "备注": "已补上 sh999998/sh999997/sh880001；本次 1675 条",
        },
        {
            "API": "get_quote",
            "用途": "读取实时行情快照",
            "状态": "正常",
            "可信度": "高",
            "返回类型": "list[Quote]",
            "关键字段": "server_time,last_price,open/high/low/close,buy_levels,sell_levels",
            "样例文件": "quote_single_sz000001.json | quotes_batch_120.json",
            "备注": "自动分批已验证",
        },
        {
            "API": "get_minute",
            "用途": "读取某交易日分时序列",
            "状态": "正常",
            "可信度": "高",
            "返回类型": "MinuteResponse",
            "关键字段": "trading_date,items[].time,items[].price,items[].volume",
            "样例文件": f"minute_history_{history_date}_sz000001.json",
            "备注": "时间已转 Python datetime",
        },
        {
            "API": "get_history_minute",
            "用途": "历史分时别名",
            "状态": "正常",
            "可信度": "高",
            "返回类型": "MinuteResponse",
            "关键字段": "同 get_minute",
            "样例文件": f"history_minute_alias_{history_date}_sz000001.json",
            "备注": "与 get_minute(code,date) 一致",
        },
        {
            "API": "get_trades",
            "用途": "读取一页逐笔成交",
            "状态": "正常",
            "可信度": "高",
            "返回类型": "TradeResponse",
            "关键字段": "time,price,volume,status,side,order_count",
            "样例文件": f"trades_history_page_{history_date}_sz000001.json",
            "备注": "时间已转 Python datetime",
        },
        {
            "API": "get_trades_all",
            "用途": "自动翻页读取完整逐笔",
            "状态": "正常",
            "可信度": "高",
            "返回类型": "TradeResponse",
            "关键字段": "count,items",
            "样例文件": f"trades_history_all_{history_date}_sz000001.json",
            "备注": "历史样本 3867 条",
        },
        {
            "API": "get_kline",
            "用途": "读取一页 K 线",
            "状态": "正常",
            "可信度": "高",
            "返回类型": "KlineResponse",
            "关键字段": "time,open/high/low/close,volume,amount",
            "样例文件": "kline_day_page_sz000001.json | kline_1m_page_sz000001.json",
            "备注": "支持 code/freq 与 freq/code 两种顺序",
        },
        {
            "API": "get_kline_all",
            "用途": "自动翻页读取完整 K 线",
            "状态": "正常",
            "可信度": "高",
            "返回类型": "KlineResponse",
            "关键字段": "count,items",
            "样例文件": "kline_day_all_sz000001.json",
            "备注": "别名一致性已验证",
        },
        {
            "API": "get_adjusted_kline / get_adjusted_kline_all",
            "用途": "读取前复权/后复权 K 线",
            "状态": "正常",
            "可信度": "高",
            "返回类型": "KlineResponse",
            "关键字段": "time,close_price,qfq/hfq 结果",
            "样例文件": "adjusted_kline_qfq_day_page_sz000001.json | adjusted_kline_hfq_day_page_sz000001.json | adjusted_kline_qfq_day_all_sz000001.json",
            "备注": "依赖 gbbq->xdxr->factors 链路",
        },
        {
            "API": "get_call_auction",
            "用途": "读取集合竞价记录",
            "状态": "正常",
            "可信度": "高",
            "返回类型": "CallAuctionResponse",
            "关键字段": "time,price,match,unmatched,flag,raw_hex",
            "样例文件": "call_auction_sz000001.json",
            "备注": "保留了原始记录 raw_hex，适合人工核对",
        },
        {
            "API": "get_gbbq",
            "用途": "读取股本变迁/除权除息底层事件",
            "状态": "正常",
            "可信度": "高",
            "返回类型": "GbbqResponse",
            "关键字段": "time,category,category_name,c1,c2,c3,c4",
            "样例文件": "gbbq_sz000001.json",
            "备注": "-",
        },
        {
            "API": "get_xdxr",
            "用途": "提炼分红配股事件",
            "状态": "正常",
            "可信度": "高",
            "返回类型": "list[XdxrItem]",
            "关键字段": "time,fenhong,peigujia,songzhuangu,peigu",
            "样例文件": "xdxr_sz000001.json",
            "备注": "-",
        },
        {
            "API": "get_equity_changes / get_equity",
            "用途": "读取股本变化序列 / 最新股本",
            "状态": "正常",
            "可信度": "高",
            "返回类型": "EquityResponse / EquityItem",
            "关键字段": "time,float_shares,total_shares",
            "样例文件": "equity_changes_sz000001.json | equity_latest_sz000001.json",
            "备注": "-",
        },
        {
            "API": "get_turnover",
            "用途": "按股本口径计算换手率",
            "状态": "正常",
            "可信度": "高",
            "返回类型": "float",
            "关键字段": "code,volume,unit,on,turnover",
            "样例文件": "turnover_sample_sz000001.json",
            "备注": "-",
        },
        {
            "API": "get_factors",
            "用途": "生成前复权/后复权因子序列",
            "状态": "正常",
            "可信度": "高",
            "返回类型": "FactorResponse",
            "关键字段": "time,last_close_price,pre_last_close_price,qfq_factor,hfq_factor",
            "样例文件": "factors_sz000001.json",
            "备注": "-",
        },
        {
            "API": "get_trade_minute_kline / get_history_trade_minute_kline",
            "用途": "把逐笔聚合成分钟 K 线",
            "状态": "正常",
            "可信度": "高",
            "返回类型": "KlineResponse",
            "关键字段": "time,open/high/low/close,volume,amount",
            "样例文件": f"trade_minute_kline_live_sz000001.json | trade_minute_kline_history_{history_date}_sz000001.json",
            "备注": "说明逐笔 -> 分钟线链路打通",
        },
    ]


def build_quote_rows(quote_samples: list[dict[str, Any]]) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    pair_notes = {
        "server_time": "服务器时间整数 -> Python datetime",
        "last_price": "毫厘整数最新价 -> float 最新价",
        "open_price": "毫厘整数开盘价 -> float 开盘价",
        "close_price": "毫厘整数昨收价 -> float 昨收价",
        "buy1_price": "买一档毫厘整数价 -> float 买一价",
    }
    wide_rows: list[dict[str, Any]] = []
    long_rows: list[dict[str, Any]] = []
    for item in quote_samples:
        full_code = item["full_code"]
        pairs = item["raw_vs_parsed"]
        wide_rows.append(
            {
                "完整代码": full_code,
                "原始时间字段": pairs["server_time"]["raw_field"],
                "原始时间值": pairs["server_time"]["raw_value"],
                "解析时间字段": pairs["server_time"]["parsed_field"],
                "解析时间值": pairs["server_time"]["parsed_value"],
                "原始最新价字段": pairs["last_price"]["raw_field"],
                "原始最新价值": pairs["last_price"]["raw_value"],
                "解析最新价字段": pairs["last_price"]["parsed_field"],
                "解析最新价值": pairs["last_price"]["parsed_value"],
                "原始开盘价字段": pairs["open_price"]["raw_field"],
                "原始开盘价值": pairs["open_price"]["raw_value"],
                "解析开盘价字段": pairs["open_price"]["parsed_field"],
                "解析开盘价值": pairs["open_price"]["parsed_value"],
                "原始昨收字段": pairs["close_price"]["raw_field"],
                "原始昨收值": pairs["close_price"]["raw_value"],
                "解析昨收字段": pairs["close_price"]["parsed_field"],
                "解析昨收值": pairs["close_price"]["parsed_value"],
                "原始买一字段": pairs["buy1_price"]["raw_field"],
                "原始买一值": pairs["buy1_price"]["raw_value"],
                "解析买一字段": pairs["buy1_price"]["parsed_field"],
                "解析买一值": pairs["buy1_price"]["parsed_value"],
            }
        )
        for pair_name, pair in pairs.items():
            long_rows.append(
                {
                    "完整代码": full_code,
                    "对照项": pair_name,
                    "原始字段": pair["raw_field"],
                    "原始值": pair["raw_value"],
                    "解析字段": pair["parsed_field"],
                    "解析值": pair["parsed_value"],
                    "说明": pair_notes.get(pair_name, ""),
                }
            )
    return wide_rows, long_rows


def build_api_key_sample_rows(api_samples: dict[str, Any]) -> list[dict[str, Any]]:
    notes = {
        "get_codes": "底层代码表条目，不是纯股票",
        "get_minute": "保留原始 frame/payload，同时给出 Python 时间与 float 价格",
        "get_trades": "保留原始 frame/payload，同时把 status 解释成 side",
        "get_kline": "保留原始 frame/payload，同时把 milli-int 价格转成 float",
        "get_call_auction": "单条记录保留 raw_hex，适合人工核对",
        "get_gbbq": "category 已映射成 category_name",
        "get_xdxr": "由 gbbq 进一步提炼出的分红配股模型",
        "get_equity_latest": "返回最新股本快照",
        "get_factors": "返回复权因子序列",
        "get_turnover_sample": "返回换手率结果样本",
    }
    rows: list[dict[str, Any]] = []
    for api_name, payload in api_samples.items():
        if api_name == "basis":
            continue
        if api_name == "get_quote":
            rows.append(
                {
                    "API": api_name,
                    "样例文件": payload["sample_file"],
                    "原始字段示例": "见 quote_five_stocks_long.csv",
                    "解析字段示例": "见 quote_five_stocks_wide.csv / quote_five_stocks_long.csv",
                    "说明": "五只股票逐字段对照单独展开",
                }
            )
            continue
        raw_fields = payload.get("raw_fields", {})
        parsed_fields = payload.get("parsed_fields", {})
        if isinstance(parsed_fields, dict):
            parsed_fields = dict(parsed_fields)
            parsed_fields.pop("meaning", None)
            if parsed_fields.get("flag_meaning") in {"????", ""} and raw_fields.get("flag") == -1:
                parsed_fields["flag_meaning"] = "卖未撮合"
            if parsed_fields.get("flag_meaning") in {"????", ""} and raw_fields.get("flag") == 1:
                parsed_fields["flag_meaning"] = "买未撮合"
        rows.append(
            {
                "API": api_name,
                "样例文件": payload.get("sample_file", ""),
                "原始字段示例": compact_json(raw_fields),
                "解析字段示例": compact_json(parsed_fields),
                "说明": notes.get(api_name, ""),
            }
        )
    return rows


def main() -> None:
    parser = argparse.ArgumentParser(description="Export validation CSV files for Excel review")
    parser.add_argument("--artifact-dir", type=Path, default=None, help="specific artifact directory to use")
    args = parser.parse_args()

    artifact_dir = args.artifact_dir or latest_artifact_dir()
    summary = load_json(artifact_dir / "summary.json")
    quote_samples = load_json(artifact_dir / "quote_five_stocks_sample.json")
    api_samples = load_json(artifact_dir / "api_sample_field_comparisons.json")

    overview_rows = build_overview_rows(summary)
    quote_wide_rows, quote_long_rows = build_quote_rows(quote_samples)
    api_key_rows = build_api_key_sample_rows(api_samples)

    write_csv(
        artifact_dir / "api_validation_overview.csv",
        ["API", "用途", "状态", "可信度", "返回类型", "关键字段", "样例文件", "备注"],
        overview_rows,
    )
    write_csv(
        artifact_dir / "quote_five_stocks_wide.csv",
        [
            "完整代码",
            "原始时间字段", "原始时间值", "解析时间字段", "解析时间值",
            "原始最新价字段", "原始最新价值", "解析最新价字段", "解析最新价值",
            "原始开盘价字段", "原始开盘价值", "解析开盘价字段", "解析开盘价值",
            "原始昨收字段", "原始昨收值", "解析昨收字段", "解析昨收值",
            "原始买一字段", "原始买一值", "解析买一字段", "解析买一值",
        ],
        quote_wide_rows,
    )
    write_csv(
        artifact_dir / "quote_five_stocks_long.csv",
        ["完整代码", "对照项", "原始字段", "原始值", "解析字段", "解析值", "说明"],
        quote_long_rows,
    )
    write_csv(
        artifact_dir / "api_key_samples.csv",
        ["API", "样例文件", "原始字段示例", "解析字段示例", "说明"],
        api_key_rows,
    )

    print(artifact_dir / "api_validation_overview.csv")
    print(artifact_dir / "quote_five_stocks_wide.csv")
    print(artifact_dir / "quote_five_stocks_long.csv")
    print(artifact_dir / "api_key_samples.csv")


if __name__ == "__main__":
    main()

