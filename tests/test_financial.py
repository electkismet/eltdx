"""专业财报下载与解析测试（纯离线，不联网）。

用鸭子类型 fake client（``resources.download_file`` 返回预置 bytes）验证
``list_archive_files`` / ``download_records`` 的串联，并覆盖 ``gpcw.txt`` 列表
解析与内存 zip 内最小 ``.dat`` 的字段解析。
"""

from __future__ import annotations

import io
import warnings
import struct
import zipfile

import pytest

from eltdx.exceptions import ResourceFormatError
from eltdx.financial import (
    CALC_HOSTS,
    FinancialFileInfo,
    FinancialRecord,
    download_records,
    list_archive_files,
    parse_financial_dat,
    parse_financial_file_list,
)

# ---------------------------------------------------------------------------
# helpers
# ---------------------------------------------------------------------------


class _FakeResources:
    """鸭子类型替身：只暴露 ``download_file(path, *, max_bytes=None) -> bytes``。"""

    def __init__(self, files: dict[str, bytes]) -> None:
        self._files = files
        self.calls: list[str] = []
        self.max_bytes_seen: list[int | None] = []

    def download_file(self, path: str, *, max_bytes: int | None = None) -> bytes:
        self.calls.append(path)
        self.max_bytes_seen.append(max_bytes)
        return self._files.get(path, b"")


class _FakeClient:
    def __init__(self, files: dict[str, bytes]) -> None:
        self.resources = _FakeResources(files)


def _build_dat(
    report_date: int = 20260331,
    stocks: list[tuple[str, int, list[float]]] | None = None,
) -> bytes:
    """构造一个最小的 ``.dat`` 二进制文件（与 parse_financial_dat 对应）。"""

    if stocks is None:
        stocks = [("600519", 1, [1.5, 2.5, 3.5])]

    num_fields = len(stocks[0][2])
    report_size = num_fields * 4
    max_count = len(stocks)

    header = struct.pack("<1hI1H3L", 0, report_date, max_count, 0, report_size, 0)

    index_fmt = "<6s1c1L"
    index_size = struct.calcsize(index_fmt)
    header_size = struct.calcsize("<1hI1H3L")
    data_start = header_size + max_count * index_size
    report_fmt = f"<{num_fields}f"

    data_chunks: list[bytes] = []
    offsets: list[int] = []
    offset = data_start
    for _code, _market, fields in stocks:
        offsets.append(offset)
        chunk = struct.pack(report_fmt, *fields)
        data_chunks.append(chunk)
        offset += len(chunk)

    index_entries = [
        struct.pack(index_fmt, code.encode("ascii"), bytes([market]), offsets[i])
        for i, (code, market, _fields) in enumerate(stocks)
    ]
    return header + b"".join(index_entries) + b"".join(data_chunks)


def _build_zip(dat_bytes: bytes, name: str = "gpcw20260331.dat") -> bytes:
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w") as archive:
        archive.writestr(name, dat_bytes)
    return buffer.getvalue()


# ---------------------------------------------------------------------------
# gpcw.txt 列表解析
# ---------------------------------------------------------------------------


class TestFileList:
    def test_parse_three_lines(self) -> None:
        data = (
            b"gpcw20260331.zip,abc123,5034901\n"
            b"gpcw20251231.zip,def456,5737165\n"
            b"gpcw20250930.zip,ghi789,4900000\n"
        )
        result = parse_financial_file_list(data)
        assert result == [
            ("gpcw20260331.zip", "abc123", 5034901),
            ("gpcw20251231.zip", "def456", 5737165),
            ("gpcw20250930.zip", "ghi789", 4900000),
        ]

    def test_empty(self) -> None:
        assert parse_financial_file_list(b"") == []

    def test_list_archive_files_duck_typed(self) -> None:
        listing = (
            b"gpcw20260331.zip,abc123,5034901\n"
            b"gpcw20251231.zip,def456,5737165\n"
            b"gpcw20250930.zip,ghi789,4900000\n"
        )
        client = _FakeClient({"tdxfin/gpcw.txt": listing})
        infos = list_archive_files(client)
        assert len(infos) == 3
        assert isinstance(infos[0], FinancialFileInfo)
        assert infos[0].filename == "gpcw20260331.zip"
        assert infos[0].hash == "abc123"
        assert infos[0].filesize == 5034901
        assert client.resources.calls == ["tdxfin/gpcw.txt"]


