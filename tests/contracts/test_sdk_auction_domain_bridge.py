"""Authored auction domain/lifetime contracts; not executed during implementation."""
from __future__ import annotations

from types import SimpleNamespace

import pytest

from eltdx.api.auctions import AuctionApi
from eltdx.client import TdxClient
from eltdx.transport import PooledSocketTransport, SocketTransport


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
@pytest.mark.parametrize("trading_date", [None, "2026-09-11"])
def test_auction_native_route_keeps_public_payload_and_result(transport_type, trading_date, monkeypatch):
    transport = transport_type(["127.0.0.1:7709"], heartbeat_interval=None)
    calls = []
    expected = object()
    monkeypatch.setattr(transport, "sdk_auction", lambda *args: calls.append(args) or expected)
    api = AuctionApi(transport, native_services=True)
    assert api.series("sh600001", trading_date, include_raw=True) is expected
    assert calls == [(0x056A, dict(code="sh600001", trading_date=trading_date, include_raw=True))]


@pytest.mark.parametrize("class_override", [False, True])
def test_auction_overridden_execute_keeps_extension_point(class_override, monkeypatch):
    transport = PooledSocketTransport(["127.0.0.1:7709"], heartbeat_interval=None)
    calls = []

    def execute(command, payload):
        calls.append((command, payload))
        return "custom"

    if class_override:
        monkeypatch.setattr(PooledSocketTransport, "execute", lambda self, *args: execute(*args))
    else:
        monkeypatch.setattr(transport, "execute", execute)
    monkeypatch.setattr(transport, "sdk_auction", lambda *_args: pytest.fail("execute override bypassed"))
    assert AuctionApi(transport, native_services=True).series("sh600001") == "custom"
    assert calls == [(0x056A, dict(code="sh600001", trading_date=None, include_raw=False))]


def test_auction_factory_stays_lazy_cached_and_recreated_after_close():
    primary = SimpleNamespace(execute=lambda *_args: pytest.fail("primary pool used"))
    created, closed = [], []

    def factory():
        index = len(created)
        pool = SimpleNamespace(execute=lambda *_args: index, close=lambda: closed.append(index))
        created.append(pool)
        return pool

    api = AuctionApi(primary, transport_factory=factory, native_services=True)
    assert created == []
    assert api.series("sh600001") == api.series("sh600001", "2026-09-11") == 0
    api._close()
    assert closed == [0]
    assert api.series("sh600001") == 1
    api._close()
    assert closed == [0, 1]


def test_auction_factory_failure_keeps_original_error_and_can_retry():
    attempts = []
    root = ValueError("factory failed")

    def factory():
        attempts.append(True)
        if len(attempts) == 1:
            raise root
        return SimpleNamespace(execute=lambda *_args: "ready", close=lambda: None)

    api = AuctionApi(object(), transport_factory=factory, native_services=True)
    with pytest.raises(ValueError) as actual:
        api.series("sh600001")
    assert actual.value is root and api._dedicated_transport is None
    assert api.series("sh600001") == "ready"
    api._close()


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
def test_auction_client_transport_subclasses_keep_custom_results(transport_type):
    class Custom(transport_type):
        def execute(self, command, payload=None):
            return command, payload

    client = TdxClient(transport=Custom(["127.0.0.1:7709"], heartbeat_interval=None))
    # A pooled subclass still retains the historical dedicated factory. Replace
    # only that existing factory, preserving its custom callback behavior.
    dedicated = Custom(["127.0.0.1:7709"], heartbeat_interval=None)

    def factory():
        return dedicated

    if transport_type is PooledSocketTransport:
        from eltdx.api.base import _weak_callback
        client.auctions._transport_factory = _weak_callback(factory)
    assert client.auctions.series("sh600001", "2026-09-11") == (
        0x056A, dict(code="sh600001", trading_date="2026-09-11", include_raw=False))
    client.close()
