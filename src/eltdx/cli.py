"""eltdx 命令行入口。

设计参考 ``easy-tdx``：子命令分组、默认 JSON 输出、``--output table`` 可读表格。
与 easy_tdx 的区别是**不引入 click / pandas 依赖**——只用标准库 ``argparse``，
输出走后端无关的 :func:`eltdx.to_jsonable`。

安装后可用的入口点（见 ``pyproject.toml`` 的 ``[project.scripts]``）::

    eltdx quote sz000001 sh600519
    eltdx kline sz000001 --period day --count 5 --output table
    eltdx fund list --market 33 --count 10 --output table
    eltdx fund nav 016947
    eltdx macex kline 33 016947 --count 5
    eltdx ex count
    eltdx offline day --file path/to/sz000001.day --count 5

也可以不安装直接运行::

    python -m eltdx quote sz000001
"""

from __future__ import annotations

import argparse
import csv
import io
import json
import sys
from collections.abc import Sequence
from datetime import date
from typing import Any

from eltdx.f10_smoke import main as f10_smoke_main
from eltdx.serialization import to_json, to_jsonable
from eltdx.smoke import main as smoke_main

__all__ = ["main", "build_parser", "smoke", "f10_smoke"]

# ── 常量表 ──────────────────────────────────────────────────────────────

#: MAC ``Period`` 枚举接受的中文/英文周期别名。
_PERIOD_ALIASES: dict[str, str] = {
    "1m": "MIN_1", "1min": "MIN_1", "minute": "MIN_1",
    "5m": "MIN_5", "5min": "MIN_5",
    "15m": "MIN_15", "15min": "MIN_15",
    "30m": "MIN_30", "30min": "MIN_30",
    "60m": "MIN_60", "60min": "MIN_60", "1h": "MIN_60", "hour": "MIN_60",
    "day": "DAILY", "1d": "DAILY", "d": "DAILY", "daily": "DAILY",
    "week": "WEEKLY", "1w": "WEEKLY", "w": "WEEKLY",
    "month": "MONTHLY", "1mo": "MONTHLY", "mo": "MONTHLY",
    "quarter": "QUARTERLY", "1q": "QUARTERLY", "q": "QUARTERLY",
    "year": "YEARLY", "1y": "YEARLY", "y": "YEARLY",
}

#: 扩展行情里的「场外基金」类市场，供 ``eltdx fund`` 使用。
FUND_MARKETS: dict[int, str] = {
    33: "开放式基金",
    34: "货币基金",
    49: "香港基金",
    56: "阳光私募基金",
    57: "券商集合理财",
    58: "券商货币理财",
    91: "货币市场",
    93: "基金估值",
}

DEFAULT_FUND_MARKET = 33

OUTPUT_FORMATS = ("json", "jsonl", "table", "csv")


# ── 7615 F10 section 注册表 ──────────────────────────────────────────────

#: ``eltdx f10`` 各 action 对应的 ``F10Client`` 方法及其合法 ``--section`` 取值。
#: 个股 / ETF 基金 / 板块指数 / 大盘指数**共用同一批方法**，靠 ``section`` 分派，
#: 因此这里是一张扁平表，不再为不同标的类型写平行命令。
_F10_SECTIONS: dict[str, dict[str, str]] = {
    "profile": {
        "0": "基本资料",
        "3": "参控股公司",
        "4": "员工构成",
        "5": "员工效益",
        "6": "研发投入",
        "8": "发行上市信息",
        "9": "指数调入调出",
        "20": "管理层",
        "21": "高管兼职",
        "22": "（暂无数据）",
        "23": "离任高管",
    },
    "digest": {
        "tips": "最新提示",
        "profile": "公司概要",
        "events": "公司大事",
        "news": "公司新闻",
        "announcements": "公司公告",
        "reports": "公司研报",
        "roadshows": "路演互动",
        "interaction": "投资者互动",
    },
    "shareholders": {
        "count": "股东人数与股价比较",
        "ranking": "同行业股东人数增减量排名",
        "controller": "控股股东与实际控制人",
        "top_circulating": "十大流通股东",
        "top": "十大股东（按报告期）",
        "bonds": "十大债券持有人",
        "changes": "重要股东持股变动",
        "institutions": "机构持股汇总",
        "institution_types": "机构持股分类汇总",
        "institution_holdings": "机构持仓明细",
        "plan": "股东增减持计划",
    },
    "capital": {
        "structure": "股本结构",
        "changes": "股本变动",
        "buyback": "股票回购",
        "restricted": "限售流通",
        "restricted_detail": "限售解禁明细（需 --date）",
    },
    "business": {
        "business": "主营业务概述",
        "composition": "主营构成（需 --report-date）",
        "customers": "前五名客户（需 --report-date）",
        "suppliers": "前五名供应商（需 --report-date）",
        "price": "产品价格",
        "changes": "财务指标大幅变动说明（需 --report-date）",
    },
    "industry": {
        "peers": "同业对比",
        "dividend_rank": "同业分红排名",
        "scale": "同业规模",
        "valuation": "同业估值",
        "news": "行业新闻",
        "reports": "行业研报",
        "market_share": "市场份额",
        "summary": "行业汇总",
        "comparable": "可比公司",
        "comparable_profile": "可比公司资料",
    },
    "trading": {
        "margin": "近期融资融券",
        "margin_history": "近 6 个月融资融券",
        "block_trades": "大宗交易",
        "dragon_board": "龙虎榜",
        "northbound": "沪深股通",
        "limit_down": "跌停分析",
        "limit_up": "涨停分析",
        "bonds": "可转债",
        "main_force": "主力盘口",
        "reports": "研报/席位",
        "kcb": "科创板标记",
    },
    "fund": {
        "name": "名称与市场",
        "managers": "基金经理",
        "company": "基金管理人",
        "basic": "基本概况",
        "seats": "交易席位",
        "scale": "规模与份额",
        "scale_history": "份额/规模历史（需 --date）",
        "nav": "单位净值序列",
        "nav_period": "区间收益",
        "nav_rank": "同类排名",
        "dividends": "基金分红",
        "asset_allocation": "资产配置",
        "industry_allocation": "行业分布",
        "holdings_compare": "重仓股变动",
        "holdings_rank": "重仓排名",
        "holding_summary": "基金持仓汇总",
        "bond_allocation": "债券配置",
        "futures_allocation": "期货持仓明细",
        "holdings": "持股情况",
        "holdings_detail": "持股明细",
        "holdings_changes": "持有基金明细",
        "fund_of_funds": "下属基金资产",
        "finance": "财务指标（--arg 选表，默认 zcfzb）",
        "sales_info": "费率",
        "sales_announcements": "销售公告",
        "sales_news": "相关资讯",
        "etf": "是否 ETF",
        "margin": "ETF 融资融券",
        "ratings": "基金评级",
        "categories": "分类/同类对比",
    },
    "market": {
        "board_info": "板块名称与市场",
        "board_basics": "板块基础资料",
        "board_description": "板块说明",
        "board_trading": "板块交易数据",
        "board_flow": "板块资金（--arg 001_1/002_1/003_1）",
        "board_members": "成份股数据（--arg 001/002/003/004/005）",
        "board_analysis": "板块三点分析",
        "board_finance_compare": "成份股财务对比",
        "board_candidates": "成份股候选数据（--arg 001/002）",
        "board_trend": "板块异动分析",
        "index_info": "指数信息",
        "index_profile": "指数概况",
        "index_tracking": "跟踪标的",
        "index_flow": "指数资金动向（--arg 003/004_1/004_2）",
        "index_flow_members": "成份股资金",
        "index_notices": "今日提示",
        "market_basic": "大盘指数基本信息",
        "market_margin": "大盘两融",
        "market_new_shares": "新股分析",
        "market_capital": "大盘市场概览（--arg 001/002/003/004）",
    },
    "forecast": {
        "ylyctj": "盈利预测统计",
        "ylycmx": "盈利预测明细",
        "yzyq": "一致预期",
        "tzpjtj": "投资评级统计",
        "ycpjyjbg": "研报明细",
    },
    "dividend": {
        "fh": "分红方案历史",
        "pf": "配股",
        "pxmz": "募资汇总",
        "zf": "增发",
        "gqjl": "股权激励",
        "fhpm_gxl": "股息率排名",
        "fhlszs_gxl": "股息率历史走势",
    },
    "finance": {
        "zcfzb": "资产负债表",
        "lyb": "利润表",
        "xjllb": "现金流量表",
        "zyzb": "主要指标",
        "dbfx": "杜邦分析",
        "cwgc": "财务概况",
        "zcdjt": "资产大计提",
        "ylnl": "盈利能力",
        "cwzd": "财务诊断",
        "cwbg": "财报披露",
    },
    "governance": {
        "wgcl": "违规处理",
        "dbmxbgq": "担保明细",
        "gljy": "关联交易",
        "gqzr": "股权转让",
        "gqzy": "股权质押",
        "sgjb": "诉讼仲裁",
        "zdsx": "重大事项",
        "zjly": "资金运用",
    },
    "score": {"pf": "综合评分"},
    "topics": {
        "zttzbkz": "板块题材",
        "zttzztk": "自选题材",
        "sjcd": "事件驱动",
        "xxmmg": "细项明细",
    },
    "news": {
        "gsyj": "公司研报",
        "jgcs": "监管措施",
        "finance_review": "财报点评",
    },
    "northbound": {"bszj": "北向资金", "sdzj": "南向资金"},
    "ranking": {"scpmdela": "市场排名", "hypmdela": "行业排名"},
}

