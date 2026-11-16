"""Authored compatibility contracts; deferred until the full source gate."""
from __future__ import annotations

import pytest

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
