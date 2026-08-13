from __future__ import annotations

import struct
from datetime import date

import pytest

from eltdx.exceptions import ProtocolError, UnsupportedCommandError
from eltdx.protocol.commands import COMMANDS, CommandSpec, build_command_frame, parse_command_response
from eltdx.protocol.commands.security import parse_security_list_payload
from eltdx.protocol.constants import (
    TYPE_AUCTION_SERIES,
    TYPE_CAPITAL_CHANGES,
    TYPE_CATEGORY_QUOTES,
    TYPE_FINANCE_BATCH,
    TYPE_FILE_CONTENT,
    TYPE_HANDSHAKE,
    TYPE_HEARTBEAT,
    TYPE_HISTORICAL_INTRADAY,
    TYPE_HISTORICAL_TICKS,
    TYPE_INTRADAY_AUX,
    TYPE_KLINES,
    TYPE_LEGACY_QUOTES,
    TYPE_REFRESH_STREAM,
    TYPE_RECENT_INTRADAY,
    TYPE_SECURITY_COUNT,
    TYPE_SECURITY_LIST,
    TYPE_SNAPSHOTS,
    TYPE_SPARKLINE,
    TYPE_SPECIAL_LIMITS,
    TYPE_TODAY_INTRADAY,
    TYPE_TODAY_TICKS,
)
from eltdx.protocol.frame import RequestFrame, ResponseFrame
from eltdx.protocol.unit import get_volume, normalize_code


def test_request_frame_bytes_match_7709_header() -> None:
    frame = RequestFrame(msg_id=123, msg_type=TYPE_SECURITY_COUNT, data=bytes.fromhex("0000a7263501"))

    assert frame.to_bytes().hex() == "0c7b00000001080008004e040000a7263501"


def test_normalize_code_infers_bse_92_prefix_before_shanghai_9_prefix() -> None:
    assert normalize_code("920001") == "bj920001"
    assert normalize_code("900901") == "sh900901"
    assert normalize_code("bj920001") == "bj920001"


def test_all_current_7709_queries_explicitly_allow_one_safe_retry() -> None:
    assert COMMANDS
    assert all(spec.retry_safe is True for spec in COMMANDS.values())


def test_legacy_six_argument_command_spec_defaults_to_non_retryable() -> None:
    spec = CommandSpec(0x1234, "legacy", "legacy", "query", False, "legacy.md")

    assert spec.retry_safe is False


def test_builds_first_migrated_7709_commands() -> None:
    assert build_command_frame(TYPE_HANDSHAKE, {}, 1).to_bytes().hex() == "0c0100000001030003000d0001"
    assert build_command_frame(TYPE_HEARTBEAT, {}, 2).to_bytes().hex() == "0c0200000001020002000400"
    assert (
        build_command_frame(TYPE_SECURITY_COUNT, {"market": "sz", "client_date_yyyymmdd": 20260519}, 3)
        .to_bytes()
        .hex()
        == "0c0300000001080008004e040000a7263501"
    )
    assert (
        build_command_frame(TYPE_SECURITY_LIST, {"market": "bj", "start": 0, "limit": 1600}, 4).data.hex()
        == "0200000000004006000000000000"
    )


def test_build_snapshots_frame() -> None:
    frame = build_command_frame(TYPE_SNAPSHOTS, {"codes": ["sz000001", "sh600000", "bj899050"]}, 9)

    assert frame.msg_type == 0x054C
    assert frame.data.hex() == (
        "0500000000000000"
        "0300"
        "00303030303031"
        "01363030303030"
        "02383939303530"
    )


def test_refresh_stream_rejects_more_than_100_codes() -> None:
    codes = [f"sz{index:06d}" for index in range(101)]

    with pytest.raises(ValueError, match="at most 100 codes"):
        build_command_frame(TYPE_REFRESH_STREAM, {"codes": codes, "cursors": {}}, 10)


