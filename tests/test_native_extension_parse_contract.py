"""EX prefix, model and custom-hook sources. Deferred until the full source gate."""
from __future__ import annotations

from collections import OrderedDict
import math
import struct
from types import SimpleNamespace

import pytest

from eltdx.exceptions import ProtocolError
from eltdx.ex.commands import get_history_bars_range as ranges
from eltdx.ex.commands import get_instrument_bars as bars
from eltdx.ex.commands import get_instrument_count as counts
from eltdx.ex.commands import get_instrument_info as info
from eltdx.ex.commands import get_instrument_quote as quote
from eltdx.ex.commands import get_instrument_quote_list as quotes
from eltdx.ex.commands import get_markets as markets
from eltdx.ex.commands import get_minute_time as minutes
from eltdx.ex.commands import get_transaction as trades


def _minute_history_header(count):
    return struct.pack("<B9s8sH", 31, b"00700", b"ignored", count)


def _transaction_header(count):
    return struct.pack("<B9s4sH", 31, b"00700", b"abcd", count)


def _bar_header(count, *, ranged=False):
    return b"\0" * (12 if ranged else 18) + struct.pack("<H", count)


@pytest.mark.parametrize("command,minimum,expected", [
    (counts.GetExInstrumentCountCmd(), 23, 0),
    (markets.GetExMarketsCmd(), 2, []),
    (info.GetExInstrumentInfoCmd(0), 6, []),
    (quote.GetExInstrumentQuoteCmd(31, "00700"), 150, None),
    (quotes.GetExInstrumentQuoteListCmd(31, 2), 2, []),
    (bars.GetExInstrumentBarsCmd(8, 31, "00700"), 20, []),
    (ranges.GetExHistoryInstrumentBarsRangeCmd(31, "00700", 0, 0), 14, []),
    (minutes.GetExMinuteTimeDataCmd(31, "00700"), 12, []),
    (minutes.GetExHistoryMinuteTimeDataCmd(31, "00700", 20261117), 20, []),
    (trades.GetExTransactionDataCmd(31, "00700"), 16, []),
    (trades.GetExHistoryTransactionDataCmd(31, "00700", 20261117), 16, []),
])
def test_native_ex_all_short_response_prefix_defaults(command, minimum, expected):
    for size in range(minimum):
        assert command.parse_response(b"\0" * size) == expected


def test_native_ex_instruments_keep_gbk_replace_right_trim_raw_and_complete_prefix():
    raw = struct.pack("<BB3s9s17s9s", 3, 31, b"abc", b"\0AB\0", b"\xff", b"desc") + b"x" * 24
    body = struct.pack("<IH", 99, 2) + raw + raw[:63]
    rows = info.GetExInstrumentInfoCmd(0).parse_response(body)
    assert len(rows) == 1
    row = rows[0]
    assert (row.category, row.market, row.code, row.name, row.desc) == (3, 31, "\0AB", "�", "desc")
    assert row._raw == raw


def test_native_ex_markets_skip_only_zero_category_and_market_and_keep_raw():
    padding = b"\0" * 64
    first = struct.pack("<B32sB2s", 0, "香港".encode("gbk"), 31, b"HK") + b"a" * 28
    second = struct.pack("<B32sB2s", 7, b"\0market\0", 0, b"\xff\0") + b"b" * 28
    body = struct.pack("<H", 4) + padding + first + second + first[:63]
    rows = markets.GetExMarketsCmd().parse_response(body)
    assert [(row.category, row.market, row.name, row.short_name) for row in rows] == [
        (0, 31, "香港", "HK"), (7, 0, "\0market", "�")]
    assert [row._raw for row in rows] == [first, second]


def test_native_ex_markets_keeps_conditional_le_and_none_record_hook(monkeypatch):
    calls = []
    count = 0

    class Length:
        def __ge__(self, other):
            calls.append(("ge", other))
            return True

    def length(body):
        nonlocal count
        count += 1
        return 66 if count == 1 else Length()

    monkeypatch.setattr(markets, "len", length, raising=False)
    monkeypatch.setattr(markets, "slice_bytes", lambda *args: calls.append(args) or None)
    assert markets.GetExMarketsCmd().parse_response(struct.pack("<H", 1)) == []
    assert calls == [("ge", 66), (struct.pack("<H", 1), 2, 64, "ex markets record")]


