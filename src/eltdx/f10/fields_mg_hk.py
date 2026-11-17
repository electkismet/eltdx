"""美股(mgf10) / 港股(hkf10) F10 字段中文映射。

与 A 股 :mod:`eltdx.f10.fields` 分离：mg/hk 的列名以英文/拼音为主，
泛化 ``Nxxx`` / ``Txxx`` 在不同 Entry 下语义不同，同样按 ``(entry, scope)``
精确定位。scope 取请求 ``Params`` 的第一个元素（section / 子标签）。

未能可靠推断语义的列名**不写入映射**，调用方按 :func:`describe_mg_hk_fields`
的 ``unmapped`` 保留原名（例如美股三大报表的 ``N003~N069`` 科目号）。
"""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from sys import modules as _MODULES
from typing import Any

# ---------------------------------------------------------------------------
# 美股 mgf10
# ---------------------------------------------------------------------------

_MG_NAME = {
    "T003": "股票名称",
    "T009": "股票代码",
    "name": "名称",
}

_MG_PROFILE = {
    "f11": "代码",
    "f12": "名称",
    "f13": "交易所",
    "f14": "上市日期",
    "f15": "所属行业",
    "f16": "公司官网",
}

_MG_QUOTE_SNAP = {
    "T003": "日期",
    "T055": "开盘价",
    "T080": "收盘价",
    "T045": "最高价",
    "T025": "最低价",
    "T073": "成交量",
    "T074": "成交额",
    "T056": "成交股数",
    "T057": "成交金额",
    "T077": "均价",
    "T079": "前收盘",
}

#: 美股三大报表（cwfx）通用列；科目号 ``N003~N069`` 语义随报表而变，暂不映射。
_MG_FINANCE_COMMON = {
    "year": "年度",
    "rq": "报告期",
    "bgid": "报告期ID",
    "bgtype": "报告期类型",
}

#: 美股资产负债表（cwfx / zcfzb）科目号。数值经 NVIDIA FY2027Q2 10-Q（2026-07-26，
#: 单位百万美元）逐项核对；加和关系：N014+N027=N029=N067，N054+N063=N067。
_MG_BALANCE_SHEET = {
    "N003": "现金及现金等价物",
    "N006": "应收账款净额",
    "N008": "预付款项及其他流动资产",
    "N009": "存货",
    "N012": "有价证券合计",  # 交易性债务证券 + 交易性权益证券
    "N014": "流动资产合计",
    "N015": "固定资产净额",
    "N019": "无形资产净额",
    "N020": "商誉",
    "N024": "递延所得税资产",
    "N025": "非交易性证券",
    "N026": "其他非流动资产",  # 经营租赁资产 + 其他资产
    "N027": "非流动资产合计",
    "N029": "资产总计",
    "N031": "短期债务",
    "N033": "应付账款",
    "N037": "应计及其他流动负债",
    "N042": "流动负债合计",
    "N043": "非流动负债",  # 长期债务 + 其他长期负债（不含经营租赁）
    "N044": "长期经营租赁负债",
    "N052": "非流动负债合计",  # 含长期经营租赁
    "N054": "负债合计",
    "N055": "普通股",
    "N058": "额外实收资本",
    "N059": "留存收益",
    "N061": "累计其他综合收益",
    "N063": "股东权益合计",
    "N066": "股东权益合计",
    "N067": "负债和股东权益总计",
}

#: 美股利润表（cwfx / lyb）科目号。经 NVDA FY2027Q2 10-Q 核对：
#: N003-N006=N010（毛利），N010-N011-N012=N018（营业利润），N018+N023=N026，N026-N027=N028。
_MG_INCOME = {
    "N003": "营业总收入",
    "N004": "营业收入",
    "N006": "营业成本",
    "N007": "营业成本",
    "N010": "毛利",
    "N011": "研发费用",
    "N012": "销售及管理费用",
    "N018": "营业利润",
    "N023": "非经营性收益",  # 利息收入、投资收益等
    "N026": "税前利润",
    "N027": "所得税费用",
    "N028": "净利润",
    "N031": "归属母公司净利润",
    "N033": "持续经营净利润",
    "N035": "扣除非经常损益后净利润",
    "N036": "基本每股收益",
    "N037": "摊薄每股收益",
}