def test_build_legacy_quotes_and_file_content_frames() -> None:
    legacy = build_command_frame(TYPE_LEGACY_QUOTES, {"codes": ["sz000001", "sh600000"]}, 10)
    resource = build_command_frame(
        TYPE_FILE_CONTENT,
        {"path": "T0002\\hq_cache.dat", "offset": 30000, "size": 12000},
        11,
    )

    assert legacy.msg_type == 0x053E
    assert legacy.data.hex() == "050000000000000002000030303030303101363030303030"
    assert resource.msg_type == 0x06B9
    assert len(resource.data) == 308
    assert resource.data[:8] == (30000).to_bytes(4, "little") + (12000).to_bytes(4, "little")
    assert resource.data[8:].rstrip(b"\x00") == b"T0002/hq_cache.dat"


def test_file_content_frame_validates_path_and_size() -> None:
    with pytest.raises(ProtocolError, match="ASCII"):
        build_command_frame(TYPE_FILE_CONTENT, {"path": "统计.zip"}, 1)
    with pytest.raises(ProtocolError, match="must be > 0"):
        build_command_frame(TYPE_FILE_CONTENT, {"path": "zhb.zip", "size": 0}, 1)
    with pytest.raises(ProtocolError, match="exceeds 300"):
        build_command_frame(TYPE_FILE_CONTENT, {"path": "a" * 301}, 1)
    maximum = build_command_frame(TYPE_FILE_CONTENT, {"path": "zhb.zip", "size": 60000}, 1)
    assert maximum.data[:8] == b"\x00\x00\x00\x00" + (60000).to_bytes(4, "little")
    with pytest.raises(ProtocolError, match="<= 60000"):
        build_command_frame(TYPE_FILE_CONTENT, {"path": "zhb.zip", "size": 60001}, 1)
    with pytest.raises(ProtocolError, match="must be an integer"):
        build_command_frame(TYPE_FILE_CONTENT, {"path": "zhb.zip", "offset": 1.5}, 1)
    with pytest.raises(ProtocolError, match="must be an integer"):
        build_command_frame(TYPE_FILE_CONTENT, {"path": "zhb.zip", "offset": True}, 1)


def test_build_klines_frame_uses_7709_period_mapping() -> None:
    day = build_command_frame(TYPE_KLINES, {"code": "sz300308", "period": "day", "start": 0, "count": 420}, 10)
    qfq = build_command_frame(
        TYPE_KLINES,
        {"code": "sz000020", "period": "day", "start": 0, "count": 420, "adjust": "fixed_qfq", "anchor_date": 20070623},
        11,
    )
    minute10 = build_command_frame(TYPE_KLINES, {"code": "sz300308", "period": "10m", "start": 0, "count": 420}, 12)
    second5 = build_command_frame(TYPE_KLINES, {"code": "sz300308", "period": "5s", "start": 0, "count": 420}, 13)

    assert day.data.hex() == (
        "0000"
        "333030333038"
        "0400"
        "0100"
        "0000"
        "a401"
        "0000"
        "00000000"
        "0000000000000000000000000000000000000000"
    )
    assert qfq.data[16:22].hex() == "0300df403201"
    assert minute10.data[8:12].hex() == "08000a00"
    assert second5.data[8:12].hex() == "0d000500"


def test_protocol_builders_reject_counts_above_observed_server_limits() -> None:
    with pytest.raises(ValueError, match="between 1 and 800"):
        build_command_frame(TYPE_KLINES, {"code": "sz000001", "count": 801}, 1)
    with pytest.raises(ValueError, match="between 0 and 1600"):
        build_command_frame(TYPE_SECURITY_LIST, {"market": "sz", "limit": 1601}, 1)
    with pytest.raises(ValueError, match="between 1 and 1800"):
        build_command_frame(TYPE_TODAY_TICKS, {"code": "sz000001", "count": 1801}, 1)
    with pytest.raises(ValueError, match="between 1 and 1800"):
        build_command_frame(
            TYPE_HISTORICAL_TICKS,
            {"code": "sz000001", "trading_date": "2026-05-20", "count": 1801},
            1,
        )


def test_build_intraday_frames() -> None:
    today = build_command_frame(TYPE_TODAY_INTRADAY, {"code": "sz000988"}, 14)
    history = build_command_frame(TYPE_HISTORICAL_INTRADAY, {"code": "sz300308", "trading_date": 20260511}, 15)

    assert today.data.hex() == "000030303039383800000093"
    assert history.data.hex() == "9f26350100333030333038"


