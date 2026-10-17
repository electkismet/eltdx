"""离线数据读写测试（纯离线，不联网、不依赖通达信安装）。

覆盖：``.day`` 写→读往返、``.lc1``/``.lc5`` 写→读往返、扩展市场 ``.day`` 写→读
往返、gbbq 合成字节解析、板块 ``.dat`` 与行业 ``tdxhy.cfg`` 合成断言。
"""

from __future__ import annotations

import struct
from pathlib import Path

import pytest

from eltdx.offline import (
    DailyBar,
    ExDailyBar,
    MinBar,
    read_block,
    read_daily_bar,
    read_ex_daily_bar,
    read_gbbq,
    read_industry_cfg,
    read_min_bar,
    write_daily_bar,
    write_ex_daily_bar,
    write_min_bar,
)
from eltdx.offline.gbbq import _BIN_KEYS

# ---------------------------------------------------------------------------
# helpers
# ---------------------------------------------------------------------------

_MASK32 = 0xFFFFFFFF


def _make_daily_bar(
    year: int = 2026,
    month: int = 6,
    day: int = 6,
    open_: float = 10.25,
    high: float = 10.50,
    low: float = 10.10,
    close: float = 10.30,
    volume: float = 100000.0,
    amount: float = 1025000.0,
    adjust_flag: int = 0,
) -> DailyBar:
    return DailyBar(
        year=year,
        month=month,
        day=day,
        open=open_,
        high=high,
        low=low,
        close=close,
        volume=volume,
        amount=amount,
        adjust_flag=adjust_flag,
    )


def _make_min_bar(
    year: int = 2026,
    month: int = 6,
    day: int = 6,
    hour: int = 9,
    minute: int = 30,
    open_: float = 10.25,
    high: float = 10.50,
    low: float = 10.10,
    close: float = 10.30,
    volume: float = 5000.0,
    amount: float = 51250.0,
    reserved: int = 0,
) -> MinBar:
    return MinBar(
        year=year,
        month=month,
        day=day,
        hour=hour,
        minute=minute,
        open=open_,
        high=high,
        low=low,
        close=close,
        volume=volume,
        amount=amount,
        reserved=reserved,
    )


def _make_ex_bar(
    year: int = 2026,
    month: int = 6,
    day: int = 6,
    open_: float = 3500.0,
    high: float = 3550.0,
    low: float = 3480.0,
    close: float = 3520.0,
    amount: int = 123456,
    volume: int = 98765,
    settlement: float = 3510.0,
) -> ExDailyBar:
    return ExDailyBar(
        year=year,
        month=month,
        day=day,
        open=open_,
        high=high,
        low=low,
        close=close,
        amount=amount,
        volume=volume,
        settlement=settlement,
    )


# gbbq 加密（read_gbbq 解密逻辑的逆）—— 用于构造合成字节


def _key_u32(offset: int) -> int:
    return struct.unpack("<I", _BIN_KEYS[offset : offset + 4])[0]


def _round_f(num: int) -> int:
    eax = _key_u32(((num & 0xFF0000) >> 16) * 4 + 0x448)
    eax = (eax + _key_u32((num >> 24) * 4 + 0x48)) & _MASK32
    eax = (eax ^ _key_u32(((num & 0xFF00) >> 8) * 4 + 0x848)) & _MASK32
    eax = (eax + _key_u32((num & 0xFF) * 4 + 0xC48)) & _MASK32
    return eax


def _encrypt_gbbq_block(block: bytes) -> bytes:
    clear_a, clear_b = struct.unpack("<II", block)
    num, numold = clear_b, (clear_a ^ _key_u32(0)) & _MASK32
    for j in range(0x04, 0x40 + 1, 4):
        eax = _round_f(numold) ^ _key_u32(j)
        num, numold = numold, (num ^ eax) & _MASK32
    return struct.pack("<II", num ^ _key_u32(0x44), numold)


def _build_gbbq(records: list[tuple[int, str, int, int, float, float, float, float]]) -> bytes:
    out = struct.pack("<I", len(records))
    for market, code, dt, category, f1, f2, f3, f4 in records:
        clear = struct.pack(
            "<B7sIBffff", market, code.encode("utf-8"), dt, category, f1, f2, f3, f4
        )
        out += (
            _encrypt_gbbq_block(clear[0:8])
            + _encrypt_gbbq_block(clear[8:16])
            + _encrypt_gbbq_block(clear[16:24])
            + clear[24:29]
        )
    return out


