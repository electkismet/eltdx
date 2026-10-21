"""扩展行情主机的并发探活与失败自动负载均衡（全部离线，用假连接）。"""

from __future__ import annotations

import pytest

from eltdx.ex import client as ex_client
from eltdx.ex import transport as ex_transport
from eltdx.ex.models import ExInstrumentInfo
from eltdx.ex.transport.sync import (
    ExHostProbe,
    probe_ex_host,
    probe_ex_hosts,
    rank_ex_hosts,
)
from eltdx.exceptions import TransportError


class FakeNet:
    """可编程的假网络：按 host 指定行为。

    ``behavior``：``"ok"`` 正常应答；``"timeout"`` TCP 通但命令超时；
    ``"reset"`` TCP 通但命令被重置；``"refuse"`` TCP 都连不上。
    """

    behavior: dict[str, str] = {}
    connects: list[str] = []
    executes: list[str] = []
    probes: list[str] = []

    @classmethod
    def reset(cls, behavior: dict[str, str] | None = None) -> None:
        cls.behavior = dict(behavior or {})
        cls.connects = []
        cls.executes = []
        cls.probes = []


class FakeConnection:
    """替代客户端用的 :class:`ExTdxConnection`，行为由 :class:`FakeNet` 决定。"""

    _sink = "executes"

    def __init__(self, host=None, port=7727, timeout=15.0, *, mac_ex_mode=False):
        self.host = host if host is not None else "unknown"
        self.port = port
        self.timeout = timeout
        self.mac_ex_mode = mac_ex_mode
        self._open = False

    def _state(self) -> str:
        return FakeNet.behavior.get(self.host, "ok")

    def connect(self) -> None:
        FakeNet.connects.append(self.host)
        if self._state() == "refuse":
            raise TransportError(f"无法连接 {self.host}:{self.port}")
        self._open = True

    def close(self) -> None:
        self._open = False

    def execute(self, cmd):
        getattr(FakeNet, self._sink).append(self.host)
        state = self._state()
        if state in ("timeout", "reset"):
            raise TransportError(f"通信错误: {state}")
        return [
            ExInstrumentInfo(
                category=8,
                market=33,
                code="016947",
                name="国泰利安中短债A",
                desc="",
                _raw=b"",
            )
        ]

    def __enter__(self):
        self.connect()
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        self.close()


class FakeProbeConnection(FakeConnection):
    """探活走的假连接；流量单独记到 ``FakeNet.probes``。"""

    _sink = "probes"


@pytest.fixture(autouse=True)
def fake_network(monkeypatch):
    FakeNet.reset()
    # 客户端与探活走的是两个模块级名字，分别替换以便区分流量。
    monkeypatch.setattr(ex_client, "ExTdxConnection", FakeConnection)
    monkeypatch.setattr(ex_transport.sync, "ExTdxConnection", FakeProbeConnection)
    yield
    FakeNet.reset()


# --------------------------------------------------------------------- #
# rank_ex_hosts：纯函数
# --------------------------------------------------------------------- #


def test_rank_ex_hosts_puts_healthy_first_by_latency():
    results = [
        ExHostProbe("a", False, None, "timeout"),
        ExHostProbe("b", True, 80.0),
        ExHostProbe("c", True, 20.0),
        ExHostProbe("d", False, None, "reset"),
    ]
    assert rank_ex_hosts(results, candidates=["a", "b", "c", "d"]) == ["c", "b", "a", "d"]


def test_rank_ex_hosts_keeps_unhealthy_hosts_as_fallback():
    """不健康主机不能丢：网络抖动后还要能回退到它。"""

    results = [ExHostProbe("a", False, None, "timeout"), ExHostProbe("b", True, 1.0)]
    ranked = rank_ex_hosts(results, candidates=["a", "b"])
    assert ranked == ["b", "a"]


def test_rank_ex_hosts_healthy_without_latency_sorts_last():
    results = [ExHostProbe("a", True, None), ExHostProbe("b", True, 5.0)]
    assert rank_ex_hosts(results, candidates=["a", "b"]) == ["b", "a"]


# --------------------------------------------------------------------- #
# 探活：TCP 通但不应答业务命令必须判为不可用
# --------------------------------------------------------------------- #


