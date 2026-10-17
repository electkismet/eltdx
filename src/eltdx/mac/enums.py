"""MAC 协议枚举常量（周期、复权、板块、市场分类、排序、过滤、扩展市场）。"""

from __future__ import annotations

from enum import IntEnum


class Period(IntEnum):
    """K 线周期。"""

    MIN_5 = 0
    MIN_15 = 1
    MIN_30 = 2
    MIN_60 = 3
    DAILY = 4
    WEEKLY = 5
    MONTHLY = 6
    MIN_1 = 7
    MINS = 8  # 多分钟（配合 times 参数）
    DAYS = 9  # 多日（配合 times 参数）
    QUARTERLY = 10
    YEARLY = 11
    SECONDS = 13  # 多秒（配合 times 参数）


class Adjust(IntEnum):
    """复权类型。"""

    NONE = 0
    QFQ = 1  # 前复权
    HFQ = 2  # 后复权


class BoardType(IntEnum):
    """板块类型。"""

    HY = 0  # 行业一级
    HY2 = 1  # 行业二级
    GN = 3  # 概念
    FG = 4  # 风格
    DQ = 5  # 地区
    OTHER = 6  # 其他
    YJ_LEVEL1 = 7  # 业绩一级
    YJ_LEVEL2 = 8  # 业绩二级
    YJ_LEVEL3 = 9  # 业绩三级
    ALL = 255  # 全部


class ExBoardType(IntEnum):
    """扩展市场板块类型。"""

    HK_ALL = 0  # 港股板块
    HK_GN = 1  # 港股概念
    HK_HY = 2  # 港股行业
    US_ALL = 3  # 美股板块
    US_GN = 4  # 美股概念
    US_HY = 5  # 美股行业


class Category(IntEnum):
    """市场分类。"""

    SH = 0  # 上证A
    SZ = 2  # 深证A
    A = 6  # 全部A股
    B = 7  # B股
    KCB = 8  # 科创板
    BJ = 12  # 北证A
    CYB = 14  # 创业板

    BOARD_ALL = 10000  # 全部板块
    BOARD_HY = 10001  # 行业一级
    BOARD_HY2 = 10002  # 行业二级
    BOARD_GN = 10004  # 概念
    BOARD_FG = 10005  # 风格
    BOARD_DQ = 10006  # 地区
    BOARD_OTHER = 10007  # 其他
    BOARD_YJ_LEVEL1 = 10008  # 业绩一级
    BOARD_YJ_LEVEL2 = 10009  # 业绩二级
    BOARD_YJ_LEVEL3 = 10010  # 业绩三级

    HGT = 0x2AF9  # 沪股通
    SGT = 0x2B01  # 深股通
    FXJS = 0x2AFF  # 风险警示
    ETF = 0x2AFD  # ETF基金
    LOF = 0x2B04  # LOF基金
    ZS = 0x2B2C  # 沪深系列指数


class SortType(IntEnum):
    """排序字段。"""

    CODE = 0x00
    NAME = 0x01
    PRE_CLOSE = 0x02
    OPEN = 0x03
    HIGH = 0x04
    LOW = 0x05
    PRICE = 0x06
    BID = 0x07
    ASK = 0x08
    VOLUME = 0x09
    TOTAL_AMOUNT = 0x0A
    LAST_VOLUME = 0x0B
    CHANGE = 0x0C
    CHANGE_PCT = 0x0E  # 涨幅%
    AMPLITUDE_PCT = 0x0F  # 振幅%
    AVG = 0x10  # 均价
    PE_DYNAMIC = 0x11  # 市盈(动)
    ENTRUST_RATIO = 0x12  # 委比%
    INSIDE_VOLUME = 0x13  # 内盘
    OUTSIDE_VOLUME = 0x14  # 外盘
    IN_OUT_RATIO = 0x15  # 内外比
    BID_VOLUME = 0x17  # 买量
    ASK_VOLUME = 0x18  # 卖量
    LOCKED_RATIO = 0x1B  # 封成比
    LOCKED_AMOUNT = 0x1C  # 封单额
    OPEN_AMOUNT = 0x1D  # 开盘金额
    OPEN_TURNOVER_PCT = 0x1E  # 开盘换手%
    VOL_RATIO = 0x23  # 量比
    TURNOVER_RATE = 0x24  # 换手%
    FLOAT_SHARES = 0x25  # 流通股(亿)
    FLOAT_MARKET_CAP = 0x26  # 流通市值
    TOTAL_MARKET_CAP_AB = 0x27  # AB股总市值
    UNMATCHED_VOLUME = 0x2A  # 未匹配量
    STRENGTH_PCT = 0x2D  # 强弱度%
    SPEED_PCT = 0x2E  # 涨速%
    ACTIVITY = 0x2F  # 活跃度
    SHORT_TURNOVER_PCT = 0xCC  # 短换手%
    VOL_SPEED_PCT = 0xD0  # 量涨速%
    MAIN_NET_AMOUNT = 0xD4  # 主力净额
    MAIN_NET_RATIO = 0xD7  # 主力净比%
    AUCTION_LIMIT_BUY = 0x102  # 竞价涨停买
    AMOUNT_2M = 0x10C  # 2分钟金额
    OPEN_SNATCH_PCT = 0x10A  # 开盘抢筹%
    OPEN_PCT = 0x119  # 开盘%
    HIGH_PCT = 0x11A  # 最高%
    LOW_PCT = 0x11B  # 最低%
    AVG_CHANGE_PCT = 0x11C  # 均涨幅%
    DRAWDOWN_PCT = 0x11E  # 回撤%
    ATTACK_PCT = 0x11F  # 攻击%


