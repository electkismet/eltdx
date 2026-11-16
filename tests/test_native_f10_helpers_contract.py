"""Native helper compatibility sources; execution waits for the full source gate."""
from types import SimpleNamespace

import pytest

from eltdx.helpers import core


def helper_with(f10):
    client = SimpleNamespace(f10=f10)
    helper = object.__new__(core.HelperApi)
    helper._client_ref = lambda: client
    helper._index_cache = {}
    return helper


@pytest.mark.parametrize("method,source", [
    ("index_members", "index_members"),
    ("index_etfs", "index_tracking_products"),
    ("board_members", "board_members"),
])
def test_f10_helper_cache_normalizes_key_but_passes_original_code(method, source):
    calls = []
    first, refreshed = object(), object()
    results = iter([first, refreshed])

    def fetch(code):
        calls.append(code)
        return next(results)

    helper = helper_with(SimpleNamespace(**{source: fetch}))
    original = " SH000300 "
    assert getattr(helper, method)(original) is first
    assert getattr(helper, method)("sh000300") is first
    assert getattr(helper, method)(original, refresh=True) is refreshed
    assert calls == [original, original]


def test_index_entry_looks_up_live_cache_override_and_source():
    marker = object()
    getter = object()
    helper = helper_with(SimpleNamespace(index_members=getter))
    calls = []
    helper._index_cached = lambda *args: calls.append(args) or marker
    assert helper.index_members("000300", refresh=True) is marker
    assert calls == [("members", "000300", True, getter)]


def test_index_cache_uses_builtin_str_and_accepts_live_global_override(monkeypatch):
    calls = []
    marker = object()
    helper = helper_with(SimpleNamespace(board_members=lambda code: calls.append(code) or marker))
    monkeypatch.setattr(core, "str", lambda code: " CUSTOM ", raising=False)
    assert helper.board_members(object()) is marker
    assert ("board_members", "custom") in helper._index_cache
    assert helper.board_members("another") is marker
    assert len(calls) == 1


def test_index_cache_none_is_a_miss_and_failed_refresh_keeps_old_entry():
    marker = object()
    calls = []

    def fetch(code):
        calls.append(code)
        if len(calls) == 1:
            return None
        if len(calls) == 2:
            return marker
        raise RuntimeError("source failed")

    helper = helper_with(SimpleNamespace(index_members=fetch))
    assert helper.index_members("000300") is None
    assert helper.index_members("000300") is marker
    with pytest.raises(RuntimeError, match="source failed"):
        helper.index_members("000300", refresh=True)
    assert helper.index_members("000300") is marker
    assert calls == ["000300", "000300", "000300"]


def test_ipo_cache_has_static_key_and_refresh_does_not_stringify(monkeypatch):
    calls = []
    marker = object()
    helper = helper_with(SimpleNamespace(ipo_calendar=lambda: calls.append("ipo") or marker))

    def forbidden(value):
        raise AssertionError("IPO does not convert a code")

    monkeypatch.setattr(core, "str", forbidden, raising=False)
    assert helper.ipo_calendar() is marker
    assert helper.ipo_calendar() is marker
    assert helper.ipo_calendar(refresh=True) is marker
    assert calls == ["ipo", "ipo"]
    assert helper._index_cache == {("ipo", ""): marker}


def test_f10_cache_hit_keeps_original_reference_and_avoids_source_lookup():
    marker = object()

    class F10:
        @property
        def board_members(self):
            raise AssertionError("cache hit must precede source lookup")

    helper = helper_with(F10())
    helper._index_cache[("board_members", "custom")] = marker
    assert helper.board_members(" CUSTOM ") is marker