def test_native_ex_count_keeps_guarded_hook_int_and_original_values(monkeypatch):
    marker = object()
    returned = object()
    calls = []
    monkeypatch.setattr(counts, "unpack_from", lambda *args: calls.append(args) or (marker,))
    monkeypatch.setattr(counts, "int", lambda value: calls.append(value) or returned, raising=False)
    body = b"\0" * 23
    assert counts.GetExInstrumentCountCmd().parse_response(body) is returned
    assert calls == [("<I", body, 19, "ex instrument count"), marker]


@pytest.mark.parametrize("history", [False, True])
def test_native_ex_minutes_keep_unvalidated_clock_native_units_and_prefix(history):
    raw = struct.pack("<HffII", 1441, 1.25, 2.5, 12345, 54321)
    header = _minute_history_header(2) if history else struct.pack("<B9sH", 31, b"00700", 2)
    command = minutes.GetExHistoryMinuteTimeDataCmd(31, "x", 20261117) if history else minutes.GetExMinuteTimeDataCmd(31, "x")
    rows = command.parse_response(header + raw + raw[:17])
    assert len(rows) == 1
    row = rows[0]
    assert (row.hour, row.minute, row.price, row.avg_price, row.volume, row.open_interest) == (24, 1, 1.25, 2.5, 12345, 54321)
    assert row._raw == raw


def test_native_ex_history_uses_live_today_class_helper_and_today_uses_self(monkeypatch):
    marker = object()
    calls = []

    class Command(minutes.GetExMinuteTimeDataCmd):
        def _parse_records(self, *args):
            calls.append(("self", args))
            return marker

    body = struct.pack("<B9sH", 31, b"x", 7)
    assert Command(31, "x").parse_response(body) is marker
    monkeypatch.setattr(minutes, "GetExMinuteTimeDataCmd", SimpleNamespace(
        _parse_records=lambda *args: calls.append(("class", args)) or marker))
    historical = _minute_history_header(9)
    assert minutes.GetExHistoryMinuteTimeDataCmd(31, "x", 0).parse_response(historical) is marker
    assert calls == [("self", (body, 12, 7)), ("class", (historical, 20, 9))]


@pytest.mark.parametrize("history", [False, True])
def test_native_ex_transactions_keep_seconds_coercion_signed_interest_raw_and_prefix(history):
    records = [struct.pack("<HIIiH", 1441, 12345000, 7, -3, value) for value in (20059, 20060)]
    command = trades.GetExHistoryTransactionDataCmd(31, "x", 0) if history else trades.GetExTransactionDataCmd(31, "x")
    rows = command.parse_response(_transaction_header(3) + b"".join(records) + records[0][:15])
    assert len(rows) == 2
    assert [(row.hour, row.minute, row.second, row.nature) for row in rows] == [(24, 1, 59, 2), (24, 1, 0, 2)]
    assert [(row.price, row.volume, row.zengcang) for row in rows] == [(12345000, 7, -3)] * 2
    assert [row._raw for row in rows] == records


def test_native_ex_historical_transactions_keep_shared_class_hook_error_identity(monkeypatch):
    failure = RuntimeError("custom transaction mapper")

    def helper(*args):
        assert args == (body, 16, 9)
        raise failure

    body = _transaction_header(9)
    monkeypatch.setattr(trades, "GetExTransactionDataCmd", SimpleNamespace(_parse_records=helper))
    with pytest.raises(RuntimeError) as caught:
        trades.GetExHistoryTransactionDataCmd(31, "x", 0).parse_response(body)
    assert caught.value is failure


def test_native_ex_bars_keep_position_float_reinterpretation_and_numeric_prefix():
    position_bits = 0x3F800000
    date = struct.pack("<I", 20261399)
    raw = date + struct.pack("<ffffIIf", 1.0, 2.0, 0.5, 1.5, position_bits, 19, 77.0)
    # A complete second datetime plus a short numeric part returns the first row.
    rows = bars.GetExInstrumentBarsCmd(4, 31, "x").parse_response(_bar_header(2) + raw + date + b"\0" * 27)
    assert len(rows) == 1
    row = rows[0]
    assert (row.year, row.month, row.day, row.hour, row.minute) == (2026, 13, 99, 15, 0)
    assert (row.open, row.high, row.low, row.close, row.position, row.trade, row.amount) == (1.0, 2.0, 0.5, 1.5, position_bits, 19, 1.0)
    assert row._raw == raw


