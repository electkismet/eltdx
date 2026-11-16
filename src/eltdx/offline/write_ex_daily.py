"""离线扩展市场日线写入 —— 追加 :class:`~eltdx.offline.models.ExDailyBar`。"""

from __future__ import annotations

from pathlib import Path
import sys

from .alignment import ensure_aligned_for_append
from .ex_daily_bar import _EX_DAILY_FMT, encode_ex_daily_bar
from .models import ExDailyBar

__all__ = [
    "write_ex_daily_bar",
    "append_ex_daily_bar",
    "get_last_ex_bar_date",
]


def get_last_ex_bar_date(filepath: str | Path) -> int | None:
    """读取扩展市场 ``.day`` 文件最后一条完整记录的日期。"""

    from eltdx._native_abi import load_native

    native = load_native()
    plan = native.offline_last_record_plan(filepath, _EX_DAILY_FMT, sys.modules[__name__], False)
    if plan is None:
        return None
    filepath, offset, size = plan
    with filepath.open("rb") as handle:
        handle.seek(offset)
        last_record = handle.read(size)
    return native.offline_last_record_value(last_record, _EX_DAILY_FMT, sys.modules[__name__], False)


def append_ex_daily_bar(filepath: str | Path, bars: list[ExDailyBar]) -> int:
    """将扩展市场 ``bars`` 追加写入 ``.day`` 文件，自动跳过不晚于末尾日期的记录。

    Raises:
        ResourceFormatError: 文件末尾存在残尾记录（长度不是记录长度的整数倍），
            此时拒绝写入以免后续记录整体错位。
    """

    filepath = Path(filepath)
    # 校验先于读末条日期：残尾文件上 ``get_last_ex_bar_date`` 读出的是对齐边界上的
    # 伪末条，若它恰好不小于所有待写 bar，``new_bars`` 会为空而把残尾静默留下。
    from eltdx._native_abi import load_native

    native = load_native()
    filepath, new_bars = native.offline_prepare_append(
        filepath, bars, lambda path: get_last_ex_bar_date(path), _EX_DAILY_FMT.size,
        "扩展市场日线", sys.modules[__name__], False
    )
    if not new_bars:
        return 0
    encoded = native.offline_encode_batch(new_bars, lambda bar: encode_ex_daily_bar(bar), None)
    with filepath.open("ab") as handle:
        handle.write(encoded)
    return len(new_bars)


def write_ex_daily_bar(filepath: str | Path, bars: list[ExDailyBar]) -> int:
    """将扩展市场 ``bars`` 同步追加到本地 ``.day`` 文件（公共入口）。

    Returns:
        实际写入的记录数。
    """

    return append_ex_daily_bar(filepath, bars)
