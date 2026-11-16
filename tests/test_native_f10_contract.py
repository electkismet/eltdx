"""F10 native compatibility sources, deferred until the complete source gate."""
from __future__ import annotations

from collections.abc import Mapping
from types import SimpleNamespace

import pytest

from eltdx.f10 import client as f10


def test_f10_shared_table_parser_preserves_duplicate_collisions_and_identity():
    marker = object()
    body = {"Params": [marker]}
    source_row = [1, 2, 3, marker, 5]
    source_table = {"ColName": ["x", "x", "x__2", ""], "Content": [source_row]}
    raw = {"ErrorCode": "17", "ResultSets": [source_table, {}], "extra": marker}
    response = f10.parse_tqlex_response("Example", body, raw)
    assert response.entry == "Example"
    assert response.request_body is body
    assert response.error_code == 17
    assert not response.ok
    assert response.raw is not raw
    assert response.raw["extra"] is marker
    first, second = response.result_sets
    assert (first.key, second.key) == ("table0", "table1")
    assert first.columns == ("x", "x", "x__2", "")
    assert first.rows == ({"x": 1, "x__2": 3, "col_3": marker, "col_4": 5},)
    assert list(first.rows[0]) == ["x", "x__2", "col_3", "col_4"]
    assert [(cell.name, cell.index) for cell in first.row_cells[0]] == [
        ("x", 0), ("x", 1), ("x__2", 2), ("col_3", 3), ("col_4", 4)]
    assert first.raw["Content"] is source_table["Content"]


@pytest.mark.parametrize("raw,message", [
    ({"ResultSets": "invalid"}, "TQLEX ResultSets must be an array"),
    ({"ResultSets": [1]}, "TQLEX ResultSet must be an object"),
    ({"ResultSets": [{"Content": "invalid"}]}, "TQLEX ResultSet Content must be an array"),
    ({"ResultSets": [{"ColName": 1}]}, "TQLEX ColName must be an array"),
    ({"ResultSets": [{"ColDes": b"invalid"}]}, "TQLEX ColDes must be an array"),
])
def test_f10_parser_preserves_exact_protocol_errors(raw, message):
    with pytest.raises(f10.ProtocolError) as caught:
        f10.parse_tqlex_response("Example", {}, raw)
    assert str(caught.value) == message


@pytest.mark.parametrize("value", [None, False, 0, "", [], {}, b""])
def test_f10_legacy_falsey_containers_keep_empty_defaults(value):
    assert f10.parse_tqlex_response("Example", {}, {"ResultSets": value}).rows == ()
    response = f10.parse_tqlex_response("Example", {}, {
        "ResultSets": [{"ResultSetKey": value, "ColName": value, "Content": value}]})
    assert response.first_table.key == "table0"
    assert response.first_table.columns == ()
    assert response.rows == ()


def test_f10_columns_use_python_str_and_colname_precedence():
    marker = SimpleNamespace(value="custom")
    raw = {"ColName": [None, True, [1], marker], "ColDes": [{"Name": "ignored"}],
           "Content": ["text", {"key": "value"}, None, [1, 2]]}
    response = f10.parse_tqlex_response("Example", {}, {"ResultSets": [raw]})
    assert response.first_table.columns == ("None", "True", "[1]", str(marker))
    assert response.rows[0] == {"None": "text"}
    assert response.rows[1]["None"] is raw["Content"][1]
    assert response.rows[2] == {"None": None}
    assert response.rows[3] == {"None": 1, "True": 2}
    assert f10._columns({"ColDes": [{"Name": "x"}, {}, None, [1]]}) == ["x", "", "None", "[1]"]


def test_f10_parser_keeps_custom_table_parser_and_constructor_order(monkeypatch):
    events = []
    marker = object()

    class ErrorCode:
        def __int__(self):
            events.append("error")
            return 7

    def parse(raw, index):
        events.append((raw, index))
        return marker

    def constructor(**values):
        events.append("response")
        return values

    monkeypatch.setattr(f10, "_parse_result_set", parse)
    monkeypatch.setattr(f10, "F10Response", constructor)
    result = f10.parse_tqlex_response("Example", {}, {"ResultSets": [1, 2], "ErrorCode": ErrorCode()})
    assert events == [(1, 0), (2, 1), "error", "response"]
    assert result["result_sets"] == (marker, marker)
    assert result["error_code"] == 7


