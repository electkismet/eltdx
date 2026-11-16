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


@pytest.mark.parametrize("method,section,expected", [
    ("mg_capital", "001", ("001", "AAPL", 1)),
    ("mg_capital", "002", ("002", "AAPL", "page")),
    ("mg_shareholders", "006", ("006", "AAPL", 1, 10)),
    ("mg_shareholders", "custom", ("custom", "AAPL", "page", "size")),
    ("mg_market", "001", ("001", "AAPL", "page", "size")),
    ("mg_market", "003", ("003", "AAPL", 0, 0)),
])
def test_f10_page_branches_keep_raw_objects_and_fixed_numbers(method, section, expected):
    client = f10.F10Client()
    client.params = lambda entry, *values: values
    options = {"page": "page"}
    if method != "mg_capital":
        options["page_size"] = "size"
    assert getattr(client, method)("AAPL", section, **options) == expected


@pytest.mark.parametrize("method,attribute,code,entry", [
    ("mg_finance_indicator", "_MG_CWZB_SECTIONS", "AAPL", "CWServ.skef10_mg_cwzb"),
    ("hk_finance_indicator", "_HK_CWZB_SECTIONS", "00700", "CWServ.skef10_hk_cwzb"),
    ("hk_finance", "_HK_CWFX_SECTIONS", "00700", "CWServ.skef10_hk_cwfx"),
])
def test_f10_mapped_sections_use_live_table_and_original_tuple_hook(monkeypatch, method, attribute, code, entry):
    events = []
    value = object()
    table = {"custom": value}
    original_require = f10._require_section

    def require(section, choices):
        events.append(("require", section, choices))
        original_require(section, choices)

    def tuple_hook(selected):
        events.append(("tuple", selected))
        # The source resolves _require_section before evaluating tuple(...).
        monkeypatch.setattr(f10, "_require_section", lambda *args: events.append("replacement"))
        return tuple(selected)

    monkeypatch.setattr(f10, attribute, table)
    monkeypatch.setattr(f10, "tuple", tuple_hook, raising=False)
    monkeypatch.setattr(f10, "_require_section", require)
    client = f10.F10Client()
    client.params = lambda *args: args
    result = getattr(client, method)(code, "custom")
    assert result == (entry, value, code)
    assert events == [("tuple", table), ("require", "custom", ("custom",))]


def test_f10_mapped_section_reloads_table_after_validation_before_code(monkeypatch):
    events = []
    original = {"custom": object()}
    selected = object()
    monkeypatch.setattr(f10, "_MG_CWZB_SECTIONS", original)

    def require(section, choices):
        events.append("require")
        monkeypatch.setattr(f10, "_MG_CWZB_SECTIONS", {"custom": selected})

    def convert(code):
        events.append("convert")
        monkeypatch.setattr(f10, "_MG_CWZB_SECTIONS", {})
        return code

    class Client(f10.F10Client):
        @property
        def params(self):
            events.append("params")
            return lambda *args: args

    monkeypatch.setattr(f10, "_require_section", require)
    monkeypatch.setattr(f10, "_us_code", convert)
    assert Client().mg_finance_indicator("AAPL", "custom") == ("CWServ.skef10_mg_cwzb", selected, "AAPL")
    assert events == ["require", "params", "convert"]


def test_f10_page_branch_compares_section_before_getting_params():
    events = []

    class Section:
        def __eq__(self, other):
            events.append(("equals", other))
            return False

    class Client(f10.F10Client):
        @property
        def params(self):
            events.append("params")
            return lambda entry, *args: args

    section = Section()
    marker = object()
    result = Client().mg_shareholders("AAPL", section, page=marker, page_size=marker)
    assert result[0] is section
    assert result[2] is marker and result[3] is marker
    assert events == [("equals", "006"), "params"]


