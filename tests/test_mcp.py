import asyncio
import os
import sys
import threading
from datetime import date
from pathlib import Path

import pytest

from eltdx import TdxClient
from eltdx.api.bars import BarApi
from eltdx.api.quotes import QuoteApi
from eltdx.f10 import F10Client
from eltdx.mcp import (
    _ClientRegistry,
    _McpTools,
    create_mcp_server,
    docs_index,
    kline,
    quote,
    quote_depth,
)
from eltdx.models import KlineBar, KlineSeries, QuoteSnapshot


def test_mcp_docs_index_lists_main_documents() -> None:
    index = docs_index()

    assert index["API"] == "eltdx://docs/api"
    assert index["helpers"] == "eltdx://docs/helpers"
    assert index["MCP"] == "eltdx://docs/mcp"


def test_mcp_quote_returns_jsonable_snapshot_and_closes(monkeypatch) -> None:
    snapshot = QuoteSnapshot(
        exchange="sz",
        market_id=0,
        code="000001",
        active1=0,
        last_price=12.0,
        pre_close_price=10.0,
        open_price=11.0,
        high_price=12.5,
        low_price=10.8,
        time_raw=0,
        unknown_after_time_raw=0,
        total_hand=5000,
        current_hand=100,
        amount=6_000_000.0,
        amount_raw=0,
        inside_dish=0,
        outer_disc=0,
        unknown_after_outer_raw=0,
        open_amount_raw=0,
        open_amount_yuan=1_000_000.0,
        buy_levels=(),
        sell_levels=(),
        tail_raw=b"",
    )
    calls = []

    monkeypatch.setattr(TdxClient, "connect", lambda self: calls.append("connect"))
    monkeypatch.setattr(TdxClient, "close", lambda self: calls.append("close"))
    monkeypatch.setattr(QuoteApi, "get_snapshots", lambda self, codes: [snapshot])

    result = quote("sz000001", timeout=1)

    assert result[0]["code"] == "000001"
    assert result[0]["last_price"] == 12.0
    assert result[0]["tail_raw"] == ""
    assert calls == ["connect", "close"]


def test_mcp_kline_returns_jsonable_series(monkeypatch) -> None:
    series = KlineSeries(
        exchange="sz",
        market_id=0,
        code="000001",
        period_raw=9,
        period_param_raw=9,
        period_name="day",
        start=0,
        request_count=1,
        adjust_mode_raw=0,
        adjust_mode="none",
        anchor_date_raw=0,
        anchor_date=date(2026, 5, 20),
        bars=(
            KlineBar(
                time=date(2026, 5, 20),
                open=10.0,
                close=11.0,
                high=11.5,
                low=9.8,
                open_price_milli=10000,
                close_price_milli=11000,
                high_price_milli=11500,
                low_price_milli=9800,
                last_close_price_milli=9900,
                volume_raw=1,
                amount_raw=2,
                volume_wire_value=100.0,
                volume_lots=100.0,
                amount=110000.0,
                open_delta_raw=0,
                close_delta_raw=0,
                high_delta_raw=0,
                low_delta_raw=0,
            ),
        ),
    )

    monkeypatch.setattr(TdxClient, "connect", lambda self: None)
    monkeypatch.setattr(TdxClient, "close", lambda self: None)
    monkeypatch.setattr(BarApi, "get", lambda self, *args, **kwargs: series)

    result = kline("sz000001", timeout=1)

    assert result["period_name"] == "day"
    assert result["anchor_date"] == "2026-05-20"
    assert result["bars"][0]["time"] == "2026-05-20"


def test_mcp_kline_rejects_unbounded_result() -> None:
    with pytest.raises(ValueError, match="count must be between 1 and 800"):
        kline("sz000001", count=801)


def test_mcp_quote_depth_rejects_more_than_100_codes() -> None:
    with pytest.raises(ValueError, match="at most 100 securities"):
        quote_depth([f"sz{index:06d}" for index in range(101)])