#: ``eltdx f10`` 的 action 列表（选择顺序即帮助里的展示顺序）。
_F10_ACTIONS: tuple[str, ...] = (
    "sections",
    "call",
    "stock-info",
    "profile",
    "digest",
    "shareholders",
    "capital",
    "business",
    "industry",
    "trading",
    "dividend",
    "finance",
    "governance",
    "forecast",
    "score",
    "topics",
    "topic-compare",
    "news",
    "northbound",
    "ranking",
    "valuation",
    "limit-board",
    "fund",
    "market",
    "detail",
)

#: ``eltdx f10 sections`` 用的默认 section（与 ``F10Client`` 的默认参数一致）。
_F10_DEFAULT_SECTION: dict[str, str] = {
    "profile": "8",
    "digest": "tips",
    "shareholders": "count",
    "capital": "structure",
    "business": "business",
    "industry": "peers",
    "trading": "margin",
    "dividend": "fh",
    "finance": "zcfzb",
    "governance": "wgcl",
    "forecast": "ylyctj",
    "score": "pf",
    "topics": "zttzbkz",
    "news": "gsyj",
    "northbound": "bszj",
    "ranking": "scpmdela",
    "fund": "basic",
    "market": "board_info",
}

# ── 输出 ────────────────────────────────────────────────────────────────


def _rows(payload: Any) -> list[dict[str, Any]]:
    """把任意 payload 规整成「一行一个 dict」，供 table / csv 渲染。"""

    if isinstance(payload, dict):
        if payload and all(isinstance(v, dict) for v in payload.values()):
            merged: list[dict[str, Any]] = []
            for key, value in payload.items():
                row: dict[str, Any] = {"code": key}
                row.update(value)
                merged.append(row)
            return merged
        return [payload]
    if isinstance(payload, list):
        return [row if isinstance(row, dict) else {"value": row} for row in payload]
    return [{"value": payload}]


def _columns(rows: Sequence[dict[str, Any]]) -> list[str]:
    ordered: list[str] = []
    for row in rows:
        for key in row:
            if key not in ordered:
                ordered.append(key)
    return ordered


def _cell(value: Any, limit: int | None = 30) -> str:
    if value is None:
        text = ""
    elif isinstance(value, bool):
        text = "true" if value else "false"
    elif isinstance(value, float):
        text = f"{value:.4f}".rstrip("0").rstrip(".")
    elif isinstance(value, (list, tuple, dict)):
        text = json.dumps(to_jsonable(value), ensure_ascii=False, separators=(",", ":"))
    else:
        text = str(value)
    text = text.replace("\n", " ")
    if limit is not None and len(text) > limit:
        text = text[: limit - 1] + "…"
    return text


def _render_table(rows: Sequence[dict[str, Any]]) -> str:
    """标准库表格渲染（不依赖 tabulate）。"""

    if not rows:
        return "(无数据)"
    columns = _columns(rows)
    cells = [[_cell(row.get(col)) for col in columns] for row in rows]
    widths = [
        max(len(columns[i]), *(len(r[i]) for r in cells)) if cells else len(columns[i])
        for i in range(len(columns))
    ]
    sep = "+" + "+".join("-" * (w + 2) for w in widths) + "+"
    lines = [sep, "| " + " | ".join(columns[i].ljust(widths[i]) for i in range(len(columns))) + " |", sep]
    for row in cells:
        lines.append("| " + " | ".join(row[i].ljust(widths[i]) for i in range(len(columns))) + " |")
    lines.append(sep)
    lines.append(f"({len(rows)} 行)")
    return "\n".join(lines)