def test_build_remaining_active_7709_frames() -> None:
    today_ticks = build_command_frame(TYPE_TODAY_TICKS, {"code": "sz000001", "start": 0, "count": 115}, 1)
    history_ticks = build_command_frame(
        TYPE_HISTORICAL_TICKS,
        {"code": "sz300308", "trading_date": 20260511, "start": 0, "count": 900},
        1,
    )
    auction = build_command_frame(TYPE_AUCTION_SERIES, {"code": "sz000988"}, 1)
    recent = build_command_frame(TYPE_RECENT_INTRADAY, {"code": "sz300308", "trading_date": 20260511}, 1)
    aux = build_command_frame(TYPE_INTRADAY_AUX, {"code": "sz000988", "kind": "buy_sell_strength"}, 1)
    aux_volume = build_command_frame(TYPE_INTRADAY_AUX, {"code": "sz000988", "kind": "volume_comparison"}, 1)
    sparkline = build_command_frame(TYPE_SPARKLINE, {"code": "sz000001", "selector": 1}, 1)
    category = build_command_frame(TYPE_CATEGORY_QUOTES, {"category": 6, "sort_type": 0, "start": 0, "count": 42}, 1)
    refresh = build_command_frame(TYPE_REFRESH_STREAM, {"codes": ["sz000001"]}, 1)
    capital = build_command_frame(TYPE_CAPITAL_CHANGES, {"code": "sz000001"}, 1)
    finance = build_command_frame(TYPE_FINANCE_BATCH, {"codes": ["sz000001"]}, 1)
    limits = build_command_frame(TYPE_SPECIAL_LIMITS, {"start_index": 0}, 1)

    assert today_ticks.data.hex() == "000030303030303100007300"
    assert history_ticks.data.hex() == "9f263501000033303033303800008403"
    assert auction.data.hex() == "000030303039383800000000030000000000000000000000f4010000"
    assert recent.data.hex() == "61d9cafe00333030333038"
    assert aux.data.hex() == "00003030303938380000000000000000000000000000000000000000"
    assert aux_volume.data.hex() == "0000303030393838000000000000000000000000000000000000000b"
    assert sparkline.data.hex() == (
        "0000303030303031"
        "00000000000000000000000000000000"
        "0100140000000001"
        "0000000000"
    )
    assert category.data.hex() == "0600000000002a0000000500000001000000"
    assert refresh.data.hex() == "01000030303030303100000000"
    assert capital.data.hex() == "010000303030303031"
    assert finance.data.hex() == "010000303030303031"
    assert limits.data.hex() == "0000000000000000000000000000"


def test_parse_heartbeat_payload() -> None:
    response = ResponseFrame(
        control=0,
        msg_id=2,
        msg_type=TYPE_HEARTBEAT,
        zip_length=10,
        length=10,
        data=bytes.fromhex("000000000000a8263501"),
        raw=b"",
    )

    parsed = parse_command_response(TYPE_HEARTBEAT, response)

    assert parsed.reserved == b"\x00" * 6
    assert parsed.server_date_raw == 20260520
    assert parsed.server_date == date(2026, 5, 20)
    assert parsed.raw_payload == response.data


def test_parse_security_count_payload() -> None:
    response = ResponseFrame(
        control=0,
        msg_id=3,
        msg_type=TYPE_SECURITY_COUNT,
        zip_length=2,
        length=2,
        data=bytes.fromhex("f55a"),
        raw=b"",
    )

    assert parse_command_response(TYPE_SECURITY_COUNT, response) == 23285


def test_parse_security_list_payload() -> None:
    record = (
        b"000001"
        + (100).to_bytes(2, "little")
        + "平安银行".encode("gbk").ljust(16, b"\x00")
        + struct.pack("<f", 3956.656494)
        + b"\x02"
        + struct.pack("<f", 10.99)
        + bytes.fromhex("67316825")
    )
    payload = (1).to_bytes(2, "little") + record
    response = ResponseFrame(
        control=0,
        msg_id=4,
        msg_type=TYPE_SECURITY_LIST,
        zip_length=len(payload),
        length=len(payload),
        data=payload,
        raw=b"",
    )

    parsed = parse_security_list_payload(response, {"market": "sz"})

    assert len(parsed) == 1
    assert parsed[0].full_code == "sz000001"
    assert parsed[0].name == "平安银行"
    assert parsed[0].multiple == 100
    assert parsed[0].decimal == 2
    assert parsed[0].previous_close_price == pytest.approx(10.99)
    assert parsed[0].category == "a_share"
    assert parsed[0].board == "szse_main_board"
    assert parsed[0].unknown3_raw == bytes.fromhex("67316825")


