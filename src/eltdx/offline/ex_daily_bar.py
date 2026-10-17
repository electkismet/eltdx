"""本地扩展市场日线读取与编码（期货/港股等 ``.day`` 文件）。

文件位于 ``vipdoc/ds/`` 目录下（如 ``29#A1801.day``），32 字节小端序：
日期(YYYYMMDD) 开盘(float) 最高(float) 最低(float) 收盘(float) 成交额(4B)
成交量(int) 结算价(float)。

.. important::
   第 6 个字段（「成交额」槽位）的**口径随品种而变**：

   * 期货 / 外盘：无符号整数，即成交额本身——用 :attr:`ExDailyBar.amount`；
   * 港股：``float32`` 的成交额——必须用 :attr:`ExDailyBar.hk_stock_amount`。

   若港股文件把该槽位当整数读，会得到 IEEE-754 位模式（例如
   ``0x4996B43C`` → ``1234613308``），而正确值是 ``1234567.5``。
   两个字段都保留，调用方按品种取用，不丢信息、也不影响写回。
"""

from __future__ import annotations

import struct
from pathlib import Path

from .models import ExDailyBar

# 日期(4B) 开(4Bf) 高 低 收 额(4B) 量(4B) 结算价(4Bf)
_EX_DAILY_FMT = struct.Struct("<IffffIIf")


def read_ex_daily_bar(filepath: str | Path) -> list[ExDailyBar]:
    """从本地扩展市场 ``.day`` 文件读取日线数据。

    Args:
        filepath: ``.day`` 文件路径（如 ``29#A1801.day``）。

    Returns:
        :class:`ExDailyBar` 列表（按时间升序）。

    Raises:
        FileNotFoundError: 文件不存在。

    说明:
        末尾不足一条完整记录（32 字节）的残尾会被**忽略**（与 easy_tdx 一致）。
    """

    filepath = Path(filepath)
    if not filepath.is_file():
        raise FileNotFoundError(f"扩展市场日线文件不存在: {filepath}")

    data = filepath.read_bytes()
    if len(data) < _EX_DAILY_FMT.size:
        return []

    results: list[ExDailyBar] = []
    record_size = _EX_DAILY_FMT.size
    # 上界写成 len(data) - record_size + 1，确保只遍历完整记录。
    for offset in range(0, len(data) - record_size + 1, record_size):
        raw = data[offset : offset + record_size]
        date_int, op, hi, lo, cl, amount, vol, settlement = _EX_DAILY_FMT.unpack(raw)
        # 同一 4 字节按 float32 再解释一次：港股文件里该槽位存的是成交额浮点值。
        (hk_stock_amount,) = struct.unpack("<f", struct.pack("<I", amount))
        results.append(
            ExDailyBar(
                year=date_int // 10000,
                month=(date_int % 10000) // 100,
                day=date_int % 100,
                open=op,
                high=hi,
                low=lo,
                close=cl,
                amount=amount,
                hk_stock_amount=hk_stock_amount,
                volume=vol,
                settlement=settlement,
                raw=raw,
            )
        )
    return results


def encode_ex_daily_bar(bar: ExDailyBar) -> bytes:
    """将 :class:`ExDailyBar` 编码为 32 字节扩展市场 ``.day`` 记录。"""

    return _EX_DAILY_FMT.pack(
        bar.date_int,
        bar.open,
        bar.high,
        bar.low,
        bar.close,
        bar.amount,
        bar.volume,
        bar.settlement,
    )
