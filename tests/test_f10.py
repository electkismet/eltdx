from eltdx import F10Client, TdxClient
from eltdx.f10 import parse_tqlex_response
from eltdx.f10.fields import describe_field_notes, describe_fields, field_labels_for


def test_parse_tqlex_colname_and_duplicate_columns() -> None:
    raw = {
        "ErrorCode": 0,
        "ResultSets": [
            {
                "ResultSetKey": "table0",
                "ColName": ["rq", "T120", "T120"],
                "Content": [["2026-03-31", 1, 2]],
            }
        ],
    }

    response = parse_tqlex_response("CWServ.test", {"Params": []}, raw)

    assert response.ok is True
    assert response.first_table is not None
    assert response.first_table.count == 1
    assert response.rows[0] == {"rq": "2026-03-31", "T120": 1, "T120__2": 2}
    assert response.first_table.row_cells[0][2].name == "T120"
    assert response.first_row() == response.rows[0]


def test_parse_tqlex_coldes() -> None:
    raw = {
        "ErrorCode": 0,
        "ResultSets": [
            {
                "ColDes": [{"Name": "ans"}, {"Name": "date"}],
                "Content": [["200745", "20260526"]],
            }
        ],
    }

    response = parse_tqlex_response("HQServ.test", [{"ReqId": "200745"}], raw)

    assert response.tables[0].key == "table0"
    assert response.rows[0] == {"ans": "200745", "date": "20260526"}


def test_f10_field_labels_are_scoped_by_entry_and_request() -> None:
    finance_labels = field_labels_for(
        "CWServ.tdxf10_gg_cwfx",
        {"Params": ["600519", "zcfzb", ""]},
        metadata={"nhytype": 0},
    )
    research_labels = field_labels_for(
        "CWServ.tdxf10_gg_gszx",
        {"Params": ["600519", "gsyj", "", "0", "1", "20"]},
    )

    assert finance_labels["T039"] == "资产总计"
    assert research_labels["T039"] == "研报标题"
    assert field_labels_for(
        "CWServ.tdxf10_gg_cwfx",
        {"Params": ["600519", "unknown_report", ""]},
    ) == {}

    bank_labels = field_labels_for(
        "CWServ.tdxf10_gg_cwfx",
        {"Params": ["000001", "zcfzb", ""]},
        metadata={"nhytype": 1},
    )
    assert bank_labels["T039"] == "固定资产"
    assert bank_labels["T048"] == "资产总计"

    for industry_type in (2, 3):
        financial_labels = field_labels_for(
            "CWServ.tdxf10_gg_cwfx",
            {"Params": ["600030", "zcfzb", ""]},
            metadata={"nhytype": industry_type},
        )
        assert financial_labels["T039"] == "固定资产"
        assert financial_labels["T048"] == "资产总计"

    assert field_labels_for(
        "CWServ.tdxf10_gg_cwfx",
        {"Params": ["600519", "zcfzb", ""]},
    ) == {}
    assert field_labels_for(
        "CWServ.tdxf10_gg_cwfx",
        {"Params": ["600519", "zcfzb", ""]},
        metadata={"nhytype": 99},
    ) == {}


def test_describe_fields_preserves_unknowns_and_duplicate_positions() -> None:
    labels, unmapped = describe_fields(
        "CWServ.tdxf10_gg_cwfx",
        {"Params": ["600519", "zcfzb", ""]},
        ["rq", "T039", "T039__2", "T999"],
        metadata={"nhytype": 0},
    )

    assert labels == {
        "rq": "报告期",
        "T039": "资产总计",
        "T039__2": "资产总计（2）",
    }
    assert unmapped == ["T999"]

    duplicate_labels, duplicate_unmapped = describe_fields(
        "CWServ.tdxf10_gg_cwfx",
        {"Params": ["600519", "zcfzb", ""]},
        ["T120", "T120__2"],
        metadata={"nhytype": 0},
    )
    assert duplicate_labels == {
        "T120": "应收票据及应收账款",
        "T120__2": "应收票据及应收账款（2）",
    }
    assert duplicate_unmapped == []


