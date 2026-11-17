"""Board compatibility contracts authored for the final unified test phase."""
from __future__ import annotations

from datetime import date, datetime
from types import FunctionType, GeneratorType, SimpleNamespace

import pytest

from eltdx.mac import client as mac


def client_with_boards(boards):
    client = mac.MacClient(hosts=["fixture"])
    client.get_board_list = lambda board_type: boards
    return client


def board(code, price=11.0, pre_close=10.0):
    return SimpleNamespace(code=code, name=f"board-{code}", market=90, price=price, pre_close=pre_close)


def summary(amount=0.0, vol=0, net=0.0):
    return dict(amount=amount, vol=vol, main_net_amount=net, up_count=2, down_count=1, member_count=3)


def bar(day, close):
    return SimpleNamespace(datetime=datetime(2026, 11, day), close=close)


def test_extension_board_summary_keeps_original_members_order_defaults_and_output_shape():
    members = [SimpleNamespace(fields=dict(amount=2, vol=3, main_net_amount=4, main_net_3d_amount=5,
                                           main_net_5d_amount=6, close=11, pre_close=10)),
               SimpleNamespace(fields=dict(amount=None, vol=7, main_net_amount=-1, main_net_3d_amount=-2,
                                           main_net_5d_amount=-3, close=9, pre_close=10))]
    calls = []
    client = client_with_boards([])
    client.get_board_members = lambda symbol, **options: calls.append((symbol, options)) or members
    result = client.get_board_summary("881001", sort_type=mac.SortType.CODE, sort_order=mac.SortOrder.ASC)
    assert result == dict(member_count=2, amount=2.0, vol=10, main_net_amount=3.0, main_net_3d=3.0,
                          main_net_5d=3.0, up_count=1, down_count=1, members=members)
    assert result["members"] is members
    assert list(result) == ["member_count", "amount", "vol", "main_net_amount", "main_net_3d", "main_net_5d",
                            "up_count", "down_count", "members"]
    assert calls[0][0] == "881001"
    assert calls[0][1]["sort_type"] is mac.SortType.CODE
    assert calls[0][1]["sort_order"] is mac.SortOrder.ASC


def test_extension_board_summary_keeps_seven_lazy_generator_passes_and_live_sum(monkeypatch):
    events, generators = [], []

    class Fields(dict):
        def get(self, key, default=None):
            events.append(key)
            return super().get(key, default)

    members = [SimpleNamespace(fields=Fields(close=11, pre_close=10)),
               SimpleNamespace(fields=Fields(close=9, pre_close=10))]
    client = client_with_boards([])
    client.get_board_members = lambda *args, **options: members

    def first_sum(values):
        assert type(values) is GeneratorType
        generators.append(values)
        assert events == []
        monkeypatch.setattr(mac, "sum", later_sum)
        return 9

    def later_sum(values):
        assert type(values) is GeneratorType
        return sum(values)

    monkeypatch.setattr(mac, "sum", first_sum, raising=False)
    result = client.get_board_summary("x")
    assert result["amount"] == 9.0
    assert events[:2] == ["main_net_amount", "main_net_amount"]
    assert events[-8:] == ["close", "pre_close"] * 4
    assert list(generators[0]) == [0.0, 0.0]
    assert events[-2:] == ["amount", "amount"]


def test_extension_board_summary_captures_outer_conversion_before_iterating_members(monkeypatch):
    calls = []
    client = client_with_boards([])

    def replacement(value):
        calls.append(("replacement", value))
        return float(value)

    class Members(list):
        def __iter__(self):
            monkeypatch.setattr(mac, "float", replacement, raising=False)
            return super().__iter__()

    members = Members([SimpleNamespace(fields={})])
    client.get_board_members = lambda *args, **options: members

    def initial(value):
        calls.append(("initial", value))
        return float(value)

    monkeypatch.setattr(mac, "float", initial, raising=False)
    client.get_board_summary("x")
    assert calls[:2] == [("replacement", 0.0), ("initial", 0.0)]


def test_extension_board_summary_preserves_generator_stopiteration_cause():
    failure = StopIteration("custom quote projection")

    class Quote:
        @property
        def fields(self):
            raise failure

    client = client_with_boards([])
    client.get_board_members = lambda *args, **options: [Quote()]
    with pytest.raises(RuntimeError, match="generator raised StopIteration") as caught:
        client.get_board_summary("x")
    assert caught.value.__cause__ is failure


def test_extension_board_ranking_change_selects_before_member_aggregation_and_keeps_last_duplicate():
    boards = [board("same", 20), board("middle", 15), board("same", 11)]
    client, calls = client_with_boards(boards), []
    client.get_board_summary = lambda code: calls.append(code) or summary()
    rows = client.get_board_ranking(top_n=2)
    assert [row["code"] for row in rows] == ["middle", "same"]
    assert [row["change_pct"] for row in rows] == [50.0, 10.0]
    assert calls == ["middle", "same"]


@pytest.mark.parametrize("sort_by", ["amount", "vol", "main_net_amount"])
def test_extension_board_ranking_money_collects_every_candidate_before_truncation(sort_by):
    boards = [board("high-change", 20), board("high-money", 10.1), board("tie", 10.2)]
    client, calls = client_with_boards(boards), []

    def aggregate(code):
        calls.append(code)
        value = 1 if code == "high-change" else 100
        return summary(value, value, value)

    client.get_board_summary = aggregate
    rows = client.get_board_ranking(top_n=1, sort_by=sort_by)
    assert calls == ["high-change", "high-money", "tie"]
    assert [row["code"] for row in rows] == ["high-money"]
    assert rows[0][sort_by] == 100


