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


@pytest.mark.parametrize("method,code,options,entry,arguments", [
    ("stock_info", "sh600000", {}, "CWServ.tdxf10_gg_comreq", ("gpquery", "600000")),
    ("company_profile", "sh600000", {"section": "9"}, "CWServ.tdxf10_gg_gsgk", ("9", "600000", "")),
    ("profit_forecast", "sh600000", {"section": "ycpjyjbg"}, "CWServ.tdxf10_gg_ybpj", ("600000", "ycpjyjbg")),
    ("mg_name", "BRK.B", {}, "CWServ.skef10_mg_idreq", ("name", "B")),
    ("mg_tips", "us.AAPL", {"section": "gsgy"}, "CWServ.skef10_mg_zxdt", ("gsgy", "AAPL", "")),
    ("mg_dividend", "AAPL", {"section": "cf"}, "CWServ.skef10_mg_fhrz", ("AAPL", "cf")),
    ("mg_rating", "AAPL", {"section": "open"}, "CWServ.skef10_mg_gspj", ("open", "AAPL")),
    ("hk_topic", "700", {}, "CWServ.skef10_hk_rdtc", ("001", "00700", "", "0", "0")),
    ("hk_shareholders", "123456", {"section": "13"}, "CWServ.skef10_hk_gdcg", ("13", "123456", "")),
    ("sb_profile", "bj430047", {"section": "custom"}, "CWServ.skef10_sb_gsgk", ("430047", "custom")),
    ("bond_profile", "sh113043", {}, "CWServ.skef10_bond_kzz_zqgk", ("sdcyrbgq", "113043", "")),
    ("gz_profile", "sz123001", {}, "CWServ.skef10_bond_gz_zjgk", ("zqgk", "123001", "0")),
    ("qyz_review", "113043", {}, "CWServ.skef10_bond_qyz_gsps", ("113043", "1")),
    ("gzhg_quotes", "sh204001", {"section": "zqsj"}, "CWServ.skef10_bond_gzhg_zqxx", ("zqsj", "204001")),
    ("zczq_finance", "sz123001", {}, "CWServ.skef10_bond_zczqh_cwfx", ("t", "123001", "0")),
    ("kjhz_finance", "sz123001", {}, "CWServ.skef10_bond_kjhz_cwfx", ("t", "123001")),
    ("bond_bulletin", "sz123001", {}, "CWServ.skef10_bond_zjzx", ("123001", "0")),
    ("hy_index", "881319", {}, "CWServ.skef10_hy_idsy", ("002", "881319")),
    ("hy_fund_holdings", "881319", {"section": "open"}, "CWServ.skef10_hy_hysj_jjzb", ("open", "881319", "")),
    ("qh_inventory", "rb", {}, "CWServ.qhf9_sp_kcbg_kcbg", ("1", "rb")),
])
def test_f10_fixed_templates_preserve_original_public_requests(method, code, options, entry, arguments):
    calls = []
    result = object()

    class Client(f10.F10Client):
        def params(self, selected_entry, *selected_arguments):
            calls.append((selected_entry, selected_arguments))
            return result

    assert getattr(Client(), method)(code, **options) is result
    assert calls == [(entry, arguments)]


def test_f10_fixed_template_resolves_params_before_code_and_market(monkeypatch):
    events = []
    marker = object()
    converted = object()
    section = object()
    market = object()

    class Client(f10.F10Client):
        @property
        def params(self):
            events.append("params lookup")
            return lambda entry, *values: events.append((entry, values)) or marker

    def code(value):
        events.append(("code", value))
        return converted

    def sc(value):
        events.append(("market", value))
        return market

    monkeypatch.setattr(f10, "_bond_code", code)
    monkeypatch.setattr(f10, "_bond_sc", sc)
    assert Client().gz_profile(marker, section) is marker
    assert events == ["params lookup", ("code", marker), ("market", marker),
                      ("CWServ.skef10_bond_gz_zjgk", (section, converted, market))]


def test_f10_fixed_template_keeps_captured_params_when_converter_changes_it(monkeypatch):
    client = f10.F10Client()
    calls = []
    client.params = lambda *args: calls.append(("old", args))

    def convert(value):
        client.params = lambda *args: calls.append(("new", args))
        return "converted"

    monkeypatch.setattr(f10, "_us_code", convert)
    client.mg_name("first")
    client.mg_name("second")
    assert calls == [("old", ("CWServ.skef10_mg_idreq", "name", "converted")),
                     ("new", ("CWServ.skef10_mg_idreq", "name", "converted"))]


def test_f10_fixed_template_validates_before_params_or_conversion(monkeypatch):
    events = []
    failure = LookupError("section hook")

    class Client(f10.F10Client):
        @property
        def params(self):
            events.append("params")
            raise AssertionError("params lookup must follow section validation")

    def require(section, choices):
        events.append((section, choices))
        raise failure

    monkeypatch.setattr(f10, "_require_section", require)
    monkeypatch.setattr(f10, "_us_code", lambda value: events.append("code"))
    with pytest.raises(LookupError) as caught:
        Client().mg_tips("AAPL", "bad")
    assert caught.value is failure
    assert events == [("bad", ("zxdt", "sypl", "gsgy", "zysj"))]


@pytest.mark.parametrize("method,expected", [
    ("hy_profile", ("CWServ.skef10_hy_hyzl_hyjs",)),
    ("qh_inventory", ("CWServ.qhf9_sp_kcbg_kcbg", "1")),
])
def test_f10_fixed_template_preserves_unconverted_symbol_objects(method, expected):
    marker = object()
    client = f10.F10Client()
    client.params = lambda *args: args
    assert getattr(client, method)(marker) == expected + (marker,)


def test_f10_fixed_template_keeps_original_code_helper_exception(monkeypatch):
    client = f10.F10Client()
    failure = RuntimeError("conversion")
    events = []

    def convert(value):
        raise failure

    monkeypatch.setattr(f10, "_bond_code", convert)
    monkeypatch.setattr(f10, "_bond_sc", lambda value: events.append("market"))
    with pytest.raises(RuntimeError) as caught:
        client.gz_profile("value")
    assert caught.value is failure
    assert events == []


def test_f10_fixed_template_keeps_open_section_identity():
    section = object()
    client = f10.F10Client()
    client.params = lambda entry, *args: args
    values = client.hk_tips("700", section)
    assert values[0] is section
    assert values[1] == "00700"
