from dataclasses import replace
from datetime import date, datetime
from threading import Barrier, Lock

import pytest

from eltdx import Client, HelperApi, TdxClient, __version__, to_json, to_jsonable
from eltdx import WorkdayService
from eltdx.api import ping
from eltdx import hosts as hosts_module
from eltdx import client as client_module
from eltdx.hosts import (
    DEFAULT_HOSTS,
    FALLBACK_HOSTS,
    HostProbeResult,
    load_server_config,
    load_server_hosts,
    load_money_flow_hosts,
    load_server_ranking,
    probe_hosts,
    rank_hosts_from_cache,
    server_ranking_path,
)
from eltdx.protocol.constants import TYPE_REFRESH_STREAM
from eltdx.protocol import COMMANDS, decode, encode, required_commands
from eltdx.transport import InMemoryTransport, PooledSocketTransport, SocketTransport
from eltdx.transport import pool as pool_module
from eltdx.transport.pool import validate_pool_size
from eltdx.models import QuoteLevel, QuoteRefreshPage, QuoteRefreshRecord, QuoteSnapshot


@pytest.fixture(autouse=True)
def local_probe_results(tmp_path, monkeypatch):
    monkeypatch.setenv("ELTDX_DATA_DIR", str(tmp_path))
    monkeypatch.setattr(
        hosts_module, "probe_host",
        lambda host, **kwargs: HostProbeResult(host=host, ok=True, latency_ms=1.0),
    )


def test_version_is_defined() -> None:
    assert __version__ == "3.2.0"


def test_packaged_server_hosts_load_from_json() -> None:
    config = load_server_config()

    assert config["schema_version"] == 1
    assert load_server_hosts() == list(DEFAULT_HOSTS)
    assert DEFAULT_HOSTS == FALLBACK_HOSTS


def test_money_flow_uses_dedicated_host_pool_lazily() -> None:
    client = TdxClient(heartbeat_interval=None)

    assert len(load_money_flow_hosts()) == 35
    assert isinstance(client.transport, PooledSocketTransport)
    assert client.transport.diagnostics.state.name == "STOPPED"
    assert client.money_flow._dedicated_transport is None


@pytest.mark.parametrize("factory", [TdxClient, TdxClient.from_hosts])
@pytest.mark.parametrize("custom_hosts", [False, True])
def test_constructor_probes_both_host_groups_once_without_connecting(
    factory, custom_hosts, monkeypatch
):
    dedicated = load_money_flow_hosts()
    ordinary = ["127.0.0.1:7709", dedicated[0]] if custom_hosts else load_server_hosts()
    candidates = list(dict.fromkeys(ordinary + dedicated))
    probes = []
    engines = []

    def probe(host, *, timeout):
        assert timeout == 0.3
        probes.append(host)
        return HostProbeResult(host=host, ok=True, latency_ms=len(candidates) - candidates.index(host))

    class FakeEngine:
        def __init__(self, hosts, **kwargs):
            self.hosts = tuple(hosts)
            self.connected = False
            engines.append(self)

        def connect(self):
            self.connected = True

        def execute(self, command, payload):
            self.connect()
            return payload

        def close(self):
            self.connected = False

    from types import SimpleNamespace

    monkeypatch.setattr(hosts_module, "probe_host", probe)
    monkeypatch.setattr(pool_module, "native_module", lambda: SimpleNamespace(NativeEngine=FakeEngine))
    monkeypatch.setattr(pool_module, "response_from_dto", lambda value: value)
    client = factory(hosts=ordinary if custom_hosts else None, probe_timeout=0.3, probe_workers=3)
    assert sorted(probes) == sorted(candidates)
    ranked = tuple(reversed(candidates))
    ordinary_rank = tuple(host for host in ranked if host in ordinary)
    dedicated_rank = tuple(host for host in ranked if host in dedicated)
    assert client.transport.hosts == ordinary_rank
    assert client._dedicated_hosts == dedicated_rank
    assert [record["host"] for record in load_server_ranking()["hosts"]] == list(ranked)
    assert engines == []
    assert client.transport._engine is None
    assert client.auctions._dedicated_transport is None
    assert client.money_flow._dedicated_transport is None

    def unexpected_probe(*args, **kwargs):
        pytest.fail("a prepared client must not probe again")

    monkeypatch.setattr(client_module, "sort_hosts_by_latency", unexpected_probe)
    monkeypatch.setattr(pool_module, "sort_hosts_by_latency", unexpected_probe)
    # A later disk ranking must not replace this client's in-memory snapshot.
    monkeypatch.setattr(pool_module, "rank_hosts_from_cache", lambda hosts: list(reversed(hosts)))
    try:
        client.connect()
        assert len(engines) == 1
        assert engines[0].hosts == ordinary_rank
        assert client.auctions._dedicated_transport is None
        assert client.money_flow._dedicated_transport is None
        client.auctions.series("sz000001")
        client.auctions.series("sz000001", "2026-08-14")
        client.money_flow.daily("sz000001")
        assert len(engines) == 3
        assert engines[1].hosts == engines[2].hosts == dedicated_rank
        assert all(engine.connected for engine in engines)
        client.close()
        assert not any(engine.connected for engine in engines)
        client.auctions.series("sz000001")
        client.money_flow.daily("sz000001")
        assert len(engines) == 5
        assert engines[3].hosts == engines[4].hosts == dedicated_rank
    finally:
        client.close()
    assert sorted(probes) == sorted(candidates)


@pytest.mark.parametrize("factory", [TdxClient, TdxClient.from_hosts])
def test_constructor_can_skip_both_probes(factory, monkeypatch):
    def unexpected_probe(*args, **kwargs):
        pytest.fail("probe_hosts=False must not probe either host group")

    monkeypatch.setattr(client_module, "sort_hosts_by_latency", unexpected_probe)
    monkeypatch.setattr(pool_module, "sort_hosts_by_latency", unexpected_probe)
    client = factory(probe_hosts=False)
    try:
        auction_pool = client.auctions._active_transport()
        money_flow_pool = client.money_flow._active_transport()
        for pool in (client.transport, auction_pool, money_flow_pool):
            assert pool._hosts_probed
            assert pool._engine is None
        assert set(auction_pool.hosts) == set(load_money_flow_hosts())
        assert auction_pool.hosts == money_flow_pool.hosts
    finally:
        client.close()


def test_each_new_client_refreshes_rankings_and_retains_unreachable_hosts(monkeypatch):
    candidates = list(dict.fromkeys(load_server_hosts() + load_money_flow_hosts()))
    probes = []
    reachable = True

    def probe(host, **kwargs):
        probes.append(host)
        if not reachable:
            return HostProbeResult(host=host, ok=False, error="TimeoutError")
        return HostProbeResult(host=host, ok=True, latency_ms=len(candidates) - candidates.index(host))

    monkeypatch.setattr(hosts_module, "probe_host", probe)
    first = TdxClient()
    reachable = False
    second = TdxClient()
    assert len(probes) == 2 * len(candidates)
    assert first.transport.hosts == second.transport.hosts
    assert first._dedicated_hosts == second._dedicated_hosts
    assert set(second.transport.hosts) == set(load_server_hosts())
    assert set(second._dedicated_hosts) == set(load_money_flow_hosts())
    assert second.transport._engine is None
    first.close()
    second.close()


def test_custom_transport_skips_host_preparation(monkeypatch):
    def unexpected_probe(*args, **kwargs):
        pytest.fail("custom transports must not probe packaged hosts")

    monkeypatch.setattr(client_module, "sort_hosts_by_latency", unexpected_probe)
    transport = InMemoryTransport()
    client = TdxClient(transport=transport)
    client.auctions.series("sz000001")
    client.money_flow.daily("sz000001")
    assert client._dedicated_hosts == ()
    assert client.auctions._active_transport() is transport
    assert client.money_flow._active_transport() is transport
    client.close()


def test_bare_pool_keeps_lazy_probing():
    pool = PooledSocketTransport()
    assert not pool._hosts_probed
    assert pool._engine is None
    pool.close()


