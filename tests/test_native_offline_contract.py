"""Offline compatibility source cases; execution waits for the full source gate."""
from __future__ import annotations

from pathlib import Path
import math
import struct
from types import SimpleNamespace

import pytest

from eltdx.offline import daily_bar, industry, paths
from eltdx.offline import min_bar
from eltdx.offline import ex_daily_bar
from eltdx.offline import alignment, write_daily, write_ex_daily, write_min_bar


def test_industry_shared_rows_keep_unicode_length_whitespace_and_last_value():
    data = (
        "0|600000|first|||sw-first\r\n"
        "1|一二三四五六|中文|||申万\x1c"
        "0|600000|second|||sw-second\n"
        "0|000001|  raw industry  |x\n"
        "0|too-long|skip\n"
    ).encode("gbk")
    assert industry.parse_tdxhy_cfg(data) == {
        "600000": ("second", "sw-second"),
        "一二三四五六": ("中文", "申万"),
        "000001": ("  raw industry  ", ""),
    }
    assert list(industry.parse_tdxhy_cfg(data)) == ["600000", "一二三四五六", "000001"]


def test_industry_custom_decode_and_replacement_are_preserved():
    class Content:
        def decode(self, encoding, *, errors):
            assert (encoding, errors) == ("gbk", "replace")
            return "0|abcdef|custom|||sw"

    assert industry.parse_tdxhy_cfg(Content()) == {"abcdef": ("custom", "sw")}
    assert industry.parse_tdxhy_cfg(b"0|600000|bad\x81\x20|||sw") == {"600000": ("bad\ufffd ", "sw")}


def test_industry_empty_input_does_not_load_native(monkeypatch):
    from eltdx import _native_abi

    def unexpected():
        raise AssertionError("empty content returns before extension load")

    monkeypatch.setattr(_native_abi, "load_native", unexpected)
    assert industry.parse_tdxhy_cfg(b"") == {}


def test_industry_file_calls_live_parser_and_wraps_unicode_failure(tmp_path, monkeypatch):
    path = tmp_path / "tdxhy.cfg"
    path.write_bytes(b"raw")
    marker = object()
    monkeypatch.setattr(industry, "parse_tdxhy_cfg", lambda data: marker)
    assert industry.read_industry_cfg(path) is marker
    failure = UnicodeDecodeError("gbk", b"\x81", 0, 1, "custom")

    def parser(data):
        raise failure

    monkeypatch.setattr(industry, "parse_tdxhy_cfg", parser)
    with pytest.raises(industry.ResourceFormatError) as caught:
        industry.read_industry_cfg(path)
    assert str(caught.value) == f"行业配置文件解析失败: {path}"
    assert caught.value.__cause__ is failure


@pytest.mark.parametrize("name,kind", [
    ("SH600000.DAY", "SH_A_STOCK"), ("sz200001.day", "SZ_B_STOCK"),
    ("sh900001.day", "SH_B_STOCK"), ("sh880001.day", "SH_INDEX"),
    ("sz390001.day", "SZ_INDEX"), ("sh580001.day", "SH_FUND"),
    ("sz180001.day", "SZ_FUND"), ("sh200001.day", "SH_BOND"),
    ("sz140001.day", "SZ_BOND"), ("bj430001.day", "UNKNOWN"),
    ("s中00.day", "UNKNOWN"), ("sz", "UNKNOWN"),
])
def test_security_prefix_shared_classification(name, kind):
    assert daily_bar.detect_security_type(name) == kind


def test_security_coefficients_preserve_runtime_mapping_and_detector(monkeypatch):
    marker = object()
    monkeypatch.setattr(daily_bar, "detect_security_type", lambda filename: "custom")
    monkeypatch.setitem(daily_bar._SECURITY_COEFFICIENTS, "custom", marker)
    assert daily_bar.security_coefficients("ignored") is marker


