from __future__ import annotations

from itertools import cycle
from unittest.mock import Mock, call

from eltdx.client import TdxClient
from eltdx.models import SecurityCode
from eltdx.protocol.frame import ResponseFrame
from eltdx.protocol import unit as protocol_unit
from eltdx.protocol.model_code import parse_code_payload
from eltdx.protocol.unit import add_prefix, is_a_share_entry, is_b_share_entry, is_etf_entry, is_index, is_stock_entry


RECORD_1_HEX = "3339353030316400d6f7b0e5a3c1b9c957860000020060ba4400000000"
RECORD_2_HEX = "3339353030326400d6f7b0e5a3c2b9c952070000020000184200000000"


def test_parse_code_payload() -> None:
    payload = bytes.fromhex("0200" + RECORD_1_HEX + RECORD_2_HEX)
    response = ResponseFrame(
        control=0x1C,
        msg_id=2,
        msg_type=0x0450,
        zip_length=len(payload),
        length=len(payload),
        data=payload,
        raw=b"\xB1\xCB\x74\x00",
    )

    parsed = parse_code_payload("sz", response)

    assert len(parsed) == 2
    assert parsed[0].exchange == "sz"
    assert parsed[0].code == "395001"
    assert parsed[0].name == "主板Ａ股"
    assert parsed[0].multiple == 100
    assert parsed[0].decimal == 2
    assert parsed[0].last_price == 1491.0
    assert parsed[1].code == "395002"
    assert parsed[1].name == "主板Ｂ股"
    assert parsed[1].last_price == 38.0


def test_parse_code_payload_without_math_exp2(monkeypatch) -> None:
    monkeypatch.delattr(protocol_unit.math, "exp2", raising=False)

    payload = bytes.fromhex("0200" + RECORD_1_HEX + RECORD_2_HEX)
    response = ResponseFrame(
        control=0x1C,
        msg_id=2,
        msg_type=0x0450,
        zip_length=len(payload),
        length=len(payload),
        data=payload,
        raw=b"\xB1\xCB\x74\x00",
    )

    parsed = parse_code_payload("sz", response)

    assert parsed[0].last_price == 1491.0
    assert parsed[1].last_price == 38.0


def test_client_get_codes_returns_page_by_default() -> None:
    client = TdxClient()
    client.connect = Mock()
    connection = Mock()
    connection.request_count.return_value = 2500
    connection.request_codes.return_value = list(range(1000))
    client._connections = [connection]
    client._round_robin = cycle(range(1))

    parsed = client.get_codes("sz")

    assert parsed.exchange == "sz"
    assert parsed.start == 0
    assert parsed.count == 1000
    assert parsed.total == 2500
    assert len(parsed) == 1000
    assert list(parsed) == list(range(1000))
    assert parsed[0] == 0
    assert connection.request_count.call_args_list == [call("sz")]
    assert connection.request_codes.call_args_list == [call("sz", 0)]


def test_client_get_codes_all_auto_pages() -> None:
    client = TdxClient()
    client.connect = Mock()
    connection = Mock()
    connection.request_count.return_value = 2500
    connection.request_codes.side_effect = [
        list(range(1000)),
        list(range(1000, 2000)),
        list(range(2000, 2500)),
    ]
    client._connections = [connection]
    client._round_robin = cycle(range(1))

    parsed = client.get_codes_all("sz")

    assert len(parsed) == 2500
    assert connection.request_codes.call_args_list == [call("sz", 0), call("sz", 1000), call("sz", 2000)]


def test_client_get_codes_honors_limit() -> None:
    client = TdxClient()
    client.connect = Mock()
    connection = Mock()
    connection.request_count.return_value = 2500
    connection.request_codes.side_effect = [
        list(range(1000)),
        list(range(1000, 2000)),
    ]
    client._connections = [connection]
    client._round_robin = cycle(range(1))

    parsed = client.get_codes("sz", limit=1200)

    assert parsed.count == 1200
    assert parsed.total == 2500
    assert len(parsed) == 1200
    assert connection.request_codes.call_args_list == [call("sz", 0), call("sz", 1000)]


def test_client_classifies_code_lists() -> None:
    client = TdxClient()
    client.get_codes_all = Mock(
        side_effect=lambda exchange: {
            "sh": [
                SecurityCode("sh", "600000", "stock", 1, 2, 10.0, 10000),
                SecurityCode("sh", "900901", "b_stock", 1, 2, 10.0, 10000),
                SecurityCode("sh", "510050", "50ETF", 1, 2, 10.0, 10000),
                SecurityCode("sh", "999998", "index", 1, 2, 10.0, 10000),
                SecurityCode("sh", "880001", "sector_index", 1, 2, 10.0, 10000),
                SecurityCode("sh", "152010", "bond", 1, 2, 10.0, 10000),
            ],
            "sz": [
                SecurityCode("sz", "000001", "stock", 1, 2, 10.0, 10000),
                SecurityCode("sz", "200011", "b_stock", 1, 2, 10.0, 10000),
                SecurityCode("sz", "159001", "ETF", 1, 2, 10.0, 10000),
                SecurityCode("sz", "399001", "index", 1, 2, 10.0, 10000),
                SecurityCode("sz", "395013", "category", 1, 2, 10.0, 10000),
            ],
            "bj": [
                SecurityCode("bj", "920001", "stock", 1, 2, 10.0, 10000),
                SecurityCode("bj", "899050", "index", 1, 2, 10.0, 10000),
            ],
        }[exchange]
    )

    assert client.get_stock_count("sh") == 2
    assert client.get_stock_count("sz") == 2
    assert client.get_stock_count("bj") == 1
    assert client.get_a_share_count("sh") == 1
    assert client.get_a_share_count("sz") == 1
    assert client.get_a_share_count("bj") == 1
    assert client.get_stock_codes_all() == ["sh600000", "sh900901", "sz000001", "sz200011", "bj920001"]
    assert client.get_a_share_codes_all() == ["sh600000", "sz000001", "bj920001"]
    assert client.get_etf_codes_all() == ["sh510050", "sz159001"]
    assert client.get_index_codes_all() == ["sh999998", "sh880001", "sz399001", "bj899050"]


def test_code_entry_classification_helpers() -> None:
    assert is_a_share_entry("sh600000") is True
    assert is_a_share_entry("sz000001") is True
    assert is_a_share_entry("bj920001") is True
    assert is_a_share_entry("sh900901") is False
    assert is_a_share_entry("sz200011") is False

    assert is_b_share_entry("sh900901") is True
    assert is_b_share_entry("sz200011") is True
    assert is_b_share_entry("bj920001") is False

    assert is_stock_entry("sh600000") is True
    assert is_stock_entry("sh900901") is True
    assert is_stock_entry("sz200011") is True
    assert is_stock_entry("bj920001") is True
    assert is_stock_entry("sh999998") is False

    assert is_etf_entry("sh510050", "50ETF") is True
    assert is_etf_entry("sz159001", "ETF") is True
    assert is_etf_entry("sh152010", "PR梧州01") is False
    assert is_etf_entry("sh880096", "ETF等权") is False

    assert is_index("sh999998") is True
    assert is_index("sh880001") is True
    assert is_index("sz399001") is True
    assert is_index("sz395013") is False
    assert is_index("bj899050") is True


def test_add_prefix_supports_more_index_codes() -> None:
    assert add_prefix("999998") == "sh999998"
    assert add_prefix("880001") == "sh880001"
    assert add_prefix("399001") == "sz399001"