def emit(args: argparse.Namespace, payload: Any) -> int:
    """按 ``--output`` 输出结果，返回进程退出码。"""

    fmt = args.output
    if fmt != "json":
        # 7615 F10 回包是「Entry + 多个结果集」的信封；表格/CSV/JSONL 只关心数据行。
        from eltdx.f10 import F10Response

        if isinstance(payload, F10Response):
            payload = [row for result_set in payload.result_sets for row in result_set.rows]
    if fmt == "json":
        sys.stdout.write(to_json(payload, ensure_ascii=False, indent=args.indent) + "\n")
        return 0
    if fmt == "jsonl":
        for row in _rows(to_jsonable(payload)):
            sys.stdout.write(json.dumps(row, ensure_ascii=False) + "\n")
        return 0
    rows = _rows(to_jsonable(payload))
    if fmt == "table":
        sys.stdout.write(_render_table(rows) + "\n")
        return 0
    columns = _columns(rows)
    buffer = io.StringIO()
    writer = csv.DictWriter(buffer, fieldnames=columns, extrasaction="ignore")
    writer.writeheader()
    for row in rows:
        writer.writerow({k: _cell(v, limit=None) for k, v in row.items()})
    sys.stdout.write(buffer.getvalue())
    return 0


def fail(message: str) -> int:
    """向 stderr 输出错误并返回非零退出码。"""

    sys.stderr.write(f"错误: {message}\n")
    return 1


# ── 输入解析 ────────────────────────────────────────────────────────────


def _codes(values: Sequence[str] | str | None) -> list[str]:
    """展开 ``sz000001`` / ``"SZ 000001"`` / ``000001,600519`` 等写法。

    ``nargs="?"`` 的位置参数拿到的是**裸字符串**（如 ``eltdx kline sh000016``），
    必须当作单个代码，不能按字符展开；``None``（未传位置参数）按空列表处理。
    """
    if values is None:
        return []
    items: Sequence[str] = [values] if isinstance(values, str) else values
    out: list[str] = []
    for value in items:
        for chunk in str(value).replace(",", " ").split():
            chunk = chunk.strip()
            if chunk:
                out.append(chunk)
    return out


def _single_code(values: Sequence[str] | str | None) -> str:
    codes = _codes(values)
    if len(codes) != 1:
        raise ValueError(f"该命令只接受一个代码，收到 {len(codes)} 个: {codes}")
    return codes[0]


def _period(value: str) -> Any:
    """把周期字符串转成 MAC ``Period`` 枚举成员。"""

    from eltdx.mac import Period

    key = value.strip().lower()
    name = _PERIOD_ALIASES.get(key)
    if name is None:
        raise ValueError(
            f"不支持的周期 {value!r}；可用: {', '.join(sorted(_PERIOD_ALIASES))}"
        )
    return getattr(Period, name)


def _adjust(value: str) -> Any:
    """把复权字符串转成 MAC ``Adjust`` 枚举成员。"""

    from eltdx.mac import Adjust

    key = (value or "none").strip().lower()
    mapping = {"none": "NONE", "qfq": "QFQ", "hfq": "HFQ", "bfq": "NONE", "": "NONE"}
    name = mapping.get(key)
    if name is None:
        raise ValueError(f"不支持的复权方式 {value!r}；可用: none, qfq, hfq")
    return getattr(Adjust, name)


def _trade_date(value: str | None) -> date | None:
    if not value:
        return None
    text = value.strip()
    if len(text) == 8 and text.isdigit():
        return date(int(text[:4]), int(text[4:6]), int(text[6:8]))
    if len(text) == 10 and text[4] == "-":
        year, month, day = text.split("-")
        return date(int(year), int(month), int(day))
    raise ValueError(f"日期格式应为 YYYYMMDD 或 YYYY-MM-DD，收到 {value!r}")


def _int_date(value: str | None) -> int | None:
    parsed = _trade_date(value)
    if parsed is None:
        return None
    return parsed.year * 10000 + parsed.month * 100 + parsed.day


#: MAC 协议的市场编号（与 ``eltdx.mac.price_rules`` 一致：0=深 1=沪 2=北）。
_MAC_MARKETS: dict[str, int] = {"SZ": 0, "SH": 1, "BJ": 2, "0": 0, "1": 1, "2": 2}


def _mac_market(value: str) -> int:
    """解析市场：``SZ``/``SH``/``BJ``/``0``/``1``/``2``/数字。"""

    key = str(value).strip().upper()
    if key in _MAC_MARKETS:
        return _MAC_MARKETS[key]
    return int(key)


def _infer_mac_market(code: str) -> int:
    """按代码推断 MAC 市场编号（复用 eltdx 自身的代码规范化）。"""

    from eltdx.protocol.unit import normalize_code

    return _MAC_MARKETS[normalize_code(code)[:2].upper()]


def _mac_stocks(values: Sequence[str]) -> list[tuple[int, str]]:
    """解析 ``"SZ 000001,SH 600000"`` / ``sz000001`` / ``000001`` 为 ``[(market, code)]``。

    参考 easy_tdx CLI 的 ``parse_stocks``，另外支持 ``sz000001`` 前缀写法与
    无前缀代码（按首位推断市场）。
    """

    tokens: list[str] = []
    for value in values:
        tokens.extend(t for t in str(value).replace(",", " ").split() if t)

    out: list[tuple[int, str]] = []
    index = 0
    while index < len(tokens):
        token = tokens[index]
        upper = token.upper()
        if upper in _MAC_MARKETS and index + 1 < len(tokens):
            out.append((_MAC_MARKETS[upper], tokens[index + 1]))
            index += 2
            continue
        if len(token) > 2 and token[:2].lower() in {"sz", "sh", "bj"}:
            out.append((_MAC_MARKETS[token[:2].upper()], token[2:]))
        else:
            out.append((_infer_mac_market(token), token))
        index += 1
    return out


# ── 客户端工厂（延迟导入，保证 --help 无需 native 扩展）─────────────────


def _tdx(args: argparse.Namespace) -> Any:
    from eltdx import TdxClient

    hosts = [args.host] if getattr(args, "host", None) else None
    return TdxClient.from_hosts(hosts=hosts, timeout=args.timeout)


def _ex(args: argparse.Namespace) -> Any:
    """扩展行情客户端。

    默认只用 :data:`eltdx.ex.MAC_EX_HOSTS`——实测那 16 台免费 ``KNOWN_EX_HOSTS``
    只稳定服务元数据（且 quote/bar 常被 RST 或超时），混在一起做延迟选主会
    选到不可用主机。需要广撒网时加 ``--all-hosts``。
    """

    from eltdx.ex import ExTdxClient
    from eltdx.ex.hosts import KNOWN_EX_HOSTS, MAC_EX_HOSTS

    if getattr(args, "host", None):
        hosts = [args.host]
    elif getattr(args, "all_hosts", False):
        hosts = list(MAC_EX_HOSTS) + [h for h in KNOWN_EX_HOSTS if h not in MAC_EX_HOSTS]
    else:
        hosts = list(MAC_EX_HOSTS)
    return ExTdxClient.from_best_host(hosts=hosts, port=args.port, timeout=args.timeout)


