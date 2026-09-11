"""Opt-in bulk trade contracts, native fixture parity, and request isolation."""
from concurrent.futures import ThreadPoolExecutor
from dataclasses import FrozenInstanceError, replace
from datetime import datetime, timedelta, timezone
from pathlib import Path
from types import SimpleNamespace

import pytest

from eltdx import TradeBatch, to_jsonable
from eltdx import _native_models as models
from eltdx.api.trades import TradeApi
from eltdx.exceptions import ConnectionClosedError, ResponseTimeoutError
from eltdx.models import TradePage
from eltdx.protocol import decode_response
from eltdx.transport import PooledSocketTransport, SocketTransport
from eltdx.transport.pool import PinnedTransportProxy, PoolState
from eltdx.transport.native import native_module
from scripts.fixtures.canonical import to_canonical
from scripts.fixtures.differential import assert_exact, discover_cases


def row(i, *, minute=570, status=0, kind="trade"):
    return (i, i + 10, minute, f"{minute // 60:02}:{minute % 60:02}",
            (2026, 9, 11, minute // 60, minute % 60, 0, 28800),
            12.5 + i / 100, 12500 + i * 10, 100 + i, 3, status,
            "buy" if status == 0 else f"status_{status}", -10, 12500,
            7, 0, "00ff", kind, None, None)


def dto(rows=(), *, start=0, code="sz000001"):
    return ("historical_ticks", (code[:2], 0 if code[:2] == "sz" else 1, code[2:],
            start, 1800, tuple(v for r in rows for v in r), (2026, 9, 11), 12.5, b"raw"))


ROWS = (row(0, minute=565, status=2, kind="opening_match"), row(1),
        row(2, status=8, kind="auction_snapshot"), row(3, minute=905, status=5))


def test_columns_selection_and_explicit_materialization(monkeypatch):
    wire = dto(ROWS)
    expected = models.response_from_dto(wire)
    batch = models.trade_batch_from_dto(wire)
    assert len(batch) == batch.count == 4
    assert batch.has_more and batch.full_code == "sz000001"
    assert len(batch.columns) == 19
    assert batch.to_page() == expected
    assert to_jsonable(batch) == to_jsonable(expected)
    assert batch.tick(-1) == expected.ticks[-1]
    with pytest.raises(FrozenInstanceError):
        batch.code = "000002"
    with pytest.raises(KeyError):
        batch.column("missing")
    for index in (4, -5):
        with pytest.raises(IndexError):
            batch.tick(index)
    with pytest.raises(TypeError):
        batch.select([0.5])

    selected = batch.select([3, 0, -1])
    assert selected.to_page().ticks == (expected.ticks[3], expected.ticks[0], expected.ticks[3])
    assert selected.start == 0 and selected.request_count == 3 and selected.raw_payload == b""
    assert batch.select([]).count == 0
    assert batch.select([]).to_page().ticks == ()
    for name in batch.columns:
        assert batch.column(name) == tuple(getattr(t, name) for t in expected.ticks)

    def forbidden(*args):
        raise AssertionError("bulk access constructed a TradeTick")
    monkeypatch.setattr(models, "_trade_tick_at", forbidden)
    assert batch.to_columns(["price", "volume"])["volume"] == (100, 101, 102, 103)
    assert batch.select([0, 2]).column("event_kind") == ("opening_match", "auction_snapshot")
    assert batch.column("trade_datetime")[0] == expected.ticks[0].trade_datetime


def test_existing_page_conversion_preserves_custom_datetimes():
    original = models.response_from_dto(dto(ROWS))
    dt = datetime(2026, 9, 11, 9, 30, 1, 123456, tzinfo=timezone(timedelta(hours=3)), fold=1)
    page = replace(original, ticks=(replace(original.ticks[0], trade_datetime=dt),))
    batch = TradeBatch.from_page(page)
    assert batch.tick(0).trade_datetime is dt
    assert batch.column("trade_datetime") == (dt,)
    assert batch.to_page() == page


@pytest.mark.parametrize("wire,error", [(('heartbeat', ()), TypeError), (dto(((1, 2),)), ValueError)])
def test_invalid_bulk_dto_is_rejected(wire, error):
    with pytest.raises(error):
        models.trade_batch_from_dto(wire)


class ReplayTransport:
    pool_size = 2
    def __init__(self):
        self.calls = []
    def execute_trade_batch(self, command, payload):
        self.calls.append((command, dict(payload)))
        start = payload['start']
        records = {0: ROWS[3:], 1: ROWS[1:3], 3: ROWS[:1], 4: ()}[start]
        return models.trade_batch_from_dto(dto(records, start=start, code=payload['code']))


def test_today_bulk_and_all_today_bulk_use_native_batch_path():
    transport = ReplayTransport()
    api = TradeApi(transport)
    page = api.today_batch('sz000001', count=10)
    assert isinstance(page, TradeBatch)
    assert page.count == 1
    result = api.all_today_batch(['sz000001', 'sh600487'], page_size=10, max_pages=4)
    assert set(result) == {'sz000001', 'sh600487'}
    assert all(isinstance(value, TradeBatch) for value in result.values())


def test_complete_pagination_merges_blocks_in_order_without_objects(monkeypatch):
    transport = ReplayTransport()
    api = TradeApi(transport)
    with monkeypatch.context() as m:
        m.setattr(models, "_trade_tick_at", lambda *a: pytest.fail("eager tick conversion"))
        result = api.all_history_batch("sz000001", "2026-09-11", include_raw=True)
        assert result.column("volume") == (100, 101, 102, 103)
    assert result.to_page() == replace(models.response_from_dto(dto(ROWS)), request_count=4)
    assert [p['start'] for _, p in transport.calls] == [0, 1, 3, 4]
    assert all(c == 0x0FC6 and p['include_raw'] is True for c, p in transport.calls)
    for method in ('actual_trades', 'auction_snapshots', 'opening_matches', 'after_hours_trades'):
        assert getattr(result.to_page(), method) == getattr(models.response_from_dto(dto(ROWS)), method)


def test_multi_code_bulk_normalization_and_pagination():
    transport = ReplayTransport()
    result = TradeApi(transport).all_history_batch(
        ['000001', 'sh600487', 'sz000001'], '2026-09-11', batch_size=99, max_pages=None,
    )
    assert list(result) == ['sz000001', 'sh600487']
    assert all(isinstance(p, TradeBatch) and p.count == 4 for p in result.values())
    assert len(transport.calls) == 8
    assert TradeApi(transport).history_batch(['sz000001'], '2026-09-11')['sz000001'].count == 1


def test_page_cap_empty_page_and_transport_error():
    api = TradeApi(ReplayTransport())
    with pytest.raises(RuntimeError, match='max_pages'):
        api.all_history_batch('sz000001', '2026-09-11', max_pages=3)
    assert api.all_history_batch('sz000001', '2026-09-11', max_pages=4).count == 4
    api._transport.execute_trade_batch = lambda *args: models.trade_batch_from_dto(dto())
    result = api.all_history_batch('sz000001', '2026-09-11', max_pages=1)
    assert result.count == 0 and result.request_count == 0 and not result.has_more
    def failure(*args):
        raise ResponseTimeoutError('test timeout')
    api._transport.execute_trade_batch = failure
    with pytest.raises(ResponseTimeoutError):
        api.all_history_batch('sz000001', '2026-09-11')


@pytest.mark.parametrize('kwargs', [{'count': 0}, {'count': 1801}, {'batch_size': 0}, {'batch_size': True}])
def test_bulk_page_validation_before_requests(kwargs):
    transport = ReplayTransport()
    with pytest.raises(ValueError):
        TradeApi(transport).history_batch('sz000001', '2026-09-11', **kwargs)
    assert not transport.calls


def test_custom_transport_can_keep_original_execute_contract():
    page = models.response_from_dto(dto(ROWS))
    transport = SimpleNamespace(execute=lambda *a: page)
    assert TradeApi(transport).history_batch('sz000001', '2026-09-11').to_page() == page
    transport.execute = lambda *a: {}
    with pytest.raises(TypeError, match='TradePage'):
        TradeApi(transport).history_batch('sz000001', '2026-09-11')


@pytest.mark.parametrize('transport_cls', [PooledSocketTransport, SocketTransport])
def test_old_and_bulk_requests_can_share_transport_concurrently(transport_cls, monkeypatch):
    kwargs = {'probe_hosts': False} if transport_cls is PooledSocketTransport else {}
    transport = transport_cls(['127.0.0.1:7709'], **kwargs)
    engine = SimpleNamespace(execute=lambda command, payload: dto(ROWS, code=payload['code']))
    monkeypatch.setattr(transport, '_native', lambda: engine)
    api = TradeApi(transport)
    def call(i):
        fn = api.history if i % 2 else api.history_batch
        return fn('sz000001', '2026-09-11')
    with ThreadPoolExecutor(max_workers=8) as executor:
        results = list(executor.map(call, range(40)))
    for i, result in enumerate(results):
        assert isinstance(result, TradePage if i % 2 else TradeBatch)
        page = result if i % 2 else result.to_page()
        assert page == models.response_from_dto(dto(ROWS))
    with pytest.raises(ValueError):
        transport.execute_trade_batch(0x0FC5, {})


def test_pinned_bulk_request_obeys_lifetime():
    engine = SimpleNamespace(execute=lambda *a: dto(ROWS), close=lambda: None)
    pool = SimpleNamespace(diagnostics=SimpleNamespace(epoch=1, state=PoolState.RUNNING))
    pin = PinnedTransportProxy(pool, engine, 1)
    assert pin.execute_trade_batch(0x0FC6, {}).count == 4
    pin.close()
    with pytest.raises(ConnectionClosedError):
        pin.execute_trade_batch(0x0FC6, {})


CASES = [c for c in discover_cases(Path(__file__).parent / 'fixtures' / '7709')
         if c.command == 'historical_ticks' and c.expected_exception is None]


@pytest.mark.parametrize('case', CASES, ids=lambda c: c.case_id)
@pytest.mark.parametrize('include_raw', [False, True])
def test_native_wire_fixture_has_identical_bulk_fields(case, include_raw):
    response = decode_response(case.response_bytes)
    payload = dict(case.request_payload, include_raw=include_raw)
    wire = native_module().parse_command_response(case.command_code, response.data, payload)
    eager = models.response_from_dto(wire)
    batch = models.trade_batch_from_dto(wire)
    assert_exact(to_canonical(eager), to_canonical(batch.to_page()), label='bulk roundtrip')
    assert to_jsonable(eager) == to_jsonable(batch)
    for name in batch.columns:
        assert batch.column(name) == tuple(getattr(t, name) for t in eager.ticks)
