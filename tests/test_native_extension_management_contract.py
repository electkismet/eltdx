"""Authored client-management contracts; execution follows the full source gate."""
from __future__ import annotations

import sys
from types import SimpleNamespace

import pytest

from eltdx.exceptions import TransportError
from eltdx.ex import client as ex
from eltdx.ex import mac_client as mac_ex
from eltdx.mac import client as mac


def stub_connection(monkeypatch, module, events):
    def create(host, port, timeout, **kwargs):
        events.append(("construct", host, port, timeout, kwargs))
        return SimpleNamespace(
            host=host,
            connect=lambda: events.append(("connect", host)),
            close=lambda: events.append(("close", host)),
            execute=lambda command: events.append(("execute", host, command)),
        )

    monkeypatch.setattr(module, "ExTdxConnection" if module is not mac else "MacConnection", create)
    return create


def ex_client(monkeypatch, events, **kwargs):
    stub_connection(monkeypatch, ex, events)
    return ex.ExTdxClient(hosts=["a", "b"], probe_on_connect=False, **kwargs)


def test_native_ex_management_constructor_keeps_candidates_and_defaults(monkeypatch):
    events = []
    stub_connection(monkeypatch, ex, events)
    monkeypatch.setattr(ex, "load_ex_hosts", lambda: ["b", "a", "b"])
    client = ex.ExTdxClient("b", timeout=30)
    assert client._hosts == ["b", "a"]
    assert client.host == "b" and not client._connected
    assert client._probe_timeout == min(ex.DEFAULT_PROBE_TIMEOUT, 30)
    assert client._timeout == 30 and events[0][0:2] == ("construct", "b")


def test_native_ex_management_empty_hosts_reject_before_connection(monkeypatch):
    events = []
    stub_connection(monkeypatch, ex, events)
    with pytest.raises(ValueError, match="hosts 不能为空"):
        ex.ExTdxClient(hosts=[])
    assert events == []


def test_native_mac_management_none_defaults_and_explicit_empty(monkeypatch):
    monkeypatch.setattr(mac, "MAC_HOSTS", ["default"])
    client = mac.MacClient(port=0, timeout=0)
    assert client.host == "default" and client._port == 0 and client._timeout == 0
    with pytest.raises(ValueError, match="至少需要一个 host"):
        mac.MacClient("explicit", hosts=[])


def test_native_mac_management_stringifies_hosts_but_preserves_single_host(monkeypatch):
    marker = object()
    assert mac.MacClient(marker).host is marker
    seen = []

    class Host:
        def __str__(self):
            seen.append("str")
            return "converted"

    client = mac.MacClient(hosts=[Host(), Host()])
    assert client.hosts == ["converted", "converted"] and seen == ["str", "str"]
    copy = client.hosts
    copy.clear()
    assert client.hosts == ["converted", "converted"]


def test_native_mac_ex_management_empty_hosts_uses_defaults(monkeypatch):
    events = []
    stub_connection(monkeypatch, mac_ex, events)
    monkeypatch.setattr(mac_ex, "MAC_EX_HOSTS", ["default", "second"])
    client = mac_ex.MacExClient(hosts=[])
    assert client._hosts == ["default", "second"] and client._host == "default"
    assert events[0][-1] == {"mac_ex_mode": True}
    assert client._bounds_cache == {} and client._macro_cache == {}


def test_native_ex_management_order_preferred_exclusion_and_reset(monkeypatch):
    client = ex_client(monkeypatch, [])
    client._ordered = ["b", "a", "c"]
    client._preferred_host = "a"
    client._unhealthy = {"a": "bad"}
    assert client._candidate_order() == ["b", "c"]
    client._unhealthy = dict.fromkeys(client._ordered, "bad")
    assert client._candidate_order() == ["a", "b", "c"] and client._unhealthy == {}


def test_native_ex_management_health_properties_return_copies(monkeypatch):
    client = ex_client(monkeypatch, [])
    client._ordered = ["b", "a", "b", "c"]
    client._healthy = {"a", "b", "c"}
    client._unhealthy = {"a": "bad"}
    assert client.healthy_hosts == ["b", "b", "c"]
    copy = client.unhealthy_hosts
    copy.clear()
    assert client._unhealthy == {"a": "bad"}


def test_native_ex_management_seed_uses_full_candidates_and_live_rank(monkeypatch):
    client = ex_client(monkeypatch, [])
    results = [SimpleNamespace(host="a", ok=True), SimpleNamespace(host="b", ok=False)]
    seen = []
    monkeypatch.setattr(ex, "rank_ex_hosts", lambda value, **kwargs: seen.append((value, kwargs)) or ["b", "a"])
    client._unhealthy["a"] = "old"
    client._seed_from_probe(results)
    assert seen == [(results, {"candidates": client._hosts})]
    assert client._ordered == ["b", "a"] and client._healthy == {"a"}
    assert client._unhealthy == {} and client._probed


