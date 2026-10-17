"""扩展行情市场枚举（ExMarket）。"""

from __future__ import annotations

from enum import IntEnum


class ExMarket(IntEnum):
    """扩展市场代码（端口 7727 的 EX 协议使用）。"""

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


__all__ = ["ExMarket"]