#: 美股现金流量表（cwfx / xjllb）科目号。仅映射加和关系明确的小计项。
_MG_CASHFLOW = {
    "N004": "净利润",
    "N031": "经营活动现金流净额",
    "N039": "投资活动现金流净额",
    "N048": "筹资活动现金流净额",
    "N059": "筹资活动现金流入",
    "N060": "筹资活动现金流出",
    "N063": "期初现金及等价物",
    "N064": "现金及等价物净增加额",
    "N065": "期末现金及等价物",
}

_MG_BUSINESS = {
    "T005": "业务名称",
    "T006": "收入",
    "zb": "占比(%)",
    "T007": "成本",
    "mll": "毛利率(%)",
    "lb": "类别",
    "rq": "报告期",
}

_MG_INDUSTRY = {
    "N001": "行业名称",
    "N002": "行业代码",
    "N003": "日期",
}

_MG_INDUSTRY_COMPARE = {
    "N001": "排名",
    "N002": "行业名称",
    "N003": "公司数",
    "N004": "涨跌幅(%)",
    "N005": "领涨股",
}

_MG_DIVIDEND_FH = {
    "aT003": "公告日期",
    "T005": "每股股利",
    "aT007": "除权日",
    "aT006": "派息日",
    "aT008": "股权登记日",
}

_MG_DIVIDEND_CF = {
    "aT003": "公告日期",
    "T005": "融资金额",
    "T004": "融资方式",
    "aT006": "生效日期",
    "aT007": "发行价格",
}

_MG_CAPITAL_001 = {
    "T003": "日期",
    "T004": "总股本",
}

_MG_CAPITAL_002 = {
    "N001": "公告日期",
    "N002": "变动股数",
    "N003": "变动后股数",
    "N004": "变动比例(%)",
    "N005": "公告类型",
    "N006": "生效日期",
    "zs": "总条数",
}

_MG_SHAREHOLDERS = {
    "N001": "代码",
    "N002": "报告期",
    "N003": "股东户数",
    "N004": "股东户数变动",
    "N005": "持股总数",
    "N006": "持股变动",
    "N007": "持股占比(%)",
    "N008": "占比变动(%)",
}

_MG_DIRECTORS = {
    "T002": "姓名",
    "T003": "职务",
    "T005": "年龄",
    "T006": "性别",
    "rq": "任职日期",
    "T007": "个人简介",
    "T008": "教育背景",
    "T009": "任职经历",
    "T010": "兼任职务",
    "rec_id": "记录ID",
}

_MG_MARKET = {
    "N001": "日期",
    "N002": "开盘价",
    "N003": "最高价",
    "N004": "最低价",
    "N005": "收盘价",
    "N006": "涨跌额",
    "N007": "涨跌幅(%)",
    "N008": "成交量",
    "N009": "成交额",
    "zs": "总条数",
}

_MG_ANNOUNCEMENTS = {
    "N001": "公告类型",
    "Title": "公告标题",
    "N003": "公告链接",
    "rq": "公告日期",
    "currq": "当前报告期",
    "rec_id": "记录ID",
}

_MG_RATING = {
    "N001": "日期",
    "N002": "周期",
    "N003": "买入",
    "N004": "增持",
    "N005": "中性",
    "N006": "减持",
    "N007": "卖出",
    "N008": "评级机构数",
}

_MG_TOPIC = {
    "N002": "入选日期",
    "N003": "题材名称",
    "N004": "关联个股数",
    "N005": "题材日期",
    "N006": "题材描述",
    "N007": "题材ID",
}

_MG_DEEP_ANALYSIS = {
    "year": "年度",
    "N002": "指标名称",
    "N003": "本期值",
    "N004": "上期值",
    "N005": "行业均值",
    "N006": "行业排名",
    "N007": "总分",
    "N008": "评级",
}

# ---------------------------------------------------------------------------
# 港股 hkf10
# ---------------------------------------------------------------------------

_HK_NAME = {
    "name": "名称",
    "sc": "市场代码",
}

