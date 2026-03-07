from __future__ import annotations

from itertools import cycle
from unittest.mock import Mock, patch

from eltdx.bse import parse_bj_codes_response
from eltdx.client import TdxClient
from eltdx.models import SecurityCode


def test_parse_bj_codes_response() -> None:
    payload = (
        'jQuery3710848510589806625_123('
        '[{"content":[{"hqzqdm":"920001","hqzqjc":"北证测试","hqzjcj":12.34}],'
        '"totalElements":1,"totalPages":1,"lastPage":true}]'
        ')'
    )

    items, last_page = parse_bj_codes_response(payload)

    assert last_page is True
    assert len(items) == 1
    assert items[0].exchange == "bj"
    assert items[0].code == "920001"
    assert items[0].name == "北证测试"
    assert items[0].last_price == 12.34
    assert items[0].last_price_raw == 12340
    assert items[0].multiple == 0
    assert items[0].decimal == 0


@patch("eltdx.client.fetch_bj_codes")
def test_client_get_count_bj_uses_bse_fallback(mock_fetch) -> None:
    mock_fetch.return_value = [
        SecurityCode("bj", "920001", "one", 0, 0, 1.0, 1000),
        SecurityCode("bj", "920002", "two", 0, 0, 2.0, 2000),
    ]
    client = TdxClient()
    client.connect = Mock()

    count = client.get_count("bj")

    assert count == 2
    client.connect.assert_not_called()
    mock_fetch.assert_called_once()


@patch("eltdx.client.fetch_bj_codes")
def test_client_get_codes_bj_uses_bse_fallback(mock_fetch) -> None:
    mock_fetch.return_value = [
        SecurityCode("bj", "920001", "one", 0, 0, 1.0, 1000),
        SecurityCode("bj", "920002", "two", 0, 0, 2.0, 2000),
        SecurityCode("bj", "920003", "three", 0, 0, 3.0, 3000),
    ]
    client = TdxClient()
    client.connect = Mock()
    connection = Mock()
    client._connections = [connection]
    client._round_robin = cycle(range(1))

    page = client.get_codes("bj", start=1, limit=1)
    all_codes = client.get_codes_all("bj")

    assert page.exchange == "bj"
    assert page.count == 1
    assert page.total == 3
    assert [item.full_code for item in page.items] == ["bj920002"]
    assert [item.full_code for item in all_codes] == ["bj920001", "bj920002", "bj920003"]
    client.connect.assert_not_called()
    connection.request_codes.assert_not_called()
    connection.request_count.assert_not_called()
    mock_fetch.assert_called_once()
