"""EX command/binary compatibility sources; execute after the full source gate."""
from __future__ import annotations

import struct
import sys
from types import SimpleNamespace

import pytest

from eltdx.exceptions import ProtocolError
from eltdx.ex import _binary as ex_binary, datetime_ as dates
from eltdx.mac import _binary as mac_binary
from eltdx.ex.commands import get_history_bars_range as ranges
from eltdx.ex.commands import get_instrument_bars as bars
from eltdx.ex.commands import get_instrument_count as counts
from eltdx.ex.commands import get_instrument_info as info
from eltdx.ex.commands import get_instrument_quote as quote
from eltdx.ex.commands import get_instrument_quote_list as quotes
from eltdx.ex.commands import get_markets as markets
from eltdx.ex.commands import get_minute_time as minutes
from eltdx.ex.commands import get_transaction as trades
from eltdx.ex.commands import login


@pytest.mark.parametrize("constructor,args,header,payload", [
    (markets.GetExMarketsCmd, (), "01 02 48 69 00 01 02 00 02 00 f4 23", b""),
    (counts.GetExInstrumentCountCmd, (), "01 03 48 66 00 01 02 00 02 00 f0 23", b""),
    (info.GetExInstrumentInfoCmd, (123, 99), "01 04 48 67 00 01 08 00 08 00 f5 23", struct.pack("<IH", 123, 99)),
    (quote.GetExInstrumentQuoteCmd, (31, "ABCDEFGHIJK"), "01 01 08 02 02 01 0c 00 0c 00 fa 23", struct.pack("<B9s", 31, b"ABCDEFGHIJK")),
    (quotes.GetExInstrumentQuoteListCmd, (31, 2, 9, 7), "01 c1 06 0b 00 02 0b 00 0b 00 00 24", struct.pack("<BHHHH", 31, 0, 9, 7, 1)),
    (bars.GetExInstrumentBarsCmd, (8, 31, "00700", 3, 4), "01 01 08 6a 01 01 16 00 16 00 ff 23", struct.pack("<B9sHHIH", 31, b"00700", 8, 1, 3, 4)),
    (minutes.GetExMinuteTimeDataCmd, (31, "00700"), "01 07 08 00 01 01 0c 00 0c 00 0b 24", struct.pack("<B9s", 31, b"00700")),
    (minutes.GetExHistoryMinuteTimeDataCmd, (31, "00700", 20261117), "01 01 30 00 01 01 10 00 10 00 0c 24", struct.pack("<IB9s", 20261117, 31, b"00700")),
    (trades.GetExTransactionDataCmd, (31, "00700", 4, 5), "01 01 08 00 03 01 12 00 12 00 fc 23", struct.pack("<B9siH", 31, b"00700", 4, 5)),
    (trades.GetExHistoryTransactionDataCmd, (31, "00700", 20261117, 4, 5), "01 01 30 00 02 01 16 00 16 00 06 24", struct.pack("<IB9siH", 20261117, 31, b"00700", 4, 5)),
])
def test_native_ex_request_headers_and_payloads_match_source_literals(constructor, args, header, payload):
    assert constructor(*args).build_request() == bytes.fromhex(header) + payload


def test_native_ex_range_request_keeps_class_sequence_wrap_and_exact_body(monkeypatch):
    cls = ranges.GetExHistoryInstrumentBarsRangeCmd
    monkeypatch.setattr(cls, "_seqid", 255)
    first = cls(31, "00700", 20261101, 20261117)
    second = cls(31, "00700", 20261101, 20261117)
    expected_tail = bytes.fromhex("38 92 00 01 16 00 16 00 0D 24")
    expected_tail += struct.pack("<B9sHII", 31, b"00700", 7, 20261101, 20261117)
    assert first.build_request() == b"\x01\xff" + expected_tail
    assert second.build_request() == b"\x01\x01" + expected_tail
    assert cls._seqid == 2


def test_native_ex_range_sequence_uses_live_type_again_after_addition(monkeypatch):
    cls = ranges.GetExHistoryInstrumentBarsRangeCmd
    first_class = SimpleNamespace()
    second_class = SimpleNamespace()
    classes = iter((first_class, second_class))

    class Seq:
        def __lt__(self, other):
            assert other == 255
            return True

        def __add__(self, other):
            assert other == 1
            return 47

    old = Seq()
    first_class._seqid = old
    monkeypatch.setattr(ranges, "type", lambda value: next(classes), raising=False)
    command = cls(31, "x", 0, 0)
    assert command._next_seqid() is old
    assert second_class._seqid == 47 and first_class._seqid is old