def test_native_ex_management_refresh_preserves_override_and_none_arguments(monkeypatch):
    client = ex_client(monkeypatch, [])
    marker = object()
    seen = []
    client._probe_and_seed = lambda **kwargs: seen.append(kwargs) or marker
    assert client.refresh_hosts(timeout=None, max_workers=0) is marker
    assert seen == [{"timeout": None, "max_workers": 0}]


def test_native_ex_management_remaining_probe_keeps_bad_hosts_in_rank(monkeypatch):
    client = ex_client(monkeypatch, [])
    client._hosts = ["a", "b", "c"]
    client._unhealthy = {"a": "bad"}
    client._probed = False
    results = [SimpleNamespace(host="c", ok=True)]
    seen = []
    monkeypatch.setattr(ex, "probe_ex_hosts", lambda *args, **kw: seen.append((args, kw)) or results)
    monkeypatch.setattr(ex, "rank_ex_hosts", lambda value, **kw: seen.append((value, kw)) or ["c", "b", "a"])
    client._reprobe_remaining()
    assert seen[0][0][0] == ["b", "c"] and seen[0][1]["stop_after_first"] is True
    assert seen[1] == (results, {"candidates": ["a", "b", "c"]})
    assert client._healthy == {"c"} and not client._probed and client._unhealthy == {"a": "bad"}


def test_native_ex_management_one_remaining_skips_probe(monkeypatch):
    client = ex_client(monkeypatch, [])
    client._unhealthy = {"a": "bad"}
    monkeypatch.setattr(ex, "probe_ex_hosts", lambda *a, **k: pytest.fail("one remaining must not probe"))
    client._reprobe_remaining()


def test_native_ex_management_connect_catches_constructor_and_preserves_old_connection(monkeypatch):
    events = []
    client = ex_client(monkeypatch, events)
    old = client._conn
    failure = TransportError("constructor")
    replacement = SimpleNamespace(connect=lambda: events.append("ready"))

    def create(host, *args):
        if host == "a":
            raise failure
        return replacement

    monkeypatch.setattr(ex, "ExTdxConnection", create)
    client.connect()
    assert client._conn is replacement and client.host == "b" and client._connected
    assert client._unhealthy == {"a": "TransportError: constructor"}
    assert ("close", old.host) not in events


def test_native_ex_management_last_attempt_reconnects_before_raising(monkeypatch):
    client = ex_client(monkeypatch, [])
    client._hosts = ["a"]
    client._connected = True
    failure = TransportError("wire")
    events = []

    def fail(command):
        raise failure

    client._conn = SimpleNamespace(execute=fail, close=lambda: events.append("close"))
    client._reprobe_remaining = lambda: events.append(("probe", sys.exc_info()[1] is failure))
    client.connect = lambda: events.append("connect")
    with pytest.raises(TransportError) as raised:
        client._execute(object())
    assert raised.value is failure
    assert events == ["close", ("probe", True), "connect"]
    assert client._unhealthy == {"a": "TransportError: wire"}


def test_native_ex_management_connect_error_replaces_request_error_with_context(monkeypatch):
    client = ex_client(monkeypatch, [])
    client._connected = True
    request_error = TransportError("request")
    connect_error = TransportError("reconnect")

    def fail(command):
        raise request_error

    def reconnect():
        assert sys.exc_info()[1] is request_error
        raise connect_error

    client._conn = SimpleNamespace(execute=fail, close=lambda: None)
    client._reprobe_remaining = reconnect
    with pytest.raises(TransportError) as raised:
        client._execute(object())
    assert raised.value is connect_error and connect_error.__context__ is request_error


def test_native_ex_management_no_failover_does_not_mark_or_close(monkeypatch):
    client = ex_client(monkeypatch, [], auto_failover=False)
    client._connected = True
    failure = TransportError("wire")

    def fail(command):
        raise failure

    client._conn = SimpleNamespace(execute=fail, close=lambda: pytest.fail("no cleanup here"))
    with pytest.raises(TransportError) as raised:
        client._execute(object())
    assert raised.value is failure and client._unhealthy == {} and client._connected


def test_native_ex_management_success_updates_health(monkeypatch):
    client = ex_client(monkeypatch, [])
    client._connected = True
    result = object()
    client._conn = SimpleNamespace(execute=lambda command: result)
    client._unhealthy = {"a": "old"}
    assert client._execute(object()) is result
    assert client._healthy == {"a"} and client._unhealthy == {}