# ---------------------------------------------------------------------------
# .dat 字段解析
# ---------------------------------------------------------------------------


class TestParseDat:
    def test_single_stock_fields(self) -> None:
        dat = _build_dat(stocks=[("600519", 1, [1.5, 2.5, 3.5])])
        result = parse_financial_dat(dat, report_date=20260331)
        assert len(result) == 1
        code, market_id, report_date, fields = result[0]
        assert code == "600519"
        assert market_id == 1
        assert report_date == 20260331
        assert len(fields) == 3
        assert abs(fields[0] - 1.5) < 1e-6
        assert abs(fields[2] - 3.5) < 1e-6

    def test_multiple_stocks(self) -> None:
        dat = _build_dat(
            stocks=[("000001", 0, [10.0, 20.0]), ("600036", 1, [30.0, 40.0])]
        )
        result = parse_financial_dat(dat, report_date=20260630)
        assert [row[0] for row in result] == ["000001", "600036"]
        assert [row[1] for row in result] == [0, 1]

    def test_report_date_from_header(self) -> None:
        dat = _build_dat(report_date=20251231, stocks=[("000001", 0, [1.0])])
        result = parse_financial_dat(dat)
        assert result[0][2] == 20251231

    def test_empty(self) -> None:
        assert parse_financial_dat(b"") == []
        assert parse_financial_dat(b"\x00" * 10) == []


# ---------------------------------------------------------------------------
# download_records 端到端（fake client + 内存 zip）
# ---------------------------------------------------------------------------


class TestDownloadRecords:
    def test_zip_round_trip(self) -> None:
        dat = _build_dat(
            stocks=[("600519", 1, [1.5, 2.5, 3.5]), ("000001", 0, [4.0, 5.0, 6.0])]
        )
        client = _FakeClient({"tdxfin/gpcw20260331.zip": _build_zip(dat)})

        records = download_records(client, "tdxfin/gpcw20260331.zip")
        assert len(records) == 2
        assert isinstance(records[0], FinancialRecord)
        first = records[0]
        assert first.code == "600519"
        assert first.market_id == 1
        assert first.report_date == 20260331  # 从文件名解析
        assert first.fields == (1.5, 2.5, 3.5)
        assert records[1].code == "000001"
        assert records[1].market_id == 0

    def test_empty_payload_returns_empty(self) -> None:
        client = _FakeClient({})
        assert download_records(client, "tdxfin/gpcw20260331.zip") == []

    def test_calc_hosts_constant(self) -> None:
        assert isinstance(CALC_HOSTS, tuple)
        assert CALC_HOSTS and CALC_HOSTS[0] == "120.76.152.87"


# ---------------------------------------------------------------------------
# 财报 zip 解压限额（内存放大防护）
# ---------------------------------------------------------------------------