def test_native_ex_code_and_page_validation_precede_encoding():
    events = []

    class Code:
        def __bool__(self):
            events.append("bool")
            return False

        def encode(self, name):
            pytest.fail("invalid code is not encoded")

    class Start:
        def __lt__(self, other):
            pytest.fail("code is checked first")

    with pytest.raises(ValueError, match="code 不能为空"):
        bars.GetExInstrumentBarsCmd(8, 31, Code(), Start(), 0)
    assert events == ["bool"]


def test_native_ex_category_validation_precedes_page_and_preserves_format(monkeypatch):
    events = []
    failure = ValueError("custom category")

    class Category:
        def __eq__(self, other):
            events.append(("eq", other))
            return False

        def __format__(self, spec):
            events.append(("format", spec))
            return "category-value"

    class Start:
        def __lt__(self, other):
            pytest.fail("unsupported category returns before page validation")

    monkeypatch.setattr(quotes, "ValueError", lambda message: events.append(message) or failure, raising=False)
    with pytest.raises(ValueError) as caught:
        quotes.GetExInstrumentQuoteListCmd(31, Category(), Start(), 0)
    assert caught.value is failure
    assert events == [("eq", 2), ("eq", 3), ("format", ""), "不支持的扩展行情类别: category-value"]


def test_native_ex_page_errors_capture_constructor_before_format_side_effect(monkeypatch):
    original = ValueError("first constructor")
    events = []

    class Start:
        def __lt__(self, other):
            return True

        def __format__(self, spec):
            monkeypatch.setattr(info, "ValueError", lambda message: pytest.fail("constructor already captured"), raising=False)
            return "negative-start"

    monkeypatch.setattr(info, "ValueError", lambda message: events.append(message) or original, raising=False)
    with pytest.raises(ValueError) as caught:
        info.GetExInstrumentInfoCmd(Start(), object())
    assert caught.value is original
    assert events == ["start 必须 >= 0，当前: negative-start"]


def test_native_ex_constructor_keeps_assignment_encoding_and_object_identity():
    events = []
    encoded = object()
    category, market, start, count = object(), object(), 2, 3

    class Code:
        def encode(self, encoding):
            events.append(("encode", encoding))
            return encoded

    class Command(bars.GetExInstrumentBarsCmd):
        def __setattr__(self, name, value):
            events.append((name, value))
            super().__setattr__(name, value)

    command = Command(category, market, Code(), start, count)
    assert events == [("category", category), ("market", market), ("encode", "utf-8"),
                      ("code", encoded), ("start", start), ("count", count)]
    assert command.code is encoded and command.market is market


def test_native_ex_constructor_does_not_add_strict_wire_narrowing():
    command = trades.GetExTransactionDataCmd(999, "abcdefghijk", 2**80, 2**80)
    assert command.start == 2**80 and command.count == 2**80
    with pytest.raises(struct.error):
        command.build_request()
    # Only the original builder encodes fields; Unicode still uses UTF-8.
    assert quote.GetExInstrumentQuoteCmd(31, "中文代码").code == "中文代码".encode("utf-8")


def test_native_ex_request_captures_pack_before_attribute_side_effect(monkeypatch):
    events = []
    original_pack = lambda *args: events.append(args) or b"payload"

    class Command(quote.GetExInstrumentQuoteCmd):
        @property
        def market(self):
            monkeypatch.setattr(quote, "struct", SimpleNamespace(pack=lambda *args: pytest.fail("pack was captured")))
            return 31

        @market.setter
        def market(self, value):
            pass

    command = Command(31, "00700")
    monkeypatch.setattr(quote, "struct", SimpleNamespace(pack=original_pack))
    result = command.build_request()
    assert result == bytes.fromhex("01 01 08 02 02 01 0c 00 0c 00 fa 23") + b"payload"
    assert events == [("<B9s", 31, b"00700")]


def test_native_ex_range_builder_keeps_overridden_sequence_hook():
    class Command(ranges.GetExHistoryInstrumentBarsRangeCmd):
        def _next_seqid(self):
            return 17

    command = Command(31, "x", 0, 0)
    assert command.build_request()[1] == 17


def test_native_mac_ex_login_keeps_live_constants_and_legacy_reply_heuristic(monkeypatch):
    monkeypatch.setattr(login, "_LOGIN_BODY", b"abc")
    monkeypatch.setattr(login, "_MSG_ID", 0x1234)
    monkeypatch.setattr(login, "_HEAD_FLAG", 3)
    command = login.MacExLoginCmd()
    inner = b"\x34\x12abc"
    assert command.build_request() == struct.pack("<BIBHH", 3, 0, 1, 5, 5) + inner
    assert [command.parse_response(value) for value in (b"", b"x", b"xx", b"rejected")] == [False, False, True, True]