def test_statement_field_labels_are_scoped_by_report_type_and_template() -> None:
    income = field_labels_for(
        "CWServ.tdxf10_gg_cwfx",
        {"Params": ["600519", "lyb", ""]},
        metadata={"nhytype": 0},
    )
    cashflow = field_labels_for(
        "CWServ.tdxf10_gg_cwfx",
        {"Params": ["600519", "xjllb", ""]},
        metadata={"nhytype": 0},
    )

    assert income["T008"] == "营业收入"
    assert cashflow["T008"] == "销售商品、提供劳务收到的现金"
    assert cashflow["T017"] == "经营活动产生的现金流量净额"
    assert cashflow["T063"] == "不涉及现金收支的重大投资和筹资活动"

    general_notes = describe_field_notes(
        "CWServ.tdxf10_gg_cwfx",
        {"Params": ["600519", "xjllb", ""]},
        ["rq", "T063", "T999"],
        metadata={"nhytype": 0},
    )
    assert general_notes == {
        "T063": "现金流量表补充资料的分组标题，通常不承载数值。"
    }

    insurance_income = field_labels_for(
        "CWServ.tdxf10_gg_cwfx",
        {"Params": ["601318", "lyb", ""]},
        metadata={"nhytype": 3},
    )
    securities_cashflow = field_labels_for(
        "CWServ.tdxf10_gg_cwfx",
        {"Params": ["600030", "xjllb", ""]},
        metadata={"nhytype": 2},
    )
    assert insurance_income["T018"] == "已赚保费"
    assert insurance_income["T032"] == "提取保险责任准备金"
    assert securities_cashflow["T107"] == "收到代理买卖证券款净额"
    assert securities_cashflow["T091"] == "其他"

    bank_labels, bank_unmapped = describe_fields(
        "CWServ.tdxf10_gg_cwfx",
        {"Params": ["000001", "xjllb", ""]},
        ["rq", "T088"],
        metadata={"nhytype": 1},
    )
    bank_notes = describe_field_notes(
        "CWServ.tdxf10_gg_cwfx",
        {"Params": ["000001", "xjllb", ""]},
        ["rq", "T088"],
        metadata={"nhytype": 1},
    )
    assert bank_labels == {"rq": "报告期"}
    assert bank_unmapped == ["T088"]
    assert bank_notes == {
        "T088": (
            "银行历史报表的动态扩展调节项，含义随证券及报告期变化；"
            "不能使用统一字段名。"
        )
    }

    duplicate_labels, duplicate_unmapped = describe_fields(
        "CWServ.tdxf10_gg_cwfx",
        {"Params": ["601318", "lyb", ""]},
        ["T030", "T030__2"],
        metadata={"nhytype": 3},
    )
    assert duplicate_labels == {
        "T030": "赔付支出",
        "T030__2": "赔付支出（2）",
    }
    assert duplicate_unmapped == []
    assert field_labels_for(
        "CWServ.tdxf10_gg_cwfx",
        {"Params": ["600519", "lyb", ""]},
    ) == {}


def test_tdx_client_mounts_f10_client() -> None:
    client = TdxClient.in_memory()

    assert isinstance(client.f10, F10Client)


def test_f10_wrappers_build_expected_requests() -> None:
    class FakeF10Client(F10Client):
        def __init__(self) -> None:
            super().__init__()
            self.calls = []

        def _post(self, entry, body):
            self.calls.append((entry, body))
            return parse_tqlex_response(entry, body, {"ErrorCode": 0, "ResultSets": []})

    client = FakeF10Client()

    client.company_profile("000034")
    client.dividend_financing("sz000034", "fh")
    client.announcements("000034")
    client.theme_market("000034", req_id="200743", page_size=5)
    client.valuation("000034", req_id="200191")

    assert client.calls[0] == ("CWServ.tdxf10_gg_gsgk", {"Params": ["8", "000034", ""]})
    assert client.calls[1] == ("CWServ.tdxf10_gg_fhrz", {"Params": ["000034", "fh"]})
    assert client.calls[2] == (
        "CWSearch.tzx_rcache",
        {"action": "get", "key": "gg:0_000034", "bin": "1", "qsid": "tdx"},
    )
    assert client.calls[3][0] == "HQServ.hq_nlp_tcihq"
    assert client.calls[3][1][0]["ReqId"] == "200743"
    assert client.calls[3][1][0]["modname"] == "mod_tcihq.dll"
    assert client.calls[4][0] == "HQServ.hq_nlp_gpsj"
    assert client.calls[4][1][0]["Code"] == "000034|0"
    assert client.calls[4][1][0]["BeginDate"] == "0"
    assert "code" not in client.calls[4][1][0]


def test_f10_business_composition_uses_latest_report_period() -> None:
    class FakeF10Client(F10Client):
        def _post(self, entry, body):
            if body == {"Params": ["zygcfx", "000034"]}:
                raw = {"ErrorCode": 0, "ResultSets": [{"ColName": ["T002"], "Content": [["20251231"]]}]}
            else:
                raw = {"ErrorCode": 0, "ResultSets": []}
            return parse_tqlex_response(entry, body, raw)

    client = FakeF10Client()
    response = client.business_composition("000034")

    assert response.entry == "CWServ.tdxf10_gg_jyfx"
    assert response.request_body == {"Params": ["000034", "zygc", "20251231"]}