def test_f10_page_branch_keeps_equality_failure_and_skips_params():
    failure = ArithmeticError("selector")

    class Section:
        def __eq__(self, other):
            raise failure

    class Client(f10.F10Client):
        @property
        def params(self):
            raise AssertionError("must not access params")

    with pytest.raises(ArithmeticError) as caught:
        Client().mg_shareholders("AAPL", Section())
    assert caught.value is failure


@pytest.mark.parametrize("method,code,options,entry,values", [
    ("hk_pepb_band", "700", {"sjfw": "2Y", "tab_value": "pe", "hqzq": "week"}, "CWServ.skef10_hk_sdfx_pepb_band", ("001", "00700", "2Y", "pe", "week")),
    ("hk_valuation", "700", {"item_id": "raw"}, "CWServ.skef10_hk_sdfx_gzfx", ("00700", "raw")),
    ("hk_analyst_forecast", "700", {"start": " 2025-01-02 ", "end": "2026-01-03"}, "CWServ.skef10_hk_sdfx_fxysyfx", ("00700", "20250102", "20260103")),
    ("allotment_details", "sh600000", {"date": "unconverted"}, "CWServ.tdxf10_gg_fhrz_zfhpmx", ("zfpg", "600000", "unconverted")),
    ("finance_report", "sh600000", {"report_type": "custom"}, "CWServ.tdxf10_gg_cwfx", ("600000", "custom", "")),
    ("finance_diagnosis", "sh600000", {"section": "custom", "scope": "raw"}, "CWServ.tdxf10_gg_cwzd", ("custom", "600000", "raw")),
    ("stock_score", "sh600000", {"arg": "raw"}, "CWServ.tdxf10_gg_ggzp", ("pf", "600000", "raw", "")),
    ("governance", "sh600000", {"arg": "raw"}, "CWServ.tdxf10_gg_zbyz", ("gljy", "600000", "raw")),
    ("hy_stage_rise", "881319", {"flag": "n"}, "CWServ.skef10_hy_jdzf", ("881319", "n")),
    ("hy_capital_stats", "881319", {"section": "001"}, "CWServ.skef10_hy_hysj_bxzjtj", ("001", "881319", "n")),
    ("hy_capital_stats", "881319", {"section": "unknown"}, "CWServ.skef10_hy_hysj_bxzjtj", ("unknown", "881319", "")),
    ("qh_basis_spread", "rb", {"period": "custom"}, "CWServ.qhf9_sp_jcjcfx", ("001", "rb", "custom", "", "")),
    ("qh_warehouse_receipt", "rb", {"date": " 2026-02-03 "}, "CWServ.qhf9_sp_ckrb_ckrb", ("1", "rb", "20260203")),
])
def test_f10_named_input_templates_keep_original_payloads(method, code, options, entry, values):
    client = f10.F10Client()
    client.params = lambda *args: args
    assert getattr(client, method)(code, **options) == (entry, *values)


@pytest.mark.parametrize("method,section,entry,values", [
    ("hy_summary", "hqtj", "CWServ.skef10_hy_hqtj", ("symbol",)),
    ("hy_summary", "hyzq", "CWServ.skef10_hy_hyzq", ("symbol",)),
    ("hy_summary", "hygy", "CWServ.skef10_hy_hygy", ("symbol",)),
    ("hy_summary", "zszfb", "CWServ.skef10_hy_zszfb", ("symbol",)),
    ("hy_members_rank", "cwzkpm", "CWServ.skef10_hy_hydw_cwzkpm", ("002", "symbol", "", "e", "")),
    ("hy_members_rank", "gsgmpm", "CWServ.skef10_hy_hydw_gsgmpm", ("symbol",)),
    ("hy_members_rank", "gzsppm", "CWServ.skef10_hy_hydw_gzsppm", ("01", "symbol", "o")),
    ("hy_members_rank", "scbxpm", "CWServ.skef10_hy_hydw_scbxpm", ("scpm", "", "symbol")),
    ("hy_members_rank", "zysrpm", "CWServ.skef10_hy_hydw_zysrpm", ("001", "symbol", "")),
    ("qh_variety_info", "pzjj", "CWServ.qhf9_sp_pzzl_pzjj", ("symbol", "1")),
    ("qh_variety_info", "hyjj", "CWServ.qhf9_sp_pzzl_hyjj", ("symbol",)),
    ("qh_variety_info", "xgzq", "CWServ.qhf9_sp_pzzl_xgzq", ("symbol",)),
    ("qh_index_info", "zsgk", "CWServ.qhf9_gz_zszl_zsgk", ("1", "symbol")),
    ("qh_index_info", "custom", "CWServ.qhf9_gz_zszl_custom", ("2", "symbol")),
])
def test_f10_sector_templates_keep_all_entry_selections(method, section, entry, values):
    client = f10.F10Client()
    client.params = lambda *args: args
    assert getattr(client, method)("symbol", section) == (entry, *values)


