"""通达信 7615「资讯」频道枚举与辅助函数（纯静态，不联网）。

资讯页（``/site/tdx-pc-content/page-cjzx.html``、``/site/zx/zx_xw.htm``）通过
两个 TQLEX Entry 拉取市场资讯：

* ``CWServ.tdxi_zxxwsy`` —— 「资讯首页」各要闻流（滚动资讯 / 今日焦点 / 要闻速递 …），
  请求 ``Params = [query, page, page_size]``，``query`` 取值见 :class:`NewsHomeTab`。
* ``CWServ.tdxi_zxxwnews`` —— 各「频道」栏目新闻，
  请求 ``Params = [query1(常为 "1"), 频道号, page, page_size, keyword]``，
  频道号取值见 :class:`NewsChannel`。

两个接口的新闻行都在响应的 **第 2 个结果集** ``result_sets[1].rows``，字段约定：

    Title        标题
    Issue_date   发布时间（字符串，如 "2026-10-27 16:37:33"）
    Summary      摘要
    rec_id       记录 ID（可用于取详情，见 ``F10Client.detail`` / ``tdxi_tdxmhcontent``）
    Info_Src     来源（财联社 / 证券时报 / 金吾财讯 …）
    Url          原文链接
    AttachFile   附件
    bProc        是否研报类标记

频道号来源：通达信资讯页脚本 ``site/zx/js/zx_xw.js`` 中的 ``firstNav``（频道组）
与 ``secondNav``（各子栏目 ``option.query2``），2026-10 服务端页面样本与脚本交叉核对后枚举，
全部 84 个频道中仅「互联网金融(1011)」会偶发 0 条（接口返回 err=0 但无数据）。
"""

from __future__ import annotations

from enum import IntEnum
from typing import Any


class NewsHomeTab(IntEnum):
    """「资讯首页」顶部要闻流（``tdxi_zxxwsy`` 的 ``query``）。"""

    ROLLING = 1  # 滚动资讯
    TODAY_FOCUS = 2  # 今日焦点
    HEADLINES = 3  # 要闻速递
    POSITIVE = 4  # 正面消息
    NEGATIVE = 5  # 负面消息
    ESSENCE = 6  # 精华集锦
    LATEST_TIPS = 7  # 最新提示
    INSTITUTION_RESEARCH = 9  # 机构调研
    # 注：query=8 为「财经数据」，通达信前端已注释停用。