def test_paths_discovery_keeps_env_first_and_lazy_candidate_order(monkeypatch):
    events = []

    class Candidate:
        def __init__(self, name, exists):
            self.name = name
            self.exists = exists

        def is_dir(self):
            events.append(self.name)
            return self.exists

    env = Candidate("environment", False)
    first = Candidate("first", False)
    second = Candidate("second", True)
    monkeypatch.setattr(paths.os, "getenv", lambda key: "configured")
    monkeypatch.setattr(paths, "Path", lambda value: env)
    monkeypatch.setattr(paths, "sys", SimpleNamespace(platform="win32"))
    monkeypatch.setattr(paths, "_WIN_CANDIDATES", [first, second, Candidate("unused", True)])
    assert paths.detect_tdx_home() is second
    assert events == ["environment", "first", "second"]


def test_paths_resolve_keeps_explicit_path_and_custom_detector(monkeypatch):
    candidate = SimpleNamespace(is_dir=lambda: True)
    monkeypatch.setattr(paths, "Path", lambda value: candidate)
    assert paths.resolve_vipdoc("configured") is candidate
    monkeypatch.setattr(paths, "detect_tdx_home", lambda: None)
    with pytest.raises(FileNotFoundError, match="无法定位通达信安装目录"):
        paths.resolve_vipdoc()


def test_market_mapping_keeps_python_equality_and_error_format():
    assert paths.market_to_exchange(False) == "sz"
    assert paths.market_to_exchange(1.0) == "sh"

    class Market:
        def __eq__(self, other):
            return other == 1

    assert paths.market_to_exchange(Market()) == "sh"
    with pytest.raises(ValueError, match="不支持的市场代码: 2"):
        paths.market_to_exchange(2)


def test_daily_path_uses_live_resolution_and_keeps_code_formatting(monkeypatch):
    calls = []

    def resolve(value):
        calls.append(("resolve", value))
        return Path("root")

    def exchange(value):
        calls.append(("market", value))
        return "custom"

    monkeypatch.setattr(daily_bar, "resolve_vipdoc", resolve)
    monkeypatch.setattr(daily_bar, "market_to_exchange", exchange)
    assert daily_bar.find_daily_bar_file(7, "../alias", "source") == Path("root/custom/lday/custom../alias.day")
    assert calls == [("resolve", "source"), ("market", 7)]


@pytest.mark.parametrize("function,folder,suffix", [
    (min_bar.find_lc1_bar_file, "minline", "lc1"),
    (min_bar.find_lc5_bar_file, "fzline", "lc5"),
    (min_bar.find_5min_bar_file, "fzline", "5"),
])
def test_minute_path_shared_builder_keeps_each_layout(function, folder, suffix, monkeypatch):
    monkeypatch.setattr(min_bar, "resolve_vipdoc", lambda value: Path("root"))
    monkeypatch.setattr(min_bar, "market_to_exchange", lambda value: "sz")
    leaf = "sz" + "000001" + "." + suffix
    assert function(0, "000001") == Path("root") / "sz" / folder / leaf


def test_industry_tests_truth_once_before_decode():
    class Content:
        calls = 0

        def __bool__(self):
            self.calls += 1
            if self.calls > 1:
                raise AssertionError("source truth must not be evaluated again")
            return True

        def decode(self, encoding, *, errors):
            return "0|abcdef|A"

    content = Content()
    assert industry.parse_tdxhy_cfg(content) == {"abcdef": ("A", "")}
    assert content.calls == 1


def test_daily_reader_keeps_invalid_raw_date_tail_and_runtime_coefficients(tmp_path, monkeypatch):
    path = tmp_path / "custom.day"
    raw = struct.pack("<IIIIIfII", 20261399, 10, 20, 30, 40, -0.0, 50, 0xFFFFFFFF)
    path.write_bytes(raw + b"tail")
    monkeypatch.setattr(daily_bar, "security_coefficients", lambda name: (2.0, 3.0))
    rows = daily_bar.read_daily_bar(path)
    assert len(rows) == 1
    row = rows[0]
    assert (row.year, row.month, row.day) == (2026, 13, 99)
    assert (row.open, row.high, row.low, row.close, row.volume) == (20, 40, 60, 80, 150)
    assert math.copysign(1.0, row.amount) == -1
    assert row.raw == raw
    assert row.adjust_flag == 0xFFFFFFFF


def test_daily_short_file_does_not_lookup_coefficients(tmp_path, monkeypatch):
    path = tmp_path / "short.day"
    path.write_bytes(b"x" * 31)

    def unexpected(name):
        raise AssertionError("short file returns before coefficient selection")

    monkeypatch.setattr(daily_bar, "security_coefficients", unexpected)
    assert daily_bar.read_daily_bar(path) == []