def _mac(args: argparse.Namespace) -> Any:
    from eltdx.mac import MacClient

    hosts = [args.host] if getattr(args, "host", None) else None
    return MacClient(host=hosts[0] if hosts else None, port=args.port, timeout=args.timeout)


def _mac_ex(args: argparse.Namespace) -> Any:
    # 从具体模块导入（而非 eltdx.ex 门面）以获得完整的类型信息。
    from eltdx.ex.mac_client import MacExClient

    hosts = [args.host] if getattr(args, "host", None) else None
    return MacExClient.from_best_host(hosts=hosts, port=args.port, timeout=args.timeout)


# ── 命令实现 ────────────────────────────────────────────────────────────


def cmd_version(args: argparse.Namespace) -> int:
    import eltdx

    return emit(args, {"version": eltdx.__version__, "python": sys.version.split()[0]})


def cmd_ping(args: argparse.Namespace) -> int:
    client = _tdx(args)
    try:
        return emit(args, {"host": client.ping()})
    finally:
        client.close()


def cmd_workday(args: argparse.Namespace) -> int:
    client = _tdx(args)
    try:
        target = _trade_date(args.date)
        payload: dict[str, Any] = {}
        if target is None:
            payload["today"] = client.workdays.today()
            payload["is_workday"] = client.workdays.is_workday()
        else:
            payload["date"] = target
            payload["is_workday"] = client.workdays.is_workday(target)
        return emit(args, payload)
    finally:
        client.close()


def cmd_quote(args: argparse.Namespace) -> int:
    client = _tdx(args)
    try:
        codes = _codes(args.code)
        result = client.quotes.get_depth(codes) if args.depth else client.quotes.get_snapshots(codes)
        return emit(args, result)
    finally:
        client.close()


def cmd_kline(args: argparse.Namespace) -> int:
    client = _tdx(args)
    try:
        result = client.bars.get(
            _single_code(args.code),
            period=args.period,
            start=args.start,
            count=args.count,
            adjust=args.adjust,
            include_raw=args.raw,
        )
        return emit(args, result)
    finally:
        client.close()


def cmd_minute(args: argparse.Namespace) -> int:
    client = _tdx(args)
    try:
        codes = _codes(args.code)
        target = _trade_date(args.date)
        result = (
            client.minutes.history(codes, target) if target else client.minutes.today(codes)
        )
        return emit(args, result)
    finally:
        client.close()


def cmd_trades(args: argparse.Namespace) -> int:
    client = _tdx(args)
    try:
        codes = _codes(args.code)
        target = _trade_date(args.date)
        if args.all:
            result = client.trades.all_today(codes, include_raw=args.raw)
        elif target:
            result = client.trades.history(
                codes, target, start=args.start, count=args.count, include_raw=args.raw
            )
        else:
            result = client.trades.today(
                codes, start=args.start, count=args.count, include_raw=args.raw
            )
        return emit(args, result)
    finally:
        client.close()


def cmd_codes(args: argparse.Namespace) -> int:
    client = _tdx(args)
    try:
        market = args.market.strip().lower()
        if args.kind == "list":
            result = client.codes.list(market, start=args.start, limit=args.limit)
        elif args.kind == "all":
            result = client.codes.all(market, page_size=args.limit)
        elif args.kind == "stocks":
            result = client.codes.stocks(market)
        elif args.kind == "etfs":
            result = client.codes.etfs(market)
        elif args.kind == "indices":
            result = client.codes.indices(market)
        elif args.kind == "count":
            result = client.codes.count(market)
        elif args.kind == "markets":
            result = client.codes.all_markets()
        else:  # pragma: no cover - argparse 已限制取值
            raise ValueError(f"未知的 kind: {args.kind}")
        return emit(args, result)
    finally:
        client.close()


def cmd_finance(args: argparse.Namespace) -> int:
    client = _tdx(args)
    try:
        result = client.corporate.finance_batch(
            _codes(args.code), include_raw=args.raw
        )
        return emit(args, result)
    finally:
        client.close()


def cmd_money_flow(args: argparse.Namespace) -> int:
    """资金流向。

    默认走 7709 ``0x0ffc``（当日 16 分档）；给了 ``--days`` 则走 7615
    ``CWServ.tdxf10_gg_sj`` 的区间通道，可取 3/5/10/20/60 日的主力净额序列。
    """

    days = getattr(args, "days", None)
    client = _tdx(args)
    try:
        codes = _codes(args.code)
        if days is None:
            result = client.money_flow.daily(codes, include_raw=args.raw)
        else:
            result = client.money_flow.history(
                codes[0] if len(codes) == 1 else codes,
                days=days,
                include_raw=args.raw,
            )
        return emit(args, result)
    finally:
        client.close()


def _f10_client(args: argparse.Namespace) -> Any:
    """构造 7615 F10 HTTP 客户端（F10 不依赖 7709 连接）。"""

    from eltdx.f10 import DEFAULT_TQLEX_BASE_URL, F10Client

    return F10Client(
        base_url=getattr(args, "base_url", None) or DEFAULT_TQLEX_BASE_URL,
        timeout=args.timeout,
    )


def _f10_single_code(args: argparse.Namespace) -> str:
    """F10 单只标的：只接受一个代码。"""

    return _single_code(args.code)


