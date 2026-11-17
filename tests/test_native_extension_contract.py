"""Extension compatibility source cases; execution awaits the T01–T30 gate."""
from __future__ import annotations

from types import GeneratorType, SimpleNamespace
from datetime import date, datetime
import math

import pytest

from eltdx.mac import bitmap
from eltdx.mac import client as mac
from eltdx.mac import adjust


@pytest.mark.parametrize("text,expected", [
    (" US0401 ", 30401), ("HK0283", 20283), ("000686", 31686),
    ("399372", 30372), ("899050", 32050), ("880686", 20686),
    ("US-7", 29993), ("HK+12", 20012), ("US１２３", 30123),
    ("１２３４５６", 123456), ("-2", -2), ("1" * 100, int("1" * 100)),
])
def test_extension_board_code_keeps_legacy_unicode_signed_and_large_int(text, expected):
    assert mac.convert_board_code(text) == expected


def test_extension_board_code_keeps_strip_slice_and_short_circuit(monkeypatch):
    events = []
    marker = object()

    class Text:
        def startswith(self, prefix):
            events.append(("prefix", prefix))
            return prefix == "US"

        def __getitem__(self, index):
            events.append(("slice", index.start, index.stop, index.step))
            return marker

        def __len__(self):
            pytest.fail("US is selected before len")

    class Input:
        def strip(self):
            events.append("strip")
            return Text()

    def integer(value):
        assert value is marker
        events.append("int")
        return 9

    monkeypatch.setattr(mac, "int", integer, raising=False)
    assert mac.convert_board_code(Input()) == 30009
    assert events == ["strip", ("prefix", "US"), ("slice", 2, None, None), "int"]


def test_extension_board_code_keeps_live_len_and_arithmetic_order(monkeypatch):
    events = []

    class Number:
        def __sub__(self, other):
            events.append(("subtract", other))
            return self

        def __add__(self, other):
            events.append(("add", other))
            return "mapped"

    class Text:
        def startswith(self, prefix):
            events.append(prefix)
            return prefix == "899"

    text = Text()
    monkeypatch.setattr(mac, "len", lambda value: events.append(("len", value)) or 6, raising=False)
    monkeypatch.setattr(mac, "int", lambda value: events.append(("int", value)) or Number(), raising=False)
    assert mac.convert_board_code(SimpleNamespace(strip=lambda: text)) == "mapped"
    assert events == ["US", "HK", ("len", text), "88", "399", "899", ("int", text),
                      ("subtract", 899000), ("add", 32000)]


def test_extension_board_code_preserves_original_exception_identity(monkeypatch):
    failure = ValueError("integer hook")

    def fail(value):
        raise failure

    monkeypatch.setattr(mac, "int", fail, raising=False)
    with pytest.raises(ValueError) as caught:
        mac.convert_board_code("880686")
    assert caught.value is failure
    with pytest.raises(AttributeError):
        mac.convert_board_code(None)


def test_extension_field_selection_keeps_order_aliases_and_objects():
    first = SimpleNamespace(_fields=(bitmap.FieldBit.CLOSE, bitmap.FieldBit.OPEN))
    value = bitmap.FieldSelection(first, bitmap.PresetField.OHLC, bitmap.FieldBit.LIMIT_UP_COUNT,
                                  bitmap.FieldBit.BID2_VOLUME)
    assert tuple(value) == (bitmap.FieldBit.CLOSE, bitmap.FieldBit.OPEN, bitmap.FieldBit.HIGH,
                            bitmap.FieldBit.LOW, bitmap.FieldBit.BID2_VOLUME)
    assert value._fields[0] is bitmap.FieldBit.CLOSE
    assert value._fields[-1] is bitmap.FieldBit.LIMIT_UP_COUNT


def test_extension_field_selection_keeps_live_set_tuple_and_error_order(monkeypatch):
    events = []
    sentinel = object()

    class Seen:
        def __contains__(self, value):
            events.append(("contains", value))
            return False

        def add(self, value):
            events.append(("add", value))

    monkeypatch.setattr(bitmap, "set", lambda: events.append("set") or Seen(), raising=False)
    monkeypatch.setattr(bitmap, "tuple", lambda values: events.append(("tuple", list(values))) or sentinel,
                        raising=False)
    value = bitmap.FieldSelection(bitmap.FieldBit.OPEN, bitmap.FieldBit.OPEN)
    assert value._fields is sentinel
    assert events == ["set", ("contains", bitmap.FieldBit.OPEN), ("add", bitmap.FieldBit.OPEN),
                      ("contains", bitmap.FieldBit.OPEN), ("add", bitmap.FieldBit.OPEN),
                      ("tuple", [bitmap.FieldBit.OPEN, bitmap.FieldBit.OPEN])]