class NewsChannel(IntEnum):
    """各「频道」栏目新闻（``tdxi_zxxwnews`` 的频道号 ``query2``）。

    按频道组（``firstNav``）分组，注释即前端栏目名。
    """

    # —— 财经频道 (firstNav key=2) ——
    FINANCE_LATEST = 200  # 最新
    FINANCE_HEADLINES = 201  # 财经要闻
    FINANCE_DOMESTIC = 202  # 国内财经
    FINANCE_INTERNATIONAL = 203  # 国际财经
    FINANCE_MARKET = 204  # 证券市场
    FINANCE_MONEY = 205  # 金融货币
    FINANCE_MACRO = 206  # 宏观经济
    FINANCE_INDUSTRY_DYNAMIC = 207  # 产经动态
    FINANCE_DIGITAL = 211  # 数字财经
    FINANCE_PEOPLE = 212  # 财经人物
    FINANCE_POLITICS = 213  # 时政要闻

    # —— 股票频道 (firstNav key=3) ——
    STOCK_LATEST = 300  # 最新
    STOCK_HEADLINES = 301  # 股票要闻
    STOCK_MARKET_ANALYSIS = 302  # 大盘分析
    STOCK_SECTOR = 303  # 板块聚焦
    STOCK_COMPANY_NEWS = 306  # 公司新闻
    STOCK_STAR_MARKET = 318  # 科创板
    STOCK_NEEQ = 304  # 三板动态
    STOCK_OVERSEAS = 309  # 海外股市
    STOCK_OVERSEAS_COMPANY = 310  # 海外公司
    STOCK_COMPANY_ANNOUNCEMENT = 307  # 公司公告
    STOCK_B_SHARE = 305  # B 股资讯

    # —— 新股频道 (firstNav key=5) ——
    IPO_LATEST = 500  # 最新
    IPO_NEWS = 501  # 新股要闻
    IPO_ANNOUNCEMENT = 502  # 新股公告
    IPO_COMMENT = 507  # 新股评论
    IPO_SH_SZ = 503  # 沪深新股
    IPO_HK = 504  # 港股新股
    IPO_US = 505  # 美股新股

    # —— 港股频道 (firstNav key=6) ——
    HK_LATEST = 600  # 最新
    HK_NEWS = 601  # 港股要闻
    HK_MARKET = 602  # 港股市场
    HK_COMPANY = 603  # 港股公司
    HK_RATING = 605  # 投行评级
    HK_WARRANT = 606  # 涡轮资讯
    HK_ANNOUNCEMENT = 604  # 港股公告
    HK_ECONOMY = 607  # 香港经济

    # —— 美股频道 (firstNav key=7) ——
    US_LATEST = 700  # 最新
    US_NEWS = 701  # 美股要闻
    US_MARKET = 702  # 美股市场
    US_STOCK_NEWS = 703  # 美股新闻
    US_CHINA_CONCEPT = 704  # 中概股
    US_COMMENT = 705  # 美股评论
    US_OVERSEAS_ECONOMY = 706  # 海外经济

    # —— 基金频道 (firstNav key=8) ——
    FUND_LATEST = 800  # 最新
    FUND_NEWS = 801  # 基金要闻
    FUND_INSTITUTION_HOLDING = 808  # 机构持仓
    FUND_PUBLIC = 802  # 公募基金
    FUND_PRIVATE = 803  # 私募基金
    FUND_SOCIAL_SECURITY = 804  # 社保基金
    FUND_TRUST = 805  # 信托基金
    FUND_ANNOUNCEMENT = 809  # 基金公告

    # —— 期货频道 (firstNav key=9) ——
    FUTURES_LATEST = 900  # 最新
    FUTURES_NEWS = 901  # 期货要闻
    FUTURES_MARKET_COMMENT = 902  # 市场评述
    FUTURES_FINANCIAL = 903  # 金融期货
    FUTURES_ENERGY_CHEM = 904  # 能源化工
    FUTURES_METAL = 905  # 金属期货
    FUTURES_AGRICULTURE = 906  # 农产品
    FUTURES_DELIVERY = 907  # 持仓交割
    FUTURES_SPOT = 908  # 现货报价
    FUTURES_SUPPLY_DEMAND = 910  # 生产供求
    FUTURES_INVENTORY = 909  # 库存数据

    # —— 理财频道 (firstNav key=10) ——
    WEALTH_LATEST = 1000  # 最新
    WEALTH_NEWS = 1001  # 理财要闻
    WEALTH_BANK = 1002  # 银行理财
    WEALTH_COMPREHENSIVE = 1008  # 综合理财
    WEALTH_PRECIOUS_METAL = 1006  # 贵金属
    WEALTH_INTERNET_FINANCE = 1011  # 互联网金融（偶发无数据）

    # —— 外汇频道 (firstNav key=11) ——
    FOREX_LATEST = 1100  # 最新
    FOREX_NEWS = 1101  # 外汇要闻
    FOREX_MARKET_COMMENT = 1111  # 汇市评论
    FOREX_CURRENCY_COLUMN = 1102  # 货币专栏
    FOREX_CENTRAL_BANK = 1103  # 央行专栏

    # —— 债券频道 (firstNav key=12) ——
    BOND_LATEST = 1200  # 最新
    BOND_NEWS = 1201  # 债券要闻
    BOND_DOMESTIC = 1202  # 国内债市
    BOND_INTERNATIONAL = 1203  # 国际债市
    BOND_NEWS_BRIEF = 1204  # 债券新闻
    BOND_ANNOUNCEMENT = 1205  # 债券公告

    # —— 期权频道 (firstNav key=18) ——
    OPTION_LATEST = 1801  # 最新
    OPTION_NEWS = 1802  # 期权要闻
    OPTION_INDEX = 1803  # 指数期权
    OPTION_COMMODITY = 1804  # 商品期权


HOME_TAB_LABELS: dict[int, str] = {
    1: "滚动资讯",
    2: "今日焦点",
    3: "要闻速递",
    4: "正面消息",
    5: "负面消息",
    6: "精华集锦",
    7: "最新提示",
    9: "机构调研",
}