def test_parse_security_list_rejects_truncated_payload() -> None:
    response = ResponseFrame(
        control=0,
        msg_id=4,
        msg_type=TYPE_SECURITY_LIST,
        zip_length=8,
        length=8,
        data=(1).to_bytes(2, "little") + b"000001",
        raw=b"",
    )

    with pytest.raises(ProtocolError, match="truncated security list payload"):
        parse_security_list_payload(response, {"market": "sz"})


def test_parse_trade_and_auction_payloads() -> None:
    today_payload = bytes.fromhex("0100 5003 0a 14 03 00 00")
    today = parse_command_response(
        TYPE_TODAY_TICKS,
        ResponseFrame(0, 1, TYPE_TODAY_TICKS, len(today_payload), len(today_payload), today_payload, b""),
        {"code": "sz000001", "start": 0, "count": 115},
    )

    assert today.full_code == "sz000001"
    assert today.count == 1
    assert today.ticks[0].time_label == "14:08"
    assert today.ticks[0].price == pytest.approx(0.1)
    assert today.ticks[0].volume == 20
    assert today.ticks[0].side == "buy"

    auction_tick_payload = bytes.fromhex("0100 2b02 00 8f0f 9603 08 00")
    auction_tick = parse_command_response(
        TYPE_TODAY_TICKS,
        ResponseFrame(0, 2, TYPE_TODAY_TICKS, len(auction_tick_payload), len(auction_tick_payload), auction_tick_payload, b""),
        {"code": "sz000001", "start": 0, "count": 115},
    )
    assert auction_tick.ticks[0].event_kind == "auction_snapshot"
    assert auction_tick.ticks[0].auction_matched_volume == 975
    assert auction_tick.ticks[0].auction_unmatched_signed_volume == 214
    assert auction_tick.ticks[0].volume == 975
    assert auction_tick.ticks[0].order_count == 214

    history_payload = (1).to_bytes(2, "little") + struct.pack("<f", 35.5) + bytes.fromhex("5003 0a 14 03 05 00")
    history = parse_command_response(
        TYPE_HISTORICAL_TICKS,
        ResponseFrame(0, 1, TYPE_HISTORICAL_TICKS, len(history_payload), len(history_payload), history_payload, b""),
        {"code": "sz300308", "trading_date": 20260511, "start": 0, "count": 900},
    )

    assert history.trading_date == date(2026, 5, 11)
    assert history.price_base_raw_f32 == pytest.approx(35.5)
    assert history.ticks[0].side == "status_5"
    assert history.ticks[0].reserved_zero == 0

    auction_record = bytes.fromhex("2b02b81e2243080a0000810900000000")
    auction_payload = (1).to_bytes(2, "little") + auction_record
    auction = parse_command_response(
        TYPE_AUCTION_SERIES,
        ResponseFrame(0, 1, TYPE_AUCTION_SERIES, len(auction_payload), len(auction_payload), auction_payload, b""),
        {"code": "sz000988"},
    )

    assert auction.full_code == "sz000988"
    assert auction.points[0].time_label == "09:15:00"
    assert auction.points[0].price == pytest.approx(162.119995)
    assert auction.points[0].matched_volume == 2568
    assert auction.points[0].unmatched_signed_raw == 2433