@pytest.mark.parametrize("section,tail", [
    ("cjcc", ("20260102",)), ("cjjg", ("20260102", "")),
    ("jcgc", ("20260102", "20260203", "")),
    ("ccjj", ("20260102", "20260203", "")),
    ("ykfx", ("20260102", "20260203", "")),
])
@pytest.mark.parametrize("index,prefix", [(False, "sp"), (True, "gz")])
def test_f10_position_report_preserves_dates_and_index_route(section, tail, index, prefix):
    client = f10.F10Client()
    client.params = lambda *args: args
    assert client.qh_position_report("IF", section, date="2026-01-02", end_date=" 2026-02-03 ", index=index) == (
        f"CWServ.qhf9_{prefix}_ccbg_{section}", "1", "IF", *tail)


@pytest.mark.parametrize("method", ["hy_summary", "qh_variety_info", "qh_position_report"])
def test_f10_sector_invalid_lookup_keeps_key_error(method):
    client = f10.F10Client()
    with pytest.raises(KeyError) as caught:
        getattr(client, method)("code", "bad")
    assert caught.value.args == ("bad",)
    with pytest.raises(TypeError, match="unhashable"):
        getattr(client, method)("code", [])


def test_f10_industry_rank_invalid_selector_keeps_original_value_error():
    with pytest.raises(ValueError) as caught:
        f10.F10Client().hy_members_rank("code", "bad")
    assert str(caught.value) == "unknown hy_members_rank section: 'bad'"


def test_f10_position_report_evaluation_order_keeps_unused_end_date():
    events = []

    class Date:
        def __init__(self, label):
            self.label = label

        def strip(self):
            events.append((self.label, "strip"))
            return self

        def replace(self, old, new):
            events.append((self.label, "replace", old, new))
            return self.label

    class Index:
        def __bool__(self):
            events.append("index")
            return True

    class Client(f10.F10Client):
        @property
        def params(self):
            events.append("params")
            return lambda *args: args

    result = Client().qh_position_report("IF", "cjcc", date=Date("first"), end_date=Date("unused"), index=Index())
    assert result == ("CWServ.qhf9_gz_ccbg_cjcc", "1", "IF", "first")
    assert events == ["index", ("first", "strip"), ("first", "replace", "-", ""),
                      ("unused", "strip"), ("unused", "replace", "-", ""), "params"]


def test_f10_position_invalid_selector_precedes_date_conversion():
    class Date:
        def strip(self):
            raise AssertionError("must not convert dates")

    with pytest.raises(KeyError):
        f10.F10Client().qh_position_report("IF", "bad", date=Date(), end_date=Date())


def test_f10_weights_converts_unused_date_before_params():
    events = []

    class Date:
        def strip(self):
            events.append("date")
            return "2026-01-02"

    class Client(f10.F10Client):
        @property
        def params(self):
            events.append("params")
            return lambda *args: args

    assert Client().qh_weights("IF", "cfghyfb", date=Date()) == ("CWServ.qhf9_gz_cfg_cfghyfb", "1", "IF")
    assert events == ["date", "params"]


