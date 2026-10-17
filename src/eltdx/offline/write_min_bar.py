"""离线分钟线写入 —— 将 :class:`~eltdx.offline.models.MinBar` 追加到分钟线文件。"""

from __future__ import annotations

from pathlib import Path

import struct
from .alignment import ensure_aligned_for_append
from .min_bar import (
    _LC_MIN_FMT,
    _MIN_FMT,
    _decode_tdx_date,
    _decode_tdx_time,
    encode_5min_bar,
    encode_min_bar,
)
from .models import MinBar

__all__ = [
    "write_min_bar",
    "append_min_bar",
    "write_5min_bar",
    "append_5min_bar",
    "get_last_min_bar_datetime",
    "get_last_5min_bar_datetime",
]


def _last_datetime(filepath: str | Path, fmt_size: int) -> tuple[int, int, int, int, int] | None:
    filepath = Path(filepath)
    if not filepath.is_file():
        return None
    size = filepath.stat().st_size
    # 先按记录长度向下对齐：写盘中断留下的残尾会让 ``seek(size - fmt_size)``
    # 落在错误偏移上，读出一个「伪末条」的日期时间。读取侧对残尾是容忍的
    # （能读多少读多少），但末条必须是**真实**的最后一条完整记录。
    size -= size % fmt_size
    if size < fmt_size:
        return None
    with filepath.open("rb") as handle:
        handle.seek(size - fmt_size)
        last_record = handle.read(fmt_size)
    date_num, time_num = struct.unpack_from("<HH", last_record, 0)
    year, month, day = _decode_tdx_date(date_num)
    hour, minute = _decode_tdx_time(time_num)
    return (year, month, day, hour, minute)


def get_last_min_bar_datetime(
    filepath: str | Path,
) -> tuple[int, int, int, int, int] | None:
    """读取 ``.lc1`` / ``.lc5`` 文件最后一条记录的日期时间。"""

    return _last_datetime(filepath, _LC_MIN_FMT.size)


def get_last_5min_bar_datetime(
    filepath: str | Path,
) -> tuple[int, int, int, int, int] | None:
    """读取 ``.5`` 文件最后一条记录的日期时间。"""

    return _last_datetime(filepath, _MIN_FMT.size)


def _append(
    filepath: str | Path,
    bars: list[MinBar],
    encode,
    last_dt_fn,
    record_size: int,
    kind: str,
) -> int:
    filepath = Path(filepath)
    # 校验先于读末条时间：残尾文件上 ``_last_datetime`` 读出的是对齐边界上的伪末条，
    # 若它恰好不小于所有待写 bar，``new_bars`` 会为空而把残尾静默留下。``last_dt_fn``
    # 是惰性传入的，确保校验通过后才真正去读盘。
    ensure_aligned_for_append(filepath, record_size, kind)

    last_dt = last_dt_fn()
    new_bars = (
        [bar for bar in bars if bar.datetime_key > last_dt]
        if last_dt is not None
        else list(bars)
    )
    if not new_bars:
        return 0
    with filepath.open("ab") as handle:
        handle.write(b"".join(encode(bar) for bar in new_bars))
    return len(new_bars)


def append_min_bar(filepath: str | Path, bars: list[MinBar]) -> int:
    """将分钟线 ``bars`` 追加写入 ``.lc1`` / ``.lc5`` 文件，自动跳过重复时间点。

    Raises:
        ResourceFormatError: 文件末尾存在残尾记录时拒绝写入。
    """

    return _append(
        filepath,
        bars,
        encode_min_bar,
        lambda: get_last_min_bar_datetime(filepath),
        _LC_MIN_FMT.size,
        "分钟线",
    )


def append_5min_bar(filepath: str | Path, bars: list[MinBar]) -> int:
    """将 5 分钟线 ``bars`` 追加写入 ``.5`` 文件，自动跳过重复时间点。

    Raises:
        ResourceFormatError: 文件末尾存在残尾记录时拒绝写入。
    """

    return _append(
        filepath,
        bars,
        encode_5min_bar,
        lambda: get_last_5min_bar_datetime(filepath),
        _MIN_FMT.size,
        "5 分钟线",
    )


def write_min_bar(filepath: str | Path, bars: list[MinBar]) -> int:
    """将分钟线 ``bars`` 同步追加到本地 ``.lc1`` / ``.lc5`` 文件（公共入口）。

    Returns:
        实际写入的记录数。
    """

    return append_min_bar(filepath, bars)


def write_5min_bar(filepath: str | Path, bars: list[MinBar]) -> int:
    """将 5 分钟线 ``bars`` 同步追加到本地 ``.5`` 文件（公共入口）。"""

    return append_5min_bar(filepath, bars)