def test_f10_table_parser_keeps_live_columns_cells_and_row_hooks(monkeypatch):
    marker = object()
    events = []
    monkeypatch.setattr(f10, "_columns", lambda raw: ["custom"])
    monkeypatch.setattr(f10, "_row_values", lambda row: [marker])

    def cell(**values):
        events.append(values)
        return SimpleNamespace(**values)

    monkeypatch.setattr(f10, "F10Cell", cell)
    monkeypatch.setattr(f10, "_row_dict", lambda cells: {"cells": cells})
    result = f10._parse_result_set({"Content": [1, 2]}, 8)
    assert result.key == "table8"
    assert result.columns == ("custom",)
    assert len(events) == 2
    assert all(values == {"name": "custom", "value": marker, "index": 0} for values in events)
    assert result.rows[0]["cells"] is result.row_cells[0]


def test_f10_projection_preserves_three_live_name_reads_and_collision_order():
    events = []

    class Cell:
        @property
        def name(self):
            events.append("name")
            return "x"

        @property
        def value(self):
            events.append("value")
            return marker

    marker = object()
    assert f10._row_dict([Cell(), Cell()]) == {"x": marker, "x__2": marker}
    assert events == ["name", "name", "name", "value"] * 2
    assert f10._row_dict([SimpleNamespace(name="x", value=1), SimpleNamespace(name="x__2", value=2),
                          SimpleNamespace(name="x", value=3)]) == {"x": 1, "x__2": 3}


def test_f10_error_code_keeps_python_integer_size_and_exception_identity():
    number = 10**100
    assert f10.parse_tqlex_response("Example", {}, {"ErrorCode": str(number)}).error_code == number
    failure = ValueError("custom integer conversion")

    class Broken:
        def __int__(self):
            raise failure

    with pytest.raises(ValueError) as caught:
        f10.parse_tqlex_response("Example", {}, {"ErrorCode": Broken()})
    assert caught.value is failure


def test_f10_custom_mapping_access_and_raw_copy_stay_visible():
    events = []

    class Source(Mapping):
        def __getitem__(self, key):
            events.append(("item", key))
            if key == "ResultSets":
                return []
            if key == "ErrorCode":
                return None
            raise KeyError(key)

        def __iter__(self):
            events.append("iter")
            return iter(("ResultSets", "ErrorCode"))

        def __len__(self):
            return 2

    response = f10.parse_tqlex_response("Example", {}, Source())
    assert response.raw == {"ResultSets": [], "ErrorCode": None}
    assert events[:2] == [("item", "ResultSets"), ("item", "ErrorCode")]
    assert "iter" in events


def test_f10_call_and_params_keep_instance_overrides_and_original_body(monkeypatch):
    client = f10.F10Client()
    marker = object()
    calls = []
    monkeypatch.setattr(client, "_post", lambda entry, body: calls.append((entry, body)) or marker)
    body = []
    assert client.call("Example", body) is marker
    assert calls[-1][1] is body
    assert client.call("Example", params=(value for value in [1, 2])) is marker
    assert calls[-1] == ("Example", {"Params": [1, 2]})
    assert client.params("Example", 3, 4) is marker
    assert calls[-1] == ("Example", {"Params": [3, 4]})
    with pytest.raises(ValueError, match="pass either body or params, not both"):
        client.call("Example", {}, params=[])
    override = object()
    monkeypatch.setattr(client, "call", lambda entry, *, params: calls.append((entry, params)) or override)
    assert client.params("Other", 5) is override
    assert calls[-1] == ("Other", (5,))


