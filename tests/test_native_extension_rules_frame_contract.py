"""Legacy rules and framing contracts authored without executing verification."""
from __future__ import annotations

from datetime import datetime
import math
import struct
import sys
from types import SimpleNamespace
import zlib

import pytest

from eltdx.exceptions import ProtocolError
from eltdx.ex import frame as ex
from eltdx.mac import codec as mac, commands, price_rules as rules
from eltdx.protocol import decompress as limited


@pytest.mark.parametrize("market,code,name,days,expected", [
    (0, "300001", "ST创业板", None, (12.0, 8.0)),
    (0, "301001", "ST创业板", None, (12.0, 8.0)),
    (0, "302001", "ST创业板", None, (12.0, 8.0)),
    (0, "303001", "普通", None, (11.0, 9.0)),
    (1, "688001", "*ST科创", None, (12.0, 8.0)),
    (2, "920001", "ST北交", 2, (13.0, 7.0)),
    (1, "600001", "*ST主板", None, (10.5, 9.5)),
    (1, "600001", "普通", 5, (None, None)),
    (1, "600001", "普通", 6, (11.0, 9.0)),
    (2, "430001", "普通", 1, (None, None)),
    (2, "830001", "普通", 0, (13.0, 7.0)),
    (2, "870001", "普通", -1, (13.0, 7.0)),
    (1, "000001", "上证", None, (None, None)),
    (0, "399001", "深证", None, (None, None)),
    (1, "600001", "行业指数", None, (None, None)),
    (0, "000001", "概念板块", None, (None, None)),
])
def test_native_mac_price_rules_keep_board_priority_windows_and_legacy_rounding(market, code, name, days, expected):
    assert rules.compute_price_limits(market, code, name, 10.0, days) == expected


def test_native_mac_price_rules_keep_live_market_constants_and_chinext_set(monkeypatch):
    monkeypatch.setattr(rules, "_MARKET_SH", 99)
    assert rules._is_index_like(99, "000001", "ordinary")
    assert not rules._is_index_like(1, "000001", "ordinary")
    monkeypatch.setattr(rules, "_CHINEXT_CODES", frozenset({"303"}))
    assert rules.compute_price_limits(0, "303001", "ST", 10.0) == (12.0, 8.0)
    assert rules.compute_price_limits(0, "300001", "ST", 10.0) == (10.5, 9.5)


def test_native_mac_rule_early_returns_and_original_custom_error_identity(monkeypatch):
    class Name:
        def upper(self):
            pytest.fail("nonpositive reference returns before name.upper")

    assert rules.compute_price_limits(object(), object(), Name(), 0) == (None, None)
    events = []
    original_window = rules.get_no_limit_window_days
    monkeypatch.setattr(rules, "_is_index_like", lambda *args: events.append("index") or True)
    monkeypatch.setattr(rules, "get_no_limit_window_days", lambda *args: pytest.fail("index early return"))

    class UpperName:
        def upper(self):
            events.append("upper")
            return "ordinary"

    assert rules.compute_price_limits(0, "000001", UpperName(), 10.0) == (None, None)
    assert events == ["upper", "index"]
    monkeypatch.setattr(rules, "get_no_limit_window_days", original_window)
    failure = RuntimeError("index hook")

    def fail(*args):
        raise failure

    monkeypatch.setattr(rules, "_is_index_like", fail)
    with pytest.raises(RuntimeError) as caught:
        rules.get_no_limit_window_days(0, "000001", "name")
    assert caught.value is failure


def test_native_mac_listed_days_chained_comparison_is_lazy_and_not_coerced(monkeypatch):
    events = []
    window = object()
    monkeypatch.setattr(rules, "_is_index_like", lambda *args: False)
    monkeypatch.setattr(rules, "get_no_limit_window_days", lambda *args: events.append("window") or window)

    class Days:
        def __gt__(self, zero):
            events.append(("positive", zero))
            return True

        def __le__(self, actual_window):
            events.append(("within", actual_window))
            return True

    assert rules.compute_price_limits(0, object(), "name", 10.0, Days()) == (None, None)
    assert events == ["window", ("positive", 0), ("within", window)]