@pytest.mark.parametrize("factory", [TdxClient, TdxClient.from_hosts])
@pytest.mark.parametrize("trading_date", [None, "2026-08-14", date(2026, 8, 14)])
def test_auctions_use_dedicated_hosts_for_all_dates(factory, trading_date, monkeypatch):
    calls = []

    def execute(transport, command, payload=None):
        calls.append((transport, command, payload))
        return payload

    monkeypatch.setattr(PooledSocketTransport, "execute", execute)
    client = factory(heartbeat_interval=None)
    ordinary = client.transport
    assert set(ordinary.hosts) == set(load_server_hosts())
    assert len(ordinary.hosts) == 43
    assert client.auctions._dedicated_transport is None

    try:
        result = client.auctions.series("sz000001", trading_date, include_raw=True)
        dedicated = client.auctions._dedicated_transport
        assert isinstance(dedicated, PooledSocketTransport)
        assert dedicated is not ordinary
        assert set(dedicated.hosts) == set(load_money_flow_hosts())
        assert len(dedicated.hosts) == 35
        assert result == {
            "code": "sz000001", "trading_date": trading_date, "include_raw": True
        }
        assert calls[-1] == (dedicated, 0x056A, result)
        client.auctions.series("sz000001")
        assert calls[-1][0] is dedicated
        assert calls[-1][2]["include_raw"] is False
        assert client.money_flow._dedicated_transport is None

        client.quotes.get_snapshots(["sz000001"])
        assert calls[-1][0] is ordinary
        for name in ("session", "codes", "quotes", "resources", "bars",
                     "minutes", "trades", "corporate", "limits"):
            assert getattr(client, name)._transport is ordinary
        assert set(ordinary.hosts) == set(load_server_hosts())
        assert ordinary.diagnostics.state.name == "STOPPED"
    finally:
        client.close()
    assert client.auctions._dedicated_transport is None


def test_auction_pool_inherits_client_configuration_and_closes(monkeypatch):
    closed = []
    monkeypatch.setattr(PooledSocketTransport, "execute", lambda *args: None)
    monkeypatch.setattr(PooledSocketTransport, "close", lambda pool: closed.append(pool))
    client = TdxClient(
        timeout=3, server_count=3, connections_per_server=2, runtime_workers=2,
        max_connections_per_host=2, connect_concurrency=3,
        connect_concurrency_per_host=1, probe_hosts=False, probe_timeout=0.4,
        probe_workers=3, heartbeat_interval=15, max_pending_requests=12,
        push_queue_size=16, push_queue_bytes=4096,
        global_raw_bytes=32 * 1024 * 1024, global_decoded_bytes=64 * 1024 * 1024,
    )
    client.auctions.series("sz000001")
    dedicated = client.auctions._dedicated_transport
    for name in (
        "timeout", "pool_size", "server_count", "connections_per_server",
        "runtime_workers", "max_connections_per_host", "connect_concurrency",
        "connect_concurrency_per_host", "probe_hosts", "probe_timeout",
        "probe_workers", "heartbeat_interval", "max_pending_requests",
        "push_queue_size", "push_queue_bytes", "global_raw_bytes", "global_decoded_bytes",
    ):
        assert getattr(dedicated, f"_{name}") == getattr(client, name)

    client.money_flow.daily("sz000001")
    money_flow = client.money_flow._dedicated_transport
    assert money_flow is not dedicated
    assert set(money_flow.hosts) == set(dedicated.hosts)
    client.close()
    assert closed == [client.transport, money_flow, dedicated]
    assert client.auctions._dedicated_transport is None
    assert client.money_flow._dedicated_transport is None
    client.close()
    assert closed.count(dedicated) == 1
    client.auctions.series("sz000001", "2026-08-14")
    assert client.auctions._dedicated_transport is not dedicated
    client.close()


def test_auction_pool_is_created_once_for_concurrent_first_calls(monkeypatch):
    from concurrent.futures import ThreadPoolExecutor
    from time import sleep

    created = []
    start = Barrier(8)

    def build(client):
        transport = InMemoryTransport()
        created.append(transport)
        sleep(0.02)
        return transport

    monkeypatch.setattr(TdxClient, "_build_dedicated_market_transport", build)
    client = TdxClient(heartbeat_interval=None)

    def query(index):
        start.wait(timeout=5)
        return client.auctions.series("sz000001", None if index % 2 else "2026-08-14")

    try:
        with ThreadPoolExecutor(max_workers=8) as executor:
            results = list(executor.map(query, range(8)))
        assert len(created) == 1
        assert len(created[0].calls) == 8
        assert all(result["command"] == "0x056a" for result in results)
    finally:
        client.close()


def test_money_flow_pool_is_created_once_for_concurrent_first_calls(monkeypatch):
    from concurrent.futures import ThreadPoolExecutor
    from time import sleep

    created = []

    def build(client):
        transport = InMemoryTransport()
        created.append(transport)
        sleep(0.02)
        return transport

    monkeypatch.setattr(TdxClient, "_build_money_flow_transport", build)
    client = TdxClient(heartbeat_interval=None)
    try:
        with ThreadPoolExecutor(max_workers=8) as executor:
            transports = list(executor.map(lambda _: client.money_flow._active_transport(), range(8)))
        assert len(created) == 1
        assert all(transport is created[0] for transport in transports)
    finally:
        client.close()


def test_auction_pool_factory_failure_can_be_retried(monkeypatch):
    transport = InMemoryTransport()
    attempts = []

    def build(client):
        attempts.append(True)
        if len(attempts) == 1:
            raise ValueError("pool creation failed")
        return transport

    monkeypatch.setattr(TdxClient, "_build_dedicated_market_transport", build)
    client = TdxClient(heartbeat_interval=None)
    try:
        with pytest.raises(ValueError, match="pool creation failed"):
            client.auctions.series("sz000001")
        assert client.auctions._dedicated_transport is None
        client.auctions.series("sz000001")
        assert client.auctions._dedicated_transport is transport
        assert len(attempts) == 2
    finally:
        client.close()


def test_in_memory_auctions_keep_the_supplied_transport():
    client = TdxClient.in_memory()
    client.auctions.series("sz000001", "2026-08-14", include_raw=True)
    assert client.auctions._dedicated_transport is None
    assert client.transport.calls == [
        (0x056A, {"code": "sz000001", "trading_date": "2026-08-14", "include_raw": True})
    ]
    client.close()


def test_probe_hosts_persists_ranking_for_next_process(tmp_path, monkeypatch) -> None:
    monkeypatch.setenv("ELTDX_DATA_DIR", str(tmp_path))
    latencies = {
        "127.0.0.1:7709": 30.0,
        "127.0.0.2:7709": 10.0,
        "127.0.0.3:7709": None,
    }

    def fake_probe(host: str, *, timeout: float) -> HostProbeResult:
        latency = latencies[host]
        if latency is None:
            return HostProbeResult(host=host, ok=False, error="TimeoutError")
        return HostProbeResult(host=host, ok=True, latency_ms=latency)

    monkeypatch.setattr(hosts_module, "probe_host", fake_probe)
    candidates = list(latencies)
    probe_hosts(candidates, timeout=0.1, max_workers=3)

    assert server_ranking_path() == tmp_path / "tdx_server_ranking.json"
    assert rank_hosts_from_cache(candidates) == [
        "127.0.0.2:7709",
        "127.0.0.1:7709",
        "127.0.0.3:7709",
    ]
    ranking = load_server_ranking()
    assert ranking["schema_version"] == 1
    assert [record["rank"] for record in ranking["hosts"]] == [1, 2, 3]
    assert ranking["hosts"][2]["consecutive_failures"] == 1

    latencies.update({host: None for host in candidates})
    probe_hosts(list(reversed(candidates)), timeout=0.1, max_workers=3)

    assert rank_hosts_from_cache(candidates) == [
        "127.0.0.2:7709",
        "127.0.0.1:7709",
        "127.0.0.3:7709",
    ]
    ranking = load_server_ranking()
    assert ranking["hosts"][0]["last_success_latency_ms"] == 10.0
    assert all(record["ok"] is False for record in ranking["hosts"])


def test_client_ping_returns_pong() -> None:
    assert Client().ping() == "pong"
    assert isinstance(TdxClient().transport, PooledSocketTransport)
    assert TdxClient.in_memory().ping() == "pong"
    assert isinstance(TdxClient.in_memory().helpers, HelperApi)


def test_public_pool_defaults_are_consistent() -> None:
    client = TdxClient(heartbeat_interval=None)
    direct = PooledSocketTransport(["127.0.0.1:9"], heartbeat_interval=None)

    assert client.pool_size == 8
    assert client.server_count == 2
    assert client.connections_per_server == 4
    assert client.transport.pool_size == 8
    assert client.transport.server_count == 2
    assert direct.pool_size == 4
    assert direct.server_count == 1
    assert direct.connections_per_server == 4
    assert client.transport.push_queue_bytes == 64 * 1024 * 1024
    assert client.probe_hosts is True


def test_client_keeps_the_v2_positional_configuration_prefix() -> None:
    transport = InMemoryTransport()
    client = TdxClient(
        transport,
        None,
        None,
        5.0,
        3,
        False,
        0.5,
        4,
        None,
        9,
        10,
        11,
    )

    assert client.transport is transport
    assert client.timeout == 5.0
    assert client.pool_size == 3
    assert client.probe_hosts is False
    assert client.probe_timeout == 0.5
    assert client.probe_workers == 4
    assert client.heartbeat_interval is None
    assert client.max_pending_requests == 9
    assert client.push_queue_size == 10
    assert client.push_queue_bytes == 11


