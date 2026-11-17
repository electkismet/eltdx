"""Batch adapter contracts, authored for the final unified validation phase."""
from types import SimpleNamespace

import pytest

from eltdx.api import bars, corporate, delisted_bars, minutes, money_flow, trades
from eltdx.models import CapitalChangeBatch, CapitalChangeBlock, MoneyFlowBatch, MoneyFlowBlock
from eltdx.transport.native import native_module

CODE = "sh" + str(600_001)
OTHER = "sh" + str(600_002)
THIRD = "sh" + str(600_003)


class Executor:
    def __init__(self, events, max_workers=1):
        self.events = events
        events.append(("workers", max_workers))

    def __enter__(self):
        self.events.append("enter")
        return self

    def __exit__(self, kind, value, traceback):
        self.events.append(("exit", kind))

    def map(self, query, codes):
        self.events.append(("map", tuple(codes)))
        return map(query, codes)


def install_executor(monkeypatch, module, events):
    monkeypatch.setattr(module, "ThreadPoolExecutor",
                        lambda *, max_workers: Executor(events, max_workers))


def capital(code=CODE, raw=b""):
    return CapitalChangeBlock(code[:2], 1, code[2:], 0, (), raw)


def flow(code=CODE):
    return MoneyFlowBlock(code[:2], 1, code[2:], ())


def test_map_batches_keeps_chunk_order_and_last_duplicate_value():
    events = []
    values = iter([object(), object(), object()])
    observed = []

    def query(code):
        value = next(values)
        observed.append((code, value))
        return value

    result = native_module().map_batches(Executor(events), [CODE, OTHER, CODE], query, 2)
    assert list(result) == [CODE, OTHER]
    assert result[CODE] is observed[2][1] and result[OTHER] is observed[1][1]
    assert events == [("workers", 1), ("map", (CODE, OTHER)), ("map", (CODE,))]


def test_map_batches_keeps_short_zip_without_inventing_missing_rows():
    class ShortExecutor:
        def map(self, query, codes):
            return iter([codes[0]])

    assert native_module().map_batches(ShortExecutor(), [CODE, OTHER, THIRD], None, 2) == {
        CODE: CODE, THIRD: THIRD,
    }


def test_map_batches_none_submits_all_codes_in_one_call():
    events = []
    codes = [CODE, OTHER, THIRD]
    result = native_module().map_batches(Executor(events), codes, lambda code: code, None)
    assert result == dict(zip(codes, codes))
    assert events[-1] == ("map", tuple(codes))


def test_batch_chunks_preserves_python_slicing_for_size_subclasses():
    events = []

    class Size(int):
        def __radd__(self, other):
            events.append(other)
            return other + int(self)

    assert native_module().batch_chunks([CODE, OTHER, THIRD], Size(2)) == [[CODE, OTHER], [THIRD]]
    assert events == [0, 2]


def test_capital_assembly_keeps_original_blocks_raw_order_and_custom_response():
    first, second = capital(raw=b"first"), capital(OTHER)
    result = native_module().assemble_capital(
        [first, CapitalChangeBatch((second,), (b"second", b"extra"))],
        CapitalChangeBlock, CapitalChangeBatch,
    )
    assert result.blocks[0] is first and result.blocks[1] is second
    assert result.raw_payloads == (b"first", b"second", b"extra")
    custom = object()
    assert native_module().assemble_capital([first, custom, second],
                                            CapitalChangeBlock, CapitalChangeBatch) is custom


def test_capital_assembly_reads_truthy_raw_property_twice():
    calls = []

    class Block:
        @property
        def raw_payload(self):
            calls.append("raw")
            return b"check" if len(calls) == 1 else b"stored"

    result = native_module().assemble_capital([Block()], Block, CapitalChangeBatch)
    assert calls == ["raw", "raw"] and result.raw_payloads == (b"stored",)


def test_capital_batch_queries_all_chunks_before_custom_result_and_assembly(monkeypatch):
    events = []
    install_executor(monkeypatch, corporate, events)
    api = corporate.CorporateApi(SimpleNamespace(pool_size=2))
    custom = object()

    def query(codes, *, include_raw):
        events.append(("query", tuple(codes), include_raw))
        return custom if codes == [CODE] else capital(codes[0])

    monkeypatch.setattr(api, "_capital_changes_chunk", query)
    assert api.capital_changes([CODE, OTHER, THIRD], batch_size=1, include_raw=True) is custom
    assert [event for event in events if isinstance(event, tuple) and event[0] == "query"] == [
        ("query", (CODE,), True), ("query", (OTHER,), True), ("query", (THIRD,), True),
    ]
    assert events[-1] == ("exit", None)


def test_capital_single_chunk_never_creates_executor(monkeypatch):
    monkeypatch.setattr(corporate, "ThreadPoolExecutor", lambda **_: pytest.fail("unneeded executor"))
    api = corporate.CorporateApi(SimpleNamespace(pool_size=2))
    item = capital()
    monkeypatch.setattr(api, "_capital_changes_chunk", lambda *_, **__: item)
    assert api.capital_changes([CODE]).blocks == (item,)