_HK_TIPS = {
    "lb": "类别代码",
    "lbname": "类别名称",
    "contents": "内容",
}

_HK_EVENTS_DIVIDEND = {
    "InfoDate": "公告日期",
    "Summary": "摘要",
    "ExDate": "除息日",
    "PayDate": "派息日",
    "CDateBegin": "截止日期起",
    "CdateEnd": "截止日期止",
    "bz": "币种",
    "gx": "每股股息",
    "cashcount": "分红次数",
}

_HK_EVENTS_CAPITAL = {
    "DeclareDate": "公告日期",
    "IssPaidNum": "已发行股数",
    "OrdNum": "普通股数",
    "HOrd": "H股数",
    "Reason": "变动原因",
    "EndDate": "变动日期",
}

_HK_TOPIC = {
    "N002": "题材名称",
    "N003": "关联度",
    "N004": "入选原因",
    "N005": "题材ID",
    "N006": "入选日期",
}

_HK_PROFILE_1 = {
    "ComName": "公司中文名",
    "StocksName": "股票简称",
    "Sort": "证券类别",
    "RegPla": "注册地",
    "RegPlaInfo": "注册地详情",
    "WebSite": "公司官网",
    "SRegister": "注册办事处",
    "Fax": "传真",
    "Auditors": "审计师",
    "enname": "英文名称",
    "Sector1": "所属行业",
    "Njr": "年报编制人",
    "clrq": "成立日期",
    "StafSum": "员工人数",
    "Email": "电子邮箱",
    "Tel": "电话",
    "HOffice": "香港办事处",
    "LawDvisors": "法律顾问",
    "Activities": "主营业务",
    "Bankers": "往来银行",
    "Introduce": "公司简介",
    "url": "相关链接",
    "Secretary": "公司秘书",
    "Name": "姓名",
    "POSITION": "职位代码",
}

_HK_PROFILE_2 = {
    "StockCode": "股票代码",
    "T016": "上市日期",
    "T021": "已发行股本",
    "T022": "已发行H股",
    "T023": "已发行股份",
    "T024": "每股面值",
    "T014": "面值",
    "T020": "发售方式",
    "T030": "招股日期",
    "T026": "保荐人",
    "T027": "主承销商",
    "T028": "账簿管理人",
    "T003": "公司简介",
    "T004": "公司地址",
    "T025": "包销商",
    "jybz": "交易币种",
}

_HK_EXECUTIVES = {
    "rec_id": "记录ID",
    "ggname": "姓名",
    "PositionName": "职务",
    "BirthYear": "出生年份",
    "Sex": "性别",
    "Degree": "学历",
    "Nationality": "国籍",
    "PostDate": "任职日期",
    "N001": "年度",
    "N002": "薪酬(万元)",
    "N003": "币种",
    "N004": "津贴",
    "N005": "奖金",
    "N006": "持股数",
    "N007": "持股比例(%)",
}

_HK_CAPITAL = {
    "DeclareDate": "公告日期",
    "EndDate": "变动日期",
    "Src": "来源",
    "IssPaidNum": "已发行实缴股数",
    "OrdNum": "普通股数",
    "Hord": "H股数",
    "Reason": "变动原因",
    "T011": "备注",
}

_HK_SHAREHOLDERS = {
    "T005": "报告期",
    "T008": "股东名称",
    "T009": "实际权益人",
    "T010": "持股数(万股)",
    "T011": "持股变动",
    "T012": "持股比例(%)",
    "T013": "股份类别",
}

