"""Legacy catalog/macro cases; do not run before the complete source gate."""
from __future__ import annotations

import math
from types import SimpleNamespace

import pytest

from eltdx.ex import mac_client as mx


def client_with(execute=lambda command: None):
    client = mx.MacExClient(hosts=["fixture"])
    client._execute = execute
    return client


def info(market, code, name="display", category=7):
    return mx.ExInstrumentInfo(category=category, market=market, code=code, name=name, desc="description", _raw=b"raw")


def goods(name, numeric=()):
    return mx.GoodsItem(name=name, category=0, u=0, index=0, switch=0, code=list(numeric))


def test_extension_goods_fallback_keeps_page_relative_index_single_float_and_nonfinite():
    source = [info(33, "00123"), info(33, "IFL0"), info(33, "", "fallback"), info(33, "nan"), info(33, "inf")]
    result = mx._to_goods_items(source)
    assert [row.index for row in result] == [0, 1, 2, 3, 4]
    assert [row.name for row in result] == ["00123", "IFL0", "fallback", "nan", "inf"]
    assert [row.code for row in result[:3]] == [[123.0], [], []]
    assert math.isnan(result[3].code[0]) and result[4].code == [float("inf")]
    assert all(row.category == 7 and (row.u, row.switch, row.c1, row.c2) == (0, 0, 0, 0) for row in result)
    assert mx._to_goods_items(source[1:2])[0].index == 0
    assert source[0]._raw == b"raw"


def test_extension_goods_fallback_catches_only_type_and_value_errors(monkeypatch):
    failure = RuntimeError("numeric hook failure")

    def convert(value):
        if value == "type":
            raise TypeError("legacy empty numeric")
        if value == "value":
            raise ValueError("legacy empty numeric")
        raise failure

    monkeypatch.setattr(mx, "float", convert, raising=False)
    assert [item.code for item in mx._to_goods_items([info(33, "type"), info(33, "value")])] == [[], []]
    with pytest.raises(RuntimeError) as caught:
        mx._to_goods_items([info(33, "other")])
    assert caught.value is failure


def test_extension_goods_fallback_keeps_custom_enumerate_and_model_constructor(monkeypatch):
    source = info(33, "12")
    observed = []
    monkeypatch.setattr(mx, "enumerate", lambda values: iter([("custom-index", source)]), raising=False)
    monkeypatch.setattr(mx, "GoodsItem", lambda **values: observed.append(values) or values)
    rows = mx._to_goods_items([source])
    assert rows[0] is observed[0]
    assert rows[0] == dict(name="12", category=7, u=0, index="custom-index", switch=0, code=[12.0], c1=0, c2=0)


def test_extension_first_code_uses_trimmed_label_before_numeric_values():
    class Item:
        name = " IFL0 "

        @property
        def code(self):
            pytest.fail("label has precedence")

    assert mx._first_code(Item()) == "IFL0"
    assert mx._first_code(goods("", [123.9, 0, -2])) == "123"
    assert mx._first_code(goods("", [-1, 5])) == ""


def test_extension_first_code_converts_every_part_before_first_value_check():
    failure = ValueError("later numeric part")

    class Bad:
        def __int__(self):
            raise failure

    with pytest.raises(ValueError) as caught:
        mx._first_code(goods("", [0, Bad()]))
    assert caught.value is failure


def test_extension_goods_count_retains_native_total_then_ex_bounds_and_global_path(monkeypatch):
    calls = []
    command = SimpleNamespace(total=17)
    monkeypatch.setattr(mx, "GoodsListCmd", lambda *values: calls.append(("goods", values)) or command)
    monkeypatch.setattr(mx, "GetExInstrumentCountCmd", lambda: "global-count")
    client = client_with(lambda value: calls.append(("execute", value)) or 123)
    client._market_bounds = lambda market: calls.append(("bounds", market)) or (11, 20)
    assert client.goods_count() == 123
    assert client.goods_count(33) == 17
    command.total = 0
    assert client.goods_count(33) == 9
    assert calls[0] == ("execute", "global-count") and calls[-1] == ("bounds", 33)