def _build_block_dat(name: str, codes: list[str]) -> bytes:
    record = name.encode("gbk")[:9].ljust(9, b"\x00")
    record += struct.pack("<HH", len(codes), 0)
    area = b"".join(code.encode("ascii").ljust(7, b"\x00") for code in codes)
    record += area.ljust(2800, b"\x00")
    return b"\x00" * 384 + struct.pack("<H", 1) + record


# ---------------------------------------------------------------------------
# .day 日线往返
# ---------------------------------------------------------------------------


class TestDailyRoundTrip:
    def test_multiple_bars_round_trip(self, tmp_path: Path) -> None:
        bars_in = [
            _make_daily_bar(day=4, open_=10.00, high=10.20, low=9.90, close=10.10, volume=100000.0),
            _make_daily_bar(day=5, open_=10.10, high=10.30, low=10.00, close=10.25, volume=120000.0),
            _make_daily_bar(day=6, open_=10.25, high=10.50, low=10.10, close=10.30, volume=130000.0),
        ]
        filepath = tmp_path / "sh600000.day"
        written = write_daily_bar(filepath, bars_in)
        assert written == 3

        bars_out = read_daily_bar(filepath)
        assert len(bars_out) == 3
        first, last = bars_out[0], bars_out[2]
        assert (first.year, first.month, first.day) == (2026, 6, 4)
        assert (last.year, last.month, last.day) == (2026, 6, 6)
        assert abs(first.open - 10.00) < 0.001
        assert abs(first.high - 10.20) < 0.001
        assert abs(first.low - 9.90) < 0.001
        assert abs(last.close - 10.30) < 0.001
        assert abs(last.amount - 1025000.0) < 1.0
        assert abs(last.volume - 130000.0) < 0.01

    def test_adjust_flag_round_trip(self, tmp_path: Path) -> None:
        filepath = tmp_path / "sh600000.day"
        write_daily_bar(filepath, [_make_daily_bar(adjust_flag=1)])
        bars = read_daily_bar(filepath)
        assert bars[0].adjust_flag == 1

    def test_amount_round_trip_float32(self, tmp_path: Path) -> None:
        filepath = tmp_path / "sz000001.day"
        write_daily_bar(filepath, [_make_daily_bar(amount=2048000.0)])
        bars = read_daily_bar(filepath)
        assert abs(bars[0].amount - 2048000.0) < 1.0

    def test_append_skips_duplicate_date(self, tmp_path: Path) -> None:
        filepath = tmp_path / "sh600000.day"
        write_daily_bar(filepath, [_make_daily_bar(day=6)])
        written = write_daily_bar(filepath, [_make_daily_bar(day=6)])
        assert written == 0
        assert len(read_daily_bar(filepath)) == 1

    def test_truncated_file_keeps_complete_records(self, tmp_path: Path) -> None:
        """末尾残尾应被忽略：能读多少读多少（与 easy_tdx 一致）。

        TDX 写盘/下载中断常留下「整数条 + 残尾」的文件，旧实现对残尾直接抛
        ResourceFormatError，导致整份历史读不出来。
        """

        filepath = tmp_path / "sh600000.day"
        filepath.write_bytes(b"\x00" * 32 + b"\x00" * 3)
        bars = read_daily_bar(filepath)
        assert len(bars) == 1

    def test_file_shorter_than_one_record_returns_empty(self, tmp_path: Path) -> None:
        filepath = tmp_path / "sh600000.day"
        filepath.write_bytes(b"\x00" * 31)
        assert read_daily_bar(filepath) == []

    def test_empty_file_returns_empty(self, tmp_path: Path) -> None:
        filepath = tmp_path / "sh600000.day"
        filepath.write_bytes(b"")
        assert read_daily_bar(filepath) == []

    def test_missing_file_raises(self, tmp_path: Path) -> None:
        with pytest.raises(FileNotFoundError):
            read_daily_bar(tmp_path / "nope.day")


# ---------------------------------------------------------------------------
# .lc1 / .lc5 分钟线往返
# ---------------------------------------------------------------------------


