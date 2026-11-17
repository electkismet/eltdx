"""Authored money-flow source-loop contracts; all execution is deferred."""
from __future__ import annotations

from types import SimpleNamespace

import pytest

from eltdx.api.money_flow import MoneyFlowApi, _EmptyFailover
from eltdx.exceptions import TransportError
from eltdx.models import MoneyFlowBatch, MoneyFlowBlock
from eltdx.transport import PooledSocketTransport, SocketTransport


CODE = "sh" + str(600_001)
EMPTY = MoneyFlowBatch(())
BLOCK = MoneyFlowBlock("sh", 1, "600001", ())
ONE = MoneyFlowBatch((BLOCK,))


class Transport:
    pool_size = 1

    def __init__(self, host, value):
        self.host, self.value = host, value
        self.calls = []
        self.closed = False

    def execute(self, command, payload):
        assert command == 0x0FFC
        self.calls.append(payload.copy())
        value = self.value(payload["code"]) if callable(self.value) else self.value
        if isinstance(value, BaseException):
            raise value
        return value

    def close(self):
        self.closed = True


class Factory:
    def __init__(self, sources, primary=EMPTY):
        self.sources, self.primary = sources, primary
        self.opened, self.transports = [], []

    def __call__(self, host=None):
        if host is not None:
            self.opened.append(host)
        value = self.primary if host is None else self.sources[host]
        if isinstance(value, BaseException):
            raise value
        transport = Transport(host, value)
        self.transports.append(transport)
        return transport


def _api(factory, native):
    return MoneyFlowApi(Transport(None, EMPTY), transport_factory=factory,
                        candidate_hosts=tuple(factory.sources), native_services=native)


@pytest.mark.parametrize("native", [False, True])
def test_money_flow_rotation_keeps_later_codes_and_duplicate_batch_entries(native):
    factory = Factory({"h1": EMPTY, "h2": EMPTY, "h3": EMPTY,
                       "h4": lambda code: ONE if code == "sh600002" else EMPTY})
    api = _api(factory, native)
    result = api.daily([CODE, "sh600002", "sh600002"])
    assert result.blocks == (BLOCK, BLOCK)
    assert factory.opened == ["h1", "h2", "h3", "h2", "h3", "h4"]
    assert api._preferred_host == "h4"
    api.close()
    assert all(transport.closed for transport in factory.transports)


@pytest.mark.parametrize("native", [False, True])
def test_money_flow_skipped_hosts_consume_budget_without_rotating_on_exhaustion(native):
    factory = Factory({"h1": EMPTY, "h2": EMPTY, "h3": EMPTY, "h4": EMPTY})
    api = _api(factory, native)
    api._preferred_host = "h1"
    state = _EmptyFailover(1)
    assert api._recover_empty(CODE, include_raw=False, failover=state) is None
    assert factory.opened == [] and state.probes_left == 0 and state.cursor == 0
    state.probes_left = 3
    assert api._recover_empty(CODE, include_raw=False, failover=state) is None
    assert factory.opened == ["h2", "h3"] and state.cursor == 1


@pytest.mark.parametrize("native", [False, True])
def test_money_flow_probe_errors_and_failed_factories_keep_empty_semantics(native):
    factory = Factory({"h1": TransportError("factory failed"),
                       "h2": lambda code: TransportError("request failed"), "h3": EMPTY})
    api = _api(factory, native)
    result = api.daily(CODE)
    assert result is EMPTY and factory.opened == ["h1", "h2", "h3"]
    probes = [transport for transport in factory.transports if transport.host is not None]
    assert len(probes) == 2 and all(transport.closed for transport in probes)
    api.close()


@pytest.mark.parametrize("native", [False, True])
def test_money_flow_selected_source_error_is_not_swallowed(native):
    root = TransportError("selected failed")
    selected = Transport("selected", root)
    factory = Factory({"h1": ONE})
    api = _api(factory, native)
    state = _EmptyFailover(3)
    state.transport = selected
    with pytest.raises(TransportError) as actual:
        api._recover_empty(CODE, include_raw=False, failover=state)
    assert actual.value is root and factory.opened == [] and state.probes_left == 3
    assert not selected.closed


