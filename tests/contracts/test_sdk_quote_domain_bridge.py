"""Authored quote-domain contracts; execution waits for all implementation."""
from __future__ import annotations

from contextlib import contextmanager
from types import SimpleNamespace

import pytest

from eltdx.api.codes import CodeApi
from eltdx.api.quotes import QuoteApi
from eltdx.client import TdxClient
from eltdx.models.quote import QuoteRefreshPage
from eltdx.transport import PooledSocketTransport, SocketTransport
from eltdx.transport.pool import PinnedTransportProxy


class NativeQuotes:
    def __init__(self):
        self.calls = []
        self.result = object()

    def sdk_quotes(self, command, payload):
        self.calls.append((command, payload))
        return self.result

    def execute(self, *_args, **_kwargs):
        pytest.fail("ordinary native quote requests must call the Rust domain")


@pytest.mark.parametrize("method,command,arguments,payload", [
    ("legacy", 0x053E, (["sh600001", "sh600001"],), {"codes": ["sh600001", "sh600001"]}),
    ("list_by_category", 0x054B, ("沪深A股",),
     dict(category="沪深A股", sort_by=None, start=0, count=80, ascending=False)),
    ("refresh", 0x0547, ("sh600001",), dict(codes=["sh600001"], cursors={})),
    ("get_depth", 0x0547, (["sh600001", "sh600001"],),
     dict(codes=["sh600001", "sh600001"], cursors={})),
])
def test_native_quote_domain_routes_preserve_payload_and_result(method, command, arguments, payload) -> None:
    transport = NativeQuotes()
    api = QuoteApi(transport, native_services=True)
    assert getattr(api, method)(*arguments) is transport.result
    assert transport.calls == [(command, payload)]


@pytest.mark.parametrize("method,name,args,price_codes", [
    ("legacy", "legacy_quotes", (["019999", "sh600001"],), ["019999", "sh600001"]),
    ("list_by_category", "category_quotes", (6,), None),
    ("refresh", "refresh_stream", (None,), []),
])
def test_native_quote_domain_retains_resolver_order_and_projection(method, name, args, price_codes) -> None:
    transport = NativeQuotes()
    events = []
    projected = object()

    def resolver(command, result, codes):
        events.append((command, result, codes))
        if result is None:
            assert transport.calls == []
        return projected if result is transport.result else result

    api = QuoteApi(transport, native_services=True, price_resolver=resolver)
    assert getattr(api, method)(*args) is projected
    assert events == [(name, None, price_codes), (name, transport.result, price_codes)]


def test_native_quote_resolver_failure_stops_before_native_request() -> None:
    transport = NativeQuotes()
    root = ValueError("metadata callback")

    def resolver(*_args):
        raise root

    with pytest.raises(ValueError) as caught:
        QuoteApi(transport, native_services=True, price_resolver=resolver).legacy([])
    assert caught.value is root and transport.calls == []


def test_native_refresh_keeps_empty_wire_poll_and_copied_cursor_map() -> None:
    transport = NativeQuotes()
    cursors = {"sh600001": 9, "sz000001": 7}
    api = QuoteApi(transport, native_services=True)
    assert api.refresh(cursors=cursors) is transport.result
    command, payload = transport.calls[0]
    assert command == 0x0547 and payload == dict(codes=[], cursors=cursors)
    assert payload["cursors"] is not cursors


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
def test_quote_transport_subclasses_retain_execute_extension(transport_type) -> None:
    class CustomTransport(transport_type):
        def execute(self, command, payload=None):
            if command == 0x044D:
                return []
            return command, payload

    client = TdxClient(transport=CustomTransport(["127.0.0.1:7709"], heartbeat_interval=None))
    assert client.quotes.legacy("sh600001") == (0x053E, {"codes": ["sh600001"]})
    assert not client.quotes._native_services


def test_native_suspended_retains_eighty_row_chunks_and_return_order() -> None:
    class SuspendedTransport:
        def __init__(self):
            self.calls = []

        def sdk_suspended(self, codes):
            self.calls.append(codes)
            return list(reversed(codes))

    transport = SuspendedTransport()
    api = CodeApi(transport, native_services=True)
    rows = [SimpleNamespace(full_code=f"sh{600000 + index}") for index in range(81)]
    api.latest_stock_list = lambda market=None: rows
    assert api.latest_suspended() == [row.full_code for row in reversed(rows[:80])] + [rows[-1].full_code]
    assert transport.calls == [[row.full_code for row in rows[:80]], [rows[-1].full_code]]


def test_native_empty_suspended_selection_never_calls_transport() -> None:
    api = CodeApi(object(), native_services=True)
    api.latest_stock_list = lambda market=None: []
    assert api.suspended() == []


def test_custom_suspended_objects_retain_int_status_and_arbitrary_rows() -> None:
    class CustomTransport:
        def execute(self, command, payload):
            assert command == 0x053E and payload == {"codes": ["sh600001"]}
            return [SimpleNamespace(trading_status_raw="32", full_code="custom-identity")]

    api = CodeApi(CustomTransport())
    api.latest_stock_list = lambda market=None: [SimpleNamespace(full_code="sh600001")]
    assert api.latest_suspended() == ["custom-identity"]


def test_native_subscription_refresh_uses_existing_pin_after_metadata(monkeypatch) -> None:
    pool = PooledSocketTransport(["127.0.0.1:7709"], pool_size=1, heartbeat_interval=None)
    events = []
    proxy = PinnedTransportProxy(pool, object(), 0)
    page = QuoteRefreshPage(("sh600001",), (), b"\x00\x00", b"\x93\x93")

    @contextmanager
    def pin():
        events.append("pin")
        try:
            yield proxy
        finally:
            events.append("release")

    def resolver(*_args):
        events.append("metadata")

    def refresh(self, command, payload):
        assert self is proxy and command == 0x0547
        assert payload == {"codes": ["sh600001"], "cursors": {"sh600001": 0}}
        events.append("native_refresh")
        return page

    monkeypatch.setattr(pool, "pin", pin)
    monkeypatch.setattr(PinnedTransportProxy, "sdk_quotes", refresh)
    api = QuoteApi(pool, native_services=True, price_resolver=resolver)
    subscription = api.subscribe("sh600001", lambda _row: None)
    try:
        assert subscription.activate() is page
        assert events == ["metadata", "pin", "native_refresh"]
    finally:
        subscription.close()
    assert events == ["metadata", "pin", "native_refresh", "release"]
