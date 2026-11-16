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

        from eltdx._native_abi import load_native

        return load_native().f10_call(self, entry, body, params, _MODULES[__name__])

    def params(self, entry: str, *params: Any) -> F10Response:
        """Call a CWServ Entry with a Params array."""

        from eltdx._native_abi import load_native

        return load_native().f10_params(self, entry, params)

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

        from eltdx._native_abi import load_native

        return load_native().f10_ladder(
            self, start_date, end_date, include_summary, _MODULES[__name__]
        )

    def limit_board_ladder(
        self,
        start_date: str | int | date | None = None,
        end_date: str | int | date | None = None,
        *,
        include_summary: bool = False,
    ) -> LimitBoardLadder:
        """Compatibility alias for :meth:`limit_up_down_list`."""

        from eltdx._native_abi import load_native

        return load_native().f10_ladder_alias(self, start_date, end_date, include_summary)

    def stock_info(self, code: str) -> F10Response:
        """股票基础信息，通常用于页面初始化。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "stock_info", code, None, _MODULES[__name__]
        )

    def business_periods(self, code: str) -> F10Response:
        """主营构成可选报告期。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "business_periods", code, None, _MODULES[__name__]
        )

    def topic_ids(self, code: str) -> F10Response:
        """股票关联题材 ID 列表。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "topic_ids", code, None, _MODULES[__name__]
        )

    def company_profile(self, code: str, section: str = "8") -> F10Response:
        """公司概况。section=8 为发行上市信息，section=9 为指数调入调出。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "company_profile", code, section, _MODULES[__name__]
        )

    def business_composition(self, code: str, report_date: str | None = None) -> F10Response:
        """主营构成。report_date 不传时先取服务端给出的最新报告期。"""

        from eltdx._native_abi import load_native

        return load_native().f10_business_composition(self, code, report_date, _MODULES[__name__])

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

        from eltdx._native_abi import load_native

        return load_native().f10_report(
            self, "shareholders", code, section,
            {"report_date": report_date, "kind": kind, "filter1": filter1, "filter2": filter2,
             "page": page, "page_size": page_size}, _MODULES[__name__]
        )

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

        from eltdx._native_abi import load_native

        return load_native().f10_shareholder_plans(
            self, code, {"page": page, "page_size": page_size, "filter1": filter1, "filter2": filter2}
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

        from eltdx._native_abi import load_native

        return load_native().f10_report(
            self, "capital_structure", code, section, {"date": date}, _MODULES[__name__]
        )

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

        from eltdx._native_abi import load_native

        return load_native().f10_report(
            self, "business_report", code, section, {"report_date": report_date}, _MODULES[__name__]
        )

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

        from eltdx._native_abi import load_native

        return load_native().f10_report(
            self, "industry_analysis", code, section, {"arg": arg}, _MODULES[__name__]
        )

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

        from eltdx._native_abi import load_native

        return load_native().f10_report(
            self, "trading_report", code, section, {"arg": arg}, _MODULES[__name__]
        )

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

        from eltdx._native_abi import load_native

        return load_native().f10_report(
            self, "fund_report", code, section,
            {"arg": arg, "date": date, "page": page, "page_size": page_size}, _MODULES[__name__]
        )

    # ------------------------------------------------------------------
    # 美股 mgf10 家族（Entry: CWServ.skef10_mg_*，网关 page2）
    # ------------------------------------------------------------------

    def mg_name(self, code: str) -> F10Response:
        """美股名称与市场类型（``skef10_mg_idreq``）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "mg_name", code, None, _MODULES[__name__]
        )

    def mg_tips(self, code: str, section: str = "zxdt") -> F10Response:
        """美股最新提示（``skef10_mg_zxdt``）。

        section: ``zxdt`` 最新动态 / ``sypl`` 首页评论 / ``gsgy`` 公司概要 /
        ``zysj`` 重要数据。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "mg_tips", code, section, _MODULES[__name__]
        )

    def mg_profile(self, code: str) -> F10Response:
        """美股公司概况（``skef10_mg_gsgk``）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "mg_profile", code, None, _MODULES[__name__]
        )

    def mg_finance(self, code: str, section: str = "zcfzb") -> F10Response:
        """美股财务分析（``skef10_mg_cwfx``）。

        section: ``zcfzb`` 资产负债表 / ``lyb`` 利润表 / ``xjllb`` 现金流量表。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "mg_finance", code, section, _MODULES[__name__]
        )

    def mg_finance_indicator(self, code: str, section: str = "jbzb") -> F10Response:
        """美股财务指标（``skef10_mg_cwzb``）。

        section: ``jbzb`` 基本指标 / ``ylnl`` 营运能力 / ``cznl`` 成长能力 /
        ``yynl`` 盈利能力 / ``ccznl`` 偿债能力（内部映射到数字 ``001``~``005``）。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "mg_finance_indicator", code, section, _MODULES[__name__]
        )

    def mg_business(self, code: str) -> F10Response:
        """美股经营分析（``skef10_mg_jyfx``）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "mg_business", code, None, _MODULES[__name__]
        )

    def mg_industry(self, code: str) -> F10Response:
        """美股行业分析（``skef10_mg_hyfx``）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "mg_industry", code, None, _MODULES[__name__]
        )

    def mg_industry_compare(self, code: str) -> F10Response:
        """美股行业对比（``skef10_mg_hydb``）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "mg_industry_compare", code, None, _MODULES[__name__]
        )

    def mg_dividend(self, code: str, section: str = "fh") -> F10Response:
        """美股分红融资（``skef10_mg_fhrz``）。

        section: ``fh`` 分红 / ``cf`` 融资。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "mg_dividend", code, section, _MODULES[__name__]
        )

    def mg_capital(self, code: str, section: str = "001", page: int = 1) -> F10Response:
        """美股股本结构（``skef10_mg_gbjg``）。

        section: ``001`` 股本结构 / ``002`` 股本变动公告。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_page_endpoint(
            self, "mg_capital", code, section, {"page": page}, _MODULES[__name__]
        )

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

        from eltdx._native_abi import load_native

        return load_native().f10_page_endpoint(
            self, "mg_shareholders", code, section, {"page": page, "page_size": page_size}, _MODULES[__name__]
        )

    def mg_directors(self, code: str, section: str = "ds") -> F10Response:
        """美股董事高管（``skef10_mg_dsgg``）。

        section: ``ds`` 董事 / ``gg`` 高管 / ``lrdjg`` 历任董监高。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "mg_directors", code, section, _MODULES[__name__]
        )

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

        from eltdx._native_abi import load_native

        return load_native().f10_page_endpoint(
            self, "mg_market", code, section, {"page": page, "page_size": page_size}, _MODULES[__name__]
        )

    def mg_announcements(self, code: str) -> F10Response:
        """美股公司公告（``skef10_mg_gsbd``）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "mg_announcements", code, None, _MODULES[__name__]
        )

    def mg_rating(self, code: str, section: str = "001") -> F10Response:
        """美股公司评级（``skef10_mg_gspj``）。section: ``001``/``002``/``003``。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "mg_rating", code, section, _MODULES[__name__]
        )

    def mg_topic(self, code: str) -> F10Response:
        """美股热点题材（``skef10_mg_rdtc``）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "mg_topic", code, None, _MODULES[__name__]
        )

    def mg_deep_analysis(self, code: str, section: str = "001") -> F10Response:
        """美股深度分析（``skef10_mg_sdfx``）。section: ``001``/``002``。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "mg_deep_analysis", code, section, _MODULES[__name__]
        )

    # ------------------------------------------------------------------
    # 港股 hkf10 家族（Entry: CWServ.skef10_hk_*，网关 page2）
    # ------------------------------------------------------------------

    def hk_name(self, code: str) -> F10Response:
        """港股名称与市场类型（``skef10_hk_idreq``）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "hk_name", code, None, _MODULES[__name__]
        )

    def hk_tips(self, code: str, section: str = "1") -> F10Response:
        """港股最新提示（``skef10_hk_zxts``）。section 为数字子标签 ``1``~``5``。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "hk_tips", code, section, _MODULES[__name__]
        )

    def hk_events(self, code: str, section: str = "1") -> F10Response:
        """港股公司大事（``skef10_hk_gsds``）。section 为数字子标签 ``1``~``6``。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "hk_events", code, section, _MODULES[__name__]
        )

    def hk_profile(self, code: str, section: str = "1") -> F10Response:
        """港股公司概况（``skef10_hk_gsgk``）。section 为数字子标签 ``1``~``6``。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "hk_profile", code, section, _MODULES[__name__]
        )

    def hk_related(self, code: str, section: str = "6") -> F10Response:
        """港股关联证券（``skef10_hk_glzq``）。section 为数字子标签（如 ``6``）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "hk_related", code, section, _MODULES[__name__]
        )

    def hk_executives(self, code: str, section: str = "1") -> F10Response:
        """港股高管资料（``skef10_hk_ggzl``）。section: ``1``/``2``/``3``。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "hk_executives", code, section, _MODULES[__name__]
        )

    def hk_capital(self, code: str, section: str = "1") -> F10Response:
        """港股股本结构（``skef10_hk_gbjg``）。section: ``1``/``2``/``3``。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "hk_capital", code, section, _MODULES[__name__]
        )

    def hk_shareholders(self, code: str, section: str = "1") -> F10Response:
        """港股股东持股（``skef10_hk_gdcg``）。section 为数字子标签 ``1``~``13``。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "hk_shareholders", code, section, _MODULES[__name__]
        )

    def hk_finance(self, code: str, section: str = "zcfzb") -> F10Response:
        """港股财务分析（``skef10_hk_cwfx``）。

        section: ``zcfzb`` 资产负债表 / ``zhsyb`` 综合损益表 / ``xjllb`` 现金流量表
        （内部映射到数字 ``2``/``1``/``3``）。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "hk_finance", code, section, _MODULES[__name__]
        )

    def hk_finance_indicator(self, code: str, section: str = "jbzb") -> F10Response:
        """港股财务指标（``skef10_hk_cwzb``）。

        section: ``jbzb`` 基本指标 / ``ylnl`` 营运能力 / ``cznl`` 成长能力 /
        ``yynl`` 盈利能力 / ``ccznl`` 偿债能力（内部映射到数字 ``1``~``5``）。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "hk_finance_indicator", code, section, _MODULES[__name__]
        )

    def hk_business(self, code: str, section: str = "1") -> F10Response:
        """港股经营分析（``skef10_hk_jyfx``）。section 为数字子标签。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "hk_business", code, section, _MODULES[__name__]
        )

    def hk_industry_compare(self, code: str, section: str = "1") -> F10Response:
        """港股行业比较（``skef10_hk_hybj``）。section: ``1``/``3``。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "hk_industry_compare", code, section, _MODULES[__name__]
        )

    def hk_market(self, code: str, section: str = "3") -> F10Response:
        """港股市场行情（``skef10_hk_schq``）。section 为数字子标签。

        实测（2026-10-22，00700）：``3``~``4``~``6``~``7`` 有数据
        （245/246/242/51 行），``1``/``2``/``5`` 对该股返回空
        （子表为空或数据缺失）；默认取数据最全的 ``3``。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "hk_market", code, section, _MODULES[__name__]
        )

    def hk_capital_flow(self, code: str, section: str = "001") -> F10Response:
        """港股资金流向（``skef10_hk_schq_zjlx``，市场行情页的资金流向表）。

        实测（2026-10-22，00700）：``001`` 20 行，``003`` 空（图表数据）。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "hk_capital_flow", code, section, _MODULES[__name__]
        )

    def hk_news(self, code: str, section: str = "3") -> F10Response:
        """港股公司资讯（``skef10_hk_gszx``）。section: ``3``/``4``。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "hk_news", code, section, _MODULES[__name__]
        )

    def hk_topic(self, code: str) -> F10Response:
        """港股热点题材（``skef10_hk_rdtc``）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "hk_topic", code, None, _MODULES[__name__]
        )

    def hk_pepb_band(
        self,
        code: str,
        *,
        sjfw: str = "",
        tab_value: str = "",
        hqzq: str = "",
    ) -> F10Response:
        """港股深度分析·PE/PB band（``skef10_hk_sdfx_pepb_band``）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_input_endpoint(
            self, "hk_pepb_band", code, None, {"sjfw": sjfw, "tab_value": tab_value, "hqzq": hqzq}, _MODULES[__name__]
        )

    def hk_valuation(self, code: str, item_id: str = "") -> F10Response:
        """港股深度分析·估值分析（``skef10_hk_sdfx_gzfx``）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_input_endpoint(
            self, "hk_valuation", code, None, {"item_id": item_id}, _MODULES[__name__]
        )

    def hk_price_chart(self, code: str) -> F10Response:
        """港股深度分析·行情涨跌图（``skef10_hk_sdfx_hqzft``）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "hk_price_chart", code, None, _MODULES[__name__]
        )

    def hk_analyst_forecast(self, code: str, start: str = "", end: str = "") -> F10Response:
        """港股深度分析·分析师盈利预测（``skef10_hk_sdfx_fxysyfx``）。

        两个日期参数会去掉 ``-`` 后传给服务端（与客户端一致）。
        **实测空日期返回 0 行**（2026-10-22，00700）：不传时默认取近一年
        （今天往前 365 天 ~ 今天），近一年区间实测 53 行。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_input_endpoint(
            self, "hk_analyst_forecast", code, None, {"start": start, "end": end}, _MODULES[__name__]
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

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "sb_name", code, None, _MODULES[__name__]
        )

    def sb_profile(self, code: str, section: str = "001") -> F10Response:
        """新三板公司概况（``skef10_sb_gsgk``）。section 为子标签。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "sb_profile", code, section, _MODULES[__name__]
        )

    def sb_tips(self, code: str, section: str = "001") -> F10Response:
        """新三板最新提示（``skef10_sb_zxts``）。section 为子标签。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "sb_tips", code, section, _MODULES[__name__]
        )

    def sb_events(self, code: str, section: str = "ydjy") -> F10Response:
        """新三板交易大事（``skef10_sb_jyds``）。section: ``ydjy`` 异动交易。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "sb_events", code, section, _MODULES[__name__]
        )

    def sb_capital(self, code: str, section: str = "gbjg") -> F10Response:
        """新三板股本结构（``skef10_sb_gbjg``）。section: ``gbjg`` 股本结构。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "sb_capital", code, section, _MODULES[__name__]
        )

    def sb_shareholders(self, code: str, section: str = "sdgd") -> F10Response:
        """新三板股东研究（``skef10_sb_gdyj``）。section: ``sdgd`` 十大股东。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "sb_shareholders", code, section, _MODULES[__name__]
        )

    def sb_compliance(self, code: str, section: str = "001") -> F10Response:
        """新三板合规运营（``skef10_sb_hgyy``）。section 为子标签。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "sb_compliance", code, section, _MODULES[__name__]
        )

    def sb_dividend(self, code: str, section: str = "fh") -> F10Response:
        """新三板分红融资（``skef10_sb_fhrz``）。section 为子标签。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "sb_dividend", code, section, _MODULES[__name__]
        )

    def sb_finance(self, code: str, section: str = "zcfzb") -> F10Response:
        """新三板财务分析（``skef10_sb_cwfx``）。

        section: ``zcfzb`` 资产负债表 / ``xjllb`` 现金流量表。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "sb_finance", code, section, _MODULES[__name__]
        )

    def sb_business(self, code: str, section: str = "001") -> F10Response:
        """新三板经营分析（``skef10_sb_jyfx``）。section 为子标签。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "sb_business", code, section, _MODULES[__name__]
        )

    def sb_major_events(self, code: str, section: str = "001") -> F10Response:
        """新三板重大事项（``skef10_sb_zdsx``）。section 为子标签。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "sb_major_events", code, section, _MODULES[__name__]
        )

    def sb_news(self, code: str, section: str = "001") -> F10Response:
        """新三板公司资讯（``skef10_sb_gszx``）。section 为子标签。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "sb_news", code, section, _MODULES[__name__]
        )

    def sb_industry_compare(self, code: str, section: str = "hyxx") -> F10Response:
        """新三板行业比较（``skef10_sb_hybj``）。section: ``hyxx`` 行业信息。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "sb_industry_compare", code, section, _MODULES[__name__]
        )

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

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "bond_profile", code, None, _MODULES[__name__]
        )

    def bond_finance(self, code: str, section: str = "zcfzb") -> F10Response:
        """可转债财务分析（``skef10_bond_kzz_cwfx``）。section 为子标签。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "bond_finance", code, section, _MODULES[__name__]
        )

    def bond_rating(self, code: str, section: str = "pjzst") -> F10Response:
        """可转债债券评级（``skef10_bond_kzz_zqpj``）。

        section: ``pjzst`` 评级状态 / ``fxjgzst`` 发行机构评级状态。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "bond_rating", code, section, _MODULES[__name__]
        )

    def bond_conversion(self, code: str, section: str = "zgjg") -> F10Response:
        """可转债转股情况（``skef10_bond_kzz_zg``）。

        section: ``zgjg`` 转股价格 / ``zgjgzs`` 转股结果 / ``zgqk`` 转股情况。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "bond_conversion", code, section, _MODULES[__name__]
        )

    def bond_rate(self, code: str, section: str = "zqll") -> F10Response:
        """可转债债券利率（``skef10_bond_kzz_zqll``）。

        section: ``zqll`` 债券利率 / ``zbz`` 转股价。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "bond_rate", code, section, _MODULES[__name__]
        )

    def bond_announcements(self, code: str, section: str = "1") -> F10Response:
        """可转债资金资讯 / 公告（``skef10_bond_kzz_zjzx``）。section 为子标签。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "bond_announcements", code, section, _MODULES[__name__]
        )

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

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "gz_profile", code, section, _MODULES[__name__]
        )

    def gz_interest(self, code: str, section: str = "fxqk") -> F10Response:
        """国债付息情况（``skef10_bond_gz_fxqk``）。

        section: ``fxqk`` 付息情况 / ``xjlfx`` 付息兑付。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "gz_interest", code, section, _MODULES[__name__]
        )

    def gz_market_data(self, code: str, section: str = "dzjy") -> F10Response:
        """国债债券数据（``skef10_bond_gz_zjsj``）。

        section: ``dzjy`` 大宗交易 / ``zsbl`` 招标比例。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "gz_market_data", code, section, _MODULES[__name__]
        )

    def gz_rating(self, code: str, section: str = "dfzf") -> F10Response:
        """国债/地方债债券评级（``skef10_bond_gz_zjpj``）。

        section: ``dfzf`` 地方债评级。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "gz_rating", code, section, _MODULES[__name__]
        )

    def gz_finance(self, code: str) -> F10Response:
        """国债财务分析（``skef10_bond_gz_cwfx``）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "gz_finance", code, None, _MODULES[__name__]
        )

    def qyz_profile(self, code: str, section: str = "zqzl") -> F10Response:
        """企业债/公司债债券概况（``skef10_bond_qyz_zqgk``）。

        section: ``zqzl`` 债券资料 / ``xtfhr`` 信托发行人 / ``fhrjbzl`` 发行人基本信息。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "qyz_profile", code, section, _MODULES[__name__]
        )

    def qyz_interest(self, code: str, section: str = "fxqk") -> F10Response:
        """企业债/公司债付息兑付（``skef10_bond_qyz_xjlfx``）。

        section: ``fxqk`` 付息情况 / ``xjlfx`` 付息兑付 / ``febd`` 份额变动。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "qyz_interest", code, section, _MODULES[__name__]
        )

    def qyz_guarantee(self, code: str) -> F10Response:
        """企业债/公司债债券担保（``skef10_bond_qyz_zqdb``）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "qyz_guarantee", code, None, _MODULES[__name__]
        )

    def qyz_rating(self, code: str, section: str = "zjpjzst") -> F10Response:
        """企业债/公司债债券评级（``skef10_bond_qyz_zjpj``）。

        section: ``zjpjzst`` 中债评级状态 / ``zjxypj`` 中债信用评级 /
        ``fxjgpj`` 发行机构评级 / ``fxjgzst`` 发行机构评级状态。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "qyz_rating", code, section, _MODULES[__name__]
        )

    def qyz_rate(self, code: str) -> F10Response:
        """企业债/公司债债券利率（``skef10_bond_qyz_zjlv``）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "qyz_rate", code, None, _MODULES[__name__]
        )

    def qyz_review(self, code: str) -> F10Response:
        """公司债公司评述（``skef10_bond_qyz_gsps``）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "qyz_review", code, None, _MODULES[__name__]
        )

    def gzhg_info(self, code: str) -> F10Response:
        """国债回购债券信息（``skef10_bond_gzhg_zqxx``）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "gzhg_info", code, None, _MODULES[__name__]
        )

    def gzhg_quotes(self, code: str, section: str = "bpzs") -> F10Response:
        """国债回购债券数据（``skef10_bond_gzhg_zqxx``）。

        section: ``bpzs`` 报价张数 / ``hgzs`` 回购张数 / ``zqsj`` 债券数据 /
        ``hgdpll`` 回购抵押利率。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "gzhg_quotes", code, section, _MODULES[__name__]
        )

    def zczq_profile(self, code: str) -> F10Response:
        """ABS 债券概况（``skef10_bond_zczqh_zqgk``）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "zczq_profile", code, None, _MODULES[__name__]
        )

    def zczq_finance(self, code: str) -> F10Response:
        """ABS 财务分析（``skef10_bond_zczqh_cwfx``）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "zczq_finance", code, None, _MODULES[__name__]
        )

    def zczq_rating(self, code: str, section: str = "pjzst") -> F10Response:
        """ABS 债券评级（``skef10_bond_zczqh_zqpj``）。

        section: ``pjzst`` 评级状态 / ``fxjgzst`` 发行机构评级状态。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "zczq_rating", code, section, _MODULES[__name__]
        )

    def kjhz_profile(self, code: str) -> F10Response:
        """可交换债债券概况（``skef10_bond_kjhz_zqgk``）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "kjhz_profile", code, None, _MODULES[__name__]
        )

    def kjhz_finance(self, code: str) -> F10Response:
        """可交换债财务分析（``skef10_bond_kjhz_cwfx``）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "kjhz_finance", code, None, _MODULES[__name__]
        )

    def kjhz_interest(self, code: str, section: str = "fxqk") -> F10Response:
        """可交换债付息情况（``skef10_bond_kjhz_fxqk``）。

        section: ``fxqk`` 付息情况 / ``xjlfx`` 付息兑付。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "kjhz_interest", code, section, _MODULES[__name__]
        )

    def kjhz_guarantee(self, code: str) -> F10Response:
        """可交换债债券担保（``skef10_bond_kjhz_zqdb``）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "kjhz_guarantee", code, None, _MODULES[__name__]
        )

    def kjhz_rating(self, code: str, section: str = "pjzst") -> F10Response:
        """可交换债债券评级（``skef10_bond_kjhz_zqpj``）。

        section: ``pjzst`` 评级状态 / ``fxjgzst`` 发行机构评级状态 /
        ``fxjgpj`` 发行机构评级 / ``xypj`` 信用评级。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "kjhz_rating", code, section, _MODULES[__name__]
        )

    def kjhz_exchange(self, code: str, section: str = "jgtzzst") -> F10Response:
        """可交换债交换情况（``skef10_bond_kjhz_jhqk``）。

        section: ``jgtzzst`` 交换投资状态 / ``jhjbqk`` 交换基本情况 /
        ``jhjgtz`` 交换价格。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "kjhz_exchange", code, section, _MODULES[__name__]
        )

    def kjhz_rate(self, code: str, section: str = "zqll") -> F10Response:
        """可交换债债券利率（``skef10_bond_kjhz_zqll``）。

        section: ``zqll`` 债券利率 / ``zst`` 指数。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "kjhz_rate", code, section, _MODULES[__name__]
        )

    def bond_terms(self, code: str, section: str = "hstk") -> F10Response:
        """债券条款（``skef10_bond_kzz_lltk``，可转债/可交换债共用）。

        section: ``hstk`` 回售条款 / ``shtk`` 赎回条款 / ``xztk`` 下修条款 /
        ``zqll`` 债券利率。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "bond_terms", code, section, _MODULES[__name__]
        )

    def bond_bulletin(self, code: str) -> F10Response:
        """债券公告 / 资讯（``skef10_bond_zjzx``，各类型通用）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "bond_bulletin", code, None, _MODULES[__name__]
        )

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

        from eltdx._native_abi import load_native

        return load_native().f10_sector_endpoint(
            self, "hy_summary", hy, section, {}, _MODULES[__name__]
        )

    def hy_stage_rise(self, hy: str, flag: str = "e") -> F10Response:
        """行业阶段涨幅（``skef10_hy_jdzf``）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_input_endpoint(
            self, "hy_stage_rise", hy, None, {"flag": flag}, _MODULES[__name__]
        )

    def hy_profile(self, hy: str) -> F10Response:
        """行业介绍（``skef10_hy_hyzl_hyjs``）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "hy_profile", hy, None, _MODULES[__name__]
        )

    def hy_commentary(self, hy: str, section: str = "001") -> F10Response:
        """行业评论库（``skef10_hy_hyzl_hyplk``）。section 为子标签。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "hy_commentary", hy, section, _MODULES[__name__]
        )

    def hy_news(self, hy: str, section: str = "hydt") -> F10Response:
        """行业最新动态（``skef10_hy_zxdt``）。

        section: ``hydt`` 行业动态 / ``hyzt`` 行业专题。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "hy_news", hy, section, _MODULES[__name__]
        )

    def hy_members_rank(self, hy: str, section: str = "cwzkpm") -> F10Response:
        """行业成份股排名（5 个 Entry 共用 section 语义）。

        section: ``cwzkpm`` 财务指标排名（Params ``["002", hy, "", "e", ""]``）/
        ``gsgmpm`` 股本规模排名 / ``gzsppm`` 估值水平排名（``01`` 市盈率/``02`` 市净率）/
        ``scbxpm`` 市场表现排名（``scpm`` 涨跌幅/``sczf`` 涨跌幅方向）/
        ``zysrpm`` 主营收入排名。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_sector_endpoint(
            self, "hy_members_rank", hy, section, {}, _MODULES[__name__]
        )

    def hy_capital_stats(self, hy: str, section: str = "001") -> F10Response:
        """行业板块资金统计（``skef10_hy_hysj_bxzjtj``）。

        section: ``001`` 板块资金 / ``002`` 板块资金（另一种口径）。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_input_endpoint(
            self, "hy_capital_stats", hy, section, {}, _MODULES[__name__]
        )

    def hy_fund_holdings(self, hy: str, section: str = "1") -> F10Response:
        """行业基金持仓（``skef10_hy_hysj_jjzb``）。section 为子标签。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "hy_fund_holdings", hy, section, _MODULES[__name__]
        )

    def hy_capital_flow(self, hy: str, section: str = "001") -> F10Response:
        """行业资金流向（``skef10_hy_hysj_zjlx``）。

        section: ``001`` 资金流向 / ``002`` 资金流向（另一种口径）。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "hy_capital_flow", hy, section, _MODULES[__name__]
        )

    def hy_analyst_index(self, hy: str, section: str = "001") -> F10Response:
        """行业分析师指数（``skef10_hy_hyyj_fxszs``）。section 为子标签。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "hy_analyst_index", hy, section, _MODULES[__name__]
        )

    def hy_major_holders(self, hy: str) -> F10Response:
        """行业某名单（``skef10_hy_hyyj_hymzgd``）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "hy_major_holders", hy, None, _MODULES[__name__]
        )

    def hy_rating_summary(self, hy: str, section: str = "001") -> F10Response:
        """行业评级汇总（``skef10_hy_hyyj_hypjhz``）。section 为子标签。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "hy_rating_summary", hy, section, _MODULES[__name__]
        )

    def hy_index(self, hy: str) -> F10Response:
        """行业指数（``skef10_hy_idsy``）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "hy_index", hy, None, _MODULES[__name__]
        )

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

        from eltdx._native_abi import load_native

        return load_native().f10_sector_endpoint(
            self, "qh_position_report", code, section, {"date": date, "end_date": end_date, "index": index}, _MODULES[__name__]
        )

    def qh_basis_spread(self, code: str, section: str = "001", period: str = "1Y") -> F10Response:
        """期货基差价差分析（``qhf9_sp_jcjcfx``）。

        section: ``001`` 基差走势 / ``002`` 基差数据（``period`` 周期）/
        ``003`` 价差日期列表 / ``004`` 价差数据 / ``005`` 跨期价差列表 /
        ``006`` 跨品种价差。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_input_endpoint(
            self, "qh_basis_spread", code, section, {"period": period}, _MODULES[__name__]
        )

    def qh_warehouse_receipt(self, code: str, *, date: str = "") -> F10Response:
        """期货仓单日报（``CWServ.qhf9_sp_ckrb_ckrb``）。

        实测需传日期（``YYYYMMDD``），空日期返回 0 行。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_input_endpoint(
            self, "qh_warehouse_receipt", code, None, {"date": date}, _MODULES[__name__]
        )

    def qh_inventory(self, code: str) -> F10Response:
        """期货库存报告（``CWServ.qhf9_sp_kcbg_kcbg``）。

        注意：实测部分品种返回 0 行，参数形式可能因品种而异，
        调用前建议先用 ``params`` 直接试探。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "qh_inventory", code, None, _MODULES[__name__]
        )

    def qh_variety_info(self, code: str, section: str = "pzjj") -> F10Response:
        """期货品种资料（``CWServ.qhf9_*_pzzl_*``）。

        section: ``pzjj`` 品种简介（``[code, "1"]``）/ ``hyjj`` 行业简介 /
        ``xgzq`` 相关证券（后两者为 ``[code]`` 单参）。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_sector_endpoint(
            self, "qh_variety_info", code, section, {}, _MODULES[__name__]
        )

    def qh_index_info(self, code: str, section: str = "zsgk") -> F10Response:
        """股指期货指数资料（``CWServ.qhf9_gz_zszl_*``）。

        section: ``zsgk`` 指数概况 / ``lsjg`` 历史价格。
        ``code`` 为指数代码（实测 ``399300`` 沪深300 可用，``000300`` 返回空）。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_sector_endpoint(
            self, "qh_index_info", code, section, {}, _MODULES[__name__]
        )

    def qh_index_valuation(
        self, code: str = "000300", section: str = "dtpe", *, start: str = "", end: str = ""
    ) -> F10Response:
        """股指期货指数估值（``CWServ.qhf9_gz_zsgz_*``）。

        section: ``dtpe`` 动态市盈率 / ``lspe`` 历史市盈率 / ``xjfh`` 现金分红。
        ``code`` 为指数代码（客户端写死 ``000300`` 沪深300）；``start``/``end``
        为日期区间（``YYYYMMDD``）。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_sector_endpoint(
            self, "qh_index_valuation", code, section, {"start": start, "end": end}, _MODULES[__name__]
        )

    def qh_weights(self, code: str = "399300", section: str = "zxcfg", *, date: str = "") -> F10Response:
        """股指期货成份权重（``CWServ.qhf9_gz_cfg_*``）。

        section: ``zxcfg`` 最新成份 / ``lscfg`` 历史成份 / ``cfgqz`` 成份权重 /
        ``cfghyfb`` 成份行业分布 / ``xwpl`` 新闻评论。
        ``code`` 为指数代码（客户端用 ``zsinfo.code``，IF→``399300``）；
        ``date`` 为截止日期（``YYYYMMDD``，空为最新）。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_sector_endpoint(
            self, "qh_weights", code, section, {"date": date}, _MODULES[__name__]
        )

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

        from eltdx._native_abi import load_native

        return load_native().f10_report(
            self, "market_report", code, section,
            {"arg": arg, "page": page, "page_size": page_size}, _MODULES[__name__]
        )

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

        from eltdx._native_abi import load_native

        return load_native().f10_collect(
            self, "index_members", code, all_pages, page_size, max_pages, _MODULES[__name__]
        )

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

        from eltdx._native_abi import load_native

        return load_native().f10_collect(
            self, "index_tracking_products", code, all_pages, page_size, max_pages, _MODULES[__name__]
        )

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

        from eltdx._native_abi import load_native

        return load_native().f10_collect(
            self, "board_members", code, all_pages, page_size, max_pages, _MODULES[__name__]
        )

    def ipo_calendar(self) -> IpoCalendar:
        """近期新股列表（含申购日、发行价、募资额）。

        底层 Entry 为 ``CWServ.skef10_dpzx_xgfx``，参数 ``002``，实测约 50 行，
        覆盖沪深北三个市场，按日期倒序。这是 TQ ``get_ipo_info`` 的等价能力。

        字段确认状态：``N001`` 日期、``N002`` 代码、``N003`` 名称、``N006`` 发行价、
        ``N008`` 募资总额已与公开数据核对一致；``N004`` / ``N005`` / ``N007``
        服务端未标注，这里按语义命名为 ``issue_pe`` / ``extra_pct`` / ``apply_limit``，
        **均为推断**，请用行内 ``raw`` 复核后再用于生产。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_collect(
            self, "ipo_calendar", None, None, None, None, _MODULES[__name__]
        )

    def dividend_financing(self, code: str, section: str = "fh") -> F10Response:
        """分红融资类数据。section=fh 为分红方案历史。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "dividend_financing", code, section, _MODULES[__name__]
        )

    def allotment_dates(self, code: str) -> F10Response:
        """增发获配明细可选日期。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "allotment_dates", code, None, _MODULES[__name__]
        )

    def allotment_details(self, code: str, date: str) -> F10Response:
        """指定增发日期的获配机构明细。"""

        from eltdx._native_abi import load_native

        return load_native().f10_input_endpoint(
            self, "allotment_details", code, None, {"date": date}, _MODULES[__name__]
        )

    def finance_report(self, code: str, report_type: str = "zcfzb") -> F10Response:
        """财务报表。report_type=zcfzb 为资产负债表。"""

        from eltdx._native_abi import load_native

        return load_native().f10_input_endpoint(
            self, "finance_report", code, None, {"report_type": report_type}, _MODULES[__name__]
        )

    def finance_diagnosis(self, code: str, section: str = "zzyj", scope: str = "") -> F10Response:
        """财务诊断（增减异常）。

        实测（2026-10-22）：服务端仅 ``zzyj``（增减异常）子表有数据；
        历史 section（``yynl`` 盈利能力等）已随页面改版下线，传了返回空。
        ``t0`` 为标志行（``isjr``），``t1`` 才是异常科目列表。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_input_endpoint(
            self, "finance_diagnosis", code, section, {"scope": scope}, _MODULES[__name__]
        )

    def stock_score(self, code: str, section: str = "pf", arg: str = "") -> F10Response:
        """个股总评。section=pf 为综合评分。"""

        from eltdx._native_abi import load_native

        return load_native().f10_input_endpoint(
            self, "stock_score", code, section, {"arg": arg}, _MODULES[__name__]
        )

    def profit_forecast(self, code: str, section: str = "ylyctj") -> F10Response:
        """盈利预测评级统计。section=ylyctj 为盈利预测统计，ylycmx 为盈利预测明细，
        yzyq 为一致预期，tzpjtj 为投资评级统计，ycpjyjbg 为研报。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "profit_forecast", code, section, _MODULES[__name__]
        )

    def ranking_detail(self, code: str, section: str = "scpmdela") -> F10Response:
        """市场排名 / 行业排名明细。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "ranking_detail", code, section, _MODULES[__name__]
        )

    def governance(self, code: str, section: str = "gljy", arg: str = "") -> F10Response:
        """资本运作治理。section=gljy 为关联交易（数据最全）。

        现行页面（gg_zbyz.js）的 5 个 section：``dbmx`` 担保明细 /
        ``gljy`` 关联交易 / ``gqtz`` 股权转让 / ``xmtz`` 项目投资 /
        ``zjly`` 资金运用。历史 section ``wgcl``（违规处理）服务端仍返回
        （多数公司 0 行，属数据缺失而非接口问题）。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_input_endpoint(
            self, "governance", code, section, {"arg": arg}, _MODULES[__name__]
        )

    def hot_topics(self, code: str, section: str = "zttzbkz") -> F10Response:
        """热点题材。section=zttzbkz 为板块题材。"""

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "hot_topics", code, section, _MODULES[__name__]
        )

    def topic_compare(self, code: str, topic_id: str, section: str = "gndbzfsj", sort_by: str = "zdf") -> F10Response:
        """题材内对比排名。"""

        from eltdx._native_abi import load_native

        return load_native().f10_input_endpoint(
            self, "topic_compare", code, section, {"topic_id": topic_id, "sort_by": sort_by}, _MODULES[__name__]
        )

    def topic_compare_first(self, code: str, section: str = "gndbzfsj", sort_by: str = "zdf") -> F10Response:
        """用股票第一个题材 ID 查询题材内对比排名。"""

        from eltdx._native_abi import load_native

        return load_native().f10_topic_compare_first(self, code, section, sort_by, _MODULES[__name__])

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

        from eltdx._native_abi import load_native

        return load_native().f10_page_endpoint(
            self, "company_news", code, section, {"keyword": keyword, "rating": rating, "page": page, "page_size": page_size}, _MODULES[__name__]
        )

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

        from eltdx._native_abi import load_native

        return load_native().f10_report(
            self, "digest", code, section, {"page_size": page_size}, _MODULES[__name__]
        )

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

        from eltdx._native_abi import load_native

        return load_native().f10_input_endpoint(
            self, "northbound_holding", code, section, {"filter_value": filter_value, "page": page, "page_size": page_size}, _MODULES[__name__]
        )

    def detail(self, detail_type: str, record_id: str | int) -> F10Response:
        """按记录 ID 查询详情正文。"""

        from eltdx._native_abi import load_native

        return load_native().f10_input_endpoint(
            self, "detail", None, None, {"detail_type": detail_type, "record_id": record_id}, _MODULES[__name__]
        )

    def cache_list(self, code: str, kind: str = "gg") -> F10Response:
        """新闻 / 公告 / 路演缓存列表。kind=xw、gg、ly。"""

        from eltdx._native_abi import load_native

        return load_native().f10_cache_list(self, code, kind, _MODULES[__name__])

    def announcements(self, code: str) -> F10Response:
        """公告列表。"""

        from eltdx._native_abi import load_native

        return load_native().f10_cache_alias(self, "announcements", code)

    def news(self, code: str) -> F10Response:
        """新闻列表。"""

        from eltdx._native_abi import load_native

        return load_native().f10_cache_alias(self, "news", code)

    def roadshows(self, code: str) -> F10Response:
        """路演列表。"""

        from eltdx._native_abi import load_native

        return load_native().f10_cache_alias(self, "roadshows", code)

    # ------------------------------------------------------------------
    # 市场资讯（通达信 7615「资讯」页；枚举与字段说明见 eltdx.f10.news）
    # ------------------------------------------------------------------
    def news_channels(self) -> list[tuple[int, str]]:
        """返回全部资讯频道 ``[(频道号, 栏目名), ...]``（静态枚举，不发网络请求）。

        频道号用于 :meth:`channel_news`；分组见 :data:`eltdx.f10.news.NEWS_CHANNEL_GROUPS`。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_news_channels(_MODULES[__name__])

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

        from eltdx._native_abi import load_native

        return load_native().f10_input_endpoint(
            self, "market_news", None, None, {"tab": tab, "page": page, "page_size": page_size}, _MODULES[__name__]
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

        from eltdx._native_abi import load_native

        return load_native().f10_input_endpoint(
            self, "channel_news", None, None, {"channel": channel, "page": page, "page_size": page_size, "query1": query1, "keyword": keyword}, _MODULES[__name__]
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

        from eltdx._native_abi import load_native

        return load_native().f10_object_request(
            self, "theme_market", code,
            {"req_id": req_id, "page": page, "page_size": page_size, "zq_num": zq_num, "extra": extra},
            _MODULES[__name__]
        )

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

        from eltdx._native_abi import load_native

        return load_native().f10_object_alias(self, "stock_stage_gain", code, {"days": days, "page_size": page_size})

    def stock_main_funds(
        self, code: str, *, days: int | str = 30, page_size: int = 10
    ) -> F10Response:
        """个股主力资金流日序列（``hq_nlp_tcihq`` ReqId 200742）。

        ``t1`` 每行：``N001`` 行业名、``N002`` 日期、``N003~N006``
        主力流入/流出/净额/占比（口径随服务端）。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_object_alias(self, "stock_main_funds", code, {"days": days, "page_size": page_size})

    def stock_industry_rank(
        self, code: str, *, days: int | str = 30, page_size: int = 10
    ) -> F10Response:
        """个股所属行业与行业内排名（``hq_nlp_tcihq`` ReqId 200745）。

        ``t0`` 为归属信息：``hy_setcode`` / ``hy_code`` / ``hy_name`` /
        ``cfg_num``（行业内标的数）/ ``rank``（排名）；
        ``t1`` 为行业指数近期走势（``N001`` 日期 / ``N002`` 点位）。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_object_alias(self, "stock_industry_rank", code, {"days": days, "page_size": page_size})

    def stock_industry_pct_rank(
        self, code: str, *, days: int | str = 30, page_size: int = 10
    ) -> F10Response:
        """个股在所属行业内的日涨跌幅排名（``hq_nlp_tcihq`` ReqId 200747）。

        ``t1`` 每行：``N001`` 日期、``N002`` 嵌套数组
        ``[排名, setcode, code, name, 涨跌幅]``。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_object_alias(self, "stock_industry_pct_rank", code, {"days": days, "page_size": page_size})

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

        from eltdx._native_abi import load_native

        return load_native().f10_object_request(
            self, "stock_board_member_quotes", board_code,
            {"board_market": board_market, "page": page, "page_size": page_size}, _MODULES[__name__]
        )

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

        from eltdx._native_abi import load_native

        return load_native().f10_object_alias(
            self, "stock_valuation_percentile", code, {"kind": kind, "page": page, "page_size": page_size}
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

        from eltdx._native_abi import load_native

        return load_native().f10_object_alias(
            self, "stock_pepb_percentile", code,
            {"indicator": indicator, "window": window, "period": period, "page": page, "page_size": page_size}
        )

    def stock_history_trend(self, code: str) -> F10Response:
        """个股年度历史走势（``hq_nlp_gpsj`` ReqId 200135）。

        ``t0`` 每行一个年度（``N001`` 年份、``N002~N014`` 年度涨跌幅与估值）。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_object_alias(self, "stock_history_trend", code, {})

    def stock_finance_weights(self, code: str, section: str = "zcfzb") -> F10Response:
        """财务指标权重（``CWServ.tdxf10_gg_zxtscwbq``，最新提示页）。

        ``section`` 为财务报告期 key（由最新提示页的财务概况列表给出）。
        实测部分 key 返回空——属数据缺失，调用前建议先取 ``digest``。
        """

        from eltdx._native_abi import load_native

        return load_native().f10_simple_endpoint(
            self, "stock_finance_weights", code, section, _MODULES[__name__]
        )

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

        from eltdx._native_abi import load_native

        return load_native().f10_object_request(
            self, "valuation", code,
            {"req_id": req_id, "page": page, "page_size": page_size, "extra": extra}, _MODULES[__name__]
        )

    def _post(self, entry: str, body: Any) -> F10Response:
        from eltdx._native_abi import load_native

        request = load_native().f10_prepare_request(self, entry, body, _MODULES[__name__])
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
    from eltdx._native_abi import load_native

    return load_native().f10_code6(_MODULES[__name__], code)


def _us_code(code: str) -> str:
    """美股代码规范化 → 裸大写代码（如 ``NVDA``）。

    接受 ``NVDA`` / ``nvda`` / ``us.NVDA`` / ``74.NVDA``，去掉市场前缀、转大写。
    美股代码含字母，可能带 ``-`` / ``.``（如 ``BRK-B``），不做长度强制。
    """

    from eltdx._native_abi import load_native

    return load_native().f10_normalize_code(code, "us", _MODULES[__name__])


def _hk_code(code: str) -> str:
    """港股代码规范化 → 5 位数字代码（如 ``00700``）。

    接受 ``00700`` / ``700`` / ``hk.00700`` / ``31.00700``，去掉市场前缀，
    前导补零到 5 位。
    """

    from eltdx._native_abi import load_native

    return load_native().f10_normalize_code(code, "hk", _MODULES[__name__])


def _bond_code(code: str) -> str:
    """债券代码规范化 → 6 位数字代码（如 ``113043``）。

    接受 ``113043`` / ``sh113043`` / ``sz128043``；不推断市场（沪深债券
    代码段高度重叠，如 ``110``/``113`` 沪深都有），仅取 6 位数字。
    """

    from eltdx._native_abi import load_native

    return load_native().f10_normalize_code(code, "bond", _MODULES[__name__])


def _bond_sc(code: str) -> str:
    """债券 F10 的市场码参数 ``sc``（实测：``1`` 沪 / ``0`` 深）。

    与 ``MacClient`` 的 market 约定一致。无法从代码推断时（如未带前缀的
    裸代码且段重叠）默认 ``1``（沪市债券段更常见）；显式 ``sh``/``sz``
    前缀优先。
    """

    from eltdx._native_abi import load_native

    return load_native().f10_normalize_code(code, "bond_market", _MODULES[__name__])


def _require_section(section: str, valid: Sequence[str]) -> None:
    """Raise ``ValueError`` listing the valid ``section`` values."""

    from eltdx._native_abi import load_native

    return load_native().f10_require_section(section, valid, _MODULES[__name__])


def _section_request(
    section: str,
    table: Mapping[str, tuple[str, tuple[Any, ...]]],
) -> tuple[str, tuple[Any, ...]]:
    """Return the ``(entry, params)`` pair registered for ``section``."""

    from eltdx._native_abi import load_native

    return load_native().f10_section_request(section, table, _MODULES[__name__])


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
