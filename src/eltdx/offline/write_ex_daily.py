"""离线扩展市场日线写入 —— 追加 :class:`~eltdx.offline.models.ExDailyBar`。"""

from __future__ import annotations

from pathlib import Path

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

    filepath = Path(filepath)
    if not filepath.is_file():
        return None
    size = filepath.stat().st_size
    if size < _EX_DAILY_FMT.size:
        return None
    size -= size % _EX_DAILY_FMT.size
    with filepath.open("rb") as handle:
        handle.seek(size - _EX_DAILY_FMT.size)
        last_record = handle.read(_EX_DAILY_FMT.size)
    (date_int, *_) = _EX_DAILY_FMT.unpack(last_record)
    return int(date_int)


def append_ex_daily_bar(filepath: str | Path, bars: list[ExDailyBar]) -> int:
    """将扩展市场 ``bars`` 追加写入 ``.day`` 文件，自动跳过不晚于末尾日期的记录。

    Raises:
        ResourceFormatError: 文件末尾存在残尾记录（长度不是记录长度的整数倍），
            此时拒绝写入以免后续记录整体错位。
    """

    filepath = Path(filepath)
    # 校验先于读末条日期：残尾文件上 ``get_last_ex_bar_date`` 读出的是对齐边界上的
    # 伪末条，若它恰好不小于所有待写 bar，``new_bars`` 会为空而把残尾静默留下。
    ensure_aligned_for_append(filepath, _EX_DAILY_FMT.size, "扩展市场日线")

    last_date = get_last_ex_bar_date(filepath)
    new_bars = (
        [bar for bar in bars if bar.date_int > last_date]
        if last_date is not None
        else list(bars)
    )
    if not new_bars:
        return 0
    encoded = b"".join(encode_ex_daily_bar(bar) for bar in new_bars)
    with filepath.open("ab") as handle:
        handle.write(encoded)
    return len(new_bars)


def write_ex_daily_bar(filepath: str | Path, bars: list[ExDailyBar]) -> int:
    """将扩展市场 ``bars`` 同步追加到本地 ``.day`` 文件（公共入口）。

    Returns:
        实际写入的记录数。
    """

    return append_ex_daily_bar(filepath, bars)