class TestMinRoundTrip:
    def test_lc1_round_trip(self, tmp_path: Path) -> None:
        bars_in = [
            _make_min_bar(minute=30),
            _make_min_bar(minute=31, open_=10.30, close=10.40),
            _make_min_bar(minute=32, open_=10.40, close=10.35),
        ]
        filepath = tmp_path / "sh600000.lc1"
        assert write_min_bar(filepath, bars_in) == 3

        bars_out = read_min_bar(filepath)
        assert len(bars_out) == 3
        assert bars_out[0].hour == 9 and bars_out[0].minute == 30
        assert abs(bars_out[0].open - 10.25) < 1e-6
        assert abs(bars_out[0].close - 10.30) < 1e-6
        assert abs(bars_out[0].volume - 5000.0) < 1e-6
        assert abs(bars_out[0].amount - 51250.0) < 1e-3
        assert bars_out[2].minute == 32

    def test_lc5_round_trip(self, tmp_path: Path) -> None:
        filepath = tmp_path / "sz000001.lc5"
        write_min_bar(filepath, [_make_min_bar(open_=11.11, close=11.22)])
        bars = read_min_bar(filepath)
        assert len(bars) == 1
        assert abs(bars[0].open - 11.11) < 1e-6
        assert abs(bars[0].close - 11.22) < 1e-6

    def test_append_skips_duplicate_datetime(self, tmp_path: Path) -> None:
        filepath = tmp_path / "sh600000.lc1"
        write_min_bar(filepath, [_make_min_bar(minute=30)])
        assert write_min_bar(filepath, [_make_min_bar(minute=30)]) == 0
        assert write_min_bar(filepath, [_make_min_bar(minute=31)]) == 1
        assert len(read_min_bar(filepath)) == 2

    def test_5min_round_trip(self, tmp_path: Path) -> None:
        from eltdx.offline import read_5min_bar, write_5min_bar

        filepath = tmp_path / "sh600000.5"
        write_5min_bar(filepath, [_make_min_bar(open_=10.25, close=10.30)])
        bars = read_5min_bar(filepath)
        assert len(bars) == 1
        assert abs(bars[0].open - 10.25) < 0.01
        assert abs(bars[0].close - 10.30) < 0.01

    def test_truncated_lc1_keeps_complete_records(self, tmp_path: Path) -> None:
        """分钟线同样忽略末尾残尾。"""

        filepath = tmp_path / "sh600000.lc1"
        write_min_bar(filepath, [_make_min_bar(minute=30), _make_min_bar(minute=31)])
        data = filepath.read_bytes()
        filepath.write_bytes(data + b"\x00" * 9)
        assert len(read_min_bar(filepath)) == 2

    def test_lc1_shorter_than_one_record_returns_empty(self, tmp_path: Path) -> None:
        filepath = tmp_path / "sh600000.lc1"
        filepath.write_bytes(b"\x00" * 31)
        assert read_min_bar(filepath) == []


# ---------------------------------------------------------------------------
# ex_daily 扩展市场日线往返
# ---------------------------------------------------------------------------