@pytest.mark.parametrize("value", [0, -1, 1.5, "2", True])
def test_client_rejects_invalid_pool_size(value) -> None:
    with pytest.raises(ValueError, match="pool_size must be a positive integer"):
        TdxClient(transport=InMemoryTransport(), pool_size=value)


@pytest.mark.parametrize("value", [0, -1, 1.5, "2", True, None])
def test_legacy_pool_size_validator_keeps_rejecting_invalid_values(value) -> None:
    with pytest.raises(ValueError, match="pool_size must be a positive integer"):
        validate_pool_size(value)


def test_legacy_pool_size_is_distributed_across_available_servers() -> None:
    single = PooledSocketTransport(["127.0.0.1:9"], pool_size=1, probe_hosts=False)
    uneven = PooledSocketTransport(
        ["127.0.0.1:9", "127.0.0.2:9", "127.0.0.3:9"],
        pool_size=7,
        server_count=3,
        probe_hosts=False,
    )

    assert single.pool_size == 1
    assert single.server_count == 1
    assert single.diagnostics.max_connections_per_host == 1
    assert uneven.pool_size == 7
    assert uneven.server_count == 3
    assert uneven.connections_per_server == 3
    assert uneven.diagnostics.max_connections_per_host == 3


def test_explicit_server_layout_and_runtime_limits_are_visible_in_diagnostics() -> None:
    transport = PooledSocketTransport(
        ["127.0.0.1:9", "127.0.0.2:9", "127.0.0.3:9"],
        server_count=3,
        connections_per_server=5,
        runtime_workers=6,
        connect_concurrency=7,
        connect_concurrency_per_host=2,
        global_raw_bytes=16 * 1024 * 1024,
        global_decoded_bytes=128 * 1024 * 1024,
        probe_hosts=False,
    )

    diagnostics = transport.diagnostics
    assert transport.pool_size == 15
    assert diagnostics.pool_size == 15
    assert diagnostics.server_count == 3
    assert diagnostics.runtime_workers == 6
    assert diagnostics.max_connections_per_host == 5
    assert diagnostics.connect_concurrency == 7
    assert diagnostics.connect_concurrency_per_host == 2
    assert diagnostics.raw_max_bytes == 16 * 1024 * 1024
    assert diagnostics.decoded_max_bytes == 128 * 1024 * 1024


def test_explicit_pool_and_server_layout_must_agree() -> None:
    with pytest.raises(
        ValueError,
        match=r"pool_size must equal server_count \* connections_per_server",
    ):
        PooledSocketTransport(
            ["127.0.0.1:9", "127.0.0.2:9"],
            pool_size=7,
            server_count=2,
            connections_per_server=4,
            probe_hosts=False,
        )


def test_api_ping_uses_default_client() -> None:
    assert ping() == "pong"


def test_protocol_round_trip() -> None:
    assert decode(encode("hello")) == "hello"


def test_command_registry_contains_7709_documents() -> None:
    assert len(COMMANDS) == 21
    assert COMMANDS["snapshots"].hex == "0x054c"
    assert COMMANDS["legacy_quotes"].hex == "0x053e"
    assert COMMANDS["file_content"].hex == "0x06b9"
    assert COMMANDS["file_content"].method == "read"
    assert COMMANDS["security_list"].document == "0x044d-代码表分页接口.md"
    assert {item.name for item in required_commands()} >= {
        "handshake",
        "heartbeat",
        "snapshots",
        "klines",
    }


def test_business_api_uses_command_numbers() -> None:
    client = TdxClient.in_memory()
    assert client.quotes.get_snapshots(["sz000001"])["command"] == "0x054c"
    assert client.quotes.legacy(["sz000001"])["command"] == "0x053e"
    assert client.quotes.get_depth(["sz000001"])["command"] == "0x0547"
    assert (
        client.quotes.list_by_category("沪深A股", sort_by="涨幅")["command"] == "0x054b"
    )
    assert client.quotes.poll_push() is None
    assert client.quotes.drain_pushes() == []
    assert client.bars.get("sz000001", period="day")["payload"]["period"] == "day"
    assert client.minutes.today("sz000001")["command"] == "0x0537"
    assert client.resources.read("zhb.zip", offset=10, size=20)["command"] == "0x06b9"


def test_bars_auto_detects_index_record_layout() -> None:
    class FakeTransport:
        pool_size = 1

        def connect(self) -> None:
            pass

        def close(self) -> None:
            pass

        def execute(self, command: int, payload=None):
            assert command == 0x052D
            return payload

    client = TdxClient(transport=FakeTransport())
    assert client.bars.get("sh000001")["kind"] == "index"
    assert client.bars.get("sz399001")["kind"] == "index"
    assert client.bars.get("bj899001")["kind"] == "index"
    assert client.bars.get("sz000001")["kind"] == "stock"
    assert client.bars.get("sh000001", kind="stock")["kind"] == "stock"


def test_today_entrypoints_replace_current_names() -> None:
    client = TdxClient.in_memory()

    assert hasattr(client.minutes, "today")
    assert not hasattr(client.minutes, "current")
    assert hasattr(client.trades, "today")
    assert hasattr(client.trades, "all_today")
    assert not hasattr(client.trades, "current")
    assert not hasattr(client.trades, "all_current")


def test_new_binary_client_entrypoints_keep_exact_payloads() -> None:
    client = TdxClient.in_memory()

    legacy = client.quotes.legacy(["sz000001", "sh600000"])
    resource = client.resources.read("zhb.zip", offset=30000, size=12000)

    assert legacy["command"] == "0x053e"
    assert legacy["payload"] == {"codes": ["sz000001", "sh600000"]}
    assert resource["command"] == "0x06b9"
    assert resource["payload"] == {"path": "zhb.zip", "offset": 30000, "size": 12000}


def test_trade_apis_accept_code_sequences_and_return_code_maps() -> None:
    from eltdx.models import TradePage

    calls = []

    class FakeTransport:
        pool_size = 2

        def connect(self) -> None:
            pass

        def close(self) -> None:
            pass

        def request(self, command: str) -> str:
            return "pong"

        def execute(self, command: int, payload=None):
            calls.append((command, payload))
            code = payload["code"]
            return TradePage(
                exchange=code[:2],
                market_id=0,
                code=code[2:],
                start=payload.get("start", 0),
                request_count=payload.get("count", payload.get("page_size", 1800)),
                ticks=(),
            )

    client = TdxClient(transport=FakeTransport())
    codes = ["600487", "sh600183", "002384"]

    today = client.trades.today(codes, count=10, batch_size=2)
    history = client.trades.history(codes, "2026-08-28", count=10, batch_size=2)
    all_today = client.trades.all_today(codes, batch_size=2)
    all_history = client.trades.all_history(codes, "2026-08-28", batch_size=2)
    opening_today = client.trades.opening_match_today(codes, batch_size=2)
    opening_history = client.trades.opening_match_history(
        codes, "2026-08-28", batch_size=2
    )

    expected = {"sh600487", "sh600183", "sz002384"}
    assert set(today) == expected
    assert set(history) == expected
    assert set(all_today) == expected
    assert set(all_history) == expected
    assert opening_today == {code: None for code in expected}
    assert opening_history == {code: None for code in expected}
    assert len(calls) == 18


def test_trade_batch_size_must_be_positive() -> None:
    with pytest.raises(ValueError, match="batch_size"):
        TdxClient.in_memory().trades.today(["600487"], batch_size=0)


def test_constructor_accepts_single_host() -> None:
    client = TdxClient(host="127.0.0.1:7709", timeout=0.1, heartbeat_interval=None)

    assert isinstance(client.transport, PooledSocketTransport)
    assert client.transport.hosts == ("127.0.0.1:7709",)
    assert client.transport.heartbeat_interval is None


def test_from_hosts_preserves_heartbeat_setting() -> None:
    client = TdxClient.from_hosts(
        ["127.0.0.1:7709"],
        timeout=0.1,
        heartbeat_interval=None,
        max_pending_requests=17,
        push_queue_size=23,
        push_queue_bytes=4096,
    )

    assert client.timeout == 0.1
    assert client.f10.timeout == 0.1
    assert client.heartbeat_interval is None
    assert client.transport.heartbeat_interval is None
    assert client.transport.max_pending_requests == 17
    assert client.transport.push_queue_size == 23
    assert client.transport.push_queue_bytes == 4096


def test_full_quotes_batches_requests() -> None:
    class FakeTransport:
        def __init__(self) -> None:
            self.payloads = []

        def connect(self) -> None:
            pass

        def close(self) -> None:
            pass

        def request(self, command: str) -> str:
            return "pong"

        def execute(self, command: int, payload=None):
            assert command == 0x054C
            self.payloads.append(payload)
            return [f"quote:{code}" for code in payload["codes"]]

    transport = FakeTransport()
    client = TdxClient(transport=transport)
    codes = [f"sz{index:06d}" for index in range(81)]

    assert client.helpers.full_quotes(codes) == [f"quote:{code}" for code in codes]
    assert [len(payload["codes"]) for payload in transport.payloads] == [80, 1]


