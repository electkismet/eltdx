"""离线数据读写子包 —— 从本地通达信安装目录读取/写入数据文件。

只依赖标准库与 ``eltdx.exceptions``，不涉及任何网络请求。公共入口：

* 日线：:func:`read_daily_bar` / :func:`write_daily_bar`
* 分钟线：:func:`read_min_bar` / :func:`write_min_bar`
* 扩展市场日线：:func:`read_ex_daily_bar` / :func:`write_ex_daily_bar`
* 板块：:func:`read_block`
* 股本变迁：:func:`read_gbbq`
* 历史财务：:func:`read_history_financial`
"""

from .alignment import ensure_aligned_for_append
from .block import read_block, read_customer_block
from .daily_bar import (
    DailyBar,
    detect_security_type,
    encode_daily_bar,
    find_daily_bar_file,
    read_daily_bar,
    security_coefficients,
)
from .ex_daily_bar import ExDailyBar, encode_ex_daily_bar, read_ex_daily_bar
from .gbbq import GbbqRecord, read_gbbq
from .history_financial import (
    FinancialRecord,
    parse_financial_dat,
    parse_financial_file_list,
    read_history_financial,
)
from .industry import parse_tdxhy_cfg, read_industry_cfg
from .min_bar import (
    MinBar,
    encode_5min_bar,
    encode_min_bar,
    find_5min_bar_file,
    find_lc1_bar_file,
    find_lc5_bar_file,
    read_5min_bar,
    read_min_bar,
)
from .models import (
    BlockInfo,
    CustomerBlock,
    FinancialFileInfo,
)
from .paths import detect_tdx_home, market_to_exchange, resolve_vipdoc
from .write_daily import append_daily_bar, get_last_bar_date, write_daily_bar
from .write_ex_daily import (
    append_ex_daily_bar,
    get_last_ex_bar_date,
    write_ex_daily_bar,
)
from .write_min_bar import (
    append_5min_bar,
    append_min_bar,
    get_last_5min_bar_datetime,
    get_last_min_bar_datetime,
    write_5min_bar,
    write_min_bar,
)

__all__ = [
    # 路径
    "detect_tdx_home",
    "resolve_vipdoc",
    "market_to_exchange",
    # 模型
    "DailyBar",
    "MinBar",
    "ExDailyBar",
    "BlockInfo",
    "CustomerBlock",
    "FinancialFileInfo",
    "FinancialRecord",
    "GbbqRecord",
    # 日线
    "read_daily_bar",
    "write_daily_bar",
    "append_daily_bar",
    "get_last_bar_date",
    "encode_daily_bar",
    "find_daily_bar_file",
    "detect_security_type",
    "security_coefficients",
    # 分钟线
    "read_min_bar",
    "write_min_bar",
    "append_min_bar",
    "read_5min_bar",
    "write_5min_bar",
    "append_5min_bar",
    "encode_min_bar",
    "encode_5min_bar",
    "get_last_min_bar_datetime",
    "get_last_5min_bar_datetime",
    "find_lc1_bar_file",
    "find_lc5_bar_file",
    "find_5min_bar_file",
    # 扩展市场日线
    "read_ex_daily_bar",
    "write_ex_daily_bar",
    "append_ex_daily_bar",
    "get_last_ex_bar_date",
    "encode_ex_daily_bar",
    # 板块
    "read_block",
    "read_customer_block",
    # 行业
    "read_industry_cfg",
    "parse_tdxhy_cfg",
    # 股本变迁
    "read_gbbq",
    # 历史财务
    "read_history_financial",
    "parse_financial_dat",
    "parse_financial_file_list",
    # 写入对齐
    "ensure_aligned_for_append",
]