class TestExDailyRoundTrip:
    def test_multiple_bars_round_trip(self, tmp_path: Path) -> None:
        bars_in = [
            _make_ex_bar(day=4, open_=3400.0),
            _make_ex_bar(day=5, open_=3450.0),
            _make_ex_bar(day=6, open_=3500.0),
        ]
        filepath = tmp_path / "29#A1801.day"
        assert write_ex_daily_bar(filepath, bars_in) == 3

        bars_out = read_ex_daily_bar(filepath)
        assert len(bars_out) == 3
        assert bars_out[0].day == 4
        assert bars_out[2].day == 6
        bar = bars_out[2]
        assert abs(bar.open - 3500.0) < 0.01
        assert abs(bar.high - 3550.0) < 0.01
        assert abs(bar.low - 3480.0) < 0.01
        assert abs(bar.close - 3520.0) < 0.01
        assert bar.amount == 123456
        assert bar.volume == 98765
        assert abs(bar.settlement - 3510.0) < 0.01

    def test_hk_amount_is_float32_view_of_same_bytes(self, tmp_path: Path) -> None:
        """港股文件的成交额槽位是 float32，不是整数。

        同一 4 字节 ``0x4996B43C``：按整数读是 1234613308（位模式），
        按 float32 读才是真实成交额 1234567.5。
        """

        import struct as _struct

        filepath = tmp_path / "29#A1801.day"
        # 该槽位在文件布局里是 4 字节无符号整数，所以先按 float32 打包再拼接。
        raw = (
            _struct.pack("<Iffff", 20260606, 1.5, 1.6, 1.4, 1.55)
            + _struct.pack("<f", 1234567.5)  # 成交额槽位的 float32 字节
            + _struct.pack("<If", 98765, 1.5)
        )
        filepath.write_bytes(raw)
        (bar,) = read_ex_daily_bar(filepath)
        assert bar.amount == 1234613308  # 整数视图（位模式）
        assert abs(bar.hk_stock_amount - 1234567.5) < 0.01  # float32 视图（真值）

    def test_truncated_ex_file_keeps_complete_records(self, tmp_path: Path) -> None:
        filepath = tmp_path / "29#A1801.day"
        filepath.write_bytes(b"\x00" * 32 + b"\x00" * 7)
        assert len(read_ex_daily_bar(filepath)) == 1

    def test_ex_file_shorter_than_one_record_returns_empty(self, tmp_path: Path) -> None:
        filepath = tmp_path / "29#A1801.day"
        filepath.write_bytes(b"\x00" * 31)
        assert read_ex_daily_bar(filepath) == []


# ---------------------------------------------------------------------------
# gbbq
# ---------------------------------------------------------------------------


class TestGbbq:
    def test_round_trip_synthetic_bytes(self, tmp_path: Path) -> None:
        records = [(1, "600519", 20260630, 1, 1.5, 2.25, 3.125, 4.0)]
        filepath = tmp_path / "gbbq"
        filepath.write_bytes(_build_gbbq(records))

        out = read_gbbq(filepath)
        assert len(out) == 1
        record = out[0]
        assert record.market == 1
        assert record.code == "600519"
        assert record.datetime == 20260630
        assert record.category == 1
        assert abs(record.hongli_panqianliutong - 1.5) < 1e-6
        assert abs(record.peigujia_qianzongguben - 2.25) < 1e-6
        assert abs(record.songgu_qianzongguben - 3.125) < 1e-6
        assert abs(record.peigu_houzongguben - 4.0) < 1e-6

    def test_empty_file(self, tmp_path: Path) -> None:
        filepath = tmp_path / "gbbq"
        filepath.write_bytes(struct.pack("<I", 0))
        assert read_gbbq(filepath) == []

    def test_missing_file_raises(self, tmp_path: Path) -> None:
        with pytest.raises(FileNotFoundError):
            read_gbbq(tmp_path / "missing.gbbq")


# ---------------------------------------------------------------------------
# block / industry
# ---------------------------------------------------------------------------


class TestBlockAndIndustry:
    def test_read_block_synthetic(self, tmp_path: Path) -> None:
        filepath = tmp_path / "block_gn.dat"
        filepath.write_bytes(_build_block_dat("白酒", ["600519", "000858"]))
        blocks = read_block(filepath)
        assert len(blocks) == 1
        block = blocks[0]
        assert block.name == "白酒"
        assert block.category == 2  # gn → 概念
        assert block.count == 2
        assert block.codes == ("600519", "000858")

    def test_read_industry_synthetic(self, tmp_path: Path) -> None:
        filepath = tmp_path / "tdxhy.cfg"
        content = (
            "0|600519|制造业|||食品饮料\n"
            "1|000001|金融业|||银行\n"
            "0|notacode|X|||Y\n"
        )
        filepath.write_bytes(content.encode("gbk"))
        mapping = read_industry_cfg(filepath)
        assert mapping["600519"] == ("制造业", "食品饮料")
        assert mapping["000001"] == ("金融业", "银行")
        assert "notacode" not in mapping


# ---------------------------------------------------------------------------
# 写入侧残尾拒绝（对齐闸门）
# ---------------------------------------------------------------------------


