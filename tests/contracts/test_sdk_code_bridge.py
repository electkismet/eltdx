"""Authored directory and selection contracts; no collection/execution yet."""
from __future__ import annotations

from types import SimpleNamespace

import pytest

from eltdx.api.codes import CodeApi
from eltdx.models.security import SecurityCode


def _row(number, category, name="TEST"):
    return SecurityCode("sh", 1, number, name, 100, 2, 10.0, 1.0,
                        b"0000", b"0000", b"0000", category, "source", "none", "source")


class NativeCodes:
    def __init__(self):
        self.calls = []
        self.rows = [_row("600001", "a_share"), _row("900001", "b_share"),
                     _row("510001", "etf"), _row("000001", "index"),
                     _row("600002", "a_share", "*ST TEST")]

    def sdk_security_request(self, command, payload):
        self.calls.append((command, payload))
        return 17 if command == 0x044E else self.rows

    def sdk_select_codes(self, rows, selection):
        self.calls.append((selection, rows))
        assert rows == [(row.category, row.name) for row in self.rows]
        return {"stocks": [0, 1, 4], "a_shares": [0, 4], "etfs": [2],
                "indices": [3], "st": [4]}[selection]

    def execute(self, *_args, **_kwargs):
        pytest.fail("native directory calls must use shared SDK entry points")


def test_native_code_count_and_list_preserve_parameters_and_sink() -> None:
    transport = NativeCodes()
    sink_rows = []

    def sink(rows):
        sink_rows.append(rows)

    api = CodeApi(transport, native_services=True, metadata_sink=sink)
    assert api.count("深市") == 17
    assert api.list("sh", start=7, limit=0) is transport.rows
    assert transport.calls == [(0x044E, {"market": "深市"}),
                               (0x044D, dict(market="sh", start=7, limit=0))]
    assert sink_rows == [transport.rows]


@pytest.mark.parametrize("method,selected", [("stocks", [0, 1, 4]), ("a_shares", [0, 4]),
                                           ("etfs", [2]), ("indices", [3])])
def test_native_code_filters_keep_original_row_objects(method, selected) -> None:
    transport = NativeCodes()
    api = CodeApi(transport, native_services=True)
    api.all = lambda market: transport.rows
    result = getattr(api, method)("sh")
    assert len(result) == len(selected)
    assert all(value is transport.rows[index] for value, index in zip(result, selected))
    assert transport.calls[0][0] == method


def test_native_stock_counts_are_lengths_of_shared_selection() -> None:
    transport = NativeCodes()
    api = CodeApi(transport, native_services=True)
    api.all = lambda market: transport.rows
    assert api.stock_count("sh") == 3
    assert api.a_share_count("sh") == 2
    assert [call[0] for call in transport.calls] == ["stocks", "a_shares"]


def test_native_all_market_ids_keep_market_scan_order() -> None:
    transport = NativeCodes()
    api = CodeApi(transport, native_services=True)
    scanned = []

    def all_rows(market):
        scanned.append(market)
        return transport.rows if market == "sh" else []

    api.all = all_rows
    assert api.all_stocks() == [transport.rows[index].full_code for index in (0, 1, 4)]
    assert scanned == ["sh", "sz", "bj"]


def test_native_st_filter_keeps_name_only_after_latest_stock_extension() -> None:
    transport = NativeCodes()
    api = CodeApi(transport, native_services=True)
    api.latest_stock_list = lambda market=None: transport.rows
    assert api.latest_st() == [transport.rows[4]]
    assert api.st() == [transport.rows[4]]


def test_native_filters_keep_custom_row_adapter_and_unicode_name_behavior() -> None:
    transport = NativeCodes()
    rows = [SimpleNamespace(category="a_share", name="\x1cſt股份"),
            SimpleNamespace(category="b_share", name="ordinary")]
    api = CodeApi(transport, native_services=True)
    api.all = lambda market: rows
    assert api.stocks("sh") == rows
    api.latest_stock_list = lambda market=None: rows
    assert api.latest_st() == rows[:1]
    assert transport.calls == []


def test_native_empty_filter_does_not_call_stateless_native_selector() -> None:
    transport = NativeCodes()
    api = CodeApi(transport, native_services=True)
    api.all = lambda market: []
    assert api.stocks("sh") == []
    assert not transport.calls


def test_code_list_keeps_custom_untyped_response_and_sink() -> None:
    result = object()
    seen = []

    class CustomTransport:
        def execute(self, command, payload):
            assert command == 0x044D and payload["limit"] == 0
            return result

    def sink(value):
        seen.append(value)

    assert CodeApi(CustomTransport(), metadata_sink=sink).list("sh", limit=0) is result
    assert seen == [result]


@pytest.mark.parametrize("method,category", [("all_stocks", "b_share"), ("all_a_shares", "a_share"),
                                            ("all_etfs", "etf"), ("all_indices", "index")])
def test_custom_identifier_selection_keeps_per_row_attribute_order(method, category) -> None:
    events = []

    class CustomRow:
        def __init__(self, number):
            self.number = number

        @property
        def category(self):
            events.append((self.number, "category"))
            return category

        @property
        def full_code(self):
            events.append((self.number, "full_code"))
            if self.number == 1:
                raise ValueError("first full_code fails before reading second category")
            return "sh600001"

    transport = NativeCodes()
    api = CodeApi(transport, native_services=True)
    api.all_markets = lambda: [CustomRow(1), CustomRow(2)]
    with pytest.raises(ValueError, match="first full_code"):
        getattr(api, method)()
    assert events == [(1, "category"), (1, "full_code")]
    assert transport.calls == []


def test_exact_security_row_with_custom_identifier_keeps_interleaved_selection() -> None:
    transport = NativeCodes()
    first = _row("600001", "a_share")
    second = _row("600002", "a_share")

    class MutatingIdentifier:
        def __format__(self, spec):
            object.__setattr__(second, "category", "etf")
            return "600001"

    object.__setattr__(first, "code", MutatingIdentifier())
    api = CodeApi(transport, native_services=True)
    api.all_markets = lambda: [first, second]
    assert api.all_a_shares() == ["sh600001"]
    assert transport.calls == []


def test_custom_category_count_keeps_original_nonboolean_equality_sum() -> None:
    class NumericEquality:
        def __eq__(self, other):
            return 7 if other == "a_share" else 0

    transport = NativeCodes()
    api = CodeApi(transport, native_services=True)
    api.all = lambda market: [SimpleNamespace(category=NumericEquality())]
    assert api.a_share_count("sh") == 7
    assert transport.calls == []