def test_extension_normalize_keeps_instance_identity_and_star_length_hint(monkeypatch):
    value = bitmap.FieldSelection(bitmap.FieldBit.OPEN)
    assert bitmap.normalize_fields(value) is value
    events = []

    class Source:
        def __iter__(self):
            events.append("iter")
            return iter((bitmap.FieldBit.OPEN, bitmap.FieldBit.CLOSE))

        def __len__(self):
            events.append("len")
            return 2

    class Selection:
        def __init__(self, *parts):
            events.append(("construct", parts))
            self.parts = parts

    monkeypatch.setattr(bitmap, "FieldSelection", Selection)
    monkeypatch.setattr(bitmap, "tuple", lambda value: pytest.fail("star expansion has no global tuple lookup"),
                        raising=False)
    result = bitmap.normalize_fields(Source())
    assert result.parts == (bitmap.FieldBit.OPEN, bitmap.FieldBit.CLOSE)
    assert events == ["iter", "len", ("construct", result.parts)]
    events.clear()
    assert bitmap.normalize_fields(bitmap.PresetField.OHLC).parts == bitmap.PresetField.OHLC.value
    assert events == [("construct", bitmap.PresetField.OHLC.value)]


def test_extension_normalize_iteration_failure_precedes_constructor(monkeypatch):
    failure = RuntimeError("iterator failed")
    events = []

    class Source:
        def __iter__(self):
            events.append("iter")
            raise failure

    class Selection:
        def __init__(self, *parts):
            pytest.fail("constructor cannot run before star expansion")

    monkeypatch.setattr(bitmap, "FieldSelection", Selection)
    with pytest.raises(RuntimeError) as caught:
        bitmap.normalize_fields(Source())
    assert caught.value is failure
    assert events == ["iter"]


def test_extension_bitmap_unknown_slots_reserved_bits_and_arbitrary_width():
    source = (1 << 47) | (1 << 48) | (1 << 150) | (1 << 151) | (1 << 159) | (1 << 300)
    raw = source.to_bytes(40, "little")
    assert list(bitmap.iter_set_bits(raw)) == [
        (0, None, None), (1, bitmap.FieldBit.PE_TTM, "<f"),
        (2, bitmap.FieldBit.CHANGE_AT_1430, "<f")]
    assert bitmap.count_set_bits(raw) == 3
    assert bitmap.get_active_fields(raw) == [
        (bitmap.FieldBit.PE_TTM, "<f"), (bitmap.FieldBit.CHANGE_AT_1430, "<f")]
    assert list(bitmap.iter_set_bits(b"")) == []
    assert list(bitmap.iter_set_bits(b"\x02")) == [(0, bitmap.FieldBit.OPEN, "<f")]


def test_extension_bitmap_is_lazy_and_live_registry_is_resolved_between_yields(monkeypatch):
    events = []

    class Integer:
        @staticmethod
        def from_bytes(value, byteorder):
            events.append((value, byteorder))
            return (1 << 1) | (1 << 4)

    monkeypatch.setattr(bitmap, "int", Integer, raising=False)
    value = bitmap.iter_set_bits(b"input")
    assert isinstance(value, GeneratorType)
    assert events == []
    assert next(value) == (0, bitmap.FieldBit.OPEN, "<f")
    assert events == [(b"input", "little")]
    monkeypatch.setitem(bitmap.FieldBit._value2member_map_, 4, bitmap.FieldBit.LOW)
    assert next(value) == (1, bitmap.FieldBit.LOW, "<f")
    with pytest.raises(StopIteration):
        next(value)


def test_extension_bitmap_limit_remains_dynamic_and_not_fixed_at_151(monkeypatch):
    monkeypatch.setattr(bitmap, "FIELD_BIT_LIMIT", 401)
    assert list(bitmap.iter_set_bits((1 << 400).to_bytes(51, "little"))) == [(0, None, None)]
    monkeypatch.setattr(bitmap, "FIELD_BIT_LIMIT", 0)
    assert bitmap.count_set_bits(b"\xff" * 100) == 0


def test_extension_bitmap_field_bits_keeps_mask_before_live_int_conversion(monkeypatch):
    events = []

    class Limit:
        def __rlshift__(self, left):
            events.append(("shift", left))
            return 8

    class Integer:
        @staticmethod
        def from_bytes(value, order):
            events.append(("bytes", value, order))
            return 255

    monkeypatch.setattr(bitmap, "FIELD_BIT_LIMIT", Limit())
    monkeypatch.setattr(bitmap, "int", Integer, raising=False)
    assert bitmap._field_bits(b"wire") == 7
    assert events == [("shift", 1), ("bytes", b"wire", "little")]