def cmd_f10(args: argparse.Namespace) -> int:
    """7615 F10 / TQLEX 资料接口。

    个股、ETF 基金、板块、指数、大盘指数**共用同一批方法**，由 ``--section`` 分派；
    ``sections`` 会列出全部合法取值（含默认值）。
    """

    action = args.action
    if action == "sections":
        payload = [
            {
                "action": name,
                "section": key,
                "default": _F10_DEFAULT_SECTION.get(name) == key,
                "说明": text,
            }
            for name, sections in _F10_SECTIONS.items()
            for key, text in sections.items()
        ]
        return emit(args, payload)

    raw_args = json.loads(args.params) if args.params else []
    section = args.section or _F10_DEFAULT_SECTION.get(action, "")
    page = args.page
    page_size = args.page_size
    arg = args.arg or ""

    client = _f10_client(args)
    if action == "call":
        if not args.entry:
            return fail("f10 call 需要 --entry（如 CWServ.tdxf10_gg_gdyj）")
        return emit(args, client.call(args.entry, params=raw_args))
    if action == "detail":
        codes = _codes(args.code)
        if len(codes) != 2:
            return fail("f10 detail 需要两个位置参数：detail_type 与 record_id，如 `f10 detail xw 22075813`")
        return emit(args, client.detail(codes[0], codes[1]))
    if action == "limit-board":
        return emit(args, client.limit_up_down_list(args.date or None))

    code = _f10_single_code(args)
    if action == "stock-info":
        return emit(args, client.stock_info(code))
    if action == "profile":
        return emit(args, client.company_profile(code, section=section))
    if action == "digest":
        return emit(args, client.digest(code, section=section, page_size=page_size))
    if action == "shareholders":
        return emit(
            args,
            client.shareholders(
                code,
                section=section,
                report_date=args.report_date,
                kind=arg or "000",
                page=page,
                page_size=page_size,
            ),
        )
    if action == "capital":
        return emit(args, client.capital_structure(code, section=section, date=args.date))
    if action == "business":
        return emit(
            args, client.business_report(code, section=section, report_date=args.report_date)
        )
    if action == "industry":
        return emit(args, client.industry_analysis(code, section=section, arg=arg))
    if action == "trading":
        return emit(
            args,
            client.trading_report(
                code, section=section, arg=arg, date=args.date, page=page, page_size=page_size
            ),
        )
    if action == "dividend":
        return emit(args, client.dividend_financing(code, section=section))
    if action == "finance":
        return emit(args, client.finance_report(code, report_type=section))
    if action == "governance":
        return emit(args, client.governance(code, section=section, arg=arg))
    if action == "forecast":
        return emit(args, client.profit_forecast(code, section=section))
    if action == "score":
        return emit(args, client.stock_score(code, section=section, arg=arg))
    if action == "topics":
        return emit(args, client.hot_topics(code, section=section))
    if action == "topic-compare":
        if args.topic:
            return emit(args, client.topic_compare(code, args.topic))
        return emit(args, client.topic_compare_first(code))
    if action == "news":
        return emit(
            args,
            client.company_news(code, section=section, page=page, page_size=page_size),
        )
    if action == "northbound":
        return emit(
            args,
            client.northbound_holding(code, section=section, page=page, page_size=page_size),
        )
    if action == "ranking":
        return emit(args, client.ranking_detail(code, section=section))
    if action == "valuation":
        return emit(args, client.valuation(code))
    if action == "fund":
        return emit(
            args,
            client.fund_report(
                code,
                section=section,
                arg=arg,
                date=args.date,
                page=page,
                page_size=page_size,
            ),
        )
    if action == "market":
        return emit(
            args,
            client.market_report(
                code,
                section=section,
                arg=arg,
                date=args.date,
                page=page,
                page_size=page_size,
            ),
        )
    return fail(f"未知的 f10 action: {action}")

def cmd_limits(args: argparse.Namespace) -> int:
    client = _tdx(args)
    try:
        target = _trade_date(args.date)
        if target is None:
            target = client.workdays.today()
        result = client.helpers.daily_price_limits(_codes(args.code) or None, trade_date=target)
        return emit(args, result)
    finally:
        client.close()


def cmd_ex(args: argparse.Namespace) -> int:
    if args.action == "markets":
        return _ex_markets(args)
    if args.action == "count":
        if args.market is None:
            client = _ex(args)
            try:
                return emit(args, {"count": client.get_instrument_count()})
            finally:
                client.close()
        # 按市场计数走 MAC_EX（0x2562 不支持时库内会退回 EX 元数据二分计数）
        client = _mac_ex(args)
        try:
            return emit(args, {"market": args.market, "count": client.goods_count(args.market)})
        finally:
            client.close()

    client = _ex(args)
    try:
        if args.action == "info":
            return emit(args, client.get_instrument_info(args.start, args.count))
        if args.action == "quote":
            return emit(args, client.get_instrument_quote(_require(args.market), _single_code(args.code)))
        if args.action == "quote-list":
            return emit(
                args,
                client.get_instrument_quote_list(
                    _require(args.market), _require(args.category), args.start, args.count
                ),
            )
        if args.action == "bars":
            return emit(
                args,
                client.get_instrument_bars(
                    _require(args.category), _require(args.market), _single_code(args.code),
                    args.start, args.count,
                ),
            )
        if args.action == "minute":
            return emit(args, client.get_minute_time_data(_require(args.market), _single_code(args.code)))
        if args.action == "txn":
            return emit(
                args,
                client.get_transaction_data(
                    _require(args.market), _single_code(args.code), args.start, args.count
                ),
            )
        raise ValueError(f"未知的 ex 子命令: {args.action}")  # pragma: no cover
    finally:
        client.close()


def _ex_markets(args: argparse.Namespace) -> int:
    """扩展市场表。

    默认**直接**使用本地 :class:`ExMarket` 枚举，不向服务器索取：实测（含与
    easy_tdx 1.20.7 的差分对照）所有内置主站都不回应 ``get_markets``（0x23f4），
    请求帧逐字节相同、easy_tdx 同样超时，服务器就是不回这条命令。而每台主机都要
    等满一个连接超时（默认组合下 2 台 = 约 30 秒）才回退，而枚举本身就是完整且
    权威的市场表——所以「先问服务器」在这里只会白白拖慢，不是更可靠。

    需要时可用 ``--probe-server`` 强制先问服务器，失败再回退。
    """

    rows: list[dict[str, Any]] | None = None
    note = ""

    if getattr(args, "probe_server", False):
        try:
            client = _ex(args)
            try:
                markets = client.get_markets()
            finally:
                client.close()
            if markets:
                rows = [
                    {
                        "market": m.market,
                        "category": m.category,
                        "name": m.name,
                        "short_name": m.short_name,
                    }
                    for m in markets
                ]
            else:
                note = "提示: 服务器返回空市场表，已回退到本地 ExMarket 枚举。\n"
        except Exception as exc:  # noqa: BLE001 - 回退是设计行为
            note = f"提示: 服务器未返回市场表（{type(exc).__name__}），已回退到本地 ExMarket 枚举。\n"

    if rows is None:
        from eltdx.ex.enums import ExMarket
        from eltdx.ex.models import KNOWN_EX_MARKETS

        rows = [
            {
                "market": member.value,
                "name": member.name,
                "alias": KNOWN_EX_MARKETS.get(member.value, ""),
            }
            for member in ExMarket
        ]
    if note:
        sys.stderr.write(note)
    return emit(args, rows)


def _require(value: Any) -> Any:
    if value is None:
        raise ValueError("缺少必需参数（见 --help）")
    return value


