"""Authored trade compatibility contracts; execution is deferred."""
from __future__ import annotations

from datetime import date, datetime

import pytest

from eltdx import _native_models
from eltdx.api.trades import TradeApi
from eltdx.client import TdxClient
from eltdx.models import TradeBatch, TradePage, TradeTick
from eltdx.transport import PooledSocketTransport, SocketTransport


CODE = "sh600001"
DATE = "2026-09-11"


class NativeTrades:
    pool_size = 2

    def __init__(self, result=None):
        self.calls = []
        self.result = object() if result is None else result

    def sdk_trades(self, command, payload):
        self.calls.append(("page", command, payload))
        return self.result

    def sdk_trade_batch(self, command, payload):
        self.calls.append(("batch", command, payload))
        if command != 0x0FC6:
            raise ValueError("trade batch only supports historical ticks")
        return self.result

    def sdk_trade_read(self, command, payload, **options):
        self.calls.append(("read", command, payload, options))
        return self.result

    def execute(self, *_args):
        pytest.fail("ordinary calls must enter the Rust trade service")


@pytest.mark.parametrize("method,args,command,payload", [
    ("today", (CODE,), 0x0FC5, dict(code=CODE, start=7, count=3, include_raw=True)),
    ("history", (CODE, DATE), 0x0FC6,
     dict(code=CODE, trading_date=DATE, start=7, count=3, include_raw=True)),
])
def test_native_trade_single_page_preserves_arguments_and_result(method, args, command, payload):
    transport = NativeTrades()
    api = TradeApi(transport, native_services=True)
    assert getattr(api, method)(*args, start=7, count=3, include_raw=True) is transport.result
    assert transport.calls == [("page", command, payload)]


@pytest.mark.parametrize("method,args,command,opening", [
    ("all_today", (CODE,), 0x0FC5, False),
    ("all_history", (CODE, DATE), 0x0FC6, False),
    ("opening_match_today", (CODE,), 0x0FC5, True),
    ("opening_match_history", (CODE, DATE), 0x0FC6, True),
])
def test_native_trade_collection_and_opening_use_one_shared_call(method, args, command, opening):
    transport = NativeTrades()
    api = TradeApi(transport, native_services=True)
    assert getattr(api, method)(*args, page_size=7, max_pages=5, include_raw=True) is transport.result
    payload = dict(code=CODE, start=0, count=7, include_raw=True)
    if command == 0x0FC6:
        payload["trading_date"] = DATE
    assert transport.calls == [("read", command, payload,
        dict(collect=not opening, opening=opening, max_pages=5))]


@pytest.mark.parametrize("limit,expected", [
    (None, None), (True, 1), (2, 2), (1.25, 2), (float("nan"), None),
    (float("inf"), None), (10**100, None),
])
def test_native_trade_collection_preserves_legacy_numeric_page_limits(limit, expected):
    transport = NativeTrades()
    TradeApi(transport, native_services=True).all_history(CODE, DATE, max_pages=limit)
    assert transport.calls[0][3]["max_pages"] == expected


@pytest.mark.parametrize("value", [date(2026, 9, 11), datetime(2026, 9, 11, 12, 30), 20260911, DATE])
def test_native_trade_collection_accepts_immutable_builtin_dates(value):
    transport = NativeTrades()
    TradeApi(transport, native_services=True).all_history(CODE, value)
    assert transport.calls[0][0] == "read"
    assert transport.calls[0][2]["trading_date"] is value


def test_native_trade_bulk_keeps_single_page_and_collection_routes():
    batch = TradeBatch("sh", 1, "600001", 0, 1800, ((),), date(2026, 9, 11))
    transport = NativeTrades(batch)
    api = TradeApi(transport, native_services=True)
    assert api.history_batch(CODE, DATE) is batch
    assert api.all_history_batch(CODE, DATE) is batch
    assert [item[0] for item in transport.calls] == ["batch", "read"]
    assert transport.calls[1][3] == dict(collect=True, opening=False, max_pages=100, bulk=True)


@pytest.mark.parametrize("method", ["today_batch", "all_today_batch"])
def test_native_trade_today_bulk_retains_existing_rejection(method):
    transport = NativeTrades()
    with pytest.raises(ValueError, match="^trade batch only supports historical ticks$"):
        getattr(TradeApi(transport, native_services=True), method)(CODE)
    assert [call[0] for call in transport.calls] == ["batch"]


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
def test_trade_transport_subclass_retains_custom_execute(transport_type):
    class CustomTransport(transport_type):
        def execute(self, command, payload=None):
            return command, payload

    client = TdxClient(transport=CustomTransport(["127.0.0.1:7709"], heartbeat_interval=None))
    assert not client.trades._native_services
    assert client.trades.today(CODE) == (0x0FC5, dict(code=CODE, start=0, count=1800, include_raw=False))


def _tick(index):
    return TradeTick(index, index, 570, "09:30", None, 10.0, 10000, 1, 2, 0,
                     "buy", 1000, 1000)