def test_extension_bitmap_custom_numeric_result_keeps_inplace_xor(monkeypatch):
    events = []

    class Number:
        live = True

        def __bool__(self):
            events.append("bool")
            return self.live

        def __neg__(self):
            events.append("neg")
            return "negative"

        def __and__(self, other):
            events.append(("and", other))
            return 2

        def __ixor__(self, other):
            events.append(("ixor", other))
            self.live = False
            return self

        def __xor__(self, other):
            pytest.fail("the legacy loop uses inplace xor")

    monkeypatch.setattr(bitmap, "_field_bits", lambda value: Number())
    assert list(bitmap.iter_set_bits(object())) == [(0, bitmap.FieldBit.OPEN, "<f")]
    assert events == ["bool", "neg", ("and", "negative"), ("ixor", 2), "bool"]


def test_extension_bitmap_negative_override_retains_lazy_lowbit_semantics(monkeypatch):
    monkeypatch.setattr(bitmap, "_field_bits", lambda value: -1)
    value = bitmap.iter_set_bits(None)
    assert [next(value) for _ in range(3)] == [
        (0, bitmap.FieldBit.PRE_CLOSE, "<f"), (1, bitmap.FieldBit.OPEN, "<f"),
        (2, bitmap.FieldBit.HIGH, "<f")]
    value.close()
    with pytest.raises(StopIteration):
        next(value)


def test_extension_bitmap_error_retires_generator_with_original_exception(monkeypatch):
    failure = LookupError("bitmap conversion")

    def fail(value):
        raise failure

    monkeypatch.setattr(bitmap, "_field_bits", fail)
    value = bitmap.iter_set_bits(b"wire")
    with pytest.raises(LookupError) as caught:
        next(value)
    assert caught.value is failure
    monkeypatch.setattr(bitmap, "_field_bits", lambda value: 1)
    with pytest.raises(StopIteration):
        next(value)


def test_extension_bitmap_count_and_active_fields_keep_live_helpers(monkeypatch):
    marker = object()
    monkeypatch.setattr(bitmap, "_field_bits", lambda value: SimpleNamespace(bit_count=lambda: marker))
    assert bitmap.count_set_bits(b"wire") is marker
    field, fmt = object(), object()
    monkeypatch.setattr(bitmap, "iter_set_bits", lambda value: iter(((9, None, fmt), (14, field, fmt))))
    value = bitmap.get_active_fields(b"wire")
    assert len(value) == 1
    assert value[0][0] is field
    assert value[0][1] is fmt


def test_extension_bitmap_builder_keeps_layout_debug_type_and_exact_errors():
    result = bitmap.build_bitmap([bitmap.FieldBit.OPEN, bitmap.FieldBit.CHANGE_AT_1430])
    assert type(result) is bytearray
    assert result == bytearray((1 << 1 | 1 << 150).to_bytes(20, "little"))
    assert bitmap.build_bitmap(bitmap.PresetField.DEBUG, width=16) == bytearray(b"\xff" * 16)
    assert bitmap.build_bitmap(None, width=True) == bytearray(b"\0")
    with pytest.raises(ValueError) as caught:
        bitmap.build_bitmap([bitmap.FieldBit.BID3_PRICE], width=16)
    assert str(caught.value) == "字段 BID3_PRICE(0x80) 超出 16 字节位图（最多 128 位，即 0..127）"
    with pytest.raises(ValueError) as caught:
        bitmap.build_bitmap(object(), width=0)
    assert str(caught.value) == "width must be between 1 and 20, got 0"
    with pytest.raises(TypeError):
        bitmap.build_bitmap(None, width=1.5)


def test_extension_bitmap_builder_retains_dynamic_normalizer_and_unbounded_custom_width(monkeypatch):
    marker = object()
    reads = []

    class Field:
        @property
        def value(self):
            reads.append("value")
            return 200

    def normalize(value):
        assert value is marker
        return [Field()]

    monkeypatch.setattr(bitmap, "BITMAP_SIZE", 32)
    monkeypatch.setattr(bitmap, "normalize_fields", normalize)
    assert bitmap.build_bitmap(marker, width=32) == bytearray((1 << 200).to_bytes(32, "little"))
    assert reads == ["value", "value"]