@pytest.mark.parametrize("method,value,expected", [
    ("_us_code", "BRK.B", "B"), ("_us_code", "us.brk-b", "BRK-B"),
    ("_us_code", "74.中一", "中一"), ("_us_code", "ß", "SS"),
    ("_hk_code", "hk.700", "00700"), ("_hk_code", "123456", "123456"),
    ("_hk_code", "²", "0000²"), ("_bond_code", "bj123456", "123456"),
    ("_bond_sc", "BJ123456", "0"), ("_bond_sc", "", "1"),
])
def test_f10_code_profiles_keep_python_unicode_and_legacy_rules(method, value, expected):
    assert getattr(f10, method)(value) == expected


@pytest.mark.parametrize("method,value,label", [
    ("_us_code", "--", "US"), ("_us_code", "BRK B", "US"),
    ("_hk_code", "hk.", "HK"), ("_bond_code", "SH123456", "bond"),
])
def test_f10_code_errors_keep_exact_original_repr(method, value, label):
    with pytest.raises(f10.ProtocolError) as caught:
        getattr(f10, method)(value)
    assert str(caught.value) == f"invalid {label} code: {value!r}"


def test_f10_code6_keeps_live_split_code_hook(monkeypatch):
    marker = object()
    monkeypatch.setattr(f10, "split_code", lambda code: (1, "sh", marker))
    assert f10._code6("ignored") is marker


def test_f10_section_error_sorted_choices_and_custom_selector_hook(monkeypatch):
    with pytest.raises(ValueError) as caught:
        f10._require_section("missing", ("b", "a"))
    assert str(caught.value) == "unknown section: 'missing'; expected one of: a, b"
    f10._require_section("a", ("b", "a"))
    calls = []
    marker = object()
    table = {"custom": marker}
    monkeypatch.setattr(f10, "_require_section", lambda section, valid: calls.append((section, valid)))
    assert f10._section_request("custom", table) is marker
    assert calls == [("custom", ("custom",))]


@pytest.mark.parametrize("entry,target", [
    ("CWServ.cfg_fx_lbtt", "ladder"), ("CWServ.skef10_mg_idreq", "international"),
    ("CWServ.skef10_bond_zjzx", "international"), ("CWServ.qhf9_sp_kcbg_kcbg", "international"),
    ("CWServ.test 中文", "ordinary"),
])
def test_f10_prepare_request_preserves_routes_json_and_http_timeout(monkeypatch, entry, target):
    from io import BytesIO
    from urllib.parse import urlencode

    calls = []

    def opener(request, *, timeout):
        calls.append((request, timeout))
        return BytesIO(b'{"ErrorCode":0,"ResultSets":[]}')

    client = f10.F10Client(base_url="http://ordinary.test/TQLEX", limit_board_ladder_base_url="http://ladder.test/TQLEX",
                           mg_hk_base_url="http://international.test/TQLEX", timeout=0.75)
    monkeypatch.setattr(f10, "open_ipv4_first", opener)
    result = client.call(entry, {"Params": ["中文", -1]})
    request, timeout = calls[0]
    assert request.full_url == f"http://{target}.test/TQLEX?{urlencode({'Entry': entry})}"
    assert request.data == '{"Params":["中文",-1]}'.encode()
    assert request.method == "POST"
    assert timeout == 0.75
    assert result.ok


def test_f10_custom_request_builder_exception_is_not_http_wrapped(monkeypatch):
    client = f10.F10Client()
    failure = TimeoutError("builder")

    def request(*args, **kwargs):
        raise failure

    monkeypatch.setattr(f10, "Request", request)
    with pytest.raises(TimeoutError) as caught:
        client.params("Example")
    assert caught.value is failure


def test_f10_legacy_network_timeout_keeps_transport_error_cause(monkeypatch):
    client = f10.F10Client()
    failure = TimeoutError("wire")

    def open_request(*args, **kwargs):
        raise failure

    monkeypatch.setattr(f10, "open_ipv4_first", open_request)
    with pytest.raises(f10.TransportError) as caught:
        client.params("Example")
    assert str(caught.value) == "TQLEX request timed out for Example"
    assert caught.value.__cause__ is failure