def test_extension_board_ranking_keeps_none_negative_and_custom_slice_end():
    boards = [board("a", 13), board("b", 12), board("c", 11)]
    client, calls = client_with_boards(boards), []
    client.get_board_summary = lambda code: calls.append(code) or summary()
    assert len(client.get_board_ranking(top_n=None)) == 3
    calls.clear()
    assert [row["code"] for row in client.get_board_ranking(top_n=-1)] == ["a", "b"]
    assert calls == ["a", "b"]

    class End:
        def __index__(self):
            calls.append("slice")
            return 1

    calls.clear()
    assert len(client.get_board_ranking(top_n=End())) == 1
    assert calls == ["slice", "a"]


def test_extension_board_ranking_keeps_zero_nan_reference_and_captured_live_sorted(monkeypatch):
    boards = [board("zero", 20, 0), board("nan", 20, float("nan")), board("rise", 11)]
    client, observed = client_with_boards(boards), []
    client.get_board_summary = lambda code: summary()

    def sort(values, *, key, reverse=False):
        assert type(key) is FunctionType
        observed.append(([key(value) for value in values], reverse))
        return sorted(values, key=key, reverse=reverse)

    monkeypatch.setattr(mac, "sorted", sort, raising=False)
    rows = client.get_board_ranking(top_n=None, ascending=True)
    assert [row["code"] for row in rows] == ["zero", "nan", "rise"]
    assert observed == [([0.0, 0.0, 10.0], False)]


def test_extension_board_ranking_validation_precedes_fetch_and_preserves_custom_error(monkeypatch):
    client = client_with_boards([])
    client.get_board_list = lambda *args: pytest.fail("invalid sort must fail before I/O")
    failure = ValueError("replacement error")
    messages = []
    monkeypatch.setattr(mac, "ValueError", lambda message: messages.append(message) or failure, raising=False)
    with pytest.raises(ValueError) as caught:
        client.get_board_ranking(sort_by="unsupported")
    assert caught.value is failure
    assert "'unsupported'" in messages[0] and "main_net_amount" in messages[0]


def test_extension_board_row_preserves_summary_hook_rounding_and_key_order():
    client, calls = client_with_boards([]), []
    client.get_board_summary = lambda code: calls.append(code) or summary(12, 8, -3)
    row = client._board_row(board("x"), 1.234)
    assert row["change_pct"] == 1.23 and row["amount"] == 12 and row["main_net_amount"] == -3
    assert calls == ["x"]
    assert list(row) == ["code", "name", "change_pct", "amount", "vol", "main_net_amount", "up_count",
                         "down_count", "member_count"]


def test_extension_board_history_validates_days_then_fetches_before_target_conversion(monkeypatch):
    events = []
    client = client_with_boards([])
    client.get_board_list = lambda kind: events.append("boards") or []
    monkeypatch.setattr(mac, "_ymd_to_date", lambda value: pytest.fail("empty board result returns first"))
    with pytest.raises(ValueError, match="days 必须 >= 1"):
        client.get_board_change_ranking(days=0)
    assert events == []
    assert client.get_board_change_ranking(target_date=object()) == []
    assert events == ["boards"]


def test_extension_board_history_keeps_short_window_duplicate_dates_and_last_eligible_occurrence():
    client = client_with_boards([board("x")])
    calls = []
    bars = [bar(3, 15), bar(1, 10), bar(2, 12), bar(2, 14)]
    client.get_stock_kline = lambda **options: calls.append(options) or bars
    rows = client.get_board_change_ranking(target_date=20261102, days=20)
    assert rows == [dict(code="x", name="board-x", close_end=14, close_start=10, change_pct=40.0)]
    assert calls == [dict(market=90, code="x", period=mac.Period.DAILY, count=30, adjust=mac.Adjust.NONE)]
    assert bars[0].datetime.date() == date(2026, 11, 3)


def test_extension_board_history_catches_only_transport_errors_and_propagates_original_other_failure():
    client = client_with_boards([board("missing"), board("ok")])
    failure = RuntimeError("custom source failure")

    def fetch(**options):
        if options["code"] == "missing":
            raise mac.TransportError("skip this board")
        return [bar(1, 10), bar(2, 12)]

    client.get_stock_kline = fetch
    assert [row["code"] for row in client.get_board_change_ranking(days=1)] == ["ok"]

    def fail(**options):
        raise failure

    client.get_stock_kline = fail
    with pytest.raises(RuntimeError) as caught:
        client.get_board_change_ranking(days=1)
    assert caught.value is failure


def test_extension_board_history_keeps_zero_start_skip_stable_sort_and_negative_top():
    client = client_with_boards([board("zero"), board("first"), board("second"), board("fall")])
    client.get_stock_kline = lambda **options: [bar(1, 0 if options["code"] == "zero" else 10),
                                               bar(2, 9 if options["code"] == "fall" else 12)]
    rows = client.get_board_change_ranking(days=1, top_n=-1)
    assert [row["code"] for row in rows] == ["first", "second"]
    assert [row["change_pct"] for row in rows] == [20.0, 20.0]
    rows = client.get_board_change_ranking(days=1, ascending=True, top_n=1)
    assert [row["code"] for row in rows] == ["fall"]


def test_extension_board_history_preserves_custom_enumerate_positions(monkeypatch):
    client = client_with_boards([board("x")])
    bars = [bar(1, 10), bar(2, 15), bar(3, 20)]
    client.get_stock_kline = lambda **options: bars
    monkeypatch.setattr(mac, "enumerate", lambda values: iter([(0, values[0]), (1, values[2])]), raising=False)
    rows = client.get_board_change_ranking(target_date=20261103, days=1)
    assert rows[0]["close_end"] == 15 and rows[0]["change_pct"] == 50.0
