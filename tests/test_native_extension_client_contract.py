"""Legacy extension orchestration contracts; authored, not yet executed."""
from __future__ import annotations

from datetime import date, datetime
from types import SimpleNamespace

import pytest

from eltdx.mac import client as mac
from eltdx.mac.adjust import XdxrEvent
from eltdx.mac.bitmap import FieldBit, PresetField
from eltdx.mac.models import MacBar


def client_with(execute):
    client = mac.MacClient(hosts=["fixture"])
    client._execute = execute
    return client


def test_extension_quote_pages_keep_remainder_server_order_and_live_category(monkeypatch):
    requests, integers = [], []
    original_rows = [object() for _ in range(100)]

    class Category:
        def __int__(self):
            integers.append("int")
            return 6

    def execute(command):
        requests.append(command)
        return original_rows[command["start"]:command["start"] + command["page_size"]]

    monkeypatch.setattr(mac, "BoardMembersQuotesCmd", lambda **values: values)
    result = client_with(execute).get_stock_quotes_list(Category(), count=100)
    assert [(r["start"], r["page_size"]) for r in requests] == [(0, 80), (80, 20)]
    assert integers == ["int", "int"]
    assert result == original_rows
    assert result[0] is original_rows[0] and result[-1] is original_rows[-1]
    assert requests[0]["fields"] is requests[1]["fields"]
    assert set(requests[0]["fields"]) == set(PresetField.BASIC.value + PresetField.VOLUME.value)


def test_extension_quote_pages_keep_dynamic_cap_and_three_len_evaluations(monkeypatch):
    requests, lengths = [], []
    monkeypatch.setattr(mac, "_BOARD_MEMBERS_PAGE_SIZE", 3)
    monkeypatch.setattr(mac, "BoardMembersQuotesCmd", lambda **values: values)
    monkeypatch.setattr(mac, "len", lambda value: lengths.append(value) or len(value), raising=False)

    def execute(command):
        requests.append(command)
        monkeypatch.setattr(mac, "_BOARD_MEMBERS_PAGE_SIZE", 2)
        return list(range(command["start"], command["start"] + command["page_size"]))

    assert client_with(execute).get_stock_quotes_list(6, count=5) == [0, 1, 2, 3, 4]
    assert [(r["start"], r["page_size"]) for r in requests] == [(0, 3), (3, 2)]
    assert [len(value) for value in lengths] == [3, 3, 3, 2, 2, 2]


def test_extension_pages_keep_overdelivery_duplicates_and_more_than_32_requests(monkeypatch):
    monkeypatch.setattr(mac, "BoardMembersQuotesCmd", lambda **values: values)
    marker = object()
    assert client_with(lambda command: [marker, marker]).get_stock_quotes_list(6, count=1) == [marker, marker]
    requests = []
    monkeypatch.setattr(mac, "_BOARD_MEMBERS_PAGE_SIZE", 1)

    def execute(command):
        requests.append(command)
        return [marker]

    assert len(client_with(execute).get_stock_quotes_list(6, count=35)) == 35
    assert len(requests) == 35


def test_extension_members_convert_once_even_for_zero_count_and_preserve_options(monkeypatch):
    calls = []
    board, fields, excluded = object(), object(), object()
    monkeypatch.setattr(mac, "convert_board_code", lambda text: calls.append(("convert", text)) or board)
    monkeypatch.setattr(mac, "BoardMembersQuotesCmd", lambda **values: values)
    client = client_with(lambda command: calls.append(command) or [])
    assert client.get_board_members("000686", count=0, fields=fields, exclude_flags=excluded) == []
    assert calls == [("convert", "000686")]
    assert client.get_board_members("000686", count=1, fields=fields, exclude_flags=excluded) == []
    assert calls[2]["board_code"] is board
    assert calls[2]["fields"] is fields
    assert calls[2]["exclude_flags"] is excluded