def test_mcp_tool_validates_codes_before_connecting(monkeypatch) -> None:
    registry = _ClientRegistry()
    tools = _McpTools(registry)
    connect_calls = []

    monkeypatch.setattr(TdxClient, "connect", lambda self: connect_calls.append(id(self)))

    with pytest.raises(ValueError, match="at most 200 securities"):
        tools.quote([f"sz{index:06d}" for index in range(201)])
    with pytest.raises(ValueError, match="at most 100 securities"):
        tools.quote_depth([f"sz{index:06d}" for index in range(101)])

    assert connect_calls == []
    assert not registry._clients
    registry.close()


def test_mcp_registry_rejects_invalid_host_before_registering_pending_key() -> None:
    registry = _ClientRegistry()

    for _ in range(2):
        with pytest.raises(ValueError, match="host:port"):
            _use_registry_once(registry, timeout=1, host="invalid-host")
        assert not registry._clients
        assert not registry._pending_keys

    registry.close()


def test_mcp_registry_rolls_back_pending_key_when_client_construction_fails(monkeypatch) -> None:
    registry = _ClientRegistry()

    def fail_client_construction(**_kwargs):
        raise RuntimeError("construction failed")

    monkeypatch.setattr("eltdx.mcp.TdxClient", fail_client_construction)

    for _ in range(2):
        with pytest.raises(RuntimeError, match="construction failed"):
            _use_registry_once(registry, timeout=1, host="valid.example:7709")
        assert not registry._clients
        assert not registry._pending_keys

    registry.close()


def test_mcp_registry_initializes_different_keys_concurrently(monkeypatch) -> None:
    registry = _ClientRegistry()
    first_connecting = threading.Event()
    release_first = threading.Event()
    second_acquired = threading.Event()

    def connect(client) -> None:
        if client.host == "first:7709":
            first_connecting.set()
            assert release_first.wait(timeout=2)

    monkeypatch.setattr(TdxClient, "connect", connect)
    monkeypatch.setattr(TdxClient, "close", lambda self: None)

    first = threading.Thread(
        target=lambda: _use_registry_once(registry, timeout=1, host="first:7709"),
    )
    second = threading.Thread(
        target=lambda: _use_registry_once(
            registry,
            timeout=1,
            host="second:7709",
            acquired=second_acquired,
        ),
    )
    first.start()
    assert first_connecting.wait(timeout=1)
    second.start()

    assert second_acquired.wait(timeout=1)
    release_first.set()
    first.join(timeout=2)
    second.join(timeout=2)
    assert not first.is_alive()
    assert not second.is_alive()
    registry.close()


def test_mcp_registry_uses_four_connections_for_same_server_threads(monkeypatch) -> None:
    registry = _ClientRegistry()
    clients = []

    monkeypatch.setattr(TdxClient, "connect", lambda self: None)
    monkeypatch.setattr(TdxClient, "close", lambda self: None)

    with registry.use(timeout=3, host="same:7709") as client:
        clients.append(client)

    assert clients[0].pool_size == 4
    assert clients[0].transport.pool_size == 4
    assert clients[0].transport.hosts == ("same:7709",)
    registry.close()


def test_mcp_registry_initializes_same_key_once_for_concurrent_calls(monkeypatch) -> None:
    registry = _ClientRegistry()
    connect_entered = threading.Event()
    release_connect = threading.Event()
    acquired_clients = []
    connect_calls = []

    def connect(client) -> None:
        connect_calls.append(id(client))
        connect_entered.set()
        assert release_connect.wait(timeout=2)

    monkeypatch.setattr(TdxClient, "connect", connect)
    monkeypatch.setattr(TdxClient, "close", lambda self: None)

    threads = [
        threading.Thread(
            target=lambda: _use_registry_once(
                registry,
                timeout=3,
                host="same:7709",
                clients=acquired_clients,
            ),
        )
        for _ in range(2)
    ]
    threads[0].start()
    assert connect_entered.wait(timeout=1)
    threads[1].start()
    release_connect.set()
    for thread in threads:
        thread.join(timeout=2)
        assert not thread.is_alive()

    assert len(connect_calls) == 1
    assert len(acquired_clients) == 2
    assert acquired_clients[0] == acquired_clients[1] == connect_calls[0]
    registry.close()