class TestAppendRejectsPartialTail:
    """追加写入必须先确认文件按记录长度对齐。

    回归：``open("ab")`` 从残尾之后开始写，新记录立刻错位，之后每条记录都从错误
    偏移被读出，而写入函数仍照常返回「写入 N 条」。实测末条读出 ``date=891618943``
    而非 ``20260102`` —— 静默写坏，没有任何异常。

    注意与 ``TestDailyRoundTrip.test_truncated_file_keeps_complete_records`` 的分工：
    **读取**容忍残尾（能读多少读多少），**写入**拒绝残尾（避免制造错位）。
    """

    def _corrupt_tail(self, filepath: Path) -> int:
        """在正常文件后追加 1 字节残尾，返回写入前的大小。"""

        with filepath.open("ab") as handle:
            handle.write(b"\x7f")
        return filepath.stat().st_size

    def test_daily_append_rejects_partial_tail(self, tmp_path: Path) -> None:
        from eltdx.exceptions import ResourceFormatError

        filepath = tmp_path / "sh600000.day"
        write_daily_bar(filepath, [_make_daily_bar(day=6)])
        size_before = self._corrupt_tail(filepath)

        with pytest.raises(ResourceFormatError, match="残尾"):
            write_daily_bar(filepath, [_make_daily_bar(day=7)])

        # 拒绝写入 = 文件一个字节都不能变
        assert filepath.stat().st_size == size_before

    def test_ex_daily_append_rejects_partial_tail(self, tmp_path: Path) -> None:
        from eltdx.exceptions import ResourceFormatError

        filepath = tmp_path / "ds_day.bin"
        write_ex_daily_bar(filepath, [_make_ex_bar(day=6)])
        size_before = self._corrupt_tail(filepath)

        with pytest.raises(ResourceFormatError, match="残尾"):
            write_ex_daily_bar(filepath, [_make_ex_bar(day=7)])

        assert filepath.stat().st_size == size_before

    def test_min_append_rejects_partial_tail(self, tmp_path: Path) -> None:
        from eltdx.exceptions import ResourceFormatError

        filepath = tmp_path / "sh600000.lc1"
        write_min_bar(filepath, [_make_min_bar(hour=9, minute=31)])
        size_before = self._corrupt_tail(filepath)

        with pytest.raises(ResourceFormatError, match="残尾"):
            write_min_bar(filepath, [_make_min_bar(hour=9, minute=32)])

        assert filepath.stat().st_size == size_before

    def test_5min_append_rejects_partial_tail(self, tmp_path: Path) -> None:
        from eltdx.exceptions import ResourceFormatError

        from eltdx.offline import write_5min_bar

        filepath = tmp_path / "sh600000.5"
        write_5min_bar(filepath, [_make_min_bar(hour=9, minute=35)])
        size_before = self._corrupt_tail(filepath)

        with pytest.raises(ResourceFormatError, match="残尾"):
            write_5min_bar(filepath, [_make_min_bar(hour=9, minute=40)])

        assert filepath.stat().st_size == size_before

    def test_append_after_manual_truncation_works_again(self, tmp_path: Path) -> None:
        """报错文案指引的修复动作（截断到记录边界）确实能恢复写入。"""

        from eltdx.exceptions import ResourceFormatError

        filepath = tmp_path / "sh600000.day"
        write_daily_bar(filepath, [_make_daily_bar(day=6)])
        self._corrupt_tail(filepath)
        with pytest.raises(ResourceFormatError):
            write_daily_bar(filepath, [_make_daily_bar(day=7)])

        # 截断回最后一条完整记录（32 字节）
        with filepath.open("r+b") as handle:
            handle.truncate(32)

        assert write_daily_bar(filepath, [_make_daily_bar(day=7)]) == 1
        bars = read_daily_bar(filepath)
        assert [b.day for b in bars] == [6, 7]

    def test_empty_and_missing_files_are_writable(self, tmp_path: Path) -> None:
        """空文件与不存在文件都是天然对齐的，不该被拦。"""

        empty = tmp_path / "empty.day"
        empty.write_bytes(b"")
        assert write_daily_bar(empty, [_make_daily_bar(day=6)]) == 1

        fresh = tmp_path / "fresh.day"
        assert write_daily_bar(fresh, [_make_daily_bar(day=6)]) == 1
        assert len(read_daily_bar(fresh)) == 1

    def test_aligned_multi_record_file_is_not_rejected(self, tmp_path: Path) -> None:
        """正常的多记录文件（含刚写完的）必须一路放行。"""

        filepath = tmp_path / "sh600000.day"
        assert write_daily_bar(filepath, [_make_daily_bar(day=4), _make_daily_bar(day=5)]) == 2
        assert write_daily_bar(filepath, [_make_daily_bar(day=6)]) == 1
        assert len(read_daily_bar(filepath)) == 3


