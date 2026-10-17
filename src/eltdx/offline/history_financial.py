"""历史财务数据解析（``gpcw.txt`` 列表 + ``gpcw*.dat`` / ``gpcw*.zip``）。

本模块不依赖任何网络组件，供 :mod:`eltdx.financial` 下载路径复用。
"""

from __future__ import annotations

import struct
import zipfile
from pathlib import Path

from eltdx.exceptions import ResourceFormatError

from .models import FinancialRecord

#: 历史财务 zip 的读取限额（防 ZIP 炸弹）。下载路径 :mod:`eltdx.financial`
#: 复用同一套口径，避免两处数值漂移。
MAX_HISTORY_ZIP_BYTES = 64 * 1024 * 1024
MAX_HISTORY_ZIP_ENTRIES = 64
MAX_HISTORY_DAT_BYTES = 256 * 1024 * 1024
#: 单条记录允许的最大 float32 字段数（``report_size / 4``）。
MAX_HISTORY_REPORT_FIELDS = 4096


def read_dat_from_zip(
    archive: zipfile.ZipFile,
    *,
    max_dat_bytes: int = MAX_HISTORY_DAT_BYTES,
    max_entries: int = MAX_HISTORY_ZIP_ENTRIES,
    source: str = "<zip>",
) -> bytes:
    """从已打开的 zip 中取出唯一的 ``.dat`` 内容，并做限额与重名校验。

    !!! warning "必须按 ``ZipInfo`` 读取，不能按文件名"
        ``archive.read(name)`` 在存在**同名条目**时会返回最后一个条目，于是对第一个
        条目做的大小 / 加密校验就形同虚设。这里校验并读取**同一个** ``ZipInfo``，
        并显式拒绝重名，杜绝绕过。

    Raises:
        ResourceFormatError: 条目过多、无 ``.dat``、``.dat`` 重名、条目加密，
            或解压后长度超出 ``max_dat_bytes``。
    """

    infos = archive.infolist()
    if len(infos) > max_entries:
        raise ResourceFormatError(f"zip 条目过多: {len(infos)} > {max_entries}（{source}）")

    dat_infos = [info for info in infos if info.filename.endswith(".dat")]
    if not dat_infos:
        raise ResourceFormatError(f"zip 中未找到 .dat 文件: {source}")

    names = [info.filename for info in dat_infos]
    duplicates = sorted({name for name in names if names.count(name) > 1})
    if duplicates:
        raise ResourceFormatError(f"zip 中存在重复的 .dat 文件名: {duplicates}（{source}）")

    info = dat_infos[0]
    if info.flag_bits & 0x1:
        raise ResourceFormatError(f"zip 条目已加密: {info.filename}")
    if info.file_size > max_dat_bytes:
        raise ResourceFormatError(
            f"{info.filename} 解压后超出上限 {max_dat_bytes} 字节"
            f"（声明 {info.file_size} 字节，{source}）"
        )
    # 传 ZipInfo 而非文件名：读的一定是刚校验过的那个条目。
    data = archive.read(info)
    if len(data) > max_dat_bytes:
        # 声明与实际不符（被构造过的 zip）：按实际长度再兜一次。
        raise ResourceFormatError(
            f"{info.filename} 实际解压长度 {len(data)} 超出上限 "
            f"{max_dat_bytes} 字节（{source}）"
        )
    return data


def parse_financial_file_list(data: bytes) -> list[tuple[str, str, int]]:
    """解析 ``tdxfin/gpcw.txt`` 的内容。

    每行格式：``filename,md5hash,filesize``。

    Returns:
        ``[(filename, hash, filesize), ...]``。
    """

    if not data:
        return []
    text = data.decode("utf-8", errors="replace").strip()
    results: list[tuple[str, str, int]] = []
    for line in text.split("\n"):
        line = line.strip()
        if not line:
            continue
        parts = line.split(",")
        if len(parts) < 3:
            continue
        try:
            filesize = int(parts[2])
        except ValueError:
            continue
        results.append((parts[0], parts[1], filesize))
    return results


