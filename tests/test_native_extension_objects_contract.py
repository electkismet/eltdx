"""Python object compatibility sources; execute only after the full source gate."""
from __future__ import annotations

from dataclasses import FrozenInstanceError, fields, replace
from datetime import date, datetime, time
import pickle
from types import SimpleNamespace

import pytest

from eltdx import ex, mac
from eltdx.ex import enums as ex_enums, frame as ex_frame, mac_commands, mac_models, models as ex_models
from eltdx.ex.transport.sync import ExHostProbe
from eltdx.mac import adjust, bitmap, codec as mac_codec, enums as mac_enums, models as mac_models_impl


@pytest.mark.parametrize("owner", [bitmap.PresetField.OHLC, bitmap.FieldSelection(bitmap.FieldBit.OPEN)])
def test_extension_field_operators_keep_forward_and_reflected_order(owner):
    own = owner.value if isinstance(owner, bitmap.PresetField) else tuple(owner)
    before = bitmap.FieldSelection(bitmap.FieldBit.PRE_CLOSE, *own)
    after = bitmap.FieldSelection(*own, bitmap.FieldBit.PRE_CLOSE)
    assert tuple(owner + bitmap.FieldBit.PRE_CLOSE) == tuple(after)
    assert tuple(owner | bitmap.FieldBit.PRE_CLOSE) == tuple(after)
    assert tuple(bitmap.FieldBit.PRE_CLOSE + owner) == tuple(before)
    assert tuple(bitmap.FieldBit.PRE_CLOSE | owner) == tuple(before)


@pytest.mark.parametrize("owner,method,argument", [
    (bitmap.PresetField.OHLC, "__add__", object()),
    (bitmap.PresetField.OHLC, "__radd__", bitmap.PresetField.BASIC),
    (bitmap.FieldSelection(), "__add__", object()),
    (bitmap.FieldSelection(), "__radd__", bitmap.FieldSelection()),
])
def test_extension_field_unsupported_direct_operators_return_not_implemented(owner, method, argument):
    assert getattr(owner, method)(argument) is NotImplemented


def test_extension_field_bit_with_bit_keeps_integer_arithmetic():
    # FieldBit remains an IntEnum; only a selection/preset triggers composition.
    assert bitmap.FieldBit.OPEN + bitmap.FieldBit.HIGH == 3
    assert bitmap.FieldBit.OPEN | bitmap.FieldBit.HIGH == 3
    assert type(bitmap.FieldBit.OPEN + bitmap.FieldBit.HIGH) is int


def test_extension_field_operator_observes_live_instance_hook_and_constructor(monkeypatch):
    owner = bitmap.PresetField.OHLC
    argument, result = object(), object()
    events = []

    def accepted(value, choices):
        events.append(("isinstance", value, choices))
        return True

    def construct(*parts):
        events.append(("construct", parts))
        return result

    # Resolve the union before the isinstance call, then fetch the constructor
    # again: an accepted hook is allowed to replace it while it runs.
    original_selection = bitmap.FieldSelection

    def replace_constructor(value, choices):
        accepted(value, choices)
        monkeypatch.setattr(bitmap, "FieldSelection", construct)
        return True

    monkeypatch.setattr(bitmap, "isinstance", replace_constructor, raising=False)
    assert owner.__add__(argument) is result
    assert events == [
        ("isinstance", argument, bitmap.FieldBit | bitmap.PresetField | original_selection),
        ("construct", (owner, argument)),
    ]


def test_extension_field_rejected_operator_does_not_construct(monkeypatch):
    owner, argument = bitmap.PresetField.OHLC, object()
    events = []
    monkeypatch.setattr(bitmap, "isinstance", lambda value, choices: events.append(value) or False, raising=False)
    assert owner.__add__(argument) is NotImplemented
    assert events == [argument]