def test_full_quotes_merges_refresh_five_levels() -> None:
    top_bid = QuoteLevel(price=10.92, volume=1232, price_delta_raw=-1)
    top_ask = QuoteLevel(price=10.93, volume=12481, price_delta_raw=0)
    full_bids = tuple(
        QuoteLevel(
            price=10.92 - index * 0.01, volume=100 + index, price_delta_raw=-(index + 1)
        )
        for index in range(5)
    )
    full_asks = tuple(
        QuoteLevel(
            price=10.93 + index * 0.01, volume=200 + index, price_delta_raw=index
        )
        for index in range(5)
    )

    snapshot = QuoteSnapshot(
        exchange="sz",
        market_id=0,
        code="000001",
        active1=1,
        last_price=10.93,
        pre_close_price=10.66,
        open_price=10.65,
        high_price=10.93,
        low_price=10.62,
        time_raw=0,
        unknown_after_time_raw=0,
        total_hand=1,
        current_hand=1,
        amount=1.0,
        amount_raw=0,
        inside_dish=0,
        outer_disc=0,
        unknown_after_outer_raw=0,
        open_amount_raw=79864,
        open_amount_yuan=7_986_400.0,
        buy_levels=(top_bid,),
        sell_levels=(top_ask,),
        tail_raw=b"",
    )
    refresh_record = QuoteRefreshRecord(
        exchange="sz",
        market_id=0,
        code="000001",
        active=1,
        update_time_raw=153306,
        last_price=10.93,
        last_close_price=10.66,
        open_price=10.65,
        high_price=10.93,
        low_price=10.62,
        status_or_reserved_raw=0,
        total_hand=1,
        current_hand=1,
        amount=1.0,
        amount_raw=0,
        inside_dish=0,
        outer_disc=0,
        unknown_after_outer_raw=0,
        open_amount_raw=798643,
        open_amount_yuan=7_986_430.0,
        buy_levels=full_bids,
        sell_levels=full_asks,
        tail_raw=b"",
    )

    class FakeTransport:
        def __init__(self) -> None:
            self.calls = []

        def connect(self) -> None:
            pass

        def close(self) -> None:
            pass

        def request(self, command: str) -> str:
            return "pong"

        def execute(self, command: int, payload=None):
            self.calls.append((command, payload))
            if command == 0x054C:
                return [snapshot]
            if command == TYPE_REFRESH_STREAM:
                return QuoteRefreshPage(
                    ("sz000001",), (refresh_record,), decoded_payload=b""
                )
            raise AssertionError(command)

    transport = FakeTransport()
    quote = TdxClient(transport=transport).helpers.full_quotes("sz000001")[0]

    assert [command for command, _ in transport.calls] == [0x054C, TYPE_REFRESH_STREAM]
    assert quote.buy_levels == full_bids
    assert quote.sell_levels == full_asks
    assert quote.open_amount_raw == 798643


def test_quote_depth_uses_refresh_interface() -> None:
    class FakeTransport:
        def __init__(self) -> None:
            self.calls = []

        def connect(self) -> None:
            pass

        def close(self) -> None:
            pass

        def request(self, command: str) -> str:
            return "pong"

        def execute(self, command: int, payload=None):
            self.calls.append((command, payload))
            return "depth"

    transport = FakeTransport()
    assert TdxClient(transport=transport).quotes.get_depth(["sz000001"]) == "depth"
    assert transport.calls == [
        (TYPE_REFRESH_STREAM, {"codes": ["sz000001"], "cursors": {}})
    ]


def test_code_filters_use_security_categories() -> None:
    from eltdx.models import SecurityCode

    def item(exchange: str, code: str, category: str) -> SecurityCode:
        return SecurityCode(
            exchange=exchange,
            market_id={"sz": 0, "sh": 1, "bj": 2}[exchange],
            code=code,
            name=code,
            multiple=1,
            decimal=2,
            previous_close_price=0.0,
            volume_ratio_base=0.0,
            unknown0_raw=b"",
            previous_close_raw=b"",
            unknown3_raw=b"",
            category=category,
            category_reason="test",
            board="none",
            board_reason="test",
        )

    pages = {
        "sh": [item("sh", "600000", "a_share"), item("sh", "900901", "b_share")],
        "sz": [item("sz", "159915", "etf"), item("sz", "399001", "index")],
        "bj": [item("bj", "920001", "a_share")],
    }

    class FakeTransport:
        def connect(self) -> None:
            pass

        def close(self) -> None:
            pass

        def request(self, command: str) -> str:
            return "pong"

        def execute(self, command: int, payload=None):
            assert command == 0x044D
            return pages[payload["market"]] if payload["start"] == 0 else []

    transport = FakeTransport()
    client = TdxClient(transport=transport)

    assert client.codes.all_stocks() == ["sh600000", "sh900901", "bj920001"]
    assert client.codes.all_a_shares() == ["sh600000", "bj920001"]
    assert client.codes.all_etfs() == ["sz159915"]
    assert client.codes.all_indices() == ["sz399001"]


def test_code_api_requests_current_data() -> None:
    from eltdx.models import SecurityCode

    def item(exchange: str, code: str) -> SecurityCode:
        return SecurityCode(
            exchange=exchange,
            market_id={"sz": 0, "sh": 1, "bj": 2}[exchange],
            code=code,
            name=code,
            multiple=1,
            decimal=2,
            previous_close_price=0.0,
            volume_ratio_base=0.0,
            unknown0_raw=b"",
            previous_close_raw=b"",
            unknown3_raw=b"",
            category="a_share",
            category_reason="test",
            board="none",
            board_reason="test",
        )

    class FakeTransport:
        def __init__(self) -> None:
            self.calls = 0

        def connect(self) -> None:
            pass

        def close(self) -> None:
            pass

        def request(self, command: str) -> str:
            return "pong"

        def execute(self, command: int, payload=None):
            if command == 0x044E:
                self.calls += 1
                return 10 + self.calls
            if command == 0x044D:
                self.calls += 1
                return (
                    [item(payload["market"], f"00000{self.calls}")]
                    if payload["start"] == 0
                    else []
                )
            raise AssertionError(f"unexpected command: {command:#x}")

    transport = FakeTransport()
    client = TdxClient(transport=transport)

    assert client.codes.count("sz") == 11
    assert client.codes.count("sz") == 12
    assert client.codes.all("sz") != client.codes.all("sz")


def test_bar_api_forwards_period_and_adjust_payload() -> None:
    class FakeTransport:
        def __init__(self) -> None:
            self.payload = None

        def connect(self) -> None:
            pass

        def close(self) -> None:
            pass

        def request(self, command: str) -> str:
            return "pong"

        def execute(self, command: int, payload=None):
            assert command == 0x052D
            self.payload = payload
            return payload

    transport = FakeTransport()
    client = TdxClient(transport=transport)

    assert (
        client.bars.get("sz000001", period="day", count=5, adjust="qfq")["code"]
        == "sz000001"
    )
    assert transport.payload["period"] == "day"
    assert transport.payload["adjust"] == "qfq"
    assert client.bars.get("sz000001", period="day", count=5)["period"] == "day"
    assert (
        client.bars.get(
            "sz000001",
            period="day",
            adjust="fixed_qfq",
            anchor_date="2024-06-03",
            count=5,
        )["code"]
        == "sz000001"
    )
    assert transport.payload["adjust"] == "fixed_qfq"
    assert transport.payload["anchor_date"] == "2024-06-03"


def test_bar_api_fetches_code_lists_concurrently() -> None:
    class FakeTransport:
        pool_size = 2

        def connect(self) -> None:
            pass

        def close(self) -> None:
            pass

        def request(self, command: str) -> str:
            return "pong"

        def execute(self, command: int, payload=None):
            assert command == 0x052D
            return {"code": payload["code"], "period": payload["period"]}

    client = TdxClient(transport=FakeTransport())
    result = client.bars.get(
        ["000001", "sh600000", "000001"],
        period="week",
        count=20,
        batch_size=1,
    )

    assert list(result) == ["sz000001", "sh600000"]
    assert result["sz000001"] == {"code": "sz000001", "period": "week"}
    assert result["sh600000"] == {"code": "sh600000", "period": "week"}


@pytest.mark.parametrize("batch_size", (0, -1, True, "2"))
def test_bar_api_batch_size_is_validated(batch_size) -> None:
    with pytest.raises(ValueError, match="batch_size"):
        TdxClient.in_memory().bars.get(["sz000001"], batch_size=batch_size)