def test_parse_recent_aux_sparkline_payloads() -> None:
    recent_payload = (
        (1).to_bytes(2, "little")
        + struct.pack("<f", 10.0)
        + struct.pack("<f", 10.1)
        + bytes.fromhex("0a 0b 0c")
    )
    recent = parse_command_response(
        TYPE_RECENT_INTRADAY,
        ResponseFrame(0, 1, TYPE_RECENT_INTRADAY, len(recent_payload), len(recent_payload), recent_payload, b""),
        {"code": "sz300308", "trading_date": 20260511},
    )

    assert recent.trading_date == date(2026, 5, 11)
    assert recent.prev_close == pytest.approx(10.0)
    assert recent.open_price == pytest.approx(10.1)
    assert recent.points[0].price == pytest.approx(0.1)
    assert recent.points[0].avg_price == pytest.approx(0.0011)

    aux_payload = bytes.fromhex("0100 05 06")
    aux = parse_command_response(
        TYPE_INTRADAY_AUX,
        ResponseFrame(0, 1, TYPE_INTRADAY_AUX, len(aux_payload), len(aux_payload), aux_payload, b""),
        {"code": "sz000988", "kind": "buy_sell_strength"},
    )
    assert aux.kind == "buy_sell_strength"
    assert aux.points[0].buy_commission == 5
    assert aux.points[0].sell_commission == 6

    volume_aux_payload = (1).to_bytes(2, "little") + struct.pack("<ff", 100.5, 120.5)
    volume_aux = parse_command_response(
        TYPE_INTRADAY_AUX,
        ResponseFrame(0, 1, TYPE_INTRADAY_AUX, len(volume_aux_payload), len(volume_aux_payload), volume_aux_payload, b""),
        {"code": "sz000988", "kind": "volume_comparison"},
    )
    assert volume_aux.kind == "volume_comparison"
    assert volume_aux.points[0].previous_day_cumulative_volume == pytest.approx(100.5)
    assert volume_aux.points[0].current_day_cumulative_volume == pytest.approx(120.5)

    spark_header = (
        bytes([0, 0])
        + b"000001"
        + b"\x00" * 16
        + bytes([1, 0])
        + (0).to_bytes(4, "little")
        + (0).to_bytes(4, "little")
        + (60).to_bytes(2, "little")
        + struct.pack("<f", 10.0)
        + (2).to_bytes(2, "little")
    )
    spark_payload = spark_header + struct.pack("<ff", 10.0, 10.1)
    spark = parse_command_response(
        TYPE_SPARKLINE,
        ResponseFrame(0, 1, TYPE_SPARKLINE, len(spark_payload), len(spark_payload), spark_payload, b""),
        {"code": "sz000001", "selector": 1},
    )
    assert spark.full_code == "sz000001"
    assert spark.max_count_raw == 60
    assert spark.prices == pytest.approx((10.0, 10.1))


def test_parse_refresh_stream_empty_payload() -> None:
    payload = bytes.fromhex("9393")
    refresh = parse_command_response(
        TYPE_REFRESH_STREAM,
        ResponseFrame(0, 1, TYPE_REFRESH_STREAM, len(payload), len(payload), payload, b""),
        {"codes": ["sz000001"]},
    )

    assert refresh.count == 0
    assert refresh.requested_codes == ("sz000001",)
    assert refresh.decoded_payload == b"\x00\x00"


def test_parse_snapshot_keeps_only_confirmed_top_level_depth() -> None:
    record = bytes.fromhex(
        "00303030303031e61185115b5c005fa4a3cf0ec51187e9aa01bfe40e40afb44eb0994298cf6800"
        "b8df094100901381c3011614120010004091fc4c000000000000000000000000ca0b9f409ffa84c200"
        "00000000000000000000000000000000000000000000e611"
    )
    payload = b"\x00\x00\x01\x00" + record
    snapshot = parse_command_response(
        TYPE_SNAPSHOTS,
        ResponseFrame(0, 1, TYPE_SNAPSHOTS, len(payload), len(payload), payload, b""),
        {"codes": ["sz000001"]},
    )[0]

    assert len(snapshot.buy_levels) == 1
    assert len(snapshot.sell_levels) == 1
    assert snapshot.buy_levels[0].price == pytest.approx(10.92)
    assert snapshot.buy_levels[0].volume == 1232
    assert snapshot.sell_levels[0].price == pytest.approx(10.93)
    assert snapshot.sell_levels[0].volume == 12481
    assert snapshot.tail_raw.startswith(bytes.fromhex("1614120010004091fc4c"))