def test_f10_dynamic_entry_concatenation_precedes_argument_conversion():
    events = []
    marker = object()

    class Section:
        def __radd__(self, prefix):
            events.append(("entry", prefix))
            return marker

        def __eq__(self, value):
            events.append(("equals", value))
            return False

    class Client(f10.F10Client):
        @property
        def params(self):
            events.append("params")
            return lambda *args: args

    assert Client().qh_index_info("IF", Section()) == (marker, "2", "IF")
    assert events == ["params", ("entry", "CWServ.qhf9_gz_zszl_"), ("equals", "zsgk")]


def test_f10_index_valuation_keeps_dynamic_selector_and_trimmed_dates():
    client = f10.F10Client()
    client.params = lambda *args: args
    assert client.qh_index_valuation(section="custom", start=" 2025-01-02 ", end="2026-03-04") == (
        "CWServ.qhf9_gz_zsgz_custom", "1", "000300", "20250102", "20260304")


def test_f10_forecast_preserves_leap_day_default_error(monkeypatch):
    from datetime import date

    class Date:
        @staticmethod
        def today():
            return date(2024, 2, 29)

    monkeypatch.setattr(f10, "date", Date)
    with pytest.raises(ValueError, match="day is out of range"):
        f10.F10Client().hk_analyst_forecast("700")


def test_f10_forecast_explicit_start_only_defaults_end(monkeypatch):
    from datetime import date

    class Date:
        @staticmethod
        def today():
            return date(2024, 2, 29)

    monkeypatch.setattr(f10, "date", Date)
    client = f10.F10Client()
    client.params = lambda *args: args
    assert client.hk_analyst_forecast("700", "2024-01-02") == (
        "CWServ.skef10_hk_sdfx_fxysyfx", "00700", "20240102", "20240229")


def test_f10_named_input_preserves_arbitrary_date_and_scope_objects():
    date = object()
    scope = object()
    client = f10.F10Client()
    client.params = lambda *args: args
    assert client.allotment_details("sh600000", date)[-1] is date
    assert client.finance_diagnosis("sh600000", scope=scope)[-1] is scope


def test_f10_news_inputs_keep_call_override_and_explicit_body():
    client = f10.F10Client()
    client.call = lambda *args: args
    marker = object()
    assert client.market_news(98765, page=-3, page_size=0) == (
        "CWServ.tdxi_zxxwsy", {"Params": ["98765", "-3", "0"]})
    assert client.channel_news(999, page=-4, query1=123, keyword=marker) == (
        "CWServ.tdxi_zxxwnews", {"Params": ["123", "999", "-4", "50", marker]})


def test_f10_news_conversion_order_keeps_live_int_and_str(monkeypatch):
    events = []
    client = f10.F10Client()
    client.call = lambda *args: args

    def convert_int(value):
        events.append(("int", value))
        return 7

    def convert_str(value):
        events.append(("str", value))
        return str(value)

    monkeypatch.setattr(f10, "int", convert_int, raising=False)
    monkeypatch.setattr(f10, "str", convert_str, raising=False)
    client.channel_news("channel", query1="query", page="page", page_size="size")
    assert events == [("str", "query"), ("int", "channel"), ("str", 7), ("str", "page"), ("str", "size")]


def test_f10_northbound_converts_page_twice_after_code(monkeypatch):
    events = []

    class Page:
        def __str__(self):
            events.append("page")
            return str(len(events))

    class Client(f10.F10Client):
        @property
        def params(self):
            events.append("params")
            return lambda *args: args

    monkeypatch.setattr(f10, "_code6", lambda value: events.append("code") or value)
    result = Client().northbound_holding("code", page=Page(), page_size=Page())
    assert result == ("CWServ.tdxf10_gg_zlcc", "code", "bszj", "", "3", "4", "5")
    assert events == ["params", "code", "page", "page", "page"]


def test_f10_company_finance_review_ignores_unused_conversions():
    class Unused:
        def __str__(self):
            raise AssertionError("unused parameter converted")

    client = f10.F10Client()
    client.params = lambda *args: args
    assert client.company_news("sh600000", "finance_review", page=-4,
                               keyword=Unused(), rating=Unused(), page_size=Unused()) == (
        "CWServ.tdxf10_gg_cwfx_cbdp", "600000", "-4")