def test_capital_changes_forwards_include_raw() -> None:
    class FakeTransport:
        def connect(self) -> None:
            pass

        def close(self) -> None:
            pass

        def request(self, command: str) -> str:
            return "pong"

        def execute(self, command: int, payload=None):
            assert command == 0x000F
            return payload

    result = TdxClient(transport=FakeTransport()).corporate.capital_changes(
        "sz000001", include_raw=True
    )

    assert result == {"code": "sz000001", "include_raw": True}


def test_money_flow_accepts_multiple_codes_and_uses_pool_concurrency() -> None:
    from eltdx.models import MoneyFlowBatch, MoneyFlowBlock

    class FakeTransport:
        pool_size = 2

        def __init__(self) -> None:
            self.barrier = Barrier(2)
            self.lock = Lock()
            self.codes = []

        def connect(self) -> None:
            pass

        def close(self) -> None:
            pass

        def request(self, command: str) -> str:
            return "pong"

        def execute(self, command: int, payload=None):
            assert command == 0x0FFC
            code = payload["code"]
            with self.lock:
                self.codes.append(code)
            self.barrier.wait(timeout=2)
            return MoneyFlowBlock(code[:2], 0, code[2:], ())

    transport = FakeTransport()
    result = TdxClient(transport=transport).money_flow.daily(
        ["sz000001", "sh600000"], batch_size=2
    )

    assert isinstance(result, MoneyFlowBatch)
    assert result.count == 0
    assert [block.full_code for block in result.blocks] == [
        "sz000001",
        "sh600000",
    ]
    assert sorted(transport.codes) == ["sh600000", "sz000001"]


@pytest.mark.parametrize("batch_size", (0, -1, True, "75"))
def test_money_flow_batch_size_is_validated(batch_size) -> None:
    with pytest.raises(ValueError, match="batch_size"):
        TdxClient.in_memory().money_flow.daily(["sz000001"], batch_size=batch_size)


def test_money_flow_block_provides_returned_period_totals() -> None:
    from eltdx.models import MoneyFlowBlock, MoneyFlowDaily

    record = MoneyFlowDaily(
        20260831,
        date(2026, 8, 31),
        100.0,
        tuple(),
        20.0,
        10.0,
        tuple(),
        "",
        30.0,
        15.0,
    )
    block = MoneyFlowBlock("sz", 0, "000001", (record,))

    assert block.main_net_total == 20.0
    assert block.main_ratio_total == 20.0
    assert block.main_buy_net_total == 30.0
    assert block.main_buy_ratio_total == 30.0


def test_capital_changes_batches_200_codes_and_uses_pool_concurrency() -> None:
    from eltdx.models import CapitalChangeBatch, CapitalChangeBlock

    class FakeTransport:
        pool_size = 3

        def __init__(self) -> None:
            self.barrier = Barrier(3)
            self.lock = Lock()
            self.batch_sizes = []

        def connect(self) -> None:
            pass

        def close(self) -> None:
            pass

        def request(self, command: str) -> str:
            return "pong"

        def execute(self, command: int, payload=None):
            assert command == 0x000F
            codes = payload["codes"]
            with self.lock:
                self.batch_sizes.append(len(codes))
            self.barrier.wait(timeout=2)
            blocks = tuple(
                CapitalChangeBlock(
                    code[:2],
                    {"sz": 0, "sh": 1, "bj": 2}[code[:2]],
                    code[2:],
                    len(codes),
                    (),
                )
                for code in codes
            )
            if len(blocks) == 1:
                return blocks[0]
            return CapitalChangeBatch(blocks)

    codes = [f"sz{index:06d}" for index in range(401)]
    transport = FakeTransport()
    result = TdxClient(transport=transport).corporate.capital_changes(
        codes, batch_size=200
    )

    assert isinstance(result, CapitalChangeBatch)
    assert sorted(transport.batch_sizes) == [1, 200, 200]
    assert result.count == 401
    assert [block.full_code for block in result.blocks] == codes


def test_capital_changes_defaults_to_75_and_accepts_custom_batch_size() -> None:
    from eltdx.models import CapitalChangeBatch, CapitalChangeBlock

    class FakeTransport:
        pool_size = 1

        def __init__(self) -> None:
            self.batch_sizes = []

        def connect(self) -> None:
            pass

        def close(self) -> None:
            pass

        def request(self, command: str) -> str:
            return "pong"

        def execute(self, command: int, payload=None):
            codes = payload["codes"]
            self.batch_sizes.append(len(codes))
            return CapitalChangeBatch(
                tuple(
                    CapitalChangeBlock(code[:2], 0, code[2:], 0, ()) for code in codes
                )
            )

    default_transport = FakeTransport()
    TdxClient(transport=default_transport).corporate.capital_changes(
        [f"sz{index:06d}" for index in range(150)]
    )
    assert default_transport.batch_sizes == [75, 75]

    custom_transport = FakeTransport()
    TdxClient(transport=custom_transport).corporate.capital_changes(
        [f"sz{index:06d}" for index in range(150)], batch_size=50
    )
    assert custom_transport.batch_sizes == [50, 50, 50]


@pytest.mark.parametrize("batch_size", (0, 201, True, "75"))
def test_capital_change_batch_size_is_validated(batch_size) -> None:
    with pytest.raises(ValueError, match="batch_size"):
        TdxClient.in_memory().corporate.capital_changes(
            ["sz000001"], batch_size=batch_size
        )


def test_adjustment_factors_batch_reuses_capital_change_batches() -> None:
    from eltdx.models import (
        AdjustmentFactorBatch,
        CapitalChangeBatch,
        CapitalChangeBlock,
    )

    class FakeTransport:
        pool_size = 2

        def __init__(self) -> None:
            self.calls = 0

        def connect(self) -> None:
            pass

        def close(self) -> None:
            pass

        def request(self, command: str) -> str:
            return "pong"

        def execute(self, command: int, payload=None):
            assert command == 0x000F
            self.calls += 1
            blocks = tuple(
                CapitalChangeBlock(code[:2], 0, code[2:], len(payload["codes"]), ())
                for code in payload["codes"]
            )
            return CapitalChangeBatch(blocks)

    transport = FakeTransport()
    result = TdxClient(transport=transport).corporate.adjustment_factors(
        ["sz000001", "sz000002"]
    )

    assert isinstance(result, AdjustmentFactorBatch)
    assert transport.calls == 1
    assert [item.full_code for item in result.responses] == ["sz000001", "sz000002"]


def test_capital_changes_continues_after_server_response_prefix() -> None:
    from eltdx.models import CapitalChangeBatch, CapitalChangeBlock

    class FakeTransport:
        pool_size = 1

        def __init__(self) -> None:
            self.requests = []

        def connect(self) -> None:
            pass

        def close(self) -> None:
            pass

        def request(self, command: str) -> str:
            return "pong"

        def execute(self, command: int, payload=None):
            codes = payload["codes"]
            self.requests.append(tuple(codes))
            returned = codes[:75] if len(codes) == 200 else codes
            return CapitalChangeBatch(
                tuple(
                    CapitalChangeBlock(code[:2], 1, code[2:], len(returned), ())
                    for code in returned
                )
            )

    codes = [f"sh{index:06d}" for index in range(200)]
    transport = FakeTransport()
    result = TdxClient(transport=transport).corporate.capital_changes(
        codes, batch_size=200
    )

    assert [len(request) for request in transport.requests] == [200, 125]
    assert [block.full_code for block in result.blocks] == codes


def test_removed_corporate_helpers_stay_removed() -> None:
    helpers = TdxClient.in_memory().helpers
    removed = {
        "capital_changes",
        "xdxr",
        "equity_changes",
        "equity",
        "turnover",
        "factors",
        "local_adjusted_kline",
        "adjusted_kline",
    }
    assert all(not hasattr(helpers, name) for name in removed)


def test_helper_finance_cache_and_clear_cache() -> None:
    from eltdx.models import FinanceBatch

    class FakeTransport:
        def __init__(self) -> None:
            self.calls = 0

        def connect(self) -> None:
            pass

        def close(self) -> None:
            pass

        def request(self, command: str) -> str:
            return "pong"

        def execute(self, command: int, payload=None):
            assert command == 0x0010
            self.calls += 1
            return FinanceBatch(records=(), raw_payload=str(self.calls).encode())

    transport = FakeTransport()
    client = TdxClient(transport=transport)

    assert client.helpers._finance_map(["sz000001"]) == {}
    assert client.helpers._finance_map(["sz000001"]) == {}
    assert transport.calls == 1
    client.clear_cache()
    assert client.helpers._finance_map(["sz000001"]) == {}
    assert transport.calls == 2


