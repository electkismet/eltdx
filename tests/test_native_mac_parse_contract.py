"""MAC response compatibility sources; deferred until the complete source gate."""
from __future__ import annotations

from datetime import date, datetime, time
import json
import math
import struct
import sys
from types import SimpleNamespace

import pytest

from eltdx.exceptions import ProtocolError
from eltdx.mac import commands as mac


def _tail():
    return struct.pack("<44sBHf5x2I5ffIf12s2fI", b"name", 2, 3, 1.0,
                       20261117, 93000, 10.0, 11.0, 12.0, 9.0, 10.5,
                       0.0, 100, 1000.0, b"", 0.25, 10.25, 7)


def _multi(*, count=3, page=2, total=5, dates=(20261113, 20261116, 20261117, 0, 0)):
    body = struct.pack("<H22s5I5fHBHH", 47, b"IFL0", *dates, 1, 2, 3, 4, 5, count, 0, page, total)
    return body + b"".join(struct.pack("<HffHH", 570 + i, float(i), 2.0, 10 + i, 0)
                            for i in range(total)) + _tail()


def _bar_body(rows, *, count=None):
    header = b"\0" * 24 + struct.pack("<HBHI", 0, 1, len(rows) if count is None else count, 0)
    return header + b"".join(struct.pack("<II7f", *row) for row in rows)


def _quote_row(market=1, code=b"\0A\0B", name=b"\0N\0M", slots=b""):
    return struct.pack("<H22s44s", market, code, name) + slots


def test_native_mac_quotes_keep_unknown_slot_width_prefix_and_decode_policy(monkeypatch):
    field = SimpleNamespace(value=74, field_name="ah_code")
    monkeypatch.setattr(mac, "iter_set_bits", lambda bitmap: [(0, None, "<f"), (1, field, "<I")])
    monkeypatch.setattr(mac, "count_set_bits", lambda bitmap: 2)
    seen = []
    monkeypatch.setattr(mac, "FIELD_POSTPROCESS", {74: lambda value, market: seen.append((value, market)) or f"mapped-{value}"})
    row = _quote_row(slots=struct.pack("<fI", 1.25, 700))
    body = b"\0" * 20 + struct.pack("<IH", 123, 2) + row + row[:-1]
    rows = mac.SymbolQuotesCmd([(1, "x")]).parse_response(body)
    assert len(rows) == 1
    assert (rows[0].market, rows[0].code, rows[0].name, rows[0].fields) == (1, "AB", "NM", {"ah_code": "mapped-700"})
    assert seen == [(700, 1)]


def test_native_mac_members_keep_partial_named_slot_row_and_replace_right_trim(monkeypatch):
    field = SimpleNamespace(value=1, field_name="price")
    monkeypatch.setattr(mac, "iter_set_bits", lambda bitmap: [(0, field, "<f")])
    monkeypatch.setattr(mac, "count_set_bits", lambda bitmap: 1)
    body = b"\0" * 20 + struct.pack("<IH", 2, 1) + _quote_row(code=b"\0A\0B", name=b"\xff", slots=b"\0\0")
    row = mac.BoardMembersQuotesCmd(1).parse_response(body)[0]
    assert row.code == "\0A\0B" and row.name == "�" and row.fields == {}
    with pytest.raises(ProtocolError, match=r"board_members row\[0\] market"):
        mac.BoardMembersQuotesCmd(1).parse_response(body[:27])


@pytest.mark.parametrize("constructor,args", [(mac.SymbolQuotesCmd, ([(1, "x")],)), (mac.BoardMembersQuotesCmd, (1,))])
def test_native_mac_both_quote_paths_apply_live_postprocess_and_cast(constructor, args, monkeypatch):
    command = constructor(*args)
    field = SimpleNamespace(value=74, field_name="ah_code")
    seen = []
    monkeypatch.setattr(mac, "iter_set_bits", lambda bitmap: [(0, field, "<I")])
    monkeypatch.setattr(mac, "count_set_bits", lambda bitmap: 1)
    monkeypatch.setattr(mac, "FIELD_POSTPROCESS", {74: lambda value, market: seen.append((value, market)) or "00700"})
    monkeypatch.setattr(mac, "cast", lambda hint, value: seen.append((hint, value)) or value)
    body = b"\0" * 20 + struct.pack("<IH", 1, 1) + _quote_row(slots=struct.pack("<I", 700))
    assert command.parse_response(body)[0].fields == {"ah_code": "00700"}
    assert seen == [(700, 1), ("object", "00700")]