_HK_FINANCE = {
    "isjrl": "是否节假日",
    "bgq": "报告期",
    "StartDate": "开始日期",
    "EndDate": "结束日期",
    "TAssets": "总资产",
    "NAssets": "净资产",
    "ProPlaEqu": "固定资产",
    "AJEquity": "调整后权益",
    "CAssets": "流动资产",
    "Inventory": "存货",
    "RAccounts": "应收账款",
    "CasDep": "现金及银行存款",
    "TLiab": "总负债",
    "CLiab": "流动负债",
    "PAccounts": "应付账款",
    "SBLoan": "短期借款",
    "CANet": "流动资产净额",
    "NLiab": "非流动负债",
    "LBLoan": "长期借款",
    "SHEquity": "股东权益",
    "MEquity": "少数股东权益",
    "SCapital": "股本",
    "Currency": "币种",
    "InfoDate": "信息日期",
    "LastEndDate": "上期结束日",
    "ProfitBT": "税前利润",
    "AJProfit": "调整后利润",
    "ProfitFO": "股东应占利润",
    "TurnOver": "营业额",
    "ONetCash": "经营现金流",
    "CExpense": "经营支出",
    "INetCash": "投资现金流",
    "FNetCash": "融资现金流",
}

_HK_FINANCE_INDICATOR = {
    "isjrl": "是否节假日",
    "Year": "年度",
    "type": "报告期类型",
    "EndDate": "结束日期",
    "bgq": "报告期",
    "EPS1": "每股收益",
    "EPSyear1": "每股收益(年化)",
    "NTA": "每股净资产",
    "CashPShare1": "每股现金流",
    "DivE": "每股股息",
    "DivP": "股息率(%)",
    "Oincome": "营业收入",
    "ProfitFO": "股东应占利润",
    "SHProfit": "归母净利润",
    "FCF": "自由现金流",
    "ONetCash": "经营现金流",
    "Currency": "币种",
}

_HK_BUSINESS = {
    "rq": "报告期",
    "nd": "年度",
    "jd": "季度",
    "lb": "类别代码",
    "lbmc": "类别名称",
    "xmmc": "项目名称",
    "yysr": "营业收入",
    "bz": "币种",
    "zb": "占比(%)",
}

_HK_INDUSTRY_COMPARE = {
    "bgrq": "报告日期",
    "bgqlx": "报告期类型",
    "xh": "序号",
    "StockCode": "股票代码",
    "StocksName": "股票简称",
    "ROE": "净资产收益率(%)",
    "EPSBasic": "基本每股收益",
    "NTA1": "每股净资产",
}

_HK_MARKET_SUSPEND = {
    "tpggrq": "停牌公告日",
    "tprq": "停牌日期",
    "tpyy": "停牌原因",
    "fpggrq": "复牌公告日",
    "fprq": "复牌日期",
}

_HK_MARKET_AB = {
    "acode": "A股代码",
    "aname": "A股名称",
    "adate": "日期",
    "aprice": "A股价格",
    "azdf": "A股涨跌幅(%)",
    "hkcode": "H股代码",
    "hkprice": "H股价格",
    "hkdj": "H股涨跌幅(%)",
    "ahzj": "A/H溢价(%)",
}

_HK_RELATED = {
    "N001": "代码",
    "N002": "名称",
    "N003": "涨跌幅(%)",
    "N004": "最新价",
    "N005": "成交量",
    "N006": "成交额",
    "N007": "5日涨跌(%)",
    "N008": "10日涨跌(%)",
    "N009": "20日涨跌(%)",
    "N010": "60日涨跌(%)",
    "N011": "月涨跌(%)",
    "N012": "季涨跌(%)",
    "N013": "年涨跌(%)",
    "N014": "标记",
}

_HK_VALUATION = {
    "N001": "日期",
    "N002": "市盈率",
    "N003": "市净率",
    "N004": "市销率",
    "N005": "涨跌幅(%)",
    "N006": "股价",
    "N007": "总市值",
    "N008": "币种",
}

_HK_PRICE_CHART = {
    "year": "年度",
    "N001": "日期",
    "N002": "开盘价",
    "N003": "最高价",
    "N004": "最低价",
    "N005": "收盘价",
    "N006": "成交量",
    "N007": "成交额",
}

_HK_ANALYST_FORECAST = {
    "N001": "预测期",
    "N002": "机构数",
    "N003": "目标价",
    "N004": "评级",
}