@pytest.mark.parametrize("at_class", [False, True])
def test_native_trade_replaced_page_method_keeps_per_page_adapter(monkeypatch, at_class):
    transport = NativeTrades()
    api = TradeApi(transport, native_services=True)
    visited = []

    def page(code, trading_date, **kwargs):
        visited.append(kwargs["start"])
        ticks = (_tick(0), _tick(1)) if kwargs["start"] == 0 else ()
        return TradePage("sh", 1, "600001", kwargs["start"], 1800, ticks,
                         date(2026, 9, 11), 12.5, b"wire")

    if at_class:
        monkeypatch.setattr(TradeApi, "history", lambda self, *a, **kw: page(*a, **kw))
    else:
        api.history = page
    result = api.all_history(CODE, DATE)
    assert visited == [0, 2] and transport.calls == []
    assert result.request_count == 2 and result.start == 0 and result.raw_payload == b""


@pytest.mark.parametrize("custom_field", ["code", "page_size", "date", "include_raw", "max_pages"])
def test_native_trade_custom_arguments_keep_per_page_extraction(custom_field):
    class CustomCode(str):
        pass

    class CustomInt(int):
        pass

    class CustomDate:
        year, month, day = 2026, 9, 11

    class CustomTruth:
        def __bool__(self):
            return True

    arguments = dict(code=CODE, trading_date=DATE, page_size=5, include_raw=False, max_pages=5)
    key, value = {
        "code": ("code", CustomCode(CODE)),
        "page_size": ("page_size", CustomInt(5)),
        "date": ("trading_date", CustomDate()),
        "include_raw": ("include_raw", CustomTruth()),
        "max_pages": ("max_pages", CustomInt(5)),
    }[custom_field]
    arguments[key] = value
    transport = NativeTrades()

    def single(command, payload):
        transport.calls.append((command, payload))
        return TradePage("sh", 1, "600001", payload["start"], 5,
                         (_tick(0),) if payload["start"] == 0 else (), date(2026, 9, 11))

    transport.sdk_trades = single
    result = TradeApi(transport, native_services=True).all_history(**arguments)
    assert result.count == 1
    assert [payload["start"] for _, payload in transport.calls] == [0, 1]


def test_trade_history_none_keeps_row_today_and_bulk_history_distinction():
    transport = NativeTrades()
    api = TradeApi(transport, native_services=True)
    api.all_history(CODE, None)
    api.all_history_batch(CODE, None)
    assert transport.calls[0][1] == 0x0FC5
    assert "trading_date" not in transport.calls[0][2]
    assert transport.calls[1][1] == 0x0FC6
    assert transport.calls[1][2]["trading_date"] is None


def test_native_trade_many_retains_normalization_order_dedup_and_pool_cap(monkeypatch):
    import eltdx.api.trades as module

    capacities = []

    class InlineExecutor:
        def __init__(self, max_workers):
            capacities.append(max_workers)

        def __enter__(self):
            return self

        def __exit__(self, *_args):
            return False

        def map(self, query, chunk):
            return map(query, chunk)

    monkeypatch.setattr(module, "ThreadPoolExecutor", InlineExecutor)
    transport = NativeTrades()
    results = TradeApi(transport, native_services=True).all_today(
        ["600001", CODE, "sh600002"], batch_size=999)
    assert list(results) == [CODE, "sh600002"]
    assert capacities == [2]
    assert [call[2]["code"] for call in transport.calls] == [CODE, "sh600002"]


@pytest.mark.parametrize("kwargs", [dict(page_size=0), dict(page_size=1801), dict(max_pages=0),
                                        dict(batch_size=True), dict(batch_size=0)])
def test_native_trade_invalid_collection_arguments_fail_before_request(kwargs):
    transport = NativeTrades()
    with pytest.raises(ValueError):
        TradeApi(transport, native_services=True).all_history(CODE, DATE, **kwargs)
    assert transport.calls == []


def test_trade_opening_dto_preserves_single_tick_and_none():
    fields = (0, 7, 565, "09:25", (2026, 9, 11, 9, 25, 0, None), 12.5, 12500,
              3, 4, 2, "neutral", 1250, 1250, None, 0, "abcd", "opening_match", None, None)
    result = _native_models.trade_opening_from_dto(("trade_tick", fields))
    assert result.is_opening_match and result.absolute_index == 7
    assert result.trade_datetime == datetime(2026, 9, 11, 9, 25)
    assert result.record_hex == "abcd"
    assert _native_models.trade_opening_from_dto(None) is None
    with pytest.raises(TypeError):
        _native_models.trade_opening_from_dto(("today_ticks", fields))


def test_trade_aggregate_dto_retains_empty_and_nonempty_blocks_without_rows(monkeypatch):
    fields = (0, 7, 565, "09:25", (2026, 9, 11, 9, 25, 0, None), 12.5, 12500,
              3, 4, 2, "neutral", 1250, 1250, None, 0, "abcd", "opening_match", None, None)
    blocks = ((), fields, fields + fields)
    expected = TradeBatch("sh", 1, "600001", 0, 3, blocks, date(2026, 9, 11), 12.5, b"")
    dto = ("historical_trade_batch", ("sh", 1, "600001", 0, 3, blocks, (2026, 9, 11), 12.5, b""))
    monkeypatch.setattr(_native_models, "_trade_tick_at", lambda *_args: pytest.fail("eager tick conversion"))
    result = _native_models.trade_batch_from_dto(dto)
    assert result == expected and result._blocks == blocks and result.count == 3