def test_mcp_registry_evicts_idle_lru_client(monkeypatch) -> None:
    registry = _ClientRegistry()
    closed_timeouts = []

    monkeypatch.setattr(TdxClient, "connect", lambda self: None)
    monkeypatch.setattr(TdxClient, "close", lambda self: closed_timeouts.append(self.timeout))

    for timeout in range(1, 18):
        _use_registry_once(registry, timeout=timeout, host=None)

    assert len(registry._clients) == 16
    assert 1.0 in closed_timeouts
    assert 1.0 not in {key[1] for key in registry._clients}
    registry.close()


def test_mcp_f10_tools_do_not_consume_market_client_slots(monkeypatch) -> None:
    registry = _ClientRegistry()
    tools = _McpTools(registry)

    monkeypatch.setattr(F10Client, "company_profile", lambda self, code, section="8": {"code": code})

    for timeout in range(1, 18):
        assert tools.company_profile("sz000001", timeout=timeout) == {"code": "sz000001"}

    assert not registry._clients
    registry.close()


def test_mcp_registry_close_waits_for_active_call(monkeypatch) -> None:
    registry = _ClientRegistry()
    acquired = threading.Event()
    release_call = threading.Event()
    close_called = threading.Event()

    monkeypatch.setattr(TdxClient, "connect", lambda self: None)
    monkeypatch.setattr(TdxClient, "close", lambda self: close_called.set())

    worker = threading.Thread(
        target=lambda: _hold_registry_client(registry, acquired, release_call),
    )
    worker.start()
    assert acquired.wait(timeout=1)

    closer = threading.Thread(target=registry.close)
    closer.start()
    assert not close_called.wait(timeout=0.1)
    release_call.set()

    worker.join(timeout=2)
    closer.join(timeout=2)
    assert not worker.is_alive()
    assert not closer.is_alive()
    assert close_called.is_set()


def test_mcp_registry_retries_failed_close(monkeypatch) -> None:
    registry = _ClientRegistry()
    close_calls = []

    monkeypatch.setattr(TdxClient, "connect", lambda self: None)

    def close(client) -> None:
        close_calls.append(id(client))
        if len(close_calls) == 1:
            raise RuntimeError("temporary close failure")

    monkeypatch.setattr(TdxClient, "close", close)
    _use_registry_once(registry, timeout=1, host=None)

    with pytest.raises(RuntimeError, match="failed to close 1"):
        registry.close()
    assert len(registry._clients) == 1

    registry.close()
    assert len(close_calls) == 2
    assert not registry._clients


def test_mcp_registry_connect_failure_cleanup_blocks_shutdown(monkeypatch) -> None:
    registry = _ClientRegistry()
    cleanup_entered = threading.Event()
    release_cleanup = threading.Event()
    shutdown_finished = threading.Event()
    acquire_errors = []

    monkeypatch.setattr(TdxClient, "connect", lambda self: (_ for _ in ()).throw(RuntimeError("connect")))

    def close(client) -> None:
        cleanup_entered.set()
        assert release_cleanup.wait(timeout=2)

    monkeypatch.setattr(TdxClient, "close", close)

    def acquire() -> None:
        try:
            _use_registry_once(registry, timeout=1, host=None)
        except RuntimeError as exc:
            acquire_errors.append(str(exc))

    worker = threading.Thread(target=acquire)
    worker.start()
    assert cleanup_entered.wait(timeout=1)

    closer = threading.Thread(target=lambda: (registry.close(), shutdown_finished.set()))
    closer.start()
    assert not shutdown_finished.wait(timeout=0.1)
    release_cleanup.set()

    worker.join(timeout=2)
    closer.join(timeout=2)
    assert acquire_errors == ["connect"]
    assert shutdown_finished.is_set()
    assert not registry._clients