def test_f10_topic_and_detail_preserve_raw_fields_and_only_stringify_selected_input():
    client = f10.F10Client()
    client.params = lambda *args: args
    marker = object()
    assert client.topic_compare("sh600000", 123, section=marker, sort_by=marker) == (
        "CWServ.tdxf10_gg_rdtc_gndb", marker, "600000", "123", marker)
    assert client.detail(marker, 987) == ("CWServ.tdxf10_gg_idreq", marker, "987")


@pytest.mark.parametrize("method,kind", [("announcements", "gg"), ("news", "xw"), ("roadshows", "ly")])
def test_f10_cache_aliases_keep_user_override(method, kind):
    client = f10.F10Client()
    marker = object()
    client.cache_list = lambda *args: args
    assert getattr(client, method)(marker) == (marker, kind)


def test_f10_cache_payload_keeps_live_qsid_and_identity_formatting(monkeypatch):
    events = []

    class Formatted:
        def __init__(self, name):
            self.name = name

        def __format__(self, spec):
            events.append((self.name, spec))
            return self.name

    monkeypatch.setattr(f10, "split_code", lambda code: (Formatted("market"), None, Formatted("number")))
    marker = object()
    monkeypatch.setattr(f10, "DEFAULT_QSID", marker)
    client = f10.F10Client()
    client.call = lambda *args: args
    assert client.cache_list("raw", "xw") == ("CWSearch.tzx_rcache", {
        "action": "get", "key": "xw:market_number", "bin": "1", "qsid": marker})
    assert events == [("market", ""), ("number", "")]


def test_f10_cache_invalid_kind_precedes_split_code(monkeypatch):
    monkeypatch.setattr(f10, "split_code", lambda code: pytest.fail("split must not run"))
    with pytest.raises(ValueError, match="kind must be one of: xw, gg, ly"):
        f10.F10Client().cache_list("code", "invalid")
    with pytest.raises(TypeError, match="unhashable"):
        f10.F10Client().cache_list("code", [])


def test_f10_cache_formatting_uses_special_method_lookup(monkeypatch):
    class Formatted:
        def __format__(self, spec):
            return "class-format"

    identity = Formatted()
    identity.__format__ = lambda spec: "instance-format"
    monkeypatch.setattr(f10, "split_code", lambda code: (identity, None, identity))
    monkeypatch.setattr(f10, "format", lambda *args: "module-format", raising=False)
    client = f10.F10Client()
    client.call = lambda *args: args
    assert client.cache_list("code")[1]["key"] == "gg:class-format_class-format"


def test_f10_cache_formatting_rejects_non_string_before_call(monkeypatch):
    class Invalid:
        def __format__(self, spec):
            return 123

    monkeypatch.setattr(f10, "split_code", lambda code: (Invalid(), None, "number"))
    client = f10.F10Client()
    client.call = lambda *args: pytest.fail("invalid format must prevent request")
    with pytest.raises(TypeError, match="__format__ must return a str"):
        client.cache_list("code")


def test_f10_news_channels_keeps_live_registry_and_sorted_hook(monkeypatch):
    registry = {9: "nine", 1: "one"}
    monkeypatch.setattr(f10, "NEWS_CHANNEL_LABELS", registry)
    assert f10.F10Client().news_channels() == [(1, "one"), (9, "nine")]
    monkeypatch.setattr(f10, "sorted", lambda values: list(values), raising=False)
    assert f10.F10Client().news_channels() == [(9, "nine"), (1, "one")]