def test_native_mac_ex_login_reads_len_twice_and_retains_comparison_result(monkeypatch):
    calls = []
    marker = object()

    class Length:
        def __ge__(self, other):
            calls.append(("ge", other))
            return marker

    monkeypatch.setattr(login, "_LOGIN_BODY", b"")
    monkeypatch.setattr(login, "len", lambda value: calls.append(value) or Length(), raising=False)
    assert login.MacExLoginCmd().parse_response(b"x") is marker
    assert calls == [b"x", ("ge", 2)]
    calls.clear()
    sizes = iter((7, 9))
    monkeypatch.setattr(login, "len", lambda value: calls.append(value) or next(sizes))
    result = login.MacExLoginCmd().build_request()
    assert struct.unpack("<BIBHH", result[:10])[-2:] == (7, 9)
    assert len(calls) == 2 and calls[0] == calls[1]


@pytest.mark.parametrize("binary", [ex_binary, mac_binary])
def test_native_extension_binary_boundaries_and_original_messages(binary):
    assert binary.require_bytes(b"abc", 1, 2, "ctx") is None
    with pytest.raises(ProtocolError, match="ctx: 非法偏移 -1"):
        binary.require_bytes(object(), -1, 9, "ctx")
    with pytest.raises(ProtocolError) as caught:
        binary.require_bytes(b"abc", 5, 2, "ctx")
    assert str(caught.value) == "ctx: 数据不足，需要 2 字节，偏移 5，实际剩余 0 字节"
    # The legacy guard did not reject a negative size on its own.
    assert binary.require_bytes(b"abc", 1, -1, "ctx") is None


@pytest.mark.parametrize("binary", [ex_binary, mac_binary])
def test_native_extension_binary_reloads_len_and_captures_max_before_second_len(monkeypatch, binary):
    events = []
    count = 0

    def maximum(left, right):
        events.append(("max", left, right))
        return max(left, right)

    def length(data):
        nonlocal count
        count += 1
        events.append(("len", count))
        if count == 2:
            monkeypatch.setattr(binary, "max", lambda *args: pytest.fail("max already captured"), raising=False)
        return 1 if count == 1 else 3

    monkeypatch.setattr(binary, "len", length, raising=False)
    monkeypatch.setattr(binary, "max", maximum, raising=False)
    with pytest.raises(ProtocolError) as caught:
        binary.require_bytes(object(), 2, 7, "ctx")
    assert events == [("len", 1), ("len", 2), ("max", 1, 0)]
    assert str(caught.value).endswith("实际剩余 1 字节")


@pytest.mark.parametrize("binary", [ex_binary, mac_binary])
def test_native_extension_unpack_captures_guard_before_calcsize_and_leaves_it_outside_try(monkeypatch, binary):
    failure = struct.error("guard failure")
    events = []

    def guard(*args):
        events.append(args)
        raise failure

    def calcsize(fmt):
        monkeypatch.setattr(binary, "require_bytes", lambda *args: pytest.fail("guard already captured"))
        return 17

    monkeypatch.setattr(binary, "require_bytes", guard)
    monkeypatch.setattr(binary, "struct", SimpleNamespace(calcsize=calcsize, error=struct.error,
                                                        unpack_from=lambda *args: pytest.fail("guard failed")))
    with pytest.raises(struct.error) as caught:
        binary.unpack_from("format", b"data", 3, "ctx")
    assert caught.value is failure
    assert events == [(b"data", 3, 17, "ctx")]


@pytest.mark.parametrize("binary", [ex_binary, mac_binary])
def test_native_extension_unpack_error_keeps_active_exception_cause_and_hooks(monkeypatch, binary):
    source = struct.error("original struct failure")
    outer = ProtocolError("outer")
    events = []

    def fail(*args):
        raise source

    def wrap(message):
        events.append((message, sys.exc_info()[1]))
        return outer

    monkeypatch.setattr(binary, "require_bytes", lambda *args: None)
    monkeypatch.setattr(binary, "struct", SimpleNamespace(calcsize=lambda fmt: 4, unpack_from=fail, error=struct.error))
    monkeypatch.setattr(binary, "ProtocolError", wrap)
    with pytest.raises(ProtocolError) as caught:
        binary.unpack_from("<I", b"", 0, "ctx")
    assert caught.value is outer and outer.__cause__ is source and outer.__context__ is source
    assert events == [("ctx: 解析失败: original struct failure", source)]


