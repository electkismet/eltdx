"""扩展行情（端口 7727）默认服务器地址与用户可覆盖配置层。

候选主机来源优先级（高 → 低）：

1. 显式参数 ``ExTdxClient(host=..., hosts=...)``
2. 环境变量 :data:`EX_HOSTS_ENV`（``ELTDX_EX_HOSTS``，逗号/分号/空白分隔）
3. 用户配置文件 ``<data_dir>/ex_servers.json``
4. 包内默认 :data:`DEFAULT_EX_HOSTS`

第 2、3 条即「用户可覆盖配置层」：**不发版也能替换主机表**，而且完全离线——
本包不做任何线上主机列表拉取，避免「远端可下发任意 IP」的供应链风险。
``data_dir()`` 的解析见 :mod:`eltdx._paths`（``ELTDX_DATA_DIR`` 可覆盖）。

> 为什么不直接读通达信客户端的 ``connect.cfg``：各平台（Windows / Linux / macOS /
> 信创）客户端安装路径与文件名不固定，且用户可能根本没装客户端。因此本包只提供
> 自己的、位置确定的配置层。
"""

from __future__ import annotations

import json
import os
import re
from pathlib import Path
from threading import get_ident
from typing import Any, Iterable, Mapping

from eltdx._paths import data_dir

DEFAULT_EX_PORT: int = 7727

EX_HOSTS_ENV: str = "ELTDX_EX_HOSTS"
EX_SERVER_FILE: str = "ex_servers.json"
EX_SERVER_SCHEMA_VERSION: int = 1

# 通达信客户端官方 ``[DSHOST]`` 的顺序（16 台，端口均为 7727），作为来源记录保留。
KNOWN_EX_HOSTS: tuple[str, ...] = (
    "112.74.214.43",
    "120.25.218.6",
    "43.139.173.246",
    "159.75.90.107",
    "106.52.170.195",
    "139.9.191.175",
    "175.24.47.69",
    "150.158.9.199",
    "150.158.20.127",
    "49.235.119.116",
    "49.234.13.160",
    "116.205.143.214",
    "124.71.223.19",
    "113.45.175.47",
    "123.60.173.210",
    "118.89.69.202",
)

# 逐台实测（16 台 × 4 类命令的能力矩阵）后唯一能应答**枚举命令 0x23f5** 的主机。
# 16 台全部应答 ``0x23f0``（商品总数），所以只按 TCP 或只按 0x23f0 探活都没有区分度。
# 实测结论：count 16/16、info 1/16、quote 0/16、bars 0/16。
# 把实测可用的主机排在默认顺序最前面，这样即使某条路径没走并发探活，
# 首选主机也是能真正取数的。
PROBE_VERIFIED_EX_HOSTS: tuple[str, ...] = ("150.158.20.127",)

# 默认候选顺序：实测可用在前，其余按官方顺序跟在后面。
DEFAULT_EX_HOSTS: tuple[str, ...] = PROBE_VERIFIED_EX_HOSTS + tuple(
    host for host in KNOWN_EX_HOSTS if host not in PROBE_VERIFIED_EX_HOSTS
)

# MAC 协议扩展行情服务器（同样监听 7727）。
MAC_EX_HOSTS: tuple[str, ...] = (
    "116.205.135.205",
    "121.37.232.167",
)

_HOST_SPLIT = re.compile(r"[,;\s]+")


def normalize_ex_host(value: Any) -> str | None:
    """把配置项规整成裸主机名/IP；非法项返回 ``None``。

    只接受裸主机（``"203.0.113.10"``、``"example.com"``、``"[::1]"``）。
    **不接受 ``host:port``**：扩展行情全部内置主机都用 7727，端口由
    ``ExTdxClient(port=)``（默认 :data:`DEFAULT_EX_PORT`）统一指定；
    这里明确拒绝而不是静默丢弃端口，免得用户以为端口生效了。
    """

    if not isinstance(value, str):
        return None
    text = value.strip()
    if not text or any(character.isspace() for character in text):
        return None
    if text.startswith("["):
        end = text.find("]")
        if end == -1 or text[end + 1 :].strip():
            return None
        return text
    if ":" in text or "/" in text or len(text) > 253:
        return None
    return text


def ex_server_file_path() -> Path:
    """用户配置文件路径 ``<data_dir>/ex_servers.json``。"""

    return data_dir() / EX_SERVER_FILE