@pytest.mark.parametrize("method,section,kwargs,expected", [
    ("shareholders", "institution_holdings", {"report_date": 123, "kind": "custom", "page": -2, "page_size": 0},
     ("CWServ.tdxf10_gg_gdyj_jgcgmx", "600000", "custom", 123, "99", "-2", "-2", "0")),
    ("shareholders", "institution_types", {"report_date": 123, "filter1": "f1", "filter2": "unused"},
     ("CWServ.tdxf10_gg_gdyj", "600000", "jgcgz", "f1", 123, "1", "1", "20")),
    ("capital_structure", "restricted_detail", {"date": "2026-03-04"},
     ("CWServ.tdxf10_gg_gbjg_xsjjmx", "600000", "20260304")),
    ("business_report", "changes", {"report_date": 123},
     ("CWServ.tdxf10_gg_jyfx", "600000", "0", "123")),
    ("industry_analysis", "comparable_profile", {}, ("CWServ.tdxf10_gg_kbgs", "0", "600000")),
    ("trading_report", "dragon_board", {}, ("CWServ.tdxf10_gg_jyds", "600000", "lgt", "2")),
    ("trading_report", "margin_history", {}, ("CWServ.tdxf10_gg_sj", "rzrq_sj", "600000", "0", "600000")),
    ("fund_report", "seats", {"arg": "seat", "page": -2, "page_size": 0},
     ("CWServ.skef10_jj_jjzl", "5", "600000", "seat", "-2", "-2", "0")),
    ("fund_report", "finance", {}, ("CWServ.skef10_jj_cwzk", "zcfzb", "600000")),
    ("market_report", "index_members", {}, ("CWServ.skef10_zs_cfgsj_cfgjcsj", "001", "600000", "1", "20")),
])
def test_f10_report_tables_keep_original_wire_values(method, section, kwargs, expected):
    client = f10.F10Client()
    client.params = lambda *args: args
    assert getattr(client, method)("sh600000", section, **kwargs) == expected


def test_f10_industry_eagerly_evaluates_unused_branches(monkeypatch):
    events = []

    class Argument:
        def __bool__(self):
            events.append("arg")
            return False

    class Client(f10.F10Client):
        @property
        def params(self):
            events.append("params")
            return lambda *args: args

    monkeypatch.setattr(f10, "_code6", lambda code: events.append("code") or code)
    result = Client().industry_analysis("raw", "comparable", arg=Argument())
    assert result == ("CWServ.tdxf10_gg_kbgs", "1", "raw")
    assert events == ["code"] + ["arg"] * 8 + ["params"]


def test_f10_report_keeps_eager_string_conversion_before_invalid_selector(monkeypatch):
    events = []

    class Date:
        def __str__(self):
            events.append("date")
            return "custom"

    def select(section, table):
        events.append(("select", section, tuple(table)))
        raise LookupError("selected")

    monkeypatch.setattr(f10, "_code6", lambda code: events.append("code") or code)
    monkeypatch.setattr(f10, "_section_request", select)
    with pytest.raises(LookupError, match="selected"):
        f10.F10Client().business_report("raw", "invalid", report_date=Date())
    assert events[:2] == ["code", "date"]
    assert events[2] == ("select", "invalid", ("business", "composition", "customers", "suppliers", "price", "changes"))


def test_f10_report_selected_iterable_expands_after_params_lookup(monkeypatch):
    events = []
    marker = object()

    def args():
        events.append("expand")
        yield marker

    class Client(f10.F10Client):
        @property
        def params(self):
            events.append("params")
            return lambda *values: values

    monkeypatch.setattr(f10, "_section_request", lambda *args_: ("custom", args()))
    assert Client().trading_report("sh600000") == ("custom", marker)
    assert events == ["params", "expand"]


def test_f10_trading_ignores_date_and_page_but_evaluates_unused_arg():
    events = []

    class Ignored:
        def __str__(self):
            raise AssertionError("unused value converted")

    class Arg:
        def __bool__(self):
            events.append("arg")
            return False

    client = f10.F10Client()
    client.params = lambda *values: values
    assert client.trading_report("sh600000", "margin", arg=Arg(), date=Ignored(),
                                 page=Ignored(), page_size=Ignored()) == (
        "CWServ.tdxf10_gg_jyds", "600000", "rzrq", "")
    assert events == ["arg"]