class TestDownloadRecordsZipLimits:
    """财报 zip 的限额必须在**解压/读取之前**校验。

    回归：``archive.read()`` 不看 ``file_size``，一个几 KB 的 zip 可以先撑爆内存
    再报错。与 :mod:`eltdx.stats` 的 ``MAX_STATS_*`` 同思路。
    """

    def test_rejects_oversized_uncompressed_dat(self, monkeypatch) -> None:
        import eltdx.financial as financial

        monkeypatch.setattr(financial, "MAX_FINANCIAL_DAT_BYTES", 16)
        client = _FakeClient(
            {"tdxfin/gpcw20260331.zip": _build_zip(b"\x00" * 1024)}
        )
        with pytest.raises(ResourceFormatError, match="解压后超出上限"):
            download_records(client, "tdxfin/gpcw20260331.zip")

    def test_rejects_oversized_zip_payload(self, monkeypatch) -> None:
        import eltdx.financial as financial

        monkeypatch.setattr(financial, "MAX_FINANCIAL_ZIP_BYTES", 8)
        client = _FakeClient(
            {"tdxfin/gpcw20260331.zip": _build_zip(b"\x00" * 64)}
        )
        with pytest.raises(ResourceFormatError, match="压缩体积上限"):
            download_records(client, "tdxfin/gpcw20260331.zip")

    def test_rejects_too_many_entries(self, monkeypatch) -> None:
        import eltdx.financial as financial

        monkeypatch.setattr(financial, "MAX_FINANCIAL_ZIP_ENTRIES", 1)
        buffer = io.BytesIO()
        with zipfile.ZipFile(buffer, "w") as archive:
            archive.writestr("a.dat", b"\x00" * 8)
            archive.writestr("b.dat", b"\x00" * 8)
        client = _FakeClient({"tdxfin/gpcw20260331.zip": buffer.getvalue()})
        with pytest.raises(ResourceFormatError, match="条目过多"):
            download_records(client, "tdxfin/gpcw20260331.zip")

    def test_normal_zip_still_works(self) -> None:
        """限额不能影响正常财报解析。"""

        dat = _build_dat(stocks=[("600519", 1, [1.5, 2.5, 3.5])])
        client = _FakeClient({"tdxfin/gpcw20260331.zip": _build_zip(dat)})
        records = download_records(client, "tdxfin/gpcw20260331.zip")
        assert len(records) == 1
        assert records[0].code == "600519"


# ---------------------------------------------------------------------------
# 第二轮评审回归项
# ---------------------------------------------------------------------------


class TestDownloadAndZipGuards:
    """回归：下载前未传 ``max_bytes``；ZIP 重名条目可绕过校验。"""

    def test_download_archive_passes_max_bytes(self) -> None:
        """限额必须下推到传输层，否则超大响应会先被完整分配进内存。"""

        import eltdx.financial as financial

        client = _FakeClient({"tdxfin/gpcw20260331.zip": b""})
        financial.download_archive(client, "tdxfin/gpcw20260331.zip")

        seen = client.resources.max_bytes_seen
        assert seen == [financial.MAX_FINANCIAL_ZIP_BYTES]

    def test_duplicate_dat_names_rejected(self) -> None:
        """同名 ``.dat`` 会让 ``archive.read(name)`` 读到后一个，绕过对前一个的校验。"""

        buffer = io.BytesIO()
        with warnings.catch_warnings():
            # zipfile 会对重名条目发 UserWarning —— 这里正是要构造这种情况。
            warnings.simplefilter("ignore", UserWarning)
            with zipfile.ZipFile(buffer, "w") as archive:
                archive.writestr("gpcw.dat", b"\x00" * 16)
                archive.writestr("gpcw.dat", b"\x00" * 4096)
        client = _FakeClient({"tdxfin/gpcw20260331.zip": buffer.getvalue()})

        with pytest.raises(ResourceFormatError, match="重复"):
            download_records(client, "tdxfin/gpcw20260331.zip")

    def test_normal_zip_still_reads(self) -> None:
        dat = _build_dat(stocks=[("600519", 1, [1.5, 2.5, 3.5])])
        client = _FakeClient({"tdxfin/gpcw20260331.zip": _build_zip(dat)})
        records = download_records(client, "tdxfin/gpcw20260331.zip")
        assert [r.code for r in records] == ["600519"]

    def test_report_size_not_multiple_of_four_rejected(self) -> None:
        """``report_size // 4`` 的静默截断改为显式报错。"""

        dat = bytearray(struct.pack("<1hI1H3L", 0, 20260331, 0, 0, 6, 0))
        client = _FakeClient({"tdxfin/gpcw20260331.zip": _build_zip(bytes(dat))})
        with pytest.raises(ResourceFormatError, match="report_size"):
            download_records(client, "tdxfin/gpcw20260331.zip")