# ``(entry, scope)`` -> 列名映射。scope 为请求 Params 的第一个元素。
FIELD_LABELS: dict[tuple[str, str], Mapping[str, str]] = {
    # ---- 美股 ----
    ("CWServ.skef10_mg_idreq", "name"): _MG_NAME,
    ("CWServ.skef10_mg_gsgk", "001"): _MG_PROFILE,
    ("CWServ.skef10_mg_zxdt", "zxdt"): _MG_QUOTE_SNAP,
    ("CWServ.skef10_mg_zxdt", "sypl"): _MG_QUOTE_SNAP,
    ("CWServ.skef10_mg_cwfx", "zcfzb"): {**_MG_FINANCE_COMMON, **_MG_BALANCE_SHEET},
    ("CWServ.skef10_mg_cwfx", "lyb"): {**_MG_FINANCE_COMMON, **_MG_INCOME},
    ("CWServ.skef10_mg_cwfx", "xjllb"): {**_MG_FINANCE_COMMON, **_MG_CASHFLOW},
    ("CWServ.skef10_mg_jyfx", "*"): _MG_BUSINESS,
    ("CWServ.skef10_mg_hyfx", "001"): _MG_INDUSTRY,
    ("CWServ.skef10_mg_hydb", "001"): _MG_INDUSTRY_COMPARE,
    ("CWServ.skef10_mg_fhrz", "fh"): _MG_DIVIDEND_FH,
    ("CWServ.skef10_mg_fhrz", "cf"): _MG_DIVIDEND_CF,
    ("CWServ.skef10_mg_gbjg", "001"): _MG_CAPITAL_001,
    ("CWServ.skef10_mg_gbjg", "002"): _MG_CAPITAL_002,
    ("CWServ.skef10_mg_gbgd", "006"): _MG_SHAREHOLDERS,
    ("CWServ.skef10_mg_gbgd", "003"): _MG_SHAREHOLDERS,
    ("CWServ.skef10_mg_dsgg", "ds"): _MG_DIRECTORS,
    ("CWServ.skef10_mg_dsgg", "gg"): _MG_DIRECTORS,
    ("CWServ.skef10_mg_dsgg", "lrdjg"): _MG_DIRECTORS,
    ("CWServ.skef10_mg_schq", "001"): _MG_MARKET,
    ("CWServ.skef10_mg_gsbd", "gsgg"): _MG_ANNOUNCEMENTS,
    ("CWServ.skef10_mg_gspj", "001"): _MG_RATING,
    ("CWServ.skef10_mg_rdtc", "001"): _MG_TOPIC,
    ("CWServ.skef10_mg_sdfx", "001"): _MG_DEEP_ANALYSIS,
    ("CWServ.skef10_mg_sdfx", "002"): _MG_DEEP_ANALYSIS,
    # ---- 港股 ----
    ("CWServ.skef10_hk_idreq", "name"): _HK_NAME,
    ("CWServ.skef10_hk_zxts", "1"): _HK_TIPS,
    ("CWServ.skef10_hk_gsds", "1"): _HK_EVENTS_DIVIDEND,
    ("CWServ.skef10_hk_gsds", "2"): _HK_EVENTS_CAPITAL,
    ("CWServ.skef10_hk_gsgk", "1"): _HK_PROFILE_1,
    ("CWServ.skef10_hk_gsgk", "2"): _HK_PROFILE_2,
    ("CWServ.skef10_hk_ggzl", "1"): _HK_EXECUTIVES,
    ("CWServ.skef10_hk_ggzl", "2"): _HK_EXECUTIVES,
    ("CWServ.skef10_hk_ggzl", "3"): _HK_EXECUTIVES,
    ("CWServ.skef10_hk_gbjg", "1"): _HK_CAPITAL,
    ("CWServ.skef10_hk_gbjg", "2"): _HK_CAPITAL,
    ("CWServ.skef10_hk_gbjg", "3"): _HK_CAPITAL,
    ("CWServ.skef10_hk_gdcg", "1"): _HK_SHAREHOLDERS,
    ("CWServ.skef10_hk_cwfx", "1"): _HK_FINANCE,
    ("CWServ.skef10_hk_cwfx", "2"): _HK_FINANCE,
    ("CWServ.skef10_hk_cwfx", "3"): _HK_FINANCE,
    ("CWServ.skef10_hk_cwzb", "1"): _HK_FINANCE_INDICATOR,
    ("CWServ.skef10_hk_cwzb", "2"): _HK_FINANCE_INDICATOR,
    ("CWServ.skef10_hk_cwzb", "3"): _HK_FINANCE_INDICATOR,
    ("CWServ.skef10_hk_cwzb", "4"): _HK_FINANCE_INDICATOR,
    ("CWServ.skef10_hk_cwzb", "5"): _HK_FINANCE_INDICATOR,
    ("CWServ.skef10_hk_jyfx", "1"): _HK_BUSINESS,
    ("CWServ.skef10_hk_hybj", "1"): _HK_INDUSTRY_COMPARE,
    ("CWServ.skef10_hk_hybj", "3"): _HK_INDUSTRY_COMPARE,
    ("CWServ.skef10_hk_schq", "1"): _HK_MARKET_SUSPEND,
    ("CWServ.skef10_hk_schq", "2"): _HK_MARKET_AB,
    ("CWServ.skef10_hk_glzq", "6"): _HK_RELATED,
    ("CWServ.skef10_hk_sdfx_gzfx", "*"): _HK_VALUATION,
    ("CWServ.skef10_hk_sdfx_hqzft", "*"): _HK_PRICE_CHART,
    ("CWServ.skef10_hk_sdfx_fxysyfx", "*"): _HK_ANALYST_FORECAST,
    ("CWServ.skef10_hk_rdtc", "*"): _HK_TOPIC,
}