def test_mcp_registry_retains_failed_connect_cleanup_for_retry(monkeypatch) -> None:
    registry = _ClientRegistry()
    close_calls = []

    monkeypatch.setattr(TdxClient, "connect", lambda self: (_ for _ in ()).throw(RuntimeError("connect")))

    def close(client) -> None:
        close_calls.append(id(client))
        if len(close_calls) == 1:
            raise RuntimeError("cleanup")

    monkeypatch.setattr(TdxClient, "close", close)

    with pytest.raises(RuntimeError, match="connection and cleanup both failed"):
        _use_registry_once(registry, timeout=1, host=None)
    assert len(registry._clients) == 1
    assert next(iter(registry._clients.values())).failed is True

    registry.close()
    assert len(close_calls) == 2
    assert not registry._clients


def test_mcp_registry_has_only_one_concurrent_shutdown_owner(monkeypatch) -> None:
    registry = _ClientRegistry()
    acquired = threading.Event()
    release_call = threading.Event()
    close_entered = threading.Event()
    release_close = threading.Event()
    close_calls = []

    monkeypatch.setattr(TdxClient, "connect", lambda self: None)

    def close(client) -> None:
        close_calls.append(id(client))
        close_entered.set()
        assert release_close.wait(timeout=2)

    monkeypatch.setattr(TdxClient, "close", close)
    worker = threading.Thread(target=lambda: _hold_registry_client(registry, acquired, release_call))
    worker.start()
    assert acquired.wait(timeout=1)

    closers = [threading.Thread(target=registry.close) for _ in range(2)]
    for closer in closers:
        closer.start()
    release_call.set()
    assert close_entered.wait(timeout=1)
    assert len(close_calls) == 1
    release_close.set()

    worker.join(timeout=2)
    for closer in closers:
        closer.join(timeout=2)
        assert not closer.is_alive()
    assert len(close_calls) == 1


def test_mcp_registry_same_new_key_evicts_only_one_client(monkeypatch) -> None:
    registry = _ClientRegistry()
    eviction_entered = threading.Event()
    release_eviction = threading.Event()
    close_calls = []
    acquired_clients = []

    monkeypatch.setattr(TdxClient, "connect", lambda self: None)

    def close(client) -> None:
        close_calls.append(client.timeout)
        if len(close_calls) == 1:
            eviction_entered.set()
            assert release_eviction.wait(timeout=2)

    monkeypatch.setattr(TdxClient, "close", close)
    for timeout in range(1, 17):
        _use_registry_once(registry, timeout=timeout, host=None)

    workers = [
        threading.Thread(
            target=lambda: _use_registry_once(
                registry,
                timeout=17,
                host=None,
                clients=acquired_clients,
            )
        )
        for _ in range(2)
    ]
    workers[0].start()
    assert eviction_entered.wait(timeout=1)
    workers[1].start()
    assert close_calls == [1.0]
    release_eviction.set()

    for worker in workers:
        worker.join(timeout=2)
        assert not worker.is_alive()
    assert len(acquired_clients) == 2
    assert acquired_clients[0] == acquired_clients[1]
    assert close_calls == [1.0]
    registry.close()


def _use_registry_once(
    registry: _ClientRegistry,
    *,
    timeout: float,
    host: str | None,
    acquired: threading.Event | None = None,
    clients: list[int] | None = None,
) -> None:
    with registry.use(timeout=timeout, host=host) as client:
        if clients is not None:
            clients.append(id(client))
        if acquired is not None:
            acquired.set()


def _hold_registry_client(
    registry: _ClientRegistry,
    acquired: threading.Event,
    release_call: threading.Event,
) -> None:
    with registry.use(timeout=1, host=None):
        acquired.set()
        assert release_call.wait(timeout=2)


