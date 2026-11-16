"""本地日线 K 线读取与编码（``.day`` 文件）。

``.day`` 记录为 32 字节小端序：日期(YYYYMMDD) 开盘 最高 最低 收盘 成交额
成交量 保留位。开盘/最高/最低/收盘为整数，需按证券类型乘以价格系数还原为
实际价格；成交量为整数，需乘以量系数。
"""

from __future__ import annotations

import struct
import sys
from pathlib import Path

from .models import DailyBar
from .paths import market_to_exchange, resolve_vipdoc

# struct 格式：日期 开 高 低 收 额(float) 量(int) 保留(int)
_DAILY_FMT = struct.Struct("<IIIIIfII")

# 证券类型 → (价格系数, 量系数)
_SECURITY_COEFFICIENTS: dict[str, tuple[float, float]] = {
    "SH_A_STOCK": (0.01, 0.01),
    "SH_B_STOCK": (0.001, 0.01),
    "SH_INDEX": (0.01, 1.0),
    "SH_FUND": (0.001, 1.0),
    "SH_BOND": (0.001, 1.0),
    "SZ_A_STOCK": (0.01, 0.01),
    "SZ_B_STOCK": (0.01, 0.01),
    "SZ_INDEX": (0.01, 1.0),
    "SZ_FUND": (0.001, 0.01),
    "SZ_BOND": (0.001, 0.01),
}


def detect_security_type(filename: str) -> str:
    """从文件名推断证券类型（``sh600000.day`` → ``SH_A_STOCK``）。

    依据上交所/深交所《证券代码段分配指南》判定；无法识别的代码段返回
    ``"UNKNOWN"``（而非默认深市 A 股），避免把基金/ETF/债券误判为股票。
    """

    from eltdx._native_abi import load_native

    return load_native().offline_security_type(filename, sys.modules[__name__])


def security_coefficients(filename: str) -> tuple[float, float]:
    """返回文件名对应证券类型的 ``(价格系数, 量系数)``。"""

    from eltdx._native_abi import load_native

    return load_native().offline_security_coefficients(filename, sys.modules[__name__])


def read_daily_bar(filepath: str | Path) -> list[DailyBar]:
    """从本地 ``.day`` 文件读取日线 K 线数据。

    Args:
        filepath: ``.day`` 文件路径。

    Returns:
        :class:`DailyBar` 列表（按文件顺序，即时间升序）。

    Raises:
        FileNotFoundError: 文件不存在。

    说明:
        末尾不足一条完整记录（32 字节）的残尾会被**忽略**：TDX 写盘或下载中断
        常留下截断文件，能读多少读多少，不因残尾让整次读取失败。
    """

    filepath = Path(filepath)
    if not filepath.is_file():
        raise FileNotFoundError(f"日线数据文件不存在: {filepath}")

    data = filepath.read_bytes()
    if len(data) < _DAILY_FMT.size:
        return []

    price_coeff, vol_coeff = security_coefficients(filepath.name)
    results: list[DailyBar] = []
    record_size = _DAILY_FMT.size
    # 上界写成 len(data) - record_size + 1，确保只遍历完整记录。
    for offset in range(0, len(data) - record_size + 1, record_size):
        raw = data[offset : offset + record_size]
        date_int, op, hi, lo, cl, amount, vol, adjust_flag = _DAILY_FMT.unpack(raw)
        results.append(
            DailyBar(
                year=date_int // 10000,
                month=(date_int % 10000) // 100,
                day=date_int % 100,
                open=op * price_coeff,
                high=hi * price_coeff,
                low=lo * price_coeff,
                close=cl * price_coeff,
                volume=vol * vol_coeff,
                amount=amount,
                adjust_flag=adjust_flag,
                raw=raw,
            )
        )
    return results


def encode_daily_bar(bar: DailyBar, price_coeff: float, vol_coeff: float) -> bytes:
    """将 :class:`DailyBar` 编码为 32 字节 ``.day`` 记录。

    Args:
        bar: K 线数据（``open``/``high``/``low``/``close`` 为实际价格）。
        price_coeff: 价格系数（A 股 0.01，基金 0.001 等）。
        vol_coeff: 成交量系数（A 股 0.01，指数 1.0 等）。
    """

    return _DAILY_FMT.pack(
        bar.date_int,
        int(round(bar.open / price_coeff)),
        int(round(bar.high / price_coeff)),
        int(round(bar.low / price_coeff)),
        int(round(bar.close / price_coeff)),
        bar.amount,
        int(round(bar.volume / vol_coeff)),
        bar.adjust_flag,
    )


def find_daily_bar_file(
    market: int,
    code: str,
    vipdoc: str | Path | None = None,
) -> Path:
    """根据市场和代码定位日线文件路径。"""

    from eltdx._native_abi import load_native

    return load_native().offline_find_bar(market, code, vipdoc, sys.modules[__name__], "lday", "day")