@pytest.mark.parametrize("binary", [ex_binary, mac_binary])
def test_native_extension_unpack_retains_non_struct_exception_identity(monkeypatch, binary):
    failure = RuntimeError("buffer object")

    def fail(*args):
        raise failure

    monkeypatch.setattr(binary, "require_bytes", lambda *args: None)
    monkeypatch.setattr(binary, "struct", SimpleNamespace(calcsize=lambda fmt: 4, unpack_from=fail, error=struct.error))
    with pytest.raises(RuntimeError) as caught:
        binary.unpack_from("<I", b"", 0, "ctx")
    assert caught.value is failure


@pytest.mark.parametrize("binary", [ex_binary, mac_binary])
def test_native_extension_slice_captures_bytes_before_add_and_keeps_original_slice(monkeypatch, binary):
    marker = object()
    events = []

    class Start:
        def __add__(self, size):
            events.append(("add", size))
            monkeypatch.setattr(binary, "bytes", lambda value: pytest.fail("bytes already captured"), raising=False)
            return 7

    start = Start()

    class Data:
        def __getitem__(self, key):
            assert key.start is start and key.stop == 7 and key.step is None
            events.append("slice")
            return marker

    monkeypatch.setattr(binary, "require_bytes", lambda *args: events.append("guard"))
    monkeypatch.setattr(binary, "bytes", lambda value: events.append(("bytes", value)) or value, raising=False)
    assert binary.slice_bytes(Data(), start, 4, "ctx") is marker
    assert events == ["guard", ("add", 4), "slice", ("bytes", marker)]


def test_native_ex_date_helpers_keep_nonvalidated_calendar_and_clock_values():
    packed = (2026 - 2004) * 2048 + 1117
    assert dates.get_datetime_minute(struct.pack("<HH", packed, 1441), 0) == (2026, 11, 17, 24, 1, 4)
    assert dates.get_datetime_day(struct.pack("<I", 20261399), 0) == (2026, 13, 99, 4)
    assert dates.get_datetime(4, struct.pack("<I", 20261399), 0) == (2026, 13, 99, 15, 0, 4)
    assert ranges.GetExHistoryInstrumentBarsRangeCmd._parse_date(-1) == (2003, 20, 47)
    assert ranges.GetExHistoryInstrumentBarsRangeCmd._parse_time(-1) == (-1, 59)


def test_native_ex_auto_datetime_keeps_live_hooks_short_circuit_and_return_identity(monkeypatch):
    marker = object()
    calls = []
    monkeypatch.setattr(dates, "get_datetime_minute", lambda *args: calls.append(args) or marker)
    monkeypatch.setattr(dates, "get_datetime_day", lambda *args: (2026, 11, 17, marker))

    class Category:
        def __lt__(self, other):
            return True

        def __eq__(self, other):
            pytest.fail("minute comparison short-circuits membership")

    assert dates.get_datetime(Category(), b"x", 9) is marker
    assert dates.get_datetime(7, b"x", 9) is marker
    assert dates.get_datetime(8, b"x", 9) is marker
    assert dates.get_datetime(9, b"x", 9) == (2026, 11, 17, 15, 0, marker)
    assert calls == [(b"x", 9)] * 3


def test_native_ex_datetime_keeps_arbitrary_size_and_day_unpack_arity(monkeypatch):
    raw = 10**90 + 20261117
    monkeypatch.setattr(dates, "unpack_from", lambda *args: (raw,))
    assert dates.get_datetime_day(b"", 4) == (raw // 10000, (raw % 10000) // 100, raw % 100, 8)
    monkeypatch.setattr(dates, "get_datetime_day", lambda *args: (1, 2, 3))
    with pytest.raises(ValueError, match="not enough values to unpack \\(expected 4, got 3\\)"):
        dates.get_datetime(4, b"", 0)
    monkeypatch.setattr(dates, "get_datetime_day", lambda *args: (1, 2, 3, 4, 5))
    with pytest.raises(ValueError, match="too many values to unpack \\(expected 4\\)"):
        dates.get_datetime(4, b"", 0)


def test_native_ex_datetime_custom_numeric_operators_keep_shift_divide_and_order(monkeypatch):
    events = []

    class Raw:
        def __rshift__(self, bits):
            events.append(("shift", bits))
            return 22

        def __mod__(self, divisor):
            events.append(("mod", divisor))
            return 1117

        def __floordiv__(self, divisor):
            events.append(("divide", divisor))
            return 22

    raw = Raw()
    monkeypatch.setattr(dates, "unpack_from", lambda *args: (raw, 61))
    assert dates.get_datetime_minute(b"", 0) == (2026, 11, 17, 1, 1, 4)
    assert events == [("shift", 11), ("mod", 2048), ("mod", 2048)]
    events.clear()
    assert ranges.GetExHistoryInstrumentBarsRangeCmd._parse_date(raw) == (2026, 11, 17)
    assert events == [("divide", 2048), ("mod", 2048), ("mod", 2048)]