def test_native_ex_bars_still_decode_datetime_before_numeric_length_check():
    command = bars.GetExInstrumentBarsCmd(4, 31, "x")
    with pytest.raises(ProtocolError, match="day datetime"):
        command.parse_response(_bar_header(1) + b"\0" * 3)
    assert command.parse_response(_bar_header(1) + struct.pack("<I", 20261117) + b"\0" * 27) == []


def test_native_ex_bars_capture_datetime_hook_before_category_property(monkeypatch):
    calls = []

    def original(category, body, pos):
        calls.append((category, pos))
        return 1, 2, 3, 4, 5, pos + 4

    class Command(bars.GetExInstrumentBarsCmd):
        @property
        def category(self):
            monkeypatch.setattr(bars, "get_datetime", lambda *args: pytest.fail("hook was captured"))
            return 88

        @category.setter
        def category(self, value):
            pass

    monkeypatch.setattr(bars, "get_datetime", original)
    row = Command(4, 31, "x").parse_response(_bar_header(1) + b"\0" * 32)[0]
    assert calls == [(88, 20)] and (row.year, row.month, row.day, row.hour, row.minute) == (1, 2, 3, 4, 5)


def test_native_ex_range_keeps_overridden_date_time_settlement_and_complete_prefix():
    calls = []

    class Command(ranges.GetExHistoryInstrumentBarsRangeCmd):
        def _parse_date(self, value):
            calls.append(("date", value))
            return -1, 99, 88

        def _parse_time(self, value):
            calls.append(("time", value))
            return 25, 77

    raw = struct.pack("<HHffffIIf", 17, 61, 1.0, 2.0, 0.5, 1.5, 123, 456, 9.25)
    rows = Command(31, "x", 0, 0).parse_response(_bar_header(2, ranged=True) + raw + raw[:31])
    assert len(rows) == 1 and calls == [("date", 17), ("time", 61)]
    row = rows[0]
    assert (row.year, row.month, row.day, row.hour, row.minute) == (-1, 99, 88, 25, 77)
    assert (row.position, row.trade, row.amount, row._raw) == (123, 456, 9.25, raw)


def test_native_ex_range_never_calls_date_helpers_for_an_incomplete_record(monkeypatch):
    command = ranges.GetExHistoryInstrumentBarsRangeCmd(31, "x", 0, 0)
    monkeypatch.setattr(command, "_parse_date", lambda value: pytest.fail("full32 bytes required"))
    assert command.parse_response(_bar_header(1, ranged=True) + b"\0" * 31) == []


def test_native_ex_quote_projects_every_slot_unknown_gaps_nonfinite_and_raw_prefix():
    float_slots = set(range(5)) | set(range(14, 19)) | set(range(24, 29))
    payload = b"".join(struct.pack("<f", index + 0.5) if index in float_slots else struct.pack("<I", index + 1000)
                       for index in range(34))
    raw = struct.pack("<B9s", 31, b"\0A\xff") + b"abcd" + payload
    row = quote.GetExInstrumentQuoteCmd(31, "x").parse_response(raw + b"trailer")
    assert row.market == 31 and row.code == "\0A�" and row._raw == raw
    expected = {"pre_close": 0, "open": 1, "high": 2, "low": 3, "price": 4, "kaicang": 5,
                "zongliang": 7, "xianliang": 8, "neipan": 10, "waipan": 11, "chicang": 13}
    for offset in range(5):
        expected[f"bid{offset + 1}"] = 14 + offset
        expected[f"bid_vol{offset + 1}"] = 19 + offset
        expected[f"ask{offset + 1}"] = 24 + offset
        expected[f"ask_vol{offset + 1}"] = 29 + offset
    for name, index in expected.items():
        assert getattr(row, name) == index + (0.5 if index in float_slots else 1000)
    nonfinite = raw[:14] + struct.pack("<I", 0x7FC00001) + raw[18:]
    assert math.isnan(quote.GetExInstrumentQuoteCmd(31, "x").parse_response(nonfinite).pre_close)


@pytest.mark.parametrize("size,expected", [(33, "not enough values to unpack (expected 34, got 33)"),
                                           (35, "too many values to unpack (expected 34)")])
