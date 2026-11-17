"""MAC request compatibility sources; run only after the complete source gate."""
from __future__ import annotations

import struct
from datetime import date
from types import SimpleNamespace

import pytest

from eltdx.mac import commands as mac


@pytest.mark.parametrize("constructor,args,kwargs,msg_id,fmt,values", [
    (mac.SymbolBarCmd, (47, "IFL0", 4, 3, 700, 1), {"times": 7}, 0x122E,
     "<H22sHHIHHbbbbH4s", (47, b"IFL0", 4, 7, 3, 700, 1, 1, 1, 0, 1, 0, b"")),
    (mac.SymbolTickChartCmd, (1, "600000", date(2026, 11, 17)), {}, 0x122D,
     "<H22sI5H", (1, b"600000", 20261117, 1, 0, 0, 0, 0)),
    (mac.TickChartsCmd, (1, "600000", None, 3), {}, 0x123E,
     "<H22sIHH6x", (1, b"600000", 0, 3, 1)),
    (mac.SymbolTransactionCmd, (1, "600000", None, 4, 9), {}, 0x122F,
     "<H22sIIH10x", (1, b"600000", 0, 4, 9)),
    (mac.SymbolInfoCmd, (1, "600000"), {}, 0x122A,
     "<H22sI12x", (1, b"600000", 1)),
    (mac.BoardListCmd, (3, 9, 150), {}, 0x1231,
     "<HHBBHH8x", (150, 3, 0, 0, 9, 1)),
    (mac.SymbolBelongBoardCmd, (1, "600000"), {}, 0x1218,
     "<H8s16x21s", (1, b"600000", b"Stock_GLHQ")),
    (mac.SymbolCapitalFlowCmd, (1, "600000"), {}, 0x1218,
     "<H8s16x21s", (1, b"600000", b"Stock_ZJLX")),
    (mac.SymbolAuctionCmd, (1, "600000", 4, 500), {}, 0x123D,
     "<H22sII10x", (1, b"600000", 4, 500)),
    (mac.UnusualCmd, (1, 8, 999), {}, 0x1237,
     "<HH2xH2xH5H", (1, 8, 600, 1, 200, 30, 40, 50, 200)),
    (mac.KlineOffsetCmd, (3, 128000), {}, 0x124A,
     "<II5x", (3, 128000)),
    (mac.ChartSamplingCmd, (47, "IFL0"), {}, 0x254D,
     "<H22sHH9x", (47, b"IFL0", 1, 20)),
    (mac.GoodsListCmd, (47, 7, 600), {}, 0x2562,
     "<HII", (47, 7, 600)),
    (mac.FileListCmd, ("中文.dat", 7), {}, 0x1215,
     "<I70s30x", (7, "中文.dat".encode("gbk"))),
    (mac.FileDownloadCmd, ("中文.dat", 2, 7, 30000), {}, 0x1217,
     "<III70s30x", (2, 7, 30000, "中文.dat".encode("gbk"))),
])
def test_native_mac_request_exact_source_layout(constructor, args, kwargs, msg_id, fmt, values):
    command = constructor(*args, **kwargs)
    body = struct.pack(fmt, *values)
    head_flag = 1 if constructor is mac.SymbolBelongBoardCmd else 2 if constructor is mac.SymbolCapitalFlowCmd else 0x1C
    expected = struct.pack("<BIBHHH", head_flag, 0, 1, len(body) + 2, len(body) + 2, msg_id)
    assert command.build_request() == expected + body


def test_native_mac_quotes_keep_original_list_and_do_not_add_rust_page_cap(monkeypatch):
    stocks = [(1, "中" * 20)] * 81
    fields = object()
    seen = []
    monkeypatch.setattr(mac, "build_bitmap", lambda selected: seen.append(selected) or b"bitmap")
    monkeypatch.setattr(mac, "build_mac_request", lambda ident, body: (ident, body))
    command = mac.SymbolQuotesCmd(stocks, fields)
    assert command._stocks is stocks and command._fields is fields
    assert seen == [fields]
    assert command.build_request() == (0x122B, b"bitmap" + struct.pack("<H", 81)
                                       + struct.pack("<H22s", 1, ("中" * 20).encode("gbk")) * 81)