def test_native_mac_management_constructor_error_is_outside_retry(monkeypatch):
    client = mac.MacClient(hosts=["a", "b"])
    seen = []
    failure = TransportError("constructor")

    def create(host, *args, **kwargs):
        seen.append(host)
        raise failure

    monkeypatch.setattr(mac, "MacConnection", create)
    with pytest.raises(TransportError) as raised:
        client.connect()
    assert raised.value is failure and seen == ["a"]


def test_native_mac_management_failed_reconnect_keeps_previous(monkeypatch):
    client = mac.MacClient(hosts=["a", "b"])
    old = object()
    client._conn = old
    closed = []
    errors = []

    def create(host, *args, **kwargs):
        failure = TransportError(host)
        errors.append(failure)

        def fail():
            raise failure

        return SimpleNamespace(connect=fail, close=lambda: closed.append((host, sys.exc_info()[1])))

    monkeypatch.setattr(mac, "MacConnection", create)
    with pytest.raises(TransportError) as raised:
        client.connect()
    assert raised.value is errors[-1] and client._conn is old
    assert closed == [("a", errors[0]), ("b", errors[1])]


def test_native_mac_management_success_publishes_before_previous_close(monkeypatch):
    client = mac.MacClient(hosts=["a"])
    replacement = SimpleNamespace(connect=lambda: None)
    seen = []
    client._conn = SimpleNamespace(close=lambda: seen.append(client._conn is replacement))
    monkeypatch.setattr(mac, "MacConnection", lambda *a, **k: replacement)
    client.connect()
    assert seen == [True] and client._conn is replacement


def test_native_mac_management_false_error_uses_fallback(monkeypatch):
    client = mac.MacClient(hosts=["a"])

    class FalseError(TransportError):
        def __bool__(self):
            return False

    def fail():
        raise FalseError("hidden")

    monkeypatch.setattr(mac, "MacConnection", lambda *a, **k: SimpleNamespace(connect=fail, close=lambda: None))
    with pytest.raises(TransportError, match="无可用 MAC 服务器"):
        client.connect()


def test_native_mac_management_execute_retries_once_in_active_context(monkeypatch):
    client = mac.MacClient(hosts=["a", "b"])
    command = object()
    first = TransportError("first")
    second = RuntimeError("second")
    events = []

    def fail(value):
        assert value is command
        raise first

    def retry(value):
        assert value is command and sys.exc_info()[1] is first
        raise second

    client._conn = SimpleNamespace(execute=fail, close=lambda: events.append("close"))

    def reconnect():
        events.append(("connect", client._host_index, sys.exc_info()[1] is first))
        client._conn = SimpleNamespace(execute=retry)

    client.connect = reconnect
    with pytest.raises(RuntimeError) as raised:
        client._execute(command)
    assert raised.value is second and second.__context__ is first
    assert events == ["close", ("connect", 1, True)]


def test_native_mac_management_ensure_captures_execute_before_command(monkeypatch):
    client = mac.MacClient(hosts=["a"])
    command = object()
    seen = []
    client._execute = lambda value: seen.append(value)

    def create(*args):
        assert args == (0, 1)
        client._execute = lambda value: pytest.fail("execute must already be captured")
        return command

    monkeypatch.setattr(mac, "KlineOffsetCmd", create)
    client.ensure_connected()
    assert seen == [command]


def test_native_mac_ex_management_false_login_is_legacy_success(monkeypatch):
    events = []
    stub_connection(monkeypatch, mac_ex, events)
    client = mac_ex.MacExClient(hosts=["a"])
    command = object()
    monkeypatch.setattr(mac_ex, "MacExLoginCmd", lambda: command)
    candidate = SimpleNamespace(connect=lambda: None, execute=lambda value: False)
    monkeypatch.setattr(mac_ex, "ExTdxConnection", lambda *a, **k: candidate)
    client.connect()
    assert client._conn is candidate and client._connected and client._host == "a"


def test_native_mac_ex_management_publishes_failed_candidate_and_keeps_caches(monkeypatch):
    events = []
    stub_connection(monkeypatch, mac_ex, events)
    client = mac_ex.MacExClient(hosts=["a"])
    client._bounds_cache[1] = (1, 2)
    client._macro_cache["x"] = object()
    bounds, macros = client._bounds_cache, client._macro_cache
    failure = TransportError("login")

    def fail(value):
        assert client._conn is candidate
        raise failure

    candidate = SimpleNamespace(connect=lambda: None, execute=fail, close=lambda: None)
    monkeypatch.setattr(mac_ex, "ExTdxConnection", lambda *a, **k: candidate)
    with pytest.raises(TransportError) as raised:
        client.connect()
    assert raised.value is failure and client._conn is candidate
    client.close()
    assert client._bounds_cache is bounds and client._macro_cache is macros