@pytest.mark.parametrize("kind", [bitmap.PresetField, bitmap.FieldSelection])
def test_extension_field_or_keeps_live_add_method_overrides(monkeypatch, kind):
    owner = kind.OHLC if kind is bitmap.PresetField else kind()
    marker, argument = object(), object()
    seen = []
    monkeypatch.setattr(kind, "__add__", lambda self, other: seen.append((self, other)) or marker)
    assert owner.__or__(argument) is marker
    assert seen == [(owner, argument)]
    monkeypatch.setattr(kind, "__radd__", lambda self, other: seen.append((other, self)) or marker)
    assert owner.__ror__(argument) is marker
    assert seen[-1] == (argument, owner)


def test_extension_field_iteration_length_truth_and_membership_keep_live_storage():
    value = bitmap.FieldSelection(bitmap.FieldBit.HIGH, bitmap.FieldBit.OPEN, bitmap.FieldBit.HIGH)
    assert tuple(value) == (bitmap.FieldBit.HIGH, bitmap.FieldBit.OPEN)
    assert len(value) == 2 and bool(value)
    assert bitmap.FieldBit.HIGH in value and bitmap.FieldBit.CLOSE not in value
    assert repr(value) == "FieldSelection([HIGH, OPEN])"
    assert repr(bitmap.FieldSelection()) == "FieldSelection([])"
    value._fields = ()
    assert not value and len(value) == 0 and tuple(value) == ()


def test_extension_field_value_helpers_keep_dynamic_builtin_hooks(monkeypatch):
    value = bitmap.FieldSelection()
    storage, iterator = object(), iter(())
    value._fields = storage
    calls = []
    monkeypatch.setattr(bitmap, "iter", lambda item: calls.append(("iter", item)) or iterator, raising=False)
    monkeypatch.setattr(bitmap, "len", lambda item: calls.append(("len", item)) or 9, raising=False)
    monkeypatch.setattr(bitmap, "bool", lambda item: calls.append(("bool", item)) or True, raising=False)
    assert value.__iter__() is iterator
    assert value.__len__() == 9
    assert value.__bool__() is True
    assert calls == [("iter", storage), ("len", storage), ("bool", storage)]


def test_extension_field_membership_preserves_container_callback_and_exception():
    value = bitmap.FieldSelection()
    item, failure = object(), LookupError("contains callback")
    calls = []

    class Storage:
        def __contains__(self, candidate):
            calls.append(candidate)
            raise failure

    value._fields = Storage()
    with pytest.raises(LookupError) as raised:
        item in value
    assert raised.value is failure and calls == [item]


def test_extension_field_repr_reads_names_in_order_and_keeps_join_type_error():
    value = bitmap.FieldSelection()
    calls = []

    class Bit:
        def __init__(self, name):
            self.text = name

        @property
        def name(self):
            calls.append(self.text)
            return self.text

    value._fields = (Bit("FIRST"), Bit("SECOND"))
    assert repr(value) == "FieldSelection([FIRST, SECOND])"
    assert calls == ["FIRST", "SECOND"]
    value._fields = (Bit(1), Bit("LAST"))
    with pytest.raises(TypeError, match="expected str instance"):
        repr(value)
    assert calls[-2:] == [1, "LAST"]


def test_extension_field_name_calls_the_original_name_lower_method():
    marker = object()
    value = SimpleNamespace(name=SimpleNamespace(lower=lambda: marker))
    assert bitmap.FieldBit.field_name.fget(value) is marker


@pytest.mark.parametrize("alias,canonical,number", [
    ("LIMIT_UP_COUNT", "BID2_VOLUME", 93),
    ("LIMIT_DOWN_COUNT", "ASK2_VOLUME", 94),
    ("UP_COUNT", "BID5_VOLUME", 136),
    ("DOWN_COUNT", "ASK5_VOLUME", 139),
])
def test_extension_field_aliases_keep_canonical_identity_and_pickle(alias, canonical, number):
    item = getattr(bitmap.FieldBit, alias)
    assert item is getattr(bitmap.FieldBit, canonical)
    assert item.value == number and item.field_name == canonical.lower()
    assert item.fmt == "<I"
    assert pickle.loads(pickle.dumps(item)) is item


