"""Authored special-limit bridge and local health compatibility cases."""
from __future__ import annotations

import pytest

from eltdx.api.limits import LimitApi
from eltdx.api.health import ping
from eltdx.client import TdxClient
from eltdx.models import SpecialLimitPage, SpecialLimitRecord
from eltdx.transport import PooledSocketTransport, SocketTransport


def _row(number):
    return SpecialLimitRecord("sh", 1, number, f"{number:06d}", 12.0, 8.0)


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
def test_native_special_limit_scan_uses_actual_count_and_keeps_over_target_page(transport_type, monkeypatch):
    transport = transport_type(["127.0.0.1:7709"], heartbeat_interval=None)
    calls = []

    def page(payload):
        start = payload["start_index"]
        calls.append(start)
        return SpecialLimitPage(start, (_row(start), _row(start + 1)))

    monkeypatch.setattr(transport, "sdk_special_limits", page)
    result = LimitApi(transport, native_services=True).scan_special(start_index=5, max_rows=3)
    assert [row.code_num for row in result] == [5, 6, 7, 8]
    assert calls == [5, 7]


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
def test_native_special_limit_scan_stops_empty_and_skips_zero_max(transport_type, monkeypatch):
    transport = transport_type(["127.0.0.1:7709"], heartbeat_interval=None)
    calls = []
    monkeypatch.setattr(transport, "sdk_special_limits", lambda payload: calls.append(payload) or SpecialLimitPage(payload["start_index"], ()))
    api = LimitApi(transport, native_services=True)
    assert api.scan_special(max_rows=0) == [] and calls == []
    assert api.scan_special(start_index=3) == []
    assert calls == [{"start_index": 3}]


def test_special_limit_custom_execute_and_page_override_stay_observable():
    class Transport:
        def execute(self, command, payload):
            assert command == 0x0452
            return SpecialLimitPage(payload["start_index"], ())

    api = LimitApi(Transport(), native_services=True)
    assert api.scan_special() == []
    calls = []
    api.special = lambda **kwargs: calls.append(kwargs) or SpecialLimitPage(0, ())
    assert api.scan_special() == [] and calls == [{"start_index": 0}]


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
def test_special_limit_builtin_execute_override_is_preserved(transport_type, monkeypatch):
    transport = transport_type(["127.0.0.1:7709"], heartbeat_interval=None)
    result = object()
    monkeypatch.setattr(transport, "execute", lambda *_: result)
    monkeypatch.setattr(transport, "sdk_special_limits", lambda *_: pytest.fail("override bypassed"))
    assert LimitApi(transport, native_services=True).special() is result


def test_local_health_helpers_keep_default_and_custom_ping_without_native_start():
    assert ping() == "pong"

    class Transport:
        def request(self, command):
            assert command == "ping"
            return "custom pong"

    assert ping(TdxClient(transport=Transport(), probe_hosts=False)) == "custom pong"


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
def test_client_health_ping_keeps_native_engine_uninitialized(transport_type):
    transport = transport_type(["127.0.0.1:7709"], heartbeat_interval=None)
    client = TdxClient(transport=transport, probe_hosts=False)
    assert client.ping() == "pong" and transport._engine is None
    client.close()
    assert client.ping() == "pong" and transport._engine is None