def test_native_mac_quotes_none_default_differs_from_false_fields(monkeypatch):
    default = object()
    monkeypatch.setattr(mac, "PresetField", SimpleNamespace(COMMON=default))
    monkeypatch.setattr(mac, "build_bitmap", lambda fields: b"")
    assert mac.SymbolQuotesCmd([(1, "x")])._fields is default
    fields = []
    assert mac.SymbolQuotesCmd([(1, "x")], fields)._fields is fields
    with pytest.raises(ValueError, match="stocks 不能为空"):
        mac.SymbolQuotesCmd([])


def test_native_mac_quote_constructor_captures_bytes_before_bitmap_hook(monkeypatch):
    encoded = object()
    events = []
    monkeypatch.setattr(mac, "bytes", lambda value: events.append(value) or encoded, raising=False)

    def bitmap(fields):
        monkeypatch.setattr(mac, "bytes", lambda value: pytest.fail("bytes callable already captured"))
        return b"bitmap"

    monkeypatch.setattr(mac, "build_bitmap", bitmap)
    assert mac.SymbolQuotesCmd([(1, "x")])._bitmap is encoded
    assert events == [b"bitmap"]


def test_native_mac_members_layout_keeps_summed_flags_and_live_bitmap_width(monkeypatch):
    fields = object()
    flags = [SimpleNamespace(value=1), SimpleNamespace(value=1), SimpleNamespace(value=8)]
    seen = []
    monkeypatch.setattr(mac, "BOARD_MEMBERS_BITMAP_SIZE", 3)
    monkeypatch.setattr(mac, "build_bitmap", lambda selected, *, width: seen.append((selected, width)) or b"abc")
    monkeypatch.setattr(mac, "build_mac_request", lambda ident, body: (ident, body))
    command = mac.BoardMembersQuotesCmd(123, 14, 7, 80, 2, fields, flags)
    assert command._fields is fields and command._exclude_flags is flags
    assert command.build_request() == (0x122C, struct.pack("<I9xHIHBB", 123, 14, 7, 80, 2, 0)
                                       + b"abc" + bytes((0, 10, 0, 1)))
    assert seen == [(fields, 3)]


def test_native_mac_members_false_flags_become_new_empty_list():
    flags = []
    command = mac.BoardMembersQuotesCmd(123, exclude_flags=flags)
    assert command._exclude_flags == [] and command._exclude_flags is not flags


def test_native_mac_bar_positional_arguments_and_assignment_order():
    events = []

    class Command(mac.SymbolBarCmd):
        def __setattr__(self, name, value):
            events.append((name, value))
            super().__setattr__(name, value)

    command = Command(47, "IFL0", 4, 0, 800, 2, times=9)
    assert events == [("_market", 47), ("_code", "IFL0"), ("_period", 4),
                      ("_times", 9), ("_start", 0), ("_count", 800), ("_fq", 2)]
    assert command._start == 0 and command._count == 800
    with pytest.raises(TypeError):
        Command(47, "IFL0", 4, 0, 800, 2, 9)


@pytest.mark.parametrize("constructor", [mac.SymbolTickChartCmd, mac.TickChartsCmd, mac.SymbolTransactionCmd])
def test_native_mac_date_constructor_keeps_numeric_protocol_and_no_calendar_validation(constructor):
    events = []

    class Date:
        @property
        def year(self):
            events.append("year")
            return 2026

        @property
        def month(self):
            events.append("month")
            return 19

        @property
        def day(self):
            events.append("day")
            return 99

    command = constructor(1, "x", Date())
    assert command._ymd == 20261999 and events == ["year", "month", "day"]
    assert constructor(1, "x")._ymd == 0


def test_native_mac_multiday_validation_precedes_all_assignments_and_captures_error(monkeypatch):
    events = []
    failure = ValueError("captured")

    class Days:
        def __ge__(self, other):
            events.append(("ge", other))
            return True

        def __le__(self, other):
            events.append(("le", other))
            return False

        def __format__(self, spec):
            monkeypatch.setattr(mac, "ValueError", lambda message: pytest.fail("captured before format"), raising=False)
            return "six"

    class Command(mac.TickChartsCmd):
        def __setattr__(self, name, value):
            pytest.fail("invalid days cannot assign command state")

    monkeypatch.setattr(mac, "ValueError", lambda message: events.append(message) or failure, raising=False)
    with pytest.raises(ValueError) as caught:
        Command(1, "x", days=Days())
    assert caught.value is failure
    assert events == [("ge", 1), ("le", 5), "days 必须在 1..5 之间，收到 six"]


