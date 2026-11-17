"""港股逐笔成交的协议路由辅助（同步）。"""

from __future__ import annotations

from collections.abc import Callable
from datetime import date, time
from sys import modules as _MODULES
from typing import TypeVar

from eltdx.ex.commands.base import BaseCommand
from eltdx.ex.commands.get_transaction import (
    GetExHistoryTransactionDataCmd,
    GetExTransactionDataCmd,
)
from eltdx.ex.mac_models import MacTransaction
from eltdx.ex.models import ExTransactionRecord

# 港股股票类市场（走 ex 协议 0x23FC / 0x2406）。
# 不含衍生品（HK_FINANCIAL_FUTURES=23 / HK_FINANCIAL_OPTIONS=24 / HK_STOCK_FUTURES=25 /
# HK_STOCK_OPTIONS=26）：期货 / 期权逐笔语义不同，且 0x122F 对 CFFEX 期货恰好可用。
HK_STOCK_MARKETS: frozenset[int] = frozenset(
    {
        27,  # HK_INDEX 香港指数
        31,  # HK_MAIN_BOARD 香港主板
        48,  # HK_GEM 香港创业板
        49,  # HK_FUND 香港基金
        71,  # HK_STOCK_GGT 港股-港股通
        98,  # HK_DARK_POOL 港股暗盘
    }
)

# ex 协议单页最大返回条数（与 GetExTransactionDataCmd 默认 count 一致）。
_HK_TRANSACTION_PAGE_SIZE = 1800
# 全量翻页安全上限：50 页 × 1800 = 90000 条。
_HK_TRANSACTION_MAX_PAGES = 50
# 港股价格整数单位：1 港元 = 1000。
_HK_PRICE_DIVISOR = 1000.0

_T = TypeVar("_T")

# 同步执行回调：传入 BaseCommand，返回其 parse_response 结果
SyncExecute = Callable[[BaseCommand[_T]], _T]


def is_hk_stock_market(market: int) -> bool:
    """判断给定市场代码是否属于港股股票类（需走 ex 协议取逐笔成交）。"""

    from eltdx._native_abi import load_native

    return load_native().hk_is_stock(market, _MODULES[__name__])


def _to_ymd(query_date: date) -> int:
    """date → YYYYMMDD int。"""

    from eltdx._native_abi import load_native

    return load_native().hk_to_ymd(query_date)


def _build_cmd(
    market: int,
    code: str,
    ymd: int | None,
    offset: int,
    page_size: int,
) -> BaseCommand[list[ExTransactionRecord]]:
    """根据是否有日期构建对应的 ex 协议命令。"""

    from eltdx._native_abi import load_native

    return load_native().hk_build_command((market, code, ymd, offset, page_size), _MODULES[__name__])


def _map_record(rec: ExTransactionRecord) -> MacTransaction:
    """把 ExTransactionRecord 映射为与 A 股一致的 MacTransaction（price ÷1000 港元）。"""

    from eltdx._native_abi import load_native

    return load_native().hk_map_record(rec, _MODULES[__name__])


def _fetch_hk_transactions_sync(
    execute_fn: SyncExecute[list[ExTransactionRecord]],
    market: int,
    code: str,
    query_date: date | None,
    start: int,
    count: int,
) -> list[MacTransaction]:
    """同步获取港股逐笔成交（自动分页）。"""

    from eltdx._native_abi import load_native

    return load_native().hk_transaction_pages(
        execute_fn, (market, code, query_date, start, count), False,
        lambda values: (value for value in values), _MODULES[__name__],
    )


def _fetch_all_hk_transactions_sync(
    execute_fn: SyncExecute[list[ExTransactionRecord]],
    market: int,
    code: str,
    query_date: date | None,
    start: int = 0,
) -> list[MacTransaction]:
    """同步获取港股某日全部逐笔成交（翻页至末页）。"""

    from eltdx._native_abi import load_native

    return load_native().hk_transaction_pages(
        execute_fn, (market, code, query_date, start), True,
        lambda values: (value for value in values), _MODULES[__name__],
    )


__all__ = [
    "HK_STOCK_MARKETS",
    "SyncExecute",
    "is_hk_stock_market",
    "_fetch_hk_transactions_sync",
    "_fetch_all_hk_transactions_sync",
]
