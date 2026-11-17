"""Probe contracts authored for the final verification phase; no network here."""
from __future__ import annotations

import sys
import types
from types import SimpleNamespace

import pytest

from eltdx.exceptions import TransportError
from eltdx.ex.transport import sync as ex
from eltdx.mac import commands, transport as mac


class Future:
    def __init__(self, value=None, error=None):
        self.value, self.error = value, error

    def result(self):
        if self.error is not None:
            raise self.error
        return self.value


class Pool:
    def __init__(self, events, futures):
        self.events, self.futures = events, iter(futures)

    def submit(self, fn, *args, **kwargs):
        self.events.append(("submit", fn, args, kwargs))
        return next(self.futures)

    def shutdown(self, **kwargs):
        self.events.append(("shutdown", kwargs))

    def __enter__(self):
        self.events.append("enter")
        return self

    def __exit__(self, *args):
        self.events.append(("exit", args[1]))


def ex_pool(monkeypatch, futures):
    events = []
    pool = Pool(events, futures)
    registry = SimpleNamespace(abort_all=lambda: events.append("abort"))
    monkeypatch.setattr(ex, "_ProbeRegistry", lambda: registry)
    monkeypatch.setattr(ex, "ThreadPoolExecutor", lambda **kwargs: events.append(("pool", kwargs)) or pool)
    monkeypatch.setattr(ex, "as_completed", lambda values: values)
    return events, pool, registry


def test_native_ex_probe_success_times_after_close(monkeypatch):
    events = []
    ticks = iter((1.0, 1.123456))
    monkeypatch.setattr(ex.time, "monotonic", lambda: events.append("time") or next(ticks))
    command = object()
    connection = SimpleNamespace(
        connect=lambda: events.append("connect"),
        execute=lambda value: events.append(("execute", value)),
        close=lambda: events.append("close"),
    )
    monkeypatch.setattr(ex, "ExTdxConnection", lambda *a, **k: events.append(("construct", a, k)) or connection)
    registry = SimpleNamespace(add=lambda value: events.append("add"), discard=lambda value: events.append("discard"))
    result = ex.probe_ex_host("a", 100, 15, command_factory=lambda: command, registry=registry)
    assert result == ex.ExHostProbe("a", True, 123.46)
    assert events == ["time", ("construct", ("a", 100, 15), {"mac_ex_mode": False}), "add", "connect", ("execute", command), "discard", "close", "time"]


def test_native_ex_probe_failure_keeps_context_and_finally_order(monkeypatch):
    events = []
    failure = TransportError("broken")

    def connect():
        raise failure

    connection = SimpleNamespace(connect=connect, close=lambda: events.append(("close", sys.exc_info()[1])))
    monkeypatch.setattr(ex, "ExTdxConnection", lambda *a, **k: connection)
    registry = SimpleNamespace(add=lambda value: None, discard=lambda value: events.append(("discard", sys.exc_info()[1])))
    result = ex.probe_ex_host("a", registry=registry)
    assert result == ex.ExHostProbe("a", False, error="TransportError: broken")
    # A return from except clears its active exception before outer finally.
    assert events == [("discard", None), ("close", None)]


def test_native_ex_probe_constructor_error_remains_outside_handler(monkeypatch):
    failure = OSError("construct")

    def create(*args, **kwargs):
        raise failure

    monkeypatch.setattr(ex, "ExTdxConnection", create)
    with pytest.raises(OSError) as raised:
        ex.probe_ex_host("a")
    assert raised.value is failure


def test_native_ex_probe_cleanup_failure_overrides_result(monkeypatch):
    failure = RuntimeError("close")

    def close():
        raise failure

    connection = SimpleNamespace(connect=lambda: None, execute=lambda value: None, close=close)
    monkeypatch.setattr(ex, "ExTdxConnection", lambda *a, **k: connection)
    with pytest.raises(RuntimeError) as raised:
        ex.probe_ex_host("a", command_factory=object)
    assert raised.value is failure


