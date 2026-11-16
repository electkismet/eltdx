"""Authored compatibility contracts; deferred until the full source gate."""
from __future__ import annotations

import pytest
import math
import struct

from eltdx.offline import history_financial as financial


def test_file_list_keeps_unicode_signed_arbitrary_precision_and_raw_fields():
    data = (
        "  first.zip , raw hash , -١_٢  ,ignored\n"
        "large.zip,hash,184467440737095516160000\n"
        "zero.zip,hash,-0\n"
        "skip.zip,hash,1__2\n"
        "missing,field\n"
    ).encode()
    assert financial.parse_financial_file_list(data) == [
        ("first.zip ", " raw hash ", -12),
        ("large.zip", "hash", 184467440737095516160000),
        ("zero.zip", "hash", 0),
    ]


def test_file_list_keeps_replacement_and_python_whitespace():
    assert financial.parse_financial_file_list(b"\x1c\xff.zip,hash,2\x1f\n") == [("\ufffd.zip", "hash", 2)]
    assert financial.parse_financial_file_list(b"") == []


def test_file_list_custom_decode_object_keeps_python_adapter():
    class Source:
        def decode(self, encoding, *, errors):
            assert (encoding, errors) == ("utf-8", "replace")
            return "custom.zip,hash,9"

    assert financial.parse_financial_file_list(Source()) == [("custom.zip", "hash", 9)]


def test_file_list_uses_runtime_integer_and_only_skips_value_error(monkeypatch):
    calls = []

    def integer(value):
        calls.append(value)
        if value == "skip":
            raise ValueError("ignored")
        if value == "stop":
            raise TypeError("original integer failure")
        return 27

    monkeypatch.setattr(financial, "int", integer, raising=False)
    assert financial.parse_financial_file_list(b"a,h,skip\nb,h,ok") == [("b", "h", 27)]
    assert calls == ["skip", "ok"]
    with pytest.raises(TypeError, match="original integer failure"):
        financial.parse_financial_file_list(b"c,h,stop")


def _dat(*, size=4, date=20260331, entries=None, payload=None):
    if entries is None:
        entries = [(b"600000", 255, 31)]
    if payload is None:
        payload = struct.pack("<f", 1.5)
    return (
        struct.pack("<hIHIII", -7, date, len(entries), 0, size, 0)
        + b"".join(struct.pack("<6sBI", *row) for row in entries)
        + payload
    )


@pytest.mark.parametrize("date", [-1, 2**100, None, "20260331"])
def test_dat_preserves_arbitrary_report_date_object(date):
    rows = financial.parse_financial_dat(_dat(), date)
    assert rows == [("600000", 255, date, [1.5])]
    assert rows[0][2] is date


def test_dat_date_comparison_precedes_zero_size_and_field_limit(monkeypatch):
    events = []

    class Date:
        def __eq__(self, other):
            events.append(("date", other))
            monkeypatch.setattr(financial, "MAX_HISTORY_REPORT_FIELDS", -1)
            return False

    date = Date()
    assert financial.parse_financial_dat(_dat(size=0), date) == []
    assert events == [("date", 0)]
    with pytest.raises(financial.ResourceFormatError) as caught:
        financial.parse_financial_dat(_dat(), date)
    assert str(caught.value) == "财报记录字段数超出上限: 1 > -1（report_size=4）"


def test_dat_short_header_does_not_compare_date_or_load_extension(monkeypatch):
    from eltdx import _native_abi

    def unexpected(*args):
        raise AssertionError("short input must return before native/date work")

    class Date:
        __eq__ = unexpected

    monkeypatch.setattr(_native_abi, "load_native", unexpected)
    assert financial.parse_financial_dat(b"\x00" * 19, Date()) == []


def test_dat_exact_alignment_and_runtime_total_limit_errors(monkeypatch):
    with pytest.raises(financial.ResourceFormatError) as caught:
        financial.parse_financial_dat(_dat(size=6))
    assert str(caught.value) == "财报记录长度非法: report_size=6（必须是 4 的整数倍）"
    monkeypatch.setattr(financial, "MAX_HISTORY_TOTAL_FLOATS", 1)
    data = _dat(entries=[(b"600000", 1, 42), (b"000001", 0, 46)], payload=struct.pack("<2f", 1, 2))
    with pytest.raises(financial.ResourceFormatError) as caught:
        financial.parse_financial_dat(data)
    assert str(caught.value) == "财报总解析量超出上限: 2 > 1 个字段（已解析 2/2 条，report_size=4）"


def test_dat_duplicate_accepted_offset_and_repeated_invalid_offsets():
    data = _dat(entries=[(b"600000", 1, 42), (b"000001", 0, 42)])
    with pytest.raises(financial.ResourceFormatError) as caught:
        financial.parse_financial_dat(data)
    assert str(caught.value) == "财报索引偏移重复: offset=42（第 1 条，code=000001）"
    invalid = _dat(entries=[(b"600000", 1, 2**32-1), (b"000001", 0, 2**32-1)])
    assert financial.parse_financial_dat(invalid) == []


def test_dat_unique_overlap_ascii_replacement_and_float_projection():
    data = _dat(entries=[(b"A\xffB", 255, 42), (b"000001", 0, 43)], payload=b"\x00" * 5)
    assert financial.parse_financial_dat(data) == [
        ("A\ufffdB", 255, 20260331, [0.0]), ("000001", 0, 20260331, [0.0])
    ]
    values = financial.parse_financial_dat(_dat(size=16, payload=struct.pack("<4I", 0x80000000, 0x7F800000, 0xFF800000, 0x7FC12345)))[0][3]
    assert math.copysign(1.0, values[0]) == -1
    assert values[1:3] == [math.inf, -math.inf]
    assert math.isnan(values[3])


def test_dat_custom_buffers_and_struct_override_keep_adapter(monkeypatch):
    data = _dat()
    assert financial.parse_financial_dat(bytearray(data)) == [("600000", 255, 20260331, [1.5])]
    calls = []
    unpack = struct.unpack

    def custom_unpack(fmt, value):
        calls.append(fmt)
        return unpack(fmt, value)

    monkeypatch.setattr(financial.struct, "unpack", custom_unpack)
    assert financial.parse_financial_dat(data) == [("600000", 255, 20260331, [1.5])]
    assert calls == ["<1hI1H3L", "<6s1c1L", "<1f"]


def test_dat_streaming_limit_keeps_runtime_comparisons_and_exception_identity(monkeypatch):
    calls = []
    failure = RuntimeError("custom limit comparison")

    class Total:
        def __lt__(self, total):
            calls.append(total)
            if total == 2:
                raise failure
            return False

    monkeypatch.setattr(financial, "MAX_HISTORY_TOTAL_FLOATS", Total())
    data = _dat(entries=[(b"600000", 1, 42), (b"000001", 0, 46)], payload=struct.pack("<2f", 1, 2))
    with pytest.raises(RuntimeError) as caught:
        financial.parse_financial_dat(data)
    assert caught.value is failure
    assert calls == [1, 2]


def test_dat_keeps_incomplete_index_and_short_data_skips():
    # One accepted row, one out-of-range row and a final incomplete index.
    data = bytearray(_dat(entries=[(b"600000", 1, 20), (b"000001", 0, 500)]))
    data[6:8] = struct.pack("<H", 3)
    rows = financial.parse_financial_dat(bytes(data))
    assert len(rows) == 1
    assert rows[0][:3] == ("600000", 1, 20260331)