def test_native_mac_bar_seconds_keep_two_distinct_seconds_and_prefix_clamp():
    first = (20261117, 34201, 10, 12, 9, 11, 1000, 123, 999)
    second = (20261117, 34202, 11, 13, 10, 12, 2000, 234, 888)
    body = _bar_body([first, second], count=99) + b"truncated"
    rows = mac.SymbolBarCmd(47, "IFL0", mac.Period.SECONDS).parse_response(body)
    assert [row.datetime for row in rows] == [datetime(2026, 11, 17, 9, 30, 1), datetime(2026, 11, 17, 9, 30, 2)]
    assert (rows[0].vol, rows[0].amount, rows[0].float_shares) == (123.0, 1000.0, 999.0)


def test_native_mac_bars_skip_outside_year_window_but_keep_calendar_error():
    values = (0, 1, 2, 3, 4, 5, 6)
    rows = [(19891231, 0, *values), (21000101, 0, *values), (20261117, 999999, *values)]
    parsed = mac.SymbolBarCmd(1, "x").parse_response(_bar_body(rows))
    assert len(parsed) == 1 and parsed[0].datetime == datetime(2026, 11, 17)
    with pytest.raises(ValueError):
        mac.SymbolBarCmd(1, "x").parse_response(_bar_body([(20260230, 0, *values)]))


def test_native_mac_bar_dto_constructor_is_captured_before_datetime_hook(monkeypatch):
    captured = []
    monkeypatch.setattr(mac, "MacBar", lambda **kwargs: captured.append(kwargs) or kwargs)

    def combine(*args):
        monkeypatch.setattr(mac, "MacBar", lambda **kwargs: pytest.fail("DTO constructor already captured"))
        return "when"

    monkeypatch.setattr(mac, "_combine_bar_datetime", combine)
    row = mac.SymbolBarCmd(1, "x").parse_response(_bar_body([(20261117, 1, 1, 2, 3, 4, 5, 6, 7)]))[0]
    assert row["datetime"] == "when" and row["amount"] == 5
    assert len(captured) == 1


def test_native_mac_single_tick_chart_wraps_minutes_and_maps_snapshot():
    body = struct.pack("<H22sIBfH", 47, b"IFL0", 99999999, 1, 123.0, 1)
    body += struct.pack("<HffIf", 1441, 1.25, 2.5, 100, -0.5) + _tail()
    result = mac.SymbolTickChartCmd(47, "IFL0").parse_response(body)
    assert result.charts[0].time == time(0, 1)
    assert (result.charts[0].price, result.charts[0].momentum, result.charts[0].vol) == (1.25, -0.5, 100)
    assert (result.market, result.code, result.name, result.pre_close, result.vol, result.turnover, result.avg) == (47, "IFL0", "name", 10, 100, 0.25, 10.25)
    with pytest.raises(ProtocolError, match="tick_chart tail"):
        mac.SymbolTickChartCmd(47, "IFL0").parse_response(body[:-1])


def test_native_mac_multiday_ticks_keep_short_oldest_then_full_day_order():
    result = mac.TickChartsCmd(47, "IFL0").parse_response(_multi())
    assert [day.date for day in result.charts] == [date(2026, 11, 13), date(2026, 11, 16), date(2026, 11, 17)]
    assert [len(day.ticks) for day in result.charts] == [1, 2, 2]
    assert [tick.price for day in result.charts for tick in day.ticks] == [0, 1, 2, 3, 4]
    assert [day.pre_close for day in result.charts] == [1, 2, 3]
    assert result.charts[0].ticks[0].momentum == 0.0


def test_native_mac_multiday_clamps_count_to_leading_dates_and_page_size():
    result = mac.TickChartsCmd(47, "IFL0").parse_response(_multi(count=99, page=0, total=2, dates=(20261116, 20261117, 0, 20261118, 0)))
    assert [len(day.ticks) for day in result.charts] == [1, 1]
    assert [day.date for day in result.charts] == [date(2026, 11, 16), date(2026, 11, 17)]
    assert mac.TickChartsCmd(47, "IFL0").parse_response(_multi(count=0, total=0, dates=(0, 0, 0, 0, 0))).charts == []