def parse_ex_hosts(values: Iterable[Any], *, source: str) -> list[str]:
    """规整一组主机项并去重；出现非法项直接抛 ``ValueError``。

    用户配置写坏了要立刻报错，不能静默回退到默认主机——否则用户会以为
    自己的配置生效了，实际一直在用包内列表。
    """

    hosts: list[str] = []
    for raw in values:
        host = normalize_ex_host(raw)
        if host is None:
            raise ValueError(
                f"{source} 中的主机项无效: {raw!r}；只接受裸主机名/IP"
                f"（例如 '203.0.113.10'），端口由 ExTdxClient(port=) 指定，默认 {DEFAULT_EX_PORT}"
            )
        if host not in hosts:
            hosts.append(host)
    return hosts


def load_ex_hosts_from_env(env: Mapping[str, str] | None = None) -> list[str]:
    """读取 ``ELTDX_EX_HOSTS``；未设置或为空返回 ``[]``。"""

    environ = os.environ if env is None else env
    text = environ.get(EX_HOSTS_ENV)
    if not text or not text.strip():
        return []
    parts = [part for part in _HOST_SPLIT.split(text.strip()) if part]
    return parse_ex_hosts(parts, source=EX_HOSTS_ENV)


def load_ex_hosts_from_file(path: str | os.PathLike[str] | None = None) -> list[str]:
    """读取用户配置文件；文件不存在返回 ``[]``，内容非法抛 ``ValueError``。"""

    target = Path(path) if path is not None else ex_server_file_path()
    try:
        raw = target.read_text(encoding="utf-8")
    except FileNotFoundError:
        return []
    except OSError as exc:
        raise ValueError(f"无法读取扩展行情主机配置 {target}: {exc}") from exc
    try:
        data = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise ValueError(f"扩展行情主机配置不是合法 JSON（{target}）: {exc}") from exc
    if not isinstance(data, dict):
        raise ValueError(f"扩展行情主机配置顶层必须是对象（{target}）")
    hosts = data.get("hosts")
    if not isinstance(hosts, list):
        raise ValueError(f"扩展行情主机配置缺少 hosts 数组（{target}）")
    return parse_ex_hosts(hosts, source=str(target))


def load_ex_hosts(
    *,
    env: Mapping[str, str] | None = None,
    path: str | os.PathLike[str] | None = None,
) -> list[str]:
    """解析生效的扩展行情主机列表：环境变量 > 用户配置 > 包内默认。"""

    from_env = load_ex_hosts_from_env(env)
    if from_env:
        return from_env
    from_file = load_ex_hosts_from_file(path)
    if from_file:
        return from_file
    return list(DEFAULT_EX_HOSTS)


def save_ex_hosts(
    hosts: Iterable[Any],
    *,
    path: str | os.PathLike[str] | None = None,
) -> Path:
    """把主机列表原子写入用户配置文件，返回写入路径。"""

    normalized = parse_ex_hosts(list(hosts), source="save_ex_hosts")
    if not normalized:
        raise ValueError("hosts 不能为空")
    target = Path(path) if path is not None else ex_server_file_path()
    target.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "schema_version": EX_SERVER_SCHEMA_VERSION,
        "hosts": normalized,
    }
    temporary = target.with_name(f".{target.name}.{os.getpid()}.{get_ident()}.tmp")
    try:
        with temporary.open("w", encoding="utf-8") as stream:
            json.dump(payload, stream, ensure_ascii=False, indent=2)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        try:
            temporary.chmod(0o600)
        except OSError:
            pass
        temporary.replace(target)
    finally:
        try:
            temporary.unlink()
        except FileNotFoundError:
            pass
    return target


__all__ = [
    "DEFAULT_EX_HOSTS",
    "DEFAULT_EX_PORT",
    "EX_HOSTS_ENV",
    "EX_SERVER_FILE",
    "EX_SERVER_SCHEMA_VERSION",
    "KNOWN_EX_HOSTS",
    "MAC_EX_HOSTS",
    "PROBE_VERIFIED_EX_HOSTS",
    "ex_server_file_path",
    "load_ex_hosts",
    "load_ex_hosts_from_env",
    "load_ex_hosts_from_file",
    "normalize_ex_host",
    "parse_ex_hosts",
    "save_ex_hosts",
]