def _sdk2_client():
    try:
        from mcp import Client
    except ImportError:
        pytest.skip("MCP SDK 2 optional dependency is not installed")
    return Client


def test_mcp_sdk2_lists_tools_and_reads_resources() -> None:
    Client = _sdk2_client()

    async def exercise() -> None:
        async with Client(create_mcp_server()) as client:
            tools = await client.list_tools()
            tool_names = {tool.name for tool in tools.tools}
            assert len(tool_names) == 17
            assert {
                "eltdx_quote",
                "eltdx_kline",
                "eltdx_minute",
                "eltdx_trades",
                "eltdx_shortline_indicators",
                "eltdx_docs_index",
            } <= tool_names
            assert all("self" not in tool.input_schema.get("properties", {}) for tool in tools.tools)
            assert all(tool.output_schema is not None for tool in tools.tools)

            result = await client.call_tool("eltdx_docs_index", {})
            assert result.is_error is False
            assert result.structured_content["MCP"] == "eltdx://docs/mcp"

            resources = await client.list_resources()
            assert len(resources.resources) == 8
            document = await client.read_resource("eltdx://docs/mcp")
            assert "# MCP" in document.contents[0].text

    asyncio.run(exercise())


def test_mcp_sdk2_real_stdio_process() -> None:
    Client = _sdk2_client()
    from mcp import StdioServerParameters, stdio_client

    source_root = str((Path(__file__).resolve().parents[1] / "src"))
    environment = dict(os.environ)
    environment["PYTHONPATH"] = os.pathsep.join(
        item for item in (source_root, environment.get("PYTHONPATH")) if item
    )

    async def exercise() -> None:
        parameters = StdioServerParameters(
            command=sys.executable,
            args=["-m", "eltdx.mcp"],
            env=environment,
        )
        async with Client(stdio_client(parameters), mode="legacy") as client:
            tools = await client.list_tools()
            assert len(tools.tools) == 17
            result = await client.call_tool("eltdx_docs_index", {})
            assert result.structured_content["MCP"] == "eltdx://docs/mcp"
            document = await client.read_resource("eltdx://docs/mcp")
            assert "# MCP" in document.contents[0].text

    asyncio.run(exercise())


def test_mcp_sdk2_reuses_and_closes_client(monkeypatch) -> None:
    Client = _sdk2_client()

    calls = []
    quote_clients = []

    monkeypatch.setattr(TdxClient, "connect", lambda self: calls.append(("connect", id(self))))
    monkeypatch.setattr(TdxClient, "close", lambda self: calls.append(("close", id(self))))
    monkeypatch.setattr(
        QuoteApi,
        "get_snapshots",
        lambda self, codes: quote_clients.append(id(self)) or [{"code": codes[0]}],
    )

    async def exercise() -> None:
        async with Client(create_mcp_server()) as client:
            first = await client.call_tool("eltdx_quote", {"codes": "sz000001", "timeout": 3})
            second = await client.call_tool("eltdx_quote", {"codes": "sh600000", "timeout": 3})
            assert first.is_error is False
            assert second.is_error is False
            assert first.structured_content["result"][0]["code"] == "sz000001"
            assert second.structured_content["result"][0]["code"] == "sh600000"
            assert quote_clients[0] == quote_clients[1]

    asyncio.run(exercise())

    connects = [call for call in calls if call[0] == "connect"]
    closes = [call for call in calls if call[0] == "close"]
    assert len(connects) == 1
    assert closes == [("close", connects[0][1])]


def test_mcp_sdk2_reports_bounded_input_errors() -> None:
    Client = _sdk2_client()

    async def exercise() -> None:
        async with Client(create_mcp_server()) as client:
            result = await client.call_tool(
                "eltdx_quote",
                {"codes": [f"sz{index:06d}" for index in range(201)]},
            )
            assert result.is_error is True
            assert "at most 200" in result.content[0].text

    asyncio.run(exercise())
