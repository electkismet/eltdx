"""Extension entry/routing contracts; authored for the final unified gate."""
from __future__ import annotations

from datetime import date, time
from types import SimpleNamespace

import pytest

from eltdx.ex import _hk_transaction as hk
from eltdx.ex import client as ex
from eltdx.ex import mac_client as mx
from eltdx.mac import client as mac


def make_client(module, execute):
    constructor = {mac: mac.MacClient, ex: ex.ExTdxClient, mx: mx.MacExClient}[module]
    client = constructor(hosts=["fixture"])
    client._execute = execute
    return client


@pytest.mark.parametrize("module,method,command,args", [
    (mac, "get_stock_quotes", "SymbolQuotesCmd", ([(1, "x")], None)),
    (mac, "get_chart_sampling", "ChartSamplingCmd", (1, "x")),
    (mac, "get_symbol_info", "SymbolInfoCmd", (1, "x")),
    (mac, "get_belong_board", "SymbolBelongBoardCmd", (1, "x")),
    (mac, "get_capital_flow", "SymbolCapitalFlowCmd", (1, "x")),
    (mac, "get_auction", "SymbolAuctionCmd", (1, "x")),
    (mac, "get_server_info", "ServerInfoCmd", ()),
    (mac, "get_kline_offset", "KlineOffsetCmd", (3, 7)),
    (mac, "get_file_meta", "FileListCmd", ("remote",)),
    (mac, "download_file_chunk", "FileDownloadCmd", ("remote", 1, 3, 7)),
    (mac, "get_goods_list", "GoodsListCmd", (33, 4, 7)),
    (ex, "get_markets", "GetExMarketsCmd", ()),
    (ex, "get_instrument_count", "GetExInstrumentCountCmd", ()),
    (ex, "get_instrument_info", "GetExInstrumentInfoCmd", (3, 7)),
    (ex, "get_instrument_quote", "GetExInstrumentQuoteCmd", (31, "x")),
    (ex, "get_instrument_quote_list", "GetExInstrumentQuoteListCmd", (31, 2, 3, 7)),
    (ex, "get_minute_time_data", "GetExMinuteTimeDataCmd", (31, "x")),
    (ex, "get_history_minute_time_data", "GetExHistoryMinuteTimeDataCmd", (31, "x", 20261117)),
    (ex, "get_transaction_data", "GetExTransactionDataCmd", (31, "x", 3, 7)),
    (ex, "get_history_transaction_data", "GetExHistoryTransactionDataCmd", (31, "x", 20261117, 3, 7)),
    (mx, "_info_page", "GetExInstrumentInfoCmd", (3, 7)),
    (mx, "goods_tick_chart", "SymbolTickChartCmd", (31, "x", date(2026, 11, 17))),
    (mx, "goods_chart_sampling", "ChartSamplingCmd", (31, "x")),
])
def test_extension_single_entries_keep_original_arguments_single_request_and_result_identity(monkeypatch, module, method, command, args):
    calls, result = [], object()
    monkeypatch.setattr(module, command, lambda *values, **options: (values, options))
    client = make_client(module, lambda value: calls.append(value) or result)
    assert getattr(client, method)(*args) is result
    assert calls == [(args, {})]


def test_extension_single_command_capture_precedes_constructor_and_keeps_original_error(monkeypatch):
    events = []
    result = object()
    client = make_client(ex, lambda value: events.append(value) or result)

    def constructor(*args):
        client._execute = lambda value: pytest.fail("execute was already captured")
        return args

    monkeypatch.setattr(ex, "GetExInstrumentInfoCmd", constructor)
    assert client.get_instrument_info(4, 1) is result
    assert events == [(4, 1)]
    failure = ex.TransportError("legacy execute failure")

    def fail(command):
        raise failure

    client._execute = fail
    with pytest.raises(ex.TransportError) as caught:
        client.get_instrument_info(4, 1)
    assert caught.value is failure


def test_extension_mac_quotes_do_not_replace_none_and_mac_ex_keeps_own_seven_fields(monkeypatch):
    seen = []
    stocks, custom = [(31, "00700")], object()
    monkeypatch.setattr(mac, "SymbolQuotesCmd", lambda *args: args)
    monkeypatch.setattr(mx, "SymbolQuotesCmd", lambda *args: args)
    a = make_client(mac, lambda command: seen.append(command) or [])
    b = make_client(mx, lambda command: seen.append(command) or [])
    a.get_stock_quotes(stocks)
    b.goods_quotes(stocks)
    b.goods_quotes(stocks, custom)
    assert seen[0] == (stocks, None)
    assert seen[1][1] is mx.DEFAULT_QUOTE_FIELDS and seen[2][1] is custom