@pytest.mark.parametrize("integer", [False, True])
def test_minute_reader_keeps_invalid_packed_time_raw_bits_and_tail(integer):
    if integer:
        raw = struct.pack("<HHIIIIfII", 0, 1501, 101, 102, 103, 104, math.inf, 2**32-1, 11)
        fmt = min_bar._MIN_FMT
    else:
        raw = struct.pack("<HHfffffII", 0, 1501, 1.0, 2.0, 3.0, 4.0, math.inf, 2**32-1, 11)
        fmt = min_bar._LC_MIN_FMT
    rows = min_bar._read_bars(raw + b"tail", fmt, integer_prices=integer)
    assert len(rows) == 1
    row = rows[0]
    assert (row.year, row.month, row.day, row.hour, row.minute) == (2004, 0, 0, 25, 1)
    assert row.open == (1.01 if integer else 1.0)
    assert row.volume == float(2**32-1)
    assert row.raw == raw
    assert row.amount == math.inf


def test_minute_reader_keeps_decoder_and_model_hooks(monkeypatch):
    raw = struct.pack("<HHfffffII", 1, 2, 1, 2, 3, 4, 5, 6, 7)
    monkeypatch.setattr(min_bar, "_decode_tdx_date", lambda value: (9, 8, 7))
    monkeypatch.setattr(min_bar, "_decode_tdx_time", lambda value: (6, 5))
    monkeypatch.setattr(min_bar, "MinBar", lambda **kw: kw)
    rows = min_bar._read_bars(raw, min_bar._LC_MIN_FMT, integer_prices=False)
    assert (rows[0]["year"], rows[0]["month"], rows[0]["day"], rows[0]["hour"], rows[0]["minute"]) == (9, 8, 7, 6, 5)


def test_ex_reader_retains_integer_amount_and_float_view(tmp_path):
    path = tmp_path / "ex.day"
    raw = struct.pack("<IffffIIf", 0, 1, 2, 3, 4, 0x80000000, 2**32-1, math.inf)
    path.write_bytes(raw + b"short")
    row, = ex_daily_bar.read_ex_daily_bar(path)
    assert (row.year, row.month, row.day) == (0, 0, 0)
    assert row.amount == 0x80000000
    assert math.copysign(1.0, row.hk_stock_amount) == -1
    assert row.volume == 2**32-1
    assert row.settlement == math.inf
    assert row.raw == raw


def test_daily_encoder_keeps_python_rounding_and_struct_errors():
    from eltdx.offline.models import DailyBar

    bar = DailyBar(2026, 13, 99, 2.5, 3.5, 4.5, 5.5, 6.5, -0.0, 7)
    assert daily_bar.encode_daily_bar(bar, 1.0, 1.0) == struct.pack("<IIIIIfII", 20261399, 2, 4, 4, 6, -0.0, 6, 7)
    with pytest.raises(ZeroDivisionError):
        daily_bar.encode_daily_bar(bar, 0.0, 1.0)
    negative = DailyBar(2026, 1, 1, -1, 1, 1, 1, 1, 1)
    with pytest.raises(struct.error):
        daily_bar.encode_daily_bar(negative, 1.0, 1.0)


def test_minute_encoders_keep_custom_date_time_round_and_pack(monkeypatch):
    events = []

    def date(year, month, day):
        events.append(("date", year, month, day))
        return 11

    def time(hour, minute):
        events.append(("time", hour, minute))
        return 12

    def pack(*fields):
        events.append(("pack", fields))
        return fields

    monkeypatch.setattr(min_bar, "_encode_tdx_date", date)
    monkeypatch.setattr(min_bar, "_encode_tdx_time", time)
    monkeypatch.setattr(min_bar, "_MIN_FMT", SimpleNamespace(pack=pack))
    bar = SimpleNamespace(year=1, month=2, day=3, hour=4, minute=5,
                          open=0.025, high=0.035, low=0.045, close=0.055,
                          volume=6.5, amount=7, reserved=8)
    assert min_bar.encode_5min_bar(bar) == (11, 12, 2, 4, 4, 6, 7, 6, 8)
    assert events[:2] == [("date", 1, 2, 3), ("time", 4, 5)]


