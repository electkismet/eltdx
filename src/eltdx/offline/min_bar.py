"""本地分钟 K 线读取与编码（``.5`` / ``.lc1`` / ``.lc5`` 文件）。

``.5`` 文件的 OHLC 为整数（实际价格 × 100）；``.lc1`` / ``.lc5`` 文件的
OHLC 为 ``float32``，无需系数转换。日期为 2 字节压缩日期，时间为从 0:00
起的分钟数。
"""

from __future__ import annotations

import struct
from pathlib import Path

from .models import MinBar
from .paths import market_to_exchange, resolve_vipdoc

# .5 文件：日期(2B) 时间(2B) 开(4B) 高(4B) 低(4B) 收(4B) 额(4Bf) 量(4B) 保留(4B)
_MIN_FMT = struct.Struct("<HHIIIIfII")

# .lc1/.lc5 文件：日期(2B) 时间(2B) 开(4Bf) 高 低 收 额(4Bf) 量(4B) 保留(4B)
_LC_MIN_FMT = struct.Struct("<HHfffffII")


def _decode_tdx_date(num: int) -> tuple[int, int, int]:
    """解码通达信 2 字节压缩日期。"""

    return num // 2048 + 2004, (num % 2048) // 100, (num % 2048) % 100


def _encode_tdx_date(year: int, month: int, day: int) -> int:
    """将 ``(year, month, day)`` 编码为 2 字节压缩日期。"""

    return (year - 2004) * 2048 + month * 100 + day


def _decode_tdx_time(num: int) -> tuple[int, int]:
    """解码从 0:00 起的分钟数为 ``(hour, minute)``。"""

    return num // 60, num % 60


def _encode_tdx_time(hour: int, minute: int) -> int:
    """将 ``(hour, minute)`` 编码为从 0:00 起的分钟数。"""

    return hour * 60 + minute


def _read_bars(
    data: bytes,
    fmt: struct.Struct,
    *,
    integer_prices: bool,
) -> list[MinBar]:
    if len(data) < fmt.size:
        return []
    results: list[MinBar] = []
    # 只遍历完整记录，忽略末尾残尾（TDX 写盘中断会留下截断文件）。
    for offset in range(0, len(data) - fmt.size + 1, fmt.size):
        raw = data[offset : offset + fmt.size]
        date_num, time_num, op, hi, lo, cl, amount, vol, reserved = fmt.unpack(raw)
        year, month, day = _decode_tdx_date(date_num)
        hour, minute = _decode_tdx_time(time_num)
        if integer_prices:
            op, hi, lo, cl = op / 100.0, hi / 100.0, lo / 100.0, cl / 100.0
        results.append(
            MinBar(
                year=year,
                month=month,
                day=day,
                hour=hour,
                minute=minute,
                open=op,
                high=hi,
                low=lo,
                close=cl,
                volume=float(vol),
                amount=amount,
                reserved=reserved,
                raw=raw,
            )
        )
    return results


def read_min_bar(filepath: str | Path) -> list[MinBar]:
    """从本地 ``.lc1`` / ``.lc5`` 文件读取分钟 K 线数据。

    Args:
        filepath: ``.lc1`` 或 ``.lc5`` 文件路径。

    Returns:
        :class:`MinBar` 列表（按时间升序）。

    Raises:
        FileNotFoundError: 文件不存在。

    说明:
        末尾不足一条完整记录的残尾会被**忽略**（与 A 股日线口径一致）。
    """

    filepath = Path(filepath)
    if not filepath.is_file():
        raise FileNotFoundError(f"分钟线数据文件不存在: {filepath}")
    return _read_bars(filepath.read_bytes(), _LC_MIN_FMT, integer_prices=False)


def read_5min_bar(filepath: str | Path) -> list[MinBar]:
    """从本地 ``.5`` 文件读取 5 分钟 K 线数据（OHLC 整数 ÷ 100）。"""

    filepath = Path(filepath)
    if not filepath.is_file():
        raise FileNotFoundError(f"分钟线数据文件不存在: {filepath}")
    return _read_bars(filepath.read_bytes(), _MIN_FMT, integer_prices=True)


def encode_min_bar(bar: MinBar) -> bytes:
    """将 :class:`MinBar` 编码为 32 字节 ``.lc1`` / ``.lc5`` 记录。"""

    return _LC_MIN_FMT.pack(
        _encode_tdx_date(bar.year, bar.month, bar.day),
        _encode_tdx_time(bar.hour, bar.minute),
        bar.open,
        bar.high,
        bar.low,
        bar.close,
        bar.amount,
        int(round(bar.volume)),
        bar.reserved,
    )


def encode_5min_bar(bar: MinBar) -> bytes:
    """将 :class:`MinBar` 编码为 32 字节 ``.5`` 记录（OHLC 整数 × 100）。"""

    return _MIN_FMT.pack(
        _encode_tdx_date(bar.year, bar.month, bar.day),
        _encode_tdx_time(bar.hour, bar.minute),
        int(round(bar.open * 100)),
        int(round(bar.high * 100)),
        int(round(bar.low * 100)),
        int(round(bar.close * 100)),
        bar.amount,
        int(round(bar.volume)),
        bar.reserved,
    )


def find_lc1_bar_file(
    market: int,
    code: str,
    vipdoc: str | Path | None = None,
) -> Path:
    """根据市场和代码定位 ``.lc1`` 分钟线文件路径。

    标准通达信目录布局里 **``.lc1`` 在 ``minline``**，只有 ``.lc5`` / ``.5``
    在 ``fzline``。早期三个查找函数一律用 ``fzline``，导致标准安装下找不到
    ``.lc1`` 文件。
    """

    vipdoc_path = resolve_vipdoc(vipdoc)
    exchange = market_to_exchange(market)
    return vipdoc_path / exchange / "minline" / f"{exchange}{code}.lc1"


def find_lc5_bar_file(
    market: int,
    code: str,
    vipdoc: str | Path | None = None,
) -> Path:
    """根据市场和代码定位 ``.lc5`` 分钟线文件路径。"""

    vipdoc_path = resolve_vipdoc(vipdoc)
    exchange = market_to_exchange(market)
    return vipdoc_path / exchange / "fzline" / f"{exchange}{code}.lc5"


def find_5min_bar_file(
    market: int,
    code: str,
    vipdoc: str | Path | None = None,
) -> Path:
    """根据市场和代码定位 ``.5`` 分钟线文件路径。"""

    vipdoc_path = resolve_vipdoc(vipdoc)
    exchange = market_to_exchange(market)
    return vipdoc_path / exchange / "fzline" / f"{exchange}{code}.5"