@pytest.mark.parametrize("count,expected", [(0, 600), (None, 600), (-2, -2), (7, 7)])
def test_extension_unusual_keeps_truthiness_default_without_integer_narrowing(monkeypatch, count, expected):
    monkeypatch.setattr(mac, "UnusualCmd", lambda *values: values)
    client = make_client(mac, lambda value: value)
    assert client.get_unusual(1, 4, count) == (1, 4, expected)


def test_extension_mac_chart_date_conversion_keeps_captured_command_and_execute(monkeypatch):
    events = []
    client = make_client(mac, lambda command: events.append(("execute", command)) or command)
    monkeypatch.setattr(mac, "SymbolTickChartCmd", lambda *args: ("original-command", args))

    def convert(value):
        events.append(("date", value))
        client._execute = lambda value: pytest.fail("execute captured before date conversion")
        monkeypatch.setattr(mac, "SymbolTickChartCmd", lambda *args: pytest.fail("command captured before date conversion"))
        return date(2026, 11, 17)

    monkeypatch.setattr(mac, "_ymd_to_date", convert)
    result = client.get_tick_chart(1, "x", 20261117)
    assert result == ("original-command", (1, "x", date(2026, 11, 17)))
    assert events[0] == ("date", 20261117)
    assert mac.MacClient.get_stock_minute_time is mac.MacClient.get_tick_chart
    assert mac.MacClient.get_stock_minute_time_hist is mac.MacClient.get_tick_charts


def test_extension_ex_bar_alignment_stays_after_single_fetch_and_uses_live_hooks(monkeypatch):
    events, bars = [], [object()]
    monkeypatch.setattr(ex, "GetExInstrumentBarsCmd", lambda *args: args)
    client = make_client(ex, lambda command: events.append(("execute", command)) or bars)
    monkeypatch.setattr(ex, "category_to_minutes", lambda value: events.append(("period", value)) or 5)
    monkeypatch.setattr(ex, "align_hour_minute_bars", lambda rows, **options: events.append(("align", rows, options)) or rows)
    assert client.get_instrument_bars(0, 31, "x", 3, 7, bar_time="end") is bars
    assert events == [("execute", (0, 31, "x", 3, 7)), ("period", 0),
                      ("align", bars, {"delta_minutes": 5, "bar_time": "end"})]


def test_extension_ex_range_validates_before_fetch_and_preserves_no_period_rule(monkeypatch):
    calls = []
    client = make_client(ex, lambda command: calls.append(command) or command)
    monkeypatch.setattr(ex, "GetExHistoryInstrumentBarsRangeCmd", lambda *args: args)
    with pytest.raises(ValueError, match="缺少周期信息"):
        client.get_history_instrument_bars_range(31, "x", 20261101, 20261117, bar_time="end")
    assert calls == []
    failure = ValueError("custom time validator")

    def check(value):
        raise failure

    monkeypatch.setattr(ex, "check_bar_time", check)
    with pytest.raises(ValueError) as caught:
        client.get_history_instrument_bars_range(31, "x", 20261101, 20261117)
    assert caught.value is failure and calls == []


def test_extension_mac_ex_bars_keep_single_page_keyword_layout(monkeypatch):
    monkeypatch.setattr(mx, "SymbolBarCmd", lambda *args, **options: (args, options))
    client = make_client(mx, lambda value: value)
    assert client.goods_kline(47, "IFL0", mx.Period.DAILY, 3, 900, mx.Adjust.HFQ) == (
        (47, "IFL0"), {"period": mx.Period.DAILY, "start": 3, "count": 900, "fq": mx.Adjust.HFQ})


def test_extension_mac_ex_quote_list_caps_80_ignores_sort_and_extracts_all_before_filtering(monkeypatch):
    events, fields = [], object()

    class Code:
        def __init__(self, value):
            self.value = value

        def __bool__(self):
            events.append(("bool", self.value))
            return bool(self.value)

    items = [object(), object(), object()]
    values = iter([Code("A"), Code(""), Code("A")])
    client = make_client(mx, lambda command: events.append(("execute", command)) or command)
    client.goods_list = lambda market, **options: events.append(("goods", market, options)) or items
    monkeypatch.setattr(mx, "_first_code", lambda item: events.append(("code", item)) or next(values))
    monkeypatch.setattr(mx, "SymbolQuotesCmd", lambda *args: args)
    monkeypatch.setattr(mx, "DEFAULT_QUOTE_FIELDS", fields)
    result = client.goods_quotes_list(33, 5, 100, object(), object())
    assert events[0] == ("goods", 33, {"start": 5, "count": 80})
    assert [event[0] for event in events[1:7]] == ["code", "code", "code", "bool", "bool", "bool"]
    assert [code.value for _, code in result[0]] == ["A", "A"] and result[1] is fields