def test_ex_encoder_keeps_amount_slot_independent_of_float_view():
    from eltdx.offline.models import ExDailyBar

    bar = ExDailyBar(2026, 13, 99, 1, 2, 3, 4, 0x7FC12345, 9, -0.0, hk_stock_amount=42)
    assert ex_daily_bar.encode_ex_daily_bar(bar) == struct.pack("<IffffIIf", 20261399, 1, 2, 3, 4, 0x7FC12345, 9, -0.0)


def test_daily_custom_struct_reader_stays_adapter(tmp_path, monkeypatch):
    path = tmp_path / "custom.day"
    path.write_bytes(b"x")
    calls = []

    def unpack(raw):
        calls.append(raw)
        return (20260101, 1, 2, 3, 4, 5, 6, 7)

    monkeypatch.setattr(daily_bar, "_DAILY_FMT", SimpleNamespace(size=1, unpack=unpack))
    monkeypatch.setattr(daily_bar, "security_coefficients", lambda name: (1, 1))
    row, = daily_bar.read_daily_bar(path)
    assert row.open == 1
    assert row.raw == b"x"
    assert calls == [b"x"]


def test_bar_model_lookup_occurs_before_custom_price_arithmetic(tmp_path, monkeypatch):
    path = tmp_path / "custom.day"
    path.write_bytes(struct.pack("<IIIIIfII", 20260101, 1, 2, 3, 4, 5, 6, 7))
    calls = []

    def original(**values):
        calls.append("original")
        return values

    class Coefficient:
        def __rmul__(self, value):
            monkeypatch.setattr(daily_bar, "DailyBar", lambda **values: calls.append("changed") or values)
            return value

    monkeypatch.setattr(daily_bar, "DailyBar", original)
    monkeypatch.setattr(daily_bar, "security_coefficients", lambda name: (Coefficient(), 1))
    assert len(daily_bar.read_daily_bar(path)) == 1
    assert calls == ["original"]


def test_minute_reader_uses_live_float_constructor(monkeypatch):
    raw = struct.pack("<HHfffffII", 1, 2, 1, 2, 3, 4, 5, 6, 7)
    marker = object()
    monkeypatch.setattr(min_bar, "float", lambda value: marker, raising=False)
    row, = min_bar._read_bars(raw, min_bar._LC_MIN_FMT, integer_prices=False)
    assert row.volume is marker


def test_append_filters_against_original_last_key_and_keeps_input_order(tmp_path):
    from eltdx.offline.models import DailyBar

    def bar(day):
        return DailyBar(2026, 1, day, 1, 1, 1, 1, 1, 1)

    path = tmp_path / "daily.day"
    assert write_daily.append_daily_bar(path, [bar(5)], 1, 1) == 1
    assert write_daily.append_daily_bar(path, [bar(7), bar(6), bar(7), bar(4)], 1, 1) == 3
    assert [value.day for value in daily_bar.read_daily_bar(path)] == [5, 7, 6, 7]


def test_append_alignment_precedes_last_key_and_empty_input(tmp_path, monkeypatch):
    path = tmp_path / "tail.day"
    path.write_bytes(b"x")

    def unexpected(path):
        raise AssertionError("last key must not be read for a corrupt file")

    monkeypatch.setattr(write_daily, "get_last_bar_date", unexpected)
    with pytest.raises(alignment.ResourceFormatError, match="残尾"):
        write_daily.append_daily_bar(path, [], 1, 1)
    assert path.read_bytes() == b"x"


def test_append_keeps_custom_encoder_last_key_and_sync_hooks(tmp_path, monkeypatch):
    path = tmp_path / "custom.day"
    events = []
    monkeypatch.setattr(write_daily, "get_last_bar_date", lambda path: 5)
    monkeypatch.setattr(write_daily, "encode_daily_bar", lambda bar, p, v: events.append((bar.date_int, p, v)) or b"x" * 32)
    monkeypatch.setattr(write_daily.os, "fsync", lambda descriptor: events.append("sync"))
    rows = [SimpleNamespace(date_int=4), SimpleNamespace(date_int=6), SimpleNamespace(date_int=6)]
    assert write_daily.append_daily_bar(path, rows, 2, 3) == 2
    assert events == [(6, 2, 3), (6, 2, 3), "sync"]
    assert path.read_bytes() == b"x" * 64