@pytest.mark.parametrize("native", [False, True])
def test_money_flow_new_unadopted_source_is_closed_and_result_returned(native):
    factory = Factory({"h1": EMPTY, "h2": ONE, "h3": EMPTY})
    api = _api(factory, native)
    existing = Transport("existing", EMPTY)
    api._preferred_transport, api._preferred_host = existing, "existing"
    state = _EmptyFailover(3)
    assert api._recover_empty(CODE, include_raw=True, failover=state) is BLOCK
    assert api._preferred_transport is existing and state.transport is None
    assert all(transport.closed for transport in factory.transports)
    api.close()
    assert existing.closed


@pytest.mark.parametrize("native", [False, True])
def test_money_flow_unknown_initial_response_never_probes(native):
    result = object()
    factory = Factory({"h1": ONE}, primary=result)
    api = _api(factory, native)
    assert api.daily(CODE) is result
    assert factory.opened == []
    api.close()


@pytest.mark.parametrize("native", [False, True])
def test_money_flow_recovery_keeps_custom_query_callback(native):
    factory = Factory({"h1": EMPTY})
    api = _api(factory, native)
    custom = object()
    calls = []
    api._query = lambda transport, code, raw: calls.append((transport.host, code, raw)) or custom
    assert api.daily(CODE, include_raw=True) is custom
    assert calls == [("h1", CODE, True)]
    api.close()


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
def test_money_flow_native_request_retains_public_block_projection(transport_type, monkeypatch):
    transport = transport_type(["127.0.0.1:7709"], heartbeat_interval=None)
    calls = []
    monkeypatch.setattr(transport, "sdk_money_flow", lambda *args: calls.append(args) or ONE)
    assert MoneyFlowApi(transport, native_services=True).daily(CODE, include_raw=True) is BLOCK
    assert calls == [(0x0FFC, dict(code=CODE, include_raw=True))]


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
def test_money_flow_execute_replacement_keeps_custom_response(transport_type, monkeypatch):
    transport = transport_type(["127.0.0.1:7709"], heartbeat_interval=None)
    calls = []
    monkeypatch.setattr(transport, "execute", lambda *args: calls.append(args) or ONE)
    monkeypatch.setattr(transport, "sdk_money_flow", lambda *_args: pytest.fail("override bypassed"))
    assert MoneyFlowApi(transport, native_services=True).daily(CODE) is BLOCK
    assert calls == [(0x0FFC, dict(code=CODE, include_raw=False))]


@pytest.mark.parametrize("native", [False, True])
def test_money_flow_batch_budget_remains_twenty_four_across_many_empty_codes(native):
    factory = Factory({f"h{index}": EMPTY for index in range(8)})
    api = _api(factory, native)
    assert api.daily([f"sh{600000 + index}" for index in range(20)]).blocks == ()
    assert len(factory.opened) == 24
    api.close()


@pytest.mark.parametrize("native", [False, True])
def test_money_flow_history_retains_f10_route_and_never_uses_daily(native):
    requests = []
    response = SimpleNamespace(rows=({"N001": "2026-09-10"}, {"N001": "2026-09-11"}), raw={"text": "中文"})

    def f10():
        return SimpleNamespace(params=lambda *args: requests.append(args) or response)

    result = MoneyFlowApi(Transport(None, RuntimeError("daily invoked")), f10_factory=f10,
                          native_services=native).history(CODE, days=10, count=1, include_raw=True)
    assert result.rows == ({"N001": "2026-09-11"},)
    assert requests == [("CWServ.tdxf10_gg_sj", "zjlx_sj", "600001", "10", "600001")]
    assert result.raw_payload == '{"text":"中文"}'.encode("utf-8")


@pytest.mark.parametrize("native", [False, True])
def test_money_flow_empty_record_block_does_not_trigger_source_probe(native):
    factory = Factory({"h1": ONE}, primary=ONE)
    api = _api(factory, native)
    assert api.daily(CODE) is BLOCK and BLOCK.count == 0
    assert factory.opened == []
    api.close()