def test_parse_legacy_quotes_decodes_five_levels_and_status() -> None:
    record = _legacy_quote_record(0, "000001")
    payload = (0x0701).to_bytes(2, "little") + (1).to_bytes(2, "little") + record
    quote = parse_command_response(
        TYPE_LEGACY_QUOTES,
        ResponseFrame(0, 1, TYPE_LEGACY_QUOTES, len(payload), len(payload), bytes(payload), b""),
        {"codes": ["sz000001"]},
    )[0]

    assert quote.full_code == "sz000001"
    assert quote.last_price == pytest.approx(10.14)
    assert quote.pre_close_price == pytest.approx(10.0)
    assert [level.price for level in quote.buy_levels] == pytest.approx([10.13, 10.12, 10.11, 10.10, 10.09])
    assert [level.volume for level in quote.sell_levels] == [428, 260, 136, 92, 71]
    assert quote.sum_buy_vol == 685
    assert quote.sum_sell_vol == 987
    assert quote.trading_status_raw == 0x20
    assert quote.trading_status_hex == "0x0020"
    assert quote.tail_metrics_raw == (1, -2, 3, -4)
    assert quote.rise_speed_raw == 21
    assert quote.active2 == 8


def test_parse_legacy_quotes_supports_short_tail_and_etf_precision() -> None:
    stock = _legacy_quote_record(0, "000001", include_optional_tail=False)
    etf = _legacy_quote_record(1, "510300", close_raw=1014)
    payload = (0x0701).to_bytes(2, "little") + (2).to_bytes(2, "little") + stock + etf
    quotes = parse_command_response(
        TYPE_LEGACY_QUOTES,
        ResponseFrame(0, 1, TYPE_LEGACY_QUOTES, len(payload), len(payload), payload, b""),
        {"codes": ["sz000001", "sh510300"]},
    )

    assert [quote.full_code for quote in quotes] == ["sz000001", "sh510300"]
    assert quotes[0].rise_speed_raw is None
    assert quotes[0].active2 is None
    assert quotes[1].last_price == pytest.approx(1.014)
    assert quotes[1].buy_levels[0].price == pytest.approx(1.013)


def test_parse_file_content_returns_exact_chunk() -> None:
    content = b"abc123"
    payload = len(content).to_bytes(4, "little") + content + b"\xaa\xbb"
    chunk = parse_command_response(
        TYPE_FILE_CONTENT,
        ResponseFrame(0, 1, TYPE_FILE_CONTENT, len(payload), len(payload), payload, b""),
        {"path": "zhb.zip", "offset": 10, "size": 30000},
    )

    assert chunk.path == "zhb.zip"
    assert chunk.offset == 10
    assert chunk.request_size == 30000
    assert chunk.chunk_len == 6
    assert chunk.content == content
    assert chunk.is_last is True
    assert chunk.raw_payload == payload


def test_parse_file_content_rejects_invalid_lengths() -> None:
    with pytest.raises(ProtocolError, match="expected 10, got 5"):
        parse_command_response(
            TYPE_FILE_CONTENT,
            ResponseFrame(0, 1, TYPE_FILE_CONTENT, 5, 5, b"\x06\x00\x00\x00a", b""),
            {"path": "zhb.zip", "size": 30000},
        )
    with pytest.raises(ProtocolError, match="exceeds requested size"):
        payload = (2).to_bytes(4, "little") + b"ab"
        parse_command_response(
            TYPE_FILE_CONTENT,
            ResponseFrame(0, 1, TYPE_FILE_CONTENT, len(payload), len(payload), payload, b""),
            {"path": "zhb.zip", "size": 1},
        )