def test_last_daily_record_aligns_tail_but_returns_raw_invalid_date(tmp_path):
    path = tmp_path / "daily.day"
    path.write_bytes(struct.pack("<IIIIIfII", 20269999, 1, 2, 3, 4, 5, 6, 7) + b"tail")
    assert write_daily.get_last_bar_date(path) == 20269999
    assert write_ex_daily.get_last_ex_bar_date(path) == 20269999


def test_alignment_exact_error_and_nonpositive_size(tmp_path):
    path = tmp_path / "tail.bin"
    path.write_bytes(b"x" * 35)
    with pytest.raises(alignment.ResourceFormatError) as caught:
        alignment.ensure_aligned_for_append(path, 32, "日线")
    assert str(caught.value) == (
        f"日线文件存在残尾记录，拒绝追加: {path} "
        "(大小 35 字节，模 32 余 3 字节)。"
        "追加会让此后每条记录错位且不报错。请先把文件截断到 32 字节"
        "（即最近一条完整记录的结尾）再写入。"
    )
    with pytest.raises(ValueError, match="record_size must be positive, got 0"):
        alignment.ensure_aligned_for_append(object(), 0, "ignored")


def test_minute_append_opens_before_custom_encoding_but_daily_does_not(tmp_path, monkeypatch):
    minute_path = tmp_path / "minute.lc1"
    daily_path = tmp_path / "daily.day"
    failure = RuntimeError("custom encoder failed")

    def fail(*args):
        raise failure

    monkeypatch.setattr(write_min_bar, "encode_min_bar", fail)
    monkeypatch.setattr(write_daily, "encode_daily_bar", fail)
    with pytest.raises(RuntimeError) as caught:
        write_min_bar.append_min_bar(minute_path, [object()])
    assert caught.value is failure
    assert minute_path.exists()
    with pytest.raises(RuntimeError) as caught:
        write_daily.append_daily_bar(daily_path, [object()], 1, 1)
    assert caught.value is failure
    assert not daily_path.exists()


def test_minute_last_key_ignores_tail_and_retains_decoder_hooks(tmp_path, monkeypatch):
    path = tmp_path / "minutes.lc1"
    path.write_bytes(struct.pack("<HHfffffII", 0, 1501, 1, 2, 3, 4, 5, 6, 7) + b"tail")
    assert write_min_bar.get_last_min_bar_datetime(path) == (2004, 0, 0, 25, 1)
    monkeypatch.setattr(write_min_bar, "_decode_tdx_date", lambda value: (3, 2, 1))
    monkeypatch.setattr(write_min_bar, "_decode_tdx_time", lambda value: (4, 5))
    assert write_min_bar.get_last_min_bar_datetime(path) == (3, 2, 1, 4, 5)


def test_append_dynamic_encoder_lookup_and_error_before_daily_open(tmp_path, monkeypatch):
    path = tmp_path / "daily.day"
    calls = []

    def second(bar, price, volume):
        calls.append("second")
        return b"b" * 32

    def first(bar, price, volume):
        calls.append("first")
        monkeypatch.setattr(write_daily, "encode_daily_bar", second)
        return b"a" * 32

    monkeypatch.setattr(write_daily, "encode_daily_bar", first)
    monkeypatch.setattr(write_daily.os, "fsync", lambda descriptor: None)
    assert write_daily.append_daily_bar(path, [object(), object()], 1, 1) == 2
    assert calls == ["first", "second"]
    assert path.read_bytes() == b"a" * 32 + b"b" * 32


def test_all_filtered_rows_skip_encoder_and_file_open(tmp_path, monkeypatch):
    path = tmp_path / "absent.day"
    monkeypatch.setattr(write_daily, "get_last_bar_date", lambda path: 10)

    def unexpected(*args):
        raise AssertionError("filtered rows must not be encoded")

    monkeypatch.setattr(write_daily, "encode_daily_bar", unexpected)
    assert write_daily.append_daily_bar(path, [SimpleNamespace(date_int=9)], 1, 1) == 0
    assert not path.exists()
