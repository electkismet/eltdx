"""Client for the 7615 TQLEX / F10 HTTP gateway."""

from __future__ import annotations

import json
from collections.abc import Mapping, Sequence
from datetime import date, datetime
from sys import modules as _MODULES
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode
from urllib.request import Request, urlopen

from eltdx.exceptions import ProtocolError, TransportError
from eltdx.protocol.unit import MARKET_TO_ID, split_code

from ._http import open_ipv4_first

from .models import (
    BoardMember,
    BoardMemberTable,
    F10Cell,
    F10Response,
    F10ResultSet,
    IndexMember,
    IndexMemberTable,
    IndexTrackingProduct,
    IndexTrackingTable,
    IpoCalendar,
    IpoEntry,
    LimitBoardLadder,
    LimitBoardLadderRow,
)
from .news import NEWS_CHANNEL_LABELS, NewsChannel, NewsHomeTab

DEFAULT_TQLEX_BASE_URL = "http://static.tdx.com.cn:7615/TQLEX"
DEFAULT_LIMIT_BOARD_LADDER_BASE_URL = "http://hot.icfqs.com:7615/TQLEX"
#: 美股(mgf10) / 港股(hkf10) F10 网关。与 A 股 static 网关不同，page2 带服务端
#: 二级缓存（响应含 ``HitCache`` / ``hitCache:true``），且 skef10_mg/hk Entry
#: 主要在此暴露。
DEFAULT_MG_HK_BASE_URL = "http://page2.tdx.com.cn:7615/TQLEX"
DEFAULT_QSID = "tdx"
LIMIT_BOARD_LADDER_ENTRY = "CWServ.cfg_fx_lbtt"

#: 美股/港股/新三板/债券/行业 F10 Entry 前缀，命中时走 ``mg_hk_base_url``（page2）。
_MG_HK_ENTRY_PREFIXES = (
    "CWServ.skef10_mg_",
    "CWServ.skef10_hk_",
    "CWServ.skef10_sb_",
    "CWServ.skef10_bond_",
    "CWServ.skef10_hy_",
)

#: 期货 F10（qhf10）Entry 前缀（``qhf9_*``），同样只在 page2 网关暴露。
_QH_ENTRY_PREFIXES = ("CWServ.qhf9_",)

#: 财务指标（cwzb）拼音子标签 → 数字参数。美股带前导零，港股不带。
_MG_CWZB_SECTIONS = {"jbzb": "001", "ylnl": "002", "cznl": "003", "yynl": "004", "ccznl": "005"}
_HK_CWZB_SECTIONS = {"jbzb": "1", "ylnl": "2", "cznl": "3", "yynl": "4", "ccznl": "5"}
#: 港股财务分析（cwfx）拼音子标签 → 数字参数（综合损益/资产负债表/现金流量）。
_HK_CWFX_SECTIONS = {"zhsyb": "1", "zcfzb": "2", "xjllb": "3"}

#: ``fund_report`` 里需要报告期（``date``）的分区 → ``skef10_jj_tzzh`` 的报告期列表参数。
#: 未显式传 ``date`` 时，用对应的 ``*bgq`` 参数取最新报告期（与客户端一致）。
_FUND_REPORT_PERIOD_ENTRIES: dict[str, str] = {
    "industry_allocation": "hyfbbgq",
    "bond_allocation": "zczqbgq",
    "futures_allocation": "ccqhmxbgq",
}

#: ``market_report`` 里不需要 ``code`` 的分区（省略或传空串均可）。
_MARKET_REPORT_CODELESS_SECTIONS: frozenset[str] = frozenset(
    {"market_new_shares", "market_capital"}
)

#: ``market_report`` 的语义别名。底层 Entry 名沿用了客户端页面叫法（例如成份股清单
#: 挂在 ``index_flow_members``「成份股资金」下），别名让调用方按语义取名。
_MARKET_REPORT_SECTION_ALIASES: dict[str, str] = {
    "index_members": "index_flow_members",
    "index_etfs": "index_tracking",
    "ipo_calendar": "market_new_shares",
}