# 频道组 -> 有序频道号列表（与通达信前端 firstNav 一致，便于文档/遍历）。
NEWS_CHANNEL_GROUPS: dict[str, list[int]] = {
    "财经": [200, 201, 202, 203, 204, 205, 206, 207, 211, 212, 213],
    "股票": [300, 301, 302, 303, 306, 318, 304, 309, 310, 307, 305],
    "新股": [500, 501, 502, 507, 503, 504, 505],
    "港股": [600, 601, 602, 603, 605, 606, 604, 607],
    "美股": [700, 701, 702, 703, 704, 705, 706],
    "基金": [800, 801, 808, 802, 803, 804, 805, 809],
    "期货": [900, 901, 902, 903, 904, 905, 906, 907, 908, 910, 909],
    "理财": [1000, 1001, 1002, 1008, 1006, 1011],
    "外汇": [1100, 1101, 1111, 1102, 1103],
    "债券": [1200, 1201, 1202, 1203, 1204, 1205],
    "期权": [1801, 1802, 1803, 1804],
}

# 频道号 -> 前端栏目中文名（由 NewsChannel 成员注释汇总）。
NEWS_CHANNEL_LABELS: dict[int, str] = {
    200: "最新", 201: "财经要闻", 202: "国内财经", 203: "国际财经", 204: "证券市场",
    205: "金融货币", 206: "宏观经济", 207: "产经动态", 211: "数字财经", 212: "财经人物", 213: "时政要闻",
    300: "最新", 301: "股票要闻", 302: "大盘分析", 303: "板块聚焦", 306: "公司新闻", 318: "科创板",
    304: "三板动态", 309: "海外股市", 310: "海外公司", 307: "公司公告", 305: "B股资讯",
    500: "最新", 501: "新股要闻", 502: "新股公告", 507: "新股评论", 503: "沪深新股", 504: "港股新股", 505: "美股新股",
    600: "最新", 601: "港股要闻", 602: "港股市场", 603: "港股公司", 605: "投行评级", 606: "涡轮资讯", 604: "港股公告", 607: "香港经济",
    700: "最新", 701: "美股要闻", 702: "美股市场", 703: "美股新闻", 704: "中概股", 705: "美股评论", 706: "海外经济",
    800: "最新", 801: "基金要闻", 808: "机构持仓", 802: "公募基金", 803: "私募基金", 804: "社保基金", 805: "信托基金", 809: "基金公告",
    900: "最新", 901: "期货要闻", 902: "市场评述", 903: "金融期货", 904: "能源化工", 905: "金属期货", 906: "农产品",
    907: "持仓交割", 908: "现货报价", 910: "生产供求", 909: "库存数据",
    1000: "最新", 1001: "理财要闻", 1002: "银行理财", 1008: "综合理财", 1006: "贵金属", 1011: "互联网金融",
    1100: "最新", 1101: "外汇要闻", 1111: "汇市评论", 1102: "货币专栏", 1103: "央行专栏",
    1200: "最新", 1201: "债券要闻", 1202: "国内债市", 1203: "国际债市", 1204: "债券新闻", 1205: "债券公告",
    1801: "最新", 1802: "期权要闻", 1803: "指数期权", 1804: "商品期权",
}


def home_tab_name(tab: "NewsHomeTab | int") -> str:
    """返回要闻流（query）对应的中文名。"""
    return HOME_TAB_LABELS.get(int(tab), str(int(tab)))


def channel_name(channel: "NewsChannel | int") -> str:
    """返回频道号对应的中文栏目名。"""
    return NEWS_CHANNEL_LABELS.get(int(channel), str(int(channel)))


def extract_news_rows(response: Any) -> list[dict[str, Any]]:
    """从 ``tdxi_zxxwsy`` / ``tdxi_zxxwnews`` 的 :class:`F10Response` 提取新闻行。

    数据在第 2 个结果集（``result_sets[1].rows``）；缺省返回空列表。
    """
    try:
        sets = response.result_sets
    except AttributeError:
        return []
    if len(sets) < 2:
        return []
    return [dict(row) for row in sets[1].rows]