# ---------------------------------------------------------------------------
# 第二轮评审回归项
# ---------------------------------------------------------------------------


def _build_block_cfg(entries: list[tuple[str, str]]) -> bytes:
    """构造 ``blocknew.cfg``：每条 120 字节（50B 名称 + 70B 文件名）。"""

    data = bytearray()
    for name, filename in entries:
        data += name.encode("gbk").ljust(50, b"\x00")[:50]
        data += filename.encode("gbk").ljust(70, b"\x00")[:70]
    return bytes(data)


class TestLastBarDatetimeAlignment:
    """回归：``get_last_*_bar_datetime()`` 未按记录长度对齐。

    残尾文件上 ``seek(size - record_size)`` 落在错误偏移，读出的是「伪末条」。
    """

    def test_last_min_bar_datetime_ignores_partial_tail(self, tmp_path: Path) -> None:
        from eltdx.offline import get_last_min_bar_datetime

        filepath = tmp_path / "sh600000.lc1"
        write_min_bar(filepath, [_make_min_bar(day=6), _make_min_bar(day=7)])
        expected = get_last_min_bar_datetime(filepath)
        assert expected is not None and expected[2] == 7

        with filepath.open("ab") as handle:
            handle.write(b"\x01\x02\x03")  # 残尾，不足一条记录

        assert get_last_min_bar_datetime(filepath) == expected

    def test_last_5min_bar_datetime_ignores_partial_tail(self, tmp_path: Path) -> None:
        from eltdx.offline import get_last_5min_bar_datetime

        filepath = tmp_path / "sh600000.5"
        write_min_bar(filepath, [_make_min_bar(day=6), _make_min_bar(day=7)])
        expected = get_last_5min_bar_datetime(filepath)
        assert expected is not None and expected[2] == 7

        with filepath.open("ab") as handle:
            handle.write(b"\x01\x02\x03")

        assert get_last_5min_bar_datetime(filepath) == expected


class TestSecurityCoefficients:
    """回归：``SZ_BOND`` 量系数写成了 1.0，债券成交量放大 100 倍。"""

    def test_sz_bond_volume_coefficient(self, tmp_path: Path) -> None:
        from eltdx.offline.daily_bar import security_coefficients

        _, vol_coeff = security_coefficients("sz123456.day")
        assert vol_coeff == pytest.approx(0.01)

    def test_sz_bond_volume_scaled_on_read(self, tmp_path: Path) -> None:
        filepath = tmp_path / "sz123456.day"
        raw = struct.pack("<IIIIIfII", 20260102, 10500, 10600, 10400, 10500, 12345.0, 10000, 0)
        filepath.write_bytes(raw)

        bars = read_daily_bar(filepath)
        assert bars[0].close == pytest.approx(10.5)
        assert bars[0].volume == pytest.approx(100.0)


class TestBarFileLocations:
    """回归：``.lc1`` 在标准布局里位于 ``minline``，不是 ``fzline``。"""

    def test_lc1_uses_minline(self, tmp_path: Path) -> None:
        from eltdx.offline import find_lc1_bar_file

        assert find_lc1_bar_file(0, "600000", tmp_path).parent.name == "minline"

    def test_lc5_and_5_use_fzline(self, tmp_path: Path) -> None:
        from eltdx.offline import find_5min_bar_file, find_lc5_bar_file

        assert find_lc5_bar_file(0, "600000", tmp_path).parent.name == "fzline"
        assert find_5min_bar_file(0, "600000", tmp_path).parent.name == "fzline"


