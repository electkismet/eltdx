"""History bridge obligations; authored without importing or running tests."""
from types import SimpleNamespace

import pytest

from eltdx.api import money_flow as module


def api_for(response):
    requests = []
    api = module.MoneyFlowApi(SimpleNamespace())
    api._resolve_f10 = lambda: SimpleNamespace(
        params=lambda *args: requests.append(args) or response,
    )
    return api, requests


def test_history_materializes_full_reverse_before_window_and_keeps_row_identity():
    events = []
    first, second, third = ({"N001": value} for value in (9, 1, 7))

    class Rows:
        def __reversed__(self):
            for item in (third, second, first):
                events.append(item)
                yield item

    class Response:
        rows = Rows()

        @property
        def raw(self):
            raise AssertionError("raw accessed for include_raw=False")

    api, requests = api_for(Response())
    result = api.history("sh" + "600001", days=5, start=1, count=1)
    assert len(events) == 3 and all(a is b for a, b in zip(events, (third, second, first)))
    assert result.rows[0] is second
    assert requests == [("CWServ.tdxf10_gg_sj", "zjlx_sj", "600001", "5", "600001")]
    assert result.raw_payload == b""


def test_history_captures_params_and_constructor_but_reloads_split_and_str(monkeypatch):
    events = []
    code, days = object(), object()
    sentinel = object()
    api = module.MoneyFlowApi(SimpleNamespace())

    def forbidden(*args, **kwargs):
        raise AssertionError("already captured callback replaced")

    def captured(*args):
        events.append(("params", args))
        return SimpleNamespace(rows=(), raw={})

    class F10:
        @property
        def params(self):
            events.append("lookup params")
            return captured

    api._resolve_f10 = lambda: F10()
    calls = 0

    def split(value):
        nonlocal calls
        assert value is code
        calls += 1
        events.append(("split", calls))
        if calls == 1:
            monkeypatch.setattr(module, "str", lambda value: events.append(("str", value)) or "DAY", raising=False)
        if calls == 3:
            monkeypatch.setattr(module, "FundFlowRangeSeries", forbidden)
        return None, None, f"code-{calls}"

    def constructor(**kwargs):
        events.append(("model", kwargs))
        return sentinel

    monkeypatch.setattr(module, "split_code", split)
    monkeypatch.setattr(module, "FundFlowRangeSeries", constructor)
    monkeypatch.setattr(module, "F10_HISTORY_ENTRY", "custom.history")
    result = api._single_history(code, days=days, start=0, count=None, include_raw=False)
    assert result is sentinel
    assert events[:5] == ["lookup params", ("split", 1), ("str", days), ("split", 2),
                          ("params", ("custom.history", "zjlx_sj", "code-1", "DAY", "code-2"))]
    assert events[5] == ("split", 3)
    kwargs = events[6][1]
    assert kwargs["code"] is code and kwargs["days"] is days
    assert kwargs["codes"] == ("code-3",) and kwargs["rows"] == ()


def test_history_private_window_keeps_python_add_slice_and_live_tuple(monkeypatch):
    events = []
    projected, stop, count = object(), object(), object()

    class Start:
        def __add__(self, other):
            assert other is count
            events.append("add")
            return stop

    start = Start()

    class Materialized:
        def __getitem__(self, key):
            events.append("slice")
            assert isinstance(key, slice)
            assert key.start is start and key.stop is stop and key.step is None
            return projected

    def custom_tuple(values):
        events.append(("tuple", list(values)))
        return Materialized()

    api, _ = api_for(SimpleNamespace(rows=[1, 2, 3]))
    monkeypatch.setattr(module, "tuple", custom_tuple, raising=False)
    monkeypatch.setattr(module, "FundFlowRangeSeries", lambda **kwargs: kwargs)
    monkeypatch.setattr(module, "slice", lambda *args: pytest.fail("slice expression looked up global slice"), raising=False)
    result = api._single_history("sh" + "600001", days=5, start=start, count=count, include_raw=False)
    assert result["rows"] is projected and result["codes"] == ("600001",)
    assert events == [("tuple", [3, 2, 1]), "add", "slice"]


@pytest.mark.parametrize("start,count,expected", [(-2, None, (2, 1)), (1, 10**100, (2, 1)),
                                               (10**100, None, ()), (0, -1, (3, 2))])
def test_history_private_slice_retains_negative_and_arbitrary_precision_inputs(start, count, expected):
    api, _ = api_for(SimpleNamespace(rows=(1, 2, 3)))
    result = api._single_history("sh" + "600001", days=10, start=start, count=count, include_raw=False)
    assert result.rows == expected


def test_history_raw_dumps_is_captured_before_property_and_encoded_with_original_kwargs(monkeypatch):
    events = []
    raw, payload = object(), object()

    class Encoded:
        def encode(self, encoding):
            events.append(("encode", encoding))
            return payload

    def dumps(value, **kwargs):
        assert value is raw
        events.append(("dumps", kwargs))
        return Encoded()

    json_hook = SimpleNamespace(dumps=dumps)

    class Response:
        rows = ()

        @property
        def raw(self):
            events.append("raw")
            json_hook.dumps = lambda *_args, **_kwargs: pytest.fail("dumps was not captured")
            return raw

    class Include:
        def __bool__(self):
            events.append("truth")
            return True

    api, _ = api_for(Response())
    monkeypatch.setattr(module, "json", json_hook)
    monkeypatch.setattr(module, "FundFlowRangeSeries", lambda **kwargs: kwargs)
    result = api._single_history("sh" + "600001", days=10, start=0, count=None, include_raw=Include())
    assert result["raw_payload"] is payload
    assert events == ["truth", "raw", ("dumps", {"ensure_ascii": False, "separators": (",", ":")}),
                      ("encode", "utf-8")]


@pytest.mark.parametrize("error", [ValueError("broken rows"), KeyboardInterrupt("cancel")])
def test_history_reverse_failure_keeps_root_and_does_not_construct_model(error, monkeypatch):
    class Rows:
        def __reversed__(self):
            yield 1
            raise error

    api, _ = api_for(SimpleNamespace(rows=Rows()))
    monkeypatch.setattr(module, "FundFlowRangeSeries", lambda **kwargs: pytest.fail("partial model"))
    with pytest.raises(type(error)) as actual:
        api._single_history("sh" + "600001", days=10, start=10**100, count=None, include_raw=True)
    assert actual.value is error


def test_history_third_split_failure_occurs_after_raw_encoding(monkeypatch):
    events = []
    root = RuntimeError("third code conversion")
    calls = 0

    def split(code):
        nonlocal calls
        calls += 1
        events.append(("split", calls))
        if calls == 3:
            raise root
        return None, None, "600001"

    def dumps(value, **kwargs):
        events.append("raw")
        return "中文"

    api, _ = api_for(SimpleNamespace(rows=(), raw={}))
    monkeypatch.setattr(module, "split_code", split)
    monkeypatch.setattr(module, "json", SimpleNamespace(dumps=dumps))
    monkeypatch.setattr(module, "FundFlowRangeSeries", lambda **kwargs: pytest.fail("model called"))
    with pytest.raises(RuntimeError) as actual:
        api._single_history("sh" + "600001", days=10, start=0, count=None, include_raw=True)
    assert actual.value is root
    assert events == [("split", 1), ("split", 2), "raw", ("split", 3)]
