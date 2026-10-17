"""eltdx.ex — 通达信扩展行情（期货、港股、外股等，端口 7727）。"""

from __future__ import annotations

from eltdx.ex.client import ExTdxClient
from eltdx.ex.enums import ExMarket
from eltdx.ex.hosts import (
    DEFAULT_EX_HOSTS,
    DEFAULT_EX_PORT,
    EX_HOSTS_ENV,
    KNOWN_EX_HOSTS,
    MAC_EX_HOSTS,
    PROBE_VERIFIED_EX_HOSTS,
    ex_server_file_path,
    load_ex_hosts,
    save_ex_hosts,
)
from eltdx.ex.models import KNOWN_EX_MARKETS

__all__ = [
    "ExTdxClient",
    "MacExClient",
    "ExMarket",
    "DEFAULT_EX_HOSTS",
    "DEFAULT_EX_PORT",
    "EX_HOSTS_ENV",
    "KNOWN_EX_HOSTS",
    "KNOWN_EX_MARKETS",
    "MAC_EX_HOSTS",
    "PROBE_VERIFIED_EX_HOSTS",
    "ex_server_file_path",
    "load_ex_hosts",
    "save_ex_hosts",
]


def __getattr__(name: str) -> object:
    """按需导出 MacExClient（其依赖 eltdx.mac 子包）。"""

    if name == "MacExClient":
        from eltdx.ex.mac_client import MacExClient

        return MacExClient
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