class TestCustomerBlockPathTraversal:
    """回归：``blocknew.cfg`` 里的文件名未校验，可跳出板块目录读取任意文件。"""

    def _prepare(self, tmp_path: Path, blk_filename: str) -> Path:
        block_dir = tmp_path / "blocknew"
        block_dir.mkdir()
        (block_dir / "blocknew.cfg").write_bytes(
            _build_block_cfg([("测试板块", blk_filename)])
        )
        return block_dir

    @pytest.mark.parametrize(
        "bad_name",
        ["../evil", "..\\evil", "/etc/passwd", "C://Windows//win", "sub/dir", ".."],
    )
    def test_rejects_path_components(self, tmp_path: Path, bad_name: str) -> None:
        from eltdx.exceptions import ResourceFormatError
        from eltdx.offline import read_customer_block

        block_dir = self._prepare(tmp_path, bad_name)
        with pytest.raises(ResourceFormatError, match="非法|越界"):
            read_customer_block(block_dir)

    def test_normal_name_still_works(self, tmp_path: Path) -> None:
        from eltdx.offline import read_customer_block

        block_dir = self._prepare(tmp_path, "ZXG")
        (block_dir / "ZXG.blk").write_text("1600000\n0000001\n", encoding="utf-8")
        blocks = read_customer_block(block_dir)
        assert [b.block_type for b in blocks] == ["ZXG"]
        assert blocks[0].codes == ("600000", "000001")


class TestBlockTruncatedRecords:
    """回归：``read_block`` 遇到截断记录直接 ``break``，静默丢掉尾部数据。"""

    def test_truncated_block_file_raises(self, tmp_path: Path) -> None:
        from eltdx.exceptions import ResourceFormatError

        filepath = tmp_path / "block_zs.dat"
        data = _build_block_dat("银行", ["600000"])
        filepath.write_bytes(data[: len(data) - 10])  # 切掉记录尾巴

        with pytest.raises(ResourceFormatError, match="截断"):
            read_block(filepath)

    def test_complete_block_file_ok(self, tmp_path: Path) -> None:
        filepath = tmp_path / "block_zs.dat"
        filepath.write_bytes(_build_block_dat("银行", ["600000"]))
        assert len(read_block(filepath)) == 1


class TestHistoryFinancialLimits:
    """回归：``history_financial`` 的 ZIP 路径无任何限额、``report_size`` 被静默截断。"""

    def test_parse_rejects_report_size_not_multiple_of_four(self) -> None:
        from eltdx.exceptions import ResourceFormatError
        from eltdx.offline import parse_financial_dat

        # <1hI1H3L>：unknown, report_date, max_count, unknown1, report_size, unknown2
        header = struct.pack("<1hI1H3L", 0, 20260331, 0, 0, 6, 0)
        with pytest.raises(ResourceFormatError, match="report_size"):
            parse_financial_dat(header)

    def test_parse_rejects_absurd_report_size(self) -> None:
        from eltdx.exceptions import ResourceFormatError
        from eltdx.offline import parse_financial_dat

        header = struct.pack("<1hI1H3L", 0, 20260331, 0, 0, 4 * 10_000_000, 0)
        with pytest.raises(ResourceFormatError, match="字段数超出上限"):
            parse_financial_dat(header)

    def test_zip_rejects_too_many_entries(self, tmp_path: Path) -> None:
        import zipfile

        from eltdx.exceptions import ResourceFormatError

        path = tmp_path / "gpcw.zip"
        with zipfile.ZipFile(path, "w") as archive:
            for i in range(5):
                archive.writestr(f"x{i}.bin", b"0")

        from eltdx.offline import history_financial as hf

        original = hf.MAX_HISTORY_ZIP_ENTRIES
        hf.MAX_HISTORY_ZIP_ENTRIES = 2
        try:
            with pytest.raises(ResourceFormatError, match="条目过多"):
                from eltdx.offline import read_history_financial

                read_history_financial(path)
        finally:
            hf.MAX_HISTORY_ZIP_ENTRIES = original

    def test_zip_rejects_oversized_dat(self, tmp_path: Path) -> None:
        import zipfile

        from eltdx.exceptions import ResourceFormatError
        from eltdx.offline import history_financial as hf

        path = tmp_path / "gpcw.zip"
        with zipfile.ZipFile(path, "w") as archive:
            archive.writestr("gpcw.dat", b"\x00" * 1024)

        original = hf.MAX_HISTORY_DAT_BYTES
        hf.MAX_HISTORY_DAT_BYTES = 16
        try:
            with pytest.raises(ResourceFormatError, match="解压后超出上限"):
                from eltdx.offline import read_history_financial

                read_history_financial(path)
        finally:
            hf.MAX_HISTORY_DAT_BYTES = original