def test_native_ex_quote_preserves_struct_tuple_arity_errors(monkeypatch, size, expected):
    def unpack(fmt, data):
        return (31, b"x") if fmt == "<B9s" else tuple(range(size))

    monkeypatch.setattr(quote, "struct", SimpleNamespace(unpack=unpack))
    with pytest.raises(ValueError) as caught:
        quote.GetExInstrumentQuoteCmd(31, "x").parse_response(b"\0" * 150)
    assert str(caught.value) == expected


def test_native_ex_hk_quote_list_accepts_missing_opaque_tail_and_keeps_all_fields():
    fmt = "<IfffffIfIIfIIIIfffffIIIIIfffffIIIII"
    payload = struct.pack(fmt, *range(35))
    body = struct.pack("<H", 1) + struct.pack("<B9s", 31, b"\0AB\0") + payload
    rows = quotes.GetExInstrumentQuoteListCmd(31, 2).parse_response(body)
    assert len(rows) == 1 and isinstance(rows[0], OrderedDict)
    row = rows[0]
    expected = [("market", 31), ("code", "AB"), ("HuoYueDu", 0), ("ZuoShou", 1), ("JinKai", 2),
                ("ZuiGao", 3), ("ZuiDi", 4), ("XianJia", 5), ("MaiRuJia", 7), ("ZongLiang", 8),
                ("XianLiang", 9), ("ZongJinE", 10), ("Nei", 13), ("Wai", 14)]
    for prefix, start in [("MaiRuJia", 15), ("MaiRuLiang", 20), ("MaiChuJia", 25), ("MaiChuLiang", 30)]:
        expected.extend((f"{prefix}{offset + 1}", start + offset) for offset in range(5))
    assert list(row.items()) == expected


def test_native_ex_futures_quote_list_preserves_original_34_target_bug_and_short_prefix():
    raw = b"\0" * 140
    rows = []
    command = quotes.GetExInstrumentQuoteListCmd(30, 3)
    with pytest.raises(ValueError, match="too many values to unpack \\(expected 34\\)"):
        command.parse_response(struct.pack("<H", 1) + struct.pack("<B9s", 30, b"future") + raw)
    assert command._parse_futures(30, "future", raw[:139], 0, rows) == 290
    assert rows == []


def test_native_ex_futures_custom_struct_34_values_retains_ordered_projection(monkeypatch):
    monkeypatch.setattr(quotes, "struct", SimpleNamespace(unpack=lambda fmt, body: tuple(range(34))))
    rows = []
    assert quotes.GetExInstrumentQuoteListCmd._parse_futures(30, "f", b"\0" * 140, 0, rows) == 290
    assert list(rows[0].items()) == [("market", 30), ("code", "f"), ("BiShu", 0), ("ZuoJie", 1),
        ("JinKai", 2), ("ZuiGao", 3), ("ZuiDi", 4), ("MaiChu", 5), ("KaiCang", 6),
        ("ZongLiang", 8), ("XianLiang", 9), ("ZongJinE", 10), ("NeiPan", 11), ("WaiPan", 12),
        ("ChiCangLiang", 14), ("MaiRuJia", 15), ("MaiRuLiang", 20), ("MaiChuJia", 25), ("MaiChuLiang", 30)]


def test_native_ex_quote_list_reads_category_per_row_and_keeps_private_helpers_live():
    calls = []

    class Command(quotes.GetExInstrumentQuoteListCmd):
        def _parse_hk_stocks(self, market, code, body, pos, results):
            calls.append(("hk", market, code, pos))
            results.append("hk marker")
            self.category = 3
            return pos

        def _parse_futures(self, market, code, body, pos, results):
            calls.append(("futures", market, code, pos))
            results.append("future marker")
            return pos

    body = struct.pack("<H", 2) + struct.pack("<B9s", 31, b"\0one\0") + struct.pack("<B9s", 30, b"two")
    assert Command(31, 2).parse_response(body) == ["hk marker", "future marker"]
    assert calls == [("hk", 31, "one", 12), ("futures", 30, "two", 22)]