def test_extension_bar_pages_prepend_whole_pages_without_reversing_rows(monkeypatch):
    calls = []
    monkeypatch.setattr(mac, "_KLINE_PAGE_SIZE", 3)
    monkeypatch.setattr(mac, "SymbolBarCmd", lambda **values: values)
    pages = iter((["late0", "late1", "late1"], ["early0", "early1"]))

    def execute(command):
        calls.append(command)
        return next(pages)

    client = client_with(execute)
    result = client._fetch_kline_pages(1, "600000", mac.Period.SECONDS, 7, 5, 3, mac.Adjust.HFQ)
    assert result == ["early0", "early1", "late0", "late1", "late1"]
    assert [(r["start"], r["count"]) for r in calls] == [(7, 3), (10, 2)]
    assert all(r["times"] == 3 and r["fq"] is mac.Adjust.HFQ for r in calls)


def test_extension_bar_pages_use_custom_batch_add_and_keep_no_request_zero_count(monkeypatch):
    events = []

    class Page(list):
        def __add__(self, previous):
            events.append(("add", previous))
            return ["custom"]

    monkeypatch.setattr(mac, "SymbolBarCmd", lambda **values: values)
    client = client_with(lambda command: events.append(command) or Page(["wire"]))
    assert client._fetch_kline_pages(1, "x", mac.Period.DAILY, 0, 0, 1, mac.Adjust.NONE) == []
    assert events == []
    assert client._fetch_kline_pages(1, "x", mac.Period.DAILY, 0, 1, 1, mac.Adjust.NONE) == ["custom"]
    assert events[-1] == ("add", [])


def test_extension_transactions_initial_short_page_still_continues_and_returns_first_object(monkeypatch):
    calls = []
    monkeypatch.setattr(mac, "_TRANSACTION_PAGE_SIZE", 3)
    monkeypatch.setattr(mac, "SymbolTransactionCmd", lambda *values: values)
    first = ["first"]
    pages = iter((first, ["second", "third"]))

    def execute(command):
        calls.append(command)
        return next(pages)

    result = client_with(execute).get_transactions(1, "x", count=5, start=8, date=20261117)
    assert result is first
    assert result == ["first", "second", "third"]
    assert calls == [(1, "x", date(2026, 11, 17), 8, 3), (1, "x", date(2026, 11, 17), 9, 3)]


@pytest.mark.parametrize("count", [0, -2])
def test_extension_transactions_keep_initial_query_for_nonpositive_count(monkeypatch, count):
    calls = []
    first = []
    monkeypatch.setattr(mac, "SymbolTransactionCmd", lambda *values: calls.append(values) or values)
    client = client_with(lambda command: first)
    assert client.get_transactions(1, "x", count=count) is first
    assert calls == [(1, "x", None, 0, count)]
    assert mac.MacClient.get_stock_transaction is mac.MacClient.get_transactions


def test_extension_board_list_initial_short_page_preserves_first_result_identity(monkeypatch):
    calls = []
    first = [object()]
    pages = iter((first, [object(), object()]))
    monkeypatch.setattr(mac, "BoardListCmd", lambda *values: values)

    def execute(command):
        calls.append(command)
        return next(pages)

    result = client_with(execute).get_board_list(mac.BoardType.GN, count=200)
    assert result is first and len(result) == 3
    assert calls == [(mac.BoardType.GN, 0, 150), (mac.BoardType.GN, 1, 150)]


def test_extension_paging_keeps_error_identity_and_first_request_date_conversion(monkeypatch):
    failure = mac.TransportError("custom execute")
    events = []

    def fail(command):
        events.append("execute")
        raise failure

    monkeypatch.setattr(mac, "SymbolTransactionCmd", lambda *values: events.append("command") or values)
    monkeypatch.setattr(mac, "_ymd_to_date", lambda value: events.append(("date", value)) or None)
    with pytest.raises(mac.TransportError) as caught:
        client_with(fail).get_transactions(1, "x", count=0, date=object())
    assert caught.value is failure
    assert events[0][0] == "date" and events[1:] == ["command", "execute"]


