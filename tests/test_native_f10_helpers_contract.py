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


def test_topics_materialize_both_sources_before_merging_and_keep_first_order(monkeypatch):
    events = []
    marker = object()

    def rows(kind, values):
        for row in values:
            events.append(kind)
            yield row

    helper = helper_with(SimpleNamespace(
        topic_ids=lambda code: SimpleNamespace(rows=rows("ids", [
            {"t001": "1", "t002": "old", "marker": marker},
            {"t001": "2", "t002": "keep"},
            {"t001": "1", "t002": "last"},
        ])),
        hot_topics=lambda code: SimpleNamespace(rows=rows("hot", [
            {"id": 1, "ztmc": None, "gld": "3", "ztnr": "reason"},
            {"id": 3, "ztmc": "third", "sslb": marker},
        ])),
    ))
    original = core._pick

    def pick(*args):
        events.append("pick")
        return original(*args)

    monkeypatch.setattr(core, "_pick", pick)
    result = helper.stock_topics("000034")
    assert events[:5] == ["ids", "ids", "ids", "hot", "hot"]
    assert [row.topic_id for row in result.topics] == ["1", "2", "3"]
    first = result.topics[0]
    assert first.topic_name is None
    assert first.relation_level == 3.0
    assert first.source == "hot_topics+topic_ids"
    assert first.raw["topic_ids"] == {"t001": "1", "t002": "last"}
    assert result.topics[2].category_raw is marker
    assert result.topics[2].raw["hot_topics"]["sslb"] is marker


def test_topics_source_failure_propagates_before_any_merge(monkeypatch):
    def failed(code):
        raise RuntimeError("required hot source")

    helper = helper_with(SimpleNamespace(
        topic_ids=lambda code: SimpleNamespace(rows=[{"t001": "1"}]), hot_topics=failed))
    monkeypatch.setattr(core, "_pick", lambda *args: pytest.fail("merge started too early"))
    with pytest.raises(RuntimeError, match="required hot source"):
        helper.stock_topics("000034")


def test_topics_first_present_none_is_not_replaced_by_an_alias():
    helper = helper_with(SimpleNamespace(
        topic_ids=lambda code: SimpleNamespace(rows=[{"t001": None, "id": "wrong", "t002": "named"}]),
        hot_topics=lambda code: SimpleNamespace(),
    ))
    result = helper.stock_topics("000034")
    assert result.topics[0].topic_id is None
    assert result.topics[0].topic_name == "named"
    assert result.topics[0].source == "topic_ids"
    assert core._pick({"x": None, "y": 5}, "x", "y") is None


def test_topic_resolver_prioritizes_id_then_name_and_uses_first_only_without_selectors():
    first = SimpleNamespace(topic_id="1", topic_name="one")
    second = SimpleNamespace(topic_id="2", topic_name="two")
    helper = helper_with(SimpleNamespace())
    helper.stock_topics = lambda code: SimpleNamespace(topics=(first, second))
    assert helper._resolve_topic("seed", "1", "two") is first
    assert helper._resolve_topic("seed", "missing", "two") is second
    assert helper._resolve_topic("seed", None, None) is first
    with pytest.raises(ValueError, match="topic not found for seed: 'missing'"):
        helper._resolve_topic("seed", "missing", None)


def test_topic_stocks_explicit_id_skips_resolution_and_keeps_order_raw_and_options():
    calls = []
    marker = object()
    rows = [{"pm": "2", "zqdm": "300975", "sc": 0, "zqjc": "second", "zdf": "19.97", "extra": marker},
            {"pm": "1", "zqdm": "600000", "sc": 1, "zqjc": "first"}]

    def compare(*args, **kwargs):
        calls.append((args, kwargs))
        return SimpleNamespace(rows=rows)

    helper = helper_with(SimpleNamespace(topic_compare=compare))
    helper._resolve_topic = lambda *args: pytest.fail("unnecessary topic resolution")
    result = helper.topic_stocks("000034", topic_id=42, sort_by="custom", section="section")
    assert calls == [((core.normalize_code("000034"), "42"), {"section": "section", "sort_by": "custom"})]
    assert [row.rank for row in result.rows] == [2, 1]
    assert result.rows[0].full_code == "sz300975"
    assert result.rows[0].change_pct == 19.97
    assert result.rows[0].raw is not rows[0]
    assert result.rows[0].raw["extra"] is marker


def test_topic_stocks_live_resolver_keeps_or_fallback_and_original_name():
    calls = []
    helper = helper_with(SimpleNamespace(topic_compare=lambda *args, **kwargs: SimpleNamespace(rows=())))
    helper._resolve_topic = lambda *args: calls.append(args) or SimpleNamespace(topic_id="resolved", topic_name="fallback")
    result = helper.topic_stocks("000034", topic_id="", topic_name="provided")
    assert calls == [(core.normalize_code("000034"), "", "provided")]
    assert result.topic_id == "resolved"
    assert result.topic_name == "provided"


def test_topic_scalar_preserves_large_int_nonfinite_and_conversion_exception_boundaries():
    assert core._int(1.9) == 1
    assert core._int(True) == 1
    assert core._int(10 ** 100) == 10 ** 100
    assert core._float("inf") == float("inf")
    assert core._int("invalid") is None
    assert core._text(" \ud800 ") == "\ud800"

    class Exploding:
        def __int__(self):
            raise RuntimeError("conversion")

    with pytest.raises(RuntimeError, match="conversion"):
        core._int(Exploding())


def test_topic_projection_captures_converter_before_pick_can_replace_it(monkeypatch):
    seen = []
    original = core._pick

    def pick(row, *names):
        if names == ("sc", "market_id"):
            monkeypatch.setattr(core, "_int", lambda value: 1)
        return original(row, *names)

    def old_int(value):
        seen.append(value)
        return 0

    monkeypatch.setattr(core, "_int", old_int)
    monkeypatch.setattr(core, "_pick", pick)
    row = core._build_topic_stock({"sc": 0, "zqdm": "300975"})
    assert row.market_id == 0
    assert row.full_code == "sz300975"
    assert seen == [0]


def test_topics_tuple_override_retains_lazy_projection(monkeypatch):
    seen = []
    helper = helper_with(SimpleNamespace(
        topic_ids=lambda code: SimpleNamespace(rows=[{"t001": "1"}, {"t001": "2"}]),
        hot_topics=lambda code: SimpleNamespace(rows=()),
    ))
    marker = object()
    monkeypatch.setattr(core, "_build_stock_topic", lambda row: seen.append(row["topic_id"]) or marker)
    monkeypatch.setattr(core, "tuple", lambda values: (next(iter(values)),), raising=False)
    result = helper.stock_topics("000034")
    assert result.topics == (marker,)
    assert seen == ["1"]


def test_topic_stock_lazy_generator_converts_model_stop_iteration_with_cause(monkeypatch):
    helper = helper_with(SimpleNamespace(topic_compare=lambda *args, **kwargs: SimpleNamespace(rows=[{}])))

    def stopped(row):
        raise StopIteration("model")

    monkeypatch.setattr(core, "_build_topic_stock", stopped)
    with pytest.raises(RuntimeError, match="generator raised StopIteration") as caught:
        helper.topic_stocks("000034", topic_id="1")
    assert isinstance(caught.value.__cause__, StopIteration)
