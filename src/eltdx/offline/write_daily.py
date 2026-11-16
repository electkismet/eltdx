"""离线日线写入 —— 将 :class:`~eltdx.offline.models.DailyBar` 追加到 ``.day`` 文件。"""

from __future__ import annotations

import os
import sys
from pathlib import Path

from .alignment import ensure_aligned_for_append
from .daily_bar import _DAILY_FMT, encode_daily_bar
from .models import DailyBar

__all__ = [
    "write_daily_bar",
    "append_daily_bar",
    "get_last_bar_date",
]


def get_last_bar_date(filepath: str | Path) -> int | None:
    """读取 ``.day`` 文件最后一条完整记录的日期（纯读，无副作用）。

    Returns:
        YYYYMMDD 整数；文件为空 / 过短 / 尾部只有不完整记录时返回 ``None``。
    """

    from eltdx._native_abi import load_native

    native = load_native()
    plan = native.offline_last_record_plan(filepath, _DAILY_FMT, sys.modules[__name__], False)
    if plan is None:
        return None
    filepath, offset, size = plan
    with filepath.open("rb") as handle:
        handle.seek(offset)
        last_record = handle.read(size)
    return native.offline_last_record_value(last_record, _DAILY_FMT, sys.modules[__name__], False)


def append_daily_bar(
    filepath: str | Path,
    bars: list[DailyBar],
    price_coeff: float,
    vol_coeff: float,
) -> int:
    """将 ``bars`` 追加写入 ``.day`` 文件，自动跳过不晚于末尾日期的新记录。

    Args:
        filepath: ``.day`` 文件路径。
        bars: 待写入的 K 线列表（按时间升序）。
        price_coeff: 价格系数。
        vol_coeff: 成交量系数。

    Returns:
        实际写入的记录数。

    Raises:
        ResourceFormatError: 文件末尾存在残尾记录（长度不是 32 的整数倍）。
            此时**拒绝写入**：直接追加会让此后每条记录错位，且不会有任何报错。
            请先把文件截断到最后一条完整记录的结尾再写。
    """

    filepath = Path(filepath)
    # 校验放在读取末条日期**之前**：残尾文件上 ``get_last_bar_date`` 会按对齐后的
    # 边界读出一条伪末条，其日期可能恰好不小于所有待写 bar，导致 ``new_bars`` 为空
    # 而直接 return 0 —— 那样残尾就被静默留下了，下次写入才爆出来。
    from eltdx._native_abi import load_native

    native = load_native()
    filepath, new_bars = native.offline_prepare_append(
        filepath, bars, lambda path: get_last_bar_date(path), _DAILY_FMT.size,
        "日线", sys.modules[__name__], False
    )
    if not new_bars:
        return 0

    encoded = native.offline_encode_batch(
        new_bars, lambda bar: encode_daily_bar(bar, price_coeff, vol_coeff), None
    )
    with filepath.open("ab") as handle:
        handle.write(encoded)
        handle.flush()
        os.fsync(handle.fileno())
    return len(new_bars)


def write_daily_bar(
    filepath: str | Path,
    bars: list[DailyBar],
    price_coeff: float = 0.01,
    vol_coeff: float = 0.01,
) -> int:
    """将 ``bars`` 同步追加到本地 ``.day`` 文件（公共入口）。

    Args:
        filepath: ``.day`` 文件路径。
        bars: 待写入的 K 线列表（按时间升序）。
        price_coeff: 价格系数（默认 A 股 0.01）。
        vol_coeff: 成交量系数（默认 A 股 0.01）。

    Returns:
        实际写入的记录数。
    """

    return append_daily_bar(filepath, bars, price_coeff, vol_coeff)