def test_native_ex_quote_private_helper_advances_inplace_before_ordered_dict_hook(monkeypatch):
    events = []

    class Position(int):
        def __iadd__(self, value):
            events.append(("iadd", value))
            return 711

    class Output:
        def append(self, row):
            events.append(("append", row))

    marker = object()
    monkeypatch.setattr(quotes, "OrderedDict", lambda fields: events.append(list(fields)) or marker)
    assert quotes.GetExInstrumentQuoteListCmd._parse_hk_stocks(31, "x", b"\0" * 140, Position(0), Output()) == 711
    assert events[0] == ("iadd", 290)
    assert events[1][:2] == [("market", 31), ("code", "x")]
    assert events[2] == ("append", marker)


def test_native_ex_minute_captures_model_before_raw_slice_hook(monkeypatch):
    marker = object()
    fields = []

    class Body(bytes):
        def __getitem__(self, key):
            if key.start == 0 and key.stop == 18 and fields:
                monkeypatch.setattr(minutes, "ExMinuteBar", lambda **kwargs: pytest.fail("model captured before raw"))
            return super().__getitem__(key)

    def unpack(fmt, raw):
        # The first body slice supplies unpack; the second is the model raw.
        fields.append("unpack")
        return 61, 1.0, 2.0, 3, 4

    def model(**kwargs):
        fields.append(kwargs)
        return marker

    monkeypatch.setattr(minutes, "struct", SimpleNamespace(unpack=unpack))
    monkeypatch.setattr(minutes, "ExMinuteBar", model)
    body = Body(b"\0" * 18)
    assert minutes.GetExMinuteTimeDataCmd._parse_records(body, 0, 1) == [marker]
    assert fields[-1] == dict(hour=1, minute=1, price=1.0, avg_price=2.0, volume=3, open_interest=4, _raw=body)


def test_native_ex_directory_keeps_none_start_prefix_slice_and_raw_identity(monkeypatch):
    marker = object()
    calls = []

    class Raw:
        def __getitem__(self, key):
            calls.append((key.start, key.stop, key.step))
            return b"\0" * 36

    raw = Raw()
    monkeypatch.setattr(markets, "slice_bytes", lambda *args: raw)
    monkeypatch.setattr(markets, "struct", SimpleNamespace(
        unpack_from=lambda *args: (1,), unpack=lambda *args: (3, b"market", 31, b"HK")))
    monkeypatch.setattr(markets, "ExMarketInfo", lambda **fields: calls.append(fields) or marker)
    assert markets.GetExMarketsCmd().parse_response(b"\0" * 66) == [marker]
    assert calls[0] == (None, 36, None) and calls[1]["_raw"] is raw


def test_native_ex_parser_keeps_custom_range_and_arbitrary_header_count(monkeypatch):
    huge = 2**90
    calls = []

    def unpack(fmt, body):
        return (0, huge) if fmt == "<IH" else (3, 31, b"", b"x", b"name", b"desc")

    monkeypatch.setattr(info, "range", lambda count: calls.append(count) or [object()], raising=False)
    monkeypatch.setattr(info, "struct", SimpleNamespace(unpack=unpack))
    rows = info.GetExInstrumentInfoCmd(0).parse_response(b"\0" * 70)
    assert len(rows) == 1 and rows[0].code == "x" and calls == [huge]


def test_native_ex_parser_preserves_iterator_failure_and_dto_constructor_errors(monkeypatch):
    failure = RuntimeError("original iteration error")

    class Broken:
        def __iter__(self):
            raise failure

    monkeypatch.setattr(counts, "unpack_from", lambda *args: Broken())
    with pytest.raises(RuntimeError) as caught:
        counts.GetExInstrumentCountCmd().parse_response(b"\0" * 23)
    assert caught.value is failure

    def bad_model(**fields):
        raise failure

    monkeypatch.setattr(minutes, "ExMinuteBar", bad_model)
    with pytest.raises(RuntimeError) as caught:
        minutes.GetExMinuteTimeDataCmd._parse_records(b"\0" * 18, 0, 1)
    assert caught.value is failure


@pytest.mark.parametrize("kind", [bytes, bytearray])
def test_native_ex_record_raw_retains_original_slice_type(kind):
    body = kind(struct.pack("<HffII", 61, 1.0, 2.0, 3, 4))
    row = minutes.GetExMinuteTimeDataCmd._parse_records(body, 0, 1)[0]
    assert type(row._raw) is kind and row._raw == body
