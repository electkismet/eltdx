import pytest

import json
import socket
from concurrent.futures import ThreadPoolExecutor
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from threading import Barrier, BrokenBarrierError, Thread

from eltdx import F10Client, TdxClient
from eltdx.f10 import parse_tqlex_response


@pytest.mark.parametrize("use_proxy", [False, True])
def test_f10_ipv4_requests_are_concurrent_and_leave_dns_unchanged(monkeypatch, use_proxy):
    """Both requests must arrive before either response is returned."""
    barrier = Barrier(2, timeout=2)
    observations = []
    original_resolver = socket.getaddrinfo

    def resolver(host, port, *args, **kwargs):
        if host == "f10.test":
            return [
                (socket.AF_INET6, socket.SOCK_STREAM, 6, "", ("::1", port, 0, 0)),
                (socket.AF_INET, socket.SOCK_STREAM, 6, "", ("127.0.0.1", port)),
            ]
        return original_resolver(host, port, *args, **kwargs)

    monkeypatch.setattr(socket, "getaddrinfo", resolver)
    monkeypatch.setenv("no_proxy", "*")
    monkeypatch.setenv("NO_PROXY", "*")

    class Handler(BaseHTTPRequestHandler):
        def do_POST(self):
            body = self.rfile.read(int(self.headers["Content-Length"]))
            concurrent = True
            try:
                barrier.wait()
            except BrokenBarrierError:
                concurrent = False
            observations.append((
                concurrent, socket.getaddrinfo is resolver,
                self.headers["Host"], json.loads(body), self.path,
            ))
            data = b'{"ErrorCode":0,"ResultSets":[]}'
            self.send_response(200)
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def log_message(self, *args):
            pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = Thread(target=server.serve_forever, daemon=True)
    thread.start()
    port = server.server_port
    try:
        if use_proxy:
            monkeypatch.setenv("no_proxy", "")
            monkeypatch.setenv("NO_PROXY", "")
            monkeypatch.setenv("http_proxy", f"http://f10.test:{port}")
            base_url = "http://upstream.test:7615/TQLEX"
            expected_host = "upstream.test:7615"
            expected_path = base_url + "?Entry=CWServ.test"
        else:
            base_url = f"http://f10.test:{port}/TQLEX"
            expected_host = f"f10.test:{port}"
            expected_path = "/TQLEX?Entry=CWServ.test"
        client = F10Client(base_url=base_url, timeout=5)
        with ThreadPoolExecutor(max_workers=2) as executor:
            results = list(executor.map(lambda _: client.params("CWServ.test", "x"), range(2)))
        assert all(result.ok for result in results)
        assert len(observations) == 2
        assert all(row[:2] == (True, True) for row in observations)
        assert all(row[2:] == (expected_host, {"Params": ["x"]}, expected_path) for row in observations)
        assert socket.getaddrinfo is resolver
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)


@pytest.mark.parametrize("scenario", ["ipv4", "fallback", "ipv6_only", "all_fail", "empty"])
def test_f10_address_order_and_connection_cleanup(monkeypatch, scenario):
    from eltdx.f10._http import _connect_ipv4_first

    v6 = (socket.AF_INET6, socket.SOCK_STREAM, 6, "", ("::1", 7615, 0, 0))
    v4a = (socket.AF_INET, socket.SOCK_STREAM, 6, "", ("127.0.0.2", 7615))
    v4b = (socket.AF_INET, socket.SOCK_STREAM, 6, "", ("127.0.0.1", 7615))
    records = [] if scenario == "empty" else ([v6] if scenario == "ipv6_only" else [v6, v4a, v4b])
    monkeypatch.setattr(socket, "getaddrinfo", lambda *args: records)
    sockets = []
    attempts = []

    class Socket:
        def __init__(self, *args):
            self.closed = False
            sockets.append(self)

        def settimeout(self, timeout):
            assert timeout == 0.25

        def bind(self, source):
            assert source == ("", 0)

        def connect(self, address):
            attempts.append(address)
            if scenario == "all_fail" or (scenario == "fallback" and len(address) == 2):
                raise TimeoutError("connection timed out")

        def close(self):
            self.closed = True

    monkeypatch.setattr(socket, "socket", Socket)
    if scenario in {"all_fail", "empty"}:
        with pytest.raises(OSError):
            _connect_ipv4_first(("f10.test", 7615), 0.25, ("", 0))
        assert all(sock.closed for sock in sockets)
    else:
        connected = _connect_ipv4_first(("f10.test", 7615), 0.25, ("", 0))
        assert connected is sockets[-1]
        assert not connected.closed
        assert all(sock.closed for sock in sockets[:-1])
    expected = {
        "ipv4": [v4a[4]], "fallback": [v4a[4], v4b[4], v6[4]],
        "ipv6_only": [v6[4]], "all_fail": [v4a[4], v4b[4], v6[4]], "empty": [],
    }
    assert attempts == expected[scenario]