def cmd_mac(args: argparse.Namespace) -> int:
    client = _mac(args)
    try:
        if args.action == "quote":
            stocks = _mac_stocks(args.code)
            if args.market is not None:
                stocks = [(args.market, code) for _m, code in stocks]
            return emit(args, client.get_stock_quotes(stocks))
        if args.action == "kline":
            market = args.market if args.market is not None else _mac_stocks(args.code)[0][0]
            return emit(
                args,
                client.get_stock_kline(
                    market, _single_code(args.code), _period(args.period),
                    args.start, args.count, adjust=_adjust(args.adjust),
                ),
            )
        if args.action == "ranking":
            return emit(
                args,
                client.get_stock_quotes_list(
                    _require(args.category), args.start, args.count
                ),
            )
        if args.action == "board-list":
            return emit(args, client.get_board_list(count=args.count))
        if args.action == "server-info":
            return emit(args, client.get_server_info())
        if args.action == "symbol-info":
            market = args.market if args.market is not None else _mac_stocks(args.code)[0][0]
            return emit(args, client.get_symbol_info(market, _single_code(args.code)))
        if args.action == "minute":
            market = args.market if args.market is not None else _mac_stocks(args.code)[0][0]
            return emit(
                args,
                client.get_stock_minute_time(market, _single_code(args.code), _int_date(args.date)),
            )
        if args.action == "txn":
            market = args.market if args.market is not None else _mac_stocks(args.code)[0][0]
            return emit(
                args,
                client.get_stock_transaction(
                    market, _single_code(args.code), args.count, args.start, _int_date(args.date)
                ),
            )
        if args.action == "auction":
            market = args.market if args.market is not None else _mac_stocks(args.code)[0][0]
            return emit(args, client.get_auction(market, _single_code(args.code)))
        if args.action == "unusual":
            return emit(args, client.get_unusual(args.start, args.count))
        if args.action == "goods-list":
            return emit(
                args, client.get_goods_list(_require(args.market), args.start, args.count)
            )
        raise ValueError(f"未知的 mac 子命令: {args.action}")  # pragma: no cover
    finally:
        client.close()


def cmd_macex(args: argparse.Namespace) -> int:
    client = _mac_ex(args)
    try:
        market = args.market
        if args.action == "count":
            if market is None:
                return emit(args, {"count": client.goods_count()})
            return emit(args, {"market": market, "count": client.goods_count(market)})
        market = _require(market)
        if args.action == "instruments":
            return emit(args, client.instruments(market, args.start, args.count))
        if args.action == "list":
            return emit(args, client.goods_list(market, args.start, args.count))
        if args.action == "quote":
            return emit(args, client.goods_quotes([(market, c) for c in _codes(args.code)]))
        if args.action == "quote-list":
            return emit(args, client.goods_quotes_list(market, args.start, args.count))
        if args.action == "kline":
            return emit(
                args,
                client.goods_kline(
                    market, _single_code(args.code), _period(args.period),
                    args.start, args.count, _adjust(args.adjust),
                ),
            )
        if args.action == "minute":
            return emit(
                args,
                client.goods_tick_chart(market, _single_code(args.code), _trade_date(args.date)),
            )
        if args.action == "txn":
            return emit(
                args,
                client.goods_transaction(
                    market, _single_code(args.code), _trade_date(args.date), args.start, args.count
                ),
            )
        raise ValueError(f"未知的 macex 子命令: {args.action}")  # pragma: no cover
    finally:
        client.close()


def cmd_fund(args: argparse.Namespace) -> int:
    """场外基金（开放式基金 / 货币基金）净值查询。

    这些品种不在 7709 交易所行情里，只能走扩展行情（7727）的 MAC 协议。
    """

    client = _mac_ex(args)
    try:
        market = args.market
        if args.action == "markets":
            return emit(
                args,
                [{"market": m, "name": n} for m, n in sorted(FUND_MARKETS.items())],
            )
        if args.action == "list":
            return emit(args, client.instruments(market, args.start, args.count))
        if args.action == "nav":
            codes = _codes(args.code)
            if not codes:
                raise ValueError("请提供至少一个基金代码，例如: eltdx fund nav 016947")
            return emit(args, client.goods_quotes([(market, c) for c in codes]))
        if args.action == "kline":
            return emit(
                args,
                client.goods_kline(
                    market, _single_code(args.code), _period(args.period), args.start, args.count
                ),
            )
        if args.action == "info":
            return emit(
                args, client.goods_quotes([(market, c) for c in _codes(args.code)])
            )
        raise ValueError(f"未知的 fund 子命令: {args.action}")  # pragma: no cover
    finally:
        client.close()


def cmd_offline(args: argparse.Namespace) -> int:
    from pathlib import Path

    from eltdx import offline

    def target_path() -> Path:
        if args.file:
            return Path(args.file)
        if args.code is None or args.market is None:
            raise ValueError("需要 --file，或同时提供 --market 与 --code")
        market = int(args.market)
        code = _single_code([args.code])
        finder = {
            "day": offline.find_daily_bar_file,
            "min": offline.find_lc1_bar_file,
            "5min": offline.find_5min_bar_file,
        }[args.action]
        return Path(finder(market, code, args.vipdoc))

    try:
        bars: list[Any]
        if args.action == "day":
            bars = offline.read_daily_bar(target_path())
        elif args.action == "min":
            bars = offline.read_min_bar(target_path())
        elif args.action == "5min":
            bars = offline.read_5min_bar(target_path())
        elif args.action == "ex-day":
            bars = offline.read_ex_daily_bar(target_path())
        elif args.action == "block":
            return emit(args, offline.read_block(target_path()))
        elif args.action == "industry":
            return emit(args, offline.read_industry_cfg(target_path()))
        elif args.action == "gbbq":
            return emit(args, offline.read_gbbq(target_path()))
        elif args.action == "financial":
            return emit(args, offline.read_history_financial(target_path()))
        else:  # pragma: no cover
            raise ValueError(f"未知的 offline 子命令: {args.action}")
    except FileNotFoundError as exc:
        return fail(f"文件不存在: {exc}")

    if args.last:
        result: Any = bars[-1] if bars else None
    elif args.count:
        result = bars[-args.count :]
    else:
        result = bars
    return emit(args, result)


# ── 参数解析 ────────────────────────────────────────────────────────────


def _add_output_options(parser: argparse.ArgumentParser) -> None:
    parser.add_argument(
        "-o", "--output", choices=OUTPUT_FORMATS, default="json",
        help="输出格式（默认 json）",
    )
    parser.add_argument("--indent", type=int, default=None, help="json 缩进空格数")