def test_native_mac_transactions_keep_seconds_flags_and_no_prefix_recovery():
    body = bytearray(39)
    struct.pack_into("<H", body, 29, 1)
    body += struct.pack("<IfIIH", 34201, 1.25, 99, 7, 5)
    row = mac.SymbolTransactionCmd(1, "x").parse_response(body)[0]
    assert (row.time, row.price, row.vol, row.trade_count, row.bs_flag) == (time(9, 30, 1), 1.25, 99, 7, 5)
    with pytest.raises(ProtocolError, match=r"transaction item\[0\]"):
        mac.SymbolTransactionCmd(1, "x").parse_response(body[:-1])
    struct.pack_into("<I", body, 39, 86400)
    with pytest.raises(ValueError):
        mac.SymbolTransactionCmd(1, "x").parse_response(body)


def test_native_mac_symbol_snapshot_uses_hhmmss_and_original_extra_offsets():
    body = bytearray(194)
    struct.pack_into("<H22s44s", body, 8, 1, b"600000", b"name")
    struct.pack_into("<III5ffIfII", body, 96, 20261117, 93001, 7, 10, 11, 12, 9, 10.5, 0.5, 99, 1000, 40, 59)
    struct.pack_into("<HIf20xI3f", body, 148, 2, 3, 4.0, 5, 6.0, 7.0, 8.0)
    row = mac.SymbolInfoCmd(1, "x").parse_response(body)
    assert row.time == datetime(2026, 11, 17, 9, 30, 1)
    assert (row.activity, row.vol, row.amount, row.inside_volume, row.outside_volume, row.turnover, row.avg) == (7, 99, 1000, 40, 59, 7, 8)


def test_native_mac_board_list_pairs_two_source_halves_and_uses_declared_halved_count():
    record = struct.pack("<H6s16s44sfffH6s16s44sfff", 1, b"\0A", b"x", b"\xff", 1, 2, 3, 0, b"000001", b"y", b"leader", 4, 5, 6)
    result = mac.BoardListCmd().parse_response(struct.pack("<HH", 3, 99) + record + b"ignored")
    assert len(result) == 1
    row = result[0]
    assert (row.code, row.name, row.price, row.symbol_code, row.symbol_name, row.symbol_price) == ("\0A", "�", 1, "000001", "leader", 4)
    with pytest.raises(ProtocolError, match=r"board_list record\[0\]"):
        mac.BoardListCmd().parse_response(struct.pack("<HH", 2, 99) + record[:-1])


def test_native_mac_belonging_json_only_accepts_nine_or_thirteen_fields():
    rows = [["3.9", "1.9", 123, "板块", "12.5", None, 0, 0, 0], [1] * 8,
            [4, 0, "abc", "second", False, "3", *([0] * 7)]]
    body = b"\0" * 27 + json.dumps(rows, ensure_ascii=False).encode("gbk")
    result = mac.SymbolBelongBoardCmd(1, "x").parse_response(body)
    assert [(row.board_type, row.market, row.board_code, row.board_name, row.close, row.pre_close) for row in result] == [
        (3, 1, "123", "板块", 12.5, 0.0), (4, 0, "abc", "second", 0.0, 3.0)]


def test_native_mac_capital_json_retains_original_column_order_and_missing_zeros():
    body = b"\0" * 27 + json.dumps([["10", "4", "2", "5"], [0, 0, 0, "8", "7"]]).encode("gbk")
    result = mac.SymbolCapitalFlowCmd(1, "x").parse_response(body)
    assert (result.main_in, result.main_out, result.main_net, result.small_in, result.small_out, result.small_net) == (10, 4, 6, 2, 5, -3)
    assert (result.mid_in, result.mid_out, result.mid_net, result.large_net, result.date) == (0, 0, 7, 8, "")
    empty = mac.SymbolCapitalFlowCmd(1, "x").parse_response(b"\0" * 27 + b"[[], []]")
    assert empty.main_net == 0 and empty.large_net == 0
    assert mac.SymbolCapitalFlowCmd(1, "x").parse_response(b"\0" * 27 + b"[[]]") is None


def test_native_mac_json_decode_error_modes_stay_different():
    data = b"\0" * 27 + b'[[1, 1, "x", "\xff", 0, 0, 0, 0, 0]]'
    assert mac.SymbolBelongBoardCmd(1, "x").parse_response(data)[0].board_name == "�"
    with pytest.raises(UnicodeDecodeError):
        mac.SymbolCapitalFlowCmd(1, "x").parse_response(data)