def test_extension_default_fields_and_presets_keep_source_order():
    assert tuple(map(int, mac_commands.DEFAULT_QUOTE_FIELDS)) == (0, 1, 2, 3, 4, 5, 7)
    assert tuple(map(int, bitmap.PresetField.BASIC.value)) == (1, 2, 3, 4, 0, 5)
    assert tuple(map(int, bitmap.PresetField.VOLUME.value)) == (5, 7, 27, 6)
    assert tuple(map(int, bitmap.PresetField.BOARD_STATS.value)) == (93, 94, 136, 139)
    assert bitmap.PresetField.ALL.value == tuple(bitmap.FieldBit)
    assert bitmap.PresetField.DEBUG.value == (-1, "", "调试用全字段")


def test_extension_ex_models_keep_fields_slots_raw_and_original_module():
    specifications = {
        "ExMarketInfo": "market category name short_name",
        "ExInstrumentInfo": "category market code name desc",
        "ExInstrumentQuote": "market code pre_close open high low price kaicang zongliang xianliang neipan waipan chicang bid1 bid2 bid3 bid4 bid5 bid_vol1 bid_vol2 bid_vol3 bid_vol4 bid_vol5 ask1 ask2 ask3 ask4 ask5 ask_vol1 ask_vol2 ask_vol3 ask_vol4 ask_vol5",
        "ExInstrumentBar": "open high low close position trade amount year month day hour minute",
        "ExMinuteBar": "hour minute price avg_price volume open_interest",
        "ExTransactionRecord": "hour minute second price volume zengcang nature",
    }
    for name, names in specifications.items():
        model = getattr(ex_models, name)
        assert model.__module__ == "eltdx.ex.models"
        assert tuple(field.name for field in fields(model)) == (*names.split(), "_raw")
        value = model(*range(len(names.split())))
        assert value._raw == b"" and "_raw" not in repr(value)
        assert replace(value, _raw=b"wire") == value
        assert not hasattr(value, "__dict__")
        with pytest.raises(FrozenInstanceError):
            value._raw = b"changed"
        assert pickle.loads(pickle.dumps(value)) == value


def test_extension_mac_models_keep_field_layouts_and_shared_reexports():
    specifications = {
        "MacQuoteField": "market code name fields",
        "MacSymbolInfo": "market code name time activity pre_close open high low close momentum vol amount inside_volume outside_volume turnover avg",
        "MacBar": "datetime open high low close vol amount float_shares",
        "MacTick": "time price avg vol momentum",
        "MacTickChart": "market code name pre_close open high low close vol amount turnover avg charts",
        "MacMultiTickDay": "date pre_close ticks",
        "MacMultiTickChart": "market code name pre_close open high low close vol amount turnover avg charts",
        "MacTransaction": "time price vol trade_count bs_flag",
        "BoardInfo": "market code name price rise_speed pre_close symbol_market symbol_code symbol_name symbol_price symbol_rise_speed symbol_pre_close",
        "AuctionItem": "time price matched unmatched",
        "UnusualItem": "index market code name time desc value unusual_type",
        "CapitalFlowData": "date main_in main_out main_net small_in small_out small_net mid_in mid_out mid_net large_in large_out large_net",
        "BelongBoardInfo": "board_type market board_code board_name close pre_close",
        "ServerSession": "today last_trading_day sessions_1 sessions_2 market_param_1 market_param_2",
        "KlineOffsetInfo": "total returned",
        "GoodsItem": "name category u index switch code c1 c2",
        "FileMeta": "offset size flag hash",
    }
    assert set(mac_models_impl.__all__) == set(specifications)
    for name, names in specifications.items():
        model = getattr(mac_models_impl, name)
        assert tuple(field.name for field in fields(model)) == tuple(names.split())
        assert model.__module__ == "eltdx.mac.models" and getattr(mac, name) is model
    for name in mac_models.__all__:
        assert getattr(mac_models, name) is getattr(mac_models_impl, name)
    from eltdx.mac import commands

    for name in mac_commands.__all__:
        if name == "DEFAULT_QUOTE_FIELDS":
            continue
        origin = mac_enums if name in ("Adjust", "Period") else commands
        assert getattr(mac_commands, name) is getattr(origin, name)