def test_extension_goods_list_requests_fixed_600_pages_then_returns_requested_prefix(monkeypatch):
    calls = []
    monkeypatch.setattr(mx, "GoodsListCmd", lambda *values: values)
    first, second = [object() for _ in range(600)], [object() for _ in range(600)]
    pages = iter((first, second))
    client = client_with(lambda command: calls.append(command) or next(pages))
    client.instruments = lambda *args, **options: pytest.fail("nonempty native goods do not fall back")
    rows = client.goods_list(33, start=5, count=601)
    assert calls == [(33, 5, 600), (33, 605, 600)]
    assert len(rows) == 601 and rows[0] is first[0] and rows[-1] is second[0]


@pytest.mark.parametrize("count", [0, -2, 4])
def test_extension_goods_list_empty_native_path_falls_back_even_for_nonpositive_count(monkeypatch, count):
    calls = []
    monkeypatch.setattr(mx, "GoodsListCmd", lambda *values: calls.append(("command", values)) or values)
    marker = object()
    client = client_with(lambda command: [])
    client.instruments = lambda market, **options: calls.append(("fallback", market, options)) or marker
    monkeypatch.setattr(mx, "_to_goods_items", lambda infos: calls.append(("convert", infos)) or [marker])
    assert client.goods_list(33, start=8, count=count) == [marker]
    assert calls[-2:] == [("fallback", 33, {"start": 8, "count": count}), ("convert", marker)]
    assert any(call[0] == "command" for call in calls) is (count > 0)


def test_extension_goods_list_captures_execute_before_constructor_and_keeps_source_error(monkeypatch):
    events = []
    row = object()
    client = client_with(lambda command: events.append("initial-execute") or [row])

    def constructor(*values):
        client._execute = lambda command: pytest.fail("old execute was already captured")
        return values

    monkeypatch.setattr(mx, "GoodsListCmd", constructor)
    assert client.goods_list(33, count=1) == [row]
    assert events == ["initial-execute"]
    failure = mx.TransportError("catalog disconnected")

    def fail(command):
        raise failure

    client._execute = fail
    with pytest.raises(mx.TransportError) as caught:
        client.goods_list(33, count=1)
    assert caught.value is failure


def test_extension_market_bounds_binary_search_and_instance_cache_keep_identity(monkeypatch):
    rows = [info(market, str(index)) for index, market in enumerate([1, 31, 33, 33, 33, 34, 38])]
    calls = []
    monkeypatch.setattr(mx, "GetExInstrumentCountCmd", lambda: "count")
    client = client_with(lambda command: calls.append(command) or len(rows))
    client._info_page = lambda start, count: calls.append((start, count)) or rows[start:start + count]
    bounds = client._market_bounds(33)
    assert bounds == (2, 5)
    length = len(calls)
    assert client._market_bounds(33) is bounds and len(calls) == length
    other = client_with(lambda command: len(rows))
    assert other._bounds_cache == {}


def test_extension_market_bounds_nonpositive_global_count_is_not_cached(monkeypatch):
    calls = []
    monkeypatch.setattr(mx, "GetExInstrumentCountCmd", lambda: "count")
    client = client_with(lambda command: calls.append(command) or 0)
    assert client._market_bounds(33) == (0, 0)
    assert client._market_bounds(33) == (0, 0)
    assert calls == ["count", "count"] and client._bounds_cache == {}


def test_extension_instruments_use_market_relative_start_and_keep_original_dtos(monkeypatch):
    source = [info(31, "prior"), info(33, "a"), info(33, "b"), info(33, "c"), info(34, "next")]
    monkeypatch.setattr(mx, "GetExInstrumentCountCmd", lambda: "count")
    client = client_with(lambda command: len(source))
    client._market_bounds = lambda market: (0, 4)
    calls = []
    client._info_page = lambda start, count: calls.append((start, count)) or source[start:start + count]
    rows = client.instruments(33, start=1, count=2)
    assert rows == source[2:4] and rows[0] is source[2] and rows[1]._raw == b"raw"
    assert calls == [(0, 2), (2, 2)]