class SortOrder(IntEnum):
    """排序方向。"""

    NONE = 0
    DESC = 1
    ASC = 2


class FilterType(IntEnum):
    """过滤标志（位掩码，可组合）。"""

    NEW = 1  # 次新股
    KC = 2  # 科创板
    ST = 4  # ST/*ST
    CY = 8  # 创业板
    HK_CONNECT = 16  # 互联互通标的(仅核准制)
    BJ = 32  # 北交所
    APPROVAL = 64  # 核准制股票
    REGISTRATION = 128  # 注册制股票


class ExMarket(IntEnum):
    """扩展市场代码。"""

    TEMP_STOCK = 1  # 临时股
    ZZ_FUTURES_OPTION = 4  # 郑州商品期权
    DL_FUTURES_OPTION = 5  # 大连商品期权
    SH_FUTURES_OPTION = 6  # 上海商品期权
    CFFEX_OPTION = 7  # 中金所期权
    SH_STOCK_OPTION = 8  # 上海股票期权
    SZ_STOCK_OPTION = 9  # 深圳股票期权
    BASIC_FX = 10  # 基本汇率
    CROSS_FX = 11  # 交叉汇率
    INTL_INDEX = 12  # 国际指数
    COMEX_FUTURES = 16  # 纽约COMEX
    NYMEX_FUTURES = 17  # 纽约NYMEX
    CBOT_FUTURES = 18  # 芝加哥CBOT
    HK_FINANCIAL_FUTURES = 23  # 香港金融期货
    HK_FINANCIAL_OPTIONS = 24  # 香港金融期权
    HK_STOCK_FUTURES = 25  # 香港股票期货
    HK_STOCK_OPTIONS = 26  # 香港股票期权
    HK_INDEX = 27  # 香港指数
    ZZ_FUTURES = 28  # 郑州商品
    DL_FUTURES = 29  # 大连商品
    SH_FUTURES = 30  # 上海期货
    HK_MAIN_BOARD = 31  # 香港主板
    OPEN_END_FUND = 33  # 开放式基金
    MONETARY_FUND = 34  # 货币型基金
    MACRO_INDICATOR = 38  # 宏观指标
    FUTURES_INDEX = 42  # 商品指数
    B_TO_H = 43  # B股转H股
    NEEQ = 44  # 股转系统
    SH_GOLD = 46  # 上海黄金
    CFFEX_FUTURES = 47  # 中金所期货
    HK_GEM = 48  # 香港创业板
    HK_FUND = 49  # 香港基金
    TREASURY_VALUATION = 54  # 国债预发行
    SUNSHINE_PRIVATE_FUND = 56  # 阳光私募基金
    BROKER_COLLECTIVE_FINANCE = 57  # 券商集合理财
    BROKER_MONETARY_FINANCE = 58  # 券商货币理财
    MAIN_FUTURES_CONTRACT = 60  # 主力期货合约
    CSI_INDEX = 62  # 中证指数
    GZ_ARBITRAGE_FUTURES = 65  # 广州套利期货
    GZ_FUTURES = 66  # 广州期货
    GZ_OPTIONS = 67  # 广州期权
    RISK_CONTROL_INDEX = 68  # 风控指数
    HUAZHENG_INDEX = 69  # 华证指数
    EXTENDED_SECTOR_INDEX = 70  # 扩展板块指数
    HK_STOCK_GGT = 71  # 港股-港股通
    GE_STOCK = 73  # 德国股票
    US_STOCK = 74  # 美国股票
    SG_STOCK = 78  # 新加坡股票
    MONEY_MARKET = 91  # 资金市场
    FUND_VALUATION = 93  # 基金估值
    HK_DARK_POOL = 98  # 港股暗盘
    CODE_MIRROR = 100  # 代码镜像
    SZSE_INDEX = 102  # 国证指数


__all__ = [
    "Adjust",
    "BoardType",
    "Category",
    "ExBoardType",
    "ExMarket",
    "FilterType",
    "Period",
    "SortOrder",
    "SortType",
]
