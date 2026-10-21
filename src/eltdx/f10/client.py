"""Client for the 7615 TQLEX / F10 HTTP gateway."""

from __future__ import annotations

import json
from collections.abc import Mapping, Sequence
from datetime import date, datetime
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode
from urllib.request import Request, urlopen

from eltdx.exceptions import ProtocolError, TransportError
from eltdx.protocol.unit import split_code

from ._http import open_ipv4_first

from .models import (
    F10Cell,
    F10Response,
    F10ResultSet,
    LimitBoardLadder,
    LimitBoardLadderRow,
)

DEFAULT_TQLEX_BASE_URL = "http://static.tdx.com.cn:7615/TQLEX"
DEFAULT_LIMIT_BOARD_LADDER_BASE_URL = "http://hot.icfqs.com:7615/TQLEX"
DEFAULT_QSID = "tdx"
LIMIT_BOARD_LADDER_ENTRY = "CWServ.cfg_fx_lbtt"


class F10Client:
    """High-level wrapper around the 7615 F10 / TQLEX gateway.

    The gateway has one physical HTTP endpoint and many logical Entry names.
    ``call`` exposes the raw Entry model; the other methods are thin product
    wrappers that fill the Entry name and parameter order for common pages.
    """

    def __init__(
        self,
        *,
        base_url: str = DEFAULT_TQLEX_BASE_URL,
        limit_board_ladder_base_url: str | None = None,
        timeout: float = 8.0,
        prefer_ipv4: bool = True,
        headers: Mapping[str, str] | None = None,
    ) -> None:
        self.base_url = base_url
        self.limit_board_ladder_base_url = (
            DEFAULT_LIMIT_BOARD_LADDER_BASE_URL
            if limit_board_ladder_base_url is None and base_url == DEFAULT_TQLEX_BASE_URL
            else (limit_board_ladder_base_url or base_url)
        )
        self.timeout = timeout
        self.prefer_ipv4 = prefer_ipv4
        self.headers = {
            "Content-Type": "application/json",
            "Accept": "application/json",
            "User-Agent": "eltdx/1.0",
        }
        if headers:
            self.headers.update(headers)

    def call(self, entry: str, body: Any | None = None, *, params: Sequence[Any] | None = None) -> F10Response:
        """Call any TQLEX Entry.

        Pass either an explicit JSON body or ``params=[...]``. ``params`` is
        encoded as ``{"Params": [...]}``, matching the CWServ convention.
        """

        if body is not None and params is not None:
            raise ValueError("pass either body or params, not both")
        request_body = {"Params": list(params)} if params is not None else (body if body is not None else {})
        return self._post(entry, request_body)

    def params(self, entry: str, *params: Any) -> F10Response:
        """Call a CWServ Entry with a Params array."""

        return self.call(entry, params=params)

    def limit_up_down_list(
        self,
        start_date: str | int | date | None = None,
        end_date: str | int | date | None = None,
        *,
        include_summary: bool = False,
    ) -> LimitBoardLadder:
        """Query the stock-level limit-up, broken-board and limit-down list.

        The page uses the dedicated ``CWServ.cfg_fx_lbtt`` 7615 Entry.  The
        detail request is ``Params=["1", start, end]``; when requested, the
        market overview is fetched with ``Params=["2", start, end]``.
        """

        selected_start, selected_end = _normalize_limit_board_dates(
            start_date, end_date
        )
        body = {"Params": ["1", selected_start, selected_end]}
        response = self._post(LIMIT_BOARD_LADDER_ENTRY, body)
        summary_rows: tuple[dict[str, Any], ...] = ()
        if include_summary:
            summary = self._post(
                LIMIT_BOARD_LADDER_ENTRY,
                {"Params": ["2", selected_start, selected_end]},
            )
            summary_rows = summary.rows

        rows = tuple(_parse_limit_board_ladder_row(row) for row in response.rows)
        return LimitBoardLadder(
            entry=response.entry,
            request_body=body,
            error_code=response.error_code,
            start_date=selected_start,
            end_date=selected_end,
            rows=rows,
            summary=summary_rows,
            raw=response.raw,
        )

    def limit_board_ladder(
        self,
        start_date: str | int | date | None = None,
        end_date: str | int | date | None = None,
        *,
        include_summary: bool = False,
    ) -> LimitBoardLadder:
        """Compatibility alias for :meth:`limit_up_down_list`."""

        return self.limit_up_down_list(
            start_date,
            end_date,
            include_summary=include_summary,
        )

    def stock_info(self, code: str) -> F10Response:
        """股票基础信息，通常用于页面初始化。"""

        return self.params("CWServ.tdxf10_gg_comreq", "gpquery", _code6(code))

    def business_periods(self, code: str) -> F10Response:
        """主营构成可选报告期。"""

        return self.params("CWServ.tdxf10_gg_comreq", "zygcfx", _code6(code))

    def topic_ids(self, code: str) -> F10Response:
        """股票关联题材 ID 列表。"""

        return self.params("CWServ.tdxf10_gg_comreq", "rdtcgn", _code6(code))

    def company_profile(self, code: str, section: str = "8") -> F10Response:
        """公司概况。section=8 为发行上市信息，section=9 为指数调入调出。"""

        return self.params("CWServ.tdxf10_gg_gsgk", section, _code6(code), "")

    def business_composition(self, code: str, report_date: str | None = None) -> F10Response:
        """主营构成。report_date 不传时先取服务端给出的最新报告期。"""

        code6 = _code6(code)
        selected_date = report_date or _first_value(self.business_periods(code6), "T002")
        if selected_date is None:
            raise ProtocolError(f"no business composition report date for {code6}")
        return self.business_report(code, "composition", report_date=str(selected_date))

    def shareholders(
        self,
        code: str,
        section: str = "count",
        *,
        report_date: str | None = None,
        kind: str = "000",
        filter1: str = "",
        filter2: str = "",
        page: int = 1,
        page_size: int = 20,
    ) -> F10Response:
        """股东研究，底层 Entry ``CWServ.tdxf10_gg_gdyj``。

        section 取值：
        ``count`` 股东人数与股价比较；``ranking`` 同行业股东人数增减量排名；
        ``controller`` 控股股东与实际控制人；``top_circulating`` 十大流通股东；
        ``top`` 十大股东（按报告期）；``bonds`` 十大债券持有人；
        ``changes`` 重要股东持股变动；``institutions`` 机构持股汇总；
        ``institution_types`` 机构持股分类汇总（``filter2`` 取 ``report_date``）；
        ``institution_holdings`` 机构持仓明细（Entry 改用
        ``CWServ.tdxf10_gg_gdyj_jgcgmx``，Params 使用 ``kind`` 与 ``report_date``）；
        ``plan`` 股东增减持计划，等价于 :meth:`shareholder_change_plans`。

        返回模型：:class:`eltdx.f10.models.F10Response`。
        """

        code6 = _code6(code)
        page_text = str(page)
        size_text = str(page_size)
        date_text = report_date or ""
        gdyj = "CWServ.tdxf10_gg_gdyj"
        table: dict[str, tuple[str, tuple[Any, ...]]] = {
            "count": (gdyj, (code6, "gdrs", filter1, filter2, page_text, page_text, size_text)),
            "ranking": (gdyj, (code6, "thygdrs", filter1, filter2, page_text, page_text, size_text)),
            "controller": (gdyj, (code6, "kggd", filter1, filter2, page_text, page_text, size_text)),
            "top_circulating": (gdyj, (code6, "ltgd", filter1, filter2, page_text, page_text, size_text)),
            "top": (gdyj, (code6, "sdgdbgq", filter1, filter2, page_text, page_text, size_text)),
            "bonds": (gdyj, (code6, "sdzqcyr", filter1, filter2, page_text, page_text, size_text)),
            "changes": (gdyj, (code6, "cgbd", filter1, filter2, page_text, page_text, size_text)),
            "institutions": (gdyj, (code6, "jgcg", filter1, filter2, page_text, page_text, size_text)),
            "institution_types": (gdyj, (code6, "jgcgz", filter1, date_text, page_text, page_text, size_text)),
            "institution_holdings": (
                "CWServ.tdxf10_gg_gdyj_jgcgmx",
                (code6, kind, date_text, "99", page_text, page_text, size_text),
            ),
            "plan": (gdyj, (code6, "gdzjcjh", filter1, filter2, page_text, page_text, size_text)),
        }
        entry, params = _section_request(section, table)
        return self.params(entry, *params)

    def shareholder_change_plans(
        self,
        code: str,
        *,
        page: int = 1,
        page_size: int = 20,
        filter1: str = "",
        filter2: str = "",
    ) -> F10Response:
        """股东增减持计划。"""

        return self.shareholders(
            code,
            "plan",
            page=page,
            page_size=page_size,
            filter1=filter1,
            filter2=filter2,
        )

    def capital_structure(
        self,
        code: str,
        section: str = "structure",
        *,
        date: str | None = None,
    ) -> F10Response:
        """股本结构，底层 Entry ``CWServ.tdxf10_gg_gbjg``。

        section 取值：
        ``structure`` 股本结构；``changes`` 股本变动；``buyback`` 股票回购；
        ``restricted`` 限售流通；``restricted_detail`` 限售解禁明细（Entry 改用
        ``CWServ.tdxf10_gg_gbjg_xsjjmx``，必须传 ``date``）。

        ``date`` 为 ``YYYYMMDD`` 字符串，仅 ``restricted_detail`` 使用。

        返回模型：:class:`eltdx.f10.models.F10Response`。
        """

        code6 = _code6(code)
        gbjg = "CWServ.tdxf10_gg_gbjg"
        table: dict[str, tuple[str, tuple[Any, ...]]] = {
            "structure": (gbjg, (code6, "gbjg")),
            "changes": (gbjg, (code6, "gbbd")),
            "buyback": (gbjg, (code6, "gphg")),
            "restricted": (gbjg, (code6, "xslt")),
        }
        _require_section(section, (*table, "restricted_detail"))
        if section == "restricted_detail":
            if not date:
                raise ValueError(
                    "capital_structure section 'restricted_detail' requires date (YYYYMMDD)"
                )
            return self.params("CWServ.tdxf10_gg_gbjg_xsjjmx", code6, _date_text(date))
        entry, params = table[section]
        return self.params(entry, *params)

    def business_report(
        self,
        code: str,
        section: str = "business",
        *,
        report_date: str | None = None,
    ) -> F10Response:
        """经营分析，底层 Entry ``CWServ.tdxf10_gg_jyfx``。

        section 取值：
        ``business`` 主营业务概述；``composition`` 主营构成（等价于
        :meth:`business_composition`）；``customers`` 前五名客户；
        ``suppliers`` 前五名供应商；``price`` 产品价格；
        ``changes`` 财务指标大幅变动说明。

        ``report_date`` 为 ``YYYYMMDD`` 字符串，``composition`` / ``customers`` /
        ``suppliers`` / ``changes`` 由服务端要求提供，未提供时按空串发送。

        返回模型：:class:`eltdx.f10.models.F10Response`。
        """

        code6 = _code6(code)
        date_text = "" if report_date is None else str(report_date)
        jyfx = "CWServ.tdxf10_gg_jyfx"
        table: dict[str, tuple[str, tuple[Any, ...]]] = {
            "business": (jyfx, (code6, "zyyw", "")),
            "composition": (jyfx, (code6, "zygc", date_text)),
            "customers": (jyfx, (code6, "qwm", date_text)),
            "suppliers": (jyfx, (code6, "qwmgys", date_text)),
            "price": (jyfx, (code6, "cpjg", "")),
            "changes": (jyfx, (code6, "0", date_text)),
        }
        entry, params = _section_request(section, table)
        return self.params(entry, *params)

    def industry_analysis(
        self,
        code: str,
        section: str = "peers",
        *,
        arg: str = "",
    ) -> F10Response:
        """行业分析，底层 Entry ``CWServ.tdxf10_gg_hyfx``，Params 为 ``[sub, code, arg]``。

        section 取值：
        ``peers`` 同行业对比（``arg`` 默认 ``"000"``）；``dividend_rank`` 分红融资排名
        （默认 ``"100"``）；``scale`` 公司规模（默认 ``"000"``）；``valuation`` 估值位置
        （默认 ``"011"``）；``news`` 行业新闻；``reports`` 行业研报；
        ``market_share`` 市场表现（默认 ``"000"``）；``summary`` 行业综述。

        ``comparable`` 可比公司列表（Entry 改用 ``CWServ.tdxf10_gg_kbgs``，Params
        ``["1", code]``）；``comparable_profile`` 本公司在本页的资料（同一 Entry，
        Params ``["0", code]``）。

        ``arg`` 为空时使用该 section 的默认值，且不影响 ``comparable`` /
        ``comparable_profile``（它们的 Params 没有附加位）。

        返回模型：:class:`eltdx.f10.models.F10Response`。
        """

        code6 = _code6(code)
        hyfx = "CWServ.tdxf10_gg_hyfx"
        kbgs = "CWServ.tdxf10_gg_kbgs"
        table: dict[str, tuple[str, tuple[Any, ...]]] = {
            "peers": (hyfx, ("cwgz", code6, arg or "000")),
            "dividend_rank": (hyfx, ("fhrzb", code6, arg or "100")),
            "scale": (hyfx, ("gsgm", code6, arg or "000")),
            "valuation": (hyfx, ("gzsp", code6, arg or "011")),
            "news": (hyfx, ("hyxw", code6, arg or "")),
            "reports": (hyfx, ("hyyb", code6, arg or "")),
            "market_share": (hyfx, ("scbx", code6, arg or "000")),
            "summary": (hyfx, ("tot", code6, arg or "")),
            "comparable": (kbgs, ("1", code6)),
            "comparable_profile": (kbgs, ("0", code6)),
        }
        entry, params = _section_request(section, table)
        return self.params(entry, *params)

    def trading_report(
        self,
        code: str,
        section: str = "margin",
        *,
        arg: str = "",
        date: str | None = None,
        page: int = 1,
        page_size: int = 20,
    ) -> F10Response:
        """交易数据，底层 Entry ``CWServ.tdxf10_gg_jyds``（``margin_history`` 走 ``CWServ.tdxf10_gg_sj``）。

        section 取值：
        ``margin`` 近期融资融券；``margin_history`` 近 6 个月融资融券（Entry
        ``CWServ.tdxf10_gg_sj``，Params ``["rzrq_sj", code, "0", code]``）；
        ``block_trades`` 大宗交易；``dragon_board`` 龙虎榜（``arg`` 默认 ``"2"``）；
        ``northbound`` 沪深股通；``limit_down`` 跌停分析；``limit_up`` 涨停分析（含涨停原因）；
        ``bonds`` 可转债；``main_force`` 主力盘口；``reports`` 研报席位；
        ``kcb`` 科创板标记。

        资金流向不在此方法内，统一由 :meth:`eltdx.api.money_flow.MoneyFlowApi.history` 提供。
        ``date`` 当前没有 section 使用，仅为与其它报表方法保持签名一致。

        返回模型：:class:`eltdx.f10.models.F10Response`。
        """

        code6 = _code6(code)
        jyds = "CWServ.tdxf10_gg_jyds"
        table: dict[str, tuple[str, tuple[Any, ...]]] = {
            "margin": (jyds, (code6, "rzrq", "")),
            "margin_history": ("CWServ.tdxf10_gg_sj", ("rzrq_sj", code6, "0", code6)),
            "block_trades": (jyds, (code6, "dzjy", "")),
            "dragon_board": (jyds, (code6, "lgt", arg or "2")),
            "northbound": (jyds, (code6, "islgt", "")),
            "limit_down": (jyds, (code6, "dtfx", "")),
            "limit_up": (jyds, (code6, "ztfx", "")),
            "bonds": (jyds, (code6, "zrq", "")),
            "main_force": (jyds, (code6, "zlpskcj", "")),
            "reports": (jyds, (code6, "yxsbxx", "")),
            "kcb": (jyds, (code6, "checkkcb", "")),
        }
        entry, params = _section_request(section, table)
        return self.params(entry, *params)

    def fund_report(
        self,
        code: str,
        section: str = "basic",
        *,
        arg: str = "",
        date: str | None = None,
        page: int = 1,
        page_size: int = 20,
    ) -> F10Response:
        """ETF / 场外基金 F10，底层 Entry 为 ``CWServ.skef10_jj_*`` 家族。

        section 取值：
        ``name`` 名称与市场类型（``skef10_jj_idreq``）；``managers`` 基金经理；
        ``company`` 基金管理人；``basic`` 基本概况；``seats`` 交易席位；
        ``scale`` 规模与份额；``scale_history`` 份额规模历史；``nav`` 单位净值序列；
        ``nav_period`` 区间收益；``nav_rank`` 同类排名；``dividends`` 基金分红；
        ``asset_allocation`` 资产配置；``industry_allocation`` 行业分布；
        ``holdings_compare`` 重仓股变动；``holdings_rank`` 重仓排名；
        ``holding_summary`` 基金持仓汇总；``bond_allocation`` 债券配置；
        ``futures_allocation`` 期货持仓明细；``holdings`` 持股情况；
        ``holdings_detail`` 持股明细；``holdings_changes`` 持有基金明细；
        ``fund_of_funds`` 下属基金资产；``finance`` 财务指标；``sales_info`` 费率；
        ``sales_announcements`` 销售公告；``sales_news`` 相关资讯；``etf`` 是否 ETF；
        ``margin`` ETF 融资融券；``ratings`` 基金评级（``arg`` 默认 ``"001"``）；
        ``categories`` 分类同类衍生表（``arg`` 默认 ``"bgqzf"``）。

        ``date`` 用于 ``industry_allocation`` / ``bond_allocation`` /
        ``futures_allocation``；``etf`` / ``margin`` 未给 ``date`` 时用 ``"3M"``。

        返回模型：:class:`eltdx.f10.models.F10Response`。
        """

        code6 = _code6(code)
        page_text = str(page)
        size_text = str(page_size)
        date_text = date or ""
        jj = "CWServ.skef10_jj_"
        jjzl = f"{jj}jjzl"
        gmfe = f"{jj}gmfe"
        jzhb = f"{jj}jzhb"
        tzzh = f"{jj}tzzh"
        cgqk = f"{jj}cgqk"
        xsxx = f"{jj}xsxx"
        etfzj = f"{jj}etfzj"
        table: dict[str, tuple[str, tuple[Any, ...]]] = {
            "name": (f"{jj}idreq", ("name", code6)),
            "managers": (jjzl, ("1", code6, "", page_text, page_text, size_text)),
            "company": (jjzl, ("2", code6, "", page_text, page_text, size_text)),
            "basic": (jjzl, ("4", code6, "", page_text, page_text, size_text)),
            "seats": (jjzl, ("5", code6, arg, page_text, page_text, size_text)),
            "scale": (gmfe, ("4", code6, "", "")),
            "scale_history": (gmfe, (arg or "1", code6, "", date_text)),
            "nav": (jzhb, ("0", code6, "0", "")),
            "nav_period": (jzhb, ("1", code6, "0", "")),
            "nav_rank": (jzhb, ("3", code6, "0", "")),
            "dividends": (jzhb, ("5", code6, "0", "")),
            "asset_allocation": (tzzh, ("zcpz", code6, "")),
            "industry_allocation": (tzzh, ("hyfb", code6, date_text)),
            "holdings_compare": (tzzh, ("cgjzd", code6, "")),
            "holdings_rank": (tzzh, ("zjpz", code6, "")),
            "holding_summary": (tzzh, ("jjxtzzh", code6, "")),
            "bond_allocation": (tzzh, ("zczq", code6, date_text)),
            "futures_allocation": (tzzh, ("ccqhmx", code6, date_text)),
            "holdings": (cgqk, ("cgqkbgq", code6, "", "", "1", "1", "1")),
            "holdings_detail": (cgqk, ("cgmxbgq", code6, "", "", "1", "1", "1")),
            "holdings_changes": (cgqk, ("cyjjmxbgq", code6, "", "", "1", "1", "1")),
            "fund_of_funds": (cgqk, ("sxjjzcbgq", code6, "", "", "1", "1", "1")),
            "finance": (f"{jj}cwzk", (arg or "zcfzb", code6)),
            "sales_info": (xsxx, ("0", code6)),
            "sales_announcements": (xsxx, ("4", code6)),
            "sales_news": (xsxx, ("5", code6)),
            "etf": (etfzj, ("etfzjlx", code6, date_text or "3M")),
            "margin": (etfzj, ("etfrzrq", code6, date_text or "3M")),
            "ratings": (f"{jj}jjpj", (arg or "001", code6)),
            "categories": (f"{jj}fxsy", (arg or "bgqzf", code6, "")),
        }
        entry, params = _section_request(section, table)
        return self.params(entry, *params)

    def market_report(
        self,
        code: str,
        section: str = "board_info",
        *,
        arg: str = "",
        date: str | None = None,
        page: int = 1,
        page_size: int = 20,
    ) -> F10Response:
        """板块 / 指数 / 大盘指数 F10，底层 Entry 为 ``CWServ.skef10_bk_*``、``skef10_zs_*``、``skef10_dpzx_*``。

        section 取值：
        ``board_info`` 板块名称与市场；``board_basics`` 板块基础资料；
        ``board_description`` 板块说明；``board_trading`` 板块交易数据；
        ``board_flow`` 板块资金（``arg`` 默认 ``"001_1"``）；``board_members`` 成份股数据
        （``arg`` 默认 ``"001"``）；``board_analysis`` 板块三点分析；
        ``board_finance_compare`` 成份股财务对比；``board_candidates`` 成份股候选数据
        （``arg`` 默认 ``"001"``）；``board_trend`` 板块异动分析；
        ``index_info`` 指数信息；``index_profile`` 指数概况；``index_tracking`` 跟踪标的；
        ``index_flow`` 指数资金动向（``arg`` 默认 ``"004_1"``）；
        ``index_flow_members`` 成份股资金（``arg`` 默认 ``"001"``）；
        ``index_notices`` 今日提示（``arg`` 默认 ``"001"``）；
        ``market_basic`` 大盘指数基本信息；``market_margin`` 大盘两融；
        ``market_new_shares`` 新股分析；``market_capital`` 大盘市场概览
        （``arg`` 默认 ``"004"``）。

        ``market_new_shares`` 与 ``market_capital`` 不需要 ``code``；
        ``page`` / ``page_size`` 仅对集合类 section 有效。

        返回模型：:class:`eltdx.f10.models.F10Response`。
        """

        code6 = _code6(code)
        page_text = str(page)
        size_text = str(page_size)
        bk = "CWServ.skef10_bk_"
        zs = "CWServ.skef10_zs_"
        dpzx = "CWServ.skef10_dpzx_"
        table: dict[str, tuple[str, tuple[Any, ...]]] = {
            "board_info": (f"{bk}getinfo", (code6,)),
            "board_basics": (f"{bk}cpbd_jczl", ("001", code6, "")),
            "board_description": (f"{bk}cpbd_jczl", ("002", code6, "")),
            "board_trading": (f"{bk}cpbd_jczl", ("004", code6, "")),
            "board_flow": (f"{bk}zjlx", (arg or "001_1", code6, "")),
            "board_members": (f"{bk}scgl", (arg or "001", code6, page_text, size_text)),
            "board_analysis": (f"{bk}sdfx", ("001", code6, "")),
            "board_finance_compare": (f"{bk}ggdb_cwdb", (code6,)),
            "board_candidates": (f"{bk}cfgsj", (arg or "001", code6)),
            "board_trend": (f"{bk}ydfx", (code6,)),
            "index_info": (f"{zs}getinfo", (code6,)),
            "index_profile": (f"{zs}jbxx_zsgk", (code6,)),
            "index_tracking": (f"{zs}jbxx_gzbd", (code6, page_text, size_text)),
            "index_flow": (f"{zs}zjdx", (arg or "004_1", code6, "", "", page_text, size_text)),
            "index_flow_members": (f"{zs}cfgsj_cfgjcsj", (arg or "001", code6, page_text, size_text)),
            "index_notices": (f"{zs}jrts", (arg or "001", code6, page_text, size_text)),
            "market_basic": (f"{dpzx}jbxx", ("002", code6)),
            "market_margin": (f"{dpzx}rzrq", ("001", code6)),
            "market_new_shares": (f"{dpzx}xgfx", ("002",)),
            "market_capital": (f"{dpzx}scgl", (arg or "004",)),
        }
        entry, params = _section_request(section, table)
        return self.params(entry, *params)

    def dividend_financing(self, code: str, section: str = "fh") -> F10Response:
        """分红融资类数据。section=fh 为分红方案历史。"""

        return self.params("CWServ.tdxf10_gg_fhrz", _code6(code), section)

    def allotment_dates(self, code: str) -> F10Response:
        """增发获配明细可选日期。"""

        return self.params("CWServ.tdxf10_gg_fhrz_zfhpmx", "zfpg_bgq", _code6(code), "")

    def allotment_details(self, code: str, date: str) -> F10Response:
        """指定增发日期的获配机构明细。"""

        return self.params("CWServ.tdxf10_gg_fhrz_zfhpmx", "zfpg", _code6(code), date)

    def finance_report(self, code: str, report_type: str = "zcfzb") -> F10Response:
        """财务报表。report_type=zcfzb 为资产负债表。"""

        return self.params("CWServ.tdxf10_gg_cwfx", _code6(code), report_type, "")

    def finance_diagnosis(self, code: str, section: str = "yynl", scope: str = "") -> F10Response:
        """财务诊断。"""

        return self.params("CWServ.tdxf10_gg_cwzd", section, _code6(code), scope)

    def stock_score(self, code: str, section: str = "pf", arg: str = "") -> F10Response:
        """个股总评。section=pf 为综合评分。"""

        return self.params("CWServ.tdxf10_gg_ggzp", section, _code6(code), arg, "")

    def profit_forecast(self, code: str, section: str = "ylyctj") -> F10Response:
        """盈利预测评级统计。section=ylyctj 为盈利预测统计，ylycmx 为盈利预测明细，
        yzyq 为一致预期，tzpjtj 为投资评级统计，ycpjyjbg 为研报。"""

        _require_section(section, ("ylyctj", "ylycmx", "yzyq", "tzpjtj", "ycpjyjbg"))
        return self.params("CWServ.tdxf10_gg_ybpj", _code6(code), section)

    def ranking_detail(self, code: str, section: str = "scpmdela") -> F10Response:
        """市场排名 / 行业排名明细。"""

        return self.params("CWServ.tdxf10_gg_zxts_rqpm", _code6(code), section)

    def governance(self, code: str, section: str = "wgcl", arg: str = "") -> F10Response:
        """资本运作治理。section=wgcl 为违规处理，dbmx 为担保明细。"""

        return self.params("CWServ.tdxf10_gg_zbyz", section, _code6(code), arg)

    def hot_topics(self, code: str, section: str = "zttzbkz") -> F10Response:
        """热点题材。section=zttzbkz 为板块题材。"""

        return self.params("CWServ.tdxf10_gg_rdtc", _code6(code), section)

    def topic_compare(self, code: str, topic_id: str, section: str = "gndbzfsj", sort_by: str = "zdf") -> F10Response:
        """题材内对比排名。"""

        return self.params("CWServ.tdxf10_gg_rdtc_gndb", section, _code6(code), str(topic_id), sort_by)

    def topic_compare_first(self, code: str, section: str = "gndbzfsj", sort_by: str = "zdf") -> F10Response:
        """用股票第一个题材 ID 查询题材内对比排名。"""

        topic_id = _first_value(self.topic_ids(code), "t001")
        if topic_id is None:
            raise ProtocolError(f"no topic id for {_code6(code)}")
        return self.topic_compare(code, str(topic_id), section=section, sort_by=sort_by)

    def company_news(
        self,
        code: str,
        section: str = "gsyj",
        *,
        keyword: str = "",
        rating: str | int = "0",
        page: int = 1,
        page_size: int = 20,
    ) -> F10Response:
        """公司资讯。section=gsyj 为研报，jgcs 为监管措施，finance_review 为财报点评。

        ``finance_review`` 的底层 Entry 改用 ``CWServ.tdxf10_gg_cwfx_cbdp``，
        Params 为 ``[code, str(page)]``，不使用 ``keyword`` / ``rating`` /
        ``page_size``。
        """

        if section == "finance_review":
            return self.params("CWServ.tdxf10_gg_cwfx_cbdp", _code6(code), str(page))
        return self.params("CWServ.tdxf10_gg_gszx", _code6(code), section, keyword, str(rating), str(page), str(page_size))

    def digest(
        self,
        code: str,
        section: str = "tips",
        *,
        page: int = 1,
        page_size: int = 20,
    ) -> F10Response:
        """最新提示，底层 Entry ``CWServ.tdxf10_gg_zxts``，Params ``[code, sub, ""]``。

        section 取值：
        ``tips`` 最新提示；``profile`` 公司概要；``events`` 公司大事；
        ``news`` 公司新闻；``announcements`` 公司公告；``reports`` 公司研报；
        ``roadshows`` 路演互动；``interaction`` 投资者互动（Entry 改用
        ``CWServ.tdxf10_gg_zxtstzzhd``，Params ``[code, "1", "1", page_size]``）。

        ``page`` 当前没有 section 使用，仅为与其它列表方法保持签名一致；
        ``page_size`` 仅 ``interaction`` 使用。

        返回模型：:class:`eltdx.f10.models.F10Response`。
        """

        code6 = _code6(code)
        zxts = "CWServ.tdxf10_gg_zxts"
        table: dict[str, tuple[str, tuple[Any, ...]]] = {
            "tips": (zxts, (code6, "zxts", "")),
            "profile": (zxts, (code6, "gsgy", "")),
            "events": (zxts, (code6, "gsds", "")),
            "news": (zxts, (code6, "gsxw", "")),
            "announcements": (zxts, (code6, "gsgg", "")),
            "reports": (zxts, (code6, "gsyj", "")),
            "roadshows": (zxts, (code6, "lyhd", "")),
            "interaction": ("CWServ.tdxf10_gg_zxtstzzhd", (code6, "1", "1", str(page_size))),
        }
        entry, params = _section_request(section, table)
        return self.params(entry, *params)

    def northbound_holding(
        self,
        code: str,
        section: str = "bszj",
        *,
        filter_value: str = "",
        page: int = 1,
        page_size: int = 20,
    ) -> F10Response:
        """沪深股通持股变化。"""

        return self.params(
            "CWServ.tdxf10_gg_zlcc",
            _code6(code),
            section,
            filter_value,
            str(page),
            str(page),
            str(page_size),
        )

    def detail(self, detail_type: str, record_id: str | int) -> F10Response:
        """按记录 ID 查询详情正文。"""

        return self.params("CWServ.tdxf10_gg_idreq", detail_type, str(record_id))

    def cache_list(self, code: str, kind: str = "gg") -> F10Response:
        """新闻 / 公告 / 路演缓存列表。kind=xw、gg、ly。"""

        if kind not in {"xw", "gg", "ly"}:
            raise ValueError("kind must be one of: xw, gg, ly")
        market_id, _, code6 = split_code(code)
        body = {"action": "get", "key": f"{kind}:{market_id}_{code6}", "bin": "1", "qsid": DEFAULT_QSID}
        return self.call("CWSearch.tzx_rcache", body)

    def announcements(self, code: str) -> F10Response:
        """公告列表。"""

        return self.cache_list(code, "gg")

    def news(self, code: str) -> F10Response:
        """新闻列表。"""

        return self.cache_list(code, "xw")

    def roadshows(self, code: str) -> F10Response:
        """路演列表。"""

        return self.cache_list(code, "ly")

    def theme_market(
        self,
        code: str,
        req_id: str | int = "200743",
        *,
        page: int = -1,
        page_size: int = 10,
        zq_num: int | str = 30,
        extra: Mapping[str, Any] | None = None,
    ) -> F10Response:
        """题材概念行情。req_id=200743 为相关板块。"""

        payload = {
            "ReqId": str(req_id),
            "setcode": split_code(code)[0],
            "code": _code6(code),
            "Page": page,
            "PageSize": str(page_size),
            "modname": "mod_tcihq.dll",
        }
        if str(req_id) in {"200742", "200745", "200747"}:
            payload["zq_num"] = str(zq_num)
        if extra:
            payload.update(extra)
        return self.call("HQServ.hq_nlp_tcihq", [payload])

    def valuation(
        self,
        code: str,
        req_id: str | int = "200191",
        *,
        page: int = 0,
        page_size: int = 20,
        extra: Mapping[str, Any] | None = None,
    ) -> F10Response:
        """估值市场数据。req_id=200191 为估值表。"""

        market_id, _, code6 = split_code(code)
        req_id_text = str(req_id)
        payload: dict[str, Any]
        if req_id_text in {"200191", "200192"}:
            payload = {
                "ReqId": req_id_text,
                "Code": f"{code6}|{market_id}",
                "BeginDate": "0",
                "EndDate": "0",
                "Page": str(page),
                "PageSize": str(page_size),
                "modname": "mod_gpsj.dll",
            }
            if req_id_text == "200192":
                payload.setdefault("Type", "0")
                payload["Page"] = str(page if page != 0 else -1)
                payload["PageSize"] = str(page_size if page_size != 20 else 5)
        elif req_id_text == "200135":
            payload = {
                "ReqId": req_id_text,
                "code": code6,
                "setcode": str(market_id),
                "modname": "mod_gpsj.dll",
            }
        elif req_id_text == "200124":
            payload = {
                "ReqId": req_id_text,
                "code": code6,
                "setcode": str(market_id),
                "sjfw": "1Y",
                "sdate": "",
                "edate": "",
                "zb": "0",
                "hqzq": "0",
                "cwppgz": 1,
                "jglx": 0,
                "Page": page if page != 0 else 1,
                "PageSize": str(page_size if page_size != 20 else 300),
                "modname": "mod_gpsj.dll",
            }
        else:
            payload = {
                "ReqId": req_id_text,
                "code": code6,
                "setcode": str(market_id),
                "modname": "mod_gpsj.dll",
            }
        if extra:
            payload.update(extra)
        return self.call("HQServ.hq_nlp_gpsj", [payload])

    def _post(self, entry: str, body: Any) -> F10Response:
        base_url = (
            self.limit_board_ladder_base_url
            if entry == LIMIT_BOARD_LADDER_ENTRY
            else self.base_url
        )
        url = f"{base_url}?{urlencode({'Entry': entry})}"
        data = json.dumps(body, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
        request = Request(url, data=data, headers=self.headers, method="POST")
        try:
            if self.prefer_ipv4:
                with open_ipv4_first(request, timeout=self.timeout) as response:
                    raw_bytes = response.read()
            else:
                with urlopen(request, timeout=self.timeout) as response:
                    raw_bytes = response.read()
        except HTTPError as exc:
            raise TransportError(f"TQLEX HTTP error {exc.code} for {entry}") from exc
        except URLError as exc:
            raise TransportError(f"TQLEX request failed for {entry}: {exc.reason}") from exc
        except TimeoutError as exc:
            raise TransportError(f"TQLEX request timed out for {entry}") from exc

        try:
            raw = json.loads(raw_bytes.decode("utf-8-sig"))
        except (UnicodeDecodeError, json.JSONDecodeError) as exc:
            raise ProtocolError(f"TQLEX returned invalid JSON for {entry}") from exc
        if not isinstance(raw, dict):
            raise ProtocolError(f"TQLEX returned non-object JSON for {entry}")
        return parse_tqlex_response(entry, body, raw)


def parse_tqlex_response(entry: str, request_body: Any, raw: Mapping[str, Any]) -> F10Response:
    """Parse a TQLEX JSON object into stable table models."""

    result_sets_raw = raw.get("ResultSets") or ()
    if not isinstance(result_sets_raw, Sequence) or isinstance(result_sets_raw, (str, bytes, bytearray)):
        raise ProtocolError("TQLEX ResultSets must be an array")

    result_sets = tuple(_parse_result_set(item, index) for index, item in enumerate(result_sets_raw))
    error_code_raw = raw.get("ErrorCode")
    error_code = int(error_code_raw) if error_code_raw is not None else None
    return F10Response(
        entry=entry,
        request_body=request_body,
        error_code=error_code,
        result_sets=result_sets,
        raw=dict(raw),
    )


def _parse_result_set(raw: Any, table_index: int) -> F10ResultSet:
    if not isinstance(raw, Mapping):
        raise ProtocolError("TQLEX ResultSet must be an object")
    columns = _columns(raw)
    content = raw.get("Content") or ()
    if not isinstance(content, Sequence) or isinstance(content, (str, bytes, bytearray)):
        raise ProtocolError("TQLEX ResultSet Content must be an array")

    rows: list[dict[str, Any]] = []
    row_cells: list[tuple[F10Cell, ...]] = []
    for row in content:
        values = _row_values(row)
        cells = tuple(F10Cell(name=_column_name(columns, index), value=value, index=index) for index, value in enumerate(values))
        row_cells.append(cells)
        rows.append(_row_dict(cells))

    return F10ResultSet(
        key=raw.get("ResultSetKey") or f"table{table_index}",
        columns=tuple(columns),
        rows=tuple(rows),
        row_cells=tuple(row_cells),
        raw=dict(raw),
    )


def _columns(raw: Mapping[str, Any]) -> list[str]:
    if "ColName" in raw:
        col_name = raw.get("ColName") or ()
        if not isinstance(col_name, Sequence) or isinstance(col_name, (str, bytes, bytearray)):
            raise ProtocolError("TQLEX ColName must be an array")
        return [str(item) for item in col_name]

    if "ColDes" in raw:
        col_des = raw.get("ColDes") or ()
        if not isinstance(col_des, Sequence) or isinstance(col_des, (str, bytes, bytearray)):
            raise ProtocolError("TQLEX ColDes must be an array")
        names = []
        for item in col_des:
            if isinstance(item, Mapping):
                names.append(str(item.get("Name", "")))
            else:
                names.append(str(item))
        return names

    return []


def _row_values(row: Any) -> list[Any]:
    if isinstance(row, Sequence) and not isinstance(row, (str, bytes, bytearray)):
        return list(row)
    return [row]


def _column_name(columns: Sequence[str], index: int) -> str:
    if index < len(columns) and columns[index]:
        return columns[index]
    return f"col_{index}"


def _row_dict(cells: Sequence[F10Cell]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    counts: dict[str, int] = {}
    for cell in cells:
        count = counts.get(cell.name, 0)
        counts[cell.name] = count + 1
        key = cell.name if count == 0 else f"{cell.name}__{count + 1}"
        result[key] = cell.value
    return result


def _code6(code: str) -> str:
    return split_code(code)[2]


def _require_section(section: str, valid: Sequence[str]) -> None:
    """Raise ``ValueError`` listing the valid ``section`` values."""

    if section not in valid:
        names = ", ".join(sorted(valid))
        raise ValueError(f"unknown section: {section!r}; expected one of: {names}")


def _section_request(
    section: str,
    table: Mapping[str, tuple[str, tuple[Any, ...]]],
) -> tuple[str, tuple[Any, ...]]:
    """Return the ``(entry, params)`` pair registered for ``section``."""

    _require_section(section, tuple(table))
    return table[section]


def _first_value(response: F10Response, field_name: str) -> Any | None:
    for row in response.rows:
        if field_name in row:
            return row[field_name]
    return None


def _normalize_limit_board_dates(
    start_date: str | int | date | None,
    end_date: str | int | date | None,
) -> tuple[str, str]:
    """Normalize optional dates to the gateway's ``YYYYMMDD`` format."""

    start = _date_text(start_date)
    end = _date_text(end_date)
    if start is None and end is None:
        today = date.today().strftime("%Y%m%d")
        return today, today
    if start is None:
        start = end
    if end is None:
        end = start
    assert start is not None and end is not None
    if start > end:
        raise ValueError("start_date must be on or before end_date")
    return start, end


def _date_text(value: str | int | date | None) -> str | None:
    if value is None:
        return None
    if isinstance(value, datetime):
        value = value.date()
    if isinstance(value, date):
        return value.strftime("%Y%m%d")
    text = str(value).strip().replace("-", "").replace("/", "")
    if len(text) != 8 or not text.isdigit():
        raise ValueError(f"invalid date: {value!r}")
    try:
        datetime.strptime(text, "%Y%m%d")
    except ValueError as exc:
        raise ValueError(f"invalid date: {value!r}") from exc
    return text


def _parse_limit_board_ladder_row(row: Mapping[str, Any]) -> LimitBoardLadderRow:
    limit_type = _as_int(row.get("ztlb"))
    board_level = _as_int(row.get("lbts"))
    return LimitBoardLadderRow(
        trading_date=row.get("rq"),
        trading_date_value=row.get("rqex"),
        board_level=(
            board_level
            if limit_type == 1 and board_level is not None and board_level > 0
            else None
        ),
        code=_as_text(row.get("ZQDM")),
        market_id=_as_int(row.get("SC")),
        limit_reason=row.get("ztyy"),
        seal_amount=_as_number(row.get("fde")),
        name=row.get("ZQJC"),
        limit_reason_extra=row.get("ztyy2"),
        limit_time=row.get("ztsj"),
        broken_count=_as_int(row.get("kbcs")),
        industry=row.get("sshy"),
        limit_type=limit_type,
        success_rate=row.get("cgl"),
        highest_board_level=_as_int(row.get("zglb")),
        raw=dict(row),
    )


def _as_text(value: Any) -> str | None:
    return None if value is None else str(value)


def _as_int(value: Any) -> int | None:
    if value is None or value == "":
        return None
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


def _as_number(value: Any) -> float | int | None:
    if value is None or value == "":
        return None
    if isinstance(value, bool):
        return int(value)
    if isinstance(value, (int, float)):
        return value
    try:
        number = float(value)
    except (TypeError, ValueError):
        return None
    return int(number) if number.is_integer() else number