def test_tdx_client_removes_legacy_flat_entrypoints() -> None:
    client = TdxClient.in_memory()
    removed = {
        "get_quote",
        "get_quote_depth",
        "get_legacy_quotes",
        "read_server_file",
        "get_count",
        "get_codes",
        "get_codes_all",
        "get_kline",
        "get_kline_all",
        "get_minute",
        "get_history_minute",
        "get_trades",
        "get_trades_all",
        "get_call_auction",
        "get_auction_0925",
        "get_gbbq",
        "get_xdxr",
        "get_equity",
        "get_turnover",
        "get_factors",
        "get_finance_batch",
    }

    assert all(not hasattr(client, name) for name in removed)
    assert all(not name.startswith("get_") for name in vars(type(client.helpers)))


def test_workday_service_weekday_fallback() -> None:
    service = WorkdayService()

    assert service.normalize("2026-05-27") == date(2026, 5, 27)
    assert service.text("20260527") == "2026-05-27"
    assert service.is_workday("2026-05-30") is False
    assert service.next_workday("2026-05-30") == date(2026, 6, 1)
    assert service.previous_workday("2026-05-30") == date(2026, 5, 29)
    assert service.range("2026-05-29", "2026-06-02") == [
        date(2026, 5, 29),
        date(2026, 6, 1),
        date(2026, 6, 2),
    ]


def test_workday_service_uses_client_daily_bars() -> None:
    from eltdx.models import KlineBar, KlineSeries

    def bar(day: date) -> KlineBar:
        return KlineBar(
            time=datetime(day.year, day.month, day.day, 15, 0),
            open=1.0,
            close=1.0,
            high=1.0,
            low=1.0,
            open_price_milli=1000,
            close_price_milli=1000,
            high_price_milli=1000,
            low_price_milli=1000,
            last_close_price_milli=1000,
            volume_raw=0,
            amount_raw=0,
            volume_wire_value=0,
            volume_lots=0,
            amount=0,
            open_delta_raw=0,
            close_delta_raw=0,
            high_delta_raw=0,
            low_delta_raw=0,
        )

    class FakeBars:
        def get(self, *args, **kwargs):
            assert kwargs["all_pages"] is True
            return KlineSeries(
                exchange="sh",
                market_id=1,
                code="000001",
                period_raw=4,
                period_param_raw=1,
                period_name="day",
                start=0,
                request_count=3,
                adjust_mode_raw=0,
                adjust_mode="none",
                anchor_date_raw=0,
                anchor_date=None,
                bars=(
                    bar(date(2026, 5, 27)),
                    bar(date(2026, 5, 29)),
                    bar(date(2026, 6, 1)),
                ),
            )

    class FakeClient:
        bars = FakeBars()

    service = WorkdayService(FakeClient())

    assert service.refresh() == 3
    assert service.is_workday("2026-05-28") is False
    assert service.next_workday("2026-05-28") == date(2026, 5, 29)
    assert service.previous_workday("2026-05-28") == date(2026, 5, 27)
    assert service.range("2026-05-27", "2026-06-01") == [
        date(2026, 5, 27),
        date(2026, 5, 29),
        date(2026, 6, 1),
    ]


def test_corporate_adjustment_factors_use_only_capital_changes() -> None:
    from eltdx.models import CapitalChangeBlock, CapitalChangeRecord

    def record(category: int, when: date, c1: float, c2: float, c3: float, c4: float):
        return CapitalChangeRecord(
            exchange="sz",
            market_id=0,
            code="000858",
            reserved_7=0,
            date_raw=int(when.strftime("%Y%m%d")),
            date=when,
            category_raw=category,
            category_name={1: "除权除息", 5: "股本变化"}.get(category),
            c1_raw=b"",
            c2_raw=b"",
            c3_raw=b"",
            c4_raw=b"",
            c1_float=c1,
            c2_float=c2,
            c3_float=c3,
            c4_float=c4,
            c1_value=c1,
            c2_value=c2,
            c3_value=c3,
            c4_value=c4,
        )

    changes = CapitalChangeBlock(
        exchange="sz",
        market_id=0,
        code="000858",
        block_count=1,
        records=(
            record(1, date(2024, 5, 1), 0.0, 25.0, 0.0, 2.0),
            record(1, date(2024, 6, 1), 0.0, 25.0, 0.0, 2.0),
            record(1, date(2024, 6, 2), 1.0, 0.0, 1.0, 0.0),
            record(5, date(2024, 6, 2), 0.0, 0.0, 100_000_000.0, 200_000_000.0),
        ),
    )

    class FakeTransport:
        def __init__(self) -> None:
            self.commands = []

        def connect(self) -> None:
            pass

        def close(self) -> None:
            pass

        def request(self, command: str) -> str:
            return "pong"

        def execute(self, command: int, payload=None):
            self.commands.append(command)
            assert command == 0x000F
            return changes

    transport = FakeTransport()
    factors = TdxClient(transport=transport).corporate.adjustment_factors(
        "sz000858", start_date="2024-05-31"
    )

    assert transport.commands == [0x000F]
    assert factors.count == 2
    assert factors.full_code == "sz000858"
    assert factors.start_date == date(2024, 5, 31)
    assert [item.date for item in factors.items] == [date(2024, 6, 1), date(2024, 6, 2)]
    assert factors.items[0].qfq_scale == pytest.approx(1.0 / 1.2 / 1.1)
    assert factors.items[0].qfq_offset == pytest.approx(((5.0 / 1.2) - 0.1) / 1.1)
    assert factors.items[0].qfq_scale * 10.0 + factors.items[
        0
    ].qfq_offset == pytest.approx(11.27272727)
    assert factors.items[1].qfq_scale == pytest.approx(1.0 / 1.1)
    assert factors.items[1].qfq_offset == pytest.approx(-0.1 / 1.1)
    assert factors.items[0].hfq_scale == pytest.approx(1.2)
    assert factors.items[0].hfq_offset == pytest.approx(-5.0)
    assert factors.items[1].hfq_scale == pytest.approx(1.32)
    assert factors.items[1].hfq_offset == pytest.approx(-4.88)
    assert factors.items[1].hfq_scale * 8.0 + factors.items[
        1
    ].hfq_offset == pytest.approx(5.68)

    all_events = TdxClient(transport=FakeTransport()).corporate.adjustment_factors(
        "sz000858"
    )
    assert all_events.count == 3

    from eltdx.equity import build_adjustment_factor_response

    same_day_changes = replace(
        changes,
        records=(
            record(1, date(2024, 6, 1), 0.0, 25.0, 0.0, 2.0),
            record(1, date(2024, 6, 1), 1.0, 0.0, 1.0, 0.0),
        ),
    )
    same_day = build_adjustment_factor_response(same_day_changes)
    assert same_day.count == 1
    assert same_day.items[0].qfq_scale == pytest.approx(1.0 / 1.2 / 1.1)
    assert same_day.items[0].qfq_offset == pytest.approx(((5.0 / 1.2) - 0.1) / 1.1)
    assert same_day.items[0].hfq_scale == pytest.approx(1.32)
    assert same_day.items[0].hfq_offset == pytest.approx(-5.4)
    assert same_day.items[0].hfq_scale * 8.0 + same_day.items[
        0
    ].hfq_offset == pytest.approx(5.16)

    partial_anchor = TdxClient(transport=FakeTransport()).corporate.adjustment_factors(
        "sz000858", anchor_date="2024-06-01", start_date="2024-05-31"
    )
    assert partial_anchor.items[0].qfq_scale == pytest.approx(1.0 / 1.2)
    assert partial_anchor.items[0].qfq_offset == pytest.approx(5.0 / 1.2)
    assert partial_anchor.items[1].qfq_scale == 1.0
    assert partial_anchor.items[1].qfq_offset == 0.0

    anchored = TdxClient(transport=FakeTransport()).corporate.adjustment_factors(
        "sz000858", anchor_date="2024-05-31", start_date="2024-05-31"
    )
    assert anchored.anchor_date == date(2024, 5, 31)
    assert all(
        item.qfq_scale == 1.0 and item.qfq_offset == 0.0 for item in anchored.items
    )

    with pytest.raises(ValueError, match="must not be earlier"):
        TdxClient(transport=FakeTransport()).corporate.adjustment_factors(
            "sz000858", anchor_date="2024-05-01", start_date="2024-05-31"
        )


def test_trades_all_pages_until_short_page() -> None:
    from eltdx.models import TradePage, TradeTick

    tick = TradeTick(
        index=0,
        absolute_index=0,
        time_minutes=570,
        time_label="09:30",
        trade_datetime=None,
        price=10.0,
        price_milli=10000,
        volume=1,
        order_count=0,
        status_raw=0,
        side="buy",
        price_delta_raw=0,
        price_acc_raw=1000,
    )

    class FakeTransport:
        def connect(self) -> None:
            pass

        def close(self) -> None:
            pass

        def request(self, command: str) -> str:
            return "pong"

        def execute(self, command: int, payload=None):
            assert command == 0x0FC5
            count = 2 if payload["start"] == 0 else 1 if payload["start"] == 2 else 0
            return TradePage(
                exchange="sz",
                market_id=0,
                code="000001",
                start=payload["start"],
                request_count=payload["count"],
                ticks=tuple(tick for _ in range(count)),
            )

    page = TdxClient(transport=FakeTransport()).trades.all_today(
        "sz000001", page_size=2
    )

    assert page.start == 0
    assert page.request_count == 3
    assert page.count == 3