def test_extension_bitmap_builder_keeps_custom_shift_inplace_or_and_captured_bytearray(monkeypatch):
    events = []
    result = object()

    class Accumulator:
        def __ror__(self, prior):
            events.append(("or", prior))
            return self

        def __ior__(self, other):
            events.append(("ior", other is self))
            return self

        def to_bytes(self, width, order):
            events.append(("bytes", width, order))
            monkeypatch.setattr(bitmap, "bytearray", lambda value: pytest.fail("resolved before to_bytes"))
            return b"custom"

    accumulator = Accumulator()

    class Bit:
        def __ge__(self, limit):
            events.append(("ge", limit))
            return False

        def __rlshift__(self, one):
            events.append(("shift", one))
            return accumulator

    monkeypatch.setattr(bitmap, "normalize_fields", lambda value: [SimpleNamespace(value=Bit())] * 2)
    monkeypatch.setattr(bitmap, "bytearray", lambda value: events.append(("output", value)) or result, raising=False)
    assert bitmap.build_bitmap(None) is result
    assert events == [("ge", 160), ("shift", 1), ("or", 0), ("ge", 160),
                      ("shift", 1), ("ior", True), ("bytes", 20, "little"), ("output", b"custom")]


def test_extension_exclude_flags_keeps_object_to_bytes_and_live_size(monkeypatch):
    marker = object()
    size = object()
    events = []

    class Flags:
        def to_bytes(self, length, order):
            events.append((length, order))
            return marker

    monkeypatch.setattr(bitmap, "CONTROL_SIZE", size)
    assert bitmap.build_exclude_flags(Flags()) is marker
    assert events == [(size, "little")]
    monkeypatch.setattr(bitmap, "CONTROL_SIZE", 4)
    with pytest.raises(OverflowError):
        bitmap.build_exclude_flags(-1)


def test_extension_ah_postprocessor_keeps_zero_membership_signed_and_live_str(monkeypatch):
    assert bitmap._post_ah_code(0, object()) == ""
    assert bitmap._post_ah_code(123, True) == "00123"
    assert bitmap._post_ah_code(-12, 31) == "-00012"
    events = []
    monkeypatch.setattr(bitmap, "str", lambda value: events.append(value) or "custom", raising=False)
    marker = object()
    assert bitmap._post_ah_code(marker, 0) == "custom"
    assert events == [marker]


@pytest.mark.parametrize("values", [
    (0.0, 0.0, 0.0, 0.0, 0.0), (-1.0, 0.0, 0.0, 0.0, 0.0),
    (1.0, 0.0, 0.0, -1.0, 0.0), (float("inf"), 0.0, 0.0, 0.0, 0.0),
    (1.0, float("nan"), 0.0, 0.0, 0.0),
])
def test_extension_adjust_factor_retains_nan_result_policy(values):
    assert math.isnan(adjust.compute_forward_factor(*values))


def test_extension_adjust_factor_keeps_numeric_protocol_and_denominator_first(monkeypatch):
    events = []

    class Number:
        def __init__(self, name):
            self.name = name

        def __le__(self, other):
            events.append(("le", self.name, other))
            return False

        def __eq__(self, other):
            events.append(("eq", self.name, other))
            return False

        def __radd__(self, other):
            events.append(("radd", self.name, other))
            return Number("stock-total")

        def __add__(self, other):
            events.append(("add", self.name, other.name))
            return Number("sum")

        def __mul__(self, other):
            events.append(("mul", self.name, other.name))
            return Number("product")

        def __sub__(self, other):
            events.append(("sub", self.name, other.name))
            return Number("difference")

        def __truediv__(self, other):
            events.append(("divide", self.name, other.name))
            return Number("factor")

        def __float__(self):
            events.append(("float", self.name))
            return 0.75

    monkeypatch.setattr(adjust, "math", SimpleNamespace(
        isfinite=lambda value: events.append(("finite", value.name)) or True))
    assert adjust.compute_forward_factor(*(Number(name) for name in ("close", "cash", "price", "stock", "rights"))) == 0.75
    assert events == [("le", "close", 0), ("radd", "stock", 1.0), ("add", "stock-total", "rights"),
                      ("mul", "close", "sum"), ("eq", "product", 0), ("sub", "close", "cash"),
                      ("mul", "price", "rights"), ("add", "difference", "product"),
                      ("divide", "sum", "product"), ("finite", "factor"), ("float", "factor")]


def _extension_bar(day, value, marker=0):
    return adjust.MacBar(datetime(2026, 11, day), value, value, value, value, marker, -0.0, 123.0)


