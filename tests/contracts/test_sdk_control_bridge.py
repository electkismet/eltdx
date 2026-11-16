"""Authored session/company bridge contracts, deferred with the full SDK suite."""
from __future__ import annotations

import pytest

from eltdx.api.company_info import CompanyInfoApi
from eltdx.api.session import SessionApi
from eltdx.client import TdxClient
from eltdx.transport import PooledSocketTransport, SocketTransport


class NativeControl:
    def __init__(self):
        self.calls = []
        self.value = object()

    def sdk_session(self, command, payload):
        self.calls.append(("session", command, payload))
        return self.value

    def sdk_company_info(self, command, payload):
        self.calls.append(("company_info", command, payload))
        return self.value

    def execute(self, *_args, **_kwargs):
        pytest.fail("ordinary native control calls must enter shared Rust services")

    def request(self, command):
        self.calls.append(("local", command))
        return "pong"


@pytest.mark.parametrize("method,command", [("handshake", 0x000D), ("heartbeat", 0x0004)])
def test_native_session_routes_keep_unmodified_result(method, command) -> None:
    transport = NativeControl()
    assert getattr(SessionApi(transport, native_services=True), method)() is transport.value
    assert transport.calls == [("session", command, {})]


def test_native_company_routes_keep_explicit_market_and_byte_parameters() -> None:
    transport = NativeControl()
    api = CompanyInfoApi(transport, native_services=True)
    assert api.category("sh600000", "sz") is transport.value
    assert api.content("600000", "sz", " 公司.txt ", 0xFFFFFFFF, 2) is transport.value
    assert transport.calls == [
        ("company_info", 0x02CF, dict(code="sh600000", market="sz")),
        ("company_info", 0x02D0, dict(code="600000", market="sz", filename=" 公司.txt ",
                                   offset=0xFFFFFFFF, length=2)),
    ]


def test_session_ping_keeps_local_transport_extension_point() -> None:
    transport = NativeControl()
    assert SessionApi(transport, native_services=True).ping() == "pong"
    assert transport.calls == [("local", "ping")]


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
def test_session_ping_does_not_initialize_native_engine_before_or_after_close(transport_type) -> None:
    transport = transport_type(["127.0.0.1:7709"], heartbeat_interval=None)
    session = SessionApi(transport, native_services=True)
    assert session.ping() == "pong" and transport._engine is None
    transport.close()
    assert session.ping() == "pong" and transport._engine is None


def test_custom_control_transport_keeps_untyped_execute_return_values() -> None:
    calls = []
    result = object()

    class CustomTransport:
        def execute(self, command, payload):
            calls.append((command, payload))
            return result

    transport = CustomTransport()
    assert SessionApi(transport).handshake() is result
    assert CompanyInfoApi(transport).category("600000", "sh") is result
    assert calls == [(0x000D, {}), (0x02CF, dict(code="600000", market="sh"))]


def test_transport_subclass_keeps_session_and_company_execute_override() -> None:
    class CustomSocket(SocketTransport):
        def execute(self, command, payload=None):
            return command, payload

    client = TdxClient(transport=CustomSocket(["127.0.0.1:7709"]))
    assert not client.session._native_services
    assert not client.company_info._native_services
    assert client.session.heartbeat() == (0x0004, {})
    assert client.company_info.category("600000", "sh")[0] == 0x02CF


def test_control_bridge_propagates_original_exception_object() -> None:
    failure = ValueError("original request failure")

    class FailingControl:
        def sdk_session(self, *_args):
            raise failure

    with pytest.raises(ValueError) as caught:
        SessionApi(FailingControl(), native_services=True).handshake()
    assert caught.value is failure and caught.value.__cause__ is None