def test_trades_all_pages_restore_chronological_order_without_renumbering_source_indexes() -> (
    None
):
    from eltdx.models import TradePage, TradeTick

    template = TradeTick(
        index=0,
        absolute_index=0,
        time_minutes=13 * 60 + 30,
        time_label="13:30",
        trade_datetime=None,
        price=10.0,
        price_milli=10000,
        volume=1,
        order_count=0,
        status_raw=0,
        side="buy",
        price_delta_raw=0,
        price_acc_raw=1000,
    )
    pages = {
        0: (
            template,
            replace(
                template,
                index=1,
                absolute_index=1,
                time_minutes=13 * 60 + 31,
                time_label="13:31",
            ),
        ),
        2: (
            replace(
                template,
                absolute_index=2,
                time_minutes=13 * 60 + 28,
                time_label="13:28",
            ),
            replace(
                template,
                index=1,
                absolute_index=3,
                time_minutes=13 * 60 + 29,
                time_label="13:29",
            ),
        ),
        4: (
            replace(
                template,
                absolute_index=4,
                time_minutes=13 * 60 + 27,
                time_label="13:27",
            ),
        ),
        5: (),
    }

    class FakeTransport:
        def connect(self) -> None:
            pass

        def close(self) -> None:
            pass

        def request(self, command: str) -> str:
            return "pong"

        def execute(self, command: int, payload=None):
            assert command == 0x0FC5
            ticks = pages[payload["start"]]
            return TradePage(
                exchange="sz",
                market_id=0,
                code="000001",
                start=payload["start"],
                request_count=payload["count"],
                ticks=ticks,
            )

    page = TdxClient(transport=FakeTransport()).trades.all_today(
        "sz000001", page_size=2
    )

    assert [tick.time_label for tick in page.ticks] == [
        "13:27",
        "13:28",
        "13:29",
        "13:30",
        "13:31",
    ]
    assert [tick.absolute_index for tick in page.ticks] == [4, 2, 3, 0, 1]


def test_trades_all_history_uses_server_page_limit_without_losing_early_ticks() -> None:
    from eltdx.models import TradePage, TradeTick

    regular = TradeTick(
        index=0,
        absolute_index=0,
        time_minutes=15 * 60,
        time_label="15:00",
        trade_datetime=datetime(2026, 5, 20, 15, 0),
        price=10.0,
        price_milli=10000,
        volume=1,
        order_count=1,
        status_raw=0,
        side="buy",
        price_delta_raw=0,
        price_acc_raw=1000,
    )
    opening = replace(
        regular,
        time_minutes=9 * 60 + 25,
        time_label="09:25",
        trade_datetime=datetime(2026, 5, 20, 9, 25),
        event_kind="opening_match",
    )
    starts = []

    class FakeTransport:
        def connect(self) -> None:
            pass

        def close(self) -> None:
            pass

        def request(self, command: str) -> str:
            return "pong"

        def execute(self, command: int, payload=None):
            assert command == 0x0FC6
            assert payload["count"] == 1800
            starts.append(payload["start"])
            ticks = (
                tuple(regular for _ in range(1800))
                if payload["start"] == 0
                else (opening,)
                if payload["start"] == 1800
                else ()
            )
            return TradePage(
                exchange="sz",
                market_id=0,
                code="000001",
                start=payload["start"],
                request_count=payload["count"],
                ticks=ticks,
                trading_date=date(2026, 5, 20),
            )

    page = TdxClient(transport=FakeTransport()).trades.all_history(
        "sz000001", "2026-05-20"
    )

    assert starts == [0, 1800, 1801]
    assert page.count == 1801
    assert page.opening_matches == (opening,)


def test_trade_event_specific_helpers_split_today_and_history_sources() -> None:
    from eltdx.models import TradePage, TradeTick

    tick = TradeTick(
        index=0,
        absolute_index=0,
        time_minutes=9 * 60 + 25,
        time_label="09:25",
        trade_datetime=None,
        price=11.11,
        price_milli=11110,
        volume=123,
        order_count=1,
        status_raw=2,
        side="neutral",
        price_delta_raw=0,
        price_acc_raw=1111,
        event_kind="opening_match",
    )

    class FakeTransport:
        def connect(self) -> None:
            pass

        def close(self) -> None:
            pass

        def request(self, command: str) -> str:
            return "pong"

        def execute(self, command: int, payload=None):
            if command == 0x000D:
                return type(
                    "Handshake",
                    (),
                    {
                        "server_date_1": date(2026, 5, 21),
                        "server_date_2": date(2026, 5, 21),
                    },
                )()
            assert command == 0x0FC6
            return TradePage(
                exchange="sz",
                market_id=0,
                code="000001",
                start=payload["start"],
                request_count=payload["count"],
                ticks=(tick,),
                trading_date=date(2026, 5, 20),
            )

    result = TdxClient(transport=FakeTransport()).trades.opening_match_history(
        "000001", "2026-05-20"
    )
    assert result is tick


def test_trade_event_specific_helpers_return_none_when_no_opening_match() -> None:
    from eltdx.models import TradePage

    class FakeTransport:
        def connect(self) -> None:
            pass

        def close(self) -> None:
            pass

        def request(self, command: str) -> str:
            return "pong"

        def execute(self, command: int, payload=None):
            assert command == 0x0FC5
            return TradePage(
                exchange="sz",
                market_id=0,
                code="000001",
                start=payload["start"],
                request_count=payload["count"],
                ticks=(),
            )

    result = TdxClient(transport=FakeTransport()).trades.opening_match_today("000001")
    assert result is None


def test_auction_series_forwards_optional_trading_date() -> None:
    class FakeTransport:
        def connect(self) -> None:
            pass

        def close(self) -> None:
            pass

        def request(self, command: str) -> str:
            return "pong"

        def execute(self, command: int, payload=None):
            assert command == 0x056A
            return payload

    auctions = TdxClient(transport=FakeTransport()).auctions
    assert auctions.series("000001")["trading_date"] is None
    assert auctions.series("000001", "2026-08-14")["trading_date"] == "2026-08-14"


def test_trade_event_specific_helpers_paginate_to_opening_match() -> None:
    from eltdx.models import TradePage, TradeTick

    regular = TradeTick(
        index=0,
        absolute_index=0,
        time_minutes=15 * 60,
        time_label="15:00",
        trade_datetime=datetime(2026, 5, 20, 15, 0),
        price=10.0,
        price_milli=10000,
        volume=1,
        order_count=1,
        status_raw=0,
        side="buy",
        price_delta_raw=0,
        price_acc_raw=1000,
    )
    opening = replace(
        regular,
        time_minutes=9 * 60 + 25,
        time_label="09:25",
        trade_datetime=datetime(2026, 5, 20, 9, 25),
        event_kind="opening_match",
    )
    starts = []

    class FakeTransport:
        def connect(self) -> None:
            pass

        def close(self) -> None:
            pass

        def request(self, command: str) -> str:
            return "pong"

        def execute(self, command: int, payload=None):
            if command == 0x000D:
                return type(
                    "Handshake",
                    (),
                    {
                        "server_date_1": date(2026, 5, 21),
                        "server_date_2": date(2026, 5, 21),
                    },
                )()
            assert command == 0x0FC6
            starts.append(payload["start"])
            ticks = (
                tuple(regular for _ in range(1800))
                if payload["start"] == 0
                else (opening,)
            )
            return TradePage(
                exchange="sz",
                market_id=0,
                code="000001",
                start=payload["start"],
                request_count=payload["count"],
                ticks=ticks,
                trading_date=date(2026, 5, 20),
            )

    result = TdxClient(transport=FakeTransport()).trades.opening_match_history(
        "000001", "2026-05-20"
    )

    assert starts == [0, 1800]
    assert result is opening