@pytest.mark.parametrize("helper,expected", [(mac._to_float, 0.0), (mac._to_int, 0)])
def test_native_mac_numeric_helpers_keep_original_error_scope(helper, expected):
    assert helper("bad") == expected and helper(None) == expected
    assert helper("3.9") == (3 if helper is mac._to_int else 3.9)
    if helper is mac._to_int:
        with pytest.raises(OverflowError):
            helper(float("inf"))
    else:
        assert math.isnan(helper(float("nan")))


def test_native_mac_numeric_conversion_captures_int_float_before_cast(monkeypatch):
    seen = []
    result = object()
    monkeypatch.setattr(mac, "int", lambda value: seen.append(("int", value)) or result, raising=False)
    monkeypatch.setattr(mac, "float", lambda value: seen.append(("float", value)) or 4.5, raising=False)

    def cast(hint, value):
        seen.append((hint, value))
        monkeypatch.setattr(mac, "int", lambda value: pytest.fail("int captured before cast"))
        monkeypatch.setattr(mac, "float", lambda value: pytest.fail("float captured before cast"))
        return "converted"

    monkeypatch.setattr(mac, "cast", cast)
    assert mac._to_int("original") is result
    assert seen == [("float", "original"), ("float", "converted"), ("int", 4.5)]


def test_native_mac_numeric_except_shell_retains_outer_active_error(monkeypatch):
    error = ValueError("conversion")
    outer = RuntimeError("outer handler")
    seen = []

    def fail(value):
        seen.append(sys.exc_info()[1])
        raise error

    monkeypatch.setattr(mac, "float", fail, raising=False)
    try:
        raise outer
    except RuntimeError:
        assert mac._to_float("x") == 0.0
        assert sys.exc_info()[1] is outer
    assert seen == [outer]
    monkeypatch.setattr(mac, "float", lambda value: (_ for _ in ()).throw(OverflowError("overflow")))
    with pytest.raises(OverflowError, match="overflow"):
        mac._to_float("x")


def test_native_mac_auction_keeps_signed_unmatched_complete_prefix_and_seconds():
    body = struct.pack("<H22sI", 1, b"600000", 2) + b"\0" * 8
    row = struct.pack("<IfIi", 34201, 12.5, 30, -7)
    result = mac.SymbolAuctionCmd(1, "x").parse_response(body + row + row[:15])
    assert len(result) == 1
    assert (result[0].time, result[0].matched, result[0].unmatched) == (time(9, 30, 1), 30, -7)


@pytest.mark.parametrize("kind,expected", [
    (3, ("主力买入", "1.25/-0.50")), (4, ("加速拉升", "125.00%")),
    (5, ("加速下跌", "")), (6, ("低位反弹", "125.00%")),
    (7, ("高位回落", "125.00%")), (8, ("撑杆跳高", "125.00%")),
    (9, ("平台跳水", "125.00%")), (10, ("单笔冲涨", "125.00%")),
    (11, ("区间放量跌", "1.2倍-50.00%")), (12, ("区间缩量", "")),
    (16, ("大单托盘", "17.00/-0.50")), (17, ("大单压盘", "1.25/-0.50")),
    (18, ("大单锁盘", "")), (19, ("竞价试买", "1.25/-0.50")),
    (255, ("异动类型0xff", "")),
])
def test_native_mac_unusual_description_matches_source_branches(kind, expected):
    assert mac._describe_unusual(kind, struct.pack("<B2fI", 0, 1.25, -0.5, 17)) == expected
    assert mac._describe_unusual(kind, b"\0" * 12) == ("", "")


@pytest.mark.parametrize("subtype,description", [(1, "逼近涨停"), (2, "封涨停板"), (4, "封涨大减"), (5, "打开涨停"), (0, "涨跌停(涨)")])
def test_native_mac_unusual_limit_branch_keeps_overlapping_alternate_fields(subtype, description):
    payload = b"\0" + struct.pack("<Bff", subtype, 1.25, 2.5) + b"\0" * 3
    assert mac._describe_unusual(20, payload) == (description, "1.25/2.50")


