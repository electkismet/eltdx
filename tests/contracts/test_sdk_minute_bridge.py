"""Authored minute bridge contracts; execution awaits all implementation sources."""
from __future__ import annotations

from types import SimpleNamespace

import pytest

from eltdx.api.minutes import MinuteApi
from eltdx.client import TdxClient
from eltdx.protocol.unit import normalize_code
from eltdx.transport import SocketTransport


class NativeMinutes:
    pool_size = 1

    def __init__(self):
        self.calls = []
        self.result = SimpleNamespace(points=("shared minute result",))

    def sdk_minutes(self, command, payload):
        self.calls.append((command, payload))
        return self.result

    def execute(self, *_args, **_kwargs):
        pytest.fail("ordinary native minutes must use the shared service")


@pytest.mark.parametrize("method,arguments,command,extra", [
    ("today", (), 0x0537, {}),
    ("history", ("2026-11-17",), 0x0FB4, {"trading_date": "2026-11-17"}),
    ("recent", (), 0x0FEB, {"trading_date": None}),
    ("aux", (), 0x051B, {"kind": "buy_sell_strength"}),
    ("sparkline", (), 0x0FD1, {"selector": 1, "window": 20}),
])
def test_native_minute_routes_preserve_payload_and_result(method, arguments, command, extra) -> None:
    transport = NativeMinutes()
    api = MinuteApi(transport, native_services=True)
    result = getattr(api, method)("sh600000", *arguments, include_raw=True)
    assert result is transport.result
    assert transport.calls == [(command, dict(code="sh600000", include_raw=True, **extra))]


@pytest.mark.parametrize("method,arguments,name", [
    ("today", (), "today_intraday"),
    ("history", (20261117,), "historical_intraday"),
    ("recent", (), "recent_intraday"),
])
def test_native_minute_keeps_resolver_order_and_projection(method, arguments, name) -> None:
    transport = NativeMinutes()
    events = []
    projected = object()

    def resolve(command, result, codes):
        events.append((command, result, codes, len(transport.calls)))
        return projected if result is not None else None

    api = MinuteApi(transport, price_resolver=resolve, native_services=True)
    assert getattr(api, method)("600000", *arguments) is projected
    assert events == [(name, None, ["600000"], 0),
                      (name, transport.result, ["600000"], 1)]


@pytest.mark.parametrize("method", ["aux", "sparkline", "buy_sell_strength", "volume_comparison"])
def test_native_auxiliary_minutes_do_not_resolve_prices(method) -> None:
    transport = NativeMinutes()

    def resolve(*_args):
        pytest.fail("auxiliary wire values must not load a security directory")

    api = MinuteApi(transport, price_resolver=resolve, native_services=True)
    assert getattr(api, method)("sh600000") is transport.result


@pytest.mark.parametrize("method,kind", [("buy_sell_strength", "buy_sell_strength"),
                                        ("volume_comparison", "volume_comparison")])
def test_native_minute_aliases_keep_selectors(method, kind) -> None:
    transport = NativeMinutes()
    getattr(MinuteApi(transport, native_services=True), method)("sh600000", include_raw=True)
    assert transport.calls == [(0x051B, dict(code="sh600000", kind=kind, include_raw=True))]


def test_native_minute_batch_keeps_normalized_first_occurrence_order() -> None:
    transport = NativeMinutes()
    result = MinuteApi(transport, native_services=True).today(
        ["600000", "sh600000", "000001"], batch_size=1,
    )
    expected = [normalize_code("600000"), normalize_code("000001")]
    assert list(result) == expected
    assert [payload["code"] for _, payload in transport.calls] == expected
    assert all(value is transport.result for value in result.values())


def test_native_minute_explicit_batch_size_retains_original_worker_count(monkeypatch) -> None:
    import eltdx.api.minutes as module

    workers = []

    class InlineExecutor:
        def __init__(self, *, max_workers):
            workers.append(max_workers)

        def __enter__(self):
            return self

        def __exit__(self, *_args):
            return None

        def map(self, query, items):
            return map(query, items)

    monkeypatch.setattr(module, "ThreadPoolExecutor", InlineExecutor)
    api = MinuteApi(NativeMinutes(), native_services=True)
    api.today(["sh600000", "sh600001", "sh600002"], batch_size=3)
    api.today(["sh600000", "sh600001", "sh600002"])
    assert workers == [3, 1]


@pytest.mark.parametrize("method,arguments", [("today", ()), ("history", (20261117,)), ("recent", ())])
def test_native_single_minute_ignores_unused_batch_size(method, arguments) -> None:
    transport = NativeMinutes()
    assert getattr(MinuteApi(transport, native_services=True), method)(
        "sh600000", *arguments, batch_size=object(),
    ) is transport.result


def test_native_minute_resolver_failure_precedes_transport() -> None:
    transport = NativeMinutes()
    failure = ValueError("metadata callback failed")

    def resolve(*_args):
        raise failure

    with pytest.raises(ValueError) as caught:
        MinuteApi(transport, native_services=True, price_resolver=resolve).today("600000")
    assert caught.value is failure
    assert not transport.calls


def test_custom_minute_transport_keeps_original_execute_extension() -> None:
    result = object()
    calls = []

    class CustomTransport:
        def execute(self, command, payload):
            calls.append((command, payload))
            return result

    assert MinuteApi(CustomTransport()).today("600000") is result
    assert calls == [(0x0537, dict(code="600000", include_raw=False))]


def test_socket_subclass_keeps_custom_minute_execute() -> None:
    class CustomSocket(SocketTransport):
        def execute(self, command, payload=None):
            return command, payload

    client = TdxClient(transport=CustomSocket(["127.0.0.1:7709"]))
    assert not client.minutes._native_services
    assert client.minutes.aux("sh600000", kind=255)[0] == 0x051B