def test_trade_page_keeps_snapshot_and_opening_match_properties() -> None:
    from eltdx.models import TradePage, TradeTick

    snapshot = TradeTick(
        index=0,
        absolute_index=0,
        time_minutes=9 * 60 + 25,
        time_label="09:25",
        trade_datetime=None,
        price=11.10,
        price_milli=11100,
        volume=0,
        order_count=0,
        status_raw=8,
        side="status_8",
        price_delta_raw=0,
        price_acc_raw=1110,
        event_kind="auction_snapshot",
    )
    opening = TradeTick(
        index=1,
        absolute_index=1,
        time_minutes=9 * 60 + 25,
        time_label="09:25",
        trade_datetime=None,
        price=11.11,
        price_milli=11110,
        volume=123,
        order_count=1,
        status_raw=2,
        side="neutral",
        price_delta_raw=0,
        price_acc_raw=1111,
        event_kind="opening_match",
    )
    closing = TradeTick(
        index=2,
        absolute_index=2,
        time_minutes=15 * 60,
        time_label="15:00",
        trade_datetime=None,
        price=11.12,
        price_milli=11120,
        volume=456,
        order_count=1,
        status_raw=2,
        side="neutral",
        price_delta_raw=1,
        price_acc_raw=1112,
    )
    after_hours = TradeTick(
        index=3,
        absolute_index=3,
        time_minutes=15 * 60 + 5,
        time_label="15:05",
        trade_datetime=None,
        price=11.12,
        price_milli=11120,
        volume=20,
        order_count=1,
        status_raw=5,
        side="status_5",
        price_delta_raw=0,
        price_acc_raw=1112,
    )

    class FakeTransport:
        def connect(self) -> None:
            pass

        def close(self) -> None:
            pass

        def request(self, command: str) -> str:
            return "pong"

        def execute(self, command: int, payload=None):
            if command == 0x000D:
                return type(
                    "Handshake",
                    (),
                    {
                        "server_date_1": date(2026, 5, 20),
                        "server_date_2": date(2026, 5, 20),
                    },
                )()
            assert command == 0x0FC5
            return TradePage(
                exchange="sz",
                market_id=0,
                code="000001",
                start=payload["start"],
                request_count=payload["count"],
                ticks=(snapshot, opening, closing, after_hours),
            )

    page = TdxClient(transport=FakeTransport()).trades.today("sz000001", count=4)
    assert page.ticks == (snapshot, opening, closing, after_hours)
    assert page.auction_snapshots == (snapshot,)
    assert page.opening_matches == (opening,)
    assert page.actual_trades == (opening, closing, after_hours)
    assert page.after_hours_trades == (after_hours,)
    assert snapshot.is_actual_trade is False
    assert opening.is_actual_trade is True
    assert closing.is_actual_trade is True
    assert after_hours.is_actual_trade is True
    assert after_hours.is_after_hours_fixed_price is True
    assert closing.is_after_hours_fixed_price is False


def test_trade_page_has_more_until_an_empty_page_confirms_completion() -> None:
    from eltdx.models import TradePage, TradeTick

    tick = TradeTick(
        index=0,
        absolute_index=0,
        time_minutes=15 * 60,
        time_label="15:00",
        trade_datetime=None,
        price=10.0,
        price_milli=10000,
        volume=1,
        order_count=1,
        status_raw=0,
        side="buy",
        price_delta_raw=0,
        price_acc_raw=1000,
    )
    short_page = TradePage("sz", 0, "000001", 0, 1800, (tick,))
    empty_page = TradePage("sz", 0, "000001", 1, 1800, ())

    assert short_page.has_more is True
    assert empty_page.has_more is False


def test_json_helpers_handle_models_and_bytes() -> None:
    from eltdx.models import QuoteLevel

    value = {
        "date": date(2026, 5, 20),
        "level": QuoteLevel(price=1.23, volume=100, price_delta_raw=1),
        "raw": b"\x01\x02",
    }

    assert to_jsonable(value) == {
        "date": "2026-05-20",
        "level": {"price": 1.23, "volume": 100, "price_delta_raw": 1},
        "raw": "0102",
    }
    assert '"raw": "0102"' in to_json(value)


def test_codes_all_pages_until_short_page() -> None:
    class FakeTransport:
        def connect(self) -> None:
            pass

        def close(self) -> None:
            pass

        def request(self, command: str) -> str:
            return "pong"

        def execute(self, command: int, payload=None):
            assert command == 0x044D
            start = payload["start"]
            if start == 0:
                return ["a", "b"]
            if start == 2:
                return ["c"]
            if start == 3:
                return []
            raise AssertionError(f"unexpected start: {start}")

    assert TdxClient(transport=FakeTransport()).codes.all("sz", page_size=2) == [
        "a",
        "b",
        "c",
    ]


def test_bars_all_pages_until_short_page() -> None:
    from eltdx.models import KlineSeries

    class FakeTransport:
        def connect(self) -> None:
            pass

        def close(self) -> None:
            pass

        def request(self, command: str) -> str:
            return "pong"

        def execute(self, command: int, payload=None):
            assert command == 0x052D
            start = payload["start"]
            count = payload["count"]
            bars = tuple(range(count)) if start == 0 else (0,) if start == 2 else ()
            return KlineSeries(
                exchange="sz",
                market_id=0,
                code="000001",
                period_raw=4,
                period_param_raw=1,
                period_name="day",
                start=start,
                request_count=count,
                adjust_mode_raw=0,
                adjust_mode="none",
                anchor_date_raw=0,
                anchor_date=None,
                bars=bars,
            )

    page = TdxClient(transport=FakeTransport()).bars.get(
        "sz000001", all_pages=True, page_size=2
    )

    assert page.start == 0
    assert page.request_count == 3
    assert page.bars == (0, 1, 0)


def test_server_page_limits_are_validated_before_request() -> None:
    client = TdxClient.in_memory()

    with pytest.raises(ValueError, match="between 1 and 1800"):
        client.trades.history("sz000001", "2026-05-20", count=1801)
    with pytest.raises(ValueError, match="between 1 and 800"):
        client.bars.get("sz000001", count=801)
    with pytest.raises(ValueError, match="between 0 and 1600"):
        client.codes.list("sz", limit=1601)
    assert client.codes.list("sz", limit=0)["payload"]["limit"] == 0


def test_finance_batch_field_filter_is_local() -> None:
    from eltdx.models import FinanceBatch, FinanceRecord

    class FakeTransport:
        def connect(self) -> None:
            pass

        def close(self) -> None:
            pass

        def request(self, command: str) -> str:
            return "pong"

        def execute(self, command: int, payload=None):
            assert command == 0x0010
            assert "fields" not in payload
            record = FinanceRecord(
                exchange="sz",
                market_id=0,
                code="000001",
                finance_info_raw=b"",
                liu_tong_gu_ben_raw_float=100.0,
                province_raw=0,
                industry_raw=0,
                updated_date_raw=0,
                updated_date=None,
                ipo_date_raw=0,
                ipo_date=None,
                zong_gu_ben_raw_float=200.0,
                guo_jia_gu_raw_float=0.0,
                fa_qi_ren_fa_ren_gu_raw_float=0.0,
                fa_ren_gu_raw_float=0.0,
                b_gu_raw_float=0.0,
                h_gu_raw_float=0.0,
                eps_raw=0.0,
                zong_zi_chan_raw_float=0.0,
                liu_dong_zi_chan_raw_float=0.0,
                gu_ding_zi_chan_raw_float=0.0,
                wu_xing_zi_chan_raw_float=0.0,
                gu_dong_ren_shu_raw_float=0.0,
                liu_dong_fu_zhai_raw_float=0.0,
                chang_qi_fu_zhai_raw_float=0.0,
                zi_ben_gong_ji_jin_raw_float=0.0,
                jing_zi_chan_raw_float=0.0,
                zhu_ying_shou_ru_raw_float=0.0,
                zhu_ying_li_run_raw_float=0.0,
                ying_shou_zhang_kuan_raw_float=0.0,
                ying_ye_li_run_raw_float=0.0,
                tou_zi_shou_yu_raw_float=0.0,
                jing_ying_xian_jin_liu_raw_float=0.0,
                zong_xian_jin_liu_raw_float=0.0,
                cun_huo_raw_float=0.0,
                li_run_zong_he_raw_float=0.0,
                shui_hou_li_run_raw_float=0.0,
                jing_li_run_raw_float=0.0,
                wei_fen_li_run_raw_float=0.0,
                mei_gu_jing_zi_chan_raw_float=0.0,
                bao_liu_2_raw_float=0.0,
            )
            return FinanceBatch(records=(record,))

    selected = TdxClient(transport=FakeTransport()).corporate.finance_batch(
        ["sz000001"], fields=["流通股本", "total_shares"]
    )

    assert selected == [
        {"full_code": "sz000001", "流通股本": 1_000_000.0, "total_shares": 2_000_000.0}
    ]


def test_socket_transport_has_no_legacy_reader_or_socket_owner_paths() -> None:
    transport = SocketTransport(hosts=["127.0.0.1:1"], timeout=0.01)

    assert all(
        not hasattr(transport, name)
        for name in (
            "_socket",
            "_reader_thread",
            "_heartbeat_thread",
            "_stop_reader",
            "_stop_heartbeat",
            "_pending",
            "_send_lock",
            "_reader_loop",
            "_heartbeat_loop",
        )
    )