def test_native_mac_goods_only_original_upper_limit_is_eager():
    assert mac.GoodsListCmd(object(), object(), -3).count == -3
    command = mac.GoodsListCmd(47, 0, 1000)
    assert command.total == 0
    with pytest.raises(ValueError, match="count 不能超过 1000，当前: 1001"):
        mac.GoodsListCmd(47, 0, 1001)
    with pytest.raises(struct.error):
        mac.GoodsListCmd(47, 0, -3).build_request()


def test_native_mac_unusual_min_retains_negative_value_and_live_min(monkeypatch):
    assert mac.UnusualCmd(1, count=-2)._count == -2
    marker = object()
    seen = []
    monkeypatch.setattr(mac, "min", lambda *args: seen.append(args) or marker, raising=False)
    command = mac.UnusualCmd(1, count=2**100)
    assert command._count is marker and seen == [(2**100, 600)]


@pytest.mark.parametrize("constructor,args", [
    (mac.SymbolInfoCmd, (2**100, "x")),
    (mac.SymbolAuctionCmd, (1, "x", -1, 2**100)),
    (mac.KlineOffsetCmd, (-1, 2**100)),
    (mac.FileDownloadCmd, ("x", -1, -2, 2**100)),
])
def test_native_mac_wire_validation_still_occurs_in_struct_builder(constructor, args):
    command = constructor(*args)
    with pytest.raises(struct.error):
        command.build_request()


def test_native_mac_sampling_width_is_read_twice_after_encoding(monkeypatch):
    events = []
    widths = iter((2, 3))

    class Command(mac.ChartSamplingCmd):
        @property
        def _CODE_LEN(self):
            value = next(widths)
            events.append(("width", value))
            return value

    class Code:
        def encode(self, name):
            events.append(("encode", name))
            return b"abcd"

    monkeypatch.setattr(mac, "build_mac_request", lambda ident, body: body)
    assert Command(47, Code()).build_request() == struct.pack("<H22sHH9x", 47, b"abc", 1, 20)
    assert events == [("encode", "gbk"), ("width", 2), ("width", 3)]


def test_native_mac_filename_width_and_padding_stay_live(monkeypatch):
    class Filename:
        def encode(self, encoding):
            assert encoding == "gbk"
            monkeypatch.setattr(mac, "_FILENAME_LEN", 3)
            monkeypatch.setattr(mac, "_FILENAME_PAD", 2)
            return b"abcdef"

    monkeypatch.setattr(mac, "build_mac_request", lambda ident, body: (ident, body))
    assert mac.FileListCmd(Filename(), 7).build_request() == (0x1215, struct.pack("<I", 7) + b"abc\0\0")


def test_native_mac_json_request_retains_eight_byte_gbk_truncation_and_head_flag(monkeypatch):
    command = mac.SymbolBelongBoardCmd(1, "中文代码长")
    command.HEAD_FLAG = 9
    monkeypatch.setattr(mac, "build_mac_request", lambda ident, body, **kwargs: (ident, body, kwargs))
    assert command.build_request() == (0x1218, struct.pack("<H8s16x21s", 1, "中文代码长".encode("gbk"), b"Stock_GLHQ"), {"head_flag": 9})


def test_native_mac_pack_callable_is_captured_before_argument_hooks(monkeypatch):
    seen = []

    class Code:
        def encode(self, encoding):
            monkeypatch.setattr(mac, "struct", SimpleNamespace(pack=lambda *args: pytest.fail("pack was captured")))
            return b"encoded"

    command = mac.SymbolInfoCmd(1, Code())
    monkeypatch.setattr(mac, "struct", SimpleNamespace(pack=lambda *args: seen.append(args) or b"payload"))
    monkeypatch.setattr(mac, "build_mac_request", lambda ident, body: body)
    assert command.build_request() == b"payload"
    assert seen == [("<H22sI12x", 1, b"encoded", 1)]