def test_f10_capital_detail_captures_params_before_date_conversion(monkeypatch):
    events = []

    class Client(f10.F10Client):
        @property
        def params(self):
            events.append("params")
            return lambda *values: values

    marker = object()
    monkeypatch.setattr(f10, "_date_text", lambda date: events.append("date") or marker)
    result = Client().capital_structure("sh600000", "restricted_detail", date=marker)
    assert result == ("CWServ.tdxf10_gg_gbjg_xsjjmx", "600000", marker)
    assert events == ["params", "date"]


def test_f10_fund_period_uses_live_map_and_captures_latest_before_request(monkeypatch):
    events = []
    marker = object()
    monkeypatch.setattr(f10, "_FUND_REPORT_PERIOD_ENTRIES", {"industry_allocation": "custom_period"})

    def latest(response):
        events.append(("latest", response))
        return marker

    monkeypatch.setattr(f10, "_latest_report_period", latest)

    class Client(f10.F10Client):
        def params(self, *values):
            events.append(values)
            if values[1] == "custom_period":
                monkeypatch.setattr(f10, "_latest_report_period", lambda response: pytest.fail("late lookup"))
                return "period-response"
            return values

    assert Client().fund_report("sh600000", "industry_allocation") == (
        "CWServ.skef10_jj_tzzh", "hyfb", "600000", marker)
    assert events[:2] == [("CWServ.skef10_jj_tzzh", "custom_period", "600000", ""), ("latest", "period-response")]


def test_f10_fund_keeps_all_eager_truthiness_evaluations():
    events = []

    class Value:
        def __init__(self, label):
            self.label = label

        def __bool__(self):
            events.append(self.label)
            return True

    arg, date = Value("arg"), Value("date")
    client = f10.F10Client()
    client.params = lambda *values: values
    assert client.fund_report("sh600000", "name", arg=arg, date=date) == (
        "CWServ.skef10_jj_idreq", "name", "600000")
    assert events == ["date", "date", "arg", "arg", "date", "date", "arg", "arg"]


def test_f10_market_keeps_live_alias_and_codeless_tables(monkeypatch):
    events = []
    monkeypatch.setattr(f10, "_MARKET_REPORT_SECTION_ALIASES", {"custom": "market_new_shares"})
    monkeypatch.setattr(f10, "_MARKET_REPORT_CODELESS_SECTIONS", {"market_new_shares"})
    monkeypatch.setattr(f10, "_code6", lambda code: pytest.fail("codeless request must not normalize code"))

    class Arg:
        def __bool__(self):
            events.append("arg")
            return False

    client = f10.F10Client()
    client.params = lambda *values: values
    assert client.market_report("", "custom", arg=Arg()) == ("CWServ.skef10_dpzx_xgfx", "002")
    assert events == ["arg"] * 8


def test_f10_shareholder_alias_preserves_public_override_and_raw_inputs():
    marker = object()
    client = f10.F10Client()
    client.shareholders = lambda *args, **kwargs: (args, kwargs)
    assert client.shareholder_change_plans(marker, page=marker, filter2=marker) == (
        (marker, "plan"), {"page": marker, "page_size": 20, "filter1": "", "filter2": marker})


def test_f10_business_composition_keeps_first_value_and_original_code(monkeypatch):
    events = []
    code, date = object(), object()
    monkeypatch.setattr(f10, "_code6", lambda raw: events.append(("normalize", raw)) or "normalized")
    monkeypatch.setattr(f10, "_first_value", lambda response, field: events.append(("first", response, field)) or date)
    monkeypatch.setattr(f10, "str", lambda value: events.append(("str", value)) or "selected", raising=False)
    client = f10.F10Client()
    client.business_periods = lambda value: events.append(("periods", value)) or "response"
    client.business_report = lambda *args, **kwargs: (args, kwargs)
    assert client.business_composition(code) == ((code, "composition"), {"report_date": "selected"})
    assert events == [("normalize", code), ("periods", "normalized"), ("first", "response", "T002"), ("str", date)]