def test_native_ex_probe_registry_remove_and_abort_outside_lock():
    events = []

    class Lock:
        def __enter__(self):
            events.append("enter")

        def __exit__(self, *args):
            events.append("exit")

    registry = ex._ProbeRegistry()
    registry._lock = Lock()
    connection = SimpleNamespace(close=lambda: events.append(("close", registry._connections)))
    registry.add(connection)
    registry.discard(object())
    registry.abort_all()
    registry.abort_all()
    assert events == ["enter", "exit", "enter", "exit", "enter", "exit", ("close", []), "enter", "exit"]


def test_native_ex_probe_empty_skips_workers_and_pool(monkeypatch):
    monkeypatch.setattr(ex, "ThreadPoolExecutor", lambda **k: pytest.fail("empty probe cannot construct pool"))

    class Bad:
        def __int__(self):
            pytest.fail("empty probe cannot coerce workers")

    assert ex.probe_ex_hosts([], max_workers=Bad()) == []


def test_native_ex_probe_none_loads_config_and_clamps_workers(monkeypatch):
    results = [ex.ExHostProbe("a", True, 3.0), ex.ExHostProbe("b", False)]
    events, _, _ = ex_pool(monkeypatch, [Future(value) for value in results])
    monkeypatch.setattr(ex, "load_ex_hosts", lambda: ["a", "b"])
    assert ex.probe_ex_hosts(None, max_workers="500") == results
    assert events[0] == ("pool", {"max_workers": 2, "thread_name_prefix": "eltdx-ex-probe"})
    assert events[-2:] == ["abort", ("shutdown", {"wait": True, "cancel_futures": True})]


def test_native_ex_probe_early_success_aborts_twice_and_marks_unprobed(monkeypatch):
    good = ex.ExHostProbe("a", True, 1.0)
    events, _, registry = ex_pool(monkeypatch, [Future(good), Future(ex.ExHostProbe("b", True, 0.5))])
    result = ex.probe_ex_hosts(["a", "b"], stop_after_first=True)
    assert result == [good, ex.ExHostProbe("b", False, error="not probed")]
    assert events[-3:] == ["abort", "abort", ("shutdown", {"wait": True, "cancel_futures": True})]
    assert events[1][3]["registry"] is registry


def test_native_ex_probe_future_failure_and_duplicate_hosts_keep_last_result(monkeypatch):
    later = ex.ExHostProbe("a", True, 8.0)
    events, _, _ = ex_pool(monkeypatch, [Future(error=RuntimeError("future")), Future(later)])
    assert ex.probe_ex_hosts(["a", "a"]) == [later, later]
    assert len([event for event in events if isinstance(event, tuple) and event[0] == "submit"]) == 2


def test_native_ex_probe_future_failure_reports_only_type(monkeypatch):
    ex_pool(monkeypatch, [Future(error=RuntimeError("details excluded"))])
    assert ex.probe_ex_hosts(["a"]) == [ex.ExHostProbe("a", False, error="RuntimeError")]


def test_native_ex_probe_base_exception_runs_shutdown(monkeypatch):
    failure = KeyboardInterrupt()
    events, _, _ = ex_pool(monkeypatch, [Future(error=failure)])
    with pytest.raises(KeyboardInterrupt) as raised:
        ex.probe_ex_hosts(["a"])
    assert raised.value is failure
    assert events[-2:] == ["abort", ("shutdown", {"wait": True, "cancel_futures": True})]


def test_native_ex_probe_default_models_are_eager_even_for_found_host(monkeypatch):
    result = ex.ExHostProbe("a", True, 1.0)
    ex_pool(monkeypatch, [Future(result)])
    constructor = ex.ExHostProbe
    seen = []
    monkeypatch.setattr(ex, "ExHostProbe", lambda **kw: seen.append(kw) or constructor(**kw))
    assert ex.probe_ex_hosts(["a"]) == [result]
    assert seen == [{"host": "a", "ok": False, "error": "not probed"}]


def test_native_ex_probe_rank_preserves_duplicates_ties_and_tail():
    results = [ex.ExHostProbe("b", True, 2), ex.ExHostProbe("a", True, 2), ex.ExHostProbe("b", True, 3), ex.ExHostProbe("c", False)]
    assert ex.rank_ex_hosts(results, candidates=["c", "b", "d", "c"]) == ["b", "a", "b", "c", "d", "c"]


