"""Extension compatibility source cases; execution awaits the T01–T30 gate."""
from __future__ import annotations

from types import GeneratorType, SimpleNamespace

import pytest

from eltdx.mac import bitmap
from eltdx.mac import client as mac


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