def test_parse_corporate_finance_and_limits_payloads() -> None:
    capital_record = (
        bytes([0])
        + b"000001"
        + b"\x00"
        + (20260511).to_bytes(4, "little")
        + bytes([15])
        + struct.pack("<ffff", 0.0, 0.0, 3.5, 0.0)
    )
    capital_payload = (1).to_bytes(2, "little") + bytes([0]) + b"000001" + (1).to_bytes(2, "little") + capital_record
    capital = parse_command_response(
        TYPE_CAPITAL_CHANGES,
        ResponseFrame(0, 1, TYPE_CAPITAL_CHANGES, len(capital_payload), len(capital_payload), capital_payload, b""),
        {"code": "sz000001"},
    )
    assert capital.full_code == "sz000001"
    assert capital.records[0].category_name == "重整调整"
    assert capital.records[0].c3_float == pytest.approx(3.5)
    assert capital.records[0].c3_value == pytest.approx(3.5)

    equity_record = (
        bytes([0])
        + b"000001"
        + b"\x00"
        + (20260512).to_bytes(4, "little")
        + bytes([5])
        + bytes.fromhex("01020304")
        + bytes.fromhex("05060708")
        + bytes.fromhex("090a0b0c")
        + bytes.fromhex("0d0e0f10")
    )
    equity_payload = (1).to_bytes(2, "little") + bytes([0]) + b"000001" + (1).to_bytes(2, "little") + equity_record
    equity = parse_command_response(
        TYPE_CAPITAL_CHANGES,
        ResponseFrame(0, 1, TYPE_CAPITAL_CHANGES, len(equity_payload), len(equity_payload), equity_payload, b""),
        {"code": "sz000001"},
    )
    assert equity.records[0].c1_value == pytest.approx(get_volume(0x04030201) * 10000.0)
    assert equity.records[0].c4_value == pytest.approx(get_volume(0x100F0E0D) * 10000.0)

    finance_info = struct.pack(
        "<fHHII30f",
        100.0,
        1,
        2,
        20260425,
        19910403,
        *([0.0] * 30),
    )
    finance_record = bytes([0]) + b"000001" + finance_info
    finance_payload = (1).to_bytes(2, "little") + finance_record
    finance = parse_command_response(
        TYPE_FINANCE_BATCH,
        ResponseFrame(0, 1, TYPE_FINANCE_BATCH, len(finance_payload), len(finance_payload), finance_payload, b""),
        {"codes": ["sz000001"]},
    )
    assert finance.count == 1
    assert finance.records[0].circulating_shares == pytest.approx(1_000_000)
    assert finance.records[0].updated_date == date(2026, 4, 25)
    assert finance.records[0].ipo_date == date(1991, 4, 3)

    limit_record = bytes([0]) + (123054).to_bytes(4, "little") + struct.pack("<ff", 212.531, 141.687)
    limit_payload = (1).to_bytes(2, "little") + limit_record
    limits = parse_command_response(
        TYPE_SPECIAL_LIMITS,
        ResponseFrame(0, 1, TYPE_SPECIAL_LIMITS, len(limit_payload), len(limit_payload), limit_payload, b""),
        {"start_index": 2},
    )
    assert limits.records[0].full_code == "sz123054"
    assert limits.records[0].limit_up_price == pytest.approx(212.531, rel=1e-6)


def test_unmigrated_7709_command_is_explicit() -> None:
    with pytest.raises(UnsupportedCommandError, match="0x9999"):
        build_command_frame(0x9999, {"code": "sz000001"}, 1)


def _signed_varint(value: int) -> bytes:
    sign = 0x40 if value < 0 else 0
    remaining = abs(value)
    first = (remaining & 0x3F) | sign
    remaining >>= 6
    if remaining:
        first |= 0x80
    output = [first]
    while remaining:
        byte = remaining & 0x7F
        remaining >>= 7
        if remaining:
            byte |= 0x80
        output.append(byte)
    return bytes(output)


def _legacy_quote_record(
    market_id: int,
    code: str,
    *,
    close_raw: int = 1014,
    include_optional_tail: bool = True,
) -> bytes:
    levels = [
        (-1, 0, 320, 428),
        (-2, 1, 118, 260),
        (-3, 2, 94, 136),
        (-4, 3, 87, 92),
        (-5, 4, 66, 71),
    ]
    record = bytearray(bytes([market_id]) + code.encode("ascii") + (7).to_bytes(2, "little"))
    for value in (close_raw, -14, -1, 6, -10, 103000, -close_raw, 1000, 15):
        record.extend(_signed_varint(value))
    record.extend((12345678).to_bytes(4, "little"))
    for value in (400, 600, 0, 100):
        record.extend(_signed_varint(value))
    for values in levels:
        for value in values:
            record.extend(_signed_varint(value))
    record.extend((0x20).to_bytes(2, "little"))
    for value in (1, -2, 3, -4):
        record.extend(_signed_varint(value))
    if include_optional_tail:
        record.extend((21).to_bytes(2, "little", signed=True))
        record.extend((8).to_bytes(2, "little"))
    return bytes(record)