def test_native_mac_rules_keep_intrinsic_prefix_slice_and_live_round_hooks(monkeypatch):
    calls = []

    class Code(str):
        def __getitem__(self, key):
            calls.append((key.start, key.stop, key.step))
            return super().__getitem__(key)

    monkeypatch.setattr(rules, "round", lambda value, digits: calls.append((value, digits)) or value, raising=False)
    result = rules.compute_price_limits(0, Code("303001"), "name", 10.0)
    assert calls[0] == (None, 3, None)
    assert calls[1:] == [(11.00001, 2), (9.00001, 2)]
    assert result == (11.00001, 9.00001)


def test_native_mac_rules_do_not_add_nonfinite_filtering():
    result = rules.compute_price_limits(0, "303001", "name", float("nan"))
    assert all(math.isnan(value) for value in result)
    assert rules.compute_price_limits(0, "303001", "name", float("inf")) == (float("inf"), float("inf"))


@pytest.mark.parametrize("market,code,name,expected", [
    (1, "600001", "name", 5), (0, "000001", "name", 5),
    (0, "300001", "name", 5), (1, "688001", "name", 5),
    (2, "920001", "name", 1), (0, "920001", "name", 1),
    (1, "000001", "name", 0), (0, "395001", "name", 0),
    (0, "000001", "指数", 0), (5, "100000", "name", 0),
])
def test_native_mac_no_limit_window_preserves_source_market_and_code_rules(market, code, name, expected):
    assert rules.get_no_limit_window_days(market, code, name) == expected


def test_native_mac_combine_datetime_retains_seconds_and_lazy_intraday_clock():
    assert commands._combine_bar_datetime(20261117, 34201, True) == datetime(2026, 11, 17, 9, 30, 1)
    assert commands._combine_bar_datetime(20261117, 34202, True) == datetime(2026, 11, 17, 9, 30, 2)
    assert commands._combine_bar_datetime(20261117, 0, True) == datetime(2026, 11, 17)

    class Seconds:
        def __bool__(self):
            pytest.fail("daily bar never evaluates clock")

    assert commands._combine_bar_datetime(20261117, Seconds(), False) == datetime(2026, 11, 17)
    with pytest.raises(ValueError):
        commands._combine_bar_datetime(20261117, 86400, True)
    with pytest.raises(ValueError):
        commands._combine_bar_datetime(20261399, 0, False)


