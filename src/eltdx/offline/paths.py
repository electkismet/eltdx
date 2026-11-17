"""通达信安装目录检测与路径解析。

只处理本地文件系统路径，不涉及任何网络请求。
"""

from __future__ import annotations

import os
import sys
from sys import modules as _MODULES
from pathlib import Path

_WIN_CANDIDATES = [
    Path(r"C:\new_jyplug"),
    Path(r"C:\new_tdx"),
    Path(r"D:\new_jyplug"),
    Path(r"D:\new_tdx"),
]

_UNIX_CANDIDATES = [
    Path.home() / "new_jyplug",
    Path.home() / "new_tdx",
]


def detect_tdx_home() -> Path | None:
    """按优先级检测通达信安装目录。

    1. ``TDX_HOME`` 环境变量
    2. 平台常见路径猜测

    Returns:
        安装目录的 :class:`~pathlib.Path`，未找到时返回 ``None``。
    """

    from eltdx._native_abi import load_native

    return load_native().offline_detect_home(_MODULES[__name__])


def resolve_vipdoc(path: str | Path | None = None) -> Path:
    """解析 ``vipdoc`` 数据目录。

    Args:
        path: 显式指定的 ``vipdoc`` 路径；为 ``None`` 时自动检测。

    Returns:
        ``vipdoc`` 目录的 :class:`~pathlib.Path`。

    Raises:
        FileNotFoundError: 无法定位 ``vipdoc`` 目录。
    """

    from eltdx._native_abi import load_native

    return load_native().offline_resolve_vipdoc(path, _MODULES[__name__])


def market_to_exchange(market: int) -> str:
    """市场代码 → ``vipdoc`` 子目录名（``sz`` / ``sh``）。

    Args:
        market: ``0`` 表示深圳，``1`` 表示上海。

    Raises:
        ValueError: 不支持的市场代码。
    """

    from eltdx._native_abi import load_native

    return load_native().offline_market_exchange(market)