def test_native_mac_encoding_and_integer_conversion_follow_source_order(monkeypatch):
    events = []

    class Code:
        def encode(self, encoding):
            events.append(("encode", encoding))
            return b"x"

    class Number:
        def __init__(self, name, value):
            self.name, self.value = name, value

        def __int__(self):
            events.append(self.name)
            return self.value

    command = mac.SymbolBarCmd(1, Code(), Number("period", 4), fq=Number("fq", 1))
    monkeypatch.setattr(mac, "build_mac_request", lambda ident, body: body)
    command.build_request()
    assert events == [("encode", "gbk"), "period", "fq"]


@pytest.mark.parametrize("row,message", [
    ((), "not enough values to unpack (expected 2, got 0)"),
    ((1,), "not enough values to unpack (expected 2, got 1)"),
    ((1, "x", 3), "too many values to unpack (expected 2)"),
])
def test_native_mac_quote_row_unpacking_retains_original_exception(row, message):
    command = mac.SymbolQuotesCmd([row])
    with pytest.raises(ValueError) as caught:
        command.build_request()
    assert str(caught.value) == message


def test_native_mac_members_flag_conversion_is_lazy_and_uses_current_int(monkeypatch):
    events = []

    class Flag:
        @property
        def value(self):
            events.append("value")
            return 4

    def total(values):
        events.append("sum")
        monkeypatch.setattr(mac, "int", lambda value: events.append(("int", value)) or 7, raising=False)
        return next(values)

    command = mac.BoardMembersQuotesCmd(1, exclude_flags=[Flag()])
    monkeypatch.setattr(mac, "sum", total, raising=False)
    monkeypatch.setattr(mac, "build_mac_request", lambda ident, body: body)
    assert command.build_request()[-4:] == bytes((0, 7, 0, 1))
    assert events == ["sum", "value", ("int", 4)]


def test_native_mac_members_generator_retains_pep479_cause():
    failure = StopIteration("from flag")

    class Flag:
        @property
        def value(self):
            raise failure

    command = mac.BoardMembersQuotesCmd(1, exclude_flags=[Flag()])
    with pytest.raises(RuntimeError, match="generator raised StopIteration") as caught:
        command.build_request()
    assert caught.value.__cause__ is failure


def test_native_mac_members_consumes_original_iterator_once(monkeypatch):
    events = []

    class Flags:
        def __iter__(self):
            events.append("iter")
            return iter((SimpleNamespace(value=1), SimpleNamespace(value=2)))

    command = mac.BoardMembersQuotesCmd(1, exclude_flags=Flags())
    monkeypatch.setattr(mac, "build_mac_request", lambda ident, body: body)
    assert command.build_request()[-4:] == b"\0\3\0\1"
    assert events == ["iter"]


def test_native_mac_quotes_capture_final_builder_before_bytes_conversion(monkeypatch):
    events = []
    command = mac.SymbolQuotesCmd([(1, "x")])
    payload = object()

    def convert(value):
        events.append("bytes")
        monkeypatch.setattr(mac, "build_mac_request", lambda *args: pytest.fail("builder already captured"))
        return payload

    monkeypatch.setattr(mac, "bytes", convert, raising=False)
    monkeypatch.setattr(mac, "build_mac_request", lambda ident, body: events.append((ident, body)) or "sent")
    assert command.build_request() == "sent"
    assert events == ["bytes", (0x122B, payload)]


def test_native_mac_server_request_keeps_literal_control_bytes(monkeypatch):
    monkeypatch.setattr(mac, "build_mac_request", lambda ident, body: (ident, body))
    assert mac.ServerInfoCmd().build_request() == (0x120F, bytes.fromhex("04002d31")
                                                 + b"\0" * 8 + b"\0\x27\x06\x0e" + b"\0" * 52)


def test_native_mac_server_uses_current_bytes_fromhex(monkeypatch):
    seen = []
    monkeypatch.setattr(mac, "bytes", SimpleNamespace(fromhex=lambda text: seen.append(text) or b"custom"), raising=False)
    monkeypatch.setattr(mac, "build_mac_request", lambda ident, body: body)
    assert mac.ServerInfoCmd().build_request() == b"custom" + b"\0" * 8 + b"\0\x27\x06\x0e" + b"\0" * 52
    assert seen == ["04002d31"]