@pytest.mark.parametrize("market,stock", [(27, True), (31, True), (48, True), (49, True), (71, True), (98, True),
                                         (23, False), (24, False), (25, False), (26, False), (47, False)])
def test_extension_hk_routing_keeps_exact_stock_markets(market, stock):
    assert hk.is_hk_stock_market(market) is stock


def test_extension_mac_ex_transactions_preserve_custom_route_fetch_and_common_command(monkeypatch):
    calls = []
    client = make_client(mx, lambda command: calls.append(("execute", command)) or command)
    monkeypatch.setattr(mx, "SymbolTransactionCmd", lambda *args: args)
    monkeypatch.setattr(mx, "is_hk_stock_market", lambda market: calls.append(("route", market)) or False)
    assert client.goods_transaction(47, "IFL0", None, 3, 7) == (47, "IFL0", None, 3, 7)
    monkeypatch.setattr(mx, "is_hk_stock_market", lambda market: True)
    marker = object()
    monkeypatch.setattr(mx, "_fetch_hk_transactions_sync", lambda *args: calls.append(("hk", args)) or marker)
    assert client.goods_transaction(31, "00700", date(2026, 11, 17), 3, 7) is marker
    assert calls[-1][1] == (client._execute, 31, "00700", date(2026, 11, 17), 3, 7)


def test_extension_mac_ex_all_transactions_rejects_non_hk_before_fetch_and_uses_legacy_helper(monkeypatch):
    client = make_client(mx, lambda command: pytest.fail("non-HK all query"))
    with pytest.raises(ValueError, match="收到 market=47"):
        client.goods_transaction_all(47, "IFL0")
    calls, marker = [], object()
    monkeypatch.setattr(mx, "_fetch_all_hk_transactions_sync", lambda *args: calls.append(args) or marker)
    assert client.goods_transaction_all(31, "00700") is marker
    assert calls == [(client._execute, 31, "00700", None)]


def test_extension_hk_record_mapping_retains_seconds_nature_volume_and_per_mille_price():
    source = SimpleNamespace(hour=13, minute=14, second=15, price=12345, volume=17, nature=5)
    result = hk._map_record(source)
    assert result.time == time(13, 14, 15) and result.price == 12.345
    assert (result.vol, result.trade_count, result.bs_flag) == (17, 0, 5)


def test_extension_hk_mapping_keeps_model_time_and_dynamic_divisor_order(monkeypatch):
    calls = []
    source = SimpleNamespace(hour=1, minute=2, second=3, price=20, volume=4, nature=9)
    constructor = lambda **options: calls.append(("model", options)) or options
    monkeypatch.setattr(hk, "MacTransaction", constructor)

    def clock(*args):
        calls.append(("time", args))
        monkeypatch.setattr(hk, "MacTransaction", lambda **options: pytest.fail("constructor was captured"))
        monkeypatch.setattr(hk, "_HK_PRICE_DIVISOR", 10)
        return "clock"

    monkeypatch.setattr(hk, "time", clock)
    assert hk._map_record(source) == dict(time="clock", price=2.0, vol=4, trade_count=0, bs_flag=9)
    assert calls[0] == ("time", (1, 2, 3))


def test_extension_hk_command_selection_preserves_query_date_and_positional_arguments(monkeypatch):
    monkeypatch.setattr(hk, "GetExTransactionDataCmd", lambda *args: ("today", args))
    monkeypatch.setattr(hk, "GetExHistoryTransactionDataCmd", lambda *args: ("history", args))
    assert hk._to_ymd(date(2026, 11, 17)) == 20261117
    assert hk._build_cmd(31, "x", None, 3, 7) == ("today", (31, "x", 3, 7))
    assert hk._build_cmd(31, "x", 20261117, 3, 7) == ("history", (31, "x", 20261117, 3, 7))