@pytest.mark.parametrize("failure", [RuntimeError("query"), KeyboardInterrupt()])
def test_map_batches_preserves_exception_identity_and_context_exit(failure):
    events = []

    def query(code):
        raise failure

    with pytest.raises(type(failure)) as raised:
        with Executor(events) as executor:
            native_module().map_batches(executor, [CODE, OTHER], query, 1)
    assert raised.value is failure
    assert events[-1] == ("exit", type(failure))
    assert ("map", (OTHER,)) not in events


def test_money_flow_custom_response_stops_later_submissions_and_exits(monkeypatch):
    events = []
    install_executor(monkeypatch, money_flow, events)
    custom = object()
    api = money_flow.MoneyFlowApi(SimpleNamespace(pool_size=1))
    monkeypatch.setattr(api, "_single", lambda code, **kw: events.append(("query", code)) or custom)
    assert api.daily([CODE, OTHER]) is custom
    assert events == [("workers", 1), "enter", ("map", (CODE,)), ("query", CODE), ("exit", None)]


def test_money_flow_batch_constructs_after_shutdown_and_shares_failover(monkeypatch):
    events, failovers = [], []
    install_executor(monkeypatch, money_flow, events)

    class Batch:
        def __init__(self, blocks):
            events.append("construct")
            self.blocks = blocks

    monkeypatch.setattr(money_flow, "MoneyFlowBatch", Batch)
    api = money_flow.MoneyFlowApi(SimpleNamespace(pool_size=2))

    def query(code, *, include_raw, failover):
        failovers.append(failover)
        return flow(code)

    monkeypatch.setattr(api, "_single", query)
    result = api.daily([CODE, OTHER, THIRD], include_raw=True, batch_size=2)
    assert tuple(item.full_code for item in result.blocks) == (CODE, OTHER, THIRD)
    assert events[-2:] == [("exit", None), "construct"]
    assert all(value is failovers[0] for value in failovers)
    assert failovers[0].probes_left == 9


def test_money_flow_bridge_flattens_mixed_blocks_without_losing_identity():
    first, second = flow(), flow(OTHER)
    values = iter([first, MoneyFlowBatch((second,))])
    custom, blocks = native_module().map_money_flow(
        Executor([]), [CODE, OTHER], lambda code: next(values), 1, MoneyFlowBlock, MoneyFlowBatch,
    )
    assert custom is False and blocks[0] is first and blocks[1] is second


@pytest.mark.parametrize("module,kind", [(bars, "BarApi"), (delisted_bars, "DelistedBarApi")])
def test_bar_batch_keeps_user_scalar_override_and_worker_bound(module, kind, monkeypatch):
    events = []
    install_executor(monkeypatch, module, events)
    base = getattr(module, kind)

    class CustomApi(base):
        def get(self, code, **kwargs):
            if isinstance(code, str):
                return code
            return super().get(code, **kwargs)

    result = CustomApi(SimpleNamespace(pool_size=2)).get([CODE, OTHER, THIRD], batch_size=2)
    assert result == {CODE: CODE, OTHER: OTHER, THIRD: THIRD}
    assert events == [("workers", 2), "enter", ("map", (CODE, OTHER)), ("map", (THIRD,)), ("exit", None)]


def test_minute_and_trade_keep_distinct_submission_patterns(monkeypatch):
    for module, api_type, all_at_once in [(minutes, minutes.MinuteApi, True), (trades, trades.TradeApi, False)]:
        events = []
        install_executor(monkeypatch, module, events)
        api = api_type(SimpleNamespace(pool_size=2))
        result = api._run_many([CODE, OTHER, THIRD, CODE], lambda code: code, 2)
        assert result == {CODE: CODE, OTHER: OTHER, THIRD: THIRD}
        calls = [event for event in events if isinstance(event, tuple) and event[0] == "map"]
        assert calls == ([("map", (CODE, OTHER, THIRD))] if all_at_once else
                         [("map", (CODE, OTHER)), ("map", (THIRD,))])


def test_adjustment_batch_keeps_builder_hook_keywords_and_custom_passthrough(monkeypatch):
    first, second = capital(), capital(OTHER)
    calls = []
    api = corporate.CorporateApi(SimpleNamespace())
    monkeypatch.setattr(api, "capital_changes", lambda *_, **__: CapitalChangeBatch((first, second)))

    def build(block, *, anchor_date, start_date):
        calls.append((block, anchor_date, start_date))
        return block

    monkeypatch.setattr(corporate, "build_adjustment_factor_response", build)
    result = api.adjustment_factors([CODE, OTHER], "anchor", start_date="start")
    assert result.responses == (first, second)
    assert calls == [(first, "anchor", "start"), (second, "anchor", "start")]
    custom = object()
    monkeypatch.setattr(api, "capital_changes", lambda *_, **__: custom)
    assert api.adjustment_factors(CODE) is custom