def test_native_ex_probe_rank_sorted_receives_generator(monkeypatch):
    original = sorted
    seen = []

    def sort(values, **kwargs):
        assert isinstance(values, types.GeneratorType)
        seen.append(list(values))
        return original(seen[-1], **kwargs)

    monkeypatch.setattr(ex, "sorted", sort, raising=False)
    good = ex.ExHostProbe("a", True, 1)
    assert ex.rank_ex_hosts([good, ex.ExHostProbe("b", False)]) == ["a", "b"]
    assert seen == [[good]]


def test_native_ex_probe_generator_preserves_pep479(monkeypatch):
    class Broken:
        host = "a"

        @property
        def ok(self):
            raise StopIteration("stop")

    with pytest.raises(RuntimeError, match="generator raised StopIteration") as raised:
        ex.rank_ex_hosts([Broken()])
    assert isinstance(raised.value.__cause__, StopIteration)


def test_native_ex_probe_ping_filters_missing_latencies_and_scales(monkeypatch):
    monkeypatch.setattr(ex, "probe_ex_hosts", lambda *a, **k: [ex.ExHostProbe("a", True, 1500), ex.ExHostProbe("b", True), ex.ExHostProbe("c", False, 2)])
    assert ex.ping_ex_all(["a", "b", "c"]) == [("a", 1.5)]
    monkeypatch.setattr(ex, "probe_ex_host", lambda *a, **k: ex.ExHostProbe("a", True, 250))
    assert ex.ping_ex_host("a") == 0.25


@pytest.mark.parametrize("today,expected", [("20260101", 0.25), ("", None)])
def test_native_mac_probe_requires_today_and_closes_before_final_time(monkeypatch, today, expected):
    events = []
    ticks = iter((5, 5.25))
    monkeypatch.setattr(mac.time, "monotonic", lambda: events.append("time") or next(ticks))
    command = object()
    monkeypatch.setattr(commands, "ServerInfoCmd", lambda: command)
    conn = SimpleNamespace(connect=lambda: None, execute=lambda value: SimpleNamespace(today=today), close=lambda: events.append("close"))
    monkeypatch.setattr(mac, "MacConnection", lambda *a, **k: conn)
    assert mac.ping_mac_host("a") == expected
    assert events == (["time", "close", "time"] if today else ["time", "close"])


def test_native_mac_probe_catches_transport_error_but_always_closes(monkeypatch):
    events = []

    def connect():
        raise TransportError("wire")

    conn = SimpleNamespace(connect=connect, close=lambda: events.append("close"))
    monkeypatch.setattr(mac, "MacConnection", lambda *a, **k: conn)
    assert mac.ping_mac_host("a") is None and events == ["close"]


def test_native_mac_probe_pool_keeps_completion_tie_order(monkeypatch):
    events = []
    pool = Pool(events, [Future(1.0), Future(1.0), Future(None)])
    monkeypatch.setattr(mac, "ThreadPoolExecutor", lambda **k: pool)
    monkeypatch.setattr(mac, "as_completed", lambda values: reversed(list(values)))
    assert mac.ping_mac_all(["a", "b", "c"]) == [("b", 1.0), ("a", 1.0)]
    assert events[0] == "enter" and events[-1] == ("exit", None)


def test_native_mac_probe_future_error_propagates_after_context_cleanup(monkeypatch):
    failure = RuntimeError("future")
    events = []
    pool = Pool(events, [Future(error=failure)])
    monkeypatch.setattr(mac, "ThreadPoolExecutor", lambda **k: pool)
    monkeypatch.setattr(mac, "as_completed", lambda values: values)
    with pytest.raises(RuntimeError) as raised:
        mac.ping_mac_all(["a"])
    assert raised.value is failure and events[-1] == ("exit", failure)


def test_native_mac_probe_falsey_hosts_use_defaults(monkeypatch):
    events = []
    pool = Pool(events, [Future(0.0)])
    monkeypatch.setattr(mac, "MAC_HOSTS", ["default"])
    monkeypatch.setattr(mac, "ThreadPoolExecutor", lambda **k: pool)
    monkeypatch.setattr(mac, "as_completed", lambda values: values)
    assert mac.ping_mac_all([]) == [("default", 0.0)]