def test_probe_marks_silent_host_unhealthy():
    FakeNet.reset({"silent": "timeout", "good": "ok"})
    good = probe_ex_host("good")
    silent = probe_ex_host("silent")
    assert good.ok is True
    assert good.latency_ms is not None
    assert silent.ok is False
    assert "TransportError" in (silent.error or "")


def test_probe_marks_refused_host_unhealthy():
    FakeNet.reset({"dead": "refuse"})
    result = probe_ex_host("dead", timeout=0.05)
    assert result.ok is False
    assert "无法连接" in (result.error or "")


def test_probe_ex_hosts_is_concurrent_and_keeps_input_order():
    hosts = [f"h{i}" for i in range(8)]
    FakeNet.reset(dict.fromkeys(hosts, "ok"))
    results = probe_ex_hosts(hosts, timeout=0.05)
    assert [r.host for r in results] == hosts
    assert all(r.ok for r in results)


def test_probe_ex_hosts_empty_candidates():
    assert probe_ex_hosts([]) == []


def test_probe_stop_after_first_keeps_input_order_and_reports_healthy():
    """早退模式：一旦有主机应答就返回，但结果仍与输入一一对应。"""

    hosts = ["h0", "h1", "h2"]
    FakeNet.reset({"h0": "timeout", "h1": "ok", "h2": "timeout"})
    results = probe_ex_hosts(hosts, timeout=0.1, stop_after_first=True)
    assert [r.host for r in results] == hosts
    assert [r.host for r in results if r.ok] == ["h1"]


def test_probe_stop_after_first_survives_nothing_answering():
    hosts = ["a", "b"]
    FakeNet.reset(dict.fromkeys(hosts, "timeout"))
    results = probe_ex_hosts(hosts, timeout=0.1, stop_after_first=True)
    assert [r.host for r in results] == hosts
    assert not any(r.ok for r in results)


def test_probe_ex_hosts_treats_all_failing_hosts_as_unhealthy():
    FakeNet.reset({"a": "timeout", "b": "reset", "c": "refuse"})
    results = probe_ex_hosts(["a", "b", "c"], timeout=0.05)
    assert [r.ok for r in results] == [False, False, False]
    assert rank_ex_hosts(results, candidates=["a", "b", "c"]) == ["a", "b", "c"]


# --------------------------------------------------------------------- #
# 客户端：探活选主
# --------------------------------------------------------------------- #


def test_connect_probes_and_picks_the_only_working_host():
    """真实场景复刻：15 台静默、1 台可用，必须选中可用的那台。"""

    bad = [f"bad{i}" for i in range(15)]
    behavior = dict.fromkeys(bad, "timeout")
    behavior["good"] = "ok"
    FakeNet.reset(behavior)

    client = ex_client.ExTdxClient(hosts=bad + ["good"], timeout=0.1, probe_timeout=0.1)
    client.connect()
    assert client.host == "good"
    assert client.healthy_hosts == ["good"]
    client.close()


def test_connect_does_not_probe_single_host():
    """单主机不该有额外开销。"""

    FakeNet.reset({"only": "ok"})
    client = ex_client.ExTdxClient(hosts=["only"], timeout=0.1)
    client.connect()
    # 只有真正的业务连接，没有探活连接
    assert FakeNet.connects == ["only"]
    assert client.healthy_hosts == []
    client.close()


def test_connect_probes_only_once_per_session():
    FakeNet.reset({"a": "ok", "b": "ok"})
    client = ex_client.ExTdxClient(hosts=["a", "b"], timeout=0.1, probe_timeout=0.1)
    client.connect()
    first = len(FakeNet.connects)
    client.close()
    client.connect()
    client.close()
    # 第二次 connect 不再重新探活（探活次数远小于重复 connect 的次数）
    assert len(FakeNet.connects) - first <= 2


def test_explicit_host_is_preferred_over_probe_result():
    FakeNet.reset({"mine": "ok", "faster": "ok"})
    client = ex_client.ExTdxClient("mine", hosts=["faster", "mine"], timeout=0.1, probe_timeout=0.1)
    client.connect()
    assert client.host == "mine"
    client.close()


# --------------------------------------------------------------------- #
# 客户端：命令级失败 → 自动换台（本次改动的核心）
# --------------------------------------------------------------------- #