def test_extension_hk_pages_keep_remainder_offsets_duplicates_and_order(monkeypatch):
    calls = []
    monkeypatch.setattr(hk, "_HK_TRANSACTION_PAGE_SIZE", 3)
    monkeypatch.setattr(hk, "_build_cmd", lambda *args: args)
    marker = object()
    monkeypatch.setattr(hk, "_map_record", lambda row: row)
    pages = iter([[marker, marker, "last"], ["older", "oldest"]])
    result = hk._fetch_hk_transactions_sync(lambda command: calls.append(command) or next(pages), 31, "x", None, 8, 5)
    assert result == [marker, marker, "last", "older", "oldest"]
    assert calls == [(31, "x", None, 8, 3), (31, "x", None, 11, 2)]


def test_extension_hk_pages_date_conversion_still_occurs_for_nonpositive_count(monkeypatch):
    events = []
    monkeypatch.setattr(hk, "_to_ymd", lambda value: events.append(value) or 20261117)
    execute = lambda command: pytest.fail("nonpositive requested count")
    assert hk._fetch_hk_transactions_sync(execute, 31, "x", "date", 0, 0) == []
    assert hk._fetch_hk_transactions_sync(execute, 31, "x", "date", 0, -1) == []
    assert events == ["date", "date"]


def test_extension_hk_all_retains_50_page_prefix_and_no_strict_limit_error(monkeypatch):
    calls = []
    monkeypatch.setattr(hk, "_HK_TRANSACTION_PAGE_SIZE", 1)
    monkeypatch.setattr(hk, "_build_cmd", lambda *args: args)
    monkeypatch.setattr(hk, "_map_record", lambda row: row)
    result = hk._fetch_all_hk_transactions_sync(lambda command: calls.append(command) or [command[3]], 31, "x", None, 7)
    assert result == list(range(7, 57)) and len(calls) == 50
    assert calls[-1] == (31, "x", None, 56, 1)


def test_extension_hk_regular_paging_has_no_new_32_request_limit_or_overdelivery_truncation(monkeypatch):
    calls = []
    monkeypatch.setattr(hk, "_HK_TRANSACTION_PAGE_SIZE", 1)
    monkeypatch.setattr(hk, "_build_cmd", lambda *args: args)
    monkeypatch.setattr(hk, "_map_record", lambda row: row)
    assert hk._fetch_hk_transactions_sync(lambda command: calls.append(command) or [1], 31, "x", None, 0, 35) == [1] * 35
    assert len(calls) == 35
    assert hk._fetch_hk_transactions_sync(lambda command: [1, 2], 31, "x", None, 0, 1) == [1, 2]


def test_extension_hk_mapping_is_lazy_live_and_wraps_body_stopiteration_with_original_cause(monkeypatch):
    events = []
    monkeypatch.setattr(hk, "_build_cmd", lambda *args: args)
    failure = StopIteration("custom mapper stopped")

    def replacement(row):
        events.append(("replacement", row))
        raise failure

    def first(row):
        events.append(("first", row))
        monkeypatch.setattr(hk, "_map_record", replacement)
        return row

    monkeypatch.setattr(hk, "_map_record", first)
    with pytest.raises(RuntimeError, match="generator raised StopIteration") as caught:
        hk._fetch_hk_transactions_sync(lambda command: [1, 2], 31, "x", None, 0, 2)
    assert caught.value.__cause__ is failure and events == [("first", 1), ("replacement", 2)]


def test_extension_hk_custom_range_and_live_page_size_remain_observable(monkeypatch):
    calls = []
    monkeypatch.setattr(hk, "range", lambda value: calls.append(("range", value)) or iter([2, 8]), raising=False)
    monkeypatch.setattr(hk, "_HK_TRANSACTION_PAGE_SIZE", 1)
    monkeypatch.setattr(hk, "_build_cmd", lambda *args: args)
    monkeypatch.setattr(hk, "_map_record", lambda row: row)

    def execute(command):
        calls.append(command)
        return [command[3]]

    assert hk._fetch_all_hk_transactions_sync(execute, 31, "x", None) == [0, 1]
    assert calls == [("range", 50), (31, "x", None, 0, 1), (31, "x", None, 1, 1)]


def test_extension_hk_page_error_propagates_same_exception_and_short_page_stops(monkeypatch):
    failure = mx.TransportError("HK source failed")
    monkeypatch.setattr(hk, "_build_cmd", lambda *args: args)

    def fail(command):
        raise failure

    with pytest.raises(mx.TransportError) as caught:
        hk._fetch_all_hk_transactions_sync(fail, 31, "x", None)
    assert caught.value is failure
    calls = []
    monkeypatch.setattr(hk, "_map_record", lambda row: row)
    assert hk._fetch_all_hk_transactions_sync(lambda command: calls.append(command) or [1], 31, "x", None) == [1]
    assert len(calls) == 1