def test_extension_mutable_model_defaults_are_independent_and_remain_mutable():
    one = mac_models_impl.MacQuoteField(1, "600000", "name")
    two = mac_models_impl.MacQuoteField(1, "600000", "name")
    one.fields["close"] = 10
    assert two.fields == {} and one.fields == {"close": 10}
    first = mac_models_impl.ServerSession("20260101", "20251231")
    second = mac_models_impl.ServerSession("20260101", "20251231")
    first.sessions_1.append({"start": 930})
    assert second.sessions_1 == [] and first.sessions_2 == []
    for value in [one, first]:
        assert not hasattr(value, "__dict__")
        assert pickle.loads(pickle.dumps(value)) == value
    with pytest.raises(FrozenInstanceError):
        one.code = "changed"


def test_extension_time_objects_and_adjustment_defaults_remain_python_values():
    tick = mac_models_impl.MacTick(time(9, 30, 5), 10, 9, 2)
    bar = mac_models_impl.MacBar(datetime(2026, 1, 2, 9, 30), 1, 2, 1, 2, 3, 4)
    event = adjust.XdxrEvent(date(2026, 1, 2))
    assert type(tick.time) is time and tick.momentum == 0.0
    assert type(bar.datetime) is datetime and bar.float_shares == 0.0
    assert type(event.date) is date
    assert (event.category, event.fenhong, event.peigujia, event.songzhuangu, event.peigu) == (1, 0.0, 0.0, 0.0, 0.0)
    assert pickle.loads(pickle.dumps(event)) == event


def test_extension_frame_and_probe_models_keep_original_types_and_defaults():
    for model in (ex_frame.FrameHeader, mac_codec.MacFrameHeader):
        assert tuple(item.name for item in fields(model)) == ("magic", "seq_id", "method", "zipsize", "unzipsize")
        header = model(7654321, 4, 5, 6, 7)
        assert not hasattr(header, "__dict__")
        assert pickle.loads(pickle.dumps(header)) == header
    probe = ExHostProbe("host", True)
    assert (probe.latency_ms, probe.error) == (None, None)
    assert tuple(item.name for item in fields(ExHostProbe)) == ("host", "ok", "latency_ms", "error")
    assert not hasattr(probe, "__dict__")
    with pytest.raises(FrozenInstanceError):
        probe.ok = False


def test_extension_market_enums_keep_distinct_classes_and_matching_values():
    assert ex.ExMarket is ex_enums.ExMarket
    assert mac.ExMarket is mac_enums.ExMarket
    assert ex.ExMarket is not mac.ExMarket
    assert {name: int(item) for name, item in ex.ExMarket.__members__.items()} == {
        name: int(item) for name, item in mac.ExMarket.__members__.items()
    }
    assert int(ex.ExMarket.HK_MAIN_BOARD) == 31
    assert int(mac.Period.SECONDS) == 13 and int(mac.SortType.OPEN_SNATCH_PCT) == 0x10A
    assert (int(mac.Adjust.NONE), int(mac.Adjust.QFQ), int(mac.Adjust.HFQ)) == (0, 1, 2)


def test_extension_package_exports_keep_lazy_mac_ex_class_and_attribute_error():
    from eltdx.ex.mac_client import MacExClient

    assert ex.MacExClient is MacExClient
    assert ex.__getattr__("MacExClient") is MacExClient
    with pytest.raises(AttributeError) as raised:
        ex.__getattr__("missing_object")
    assert str(raised.value) == "module 'eltdx.ex' has no attribute 'missing_object'"
    for name in ex.__all__:
        assert getattr(ex, name) is not None
    for name in mac.__all__:
        assert getattr(mac, name) is not None