def test_extension_adjust_no_events_is_sorted_shallow_copy_and_skips_event_scan_on_empty():
    first, second = _extension_bar(1, 10), _extension_bar(2, 20)
    source = [second, first]
    result = adjust.apply_forward_adjust(source, [])
    assert result == [first, second]
    assert result is not source
    assert result[0] is first and result[1] is second

    class Events:
        def __iter__(self):
            pytest.fail("empty bars do not inspect events")

    assert adjust.apply_forward_adjust([], Events()) == []


def test_extension_adjust_sequential_same_date_events_use_already_adjusted_close():
    source = [_extension_bar(3, 30, 3), _extension_bar(1, 8, 1),
              _extension_bar(2, 20, 2), _extension_bar(1, 10, 4)]
    events = [adjust.XdxrEvent(date(2026, 11, 3), fenhong=1),
              adjust.XdxrEvent(date(2026, 11, 2), fenhong=1, songzhuangu=1),
              adjust.XdxrEvent(date(2026, 11, 2), fenhong=1)]
    result = adjust.apply_forward_adjust(source, events)
    assert [bar.close for bar in result] == pytest.approx([2.66, 3.325, 19, 30])
    assert [bar.vol for bar in result] == [1, 4, 2, 3]
    assert all(bar.float_shares == 123 and math.copysign(1, bar.amount) == -1 for bar in result)
    assert all(result[index] is not source[old] for index, old in enumerate([1, 3, 2, 0]))
    assert [bar.close for bar in source] == [30, 8, 20, 10]


def test_extension_adjust_nonempty_all_skipped_still_constructs_original_dto():
    source = [_extension_bar(1, 10), _extension_bar(2, 20)]
    # Selected event precedes every bar, so no factor exists but output DTOs
    # are still newly constructed. Category/zero events alone take shallow copy.
    result = adjust.apply_forward_adjust(source, [adjust.XdxrEvent(date(2026, 10, 1), fenhong=1)])
    assert result == source
    assert all(a is not b for a, b in zip(result, source))
    result = adjust.apply_forward_adjust(source, [adjust.XdxrEvent(date(2026, 11, 2), category=2, fenhong=1)])
    assert all(a is b for a, b in zip(result, source))


def test_extension_adjust_keeps_live_factor_range_inplace_order_and_model(monkeypatch):
    events = []

    class Price:
        def __init__(self, label):
            self.label = label

        def __imul__(self, factor):
            events.append((self.label, factor))
            return self

    source = [SimpleNamespace(datetime=datetime(2026, 11, day),
                              **{field: Price(f"{field}{day}") for field in ("open", "high", "low", "close")},
                              vol=0, amount=0, float_shares=0) for day in (1, 2, 3)]
    monkeypatch.setattr(adjust, "compute_forward_factor", lambda *values: 2.0)
    monkeypatch.setattr(adjust, "range", lambda count: events.append(("range", count)) or (1, 0), raising=False)
    monkeypatch.setattr(adjust, "MacBar", lambda **values: values)
    result = adjust.apply_forward_adjust(source, [adjust.XdxrEvent(date(2026, 11, 3), fenhong=1)])
    assert events == [("range", 2)] + [(f"{field}{day}", 2.0) for day in (2, 1) for field in ("open", "high", "low", "close")]
    assert len(result) == 3
    assert result[0]["open"] is source[0].open


def test_extension_bad_prices_reads_full_tuple_before_finite_short_circuit(monkeypatch):
    events = []

    class Bar:
        def __getattr__(self, field):
            events.append(field)
            return field

    monkeypatch.setattr(adjust, "math", SimpleNamespace(isfinite=lambda value: events.append(("finite", value)) or False))
    assert adjust.has_bad_prices([Bar()])
    assert events == ["open", "high", "low", "close", ("finite", "open")]
    assert not adjust.has_bad_prices([])


def test_extension_board_code_captures_int_before_custom_slice(monkeypatch):
    class Text:
        def strip(self):
            return self

        def startswith(self, prefix):
            return prefix == "US"

        def __getitem__(self, index):
            monkeypatch.setattr(mac, "int", lambda value: 99)
            return "7"

    monkeypatch.setattr(mac, "int", lambda value: 7, raising=False)
    assert mac.convert_board_code(Text()) == 30007


def test_extension_bitmap_rereads_field_class_after_custom_registry_get(monkeypatch):
    class After:
        fmt = "custom"

    marker = After()

    class Registry:
        def get(self, position):
            assert position == 1
            monkeypatch.setattr(bitmap, "FieldBit", After)
            return marker

    class Before:
        _value2member_map_ = Registry()

    monkeypatch.setattr(bitmap, "FieldBit", Before)
    assert list(bitmap.iter_set_bits(b"\x02")) == [(0, marker, "custom")]
