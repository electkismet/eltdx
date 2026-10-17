"""港股逐笔成交的协议路由辅助（同步）。"""

from __future__ import annotations

from collections.abc import Callable
from datetime import date, time
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

    return market in HK_STOCK_MARKETS


def _to_ymd(query_date: date) -> int:
    """date → YYYYMMDD int。"""

    return query_date.year * 10000 + query_date.month * 100 + query_date.day


def _build_cmd(
    market: int,
    code: str,
    ymd: int | None,
    offset: int,
    page_size: int,
) -> BaseCommand[list[ExTransactionRecord]]:
    """根据是否有日期构建对应的 ex 协议命令。"""

    if ymd is None:
        return GetExTransactionDataCmd(market, code, offset, page_size)
    return GetExHistoryTransactionDataCmd(market, code, ymd, offset, page_size)


def _map_record(rec: ExTransactionRecord) -> MacTransaction:
    """把 ExTransactionRecord 映射为与 A 股一致的 MacTransaction（price ÷1000 港元）。"""

    return MacTransaction(
        time=time(rec.hour, rec.minute, rec.second),
        price=rec.price / _HK_PRICE_DIVISOR,
        vol=rec.volume,
        trade_count=0,
        bs_flag=rec.nature,
    )


def _fetch_hk_transactions_sync(
    execute_fn: SyncExecute[list[ExTransactionRecord]],
    market: int,
    code: str,
    query_date: date | None,
    start: int,
    count: int,
) -> list[MacTransaction]:
    """同步获取港股逐笔成交（自动分页）。"""

    ymd = _to_ymd(query_date) if query_date is not None else None
    results: list[MacTransaction] = []
    fetched = 0
    offset = start
    while fetched < count:
        page_size = min(count - fetched, _HK_TRANSACTION_PAGE_SIZE)
        batch = execute_fn(_build_cmd(market, code, ymd, offset, page_size))
        if not batch:
            break
        results.extend(_map_record(r) for r in batch)
        fetched += len(batch)
        offset += len(batch)
        if len(batch) < page_size:
            break
    return results


def _fetch_all_hk_transactions_sync(
    execute_fn: SyncExecute[list[ExTransactionRecord]],
    market: int,
    code: str,
    query_date: date | None,
    start: int = 0,
) -> list[MacTransaction]:
    """同步获取港股某日全部逐笔成交（翻页至末页）。"""

    ymd = _to_ymd(query_date) if query_date is not None else None
    results: list[MacTransaction] = []
    offset = start
    for _ in range(_HK_TRANSACTION_MAX_PAGES):
        batch = execute_fn(_build_cmd(market, code, ymd, offset, _HK_TRANSACTION_PAGE_SIZE))
        if not batch:
            break
        results.extend(_map_record(r) for r in batch)
        offset += len(batch)
        if len(batch) < _HK_TRANSACTION_PAGE_SIZE:
            break
    return results


__all__ = [
    "HK_STOCK_MARKETS",
    "SyncExecute",
    "is_hk_stock_market",
    "_fetch_hk_transactions_sync",
    "_fetch_all_hk_transactions_sync",
]
