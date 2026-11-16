"""通达信行业配置文件（``tdxhy.cfg``）解析。"""

from __future__ import annotations

from pathlib import Path

from eltdx.exceptions import ResourceFormatError


def parse_tdxhy_cfg(content: bytes) -> dict[str, tuple[str, str]]:
    """解析 ``tdxhy.cfg`` 字节内容。

    每行以 ``|`` 分隔，格式形如 ``市场|代码|通达信行业|||申万行业``。

    Returns:
        ``{代码: (通达信行业, 申万行业)}`` 字典，仅保留 6 位 A 股代码。
    """

    if not content:
        return {}
    from eltdx._native_abi import load_native

    return load_native().offline_industry(content)


def read_industry_cfg(filepath: str | Path) -> dict[str, tuple[str, str]]:
    """从本地 ``tdxhy.cfg`` 文件读取行业配置。

    Args:
        filepath: ``tdxhy.cfg`` 文件路径。

    Returns:
        ``{代码: (通达信行业, 申万行业)}`` 字典。

    Raises:
        FileNotFoundError: 文件不存在。
        ResourceFormatError: 文件无法按 GBK 解析为文本内容。
    """

    filepath = Path(filepath)
    if not filepath.is_file():
        raise FileNotFoundError(f"行业配置文件不存在: {filepath}")
    try:
        return parse_tdxhy_cfg(filepath.read_bytes())
    except UnicodeDecodeError as exc:  # pragma: no cover - GBK+replace 不会抛出
        raise ResourceFormatError(f"行业配置文件解析失败: {filepath}") from exc