def test_native_mac_combine_datetime_keeps_custom_year_and_live_constructor(monkeypatch):
    marker = object()
    calls = []
    raw = 10**100 + 20261117
    monkeypatch.setattr(commands, "datetime", lambda *args: calls.append(args) or marker)
    assert commands._combine_bar_datetime(raw, 34201, True) is marker
    assert calls == [(raw // 10000, (raw % 10000) // 100, raw % 100, 9, 30, 1)]


def test_native_mac_decode_gbk_keeps_ignore_and_all_nul_removal_and_custom_hooks():
    assert commands._decode_gbk(b"\0A\0\xffB\0") == "AB"
    marker = object()
    calls = []

    class Decoded:
        def replace(self, old, new):
            calls.append((old, new))
            return marker

    class Raw:
        def decode(self, encoding, **kwargs):
            calls.append((encoding, kwargs))
            return Decoded()

    assert commands._decode_gbk(Raw()) is marker
    assert calls == [("gbk", {"errors": "ignore"}), ("\0", "")]


def test_native_mac_frame_build_defaults_and_override_and_original_struct_errors():
    inner = struct.pack("<H", 0x122B) + b"abc"
    assert mac.build_mac_request(0x122B, b"abc") == struct.pack("<BIBHH", 0x1C, 0, 1, 5, 5) + inner
    assert mac.build_mac_request(0x122B, b"abc", head_flag=1) == struct.pack("<BIBHH", 1, 0, 1, 5, 5) + inner
    with pytest.raises(struct.error):
        mac.build_mac_request(0x10000, b"")
    with pytest.raises(struct.error):
        mac.build_mac_request(1, b"x" * 65534)


def test_native_mac_frame_build_keeps_live_format_constants_and_two_lengths(monkeypatch):
    sizes = iter((7, 9))
    calls = []
    monkeypatch.setattr(mac, "_MAC_CUSTOMIZE", 37)
    monkeypatch.setattr(mac, "_MAC_VERSION", 8)
    monkeypatch.setattr(mac, "len", lambda value: calls.append(value) or next(sizes), raising=False)
    wire = mac.build_mac_request(2, b"abc", head_flag=5)
    assert wire == struct.pack("<BIBHH", 5, 37, 8, 7, 9) + b"\x02\0abc"
    assert calls == [b"\x02\0abc"] * 2


def test_native_ex_header_encode_parse_unknown_magic_and_dataclass_contract():
    wire = ex.encode_header(seq_id=7, method=0x1234, zipsize=3)
    assert wire == struct.pack("<IIIHH", 7654321, 7, 0x1234, 3, 3)
    header = ex.parse_header(struct.pack("<IIIHH", 0xFFFFFFFF, 7, 0x1234, 3, 9) + b"tail")
    assert header == ex.FrameHeader(0xFFFFFFFF, 7, 0x1234, 3, 9)
    assert mac.parse_mac_header(wire) == mac.MacFrameHeader(7654321, 7, 0x1234, 3, 3)
    with pytest.raises(ProtocolError, match="响应帧头长度不足: 需要 16，实际 15"):
        ex.parse_header(wire[:15])
    with pytest.raises(ProtocolError, match="mac frame header"):
        mac.parse_mac_header(wire[:15])


def test_native_ex_header_short_error_keeps_two_length_calls_and_live_header_size(monkeypatch):
    calls = []
    sizes = iter((1, 3))
    monkeypatch.setattr(ex, "HEADER_SIZE", 23)
    monkeypatch.setattr(ex, "len", lambda data: calls.append(data) or next(sizes), raising=False)
    with pytest.raises(ProtocolError) as caught:
        ex.parse_header(b"x")
    assert str(caught.value) == "响应帧头长度不足: 需要 23，实际 3"
    assert calls == [b"x", b"x"]


def test_native_ex_header_struct_error_keeps_active_context_and_constructor_outside_try(monkeypatch):
    source = struct.error("source unpack")
    outer = ProtocolError("outer")
    calls = []

    def fail(*args):
        raise source

    monkeypatch.setattr(ex, "struct", SimpleNamespace(unpack_from=fail, error=struct.error))
    monkeypatch.setattr(ex, "ProtocolError", lambda message: calls.append((message, sys.exc_info()[1])) or outer)
    with pytest.raises(ProtocolError) as caught:
        ex.parse_header(b"\0" * 16)
    assert caught.value is outer and outer.__cause__ is source and outer.__context__ is source
    assert calls == [("响应帧头解析失败: source unpack", source)]
    monkeypatch.setattr(ex, "struct", SimpleNamespace(unpack_from=lambda *args: (1, 2, 3, 4, 5), error=struct.error))

    def constructor(**values):
        raise source

    monkeypatch.setattr(ex, "FrameHeader", constructor)
    with pytest.raises(struct.error) as caught:
        ex.parse_header(b"\0" * 16)
    assert caught.value is source


@pytest.mark.parametrize("module,parse", [(ex, ex.parse_header), (mac, mac.parse_mac_header)])
def test_native_extension_header_model_and_unpack_hooks_keep_order_and_arity(monkeypatch, module, parse):
    marker = object()
    calls = []
    if module is ex:
        monkeypatch.setattr(module, "struct", SimpleNamespace(unpack_from=lambda *args: (1, 2, 3, 4, 5), error=struct.error))
        monkeypatch.setattr(module, "FrameHeader", lambda **kwargs: calls.append(kwargs) or marker)
        assert parse(b"\0" * 16) is marker
        assert calls == [dict(magic=1, seq_id=2, method=3, zipsize=4, unzipsize=5)]
        monkeypatch.setattr(module, "struct", SimpleNamespace(unpack_from=lambda *args: (1, 2), error=struct.error))
    else:
        monkeypatch.setattr(module, "unpack_from", lambda *args: calls.append(args) or (1, 2, 3, 4, 5))
        monkeypatch.setattr(module, "MacFrameHeader", lambda *args: calls.append(args) or marker)
        assert parse(b"\0" * 16) is marker
        assert calls == [("<IIIHH", b"\0" * 16, 0, "mac frame header"), (1, 2, 3, 4, 5)]
        monkeypatch.setattr(module, "unpack_from", lambda *args: (1, 2))
    with pytest.raises(ValueError, match="not enough values to unpack \\(expected 5, got 2\\)"):
        parse(b"\0" * 16)


@pytest.mark.parametrize("module,method,header", [
    (ex, ex.decompress_body, ex.FrameHeader), (mac, mac.decompress_mac_body, mac.MacFrameHeader),
])
def test_native_extension_body_keeps_raw_identity_and_compressed_output(module, method, header):
    raw = bytearray(b"abc")
    assert method(header(0, 0, 0, 3, 3), raw) is raw
    compressed = zlib.compress(b"a" * 100)
    assert method(header(0, 0, 0, len(compressed), 100), compressed) == b"a" * 100
    with pytest.raises(ProtocolError, match="body 长度不符"):
        method(header(0, 0, 0, 4, 3), b"abc")
    with pytest.raises(ProtocolError, match="解压长度不符"):
        method(header(0, 0, 0, len(compressed), 99), compressed)


@pytest.mark.parametrize("module,method", [(ex, ex.decompress_body), (mac, mac.decompress_mac_body)])
def test_native_extension_body_preserves_custom_decompressor_result_and_live_limits(monkeypatch, module, method):
    marker = object()
    calls = []
    cap = object()
    monkeypatch.setattr(module, "MAX_FRAME_UNZIPPED_BYTES", cap)
    monkeypatch.setattr(module, "decompress_limited", lambda *args: calls.append(args) or marker)
    monkeypatch.setattr(module, "len", lambda data: 7 if data is marker else 3, raising=False)
    assert method(SimpleNamespace(zipsize=3, unzipsize=7), b"abc") is marker
    assert calls == [(b"abc", cap)]


@pytest.mark.parametrize("module,method,prefix", [(ex, ex.decompress_body, "响应 body"),
                                                (mac, mac.decompress_mac_body, "mac frame body")])
@pytest.mark.parametrize("source,word", [(limited.DecompressLimitError("cap"), "解压超限"),
                                       (zlib.error("invalid stream"), "zlib 解压失败")])
def test_native_extension_decompression_exceptions_keep_cause_context_and_message(monkeypatch, module, method, prefix, source, word):
    outer = ProtocolError("outer")
    calls = []

    def fail(*args):
        raise source

    monkeypatch.setattr(module, "decompress_limited", fail)
    monkeypatch.setattr(module, "ProtocolError", lambda message: calls.append((message, sys.exc_info()[1])) or outer)
    with pytest.raises(ProtocolError) as caught:
        method(SimpleNamespace(zipsize=3, unzipsize=7), b"abc")
    assert caught.value is outer and outer.__cause__ is source and outer.__context__ is source
    assert calls == [(f"{prefix} {word}: {source}", source)]


def test_native_limited_zlib_matches_complete_cap_truncation_trailing_and_overflow_contracts():
    raw = b"abc" * 100
    wire = zlib.compress(raw)
    assert limited.decompress_limited(wire, len(raw)) == raw
    with pytest.raises(limited.DecompressLimitError, match="解压输出超过上限 299 字节"):
        limited.decompress_limited(wire, len(raw) - 1)
    with pytest.raises(limited.DecompressLimitError):
        limited.decompress_limited(wire + b"trailing", 65535)
    with pytest.raises(zlib.error, match="incomplete or truncated stream"):
        limited.decompress_limited(wire[:-4], 65535)


def test_native_limited_zlib_keeps_custom_decompressor_eof_and_original_output(monkeypatch):
    marker = object()
    cap = object()
    calls = []

    class Decoder:
        def decompress(self, data, maximum):
            calls.append((data, maximum))
            return marker

        @property
        def unconsumed_tail(self):
            calls.append("tail")
            return b""

        @property
        def unused_data(self):
            calls.append("unused")
            return b""

        @property
        def eof(self):
            calls.append("eof")
            return True

    monkeypatch.setattr(limited, "zlib", SimpleNamespace(decompressobj=Decoder, error=zlib.error))
    assert limited.decompress_limited(b"input", cap) is marker
    assert calls == [(b"input", cap), "tail", "unused", "eof"]


def test_native_limited_zlib_short_circuits_unused_and_eof_and_keeps_error_constructor(monkeypatch):
    original = limited.DecompressLimitError("custom")
    calls = []

    class Decoder:
        def decompress(self, data, maximum):
            return b"out"

        unconsumed_tail = b"remaining"

        @property
        def unused_data(self):
            pytest.fail("tail short-circuits unused")

        @property
        def eof(self):
            pytest.fail("tail short-circuits eof")

    monkeypatch.setattr(limited, "zlib", SimpleNamespace(decompressobj=Decoder, error=zlib.error))
    monkeypatch.setattr(limited, "DecompressLimitError", lambda message: calls.append(message) or original)
    with pytest.raises(Exception) as caught:
        limited.decompress_limited(b"input", 17)
    assert caught.value is original
    assert calls == ["解压输出超过上限 17 字节，已中止（声明长度不匹配或响应异常）"]