@pytest.mark.parametrize("module,cls", [(ex, ex.ExTdxClient), (mac_ex, mac_ex.MacExClient), (mac, mac.MacClient)])
def test_native_extension_management_close_failure_preserves_state(monkeypatch, module, cls):
    stub_connection(monkeypatch, module, [])
    client = cls(hosts=["a"])
    failure = RuntimeError("close")

    def fail():
        raise failure

    conn = SimpleNamespace(close=fail)
    client._conn = conn
    client._connected = True
    with pytest.raises(RuntimeError) as raised:
        client.close()
    assert raised.value is failure and client._conn is conn and client._connected


@pytest.mark.parametrize("module,cls", [(ex, ex.ExTdxClient), (mac_ex, mac_ex.MacExClient), (mac, mac.MacClient)])
def test_native_extension_management_context_preserves_instance_overrides(monkeypatch, module, cls):
    stub_connection(monkeypatch, module, [])
    client = cls(hosts=["a"])
    events = []
    client.connect = lambda: events.append("connect")
    client.close = lambda: events.append("close")
    assert client.__enter__() is client
    assert client.__exit__(None, None, None) is None
    assert events == ["connect", "close"]


def test_native_mac_management_best_host_probes_before_consuming_candidates(monkeypatch):
    events = []

    class Hosts:
        def __iter__(self):
            events.append("iterate")
            yield "a"
            yield "b"

    hosts = Hosts()
    monkeypatch.setattr(mac, "ping_mac_all", lambda *a: events.append("ping") or [("b", 0.1)])

    class Capture(mac.MacClient):
        def __init__(self, *args, **kwargs):
            self.arguments = args, kwargs

    client = Capture.from_best_host(hosts)
    assert events == ["ping", "iterate"]
    assert client.arguments == (("b", None, None), {"hosts": ["b", "a"]})


def test_native_ex_management_best_host_seeds_without_connect(monkeypatch):
    results = [SimpleNamespace(host="b", ok=True)]
    monkeypatch.setattr(ex, "probe_ex_hosts", lambda *a: results)
    monkeypatch.setattr(ex, "rank_ex_hosts", lambda value, **kw: ["b", "a"])
    seen = []

    class Capture(ex.ExTdxClient):
        def __init__(self, *args, **kwargs):
            seen.append((args, kwargs))

        def _seed_from_probe(self, values):
            seen.append(values)

        def connect(self):
            pytest.fail("from_best_host must not connect")

    Capture.from_best_host(["a", "b"])
    assert seen == [(("b", ex.DEFAULT_EX_PORT, 15.0), {"hosts": ["a", "b"], "probe_on_connect": False}), results]


def test_native_mac_ex_management_ping_keeps_login_factory(monkeypatch):
    seen = []
    marker = object()
    monkeypatch.setattr(mac_ex, "MAC_EX_HOSTS", ["default"])
    monkeypatch.setattr(mac_ex, "ping_ex_all", lambda *a, **k: seen.append((a, k)) or marker)
    assert mac_ex.MacExClient.ping_all([], 0, 0) is marker
    assert seen == [((["default"], 0, 0), {"mac_ex_mode": True, "command_factory": mac_ex.MacExLoginCmd})]


def test_native_ex_management_probe_captures_callable_before_reading_hosts(monkeypatch):
    seen = []
    results = []
    first = lambda *a, **k: seen.append((a, k)) or results
    monkeypatch.setattr(ex, "probe_ex_hosts", first)

    class Client(ex.ExTdxClient):
        def __getattribute__(self, name):
            if name == "_hosts":
                monkeypatch.setattr(ex, "probe_ex_hosts", lambda *a, **k: pytest.fail("probe already captured"))
            return super().__getattribute__(name)

    client = object.__new__(Client)
    client._hosts, client._port = ["a"], 123
    client._probe_timeout, client._max_probe_workers = 4, 6
    client._seed_from_probe = lambda value: seen.append(value)
    assert client._probe_and_seed() is results
    assert seen == [((["a"], 123, 4), {"max_workers": 6, "stop_after_first": False}), results]


def test_native_mac_ex_management_order_uses_live_current_host(monkeypatch):
    stub_connection(monkeypatch, mac_ex, [])
    client = mac_ex.MacExClient(hosts=["a", "b"])
    first = client._host

    class Host:
        def __ne__(self, other):
            assert other == first
            client._host = "changed"
            return True

    marker = Host()
    client._hosts = [marker, "changed", "tail"]
    assert client._ordered_hosts() == [first, marker, "tail"]
