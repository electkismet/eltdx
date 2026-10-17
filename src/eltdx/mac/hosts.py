"""MAC 行情服务器默认地址常量。

移植自 easy_tdx ``config.py`` 的 ``_FALLBACK_MAC_HOSTS``。
"""

from __future__ import annotations

# MAC 协议（0x1c 帧头）行情服务器默认地址池。
MAC_HOSTS: tuple[str, ...] = (
    "121.36.248.138",
    "123.60.47.136",
    "121.37.207.165",
)

# MAC 协议默认端口（与标准 7709 行情共用）。
MAC_PORT: int = 7709

# MAC 协议默认超时（秒）。
MAC_TIMEOUT: float = 15.0

__all__ = ["MAC_HOSTS", "MAC_PORT", "MAC_TIMEOUT"]