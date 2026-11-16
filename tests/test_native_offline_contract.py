"""Offline compatibility source cases; execution waits for the full source gate."""
from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace

import pytest

from eltdx.offline import daily_bar, industry, paths
from eltdx.offline import min_bar


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
