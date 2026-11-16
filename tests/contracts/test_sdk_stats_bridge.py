"""Authored shared statistics decoder obligations; execution deferred."""
from __future__ import annotations

from io import BytesIO
from zipfile import ZIP_BZIP2, ZIP_LZMA, ZIP_STORED, ZipFile

import pytest

from eltdx import stats
from eltdx.exceptions import ResourceFormatError


def _line(width, *, market="1", code="600001", day="20260710", value="1.25"):
    fields = [value] * width
    fields[0], fields[1] = market, code
    fields[4 if width == 35 else 2] = day
    return "|".join(fields)


def _zip(stat, stat2, *, compression=ZIP_STORED):
    output = BytesIO()
    with ZipFile(output, "w", compression=compression) as archive:
        archive.writestr("tdxstat.cfg", stat)
        archive.writestr("tdxstat2.cfg", stat2)
    return output.getvalue()


def _legacy(payload, monkeypatch):
    with monkeypatch.context() as context:
        original = stats._decode_lines
        context.setattr(stats, "_decode_lines", lambda payload: original(payload))
        return stats.parse_tdx_stats_archive(payload)


def test_native_stats_ordinary_rows_match_original_models_and_source_path(monkeypatch):
    payload = _zip(_line(35).encode(), _line(21).encode())
    assert stats.parse_tdx_stats_archive(payload) == _legacy(payload, monkeypatch)
    result = stats.parse_tdx_stats_archive(payload, source_path="自定义来源" * 100)
    assert result.source_path == "自定义来源" * 100 and result.stats_date == "20260710"


@pytest.mark.parametrize("value", ["-3.75", "1_000.25", "１．２", "١٢.٥", "1e100", "nan", "-inf", ""])
def test_native_stats_scalar_conversion_matches_python_float_and_bigint(value, monkeypatch):
    payload = _zip(_line(35, market="-2.9", value=value).encode("gbk", errors="ignore"),
                   _line(21, market="1e100", value=value).encode("gbk", errors="ignore"))
    assert stats.parse_tdx_stats_archive(payload) == _legacy(payload, monkeypatch)


def test_native_stats_line_splitting_gbk_ignore_and_extra_columns_match_original(monkeypatch):
    first = _line(35, code="中文").encode("gbk")
    second = _line(35, code="600002").encode() + b"|extra|ignored"
    other = _line(21).encode()
    payload = _zip(b"short\r\n" + first + b"\x0b" + second + b"\n\xff", b"short\n" + other)
    assert stats.parse_tdx_stats_archive(payload) == _legacy(payload, monkeypatch)


def test_native_stats_duplicate_errors_keep_original_message_and_five_key_limit(monkeypatch):
    rows = b"\n".join(_line(35, code=str(code)).encode() for code in range(6) for _ in range(2))
    payload = _zip(rows, _line(21).encode())
    with pytest.raises(ResourceFormatError) as expected:
        _legacy(payload, monkeypatch)
    with pytest.raises(ResourceFormatError) as actual:
        stats.parse_tdx_stats_archive(payload)
    assert str(actual.value) == str(expected.value)


def test_native_stats_empty_table_error_precedes_duplicate_errors(monkeypatch):
    row = _line(35).encode()
    payload = _zip(row + b"\n" + row, b"short")
    for call in (lambda: _legacy(payload, monkeypatch), lambda: stats.parse_tdx_stats_archive(payload)):
        with pytest.raises(ResourceFormatError, match="contains no usable rows"):
            call()


@pytest.mark.parametrize("compression", [ZIP_BZIP2, ZIP_LZMA])
def test_native_stats_keeps_python_zip_codec_compatibility(compression, monkeypatch):
    payload = _zip(_line(35).encode(), _line(21).encode(), compression=compression)
    assert stats.parse_tdx_stats_archive(payload) == _legacy(payload, monkeypatch)


def test_native_stats_custom_scalar_parser_keeps_original_adapter(monkeypatch):
    calls = []
    monkeypatch.setattr(stats, "_float_value", lambda text: calls.append(text) or 3.0)
    result = stats.parse_tdx_stats_archive(_zip(_line(35).encode(), _line(21).encode()))
    assert result.stat[(3, "600001")].beta_60d == 3.0
    assert calls


def test_native_stats_invalid_archive_keeps_python_zip_error_cause(monkeypatch):
    with pytest.raises(ResourceFormatError) as expected:
        _legacy(b"invalid zip", monkeypatch)
    with pytest.raises(ResourceFormatError) as actual:
        stats.parse_tdx_stats_archive(b"invalid zip")
    assert str(actual.value) == str(expected.value)
    assert type(actual.value.__cause__) is type(expected.value.__cause__)
