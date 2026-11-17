"""A 股价格限制兼容规则（共用 native 算术，保留原 Python 行为）。

从 easy_tdx ``codec/price_rules.py`` 移植。``market`` 采用与 easy_tdx
``Market`` 枚举一致的数字约定：``0=深圳``、``1=上海``、``2=北京``。
移植时去掉了对 ``FinanceInfo`` 的依赖（原实现未使用），保留 ``listed_days``
上市交易日数输入。
"""

from __future__ import annotations

from sys import modules as _MODULES

_MARKET_SZ = 0
_MARKET_SH = 1

# 创业板代码段（注册制，涨跌幅 20%）。含 2024 年启用的 302 段。
_CHINEXT_CODES = frozenset({"300", "301", "302"})


def get_no_limit_window_days(market: int, code: str, name: str) -> int:
    """返回上市初期不设涨跌幅限制的交易日窗口。

    返回值:

    * ``0``: 默认按常规涨跌幅限制处理
    * ``1``: 北交所上市首日不设涨跌幅限制
    * ``5``: 沪深主板/创业板/科创板上市前 5 个交易日不设涨跌幅限制
    """

    from eltdx._native_abi import load_native
    return load_native().mac_no_limit_window(market, code, name, _MODULES[__name__])


def _is_index_like(market: int, code: str, name: str) -> bool:
    """判断是否为指数/板块类代码。"""

    from eltdx._native_abi import load_native
    return load_native().mac_index_like(market, code, name, _MODULES[__name__])


def compute_price_limits(
    market: int,
    code: str,
    name: str,
    pre_close: float,
    listed_days: int | None = None,
) -> tuple[float | None, float | None]:
    """根据板块规则计算涨跌停价。

    Returns:
        ``(limit_up, limit_down)``；无涨跌幅限制或规则无法可靠判断时返回
        ``(None, None)``。

    Args:
        listed_days: 已上市交易天数（按交易日计，首日=1）。若提供该值，
            函数会按上市初期无涨跌幅限制规则优先返回 ``(None, None)``。
    """

    from eltdx._native_abi import load_native
    return load_native().mac_price_limits(market, code, name, pre_close, listed_days, _MODULES[__name__])


__all__ = ["compute_price_limits", "get_no_limit_window_days"]
