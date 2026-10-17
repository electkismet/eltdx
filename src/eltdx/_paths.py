"""跨平台用户数据目录解析。

7709 主机排名（``tdx_server_ranking.json``）与扩展行情主机配置
（``ex_servers.json``）都放在这里：按用户存放、与安装包分离，因此
重装或升级不会覆盖用户自己的配置。

优先级：``ELTDX_DATA_DIR`` 环境变量 > 各平台标准位置。
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

ENV_DATA_DIR = "ELTDX_DATA_DIR"


def data_dir() -> Path:
    """返回本包的用户数据目录（不保证已存在）。"""

    override = os.environ.get(ENV_DATA_DIR)
    if override:
        return Path(override).expanduser()
    if os.name == "nt":
        windows_data = os.environ.get("LOCALAPPDATA") or os.environ.get("APPDATA")
        return Path(windows_data or Path.home()) / "eltdx"
    if sys.platform == "darwin":
        return Path.home() / "Library" / "Application Support" / "eltdx"
    xdg_data = os.environ.get("XDG_DATA_HOME")
    return Path(xdg_data or Path.home() / ".local" / "share") / "eltdx"


__all__ = ["ENV_DATA_DIR", "data_dir"]