def _add_connection_options(
    parser: argparse.ArgumentParser, *, port: int | None = None, host: bool = True
) -> None:
    if host:
        parser.add_argument("--host", default=None, help="指定服务器地址")
    if port is not None:
        parser.add_argument("--port", type=int, default=port, help=f"端口（默认 {port}）")
    parser.add_argument("--timeout", type=float, default=15.0, help="连接/读取超时秒数")


def _add_code_positional(parser: argparse.ArgumentParser, *, nargs: str = "+", name: str = "code") -> None:
    parser.add_argument(
        name, nargs=nargs,
        help="证券代码，如 sz000001 / 000001 / 'SH 600519'（逗号或空格分隔多个）",
    )


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="eltdx",
        description="eltdx 命令行 —— 通达信 7709 行情 / 7615 F10 / 7727 扩展行情 / 本地行情文件。",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=(
            "示例:\n"
            "  eltdx quote sz000001 sh600519\n"
            "  eltdx kline sz000001 --period day --count 5 --output table\n"
            "  eltdx fund list --market 33 --count 10 --output table\n"
            "  eltdx fund nav 016947\n"
            "  eltdx macex kline 33 016947 --count 5\n"
            "  eltdx offline day --file sz000001.day --count 5\n"
            "\n场外基金（016947 等）不在 7709 行情里，请用 `eltdx fund`。\n"
        ),
    )
    parser.add_argument("--version", action="version", version=_version_text())
    sub = parser.add_subparsers(dest="command", metavar="<命令>")

    # version
    p = sub.add_parser("version", help="显示版本")
    _add_output_options(p)
    p.set_defaults(func=cmd_version)

    # ping
    p = sub.add_parser("ping", help="探测 7709 服务器")
    _add_output_options(p)
    _add_connection_options(p)
    p.set_defaults(func=cmd_ping)

    # workday
    p = sub.add_parser("workday", help="交易日判断")
    _add_output_options(p)
    _add_connection_options(p)
    p.add_argument("--date", default=None, help="日期 YYYYMMDD（默认今天）")
    p.set_defaults(func=cmd_workday)

    # quote
    p = sub.add_parser("quote", help="实时快照 / 五档盘口")
    _add_output_options(p)
    _add_connection_options(p)
    _add_code_positional(p)
    p.add_argument("--depth", action="store_true", help="返回五档盘口（默认快照）")
    p.set_defaults(func=cmd_quote)

    # kline
    p = sub.add_parser("kline", help="K 线（7709）")
    _add_output_options(p)
    _add_connection_options(p)
    _add_code_positional(p, nargs="?")
    p.add_argument("--period", default="day", help="周期: 1m/5m/15m/30m/60m/day/week/month/quarter/year")
    p.add_argument("--adjust", default=None, help="复权: qfq/hfq/none")
    p.add_argument("--start", type=int, default=0, help="起始偏移")
    p.add_argument("--count", type=int, default=100, help="条数")
    p.add_argument("--raw", action="store_true", help="附带原始字节")
    p.set_defaults(func=cmd_kline)

    # minute
    p = sub.add_parser("minute", help="分时数据（7709）")
    _add_output_options(p)
    _add_connection_options(p)
    _add_code_positional(p)
    p.add_argument("--date", default=None, help="历史日期 YYYYMMDD（默认当日）")
    p.set_defaults(func=cmd_minute)

    # trades
    p = sub.add_parser("trades", help="逐笔成交（7709）")
    _add_output_options(p)
    _add_connection_options(p)
    _add_code_positional(p)
    p.add_argument("--date", default=None, help="历史日期 YYYYMMDD（默认当日）")
    p.add_argument("--start", type=int, default=0)
    p.add_argument("--count", type=int, default=100)
    p.add_argument("--all", action="store_true", help="取当日全部（自动翻页）")
    p.add_argument("--raw", action="store_true", help="附带原始字节")
    p.set_defaults(func=cmd_trades)

    # codes
    p = sub.add_parser("codes", help="证券代码表（0x044d）")
    _add_output_options(p)
    _add_connection_options(p)
    p.add_argument("--market", default="sz", help="市场: sh/sz/bj")
    p.add_argument(
        "--kind", default="stocks",
        choices=("stocks", "etfs", "indices", "all", "list", "count", "markets"),
        help="返回内容（默认 stocks）",
    )
    p.add_argument("--start", type=int, default=0)
    p.add_argument("--limit", type=int, default=100)
    p.set_defaults(func=cmd_codes)

    # finance
    p = sub.add_parser("finance", help="财务数据（专业财报）")
    _add_output_options(p)
    _add_connection_options(p)
    _add_code_positional(p)
    p.add_argument("--raw", action="store_true")
    p.set_defaults(func=cmd_finance)

    # money-flow
    p = sub.add_parser("money-flow", help="资金流向（默认 0x0ffc 当日；--days 走 7615 区间）")
    _add_output_options(p)
    _add_connection_options(p)
    _add_code_positional(p)
    p.add_argument(
        "--days", type=int, default=None, choices=(3, 5, 10, 20, 60),
        help="区间天数（3/5/10/20/60，走 7615 CWServ.tdxf10_gg_sj；不传则走 0x0ffc 当日分档）",
    )
    p.add_argument("--raw", action="store_true")
    p.set_defaults(func=cmd_money_flow)

    # f10
    p = sub.add_parser(
        "f10", help="7615 F10 资料接口（个股 / ETF基金 / 板块 / 指数 / 大盘）"
    )
    _add_output_options(p)
    _add_connection_options(p, host=False)
    p.add_argument("--base-url", default=None, help="TQLEX 网关地址（默认 static.tdx.com.cn:7615）")
    p.add_argument("action", choices=_F10_ACTIONS, help="子操作（sections 列出全部 section）")
    _add_code_positional(p, nargs="*")
    p.add_argument("--section", default=None, help="分区，见 `eltdx f10 sections`")
    p.add_argument("--arg", default=None, help="分区附加参数（如 kind / reqtype / 排序键）")
    p.add_argument("--date", default=None, help="日期（YYYYMMDD 或 YYYY-MM-DD）")
    p.add_argument("--report-date", default=None, help="报告期（YYYYMMDD）")
    p.add_argument("--topic", default=None, help="题材 ID（topic-compare）")
    p.add_argument("--entry", default=None, help="TQLEX Entry（call，如 CWServ.tdxf10_gg_gdyj）")
    p.add_argument("--params", default=None, help="Params JSON 数组（call）")
    p.add_argument("--page", type=int, default=1, help="页码（默认 1）")
    p.add_argument("--page-size", type=int, default=20, help="每页条数（默认 20）")
    p.set_defaults(func=cmd_f10)
    # limits
    p = sub.add_parser("limits", help="每日涨跌停价")
    _add_output_options(p)
    _add_connection_options(p)
    _add_code_positional(p, nargs="*")
    p.add_argument("--date", default=None, help="交易日 YYYYMMDD（默认最近交易日）")
    p.set_defaults(func=cmd_limits)

    # ex
    p = sub.add_parser("ex", help="扩展行情（7727，EX 协议）")
    _add_output_options(p)
    _add_connection_options(p, port=7727)
    p.add_argument(
        "action",
        choices=("markets", "count", "info", "quote", "quote-list", "bars", "minute", "txn"),
        help="子操作",
    )
    p.add_argument("code", nargs="*", default=[], help="合约代码（如 IFL0）")
    p.add_argument("--market", type=int, default=None, help="扩展市场编号")
    p.add_argument("--category", type=int, default=None, help="品种类别编号")
    p.add_argument("--start", type=int, default=0)
    p.add_argument("--count", type=int, default=20)
    p.add_argument("--all-hosts", action="store_true", help="连同不稳定的免费 KNOWN_EX 主机一起测速")
    p.add_argument(
        "--probe-server",
        action="store_true",
        help="先向服务器索取市场表（实测所有内置主站都不回应，会阻塞到超时），失败后才回退枚举",
    )
    p.set_defaults(func=cmd_ex)

    # mac
    p = sub.add_parser("mac", help="MAC 协议（7709 MAC 服务器）")
    _add_output_options(p)
    _add_connection_options(p, port=7709)
    p.add_argument(
        "action",
        choices=("quote", "kline", "ranking", "board-list", "server-info", "symbol-info",
                 "minute", "txn", "auction", "unusual", "goods-list"),
        help="子操作",
    )
    p.add_argument("code", nargs="*", default=[], help="证券代码，如 'SZ 000001' / sz000001 / 600519")
    p.add_argument("--market", type=int, default=None, help="市场编号（0=深 1=沪 2=北）")
    p.add_argument("--category", type=int, default=None, help="排行分类编号")
    p.add_argument("--period", default="day")
    p.add_argument("--adjust", default="none")
    p.add_argument("--date", default=None)
    p.add_argument("--start", type=int, default=0)
    p.add_argument("--count", type=int, default=100)
    p.set_defaults(func=cmd_mac)

    # macex
    p = sub.add_parser("macex", help="MAC 扩展行情（7727，MAC_EX 主站）")
    _add_output_options(p)
    _add_connection_options(p, port=7727)
    p.add_argument(
        "action",
        choices=("count", "instruments", "list", "quote", "quote-list", "kline", "minute", "txn"),
        help="子操作",
    )
    p.add_argument("code", nargs="*", default=[], help="标的代码")
    p.add_argument("--market", type=int, default=None, help="扩展市场编号")
    p.add_argument("--period", default="day")
    p.add_argument("--adjust", default="none")
    p.add_argument("--date", default=None)
    p.add_argument("--start", type=int, default=0)
    p.add_argument("--count", type=int, default=20)
    p.add_argument("--all-hosts", action="store_true", help="连同不稳定的免费 KNOWN_EX 主机一起测速")
    p.set_defaults(func=cmd_macex)

    # fund
    p = sub.add_parser("fund", help="场外基金净值（开放式基金/货币基金，走扩展行情）")
    _add_output_options(p)
    _add_connection_options(p, port=7727)
    p.add_argument("action", choices=("markets", "list", "nav", "kline", "info"), help="子操作")
    p.add_argument("code", nargs="*", default=[], help="基金代码（如 016947）")
    p.add_argument(
        "--market", type=int, default=DEFAULT_FUND_MARKET,
        help=f"扩展市场编号（默认 {DEFAULT_FUND_MARKET} 开放式基金；34 货币基金、49 香港基金 等）",
    )
    p.add_argument("--period", default="day")
    p.add_argument("--start", type=int, default=0)
    p.add_argument("--count", type=int, default=20)
    p.set_defaults(func=cmd_fund)

    # offline
    p = sub.add_parser("offline", help="本地行情文件（vipdoc）")
    _add_output_options(p)
    p.add_argument(
        "action",
        choices=("day", "min", "5min", "ex-day", "block", "industry", "gbbq", "financial"),
        help="子操作",
    )
    p.add_argument("--file", default=None, help="直接指定文件路径")
    p.add_argument("--vipdoc", default=None, help="vipdoc 根目录（默认自动探测）")
    p.add_argument("--market", type=int, default=None, help="市场编号（配合 --code 定位文件）")
    p.add_argument("--code", default=None, help="证券代码（配合 --market 定位文件）")
    p.add_argument("--count", type=int, default=0, help="只显示最后 N 条（0 = 全部）")
    p.add_argument("--last", action="store_true", help="只显示最后一条")
    p.set_defaults(func=cmd_offline)

    return parser