def parse_financial_dat(
    data: bytes,
    report_date: int = 0,
) -> list[tuple[str, int, int, list[float]]]:
    """解析 ``gpcw*.zip`` 内的 ``.dat`` 二进制文件。

    二进制格式（参考 pytdx.crawler.history_financial_crawler）：

    * Header: 20 字节 ``<1hI1H3L``
        - unknown:     int16
        - report_date: uint32
        - max_count:   uint16   —— 股票索引条目数
        - unknown1:    uint32
        - report_size: uint32   —— 每条股票数据字节长度
        - unknown2:    uint32
    * Index: ``max_count`` 条，每条 11 字节 ``<6s1c1L``
        - code:        6 字节 —— 股票代码
        - market:      1 字节 —— 市场标识（0=SZ, 1=SH）
        - file_offset: uint32 —— 绝对偏移（从文件开头算）
    * Data: 在 ``file_offset`` 位置读取 ``report_size // 4`` 个 ``float32``

    Args:
        data: ``.dat`` 文件的完整字节。
        report_date: 报告期 YYYYMMDD（``0`` 则使用 header 中的值）。

    Returns:
        ``[(code, market_byte, report_date, [float, ...]), ...]``。

    Raises:
        ResourceFormatError: ``report_size`` 不是 4 的正整数倍，或字段数超出
            :data:`MAX_HISTORY_REPORT_FIELDS`（异常 header 可能申请超大内存）。
    """

    header_fmt = "<1hI1H3L"
    header_size = struct.calcsize(header_fmt)
    if len(data) < header_size:
        return []

    header = struct.unpack(header_fmt, data[:header_size])
    max_count = header[2]
    dat_report_date = header[1]
    report_size = header[4]
    if report_date == 0:
        report_date = dat_report_date

    # report_size 是记录字节长度，数据体按 float32 读取，因此必须是 4 的整数倍。
    # 早期直接 ``// 4``：非 4 的倍数被静默截断，异常/超大值也没有报错。
    if report_size == 0:
        return []
    if report_size % 4 != 0:
        raise ResourceFormatError(
            f"财报记录长度非法: report_size={report_size}（必须是 4 的整数倍）"
        )
    num_fields = report_size // 4
    if num_fields > MAX_HISTORY_REPORT_FIELDS:
        raise ResourceFormatError(
            f"财报记录字段数超出上限: {num_fields} > {MAX_HISTORY_REPORT_FIELDS}"
            f"（report_size={report_size}）"
        )

    index_fmt = "<6s1c1L"
    index_size = struct.calcsize(index_fmt)
    index_base = header_size
    report_fmt = f"<{num_fields}f"
    report_pack_size = struct.calcsize(report_fmt)

    results: list[tuple[str, int, int, list[float]]] = []
    for i in range(max_count):
        idx_pos = index_base + i * index_size
        if idx_pos + index_size > len(data):
            break
        code_bytes, market_byte, file_offset = struct.unpack(
            index_fmt, data[idx_pos : idx_pos + index_size]
        )
        code = code_bytes.decode("ascii", errors="replace").rstrip("\x00")
        if not code or file_offset == 0:
            continue
        if file_offset + report_pack_size > len(data):
            continue
        floats = list(
            struct.unpack(report_fmt, data[file_offset : file_offset + report_pack_size])
        )
        results.append((code, market_byte[0], report_date, floats))
    return results


def _read_from_zip(zip_path: Path) -> bytes:
    """从 zip 中提取第一个 ``.dat`` 文件内容（带体积 / 条目 / 解压后大小限额）。"""

    size = zip_path.stat().st_size
    if size > MAX_HISTORY_ZIP_BYTES:
        raise ResourceFormatError(
            f"历史财务 zip 超出体积上限 {MAX_HISTORY_ZIP_BYTES} 字节: "
            f"{zip_path}（实际 {size} 字节）"
        )
    try:
        with zipfile.ZipFile(zip_path, "r") as archive:
            # 显式传限额（而不是依赖 read_dat_from_zip 的默认参数）：
            # 默认值在函数定义时求值，模块级常量被调整后不会生效。
            return read_dat_from_zip(
                archive,
                max_dat_bytes=MAX_HISTORY_DAT_BYTES,
                max_entries=MAX_HISTORY_ZIP_ENTRIES,
                source=str(zip_path),
            )
    except zipfile.BadZipFile as exc:
        raise ResourceFormatError(f"无效的 zip 文件: {zip_path}") from exc


def read_history_financial(filepath: str | Path) -> list[FinancialRecord]:
    """从本地 ``gpcw*.dat`` 或 ``gpcw*.zip`` 文件读取历史财务数据。

    Args:
        filepath: ``.dat`` 或 ``.zip`` 文件路径。

    Returns:
        :class:`~eltdx.offline.models.FinancialRecord` 列表。

    Raises:
        FileNotFoundError: 文件不存在。
        ResourceFormatError: 文件格式非法。
    """

    filepath = Path(filepath)
    if not filepath.is_file():
        raise FileNotFoundError(f"历史财务数据文件不存在: {filepath}")

    if filepath.suffix.lower() == ".zip":
        data = _read_from_zip(filepath)
    else:
        data = filepath.read_bytes()

    results: list[FinancialRecord] = []
    for code, market_byte, report_date, fields in parse_financial_dat(data):
        results.append(
            FinancialRecord(
                code=code,
                market_id=market_byte,
                report_date=report_date,
                fields=tuple(fields),
            )
        )
    return results