#: scope 不区分时的通配 key。
_WILDCARD = "*"

#: 少数 Entry 的 Params 是 ``[code, section]``（section 在索引 1）而非 ``[section, ...]``。
_SCOPE_PARAM_INDEX: dict[str, int] = {
    "CWServ.skef10_mg_fhrz": 1,
    "CWServ.skef10_mg_dsgg": 1,
    # 新三板 sb 家族统一 ``[code, section]``。
    "CWServ.skef10_sb_gsgk": 1,
    "CWServ.skef10_sb_zxts": 1,
    "CWServ.skef10_sb_jyds": 1,
    "CWServ.skef10_sb_gbjg": 1,
    "CWServ.skef10_sb_gdyj": 1,
    "CWServ.skef10_sb_hgyy": 1,
    "CWServ.skef10_sb_fhrz": 1,
    "CWServ.skef10_sb_cwfx": 1,
    "CWServ.skef10_sb_jyfx": 1,
    "CWServ.skef10_sb_zdsx": 1,
    "CWServ.skef10_sb_gszx": 1,
    "CWServ.skef10_sb_hybj": 1,
}


def _scope(entry: str, request_body: Any) -> str | None:
    """从请求 ``Params`` 取 section（默认索引 0，个别 Entry 见 _SCOPE_PARAM_INDEX）。"""

    from eltdx._native_abi import load_native

    return load_native().f10_field_scope(
        entry, request_body, None, True, _MODULES[__name__]
    )


def _split_duplicate_suffix(field: str) -> tuple[str, int | None]:
    from eltdx._native_abi import load_native

    return load_native().f10_duplicate_suffix(field, _MODULES[__name__])


def labels_for(entry: str, request_body: Any) -> dict[str, str]:
    """返回该 Entry+scope 下已知的字段中文映射；未知 scope 返回空 dict。"""

    from eltdx._native_abi import load_native

    return load_native().f10_field_labels(
        entry, request_body, None, "international", _MODULES[__name__]
    )


def describe_fields(
    entry: str,
    request_body: Any,
    fields: Sequence[str],
) -> tuple[dict[str, str], list[str]]:
    """把响应字段拆成「已知中文」与「未映射（保留原名）」两部分。"""

    from eltdx._native_abi import load_native

    return load_native().f10_describe_fields(
        entry, request_body, fields, None, "international", _MODULES[__name__]
    )


def is_mg_hk_entry(entry: str) -> bool:
    """是否美股/港股/新三板 F10 Entry（走 page2 网关）。"""

    from eltdx._native_abi import load_native

    return load_native().f10_international_entry(entry)


__all__ = [
    "FIELD_LABELS",
    "describe_fields",
    "is_mg_hk_entry",
    "labels_for",
]