def test_f10_system_order_opt_out(monkeypatch):
    from io import BytesIO
    from eltdx.f10 import client as module

    calls = []

    def open_default(request, *, timeout):
        calls.append((request.full_url, timeout))
        return BytesIO(b'{"ErrorCode":0,"ResultSets":[]}')

    def unexpected(*args, **kwargs):
        raise AssertionError("IPv4 preference must be disabled")

    monkeypatch.setattr(module, "urlopen", open_default)
    monkeypatch.setattr(module, "open_ipv4_first", unexpected)
    assert F10Client(prefer_ipv4=False, timeout=2).params("CWServ.test").ok
    assert len(calls) == 1
    assert calls[0][1] == 2


def test_f10_https_retains_sni_and_certificate_checks(monkeypatch):
    import ssl
    from unittest.mock import Mock
    from eltdx.f10._http import _IPv4HTTPSConnection

    connection = _IPv4HTTPSConnection("f10.test", timeout=2)
    assert connection._context.check_hostname
    assert connection._context.verify_mode == ssl.CERT_REQUIRED
    raw_socket = Mock()
    connector = Mock(return_value=raw_socket)
    connection._create_connection = connector
    wrapped_socket = Mock()
    wrapper = Mock(return_value=wrapped_socket)
    monkeypatch.setattr(connection._context, "wrap_socket", wrapper)
    connection.connect()
    connector.assert_called_once_with(("f10.test", 443), 2, None)
    wrapper.assert_called_once_with(raw_socket, server_hostname="f10.test")
    connection.close()


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
    assert result.rows[0].board_level == 1
    assert result.rows[0].highest_board_level == 5
    assert result.rows[0].board_level == 1
    assert result.rows[0].seal_amount == 34826800
    assert result.rows[0].ZQDM == "600815"
    assert result.rows[0].raw["ztyy2"] == "地下管网"
    assert result.rows[0].status == "limit_up"
    assert result.rows[0].reason == "工程机械"


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


def test_limit_board_ladder_exposes_market_overview_and_ladder_totals() -> None:
    class FakeF10Client(F10Client):
        def _post(self, entry, body):
            if body["Params"][0] == "2":
                raw = {
                    "ErrorCode": 0,
                    "ResultSets": [{"ColName": ["t001", "N002", "N003", "N004", "N005", "N006", "N007", "N008", "N001", "N009", "N010"], "Content": [["2026-08-10", 10, 20, 3, 4, 2, 100, 90, "2026-08-10", "题材A", 7]]}],
                }
            else:
                raw = {
                    "ErrorCode": 0,
                    "ResultSets": [{"ColName": ["rq", "rqex", "zglb", "lbts", "ZQDM", "SC", "ztyy", "fde", "ZQJC", "ztyy2", "ztsj", "kbcs", "sshy", "ztlb", "cgl"], "Content": [["", "20260810", 3, 3, "000001", "0", "", 1, "", "", "", 0, "", 1, None], ["", "20260810", 3, 2, "000002", "0", "", 1, "", "", "", 0, "", 1, None], ["", "20260810", 3, 1, "000003", "0", "", 1, "", "", "", 0, "", 1, None], ["", "20260810", 3, 1, "000004", "0", "", 1, "", "", "", 0, "", 1, None]]}],
                }
            return parse_tqlex_response(entry, body, raw)

    result = FakeF10Client().limit_up_down_list("20260810", include_summary=True)
    assert result.market_overview[0]["flat_count"] == 3
    assert result.market_overview[0]["limit_up_count"] == 4
    assert result.market_overview[0]["hot_topics"] == ({"name": "题材A", "count": 7},)
    assert result.ladder_counts == {3: 1, 2: 1, 1: 2}
    assert result.promotion_rates == {3: 1.0, 2: 0.5}


def test_limit_up_down_list_normalizes_board_and_status_by_row_type() -> None:
    class FakeF10Client(F10Client):
        def _post(self, entry, body):
            return parse_tqlex_response(
                entry,
                body,
                {
                    "ErrorCode": 0,
                    "ResultSets": [
                        {
                            "ColName": [
                                "rq", "rqex", "zglb", "lbts", "ZQDM", "SC",
                                "ztyy", "fde", "ZQJC", "ztyy2", "ztsj",
                                "kbcs", "sshy", "ztlb", "cgl",
                            ],
                            "Content": [
                                ["09月04日", "20260904", 5, 2, "600001", "1", "题材", 1, "涨停股", "补充", "09:30:00", 0, "行业", 1, None],
                                ["09月04日", "20260904", 5, 0, "000001", "0", "炸板原因", 0, "炸板股", None, "09:31:00", 3, "行业", 3, None],
                                ["09月04日", "20260904", 5, 0, "600002", "1", "个股调整", 123, "跌停股", None, "09:32:00", 4, "行业", 0, None],
                            ],
                        }
                    ],
                },
            )

    result = FakeF10Client().limit_up_down_list("20260904")
    up, broken, down = result.rows
    assert (up.board_level, up.highest_board_level, up.status, up.reason) == (2, 5, "limit_up", "题材")
    assert (broken.board_level, broken.status) == (None, "broken")
    assert (down.board_level, down.status) == (None, "limit_down")


def test_limit_board_ladder_rejects_reversed_or_invalid_dates() -> None:
    client = F10Client()

    with pytest.raises(ValueError, match="on or before"):
        client.limit_board_ladder("20260811", "20260810")
    with pytest.raises(ValueError, match="invalid date"):
        client.limit_board_ladder("not-a-date")