def test_native_mac_unusual_preserves_second_model_pass_and_declared_text_offset(monkeypatch):
    row = bytearray(32)
    struct.pack_into("<H6sBBBHH", row, 0, 1, b"600000", 0, 4, 0, 7, 0)
    row[15:28] = struct.pack("<B2fI", 0, 1.25, 2.5, 17)
    struct.pack_into("<BH", row, 29, 9, 3001)
    seen = []
    original = mac.UnusualItem
    monkeypatch.setattr(mac, "UnusualItem", lambda **kwargs: seen.append(kwargs.copy()) or original(**kwargs))
    result = mac.UnusualCmd(1).parse_response(struct.pack("<H", 1) + row + b",name,")
    assert len(seen) == 2 and seen[0]["name"] == "" and seen[1]["name"] == "name"
    assert (result[0].index, result[0].time, result[0].desc, result[0].value) == (7, time(9, 30, 1), "加速拉升", "125.00%")


def test_native_mac_server_keeps_short_default_and_both_session_tables():
    command = mac.ServerInfoCmd()
    assert command.parse_response(b"\0" * 86).today == ""
    body = bytearray(87)
    struct.pack_into("<I", body, 22, 20261117)
    struct.pack_into("<8H", body, 30, 570, 690, 780, 900, 0, 0, 1441, 1502)
    struct.pack_into("<8H", body, 46, 1, 2, 3, 4, 5, 6, 7, 8)
    struct.pack_into("<I", body, 63, 20261116)
    struct.pack_into("<II", body, 71, 123, 456)
    result = command.parse_response(body)
    assert (result.today, result.last_trading_day, result.market_param_1, result.market_param_2) == ("2026-11-17", "2026-11-16", 123, 456)
    assert result.sessions_1 == [{"open": "9:30", "close": "11:30"}, {"open": "13:00", "close": "15:00"},
                                 {"open": "0:00", "close": "0:00"}, {"open": "24:01", "close": "25:02"}]
    assert result.sessions_2[0] == {"open": "0:01", "close": "0:02"}


def test_native_mac_kline_offsets_preserve_big_then_little_endian_and_extra_tail():
    command = mac.KlineOffsetCmd()
    assert command.parse_response(b"\0" * 7).total == 0
    result = command.parse_response(struct.pack(">I", 0x01020304) + struct.pack("<I", 0x05060708) + b"tail")
    assert (result.total, result.returned) == (0x01020304, 0x05060708)


def test_native_mac_sampling_requires_every_declared_price_and_keeps_nonfinite():
    command = mac.ChartSamplingCmd(1, "x")
    assert command.parse_response(b"\0" * 41) == []
    body = b"\0" * 40 + struct.pack("<Hff", 2, -0.0, float("nan"))
    values = command.parse_response(body)
    assert math.copysign(1.0, values[0]) == -1 and math.isnan(values[1])
    with pytest.raises(ProtocolError, match=r"ChartSamplingCmd price\[1\]"):
        command.parse_response(body[:-1])


def test_native_mac_goods_set_total_before_record_error_and_keep_code_vector():
    command = mac.GoodsListCmd(47)
    body = struct.pack("<H", 1) + struct.pack("<H23sHIBfffHH", 3, b"\xff", 7, 99, 1, 1.25, -0.0, float("inf"), 8, 9)
    result = command.parse_response(body)
    assert command.total == 1 and result[0].name == "�"
    assert (result[0].category, result[0].u, result[0].index, result[0].switch, result[0].c1, result[0].c2) == (3, 7, 99, 1, 8, 9)
    assert result[0].code[0] == 1.25 and math.copysign(1, result[0].code[1]) == -1 and math.isinf(result[0].code[2])
    with pytest.raises(ProtocolError):
        command.parse_response(struct.pack("<H", 2) + body[2:])
    assert command.total == 2


def test_native_mac_file_meta_keeps_signed_flag_and_ascii_replacement():
    body = struct.pack("<IIb32s", 7, 123, -2, b"\0ab\xff")
    row = mac.FileListCmd("x").parse_response(body)
    assert (row.offset, row.size, row.flag, row.hash) == (7, 123, -2, "\0ab�")
    with pytest.raises(ProtocolError, match="FileListCmd"):
        mac.FileListCmd("x").parse_response(body[:-1])


@pytest.mark.parametrize("factory", [bytes, bytearray, memoryview])
def test_native_mac_download_response_retains_original_slice_type(factory):
    command = mac.FileDownloadCmd("x")
    source = factory(b"\0" * 8 + b"payload")
    result = command.parse_response(source)
    assert type(result) is type(source) and result == source[8:]
    assert command.parse_response(factory(b"short")) == b""
