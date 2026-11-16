"""Authored bridge contracts; execution is deferred until T01–T30 source completion."""
from __future__ import annotations

from types import SimpleNamespace

import pytest

from eltdx.api.bars import BarApi
from eltdx.api.delisted_bars import DelistedBarApi
from eltdx.client import TdxClient
from eltdx.protocol.unit import normalize_code
from eltdx.transport import SocketTransport


class NativeBars:
    def __init__(self):
        self.calls = []
        self.result = SimpleNamespace(bars=("already ordered and aligned",), count=1)

    def sdk_bars(self, command, payload, **policy):
        self.calls.append((command, payload, policy))
        return self.result

    def execute(self, *_args, **_kwargs):
        pytest.fail("ordinary native bars must not run the Python paginator")


def test_native_bars_returns_shared_service_result_without_second_alignment() -> None:
    transport = NativeBars()
    result = BarApi(transport, native_services=True).get(
        "sh000001", period="5m", all_pages=True, page_size=7,
        max_pages=None, include_raw=True, bar_time="end",
    )
    assert result is transport.result
    assert transport.calls == [(0x052D, dict(
        code="sh000001", period="5m", start=0, count=7, adjust=None,
        anchor_date=None, kind="index", include_raw=True,
    ), dict(collect=True, max_pages=None, bar_time="end"))]


@pytest.mark.parametrize("value,expected", [(True, 1), (1.5, 2), (2.0, 2),
    (float("nan"), None), (float("inf"), None), (10 ** 100, None)])
def test_native_bars_preserves_historical_page_limit_comparisons(value, expected) -> None:
    transport = NativeBars()
    BarApi(transport, native_services=True).get("sh600000", all_pages=True, max_pages=value)
    assert transport.calls[0][2]["max_pages"] == expected


def test_native_bar_single_page_ignores_unused_collection_parameters() -> None:
    transport = NativeBars()
    result = BarApi(transport, native_services=True).get(
        "sz000001", count=3, page_size=-1, max_pages="unused",
    )
    assert result is transport.result
    assert transport.calls[0][1]["count"] == 3
    assert transport.calls[0][2] == dict(collect=False, max_pages=None, bar_time="start")


def test_native_delisted_retains_t_prefix_for_existing_request_extractor() -> None:
    transport = NativeBars()
    result = DelistedBarApi(transport, native_services=True).get(
        "T000038", all_pages=True, page_size=20, max_pages=None, include_raw=True,
    )
    assert result is transport.result
    assert transport.calls == [(0x052B, dict(code="T000038", start=0, count=20,
        include_raw=True), dict(collect=True, max_pages=None))]


def test_delisted_invalid_page_limit_still_precedes_native_request() -> None:
    transport = NativeBars()
    with pytest.raises(ValueError, match="max_pages"):
        DelistedBarApi(transport, native_services=True).get("T000038", all_pages=True, max_pages=True)
    assert not transport.calls


def test_native_bar_batch_keeps_stable_normalized_key_order() -> None:
    transport = NativeBars()
    rows = BarApi(transport, native_services=True).get(
        ["600000", "sh600000", "000001"], batch_size=1,
    )
    assert list(rows) == [normalize_code("600000"), normalize_code("000001")]
    assert [call[1]["code"] for call in transport.calls] == list(rows)
    assert all(row is transport.result for row in rows.values())


def test_custom_bar_transport_keeps_untyped_response_escape() -> None:
    value = object()

    class CustomTransport:
        def execute(self, command, payload):
            assert command == 0x052D and payload["count"] == 800
            return value

    assert BarApi(CustomTransport()).get("sh600000", all_pages=True) is value


def test_socket_subclass_keeps_custom_bar_execute_extension_point() -> None:
    class CustomSocket(SocketTransport):
        def execute(self, command, payload=None):
            return command, payload

    client = TdxClient(transport=CustomSocket(["127.0.0.1:7709"]))
    assert not client.bars._native_services
    assert not client.delisted_bars._native_services
    assert client.bars.get("sh600000")[0] == 0x052D


def test_custom_limit_object_keeps_its_comparison_timing() -> None:
    events = []

    class Limit:
        def __le__(self, other):
            events.append(("compare", other))
            return False

    class CustomTransport:
        def execute(self, _command, _payload):
            events.append("request")
            return object()

    BarApi(CustomTransport(), native_services=True).get("sh600000", all_pages=True, max_pages=Limit())
    assert events == [("compare", 0), "request"]