#: 分页拉取时的单页条数上限（服务端实测接受 100）。
_MAX_PAGE_SIZE = 100


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
        mg_hk_base_url: str | None = None,
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
        # 美股/港股 F10 默认走 page2（命中服务端缓存）；传 None 时回退到 base_url。
        self.mg_hk_base_url = mg_hk_base_url or DEFAULT_MG_HK_BASE_URL
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

        **注意（指数两融请用 market_report）**：``margin`` / ``margin_history`` 是**个股**两融
        区间通道。拿它查指数（如 ``sh000905`` / ``sh000001``）服务端会按数字代码返回**深市同名
        个股**——实测 ``000905`` → 厦门港务、``000001`` → 平安银行（各约 122 行），不是指数数据。
        指数两融日序列请用 :meth:`market_report` 的 ``market_margin_summary``（``["002", code]``，
        ``000001`` 实测 8676 行、2010-03-31 起）或 ``market_margin_investors``（``["003", window]``）。

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
        ``futures_allocation``（未给时自动取最新报告期）；``etf`` / ``margin`` 未给
        ``date`` 时用 ``"3M"``。

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
        if not date_text and section in _FUND_REPORT_PERIOD_ENTRIES:
            date_text = _latest_report_period(
                self.params(tzzh, _FUND_REPORT_PERIOD_ENTRIES[section], code6, "")
            )
        table: dict[str, tuple[str, tuple[Any, ...]]] = {
            "name": (f"{jj}idreq", ("name", code6)),
            "managers": (jjzl, ("1", code6, "", page_text, page_text, size_text)),
            "company": (jjzl, ("2", code6, "", page_text, page_text, size_text)),
            "basic": (jjzl, ("4", code6, "", page_text, page_text, size_text)),
            "seats": (jjzl, ("5", code6, arg, page_text, page_text, size_text)),
            "scale": (gmfe, ("0", code6, "", "")),
            "scale_history": (gmfe, (arg or "0", code6, "", date_text)),
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

    # ------------------------------------------------------------------
    # 美股 mgf10 家族（Entry: CWServ.skef10_mg_*，网关 page2）
    # ------------------------------------------------------------------

    def mg_name(self, code: str) -> F10Response:
        """美股名称与市场类型（``skef10_mg_idreq``）。"""

        return self.params("CWServ.skef10_mg_idreq", "name", _us_code(code))

    def mg_tips(self, code: str, section: str = "zxdt") -> F10Response:
        """美股最新提示（``skef10_mg_zxdt``）。

        section: ``zxdt`` 最新动态 / ``sypl`` 首页评论 / ``gsgy`` 公司概要 /
        ``zysj`` 重要数据。
        """

        _require_section(section, ("zxdt", "sypl", "gsgy", "zysj"))
        return self.params("CWServ.skef10_mg_zxdt", section, _us_code(code), "")

    def mg_profile(self, code: str) -> F10Response:
        """美股公司概况（``skef10_mg_gsgk``）。"""

        return self.params("CWServ.skef10_mg_gsgk", "001", _us_code(code))

    def mg_finance(self, code: str, section: str = "zcfzb") -> F10Response:
        """美股财务分析（``skef10_mg_cwfx``）。

        section: ``zcfzb`` 资产负债表 / ``lyb`` 利润表 / ``xjllb`` 现金流量表。
        """

        _require_section(section, ("zcfzb", "lyb", "xjllb"))
        return self.params("CWServ.skef10_mg_cwfx", section, _us_code(code))

    def mg_finance_indicator(self, code: str, section: str = "jbzb") -> F10Response:
        """美股财务指标（``skef10_mg_cwzb``）。

        section: ``jbzb`` 基本指标 / ``ylnl`` 营运能力 / ``cznl`` 成长能力 /
        ``yynl`` 盈利能力 / ``ccznl`` 偿债能力（内部映射到数字 ``001``~``005``）。
        """

        _require_section(section, tuple(_MG_CWZB_SECTIONS))
        return self.params("CWServ.skef10_mg_cwzb", _MG_CWZB_SECTIONS[section], _us_code(code))

    def mg_business(self, code: str) -> F10Response:
        """美股经营分析（``skef10_mg_jyfx``）。"""

        return self.params("CWServ.skef10_mg_jyfx", _us_code(code))

    def mg_industry(self, code: str) -> F10Response:
        """美股行业分析（``skef10_mg_hyfx``）。"""

        return self.params("CWServ.skef10_mg_hyfx", "001", _us_code(code), "4", "1")

    def mg_industry_compare(self, code: str) -> F10Response:
        """美股行业对比（``skef10_mg_hydb``）。"""

        return self.params("CWServ.skef10_mg_hydb", "001", _us_code(code), "", "0", "0")

    def mg_dividend(self, code: str, section: str = "fh") -> F10Response:
        """美股分红融资（``skef10_mg_fhrz``）。

        section: ``fh`` 分红 / ``cf`` 融资。
        """

        _require_section(section, ("fh", "cf"))
        return self.params("CWServ.skef10_mg_fhrz", _us_code(code), section)

    def mg_capital(self, code: str, section: str = "001", page: int = 1) -> F10Response:
        """美股股本结构（``skef10_mg_gbjg``）。

        section: ``001`` 股本结构 / ``002`` 股本变动公告。
        """

        _require_section(section, ("001", "002"))
        if section == "001":
            return self.params("CWServ.skef10_mg_gbjg", "001", _us_code(code), 1)
        return self.params("CWServ.skef10_mg_gbjg", "002", _us_code(code), page)

    def mg_shareholders(
        self,
        code: str,
        section: str = "006",
        *,
        page: int = 1,
        page_size: int = 10,
    ) -> F10Response:
        """美股股东权益（``skef10_mg_gbgd``）。

        section 为数字子标签（``003``/``004``/``005``/``006``/``007``/
        ``008``/``011``/``014`` 等）；``006`` 为股东权益主表，其余带分页。
        """

        if section == "006":
            return self.params("CWServ.skef10_mg_gbgd", "006", _us_code(code), 1, 10)
        return self.params("CWServ.skef10_mg_gbgd", section, _us_code(code), page, page_size)

    def mg_directors(self, code: str, section: str = "ds") -> F10Response:
        """美股董事高管（``skef10_mg_dsgg``）。

        section: ``ds`` 董事 / ``gg`` 高管 / ``lrdjg`` 历任董监高。
        """

        _require_section(section, ("ds", "gg", "lrdjg"))
        return self.params("CWServ.skef10_mg_dsgg", _us_code(code), section)

    def mg_market(
        self,
        code: str,
        section: str = "001",
        *,
        page: int = 1,
        page_size: int = 20,
    ) -> F10Response:
        """美股市场行情（``skef10_mg_schq``）。

        section: ``001`` 行情 / ``003`` 资金流向。
        """

        _require_section(section, ("001", "003"))
        if section == "001":
            return self.params("CWServ.skef10_mg_schq", "001", _us_code(code), page, page_size)
        return self.params("CWServ.skef10_mg_schq", "003", _us_code(code), 0, 0)

    def mg_announcements(self, code: str) -> F10Response:
        """美股公司公告（``skef10_mg_gsbd``）。"""

        return self.params("CWServ.skef10_mg_gsbd", "gsgg", _us_code(code))

    def mg_rating(self, code: str, section: str = "001") -> F10Response:
        """美股公司评级（``skef10_mg_gspj``）。section: ``001``/``002``/``003``。"""

        return self.params("CWServ.skef10_mg_gspj", section, _us_code(code))

    def mg_topic(self, code: str) -> F10Response:
        """美股热点题材（``skef10_mg_rdtc``）。"""

        return self.params("CWServ.skef10_mg_rdtc", "001", _us_code(code), "", "4", "1")

    def mg_deep_analysis(self, code: str, section: str = "001") -> F10Response:
        """美股深度分析（``skef10_mg_sdfx``）。section: ``001``/``002``。"""

        return self.params("CWServ.skef10_mg_sdfx", section, _us_code(code))

    # ------------------------------------------------------------------
    # 港股 hkf10 家族（Entry: CWServ.skef10_hk_*，网关 page2）
    # ------------------------------------------------------------------

    def hk_name(self, code: str) -> F10Response:
        """港股名称与市场类型（``skef10_hk_idreq``）。"""

        return self.params("CWServ.skef10_hk_idreq", "name", _hk_code(code))

    def hk_tips(self, code: str, section: str = "1") -> F10Response:
        """港股最新提示（``skef10_hk_zxts``）。section 为数字子标签 ``1``~``5``。"""

        return self.params("CWServ.skef10_hk_zxts", section, _hk_code(code))

    def hk_events(self, code: str, section: str = "1") -> F10Response:
        """港股公司大事（``skef10_hk_gsds``）。section 为数字子标签 ``1``~``6``。"""

        return self.params("CWServ.skef10_hk_gsds", section, _hk_code(code))

    def hk_profile(self, code: str, section: str = "1") -> F10Response:
        """港股公司概况（``skef10_hk_gsgk``）。section 为数字子标签 ``1``~``6``。"""

        return self.params("CWServ.skef10_hk_gsgk", section, _hk_code(code))

    def hk_related(self, code: str, section: str = "6") -> F10Response:
        """港股关联证券（``skef10_hk_glzq``）。section 为数字子标签（如 ``6``）。"""

        return self.params("CWServ.skef10_hk_glzq", section, _hk_code(code))

    def hk_executives(self, code: str, section: str = "1") -> F10Response:
        """港股高管资料（``skef10_hk_ggzl``）。section: ``1``/``2``/``3``。"""

        return self.params("CWServ.skef10_hk_ggzl", section, _hk_code(code))

    def hk_capital(self, code: str, section: str = "1") -> F10Response:
        """港股股本结构（``skef10_hk_gbjg``）。section: ``1``/``2``/``3``。"""

        return self.params("CWServ.skef10_hk_gbjg", section, _hk_code(code))

    def hk_shareholders(self, code: str, section: str = "1") -> F10Response:
        """港股股东持股（``skef10_hk_gdcg``）。section 为数字子标签 ``1``~``13``。"""

        return self.params("CWServ.skef10_hk_gdcg", section, _hk_code(code), "")

    def hk_finance(self, code: str, section: str = "zcfzb") -> F10Response:
        """港股财务分析（``skef10_hk_cwfx``）。

        section: ``zcfzb`` 资产负债表 / ``zhsyb`` 综合损益表 / ``xjllb`` 现金流量表
        （内部映射到数字 ``2``/``1``/``3``）。
        """

        _require_section(section, tuple(_HK_CWFX_SECTIONS))
        return self.params("CWServ.skef10_hk_cwfx", _HK_CWFX_SECTIONS[section], _hk_code(code))

    def hk_finance_indicator(self, code: str, section: str = "jbzb") -> F10Response:
        """港股财务指标（``skef10_hk_cwzb``）。

        section: ``jbzb`` 基本指标 / ``ylnl`` 营运能力 / ``cznl`` 成长能力 /
        ``yynl`` 盈利能力 / ``ccznl`` 偿债能力（内部映射到数字 ``1``~``5``）。
        """

        _require_section(section, tuple(_HK_CWZB_SECTIONS))
        return self.params("CWServ.skef10_hk_cwzb", _HK_CWZB_SECTIONS[section], _hk_code(code))

    def hk_business(self, code: str, section: str = "1") -> F10Response:
        """港股经营分析（``skef10_hk_jyfx``）。section 为数字子标签。"""

        return self.params("CWServ.skef10_hk_jyfx", section, _hk_code(code))

    def hk_industry_compare(self, code: str, section: str = "1") -> F10Response:
        """港股行业比较（``skef10_hk_hybj``）。section: ``1``/``3``。"""

        return self.params("CWServ.skef10_hk_hybj", section, _hk_code(code), "0", "0")

    def hk_market(self, code: str, section: str = "3") -> F10Response:
        """港股市场行情（``skef10_hk_schq``）。section 为数字子标签。

        实测（2026-10-22，00700）：``3``~``4``~``6``~``7`` 有数据
        （245/246/242/51 行），``1``/``2``/``5`` 对该股返回空
        （子表为空或数据缺失）；默认取数据最全的 ``3``。
        """

        return self.params("CWServ.skef10_hk_schq", section, _hk_code(code))

    def hk_capital_flow(self, code: str, section: str = "001") -> F10Response:
        """港股资金流向（``skef10_hk_schq_zjlx``，市场行情页的资金流向表）。

        实测（2026-10-22，00700）：``001`` 20 行，``003`` 空（图表数据）。
        """

        return self.params("CWServ.skef10_hk_schq_zjlx", section, _hk_code(code), "0")

    def hk_news(self, code: str, section: str = "3") -> F10Response:
        """港股公司资讯（``skef10_hk_gszx``）。section: ``3``/``4``。"""

        return self.params("CWServ.skef10_hk_gszx", section, _hk_code(code))

    def hk_topic(self, code: str) -> F10Response:
        """港股热点题材（``skef10_hk_rdtc``）。"""

        return self.params("CWServ.skef10_hk_rdtc", "001", _hk_code(code), "", "0", "0")

    def hk_pepb_band(
        self,
        code: str,
        *,
        sjfw: str = "",
        tab_value: str = "",
        hqzq: str = "",
    ) -> F10Response:
        """港股深度分析·PE/PB band（``skef10_hk_sdfx_pepb_band``）。"""

        return self.params(
            "CWServ.skef10_hk_sdfx_pepb_band", "001", _hk_code(code), sjfw, tab_value, hqzq
        )

    def hk_valuation(self, code: str, item_id: str = "") -> F10Response:
        """港股深度分析·估值分析（``skef10_hk_sdfx_gzfx``）。"""

        return self.params("CWServ.skef10_hk_sdfx_gzfx", _hk_code(code), item_id)

    def hk_price_chart(self, code: str) -> F10Response:
        """港股深度分析·行情涨跌图（``skef10_hk_sdfx_hqzft``）。"""

        return self.params("CWServ.skef10_hk_sdfx_hqzft", _hk_code(code))

    def hk_analyst_forecast(self, code: str, start: str = "", end: str = "") -> F10Response:
        """港股深度分析·分析师盈利预测（``skef10_hk_sdfx_fxysyfx``）。

        两个日期参数会去掉 ``-`` 后传给服务端（与客户端一致）。
        **实测空日期返回 0 行**（2026-10-22，00700）：不传时默认取近一年
        （今天往前 365 天 ~ 今天），近一年区间实测 53 行。
        """

        if not start or not end:
            today = date.today()
            start = start or today.replace(year=today.year - 1).isoformat()
            end = end or today.isoformat()
        return self.params(
            "CWServ.skef10_hk_sdfx_fxysyfx",
            _hk_code(code),
            start.strip().replace("-", ""),
            end.strip().replace("-", ""),
        )

    # ------------------------------------------------------------------
    # 新三板 sbf10 家族（Entry: CWServ.skef10_sb_*，网关 page2）
    #
    # 代码为 6 位数字（如 ``430047``），不带市场前缀；Params 统一为
    # ``[code, section]``。注意新三板的公司概况/财务也可走 A 股家族
    # （``tdxf10_gg_*``），sb 家族是其补充（股本/股东/行业比较/现金流等）。
    # ------------------------------------------------------------------

    def sb_name(self, code: str) -> F10Response:
        """新三板名称与市场类型（``skef10_sb_idreq``）。"""

        return self.params("CWServ.skef10_sb_idreq", "name", _code6(code))

    def sb_profile(self, code: str, section: str = "001") -> F10Response:
        """新三板公司概况（``skef10_sb_gsgk``）。section 为子标签。"""

        return self.params("CWServ.skef10_sb_gsgk", _code6(code), section)

    def sb_tips(self, code: str, section: str = "001") -> F10Response:
        """新三板最新提示（``skef10_sb_zxts``）。section 为子标签。"""

        return self.params("CWServ.skef10_sb_zxts", _code6(code), section)

    def sb_events(self, code: str, section: str = "ydjy") -> F10Response:
        """新三板交易大事（``skef10_sb_jyds``）。section: ``ydjy`` 异动交易。"""

        return self.params("CWServ.skef10_sb_jyds", _code6(code), section)

    def sb_capital(self, code: str, section: str = "gbjg") -> F10Response:
        """新三板股本结构（``skef10_sb_gbjg``）。section: ``gbjg`` 股本结构。"""

        return self.params("CWServ.skef10_sb_gbjg", _code6(code), section)

    def sb_shareholders(self, code: str, section: str = "sdgd") -> F10Response:
        """新三板股东研究（``skef10_sb_gdyj``）。section: ``sdgd`` 十大股东。"""

        return self.params("CWServ.skef10_sb_gdyj", _code6(code), section)

    def sb_compliance(self, code: str, section: str = "001") -> F10Response:
        """新三板合规运营（``skef10_sb_hgyy``）。section 为子标签。"""

        return self.params("CWServ.skef10_sb_hgyy", _code6(code), section)

    def sb_dividend(self, code: str, section: str = "fh") -> F10Response:
        """新三板分红融资（``skef10_sb_fhrz``）。section 为子标签。"""

        return self.params("CWServ.skef10_sb_fhrz", _code6(code), section)

    def sb_finance(self, code: str, section: str = "zcfzb") -> F10Response:
        """新三板财务分析（``skef10_sb_cwfx``）。

        section: ``zcfzb`` 资产负债表 / ``xjllb`` 现金流量表。
        """

        return self.params("CWServ.skef10_sb_cwfx", _code6(code), section)

    def sb_business(self, code: str, section: str = "001") -> F10Response:
        """新三板经营分析（``skef10_sb_jyfx``）。section 为子标签。"""

        return self.params("CWServ.skef10_sb_jyfx", _code6(code), section)

    def sb_major_events(self, code: str, section: str = "001") -> F10Response:
        """新三板重大事项（``skef10_sb_zdsx``）。section 为子标签。"""

        return self.params("CWServ.skef10_sb_zdsx", _code6(code), section)

    def sb_news(self, code: str, section: str = "001") -> F10Response:
        """新三板公司资讯（``skef10_sb_gszx``）。section 为子标签。"""

        return self.params("CWServ.skef10_sb_gszx", _code6(code), section)

    def sb_industry_compare(self, code: str, section: str = "hyxx") -> F10Response:
        """新三板行业比较（``skef10_sb_hybj``）。section: ``hyxx`` 行业信息。"""

        return self.params("CWServ.skef10_sb_hybj", _code6(code), section)

    # ------------------------------------------------------------------
    # 债券 zqf10 家族（Entry: CWServ.skef10_bond_*，网关 page2）
    #
    # 通达信把债券 F10 按类型分 8 个子家族（gzhg 国债回购 / gz 国债 / gzdfz
    # 地方债 / gzzcz / kzz 可转债 / qyz 企业债 / gsz 公司债 / kjhz 可交换债），
    # 此处先封装最常用的**可转债（kzz）**；其余类型的 Entry 命名同构
    # （``skef10_bond_<type>_<menu>``），可仿照扩展。
    # 代码为 6 位数字（如 ``113043``），沪市债券段见 normalize_code。
    # ------------------------------------------------------------------

    def bond_profile(self, code: str) -> F10Response:
        """可转债债券概况（``skef10_bond_kzz_zqgk``）。"""

        return self.params("CWServ.skef10_bond_kzz_zqgk", "sdcyrbgq", _code6(code), "")

    def bond_finance(self, code: str, section: str = "zcfzb") -> F10Response:
        """可转债财务分析（``skef10_bond_kzz_cwfx``）。section 为子标签。"""

        return self.params("CWServ.skef10_bond_kzz_cwfx", section, _code6(code))

    def bond_rating(self, code: str, section: str = "pjzst") -> F10Response:
        """可转债债券评级（``skef10_bond_kzz_zqpj``）。

        section: ``pjzst`` 评级状态 / ``fxjgzst`` 发行机构评级状态。
        """

        return self.params("CWServ.skef10_bond_kzz_zqpj", section, _code6(code))

    def bond_conversion(self, code: str, section: str = "zgjg") -> F10Response:
        """可转债转股情况（``skef10_bond_kzz_zg``）。

        section: ``zgjg`` 转股价格 / ``zgjgzs`` 转股结果 / ``zgqk`` 转股情况。
        """

        return self.params("CWServ.skef10_bond_kzz_zg", section, _code6(code))

    def bond_rate(self, code: str, section: str = "zqll") -> F10Response:
        """可转债债券利率（``skef10_bond_kzz_zqll``）。

        section: ``zqll`` 债券利率 / ``zbz`` 转股价。
        """

        return self.params("CWServ.skef10_bond_kzz_zqll", section, _code6(code))

    def bond_announcements(self, code: str, section: str = "1") -> F10Response:
        """可转债资金资讯 / 公告（``skef10_bond_kzz_zjzx``）。section 为子标签。"""

        return self.params("CWServ.skef10_bond_kzz_zjzx", _code6(code), section)

    # ------------------------------------------------------------------
    # 债券 zqf10 家族·其余类型（国债 gz / 国债回购 gzhg / 企业债 qyz /
    # 公司债 gsz（复用 qyz Entry）/ 可交换债 kjhz / ABS zczq）
    #
    # 参数模式（实测确认）：
    # * 概况/财务/评级/利率等：``[section, code, sc]``，``sc`` 为市场码
    #   （``1`` 沪 / ``0`` 深，与 MacClient 的 market 一致）；
    # * 国债回购：``[section, code]``（无 sc）；
    # * 债券公告（zjzx）：``[code, sc]``（无 section）。
    # 各 section 语义见各方法 docstring；子标签命名与客户端 HTML 一致。
    # ------------------------------------------------------------------

    def gz_profile(self, code: str, section: str = "zqgk") -> F10Response:
        """国债/地方政府债债券概况（``skef10_bond_gz_zjgk``）。

        section: ``zqgk`` 债券概况 / ``fxgk`` 发行概况。
        """

        return self.params(
            "CWServ.skef10_bond_gz_zjgk", section, _bond_code(code), _bond_sc(code)
        )

    def gz_interest(self, code: str, section: str = "fxqk") -> F10Response:
        """国债付息情况（``skef10_bond_gz_fxqk``）。

        section: ``fxqk`` 付息情况 / ``xjlfx`` 付息兑付。
        """

        return self.params(
            "CWServ.skef10_bond_gz_fxqk", section, _bond_code(code), _bond_sc(code)
        )

    def gz_market_data(self, code: str, section: str = "dzjy") -> F10Response:
        """国债债券数据（``skef10_bond_gz_zjsj``）。

        section: ``dzjy`` 大宗交易 / ``zsbl`` 招标比例。
        """

        return self.params(
            "CWServ.skef10_bond_gz_zjsj", section, _bond_code(code), _bond_sc(code)
        )

    def gz_rating(self, code: str, section: str = "dfzf") -> F10Response:
        """国债/地方债债券评级（``skef10_bond_gz_zjpj``）。

        section: ``dfzf`` 地方债评级。
        """

        return self.params(
            "CWServ.skef10_bond_gz_zjpj", section, _bond_code(code), _bond_sc(code)
        )

    def gz_finance(self, code: str) -> F10Response:
        """国债财务分析（``skef10_bond_gz_cwfx``）。"""

        return self.params("CWServ.skef10_bond_gz_cwfx", "t", _bond_code(code), _bond_sc(code))

    def qyz_profile(self, code: str, section: str = "zqzl") -> F10Response:
        """企业债/公司债债券概况（``skef10_bond_qyz_zqgk``）。

        section: ``zqzl`` 债券资料 / ``xtfhr`` 信托发行人 / ``fhrjbzl`` 发行人基本信息。
        """

        return self.params(
            "CWServ.skef10_bond_qyz_zqgk", section, _bond_code(code), _bond_sc(code)
        )

    def qyz_interest(self, code: str, section: str = "fxqk") -> F10Response:
        """企业债/公司债付息兑付（``skef10_bond_qyz_xjlfx``）。

        section: ``fxqk`` 付息情况 / ``xjlfx`` 付息兑付 / ``febd`` 份额变动。
        """

        return self.params(
            "CWServ.skef10_bond_qyz_xjlfx", section, _bond_code(code), _bond_sc(code)
        )

    def qyz_guarantee(self, code: str) -> F10Response:
        """企业债/公司债债券担保（``skef10_bond_qyz_zqdb``）。"""

        return self.params(
            "CWServ.skef10_bond_qyz_zqdb", "dbsm", _bond_code(code), _bond_sc(code)
        )

    def qyz_rating(self, code: str, section: str = "zjpjzst") -> F10Response:
        """企业债/公司债债券评级（``skef10_bond_qyz_zjpj``）。

        section: ``zjpjzst`` 中债评级状态 / ``zjxypj`` 中债信用评级 /
        ``fxjgpj`` 发行机构评级 / ``fxjgzst`` 发行机构评级状态。
        """

        return self.params(
            "CWServ.skef10_bond_qyz_zjpj", section, _bond_code(code), _bond_sc(code)
        )

    def qyz_rate(self, code: str) -> F10Response:
        """企业债/公司债债券利率（``skef10_bond_qyz_zjlv``）。"""

        return self.params(
            "CWServ.skef10_bond_qyz_zjlv", "zqlv", _bond_code(code), _bond_sc(code)
        )

    def qyz_review(self, code: str) -> F10Response:
        """公司债公司评述（``skef10_bond_qyz_gsps``）。"""

        return self.params(
            "CWServ.skef10_bond_qyz_gsps", _bond_code(code), _bond_sc(code)
        )

    def gzhg_info(self, code: str) -> F10Response:
        """国债回购债券信息（``skef10_bond_gzhg_zqxx``）。"""

        return self.params("CWServ.skef10_bond_gzhg_zqxx", "zqgk", _code6(code))

    def gzhg_quotes(self, code: str, section: str = "bpzs") -> F10Response:
        """国债回购债券数据（``skef10_bond_gzhg_zqxx``）。

        section: ``bpzs`` 报价张数 / ``hgzs`` 回购张数 / ``zqsj`` 债券数据 /
        ``hgdpll`` 回购抵押利率。
        """

        return self.params("CWServ.skef10_bond_gzhg_zqxx", section, _code6(code))

    def zczq_profile(self, code: str) -> F10Response:
        """ABS 债券概况（``skef10_bond_zczqh_zqgk``）。"""

        return self.params(
            "CWServ.skef10_bond_zczqh_zqgk", _bond_code(code), _bond_sc(code)
        )

    def zczq_finance(self, code: str) -> F10Response:
        """ABS 财务分析（``skef10_bond_zczqh_cwfx``）。"""

        return self.params(
            "CWServ.skef10_bond_zczqh_cwfx", "t", _bond_code(code), _bond_sc(code)
        )

    def zczq_rating(self, code: str, section: str = "pjzst") -> F10Response:
        """ABS 债券评级（``skef10_bond_zczqh_zqpj``）。

        section: ``pjzst`` 评级状态 / ``fxjgzst`` 发行机构评级状态。
        """

        return self.params(
            "CWServ.skef10_bond_zczqh_zqpj", section, _bond_code(code), _bond_sc(code)
        )

    def kjhz_profile(self, code: str) -> F10Response:
        """可交换债债券概况（``skef10_bond_kjhz_zqgk``）。"""

        return self.params(
            "CWServ.skef10_bond_kjhz_zqgk", _bond_code(code), _bond_sc(code)
        )

    def kjhz_finance(self, code: str) -> F10Response:
        """可交换债财务分析（``skef10_bond_kjhz_cwfx``）。"""

        return self.params("CWServ.skef10_bond_kjhz_cwfx", "t", _bond_code(code))

    def kjhz_interest(self, code: str, section: str = "fxqk") -> F10Response:
        """可交换债付息情况（``skef10_bond_kjhz_fxqk``）。

        section: ``fxqk`` 付息情况 / ``xjlfx`` 付息兑付。
        """

        return self.params("CWServ.skef10_bond_kjhz_fxqk", section, _bond_code(code))

    def kjhz_guarantee(self, code: str) -> F10Response:
        """可交换债债券担保（``skef10_bond_kjhz_zqdb``）。"""

        return self.params("CWServ.skef10_bond_kjhz_zqdb", "dbsm", _bond_code(code))

    def kjhz_rating(self, code: str, section: str = "pjzst") -> F10Response:
        """可交换债债券评级（``skef10_bond_kjhz_zqpj``）。

        section: ``pjzst`` 评级状态 / ``fxjgzst`` 发行机构评级状态 /
        ``fxjgpj`` 发行机构评级 / ``xypj`` 信用评级。
        """

        return self.params("CWServ.skef10_bond_kjhz_zqpj", section, _bond_code(code))

    def kjhz_exchange(self, code: str, section: str = "jgtzzst") -> F10Response:
        """可交换债交换情况（``skef10_bond_kjhz_jhqk``）。

        section: ``jgtzzst`` 交换投资状态 / ``jhjbqk`` 交换基本情况 /
        ``jhjgtz`` 交换价格。
        """

        return self.params("CWServ.skef10_bond_kjhz_jhqk", section, _bond_code(code))

    def kjhz_rate(self, code: str, section: str = "zqll") -> F10Response:
        """可交换债债券利率（``skef10_bond_kjhz_zqll``）。

        section: ``zqll`` 债券利率 / ``zst`` 指数。
        """

        return self.params("CWServ.skef10_bond_kjhz_zqll", section, _bond_code(code))

    def bond_terms(self, code: str, section: str = "hstk") -> F10Response:
        """债券条款（``skef10_bond_kzz_lltk``，可转债/可交换债共用）。

        section: ``hstk`` 回售条款 / ``shtk`` 赎回条款 / ``xztk`` 下修条款 /
        ``zqll`` 债券利率。
        """

        return self.params("CWServ.skef10_bond_kzz_lltk", section, _code6(code))

    def bond_bulletin(self, code: str) -> F10Response:
        """债券公告 / 资讯（``skef10_bond_zjzx``，各类型通用）。"""

        return self.params("CWServ.skef10_bond_zjzx", _bond_code(code), _bond_sc(code))

    # ------------------------------------------------------------------
    # 行业 hyf10 家族（Entry: CWServ.skef10_hy_*，网关 page2）
    #
    # ``hy`` 为通达信行业板块代码（6 位数字，如 ``881319`` 半导体，
    # 可由 MacClient.get_board_list() 获取）。参数模式：
    # * ``[hy]``：总览/介绍/名单类；
    # * ``[section, hy]``：排名/数据类（section 为子标签）；
    # * ``[section, hy, flag]``：部分数据类带方向/周期标志。
    # ------------------------------------------------------------------

    def hy_summary(self, hy: str, section: str = "hqtj") -> F10Response:
        """行业总览（``skef10_hy_*``，一页多表）。

        section: ``hqtj`` 行情统计 / ``hyzq`` 行业行情 / ``hygy`` 行业供应 /
        ``zszfb`` 指数分布。
        """

        entry = {
            "hqtj": "CWServ.skef10_hy_hqtj",
            "hyzq": "CWServ.skef10_hy_hyzq",
            "hygy": "CWServ.skef10_hy_hygy",
            "zszfb": "CWServ.skef10_hy_zszfb",
        }[section]
        return self.params(entry, hy)

    def hy_stage_rise(self, hy: str, flag: str = "e") -> F10Response:
        """行业阶段涨幅（``skef10_hy_jdzf``）。"""

        return self.params("CWServ.skef10_hy_jdzf", hy, flag)

    def hy_profile(self, hy: str) -> F10Response:
        """行业介绍（``skef10_hy_hyzl_hyjs``）。"""

        return self.params("CWServ.skef10_hy_hyzl_hyjs", hy)

    def hy_commentary(self, hy: str, section: str = "001") -> F10Response:
        """行业评论库（``skef10_hy_hyzl_hyplk``）。section 为子标签。"""

        return self.params("CWServ.skef10_hy_hyzl_hyplk", section, hy)

    def hy_news(self, hy: str, section: str = "hydt") -> F10Response:
        """行业最新动态（``skef10_hy_zxdt``）。

        section: ``hydt`` 行业动态 / ``hyzt`` 行业专题。
        """

        return self.params("CWServ.skef10_hy_zxdt", section, hy)

    def hy_members_rank(self, hy: str, section: str = "cwzkpm") -> F10Response:
        """行业成份股排名（5 个 Entry 共用 section 语义）。

        section: ``cwzkpm`` 财务指标排名（Params ``["002", hy, "", "e", ""]``）/
        ``gsgmpm`` 股本规模排名 / ``gzsppm`` 估值水平排名（``01`` 市盈率/``02`` 市净率）/
        ``scbxpm`` 市场表现排名（``scpm`` 涨跌幅/``sczf`` 涨跌幅方向）/
        ``zysrpm`` 主营收入排名。
        """

        if section == "cwzkpm":
            return self.params("CWServ.skef10_hy_hydw_cwzkpm", "002", hy, "", "e", "")
        if section == "gsgmpm":
            return self.params("CWServ.skef10_hy_hydw_gsgmpm", hy)
        if section == "gzsppm":
            return self.params("CWServ.skef10_hy_hydw_gzsppm", "01", hy, "o")
        if section == "scbxpm":
            return self.params("CWServ.skef10_hy_hydw_scbxpm", "scpm", "", hy)
        if section == "zysrpm":
            return self.params("CWServ.skef10_hy_hydw_zysrpm", "001", hy, "")
        raise ValueError(f"unknown hy_members_rank section: {section!r}")

    def hy_capital_stats(self, hy: str, section: str = "001") -> F10Response:
        """行业板块资金统计（``skef10_hy_hysj_bxzjtj``）。

        section: ``001`` 板块资金 / ``002`` 板块资金（另一种口径）。
        """

        return self.params("CWServ.skef10_hy_hysj_bxzjtj", section, hy, "n" if section == "001" else "")

    def hy_fund_holdings(self, hy: str, section: str = "1") -> F10Response:
        """行业基金持仓（``skef10_hy_hysj_jjzb``）。section 为子标签。"""

        return self.params("CWServ.skef10_hy_hysj_jjzb", section, hy, "")

    def hy_capital_flow(self, hy: str, section: str = "001") -> F10Response:
        """行业资金流向（``skef10_hy_hysj_zjlx``）。

        section: ``001`` 资金流向 / ``002`` 资金流向（另一种口径）。
        """

        return self.params("CWServ.skef10_hy_hysj_zjlx", section, hy)

    def hy_analyst_index(self, hy: str, section: str = "001") -> F10Response:
        """行业分析师指数（``skef10_hy_hyyj_fxszs``）。section 为子标签。"""

        return self.params("CWServ.skef10_hy_hyyj_fxszs", section, hy, "", "")

    def hy_major_holders(self, hy: str) -> F10Response:
        """行业某名单（``skef10_hy_hyyj_hymzgd``）。"""

        return self.params("CWServ.skef10_hy_hyyj_hymzgd", hy)

    def hy_rating_summary(self, hy: str, section: str = "001") -> F10Response:
        """行业评级汇总（``skef10_hy_hyyj_hypjhz``）。section 为子标签。"""

        return self.params("CWServ.skef10_hy_hyyj_hypjhz", section, hy, "")

    def hy_index(self, hy: str) -> F10Response:
        """行业指数（``skef10_hy_idsy``）。"""

        return self.params("CWServ.skef10_hy_idsy", "002", hy)

    # ------------------------------------------------------------------
    # 期货 qhf10 家族（Entry: CWServ.qhf9_*，网关 page2）
    #
    # JS 里 Entry 写为 ``qhf9_*``（不带前缀），由 req.js 的 CallTQL 自动补
    # ``CWServ.``；区分商品（``qhf9_sp_*``）与股指（``qhf9_gz_*``）两套。
    # ``code`` 为品种代码（商品如 ``rb`` 螺纹钢；股指如 ``IF``/``IC``）。
    # ------------------------------------------------------------------

    def qh_position_report(
        self,
        code: str,
        section: str = "cjcc",
        *,
        date: str = "",
        end_date: str = "",
        index: bool = False,
    ) -> F10Response:
        """期货持仓报告（``CWServ.qhf9_*_ccbg_*``）。

        section: ``cjcc`` 仓差层次 / ``cjjg`` 持仓结构 / ``jcgc`` 会员席位 /
        ``ccjj`` 成交均价 / ``ykfx`` 盈亏分析。
        ``cjcc``/``cjjg`` 只需 ``date``（单日）；``jcgc``/``ccjj``/``ykfx``
        需要日期区间 ``date``~``end_date``（实测空日期返回 0 行）。
        ``index=True`` 走股指（``qhf9_gz_*``）Entry，否则商品（``qhf9_sp_*``）。
        """

        prefix = "CWServ.qhf9_gz_ccbg_" if index else "CWServ.qhf9_sp_ccbg_"
        suffix = {"cjcc": "cjcc", "cjjg": "cjjg", "jcgc": "jcgc", "ccjj": "ccjj", "ykfx": "ykfx"}[section]
        d1 = date.strip().replace("-", "")
        d2 = end_date.strip().replace("-", "")
        if section in ("cjcc",):
            return self.params(prefix + suffix, "1", code, d1)
        if section == "cjjg":
            return self.params(prefix + suffix, "1", code, d1, "")
        return self.params(prefix + suffix, "1", code, d1, d2, "")

    def qh_basis_spread(self, code: str, section: str = "001", period: str = "1Y") -> F10Response:
        """期货基差价差分析（``qhf9_sp_jcjcfx``）。

        section: ``001`` 基差走势 / ``002`` 基差数据（``period`` 周期）/
        ``003`` 价差日期列表 / ``004`` 价差数据 / ``005`` 跨期价差列表 /
        ``006`` 跨品种价差。
        """

        return self.params("CWServ.qhf9_sp_jcjcfx", section, code, period, "", "")

    def qh_warehouse_receipt(self, code: str, *, date: str = "") -> F10Response:
        """期货仓单日报（``CWServ.qhf9_sp_ckrb_ckrb``）。

        实测需传日期（``YYYYMMDD``），空日期返回 0 行。
        """

        return self.params("CWServ.qhf9_sp_ckrb_ckrb", "1", code, date.strip().replace("-", ""))

    def qh_inventory(self, code: str) -> F10Response:
        """期货库存报告（``CWServ.qhf9_sp_kcbg_kcbg``）。

        注意：实测部分品种返回 0 行，参数形式可能因品种而异，
        调用前建议先用 ``params`` 直接试探。
        """

        return self.params("CWServ.qhf9_sp_kcbg_kcbg", "1", code)

    def qh_variety_info(self, code: str, section: str = "pzjj") -> F10Response:
        """期货品种资料（``CWServ.qhf9_*_pzzl_*``）。

        section: ``pzjj`` 品种简介（``[code, "1"]``）/ ``hyjj`` 行业简介 /
        ``xgzq`` 相关证券（后两者为 ``[code]`` 单参）。
        """

        if section == "pzjj":
            return self.params("CWServ.qhf9_sp_pzzl_pzjj", code, "1")
        entry = {
            "hyjj": "CWServ.qhf9_sp_pzzl_hyjj",
            "xgzq": "CWServ.qhf9_sp_pzzl_xgzq",
        }[section]
        return self.params(entry, code)

    def qh_index_info(self, code: str, section: str = "zsgk") -> F10Response:
        """股指期货指数资料（``CWServ.qhf9_gz_zszl_*``）。

        section: ``zsgk`` 指数概况 / ``lsjg`` 历史价格。
        ``code`` 为指数代码（实测 ``399300`` 沪深300 可用，``000300`` 返回空）。
        """

        return self.params("CWServ.qhf9_gz_zszl_" + section, "1" if section == "zsgk" else "2", code)

    def qh_index_valuation(
        self, code: str = "000300", section: str = "dtpe", *, start: str = "", end: str = ""
    ) -> F10Response:
        """股指期货指数估值（``CWServ.qhf9_gz_zsgz_*``）。

        section: ``dtpe`` 动态市盈率 / ``lspe`` 历史市盈率 / ``xjfh`` 现金分红。
        ``code`` 为指数代码（客户端写死 ``000300`` 沪深300）；``start``/``end``
        为日期区间（``YYYYMMDD``）。
        """

        return self.params(
            "CWServ.qhf9_gz_zsgz_" + section,
            "1",
            code,
            start.strip().replace("-", ""),
            end.strip().replace("-", ""),
        )

    def qh_weights(self, code: str = "399300", section: str = "zxcfg", *, date: str = "") -> F10Response:
        """股指期货成份权重（``CWServ.qhf9_gz_cfg_*``）。

        section: ``zxcfg`` 最新成份 / ``lscfg`` 历史成份 / ``cfgqz`` 成份权重 /
        ``cfghyfb`` 成份行业分布 / ``xwpl`` 新闻评论。
        ``code`` 为指数代码（客户端用 ``zsinfo.code``，IF→``399300``）；
        ``date`` 为截止日期（``YYYYMMDD``，空为最新）。
        """

        d = date.strip().replace("-", "")
        if section == "cfghyfb":
            return self.params("CWServ.qhf9_gz_cfg_cfghyfb", "1", code)
        return self.params("CWServ.qhf9_gz_cfg_" + section, "1", code, d)

    def market_report(
        self,
        code: str = "",
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
        ``index_info`` 指数信息；``index_profile`` 指数概况；``index_valuation`` 指数估值快照
        （Entry ``CWServ.skef10_zs_jbxx_gzxx``，Params ``["001", code, "", ""]``：``N001`` 市盈率TTM /
        ``N002`` 市盈率LYR / ``N003`` 市净率MRQ / ``N004`` 市净率加权 / ``N005`` 市销率TTM /
        ``N006`` 市销率LYR / ``N007`` 市现率TTM / ``N008`` 市现率LYR / ``N009`` 股息率% /
        ``N010`` 净资产收益率（按板块版 ``board_analysis`` 同名列推断）/
        ``N010__2`` 报告期（服务端两列同名 ``N010``，解析时加 ``__2`` 区分））；``index_tracking`` 跟踪标的；
        ``index_flow`` 指数资金动向（``arg`` 默认 ``"004_1"``）；
        ``index_flow_members`` 成份股资金（``arg`` 默认 ``"001"``）；
        ``index_notices`` 今日提示（``arg`` 默认 ``"001"``）；
        ``market_basic`` 大盘指数基本信息；``market_margin`` 大盘两融**活跃个股列表**
        （Entry ``CWServ.skef10_dpzx_rzrq``，Params ``["001", code]``，``000001`` 实测 1676 行）；
        ``market_margin_summary`` 大盘两融**日序列汇总**（同一 Entry，Params ``["002", code]``，
        ``000001`` 实测 8676 行、2010-03-31 起；``N010`` 是三市标记 ``012001`` 沪 / ``012002`` 深 /
        ``012046`` 北 / ``total`` 汇总，需按它过滤）；
        ``market_margin_investors`` 两融投资者情况统计（Params ``["003", window]``，``arg`` 取
        ``1m`` / ``3m`` / ``6m`` / ``1y``，实测 15 / 60 / 120 / 242 行）；
        ``market_new_shares`` 新股分析；``market_capital`` 大盘市场概览
        （``arg`` 默认 ``"004"``）。

        指数两融**有**序列：用 ``market_margin_summary``。不要用 :meth:`trading_report` 的
        ``margin_history`` 查指数——那是**个股**区间通道，服务端按数字代码返回深市同名个股
        （实测 ``000905`` → 厦门港务、``000001`` → 平安银行，各约 122 行）。

        ``market_new_shares`` 与 ``market_capital`` 不需要 ``code``（可省略或传空串）；
        ``page`` / ``page_size`` 仅对集合类 section 有效。

        语义别名：``index_members`` = ``index_flow_members``（成份股清单）、
        ``index_etfs`` = ``index_tracking``（跟踪标的）、
        ``ipo_calendar`` = ``market_new_shares``（新股）。

        返回模型：:class:`eltdx.f10.models.F10Response`。
        """

        section = _MARKET_REPORT_SECTION_ALIASES.get(section, section)
        code_less = section in _MARKET_REPORT_CODELESS_SECTIONS
        code6 = _code6(code) if (code or not code_less) else ""
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
            "index_valuation": (f"{zs}jbxx_gzxx", ("001", code6, "", "")),
            "index_tracking": (f"{zs}jbxx_gzbd", (code6, page_text, size_text)),
            "index_flow": (f"{zs}zjdx", (arg or "004_1", code6, "", "", page_text, size_text)),
            "index_flow_members": (f"{zs}cfgsj_cfgjcsj", (arg or "001", code6, page_text, size_text)),
            "index_notices": (f"{zs}jrts", (arg or "001", code6, page_text, size_text)),
            "market_basic": (f"{dpzx}jbxx", ("002", code6)),
            "market_margin": (f"{dpzx}rzrq", ("001", code6)),
            "market_margin_summary": (f"{dpzx}rzrq", ("002", code6)),
            "market_margin_investors": (f"{dpzx}rzrq", ("003", arg or "1m")),
            "market_new_shares": (f"{dpzx}xgfx", ("002",)),
            "market_capital": (f"{dpzx}scgl", (arg or "004",)),
        }
        entry, params = _section_request(section, table)
        return self.params(entry, *params)

    def index_members(
        self,
        code: str,
        *,
        all_pages: bool = True,
        page_size: int = _MAX_PAGE_SIZE,
        max_pages: int = 100,
    ) -> IndexMemberTable:
        """指数成份股清单（覆盖中证、国证和上证/深证系列指数）。

        底层 Entry 为 ``CWServ.skef10_zs_cfgsj_cfgjcsj``。实测 ``000300`` 沪深 300
        返回 300 只、``000905`` 中证 500 返回 500 只、``399006`` 创业板指返回 100 只、
        ``000001`` 上证指数返回 2229 只，分页可拉全。

        Args:
            code: 指数代码，6 位数字或带市场前缀（如 ``000300`` / ``sh000300``）。
            all_pages: 是否自动翻页取全部成份股。``False`` 时只取第一页。
            page_size: 单页条数，服务端上限 100。
            max_pages: 翻页保护上限，防止指数样本过大时死循环。

        Returns:
            :class:`~eltdx.f10.models.IndexMemberTable`。``total`` 是服务端报告的
            样本总数；``rows`` 内每行只有代码、名称、纳入日期和市场标记——
            **服务端不返回成份权重**，需要权重请走通达信客户端。
        """

        size = max(1, min(int(page_size), _MAX_PAGE_SIZE))
        rows: list[IndexMember] = []
        total: int | None = None
        page = 1
        while True:
            response = self.market_report(code, "index_members", page=page, page_size=size)
            table = response.first_table
            batch = table.rows if table is not None else ()
            if total is None:
                total = _result_total(response)
            for row in batch:
                rows.append(
                    IndexMember(
                        code=_text(row.get("N001")),
                        name=_text(row.get("N002")),
                        market_id=_market_flag(row.get("N004")),
                        join_date=row.get("N003"),
                        raw=dict(row),
                    )
                )
            if not all_pages or len(batch) < size or page >= max_pages:
                break
            if total is not None and len(rows) >= total:
                break
            page += 1
        return IndexMemberTable(index_code=_code6(code), total=total, rows=tuple(rows))

    def index_tracking_products(
        self,
        code: str,
        *,
        all_pages: bool = True,
        page_size: int = _MAX_PAGE_SIZE,
        max_pages: int = 100,
    ) -> IndexTrackingTable:
        """跟踪某个指数的全部产品（ETF / LOF / 场外基金等）。

        底层 Entry 为 ``CWServ.skef10_zs_jbxx_gzbd``。这是 TQ
        ``get_trackzs_etf_info`` 的等价能力，但方向相反：给定指数代码，返回跟踪它的
        全部产品；实测 ``000300`` 返回 299 只。

        Args:
            code: 指数代码。其余参数同 :meth:`index_members`。

        Returns:
            :class:`~eltdx.f10.models.IndexTrackingTable`。``N004`` 与 ``N007``
            服务端未标注含义，不做猜测，只保留在各行的 ``raw`` 里。
        """

        size = max(1, min(int(page_size), _MAX_PAGE_SIZE))
        rows: list[IndexTrackingProduct] = []
        total: int | None = None
        page = 1
        while True:
            response = self.market_report(code, "index_etfs", page=page, page_size=size)
            table = response.first_table
            batch = table.rows if table is not None else ()
            if total is None:
                total = _result_total(response)
            for row in batch:
                rows.append(
                    IndexTrackingProduct(
                        code=_text(row.get("N001")),
                        name=_text(row.get("N002")),
                        market_id=_int(row.get("sc")),
                        scale_yuan=_float(row.get("N003")),
                        change_pct=_float(row.get("N005")),
                        amount_yuan=_float(row.get("N006")),
                        inception_date=row.get("N008"),
                        manager=_text(row.get("N009")),
                        company=_text(row.get("N010")),
                        nav=_float(row.get("N011")),
                        fee_rate_pct=_float(row.get("N012")),
                        raw=dict(row),
                    )
                )
            if not all_pages or len(batch) < size or page >= max_pages:
                break
            if total is not None and len(rows) >= total:
                break
            page += 1
        return IndexTrackingTable(index_code=_code6(code), total=total, rows=tuple(rows))

    def board_members(
        self,
        code: str,
        *,
        all_pages: bool = True,
        page_size: int = _MAX_PAGE_SIZE,
        max_pages: int = 50,
    ) -> BoardMemberTable:
        """板块成份股的分红记录清单，按记录去重自动翻页。

        底层 Entry 为 ``CWServ.skef10_bk_getinfo`` 的 ``board_members`` 分区。
        实测 ``880472``（证券板块）共 67 行记录、44 只证券（21 只有中报 + 年报
        两条记录），按公告日期降序。

        !!! warning "一行 = 一条分红记录，不是一只股票"
            同一证券的中报 / 年报分红各占一行，所以 ``rows`` 里同一代码会出现多次；
            ``total``（第二个 ResultSet 的 ``zs`` 列）是**记录总数**，不是证券只数。
            只要证券清单请用 ``BoardMemberTable.stocks`` / ``full_codes``。

        !!! warning "服务端翻页对同日记录不稳定"
            排序按公告日期降序，但**同日记录的服务端次序在两次请求间会变**
            （实测同一 ``page=2`` 连续请求两次结果不同，出现跨页重复行）。
            因此本方法按**记录身份**（代码 + 方案 + 三个日期 + 报告期）去重，
            而不是按代码去重——按代码去重会丢掉合法的多条分红记录。
            想完全绕开翻页，用 ``page_size=100``（默认）单页取全。
        """

        size = max(1, min(int(page_size), _MAX_PAGE_SIZE))
        rows: list[BoardMember] = []
        seen: set[tuple] = set()
        total: int | None = None
        page = 1
        while True:
            response = self.market_report(code, "board_members", page=page, page_size=size)
            table = response.first_table
            batch = table.rows if table is not None else ()
            if total is None:
                total = _result_total(response)
            added = 0
            for row in batch:
                item = BoardMember(
                    code=_text(row.get("N001")),
                    name=_text(row.get("N002")),
                    market_id=_market_flag(row.get("sc")),
                    dividend_plan=_text(row.get("N003")),
                    progress=_text(row.get("N004")),
                    announce_date=row.get("N005"),
                    record_date=row.get("N006"),
                    ex_date=row.get("N007"),
                    report_period=row.get("N008"),
                    raw=dict(row),
                )
                key = item.record_key
                if key in seen:
                    continue
                seen.add(key)
                rows.append(item)
                added += 1
            if not all_pages or not batch or len(batch) < size or page >= max_pages:
                break
            if total is not None and len(rows) >= total:
                break
            if added == 0:
                # 整页都是已见记录：服务端页界在原地打滑，再翻也不会扩容。
                break
            page += 1
        return BoardMemberTable(board_code=_code6(code), total=total, rows=tuple(rows))

    def ipo_calendar(self) -> IpoCalendar:
        """近期新股列表（含申购日、发行价、募资额）。

        底层 Entry 为 ``CWServ.skef10_dpzx_xgfx``，参数 ``002``，实测约 50 行，
        覆盖沪深北三个市场，按日期倒序。这是 TQ ``get_ipo_info`` 的等价能力。

        字段确认状态：``N001`` 日期、``N002`` 代码、``N003`` 名称、``N006`` 发行价、
        ``N008`` 募资总额已与公开数据核对一致；``N004`` / ``N005`` / ``N007``
        服务端未标注，这里按语义命名为 ``issue_pe`` / ``extra_pct`` / ``apply_limit``，
        **均为推断**，请用行内 ``raw`` 复核后再用于生产。
        """

        response = self.market_report(section="ipo_calendar")
        table = response.first_table
        rows: list[IpoEntry] = []
        for row in table.rows if table is not None else ():
            rows.append(
                IpoEntry(
                    apply_date=row.get("N001"),
                    code=_text(row.get("N002")),
                    name=_text(row.get("N003")),
                    market_id=_int(row.get("sc")),
                    issue_pe=_float(row.get("N004")),
                    extra_pct=_float(row.get("N005")),
                    issue_price=_float(row.get("N006")),
                    apply_limit=_float(row.get("N007")),
                    raise_amount=_float(row.get("N008")),
                    raw=dict(row),
                )
            )
        return IpoCalendar(rows=tuple(rows))

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

    def finance_diagnosis(self, code: str, section: str = "zzyj", scope: str = "") -> F10Response:
        """财务诊断（增减异常）。

        实测（2026-10-22）：服务端仅 ``zzyj``（增减异常）子表有数据；
        历史 section（``yynl`` 盈利能力等）已随页面改版下线，传了返回空。
        ``t0`` 为标志行（``isjr``），``t1`` 才是异常科目列表。
        """

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

    def governance(self, code: str, section: str = "gljy", arg: str = "") -> F10Response:
        """资本运作治理。section=gljy 为关联交易（数据最全）。

        现行页面（gg_zbyz.js）的 5 个 section：``dbmx`` 担保明细 /
        ``gljy`` 关联交易 / ``gqtz`` 股权转让 / ``xmtz`` 项目投资 /
        ``zjly`` 资金运用。历史 section ``wgcl``（违规处理）服务端仍返回
        （多数公司 0 行，属数据缺失而非接口问题）。
        """

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

    # ------------------------------------------------------------------
    # 市场资讯（通达信 7615「资讯」页；枚举与字段说明见 eltdx.f10.news）
    # ------------------------------------------------------------------
    def news_channels(self) -> list[tuple[int, str]]:
        """返回全部资讯频道 ``[(频道号, 栏目名), ...]``（静态枚举，不发网络请求）。

        频道号用于 :meth:`channel_news`；分组见 :data:`eltdx.f10.news.NEWS_CHANNEL_GROUPS`。
        """
        return sorted(NEWS_CHANNEL_LABELS.items())

    def market_news(
        self,
        tab: "NewsHomeTab | int" = NewsHomeTab.HEADLINES,
        *,
        page: int = 1,
        page_size: int = 50,
    ) -> F10Response:
        """「资讯首页」顶部要闻流，底层 Entry ``CWServ.tdxi_zxxwsy``。

        Params = ``[tab, page, page_size]``，``tab`` 见 :class:`eltdx.f10.news.NewsHomeTab`
        （默认 ``HEADLINES`` 要闻速递）。新闻行在响应 ``result_sets[1].rows``，
        可用 :func:`eltdx.f10.news.extract_news_rows` 提取。
        """
        return self.call(
            "CWServ.tdxi_zxxwsy",
            {"Params": [str(int(tab)), str(page), str(page_size)]},
        )

    def channel_news(
        self,
        channel: "NewsChannel | int",
        *,
        page: int = 1,
        page_size: int = 50,
        query1: str = "1",
        keyword: str = "",
    ) -> F10Response:
        """按频道号拉取栏目新闻，底层 Entry ``CWServ.tdxi_zxxwnews``。

        Params = ``[query1(常为 "1"), 频道号, page, page_size, keyword]``；
        频道号见 :class:`eltdx.f10.news.NewsChannel`。新闻行在 ``result_sets[1].rows``。
        """
        return self.call(
            "CWServ.tdxi_zxxwnews",
            {"Params": [str(query1), str(int(channel)), str(page), str(page_size), keyword]},
        )
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
        if str(req_id) in {"200740", "200742", "200745", "200747"}:
            payload["zq_num"] = str(zq_num)
        if extra:
            payload.update(extra)
        return self.call("HQServ.hq_nlp_tcihq", [payload])

    # ------------------------------------------------------------------
    # 个股总评（ggzp）与深度分析（sdfx）页面的语义化封装
    #
    # 这两个页面共用 ``HQServ.hq_nlp_tcihq``（题材/资金/排名）与
    # ``HQServ.hq_nlp_gpsj``（估值/走势）两个服务；底层 req_id 已可由
    # :meth:`theme_market` / :meth:`valuation` 直接传，这里按页面语义
    # 暴露具名方法。实测（2026-10-22，平安 601318 / 茅台 600519）全部可用。
    # ------------------------------------------------------------------

    def stock_stage_gain(
        self, code: str, *, days: int | str = 60, page_size: int = 10
    ) -> F10Response:
        """个股阶段涨幅日线序列（``hq_nlp_tcihq`` ReqId 200740）。

        ``t0`` 为区间统计（``T011`` 区间涨幅 / ``T012`` 区间振幅），
        ``t1`` 为日线序列（``N001`` 日期、``N002~N010`` 价格与涨跌幅）。
        ``days`` 为回看天数（客户端默认 60）。
        """

        return self.theme_market(
            code, 200740, page_size=page_size, zq_num=days
        )

    def stock_main_funds(
        self, code: str, *, days: int | str = 30, page_size: int = 10
    ) -> F10Response:
        """个股主力资金流日序列（``hq_nlp_tcihq`` ReqId 200742）。

        ``t1`` 每行：``N001`` 行业名、``N002`` 日期、``N003~N006``
        主力流入/流出/净额/占比（口径随服务端）。
        """

        return self.theme_market(
            code, 200742, page_size=page_size, zq_num=days
        )

    def stock_industry_rank(
        self, code: str, *, days: int | str = 30, page_size: int = 10
    ) -> F10Response:
        """个股所属行业与行业内排名（``hq_nlp_tcihq`` ReqId 200745）。

        ``t0`` 为归属信息：``hy_setcode`` / ``hy_code`` / ``hy_name`` /
        ``cfg_num``（行业内标的数）/ ``rank``（排名）；
        ``t1`` 为行业指数近期走势（``N001`` 日期 / ``N002`` 点位）。
        """

        return self.theme_market(
            code, 200745, page_size=page_size, zq_num=days
        )

    def stock_industry_pct_rank(
        self, code: str, *, days: int | str = 30, page_size: int = 10
    ) -> F10Response:
        """个股在所属行业内的日涨跌幅排名（``hq_nlp_tcihq`` ReqId 200747）。

        ``t1`` 每行：``N001`` 日期、``N002`` 嵌套数组
        ``[排名, setcode, code, name, 涨跌幅]``。
        """

        return self.theme_market(
            code, 200747, page_size=page_size, zq_num=days
        )

    def stock_board_member_quotes(
        self,
        board_code: str,
        *,
        board_market: int = 1,
        page: int = -1,
        page_size: int = -1,
    ) -> F10Response:
        """关联板块的成分股行情（``hq_nlp_tcihq`` ReqId 200744）。

        ``board_code`` / ``board_market`` 为板块代码与市场（如 ``881001`` 沪）；
        板块列表可由 :meth:`theme_market`（200743）获得。
        ``t1`` 每行：``N001`` setcode、``N002`` code、``N003`` name、
        ``N004`` / ``N005`` 涨跌幅与量比。
        """

        payload = {
            "ReqId": "200744",
            "setcode": str(board_market),
            "code": _code6(board_code),
            "Page": page,
            "PageSize": str(page_size),
            "modname": "mod_tcihq.dll",
        }
        return self.call("HQServ.hq_nlp_tcihq", [payload])

    def stock_valuation_percentile(
        self,
        code: str,
        kind: str = "0",
        *,
        page: int = -1,
        page_size: int = 5,
    ) -> F10Response:
        """个股估值历史分位（``hq_nlp_gpsj`` ReqId 200192）。

        ``kind``：``0`` PE(TTM) / ``1`` PB(MRQ) / ``2`` 市现率 / ``3`` 市销率。
        ``t1`` 每行：``DATE`` 日期、``value`` 估值、``bfw`` 百分位。
        """

        return self.valuation(
            code, 200192, page=page, page_size=page_size, extra={"Type": kind}
        )

    def stock_pepb_percentile(
        self,
        code: str,
        indicator: str = "0",
        *,
        window: str = "1Y",
        period: str = "0",
        page: int = 1,
        page_size: int = 300,
    ) -> F10Response:
        """个股 PE/PB 历史与分位（``hq_nlp_gpsj`` ReqId 200124，深度分析估值带）。

        ``indicator``：``0`` PE(TTM) / ``1`` PB(MRQ) / ``2`` 市现率 / ``3`` 市销率；
        ``window`` 时间范围（``1Y`` / ``3Y`` / ``5Y`` / ``10Y``）；
        ``period`` 行情周期（``0`` 日 / ``1`` 周 / ``2`` 月）。
        ``t0`` 每行：``N001`` 日期、``N002`` 指标值、``N003``/``N004`` 分位；
        ``t1`` 为区间统计（``TotalNum`` / ``Min`` / ``Mid``）。
        """

        return self.valuation(
            code,
            200124,
            page=page,
            page_size=page_size,
            extra={"sjfw": window, "zb": indicator, "hqzq": period},
        )

    def stock_history_trend(self, code: str) -> F10Response:
        """个股年度历史走势（``hq_nlp_gpsj`` ReqId 200135）。

        ``t0`` 每行一个年度（``N001`` 年份、``N002~N014`` 年度涨跌幅与估值）。
        """

        return self.valuation(code, 200135)

    def stock_finance_weights(self, code: str, section: str = "zcfzb") -> F10Response:
        """财务指标权重（``CWServ.tdxf10_gg_zxtscwbq``，最新提示页）。

        ``section`` 为财务报告期 key（由最新提示页的财务概况列表给出）。
        实测部分 key 返回空——属数据缺失，调用前建议先取 ``digest``。
        """

        return self.params("CWServ.tdxf10_gg_zxtscwbq", _code6(code), section)

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
        if entry == LIMIT_BOARD_LADDER_ENTRY:
            base_url = self.limit_board_ladder_base_url
        elif entry.startswith(_MG_HK_ENTRY_PREFIXES) or entry.startswith(_QH_ENTRY_PREFIXES):
            base_url = self.mg_hk_base_url
        else:
            base_url = self.base_url
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

    from eltdx._native_abi import load_native

    return load_native().f10_parse_response(entry, request_body, raw, _MODULES[__name__])


def _parse_result_set(raw: Any, table_index: int) -> F10ResultSet:
    from eltdx._native_abi import load_native

    return load_native().f10_parse_table(raw, table_index, _MODULES[__name__])


def _columns(raw: Mapping[str, Any]) -> list[str]:
    from eltdx._native_abi import load_native

    return load_native().f10_columns(raw, _MODULES[__name__])


def _row_values(row: Any) -> list[Any]:
    from eltdx._native_abi import load_native

    return load_native().f10_row_values(row, _MODULES[__name__])


def _column_name(columns: Sequence[str], index: int) -> str:
    from eltdx._native_abi import load_native

    return load_native().f10_column_name(columns, index)


def _row_dict(cells: Sequence[F10Cell]) -> dict[str, Any]:
    from eltdx._native_abi import load_native

    return load_native().f10_row_dict(cells, _MODULES[__name__])


def _code6(code: str) -> str:
    return split_code(code)[2]


def _us_code(code: str) -> str:
    """美股代码规范化 → 裸大写代码（如 ``NVDA``）。

    接受 ``NVDA`` / ``nvda`` / ``us.NVDA`` / ``74.NVDA``，去掉市场前缀、转大写。
    美股代码含字母，可能带 ``-`` / ``.``（如 ``BRK-B``），不做长度强制。
    """

    text = str(code).strip()
    if "." in text:
        text = text.split(".")[-1]
    text = text.upper()
    # 至少一个字母或数字（拒绝 "-" / "--" 这类纯符号），其余字符限字母数字与连字符
    if not text or not any(c.isalnum() for c in text):
        raise ProtocolError(f"invalid US code: {code!r}")
    if not all(c.isalnum() or c == "-" for c in text):
        raise ProtocolError(f"invalid US code: {code!r}")
    return text


def _hk_code(code: str) -> str:
    """港股代码规范化 → 5 位数字代码（如 ``00700``）。

    接受 ``00700`` / ``700`` / ``hk.00700`` / ``31.00700``，去掉市场前缀，
    前导补零到 5 位。
    """

    text = str(code).strip()
    if "." in text:
        text = text.split(".")[-1]
    # 去前缀后必须全是数字（拒绝 "00700A" 这类混入字母的输入）
    if not text.isdigit():
        raise ProtocolError(f"invalid HK code: {code!r}")
    return text.zfill(5)


def _bond_code(code: str) -> str:
    """债券代码规范化 → 6 位数字代码（如 ``113043``）。

    接受 ``113043`` / ``sh113043`` / ``sz128043``；不推断市场（沪深债券
    代码段高度重叠，如 ``110``/``113`` 沪深都有），仅取 6 位数字。
    """

    text = str(code).strip()
    if len(text) == 8 and text[:2] in MARKET_TO_ID:
        text = text[2:]
    if len(text) != 6 or not text.isdigit():
        raise ProtocolError(f"invalid bond code: {code!r}")
    return text


def _bond_sc(code: str) -> str:
    """债券 F10 的市场码参数 ``sc``（实测：``1`` 沪 / ``0`` 深）。

    与 ``MacClient`` 的 market 约定一致。无法从代码推断时（如未带前缀的
    裸代码且段重叠）默认 ``1``（沪市债券段更常见）；显式 ``sh``/``sz``
    前缀优先。
    """

    text = str(code).strip().lower()
    if len(text) == 8 and text[:2] in MARKET_TO_ID:
        return "1" if text[:2] == "sh" else "0"
    # 沪深重叠段（110/113/120/122/124 等）无法纯代码推断，默认沪市。
    return "1"


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


def _text(value: Any) -> str | None:
    """把服务端标量转成字符串；空值、占位符一律返回 ``None``。"""

    if value is None:
        return None
    text = str(value).strip()
    return text or None


def _int(value: Any) -> int | None:
    if value is None or value == "":
        return None
    try:
        return int(str(value).strip())
    except (TypeError, ValueError):
        return None


def _float(value: Any) -> float | None:
    if value is None or value == "":
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def _market_flag(value: Any) -> int | None:
    """指数成份股的市场标记：``"1"`` 沪市、``"0"`` 深市。

    服务端把市场标记放在 ``N004`` 且是字符串；无法识别时返回 ``None``，
    由 :class:`~eltdx.f10.models.IndexMember` 按代码号段回退推断。
    """

    text = _text(value)
    if text is None:
        return None
    return {"1": 1, "0": 0}.get(text)


def _result_total(response: F10Response) -> int | None:
    """分页接口把样本总数放在第二个 ResultSet（列名 ``zs``）里。"""

    for table in response.result_sets:
        if table.key == "table0":
            continue
        for row in table.rows:
            for value in row.values():
                if isinstance(value, int):
                    return value
                parsed = _int(value)
                if parsed is not None:
                    return parsed
    return None


def _latest_report_period(response: F10Response) -> str:
    """从 ``*bgq`` 报告期列表里取最新报告期（首行 ``rq``）；为空时返回空串。"""

    rows = response.rows
    if not rows:
        return ""
    return str(rows[0].get("rq") or "")


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