def test_extension_instruments_nonpositive_count_performs_no_catalog_or_execute_lookup():
    client = client_with(lambda command: pytest.fail("nonpositive count"))
    client._market_bounds = lambda market: pytest.fail("nonpositive count")
    assert client.instruments(33, count=0) == []
    assert client.instruments(33, count=-1) == []


def test_extension_macro_full_code_bypasses_lookup_and_returns_stripped_code():
    client = client_with()
    client.macro_indicators = lambda: pytest.fail("full identity needs no lookup")
    assert client._macro_code(" 1_GDP ") == "1_GDP"
    assert client._macro_cache == {}


def test_extension_macro_suffix_unknown_and_ambiguity_retain_errors_and_sorted_candidates():
    client = client_with()
    client._macro_cache = {"8_XAU": goods("8_XAU"), "1_GDP": goods("1_GDP"), "5_XAU": goods("5_XAU")}
    assert client._macro_code("GDP") == "1_GDP"
    with pytest.raises(ValueError, match="candidates: 5_XAU, 8_XAU"):
        client._macro_code("XAU")
    with pytest.raises(mx.ProtocolError, match="unknown macro indicator: 'missing'"):
        client._macro_code("missing")


def test_extension_macro_indicators_keep_first_result_and_last_duplicate_cached_object():
    first, replacement, other = goods("1_GDP"), goods("1_GDP"), goods("2_CPI")
    source = [first, other, replacement]
    client, calls = client_with(), []
    client.goods_list = lambda *args: calls.append(args) or source
    assert client.macro_indicators() is source
    cached = client.macro_indicators()
    assert cached == [replacement, other] and cached[0] is replacement
    assert calls == [(mx.ExMarket.MACRO_INDICATOR, 0, 600)]
    assert cached is not source


def test_extension_macro_cache_has_no_new_ttl_and_is_instance_local():
    client, calls = client_with(), []
    original = goods("1_GDP")
    client.goods_list = lambda *args: calls.append(args) or [original]
    assert client._macro_code("GDP") == "1_GDP"
    client.goods_list = lambda *args: pytest.fail("legacy cache has no TTL refresh")
    assert client._macro_code("GDP") == "1_GDP" and client.macro_indicators()[0] is original
    assert len(calls) == 1 and client_with()._macro_cache == {}


def test_extension_macro_quotes_preserve_input_order_duplicates_fields_and_no_extra_batch_limit():
    client, calls = client_with(), []
    fields = object()
    client._macro_code = lambda value: calls.append(value) or f"1_{value}"
    results = [object()]
    client.goods_quotes = lambda codes, selected: calls.append((codes, selected)) or results
    assert client.macro_quotes("GDP", fields) is results
    assert calls == ["GDP", ([(mx.ExMarket.MACRO_INDICATOR, "1_GDP")], fields)]
    calls.clear()
    assert client.macro_quotes(iter(["GDP"] * 90), fields) is results
    assert calls[:90] == ["GDP"] * 90 and len(calls[-1][0]) == 90
    assert client.macro_quotes([], fields) == []


def test_extension_macro_kline_captures_method_before_resolution_and_preserves_all_options():
    events = []
    client = client_with()
    rows = [object()]
    client.goods_kline = lambda *args, **options: events.append((args, options)) or rows

    def resolve(code):
        client.goods_kline = lambda *args, **options: pytest.fail("captured original method")
        return "1_GDP"

    client._macro_code = resolve
    assert client.macro_kline("GDP", mx.Period.QUARTERLY, 3, 9, mx.Adjust.HFQ) is rows
    assert events == [((mx.ExMarket.MACRO_INDICATOR, "1_GDP"),
                       dict(period=mx.Period.QUARTERLY, start=3, count=9, adjust=mx.Adjust.HFQ))]


def test_extension_macro_suffix_keeps_custom_partition_and_legacy_missing_separator_empty_suffix():
    events = []

    class Name(str):
        def partition(self, separator):
            events.append(separator)
            return ("custom", separator, "GDP")

    name = Name("custom")
    client = client_with()
    client._macro_cache = {name: goods(name)}
    assert client._macro_code("GDP") is name
    assert events == ["_"]
    client._macro_cache = {"plain": goods("plain")}
    assert client._macro_code("") == "plain"
