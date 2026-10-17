"""追加写入前的记录对齐校验。

通达信数据文件（``.day`` / ``.lc1`` / ``.5``）是定长记录的裸文件：每条记录
固定 ``record_size`` 字节，文件长度必须是它的整数倍。写盘 / 下载中断常留下
一个不足一条记录的**残尾**（partial tail）。

读取侧能自洽 —— :func:`~eltdx.offline.daily_bar.read_daily_bar` 只遍历完整
记录并忽略残尾。但**追加**侧不行：``open("ab")`` 从文件末尾（残尾之后）开始写，
新记录立刻错位，此后每条记录都从错误偏移开始被读出来，而且没有任何地方会报错。
实测：写 1 条正常记录 → 追加 1 字节残尾 → 再写 1 条，函数照常返回「写入 1 条」，
回读末条却是 ``date=891618943, close=5248.0``（期望 ``20260102 / 20.5``）。

所以本模块提供 :func:`ensure_aligned_for_append`，在写入前检查对齐；不对齐
就抛 :class:`~eltdx.exceptions.ResourceFormatError` **拒绝写入**，而不是静默写坏。
修复残尾是明确的人工动作，请自行截断到记录边界后再写。
"""

from __future__ import annotations

from pathlib import Path

from eltdx.exceptions import ResourceFormatError

__all__ = ["ensure_aligned_for_append"]


def ensure_aligned_for_append(filepath: str | Path, record_size: int, kind: str) -> None:
    """确保 ``filepath`` 可以安全追加 ``record_size`` 字节定长记录。

    Args:
        filepath: 目标文件（不存在时视为对齐，空文件可直接写）。
        record_size: 单条记录的字节数，必须为正。
        kind: 文件类型描述，用于报错文案（如 ``"日线"`` / ``"5 分钟线"``）。

    Raises:
        ValueError: ``record_size`` 非正数。
        ResourceFormatError: 文件存在但长度不是 ``record_size`` 的整数倍，
            即末尾有残尾记录。**此时不做任何写入**，避免静默破坏后续记录。
    """

    if record_size <= 0:
        raise ValueError(f"record_size must be positive, got {record_size}")

    path = Path(filepath)
    if not path.exists():
        return
    size = path.stat().st_size
    remainder = size % record_size
    if remainder == 0:
        return
    raise ResourceFormatError(
        f"{kind}文件存在残尾记录，拒绝追加: {path} "
        f"(大小 {size} 字节，模 {record_size} 余 {remainder} 字节)。"
        f"追加会让此后每条记录错位且不报错。请先把文件截断到 {size - remainder} 字节"
        f"（即最近一条完整记录的结尾）再写入。"
    )