def test_extension_kline_qfq_refetch_and_fallback_keep_hook_order_and_seven_args(monkeypatch):
    events = []
    original, unadjusted, repaired = [object()], [object()], [object()]
    client = client_with(lambda command: pytest.fail("custom page hook used"))

    def fetch(*args):
        events.append(("fetch", args))
        return original if args[-1] is mac.Adjust.QFQ else unadjusted

    client._fetch_kline_pages = fetch
    client._local_recompute_qfq = lambda bars, market, code: events.append(("repair", bars, market, code)) or repaired
    monkeypatch.setattr(mac, "has_bad_prices", lambda bars: events.append(("bad", bars)) or True)
    monkeypatch.setattr(mac, "period_to_minutes", lambda period, times: events.append(("period", period, times)) or 5)
    monkeypatch.setattr(mac, "align_datetime_bars", lambda bars, **values: events.append(("align", bars, values)) or bars)
    assert client.get_stock_kline(1, "x", mac.Period.MINS, 2, 3, 5, mac.Adjust.QFQ, bar_time="end") is repaired
    assert events == [("fetch", (1, "x", mac.Period.MINS, 2, 3, 5, mac.Adjust.QFQ)), ("bad", original),
                      ("fetch", (1, "x", mac.Period.MINS, 2, 3, 5, mac.Adjust.NONE)),
                      ("repair", unadjusted, 1, "x"), ("period", mac.Period.MINS, 5),
                      ("align", repaired, {"delta_minutes": 5, "bar_time": "end"})]


def test_extension_kline_invalid_alignment_is_still_checked_after_fetch(monkeypatch):
    events = []
    failure = ValueError("period hook")
    client = client_with(lambda command: None)
    client._fetch_kline_pages = lambda *values: events.append("fetch") or []

    def period(*values):
        events.append("period")
        raise failure

    monkeypatch.setattr(mac, "period_to_minutes", period)
    monkeypatch.setattr(mac, "has_bad_prices", lambda value: pytest.fail("NONE short-circuits invalid check"))
    with pytest.raises(ValueError) as caught:
        client.get_stock_kline(1, "x")
    assert caught.value is failure and events == ["fetch", "period"]


def test_extension_local_qfq_keeps_valid_unadjusted_series_when_all_events_skip():
    bars = [MacBar(datetime(2026, 11, 17), 10, 10, 10, 10, 100, 0)]
    client = client_with(lambda command: None)
    client._xdxr_provider = lambda market, code: [XdxrEvent(date(2026, 1, 1), fenhong=1)]
    out = client._local_recompute_qfq(bars, 1, "x")
    assert out == bars and out is not None
    client._xdxr_provider = None
    assert client._local_recompute_qfq(bars, 1, "x") is None


def test_extension_download_keeps_metadata_short_prefix_and_chunk_numbering(monkeypatch):
    calls = []
    monkeypatch.setattr(mac, "FileListCmd", lambda filename: ("meta", filename))
    monkeypatch.setattr(mac, "FileDownloadCmd", lambda *values: ("chunk",) + values)
    chunks = iter((b"a" * 30000, b"bc"))

    def execute(command):
        calls.append(command)
        return SimpleNamespace(size=100000) if command[0] == "meta" else next(chunks)

    result = client_with(execute).download_file("file.dat", filesize=-1)
    assert type(result) is bytearray and result == bytearray(b"a" * 30000 + b"bc")
    assert calls == [("meta", "file.dat"), ("chunk", "file.dat", 1, 0, 30000),
                     ("chunk", "file.dat", 2, 30000, 30000)]


def test_extension_download_does_not_clip_declared_size_or_hide_transport_error(monkeypatch):
    calls = []
    monkeypatch.setattr(mac, "FileDownloadCmd", lambda *values: values)
    monkeypatch.setattr(mac, "FileListCmd", lambda *values: pytest.fail("positive size skips metadata"))
    result = client_with(lambda command: calls.append(command) or b"a" * 30000).download_file("x", filesize=1)
    assert len(result) == 30000 and calls == [("x", 1, 0, 30000)]
    failure = mac.TransportError("download failed")

    def fail(command):
        raise failure

    with pytest.raises(mac.TransportError) as caught:
        client_with(fail).download_file("x", filesize=1)
    assert caught.value is failure


def test_extension_paging_captures_execute_before_command_constructor(monkeypatch):
    events = []
    client = client_with(lambda command: events.append("first") or [1])
    monkeypatch.setattr(mac, "_BOARD_MEMBERS_PAGE_SIZE", 1)

    def constructor(**values):
        client._execute = lambda command: events.append("replacement") or [2]
        return values

    monkeypatch.setattr(mac, "BoardMembersQuotesCmd", constructor)
    assert client.get_stock_quotes_list(6, count=2) == [1, 2]
    assert events == ["first", "replacement"]