def _version_text() -> str:
    try:
        import eltdx

        return f"eltdx {eltdx.__version__}"
    except Exception:  # noqa: BLE001 - --version 不应因导入失败而崩
        return "eltdx (unknown)"


def main(argv: Sequence[str] | None = None) -> int:
    """CLI 主入口。返回进程退出码。"""

    parser = build_parser()
    # argparse 的已知局限：`nargs="*"` 位置参数写在选项之后（如
    # `macex quote --market 33 016947`）会被判为无法识别。这里用
    # parse_known_args 收下这些「裸 token」补回 code；真正的未知选项仍然报错。
    args, extra = parser.parse_known_args(argv)
    if extra:
        unknown_options = [token for token in extra if token.startswith("-")]
        if unknown_options:
            parser.error(f"unrecognized arguments: {' '.join(unknown_options)}")
        current = getattr(args, "code", None)
        if isinstance(current, list):
            args.code = [*current, *extra]
        elif current is None:
            args.code = list(extra)
        else:
            args.code = [current, *extra]

    if not getattr(args, "func", None):
        parser.print_help()
        return 0
    try:
        return int(args.func(args))
    except KeyboardInterrupt:  # pragma: no cover
        return 130
    except (ValueError, RuntimeError, OSError) as exc:
        return fail(f"{type(exc).__name__}: {exc}")
    except Exception as exc:  # noqa: BLE001 - CLI 顶层兜底，避免堆栈噪音
        name = type(exc).__name__
        if name.endswith("Error"):
            return fail(f"{name}: {exc}")
        raise


# ── 既有入口点（保持向后兼容）────────────────────────────────────────


def smoke() -> int:
    return smoke_main()


def f10_smoke() -> int:
    return f10_smoke_main()


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