def test_command_failure_switches_host_instead_of_returning_to_it():
    """旧实现的 bug：失败后重连仍回到同一台。现在必须换到另一台并成功。"""

    FakeNet.reset({"silent": "timeout", "good": "ok"})
    client = ex_client.ExTdxClient(
        "silent", hosts=["silent", "good"], timeout=0.1, probe_on_connect=False
    )
    client.connect()
    assert client.host == "silent"

    rows = client.get_instrument_info(0, 1)

    assert len(rows) == 1
    assert client.host == "good"
    assert "silent" in client.unhealthy_hosts
    assert client.healthy_hosts == ["good"]
    client.close()


def test_failed_host_is_not_retried_within_the_same_call():
    FakeNet.reset({"bad": "timeout", "good": "ok"})
    client = ex_client.ExTdxClient(
        "bad", hosts=["bad", "good"], timeout=0.1, probe_on_connect=False
    )
    client.get_instrument_info(0, 1)
    assert FakeNet.executes.count("bad") == 1
    assert FakeNet.executes.count("good") == 1
    client.close()


def test_failure_reprobes_remaining_hosts_instead_of_blind_serial_retries():
    """失败后必须并发重探剩余主机；不能对静默主机逐个盲试（会拖成 15×timeout）。"""

    FakeNet.reset({"bad": "timeout", "silent": "timeout", "good": "ok"})
    client = ex_client.ExTdxClient(
        "bad",
        hosts=["bad", "silent", "good"],
        timeout=0.1,
        probe_timeout=0.1,
        probe_on_connect=False,
    )
    rows = client.get_instrument_info(0, 1)

    assert len(rows) == 1
    assert client.host == "good"
    # 静默主机只在探活阶段被碰过，没有被当成业务目标盲试
    assert "silent" not in FakeNet.executes
    assert "silent" in FakeNet.probes
    assert FakeNet.executes == ["bad", "good"]
    client.close()


def test_all_hosts_failing_raises_and_stays_bounded():
    hosts = ["a", "b", "c"]
    FakeNet.reset(dict.fromkeys(hosts, "timeout"))
    client = ex_client.ExTdxClient("a", hosts=hosts, timeout=0.1, probe_on_connect=False)
    with pytest.raises(TransportError):
        client.get_instrument_info(0, 1)
    # 有界：每个候选最多尝试一次，不能无限循环
    assert len(FakeNet.executes) == len(hosts)
    client.close()


def test_auto_failover_disabled_makes_a_single_attempt():
    FakeNet.reset({"bad": "timeout", "good": "ok"})
    client = ex_client.ExTdxClient(
        "bad", hosts=["bad", "good"], timeout=0.1, auto_failover=False
    )
    with pytest.raises(TransportError):
        client.get_instrument_info(0, 1)
    assert FakeNet.executes == ["bad"]
    client.close()


def test_success_heals_a_previously_failed_host():
    FakeNet.reset({"flaky": "timeout", "good": "ok"})
    client = ex_client.ExTdxClient(
        "flaky", hosts=["flaky", "good"], timeout=0.1, probe_on_connect=False
    )
    client.get_instrument_info(0, 1)
    assert "flaky" in client.unhealthy_hosts

    FakeNet.behavior["flaky"] = "ok"
    client.refresh_hosts(timeout=0.1)

    assert client.unhealthy_hosts == {}
    assert set(client.healthy_hosts) == {"flaky", "good"}
    client.close()


def test_tcp_refused_host_falls_through_to_next():
    FakeNet.reset({"dead": "refuse", "good": "ok"})
    client = ex_client.ExTdxClient(
        "dead", hosts=["dead", "good"], timeout=0.1, probe_on_connect=False
    )
    client.connect()
    assert client.host == "good"
    assert "dead" in client.unhealthy_hosts
    client.close()


def test_all_hosts_refused_raises_on_connect():
    FakeNet.reset({"a": "refuse", "b": "refuse"})
    client = ex_client.ExTdxClient("a", hosts=["a", "b"], timeout=0.1, probe_on_connect=False)
    with pytest.raises(TransportError):
        client.connect()
    client.close()


def test_empty_hosts_rejected():
    with pytest.raises(ValueError):
        ex_client.ExTdxClient(hosts=[])


