import pytest

from eltdx import F10Client, TdxClient
from eltdx.f10 import parse_tqlex_response


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


def test_limit_board_ladder_builds_detail_request_and_normalizes_rows() -> None:
    class FakeF10Client(F10Client):
        def __init__(self) -> None:
            super().__init__()
            self.calls = []

        def _post(self, entry, body):
            self.calls.append((entry, body))
            return parse_tqlex_response(
                entry,
                body,
                {
                    "ErrorCode": 0,
                    "ResultSets": [
                        {
                            "ResultSetKey": "table0",
                            "ColName": [
                                "rq", "rqex", "zglb", "lbts", "ZQDM", "SC",
                                "ztyy", "fde", "ZQJC", "ztyy2", "ztsj",
                                "kbcs", "sshy", "ztlb", "cgl",
                            ],
                            "Content": [[
                                "08月10日", "20260810", 5, 1, "600815", "1",
                                "工程机械", 34826800, "厦工股份", "地下管网",
                                "13:56:56", 1, "工程机械", 1, None,
                            ]],
                        }
                    ],
                },
            )

    client = FakeF10Client()
    result = client.limit_board_ladder("2026-08-10")

    assert client.calls == [
        ("CWServ.cfg_fx_lbtt", {"Params": ["1", "20260810", "20260810"]})
    ]
    assert result.ok is True
    assert result.trade_date == "20260810"
    assert result.count == 1
    assert result.rows[0].full_code == "sh600815"
    assert result.rows[0].board_level == 5
    assert result.rows[0].ladder_days == 1
    assert result.rows[0].seal_amount == 34826800
    assert result.rows[0].ZQDM == "600815"
    assert result.rows[0].raw["ztyy2"] == "地下管网"


def test_limit_board_ladder_can_include_market_summary() -> None:
    class FakeF10Client(F10Client):
        def __init__(self) -> None:
            super().__init__()
            self.calls = []

        def _post(self, entry, body):
            self.calls.append((entry, body))
            if body["Params"][0] == "2":
                raw = {
                    "ErrorCode": 0,
                    "ResultSets": [{"ColName": ["t001"], "Content": [["2026-08-10"]]}],
                }
            else:
                raw = {"ErrorCode": 0, "ResultSets": []}
            return parse_tqlex_response(entry, body, raw)

    client = FakeF10Client()
    result = client.limit_board_ladder(
        "20260807", "2026-08-10", include_summary=True
    )

    assert client.calls == [
        ("CWServ.cfg_fx_lbtt", {"Params": ["1", "20260807", "20260810"]}),
        ("CWServ.cfg_fx_lbtt", {"Params": ["2", "20260807", "20260810"]}),
    ]
    assert result.trade_date is None
    assert result.summary == ({"t001": "2026-08-10"},)


def test_limit_board_ladder_rejects_reversed_or_invalid_dates() -> None:
    client = F10Client()

    with pytest.raises(ValueError, match="on or before"):
        client.limit_board_ladder("20260811", "20260810")
    with pytest.raises(ValueError, match="invalid date"):
        client.limit_board_ladder("not-a-date")