# --------------------------------------------------------------------- #
# ping_all / from_best_host 语义修正
# --------------------------------------------------------------------- #


def test_ping_all_only_returns_hosts_that_answer_business_commands():
    FakeNet.reset({"silent": "timeout", "good": "ok"})
    ranked = ex_client.ExTdxClient.ping_all(["silent", "good"], timeout=0.1)
    assert [host for host, _ in ranked] == ["good"]


def test_from_best_host_picks_a_working_host_and_does_not_reprobe():
    FakeNet.reset({"silent": "timeout", "good": "ok"})
    client = ex_client.ExTdxClient.from_best_host(["silent", "good"], timeout=0.1, ping_timeout=0.1)
    assert client.host == "good"
    assert client.healthy_hosts == ["good"]

    probes_before = len(FakeNet.connects)
    client.connect()
    # 已经探过，connect 不再重复探活
    assert len(FakeNet.connects) - probes_before <= 1
    client.close()


def test_from_best_host_falls_back_when_nothing_answers():
    FakeNet.reset({"a": "timeout", "b": "timeout"})
    client = ex_client.ExTdxClient.from_best_host(["a", "b"], timeout=0.1, ping_timeout=0.1)
    assert client.host in {"a", "b"}
    client.close()


# --------------------------------------------------------------------- #
# 早退不得留下滞留线程（回归：曾用 shutdown(wait=False) 造成线程暴涨）
# --------------------------------------------------------------------- #


def test_probe_registry_aborts_registered_connections():
    from eltdx.ex.transport.sync import _ProbeRegistry

    closed: list[str] = []

    class Conn:
        def __init__(self, tag: str) -> None:
            self.tag = tag

        def close(self) -> None:
            closed.append(self.tag)

    registry = _ProbeRegistry()
    first, second = Conn("a"), Conn("b")
    registry.add(first)
    registry.add(second)
    registry.discard(first)
    registry.abort_all()

    assert closed == ["b"]
    # 幂等：再次中断不得重复关闭
    registry.abort_all()
    assert closed == ["b"]


def test_probe_ex_host_registers_and_releases_connection():
    from eltdx.ex.transport.sync import _ProbeRegistry

    seen: list[int] = []

    class RecordingRegistry(_ProbeRegistry):
        def add(self, connection):
            seen.append(1)
            super().add(connection)

        def discard(self, connection):
            seen.append(-1)
            super().discard(connection)

    FakeNet.reset({"h": "ok"})
    result = probe_ex_host("h", registry=RecordingRegistry())

    assert result.ok is True
    assert seen == [1, -1]


def test_early_exit_does_not_leave_probe_threads(monkeypatch):
    """早退必须主动中断在途探活，而不是把它们留到超时。

    用一个「只有被 close 才会解阻塞」的假连接模拟卡在 TCP 握手里的线程：
    修复前这些线程要等满超时才退出，短时间内反复创建客户端就会把线程数顶上去。
    """

    import threading
    import time

    release = threading.Event()

    class BlockingConnection(FakeConnection):
        def connect(self) -> None:
            FakeNet.connects.append(self.host)
            if self.host == "good":
                self._open = True
                return
            # 模拟阻塞中的握手：只由 close() 解开
            release.wait(timeout=5.0)
            raise TransportError(f"aborted: {self.host}")

        def close(self) -> None:
            self._open = False
            release.set()

    monkeypatch.setattr(ex_transport.sync, "ExTdxConnection", BlockingConnection)

    before = len([t for t in threading.enumerate() if "eltdx-ex-probe" in t.name])
    started = time.monotonic()
    results = probe_ex_hosts(["good", "b1", "b2", "b3"], timeout=0.1, stop_after_first=True)
    elapsed = time.monotonic() - started

    deadline = time.monotonic() + 2.0
    while time.monotonic() < deadline:
        if len([t for t in threading.enumerate() if "eltdx-ex-probe" in t.name]) <= before:
            break
        time.sleep(0.02)
    after = [t for t in threading.enumerate() if "eltdx-ex-probe" in t.name]

    assert [r.host for r in results if r.ok] == ["good"]
    assert elapsed < 3.0, f"早退被在途探活拖住: {elapsed:.2f}s"
    assert len(after) <= before, f"残留探活线程: {[t.name for t in after]}"
