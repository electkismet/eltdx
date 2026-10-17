"""MAC 协议栈（eltdx.mac）单元测试。

覆盖：帧编解码往返与异常、FieldBit 位图合成字节、各命令 build/parse、
本地 QFQ 负价兜底重算、以及 fake socket 传输层（setup/收发/超时）。

全部测试离线运行，**不得真实联网**：传输层统一注入斐波 socket。
"""

from __future__ import annotations

import socket
import json
import struct
import zlib
from datetime import date, datetime, timedelta, time

import pytest

from eltdx.exceptions import (
    ConnectionClosedError,
    ProtocolError,
    ResponseTimeoutError,
    TransportError,
)
from eltdx.mac.adjust import (
    XdxrEvent,
    apply_forward_adjust,
    compute_forward_factor,
    has_bad_prices,
)
from eltdx.mac.bitmap import (
    BITMAP_SIZE,
    FIELD_BIT_LIMIT,
    FieldBit,
    PresetField,
    build_bitmap,
    count_set_bits,
    get_active_fields,
    iter_set_bits,
)
from eltdx.mac.client import MacClient, convert_board_code
from eltdx.mac.codec import build_mac_request, decompress_mac_body, parse_mac_header
from eltdx.mac.commands import (
    BoardListCmd,
    BoardMembersQuotesCmd,
    ChartSamplingCmd,
    FileDownloadCmd,
    FileListCmd,
    GoodsListCmd,
    KlineOffsetCmd,
    ServerInfoCmd,
    SymbolAuctionCmd,
    SymbolBarCmd,
    SymbolBelongBoardCmd,
    SymbolCapitalFlowCmd,
    SymbolInfoCmd,
    SymbolQuotesCmd,
    SymbolTickChartCmd,
    SymbolTransactionCmd,
    TickChartsCmd,
    UnusualCmd,
)
from eltdx.mac.enums import Adjust, BoardType, Category, Period
from eltdx.mac.hosts import MAC_HOSTS, MAC_PORT, MAC_TIMEOUT
from eltdx.mac.models import BoardInfo, MacQuoteField
from eltdx.protocol.decompress import (
    MAX_FRAME_UNZIPPED_BYTES,
    DecompressLimitError,
    decompress_limited,
)
from eltdx.mac.models import MacBar
from eltdx.mac.transport import SETUP_COMMANDS, MacConnection, ping_mac_all, ping_mac_host

# --------------------------------------------------------------------------- #
# 辅助
# --------------------------------------------------------------------------- #


def assert_mac_frame(req: bytes, msg_id: int, head_flag: int = 0x1C) -> None:
    """断言请求帧的 10 字节 MAC 头与 msg_id 前缀。"""

    assert len(req) >= 12
    assert req[0] == head_flag
    assert struct.unpack_from("<I", req, 1)[0] == 0  # customize
    assert req[5] == 1  # version
    inner_len = len(req) - 10
    assert struct.unpack_from("<H", req, 6)[0] == inner_len  # zipsize
    assert struct.unpack_from("<H", req, 8)[0] == inner_len  # unzipsize
    assert struct.unpack_from("<H", req, 10)[0] == msg_id


def response_frame(payload: bytes, *, compress: bool = False) -> bytes:
    """构造一个 16 字节响应帧头 + body（可选 zlib 压缩）。"""

    if compress:
        compressed = zlib.compress(payload)
        return struct.pack("<IIIHH", 7654321, 0, 0, len(compressed), len(payload)) + compressed
    return struct.pack("<IIIHH", 7654321, 0, 0, len(payload), len(payload)) + payload


def _bars(closes: list[float], start: date = date(2024, 1, 1)) -> list[MacBar]:
    """构造最小 K 线：OHLC 全等于给定 close 序列，逐日递增。"""

    base = datetime(start.year, start.month, start.day)
    return [
        MacBar(
            datetime=base + timedelta(days=i),
            open=c,
            high=c,
            low=c,
            close=c,
            vol=100.0,
            amount=0.0,
        )
        for i, c in enumerate(closes)
    ]


# --------------------------------------------------------------------------- #
# 帧编解码
# --------------------------------------------------------------------------- #


def test_build_mac_request_layout() -> None:
    req = build_mac_request(0x122B, b"\x01\x02\x03")
    assert len(req) == 10 + 2 + 3
    assert req[0] == 0x1C
    assert req[10:12] == struct.pack("<H", 0x122B)
    assert req[12:] == b"\x01\x02\x03"


def test_build_mac_request_custom_head_flag() -> None:
    req = build_mac_request(0x1218, b"", head_flag=1)
    assert req[0] == 1
    assert struct.unpack_from("<H", req, 6)[0] == 2


def test_parse_header_uncompressed_roundtrip() -> None:
    payload = b"hello-mac"
    frame = response_frame(payload)
    hdr = parse_mac_header(frame)
    assert hdr.zipsize == len(payload)
    assert hdr.unzipsize == len(payload)
    assert decompress_mac_body(hdr, frame[16:]) == payload


def test_decompress_mac_body_zlib() -> None:
    payload = b"x" * 1000
    frame = response_frame(payload, compress=True)
    hdr = parse_mac_header(frame)
    assert hdr.zipsize < hdr.unzipsize
    assert decompress_mac_body(hdr, frame[16:]) == payload


def test_decompress_body_length_mismatch_raises() -> None:
    hdr = parse_mac_header(response_frame(b"abcd"))
    with pytest.raises(ProtocolError):
        decompress_mac_body(hdr, b"ab")


def test_parse_header_short_buffer_raises() -> None:
    with pytest.raises(ProtocolError):
        parse_mac_header(b"\x00" * 8)


# --------------------------------------------------------------------------- #
# FieldBit 位图
# --------------------------------------------------------------------------- #


def test_fieldbit_key_fields() -> None:
    assert FieldBit.CLOSE.value == 0x04  # 最新价
    assert FieldBit.BUY_PRICE_LIMIT.value == 0x20  # 涨停价
    assert FieldBit.SELL_PRICE_LIMIT.value == 0x21  # 跌停价
    assert FieldBit.BID_ASK_RATIO.value == 0x39  # 委比
    assert FieldBit.MAIN_NET_AMOUNT.value == 0x38  # 主力净流入
    assert FieldBit.CLOSE.fmt == "<f"
    assert FieldBit.VOL.fmt == "<I"
    assert FieldBit.CLOSE.field_name == "close"


def test_build_bitmap_sets_expected_bits() -> None:
    fields = [
        FieldBit.CLOSE,
        FieldBit.BUY_PRICE_LIMIT,
        FieldBit.SELL_PRICE_LIMIT,
        FieldBit.BID_ASK_RATIO,
        FieldBit.MAIN_NET_AMOUNT,
    ]
    bm = build_bitmap(fields)
    assert len(bm) == 20
    value = int.from_bytes(bytes(bm[:16]), "little")
    for bit in fields:
        assert value & (1 << bit.value)
    assert value.bit_count() == 5


def test_get_active_fields_synthetic_bytes() -> None:
    value = (1 << FieldBit.CLOSE.value) | (1 << FieldBit.BUY_PRICE_LIMIT.value)
    active = get_active_fields(value.to_bytes(16, "little"))
    assert [f.field_name for f, _ in active] == ["close", "buy_price_limit"]
    assert [fmt for _, fmt in active] == ["<f", "<f"]


def test_bitmap_debug_all_ones() -> None:
    assert bytes(build_bitmap(PresetField.DEBUG)) == b"\xff" * 20


# --------------------------------------------------------------------------- #
# 高位字段（>=128）编码：协议实测字段位为 0..150
# --------------------------------------------------------------------------- #

#: 实测（2026-10-16）：字段位合法范围 0–150，位 151–159 为保留区。
#: 早期实现按「16 字节字段位图 + 4 字节控制区」编码，`>=128` 的字段一律
#: OverflowError，导致 HANDICAP / BOARD_STATS / ALL 三个公开预设完全不可用。
_HIGH_BIT_FIELDS = [f for f in FieldBit if f.value >= 128]
assert _HIGH_BIT_FIELDS  # 确实存在高位字段，否则这组测试失去意义


def test_build_bitmap_encodes_high_bit_fields() -> None:
    """位 >=128 的字段必须能被编码进 20 字节位图，而不是抛 OverflowError。"""

    bm = build_bitmap([FieldBit.BID3_PRICE])  # 0x80
    assert len(bm) == BITMAP_SIZE
    value = int.from_bytes(bytes(bm), "little")
    assert value == 1 << 0x80


@pytest.mark.parametrize(
    "preset_name",
    ["HANDICAP", "BOARD_STATS", "ALL", "COMMON", "BASIC", "QUOTE", "VOLUME", "ENHANCED"],
)
def test_every_preset_is_encodable(preset_name: str) -> None:
    """所有公开预设都必须能编码 —— 回归 HANDICAP/BOARD_STATS/ALL 的 OverflowError。"""

    preset = getattr(PresetField, preset_name)
    bm = build_bitmap(preset)
    assert len(bm) == BITMAP_SIZE


def test_handicap_preset_covers_all_five_levels() -> None:
    """HANDICAP 的买三~买五/卖三~卖五落在 0x80 以上，正是原先崩溃的那批位。"""

    value = int.from_bytes(bytes(build_bitmap(PresetField.HANDICAP)), "little")
    for field in (FieldBit.BID3_PRICE, FieldBit.BID5_PRICE, FieldBit.ASK3_PRICE, FieldBit.ASK5_PRICE):
        assert value >> field.value & 1


def test_field_bit_limit_matches_max_field_bit() -> None:
    """字段位上界 = 最大 FieldBit + 1（实测 151），保留区从 151 开始。"""

    assert FIELD_BIT_LIMIT == max(int(f) for f in FieldBit) + 1
    assert max(int(f) for f in FieldBit) == 0x96


def test_reserved_bits_do_not_occupy_slots() -> None:
    """位 151–159 是保留区，不占记录槽位。

    回归：若不屏蔽，一个保留位置位就会凭空多出一个槽位，把后续字段整体推错 ——
    与「未知字段错位」同一类故障。
    """

    bitmap = bytearray(BITMAP_SIZE)
    bitmap[FieldBit.PERCENT_BASE.value // 8] |= 1 << (FieldBit.PERCENT_BASE.value % 8)
    assert count_set_bits(bytes(bitmap)) == 1

    for reserved in (151, 152, 159):
        bitmap[reserved // 8] |= 1 << (reserved % 8)
    assert count_set_bits(bytes(bitmap)) == 1
    assert [slot for slot, _, _ in iter_set_bits(bytes(bitmap))] == [0]


def test_high_bit_field_occupies_a_slot_in_response() -> None:
    """响应里高位字段（0x80）同样占一个槽位，解析不得错位。"""

    bits = [FieldBit.BID_PRICE.value, FieldBit.BID3_PRICE.value]  # 0x11, 0x80
    bitmap = bytearray(20)
    for b in bits:
        bitmap[b // 8] |= 1 << (b % 8)
    body = bytes(bitmap) + struct.pack("<IH", 1, 1)
    body += struct.pack("<H22s44s", 0, b"600000", "浦发银行".encode("gbk"))
    body += struct.pack("<f", 11.11) + struct.pack("<f", 3.33)
    res = SymbolQuotesCmd([(0, "600000")]).parse_response(body)
    assert len(res) == 1
    assert res[0].fields["bid_price"] == pytest.approx(11.11)
    assert res[0].fields["bid3_price"] == pytest.approx(3.33)


def test_symbol_quotes_wire_uses_full_20_byte_bitmap() -> None:
    """0x122B 的 body 就是「20 字节位图 + 总数 + 股票」，不多不少。

    回归：把 20 字节当成「16 位图 + 4 控制」再追加控制区会让 body 多出 4 字节。
    """

    cmd = SymbolQuotesCmd([(0, "000001")], PresetField.BASIC)
    req = cmd.build_request()
    body = req[12:]
    assert len(body) == 20 + 2 + 24
    assert body[:20] == bytes(build_bitmap(PresetField.BASIC))


def test_board_members_rejects_high_bit_fields() -> None:
    """0x122C 的位图只有 16 字节（末 4 字节被 exclude_flags 占用），
    请求高位字段必须显式报错，而不能静默截断。"""

    with pytest.raises(ValueError, match="超出 16 字节位图"):
        BoardMembersQuotesCmd(21001, fields=PresetField.HANDICAP).build_request()


def test_board_members_low_bit_fields_still_work() -> None:
    """0x122C 用位 0–127 内的字段时行为不变：16 字节位图 + 4 字节控制区。"""

    cmd = BoardMembersQuotesCmd(21001, fields=[FieldBit.CLOSE])
    body = cmd.build_request()[12:]
    # 23 字节参数 + 16 字节位图 + 4 字节控制区
    assert len(body) == 23 + 16 + 4
    assert body[23:39] == bytes(build_bitmap([FieldBit.CLOSE], width=16))


def test_preset_field_combine_dedup() -> None:
    selection = PresetField.BASIC + FieldBit.CLOSE + FieldBit.AH_CODE
    names = [bit.name for bit in selection]
    assert names.count("CLOSE") == 1
    assert "AH_CODE" in names


# --------------------------------------------------------------------------- #
# 本地 QFQ 重算
# --------------------------------------------------------------------------- #


def test_factor_pure_cash_dividend() -> None:
    assert compute_forward_factor(10.0, 2.0, 0.0, 0.0, 0.0) == 0.8


def test_factor_songzhuangu_split() -> None:
    assert compute_forward_factor(10.0, 0.0, 0.0, 1.0, 0.0) == 0.5


def test_factor_invalid_returns_nan() -> None:
    import math

    assert math.isnan(compute_forward_factor(0.0, 1.0, 0.0, 0.0, 0.0))
    assert math.isnan(compute_forward_factor(-1.0, 1.0, 0.0, 0.0, 0.0))
    assert math.isnan(compute_forward_factor(10.0, 0.0, 0.0, -1.0, 0.0))


def test_apply_pure_cash_dividend_scales_pre_event_only() -> None:
    out = apply_forward_adjust(_bars([10.0, 10.0, 8.0, 9.0]), [XdxrEvent(date(2024, 1, 3), fenhong=2.0)])
    assert [b.close for b in out] == [8.0, 8.0, 8.0, 9.0]


def test_apply_latest_price_anchored() -> None:
    out = apply_forward_adjust(_bars([20.0, 10.0, 11.0]), [XdxrEvent(date(2024, 1, 2), fenhong=10.0)])
    assert out[0].close == 10.0
    assert out[-1].close == 11.0


def test_apply_no_events_returns_unchanged() -> None:
    out = apply_forward_adjust(_bars([10.0, 11.0, 12.0]), [])
    assert [b.close for b in out] == [10.0, 11.0, 12.0]


def test_apply_nan_factor_event_skipped() -> None:
    out = apply_forward_adjust(_bars([0.0, 5.0, 6.0]), [XdxrEvent(date(2024, 1, 2), fenhong=1.0)])
    assert [b.close for b in out] == [0.0, 5.0, 6.0]


def test_apply_multiple_events_cumulative() -> None:
    events = [
        XdxrEvent(date(2024, 1, 2), fenhong=2.0),
        XdxrEvent(date(2024, 1, 4), fenhong=2.0),
    ]
    out = apply_forward_adjust(_bars([20.0, 18.0, 12.0, 10.0, 10.0]), events)
    closes = [b.close for b in out]
    assert round(closes[0], 4) == 15.0
    assert round(closes[2], 4) == 10.0
    assert closes[3] == 10.0
    assert closes[4] == 10.0


def test_apply_songzhuangu_halves_pre_event_prices() -> None:
    out = apply_forward_adjust(_bars([20.0, 20.0, 10.0, 11.0]), [XdxrEvent(date(2024, 1, 3), songzhuangu=1.0)])
    assert [b.close for b in out] == [10.0, 10.0, 10.0, 11.0]


def test_apply_does_not_mutate_input() -> None:
    bars = _bars([10.0, 10.0, 8.0])
    apply_forward_adjust(bars, [XdxrEvent(date(2024, 1, 3), fenhong=2.0)])
    assert [b.close for b in bars] == [10.0, 10.0, 8.0]


def test_has_bad_prices() -> None:
    assert has_bad_prices(_bars([-0.5, 1.0])) is True
    assert has_bad_prices(_bars([0.0, 1.0])) is True
    assert has_bad_prices(_bars([float("nan")])) is True
    assert has_bad_prices(_bars([1.0, 2.0, 0.5])) is False

# --------------------------------------------------------------------------- #
# 命令 build（字节级断言）
# --------------------------------------------------------------------------- #


def test_build_symbol_quotes() -> None:
    cmd = SymbolQuotesCmd([(0, "600000")], fields=PresetField.BASIC)
    req = cmd.build_request()
    assert_mac_frame(req, 0x122B)
    body = req[12:]
    assert body[:20] == bytes(build_bitmap(PresetField.BASIC))
    assert struct.unpack_from("<H", body, 20)[0] == 1
    assert struct.unpack_from("<H", body, 22)[0] == 0
    assert body[24:46] == b"600000".ljust(22, b"\x00")


def test_symbol_quotes_empty_raises() -> None:
    with pytest.raises(ValueError):
        SymbolQuotesCmd([])


def test_build_board_members_quotes() -> None:
    req = BoardMembersQuotesCmd(881001, fields=PresetField.BASIC).build_request()
    assert_mac_frame(req, 0x122C)
    assert struct.unpack_from("<I", req, 12)[0] == 881001


def test_build_symbol_bar() -> None:
    req = SymbolBarCmd(0, "600000", Period.DAILY, count=100).build_request()
    assert_mac_frame(req, 0x122E)
    assert struct.unpack_from("<H", req, 12)[0] == 0
    assert req[14:20] == b"600000"


def test_build_symbol_tick_chart() -> None:
    req = SymbolTickChartCmd(0, "600000").build_request()
    assert_mac_frame(req, 0x122D)


def test_build_tick_charts() -> None:
    req = TickChartsCmd(0, "600000", days=5).build_request()
    assert_mac_frame(req, 0x123E)


def test_build_symbol_transaction() -> None:
    req = SymbolTransactionCmd(0, "600000").build_request()
    assert_mac_frame(req, 0x122F)


def test_build_symbol_info() -> None:
    req = SymbolInfoCmd(0, "600000").build_request()
    assert_mac_frame(req, 0x122A)


def test_build_board_list() -> None:
    req = BoardListCmd(BoardType.HY).build_request()
    assert_mac_frame(req, 0x1231)


def test_build_symbol_belong_board_head_flag() -> None:
    req = SymbolBelongBoardCmd(0, "600000").build_request()
    assert_mac_frame(req, 0x1218, head_flag=1)
    assert req[38:48] == b"Stock_GLHQ"


def test_build_symbol_capital_flow_head_flag() -> None:
    req = SymbolCapitalFlowCmd(0, "600000").build_request()
    assert_mac_frame(req, 0x1218, head_flag=2)
    assert req[38:48] == b"Stock_ZJLX"


def test_build_symbol_auction() -> None:
    req = SymbolAuctionCmd(0, "600000").build_request()
    assert_mac_frame(req, 0x123D)


def test_build_unusual() -> None:
    req = UnusualCmd(0).build_request()
    assert_mac_frame(req, 0x1237)


def test_build_server_info() -> None:
    req = ServerInfoCmd().build_request()
    assert_mac_frame(req, 0x120F)
    assert req[12 : 12 + 4] == bytes.fromhex("04002d31")


def test_build_kline_offset() -> None:
    req = KlineOffsetCmd().build_request()
    assert_mac_frame(req, 0x124A)


def test_build_chart_sampling() -> None:
    req = ChartSamplingCmd(0, "600000").build_request()
    assert_mac_frame(req, 0x254D)


def test_build_goods_list() -> None:
    req = GoodsListCmd(4).build_request()
    assert_mac_frame(req, 0x2562)


def test_goods_list_count_limit() -> None:
    with pytest.raises(ValueError):
        GoodsListCmd(4, count=2000)


def test_build_file_list() -> None:
    req = FileListCmd("tdxhy.cfg").build_request()
    assert_mac_frame(req, 0x1215)
    assert req[12 + 4 : 12 + 4 + 9] == b"tdxhy.cfg"


def test_build_file_download() -> None:
    req = FileDownloadCmd("tdxhy.cfg").build_request()
    assert_mac_frame(req, 0x1217)

# --------------------------------------------------------------------------- #
# 命令 parse（合成字节）
# --------------------------------------------------------------------------- #


def test_parse_symbol_quotes() -> None:
    bitmap = (
        (1 << FieldBit.CLOSE.value) | (1 << FieldBit.BUY_PRICE_LIMIT.value)
    ).to_bytes(16, "little")
    body = bytes(bitmap) + b"\x00" * 4
    body += struct.pack("<IH", 1, 1)
    body += struct.pack("<H22s44s", 0, b"600000", "浦发银行".encode("gbk"))
    body += struct.pack("<f", 10.5) + struct.pack("<f", 11.0)
    res = SymbolQuotesCmd([(0, "600000")]).parse_response(body)
    assert len(res) == 1
    assert res[0].code == "600000"
    assert res[0].name == "浦发银行"
    assert res[0].fields["close"] == 10.5
    assert res[0].fields["buy_price_limit"] == 11.0


def test_parse_board_members_quotes() -> None:
    bitmap = (1 << FieldBit.CLOSE.value).to_bytes(16, "little")
    body = bytes(bitmap) + b"\x00" * 4
    body += struct.pack("<IH", 1, 1)
    body += struct.pack("<H22s44s", 0, b"600000", "浦发银行".encode("gbk"))
    body += struct.pack("<f", 10.5)
    res = BoardMembersQuotesCmd(881001).parse_response(body)
    assert len(res) == 1
    assert res[0].code == "600000"
    assert res[0].fields["close"] == 10.5


# --------------------------------------------------------------------------- #
# 响应含未知字段时的槽位对齐（静默错价回归）
# --------------------------------------------------------------------------- #

#: 本模块未定义的位（线上可能有新字段置位）。取 0x2F：0x11 < 0x2F < 0x3f，
#: 能把两个已知字段夹在中间，从而暴露「未知位挤掉后续字段」的错位。
_UNKNOWN_BIT = 0x2F
assert _UNKNOWN_BIT not in FieldBit._value2member_map_
assert FieldBit.BID_PRICE.value < _UNKNOWN_BIT < FieldBit.PERCENT_BASE.value


def _quotes_body_with_unknown_field(rows: list[tuple[float, float, int]]) -> bytes:
    """构造含未知置位位的 symbol_quotes 响应体。

    线上按**位序**下发槽位，所以每行的三个值依次是
    ``BID_PRICE / <未知字段> / PERCENT_BASE``。
    """

    bitmap = (
        (1 << FieldBit.BID_PRICE.value)
        | (1 << _UNKNOWN_BIT)
        | (1 << FieldBit.PERCENT_BASE.value)
    ).to_bytes(16, "little")
    body = bytes(bitmap) + b"\x00" * 4
    body += struct.pack("<IH", len(rows), len(rows))
    for bid, unknown, percent_base in rows:
        body += struct.pack("<H22s44s", 0, b"600000", "浦发银行".encode("gbk"))
        body += struct.pack("<f", bid)
        body += struct.pack("<f", unknown)  # 未知字段的槽位，客户端不应命名它
        body += struct.pack("<I", percent_base)
    return body


def test_symbol_quotes_unknown_field_does_not_shift_known_fields() -> None:
    """未知置位字段占槽位，后续已知字段不得位移。

    回归：旧实现用 ``get_active_fields()`` 的长度算行宽、并按已知字段序号取偏移，
    未知字段被完全忽略 —— ``PERCENT_BASE`` 于是读到未知槽位的位模式（实测
    ``1120402145`` 而非 ``63``），且不抛任何异常。
    """

    body = _quotes_body_with_unknown_field([(11.11, 99.99, 63)])
    res = SymbolQuotesCmd([(0, "600000")]).parse_response(body)
    assert len(res) == 1
    assert res[0].fields["bid_price"] == pytest.approx(11.11)
    assert res[0].fields["percent_base"] == 63
    # 未知字段不产出任何键
    assert set(res[0].fields) == {"bid_price", "percent_base"}


def test_symbol_quotes_unknown_field_keeps_row_alignment() -> None:
    """多行记录时，未知字段不能让第 2 行整体错位。"""

    body = _quotes_body_with_unknown_field([(11.11, 99.9, 63), (22.22, 88.8, 64)])
    res = SymbolQuotesCmd([(0, "600000")]).parse_response(body)
    assert len(res) == 2
    assert res[0].fields["bid_price"] == pytest.approx(11.11)
    assert res[0].fields["percent_base"] == 63
    assert res[1].fields["bid_price"] == pytest.approx(22.22)
    assert res[1].fields["percent_base"] == 64


def test_board_members_unknown_field_does_not_shift_known_fields() -> None:
    """0x122C 走同样的位图解析，必须同样按槽位对齐。"""

    body = _quotes_body_with_unknown_field([(11.11, 99.9, 63), (22.22, 88.8, 64)])
    res = BoardMembersQuotesCmd(881001).parse_response(body)
    assert len(res) == 2
    assert res[0].fields["bid_price"] == pytest.approx(11.11)
    assert res[1].fields["percent_base"] == 64


def test_row_width_counts_all_set_bits() -> None:
    """行宽按全部置位位算，不是按已知字段数算。"""

    bitmap = (
        (1 << FieldBit.BID_PRICE.value)
        | (1 << _UNKNOWN_BIT)
        | (1 << FieldBit.PERCENT_BASE.value)
    ).to_bytes(16, "little")
    assert count_set_bits(bitmap) == 3
    assert len(get_active_fields(bitmap)) == 2  # 已知字段只有 2 个


def test_iter_set_bits_yields_unknown_slots_without_shifting() -> None:
    """``iter_set_bits`` 必须为未知位也产出槽位序号。"""

    bitmap = (
        (1 << FieldBit.BID_PRICE.value)
        | (1 << _UNKNOWN_BIT)
        | (1 << FieldBit.PERCENT_BASE.value)
    ).to_bytes(16, "little")
    slots = list(iter_set_bits(bitmap))
    assert [slot for slot, _, _ in slots] == [0, 1, 2]
    assert slots[0][1] is FieldBit.BID_PRICE
    assert slots[1][1] is None  # 未知位：占槽但无字段
    assert slots[2][1] is FieldBit.PERCENT_BASE


def test_bit_zero_field_is_not_treated_as_unknown() -> None:
    """``PRE_CLOSE`` 的位值是 0：任何 ``if field`` 式判断都会把它误判成未知。"""

    bitmap = (1 << 0).to_bytes(16, "little")
    slots = list(iter_set_bits(bitmap))
    assert len(slots) == 1
    assert slots[0][1] is FieldBit.PRE_CLOSE


def test_parse_board_list() -> None:
    rec = struct.pack(
        "<H6s16s44sfffH6s16s44sfff",
        0,
        b"881001",
        b"",
        "银行".encode("gbk"),
        10.0,
        1.0,
        9.5,
        0,
        b"600000",
        b"",
        "浦发银行".encode("gbk"),
        9.9,
        0.5,
        9.5,
    )
    body = struct.pack("<HH", 2, 1) + rec
    res = BoardListCmd().parse_response(body)
    assert len(res) == 1
    assert res[0].code == "881001"
    assert res[0].name == "银行"
    assert res[0].symbol_code == "600000"
    assert res[0].price == 10.0


def test_parse_symbol_bar() -> None:
    head = struct.pack("<H22s", 0, b"600000") + struct.pack("<HBHI", 4, 0, 1, 0)
    bar = struct.pack("<II7f", 20240102, 0, 10.0, 11.0, 9.0, 10.5, 1000.0, 200.0, 300.0)
    res = SymbolBarCmd(0, "600000").parse_response(head + bar)
    assert len(res) == 1
    assert res[0].datetime == datetime(2024, 1, 2)
    assert res[0].close == 10.5
    assert res[0].vol == 200.0


def test_parse_symbol_auction() -> None:
    body = struct.pack("<H22sI", 0, b"600000", 1) + b"\x00" * 8
    body += struct.pack("<IfIi", 34200, 10.5, 100, -50)
    res = SymbolAuctionCmd(0, "600000").parse_response(body)
    assert len(res) == 1
    assert res[0].time == time(9, 30)
    assert res[0].price == 10.5
    assert res[0].matched == 100
    assert res[0].unmatched == -50


def test_parse_symbol_transaction() -> None:
    body = b"\x00" * 29 + struct.pack("<H", 1) + b"\x00" * 8
    body += struct.pack("<IfIIH", 34200, 10.5, 100, 1, 0)
    res = SymbolTransactionCmd(0, "600000").parse_response(body)
    assert len(res) == 1
    assert res[0].price == 10.5
    assert res[0].vol == 100
    assert res[0].trade_count == 1
    assert res[0].bs_flag == 0


def test_parse_symbol_info() -> None:
    head = b"\x00" * 8
    head += struct.pack("<H22s44s", 0, b"600000", "浦发银行".encode("gbk"))
    head += b"\x00" * 20
    core = struct.pack(
        "<III5ffIfII",
        20240102,
        93000,
        7,
        9.0,
        9.5,
        10.0,
        8.8,
        9.8,
        0.1,
        12345,
        6789.0,
        100,
        200,
    )
    extra = struct.pack("<HIf20xI3f", 2, 1, 1.0, 3, 5.0, 6.0, 7.0)
    res = SymbolInfoCmd(0, "600000").parse_response(head + core + extra)
    assert res.code == "600000"
    assert res.name == "浦发银行"
    assert res.close == pytest.approx(9.8)
    assert res.vol == 12345
    assert res.turnover == 6.0
    assert res.avg == 7.0


def test_parse_kline_offset() -> None:
    res = KlineOffsetCmd().parse_response(struct.pack(">I", 1234) + struct.pack("<I", 5))
    assert res.total == 1234
    assert res.returned == 5
    short = KlineOffsetCmd().parse_response(b"\x00" * 4)
    assert short.total == 0 and short.returned == 0


def test_parse_chart_sampling() -> None:
    body = b"\x00" * 40 + struct.pack("<H", 2) + struct.pack("<ff", 1.5, 2.5)
    assert ChartSamplingCmd(0, "600000").parse_response(body) == [1.5, 2.5]
    assert ChartSamplingCmd(0, "600000").parse_response(b"\x00" * 10) == []


def test_parse_goods_list() -> None:
    record = struct.pack("<H23sHIBfffHH", 7, "螺纹钢".encode("gbk"), 1, 2, 3, 1.0, 2.0, 3.0, 4, 5)
    body = struct.pack("<H", 1) + record.ljust(48, b"\x00")
    res = GoodsListCmd(4).parse_response(body)
    assert len(res) == 1
    assert res[0].name == "螺纹钢"
    assert res[0].category == 7
    assert res[0].code == [1.0, 2.0, 3.0]
    assert res[0].c1 == 4 and res[0].c2 == 5


def test_parse_file_list() -> None:
    body = struct.pack("<IIb", 5, 1024, 0) + b"HASH".ljust(32, b"\x00")
    res = FileListCmd("x").parse_response(body)
    assert res.offset == 5 and res.size == 1024 and res.flag == 0 and res.hash == "HASH"


def test_parse_file_download() -> None:
    assert FileDownloadCmd("x").parse_response(b"12345678DATA") == b"DATA"
    assert FileDownloadCmd("x").parse_response(b"123") == b""


def test_parse_server_info() -> None:
    body = bytearray(87)
    struct.pack_into("<H", body, 0, 3)
    struct.pack_into("<I", body, 22, 20240102)
    struct.pack_into("<I", body, 26, 1700000000)
    struct.pack_into("<8H", body, 30, 570, 690, 780, 900, 0, 0, 0, 0)
    struct.pack_into("<8H", body, 46, 571, 691, 781, 901, 0, 0, 0, 0)
    body[62] = 1
    struct.pack_into("<I", body, 63, 20231229)
    struct.pack_into("<I", body, 67, 0)
    struct.pack_into("<I", body, 71, 111)
    struct.pack_into("<I", body, 75, 222)
    res = ServerInfoCmd().parse_response(bytes(body))
    assert res.today == "2024-01-02"
    assert res.last_trading_day == "2023-12-29"
    assert res.sessions_1[0] == {"open": "9:30", "close": "11:30"}
    assert res.market_param_1 == 111 and res.market_param_2 == 222
    empty = ServerInfoCmd().parse_response(b"\x00" * 10)
    assert empty.today == "" and empty.last_trading_day == ""


def test_parse_symbol_belong_board() -> None:
    rows = [[1, 0, "881001", "银行", 10.0, 9.5, 0, 0, 0]]
    body = b"\x00" * 27 + json.dumps(rows).encode("gbk")
    res = SymbolBelongBoardCmd(0, "600000").parse_response(body)
    assert len(res) == 1
    assert res[0].board_code == "881001"
    assert res[0].board_name == "银行"
    assert res[0].close == 10.0
    assert res[0].pre_close == 9.5


def test_parse_symbol_capital_flow() -> None:
    today = [100.0, 50.0, 30.0, 20.0]
    five = [0, 0, 0, 40.0, 60.0, 0]
    body = b"\x00" * 27 + json.dumps([today, five]).encode("gbk")
    res = SymbolCapitalFlowCmd(0, "600000").parse_response(body)
    assert res is not None
    assert res.main_net == 50.0
    assert res.small_net == 10.0
    assert res.mid_net == 60.0
    assert res.large_net == 40.0
    assert SymbolCapitalFlowCmd(0, "600000").parse_response(b"\x00" * 10) is None


def test_parse_unusual() -> None:
    record = struct.pack("<H6sBBBHH", 0, b"600000", 0, 0x04, 0, 5, 0)
    data = struct.pack("<B2fI", 0, 0.0123, 0.0, 0)
    record += data + b"\x00" + struct.pack("<BH", 9, 3000)
    body = struct.pack("<H", 1) + record + "浦发银行".encode("gbk")
    res = UnusualCmd(0).parse_response(body)
    assert len(res) == 1
    assert res[0].desc == "加速拉升"
    assert res[0].value == "1.23%"
    assert res[0].name == "浦发银行"
    assert res[0].time == time(9, 30)


def test_parse_symbol_tick_chart() -> None:
    hdr = struct.pack("<H22sIBfH", 0, b"600000", 20240102, 1, 9.5, 1)
    tick = struct.pack("<HffIf", 570, 9.8, 9.7, 100, 0.0)
    tail = struct.pack(
        "<44sBHf5x2I5ffIf12s2fI",
        "浦发银行".encode("gbk").ljust(44, b"\x00"),
        2,
        0,
        1.0,
        20240102,
        93000,
        9.5,
        9.6,
        10.0,
        9.4,
        9.9,
        0.1,
        12345,
        67890.0,
        b"",
        1.2,
        9.8,
        0,
    )
    res = SymbolTickChartCmd(0, "600000").parse_response(hdr + tick + tail)
    assert res.code == "600000"
    assert res.name == "浦发银行"
    assert res.close == pytest.approx(9.9)
    assert res.avg == pytest.approx(9.8)
    assert len(res.charts) == 1
    assert res.charts[0].price == pytest.approx(9.8)


def test_parse_tick_charts() -> None:
    hdr = struct.pack("<H22s", 0, b"600000")
    hdr += struct.pack("<5I", 20240102, 0, 0, 0, 0)
    hdr += struct.pack("<5f", 9.5, 0, 0, 0, 0)
    hdr += struct.pack("<HBHH", 1, 0, 100, 0)
    tail = struct.pack(
        "<44sBHf5x2I5ffIf12s2fI",
        "浦发银行".encode("gbk").ljust(44, b"\x00"),
        2,
        0,
        1.0,
        20240102,
        93000,
        9.5,
        9.6,
        10.0,
        9.4,
        9.9,
        0.1,
        12345,
        67890.0,
        b"",
        1.2,
        9.8,
        0,
    )
    res = TickChartsCmd(0, "600000").parse_response(hdr + tail)
    assert res.code == "600000"
    assert len(res.charts) == 1
    assert res.charts[0].date == date(2024, 1, 2)
    assert res.charts[0].pre_close == 9.5

# --------------------------------------------------------------------------- #
# 传输层（fake socket，离线）
# --------------------------------------------------------------------------- #


class FakeSocket:
    """最小 socket 替身：从 ``incoming`` 顺序供数，记录 ``sent`` 字节。"""

    def __init__(self, incoming: bytes = b"", *, connect_error=None, recv_timeout=False):
        self._buf = bytearray(incoming)
        self._pos = 0
        self.sent: list[bytes] = []
        self.closed = False
        self.timeout = None
        self.connected = False
        self._connect_error = connect_error
        self._recv_timeout = recv_timeout

    def settimeout(self, value):
        self.timeout = value

    def connect(self, address):
        if self._connect_error is not None:
            raise self._connect_error
        self.connected = True

    def sendall(self, data):
        self.sent.append(bytes(data))

    def recv(self, n):
        if self._recv_timeout:
            raise socket.timeout("timed out")
        if self._pos >= len(self._buf):
            return b""
        chunk = bytes(self._buf[self._pos : self._pos + n])
        self._pos += len(chunk)
        return chunk

    def close(self):
        self.closed = True


class _ScriptFactory:
    """按顺序返回预置 socket 的工厂。"""

    def __init__(self, sockets):
        self._sockets = list(sockets)

    def __call__(self, family, type_):
        return self._sockets.pop(0)


def _handshake_bytes() -> bytes:
    return b"\x00" * (16 * len(SETUP_COMMANDS))


def test_connection_setup_sends_three_commands() -> None:
    fake = FakeSocket(_handshake_bytes())
    conn = MacConnection("h", 1, 5.0, socket_factory=lambda f, t: fake)
    conn.connect()
    assert fake.connected
    assert fake.sent == list(SETUP_COMMANDS)
    conn.close()
    assert fake.closed
    assert not conn.connected


def test_connection_execute_receives_and_parses() -> None:
    payload = struct.pack(">I", 4321) + struct.pack("<I", 9)
    fake = FakeSocket(_handshake_bytes() + response_frame(payload))
    conn = MacConnection("h", 1, 5.0, socket_factory=lambda f, t: fake)
    conn.connect()
    res = conn.execute(KlineOffsetCmd(0, 1))
    assert res.total == 4321
    assert res.returned == 9
    assert fake.sent[-1] == KlineOffsetCmd(0, 1).build_request()


def test_connection_connect_error_raises_transport_error() -> None:
    fake = FakeSocket(connect_error=OSError("boom"))
    conn = MacConnection("h", 1, 5.0, socket_factory=lambda f, t: fake)
    with pytest.raises(TransportError):
        conn.connect()


def test_connection_recv_timeout_raises_response_timeout() -> None:
    fake = FakeSocket(recv_timeout=True)
    conn = MacConnection("h", 1, 5.0, socket_factory=lambda f, t: fake)
    conn.connect()  # 握手读取失败被忽略
    with pytest.raises(ResponseTimeoutError):
        conn.execute(KlineOffsetCmd(0, 1))


def test_connection_closed_raises_connection_closed() -> None:
    fake = FakeSocket(b"")
    conn = MacConnection("h", 1, 5.0, socket_factory=lambda f, t: fake)
    conn.connect()
    with pytest.raises(ConnectionClosedError):
        conn.execute(KlineOffsetCmd(0, 1))


def test_connection_execute_without_connect_raises() -> None:
    conn = MacConnection("h", 1, 5.0, socket_factory=lambda f, t: FakeSocket(b""))
    with pytest.raises(TransportError):
        conn.execute(KlineOffsetCmd(0, 1))


# --------------------------------------------------------------------------- #
# 客户端（hosts 故障转移 / QFQ 兜底）
# --------------------------------------------------------------------------- #


def test_client_default_hosts_and_settings() -> None:
    client = MacClient()
    assert client.hosts == list(MAC_HOSTS)
    assert client._port == MAC_PORT
    assert client._timeout == MAC_TIMEOUT


def test_client_empty_hosts_raises() -> None:
    with pytest.raises(ValueError):
        MacClient(hosts=[])


def test_client_host_failover_on_connect() -> None:
    bad = FakeSocket(connect_error=OSError("no route"))
    good = FakeSocket(_handshake_bytes())
    client = MacClient(hosts=["bad", "good"], socket_factory=_ScriptFactory([bad, good]))
    client.connect()
    assert client.host == "good"
    client.close()


def test_client_execute_failover_switches_host() -> None:
    sock1 = FakeSocket(recv_timeout=True)
    payload = struct.pack(">I", 7) + struct.pack("<I", 1)
    sock2 = FakeSocket(_handshake_bytes() + response_frame(payload))
    client = MacClient(hosts=["h1", "h2"], socket_factory=_ScriptFactory([sock1, sock2]))
    client.connect()
    assert client.host == "h1"
    res = client._execute(KlineOffsetCmd(0, 1))
    assert res.total == 7
    assert client.host == "h2"
    client.close()


class _StubClient(MacClient):
    """覆写分页取数以注入合成 K 线（不触碰网络）。"""

    def __init__(self, qfq_bars, none_bars, xdxr_events=None):
        provider = (lambda m, c: xdxr_events) if xdxr_events is not None else None
        super().__init__(hosts=["x"], xdxr_provider=provider)
        self._qfq_bars = qfq_bars
        self._none_bars = none_bars
        self.fq_calls: list[Adjust] = []

    def _fetch_kline_pages(self, market, code, period, start, count, times, fq):
        self.fq_calls.append(fq)
        return list(self._qfq_bars) if fq == Adjust.QFQ else list(self._none_bars)


def test_get_stock_kline_qfq_fallback_recomputes() -> None:
    events = [XdxrEvent(date(2024, 1, 3), fenhong=2.0)]
    stub = _StubClient(_bars([-1.0, -2.0, -3.0]), _bars([10.0, 10.0, 8.0, 9.0]), events)
    out = stub.get_stock_kline(0, "600000", Period.DAILY, adjust=Adjust.QFQ)
    assert [b.close for b in out] == [8.0, 8.0, 8.0, 9.0]
    assert stub.fq_calls == [Adjust.QFQ, Adjust.NONE]


def test_get_stock_kline_qfq_without_provider_keeps_server_result() -> None:
    stub = _StubClient(_bars([-1.0, -2.0, -3.0]), _bars([10.0, 10.0, 8.0, 9.0]), None)
    out = stub.get_stock_kline(0, "600000", Period.DAILY, adjust=Adjust.QFQ)
    assert [b.close for b in out] == [-1.0, -2.0, -3.0]


def test_local_recompute_qfq_without_provider_returns_none() -> None:
    client = MacClient(hosts=["x"])
    assert client._local_recompute_qfq(_bars([1.0, 2.0]), 0, "600000") is None


def test_convert_board_code() -> None:
    assert convert_board_code("881001") == 21001
    assert convert_board_code("399001") == 30001
    assert convert_board_code("899050") == 32050
    assert convert_board_code("000686") == 31686
    assert convert_board_code("600000") == 600000


def test_minute_time_alias() -> None:
    assert MacClient.get_stock_minute_time is MacClient.get_tick_chart
    assert MacClient.get_stock_minute_time_hist is MacClient.get_tick_charts
    assert MacClient.get_stock_transaction is MacClient.get_transactions


# --------------------------------------------------------------------------- #
# 选主：ping_mac_host / ping_mac_all / MacClient.from_best_host（全部离线）
# --------------------------------------------------------------------------- #


def _server_info_payload() -> bytes:
    """构造一条可被 ServerInfoCmd 解析（且带交易日）的响应体。

    ServerInfoCmd.parse_response 要求至少 87 字节，否则会宽容地返回
    ``today=""``；这里补足到 87 字节以确保走真实解析分支。
    """

    body = struct.pack("<H", 1)  # count
    body += b"\x00" * (8 + 3 + 9)  # flags + tag + reserved
    body += struct.pack("<I", 20240102)  # today
    body += struct.pack("<I", 0)  # ts1
    body += struct.pack("<8H", *([570, 900] * 4))  # sessions_1
    body += struct.pack("<8H", *([0, 0] * 4))  # sessions_2
    body += b"\x00"  # flag byte
    body += struct.pack("<I", 20240101)  # last_trading_day
    body += struct.pack("<I", 0)  # ts2
    body += b"\x00" * 16  # 参数区 + 补齐到 87 字节
    assert len(body) >= 87, len(body)
    return body


def test_ping_mac_host_reports_latency_after_real_response() -> None:
    """握手 + 一条带交易日的服务器信息响应都成功，才算探测成功。"""

    fake = FakeSocket(_handshake_bytes() + response_frame(_server_info_payload()))
    latency = ping_mac_host("1.2.3.4", 7709, 5.0, socket_factory=lambda f, t: fake)

    assert latency is not None
    assert latency >= 0.0
    # 不仅握手：确实发出了 ServerInfoCmd
    assert ServerInfoCmd().build_request() in fake.sent
    assert fake.closed


def test_ping_mac_host_returns_none_when_connect_fails() -> None:
    fake = FakeSocket(connect_error=OSError("no route"))
    assert ping_mac_host("1.2.3.4", 7709, 5.0, socket_factory=lambda f, t: fake) is None


def test_ping_mac_host_returns_none_when_response_has_no_trading_day() -> None:
    """TCP 可达但没给出交易日 → 必须判为失败，不能选成最佳节点。

    ``ServerInfoCmd`` 对过短响应体是宽容的（返回 ``today=""`` 而不抛异常），
    所以这里正是「仅靠不抛异常不够」的回归保护。
    """

    fake = FakeSocket(_handshake_bytes() + response_frame(b"\x00\x00"))
    assert ping_mac_host("1.2.3.4", 7709, 5.0, socket_factory=lambda f, t: fake) is None


def test_ping_mac_host_returns_none_when_frame_is_malformed() -> None:
    """连帧头都不合法（非 MAC 协议）→ 失败。"""

    fake = FakeSocket(_handshake_bytes() + b"not-a-mac-frame-at-all")
    assert ping_mac_host("1.2.3.4", 7709, 5.0, socket_factory=lambda f, t: fake) is None


def test_ping_mac_all_filters_failures_and_sorts(monkeypatch: pytest.MonkeyPatch) -> None:
    latencies = {"a": 0.5, "b": None, "c": 0.01}

    def fake_ping(host, port=7709, timeout=5.0, *, socket_factory=None):
        return latencies[host]

    monkeypatch.setattr("eltdx.mac.transport.ping_mac_host", fake_ping)
    assert ping_mac_all(["a", "b", "c"]) == [("c", 0.01), ("a", 0.5)]


def test_mac_client_from_best_host_picks_best_and_keeps_candidates(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """最佳主机必须成为**当前**主机（MacClient 的 hosts 参数优先于 host）。"""

    candidates = ["10.0.0.1", "10.0.0.2", "10.0.0.3"]
    monkeypatch.setattr(
        "eltdx.mac.client.ping_mac_all",
        lambda *a, **k: [("10.0.0.3", 0.01), ("10.0.0.1", 0.5)],
    )

    client = MacClient.from_best_host(hosts=candidates)

    assert client.host == "10.0.0.3"
    assert client.hosts == ["10.0.0.3", "10.0.0.1", "10.0.0.2"]


def test_mac_client_from_best_host_falls_back_to_first_candidate(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr("eltdx.mac.client.ping_mac_all", lambda *a, **k: [])
    client = MacClient.from_best_host(hosts=["10.0.0.1", "10.0.0.2"])
    assert client.host == "10.0.0.1"
    assert client.hosts == ["10.0.0.1", "10.0.0.2"]


def test_mac_client_from_best_host_empty_hosts_raises(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr("eltdx.mac.client.ping_mac_all", lambda *a, **k: [])
    monkeypatch.setattr("eltdx.mac.client.MAC_HOSTS", ())
    with pytest.raises(ValueError):
        MacClient.from_best_host(hosts=[])


def test_mac_client_ping_all_passes_port_and_timeout(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    seen: list[tuple] = []

    def fake_ping(hosts, port, timeout):
        seen.append((list(hosts) if hosts else None, port, timeout))
        return [("10.0.0.1", 0.1)]

    monkeypatch.setattr("eltdx.mac.client.ping_mac_all", fake_ping)

    assert MacClient.ping_all(hosts=["10.0.0.1"], port=8888, timeout=1.5) == [
        ("10.0.0.1", 0.1)
    ]
    assert seen[-1] == (["10.0.0.1"], 8888, 1.5)

    # 省略 port 时回落到 MAC_PORT
    MacClient.ping_all(hosts=["10.0.0.1"])
    assert seen[-1][1] == 7709


# --------------------------------------------------------------------------- #
# ensure_connected：用真实探测而不是只看标志位
# --------------------------------------------------------------------------- #


def _kline_offset_response() -> bytes:
    return response_frame(struct.pack(">I", 4321) + struct.pack("<I", 9))


def test_ensure_connected_connects_and_probes() -> None:
    fake = FakeSocket(_handshake_bytes() + _kline_offset_response())
    client = MacClient("1.2.3.4", 7709, 5.0, socket_factory=lambda f, t: fake)

    client.ensure_connected()

    assert fake.connected
    # 探测命令确实发出去了（不是只看标志位就返回）
    assert fake.sent[-1] == KlineOffsetCmd(0, 1).build_request()
    client.close()


def test_ensure_connected_reconnects_after_close() -> None:
    first = FakeSocket(_handshake_bytes() + _kline_offset_response())
    client = MacClient("1.2.3.4", 7709, 5.0, socket_factory=lambda f, t: first)
    client.ensure_connected()
    client.close()
    assert client._conn is None
    assert first.closed

    second = FakeSocket(_handshake_bytes() + _kline_offset_response())
    client._socket_factory = lambda f, t: second
    client.ensure_connected()

    assert second.connected
    assert client._conn is not None
    client.close()


def test_ensure_connected_fails_over_when_current_host_is_dead() -> None:
    """当前主机的探测超时 → 切到候选列表下一台。"""

    dead = FakeSocket(_handshake_bytes(), recv_timeout=True)
    alive = FakeSocket(_handshake_bytes() + _kline_offset_response())
    sockets = [dead, alive]

    client = MacClient(
        hosts=["10.0.0.1", "10.0.0.2"],
        socket_factory=lambda f, t: sockets.pop(0) if sockets else FakeSocket(b""),
    )
    assert client.host == "10.0.0.1"

    client.ensure_connected()

    assert client.host == "10.0.0.2"
    client.close()


# --------------------------------------------------------------------------- #
# 报价分页：条数与跨页顺序（静默错序回归）
# --------------------------------------------------------------------------- #


class _PagingStub(MacClient):
    """按 ``(start, page_size)`` 合成行情，并记录每次请求。"""

    def __init__(self) -> None:
        super().__init__(hosts=["x"])
        self.page_requests: list[tuple[int, int]] = []

    def _execute(self, cmd):  # type: ignore[override]
        start, page_size = cmd._start, cmd._page_size
        self.page_requests.append((start, page_size))
        return [
            _make_quote(start + i) for i in range(page_size)
        ]


def _make_quote(index: int) -> MacQuoteField:
    return MacQuoteField(
        market=0, code=f"{index:06d}", name="x", fields={"close": float(index)}
    )


def test_stock_quotes_list_last_page_does_not_overfetch() -> None:
    """回归：最后一页必须按**剩余条数**请求，不能固定 80。

    旧实现把 ``page_size`` 算在循环外，``count=100`` 时两页各取 80 条 → 返回 160 条。
    """

    stub = _PagingStub()
    out = stub.get_stock_quotes_list(Category.A, 0, 100)
    assert stub.page_requests == [(0, 80), (80, 20)]
    assert len(out) == 100


def test_stock_quotes_list_preserves_server_order_across_pages() -> None:
    """跨页必须顺序拼接，不能整页前插把第 2 页排到最前。"""

    stub = _PagingStub()
    out = stub.get_stock_quotes_list(Category.A, 0, 100)
    assert [q.code for q in out[:3]] == ["000000", "000001", "000002"]
    assert [q.code for q in out[-3:]] == ["000097", "000098", "000099"]


def test_board_members_preserves_server_order_across_pages() -> None:
    """``get_board_members`` 同样不能整页前插。"""

    stub = _PagingStub()
    out = stub.get_board_members("881001", count=100)
    assert len(out) == 100
    assert [q.code for q in out[:2]] == ["000000", "000001"]
    assert out[-1].code == "000099"


@pytest.mark.parametrize(
    ("count", "expected_requests"),
    [
        (30, [(0, 30)]),  # 少于一页
        (80, [(0, 80)]),  # 恰好一页
        (160, [(0, 80), (80, 80)]),  # 整除页大小
        (100, [(0, 80), (80, 20)]),  # 不能整除
    ],
)
def test_paging_request_shapes(count: int, expected_requests: list[tuple[int, int]]) -> None:
    stub = _PagingStub()
    out = stub.get_stock_quotes_list(Category.A, 0, count)
    assert stub.page_requests == expected_requests
    assert len(out) == count


def test_paging_respects_non_zero_start() -> None:
    stub = _PagingStub()
    out = stub.get_stock_quotes_list(Category.A, 200, 100)
    assert stub.page_requests == [(200, 80), (280, 20)]
    assert out[0].code == "000200"
    assert out[-1].code == "000299"


def test_paging_stops_when_server_returns_short_page() -> None:
    """服务端返回不足一页时应停止，不再继续请求。"""

    class _ShortPage(MacClient):
        def __init__(self) -> None:
            super().__init__(hosts=["x"])
            self.calls = 0

        def _execute(self, cmd):  # type: ignore[override]
            self.calls += 1
            return [_make_quote(i) for i in range(10)]

    stub = _ShortPage()
    out = stub.get_stock_quotes_list(Category.A, 0, 100)
    assert stub.calls == 1
    assert len(out) == 10


# --------------------------------------------------------------------------- #
# 板块排行榜：先按目标指标排序再截断（静默错排回归）
# --------------------------------------------------------------------------- #


def _board(code: str, name: str, price: float, pre_close: float) -> BoardInfo:
    return BoardInfo(
        market=0,
        code=code,
        name=name,
        price=price,
        rise_speed=(price - pre_close) / pre_close * 100,
        pre_close=pre_close,
        symbol_market=0,
        symbol_code="000000",
        symbol_name="x",
        symbol_price=0.0,
        symbol_rise_speed=0.0,
        symbol_pre_close=0.0,
    )


#: 三个板块刻意让「涨幅最高」与「成交额最高」不是同一个。
_RANKING_BOARDS = [
    _board("880001", "涨幅王", 10.9, 10.0),  # change_pct 最高
    _board("880002", "成交额王", 10.2, 10.0),  # amount / main_net_amount 最高
    _board("880003", "中间", 10.3, 10.0),
]

_RANKING_METRICS = {
    "880001": {"amount": 100.0, "vol": 1, "main_net_amount": 1},
    "880002": {"amount": 9999.0, "vol": 2, "main_net_amount": 50},
    "880003": {"amount": 500.0, "vol": 3, "main_net_amount": 3},
}


class _RankingStub(MacClient):
    def __init__(self) -> None:
        super().__init__(hosts=["x"])
        self.summarized: list[str] = []

    def get_board_list(self, board_type=None):  # type: ignore[override]
        return list(_RANKING_BOARDS)

    def get_board_summary(self, board_symbol, sort_type=None, sort_order=None):  # type: ignore[override]
        self.summarized.append(board_symbol)
        metrics = _RANKING_METRICS[board_symbol]
        return {
            "amount": metrics["amount"],
            "vol": metrics["vol"],
            "main_net_amount": metrics["main_net_amount"],
            "up_count": 0,
            "down_count": 0,
            "member_count": 10,
        }


def test_board_ranking_by_amount_returns_highest_amount_board() -> None:
    """回归：``sort_by="amount"`` 必须返回成交额最高的板块。

    旧实现先按涨幅截断 ``top_n`` 再按成交额重排，「涨幅最高但成交额低」的板块
    在第一轮就被丢弃，排行给出的不是目标指标的前 N 名。
    """

    rows = _RankingStub().get_board_ranking(top_n=1, sort_by="amount")
    assert [r["name"] for r in rows] == ["成交额王"]
    assert rows[0]["amount"] == 9999.0


@pytest.mark.parametrize(
    ("sort_by", "expected"),
    [
        ("amount", "成交额王"),
        ("main_net_amount", "成交额王"),
        ("vol", "中间"),
        ("change_pct", "涨幅王"),
    ],
)
def test_board_ranking_respects_every_sort_key(sort_by: str, expected: str) -> None:
    rows = _RankingStub().get_board_ranking(top_n=1, sort_by=sort_by)
    assert [r["name"] for r in rows] == [expected]


def test_board_ranking_change_pct_keeps_fast_path() -> None:
    """涨跌幅可直接从板块列表得到，只需聚合 ``top_n`` 个。"""

    stub = _RankingStub()
    rows = stub.get_board_ranking(top_n=2, sort_by="change_pct")
    assert len(rows) == 2
    assert len(stub.summarized) == 2


def test_board_ranking_other_keys_must_aggregate_all_boards() -> None:
    """其余指标必须聚合全部板块才能排序 —— 请求变多是拿到正确排名的代价。"""

    stub = _RankingStub()
    rows = stub.get_board_ranking(top_n=1, sort_by="amount")
    assert len(rows) == 1
    assert len(stub.summarized) == len(_RANKING_BOARDS)


def test_board_ranking_ascending() -> None:
    rows = _RankingStub().get_board_ranking(top_n=1, sort_by="amount", ascending=True)
    assert [r["name"] for r in rows] == ["涨幅王"]


def test_board_ranking_top_n_truncates_after_sorting() -> None:
    rows = _RankingStub().get_board_ranking(top_n=2, sort_by="amount")
    assert [r["name"] for r in rows] == ["成交额王", "中间"]


def test_board_ranking_rejects_unknown_sort_key() -> None:
    with pytest.raises(ValueError):
        _RankingStub().get_board_ranking(sort_by="bogus")


# --------------------------------------------------------------------------- #
# 解压输出上限（内存放大防护）
# --------------------------------------------------------------------------- #


def test_decompress_mac_body_rejects_zip_bomb_before_allocating() -> None:
    """异常压缩体必须在**产出过程中**被截断，而不是先解完再报错。

    回归：``zlib.decompress`` 会把 8 KiB 压缩体完整解成 8 MiB（放大约 1000×）
    之后才抛「长度不符」，异常响应因此能造成过量内存分配。
    """

    bomb = zlib.compress(b"\x00" * (8 * 1024 * 1024), 9)
    assert len(bomb) < 16 * 1024  # 压缩体很小
    hdr = parse_mac_header(
        struct.pack("<IIIHH", 7654321, 1, 0, len(bomb), MAX_FRAME_UNZIPPED_BYTES)
    )
    with pytest.raises(ProtocolError, match="解压超限"):
        decompress_mac_body(hdr, bomb)


def test_decompress_mac_body_accepts_body_just_under_limit() -> None:
    """接近上限的合法响应仍要正常解出。"""

    payload = b"\x01" * (MAX_FRAME_UNZIPPED_BYTES - 100)
    comp = zlib.compress(payload, 9)
    hdr = parse_mac_header(
        struct.pack("<IIIHH", 7654321, 1, 0, len(comp), len(payload))
    )
    assert decompress_mac_body(hdr, comp) == payload


def test_decompress_limited_returns_output_within_limit() -> None:
    data = zlib.compress(b"payload" * 100, 9)
    assert decompress_limited(data, MAX_FRAME_UNZIPPED_BYTES) == b"payload" * 100


def test_decompress_limited_raises_on_oversized_output() -> None:
    data = zlib.compress(b"\x00" * (1024 * 1024), 9)
    with pytest.raises(DecompressLimitError):
        decompress_limited(data, 4096)


def test_max_frame_unzipped_matches_header_field_width() -> None:
    """上限应与帧头 unzipsize 的 2 字节宽度一致（<IIIHH）。"""

    assert MAX_FRAME_UNZIPPED_BYTES == 0xFFFF


# --------------------------------------------------------------------------- #
# 第二轮评审回归项
# --------------------------------------------------------------------------- #


def test_decompress_limited_rejects_truncated_stream() -> None:
    """回归：截断的 zlib 流 ``eof`` 为 False，但输出仍可能完整。

    只检查 ``unconsumed_tail`` / ``unused_data`` 会把它们当成合法响应放过去。
    """

    payload = b"hello-world" * 10
    full = zlib.compress(payload, 9)
    assert decompress_limited(full, MAX_FRAME_UNZIPPED_BYTES) == payload

    for cut in (1, 2, 4):
        with pytest.raises(zlib.error):
            decompress_limited(full[:-cut], MAX_FRAME_UNZIPPED_BYTES)


def test_symbol_bar_seconds_period_keeps_intraday_time() -> None:
    """回归：``SECONDS`` 漏在 ``is_intraday`` 之外 → time_num 被丢弃，退化成 00:00。"""

    head = struct.pack("<H22s", 0, b"600000") + struct.pack("<HBHI", 4, 0, 1, 0)
    # 34200 秒 = 09:30
    bar = struct.pack("<II7f", 20240102, 34200, 10.0, 11.0, 9.0, 10.5, 1000.0, 200.0, 300.0)
    res = SymbolBarCmd(0, "600000", Period.SECONDS).parse_response(head + bar)
    assert res[0].datetime.date() == date(2024, 1, 2)
    assert res[0].datetime.time() == time(9, 30)


def test_symbol_bar_daily_still_ignores_time() -> None:
    """日线及以上仍然不带时间，避免误把 SECONDS 的修复外溢。"""

    head = struct.pack("<H22s", 0, b"600000") + struct.pack("<HBHI", 4, 0, 1, 0)
    bar = struct.pack("<II7f", 20240102, 34200, 10.0, 11.0, 9.0, 10.5, 1000.0, 200.0, 300.0)
    res = SymbolBarCmd(0, "600000", Period.DAILY).parse_response(head + bar)
    assert res[0].datetime == datetime(2024, 1, 2)


@pytest.mark.parametrize("days", [0, -1, 6, 99])
def test_tick_charts_rejects_out_of_range_days(days: int) -> None:
    """回归：``days`` 无校验，而响应头固定只有 5 个日期/昨收槽位。"""

    with pytest.raises(ValueError, match="days"):
        TickChartsCmd(0, "600000", days=days)


def test_parse_tick_charts_clamps_bogus_day_count() -> None:
    """回归：响应 count>5 时 ``date_ints[d]`` 会 IndexError。"""

    hdr = struct.pack("<H22s", 0, b"600000")
    hdr += struct.pack("<5I", 20240102, 0, 0, 0, 0)
    hdr += struct.pack("<5f", 9.5, 0, 0, 0, 0)
    hdr += struct.pack("<HBHH", 9, 0, 100, 0)  # 谎报 9 天
    tail = struct.pack(
        "<44sBHf5x2I5ffIf12s2fI",
        "浦发银行".encode("gbk").ljust(44, b"\x00"),
        2, 0, 1.0, 20240102, 93000,
        9.5, 9.6, 10.0, 9.4, 9.9, 0.1,
        12345, 67890.0, b"", 1.2, 9.8, 0,
    )
    res = TickChartsCmd(0, "600000").parse_response(hdr + tail)
    # 只有第 1 个日期槽有值 -> 只应产出 1 天，其余空槽不能拿去构造 date(0,0,0)
    assert len(res.charts) == 1
    assert res.charts[0].date == date(2024, 1, 2)


def test_parse_tick_charts_survives_zero_page_size() -> None:
    """page_size=0 不能让后续整除崩掉。"""

    hdr = struct.pack("<H22s", 0, b"600000")
    hdr += struct.pack("<5I", 20240102, 0, 0, 0, 0)
    hdr += struct.pack("<5f", 9.5, 0, 0, 0, 0)
    hdr += struct.pack("<HBHH", 1, 0, 0, 0)
    tail = struct.pack(
        "<44sBHf5x2I5ffIf12s2fI",
        b"", 2, 0, 1.0, 20240102, 93000,
        9.5, 9.6, 10.0, 9.4, 9.9, 0.1,
        12345, 67890.0, b"", 1.2, 9.8, 0,
    )
    res = TickChartsCmd(0, "600000").parse_response(hdr + tail)
    assert res.code == "600000"


def test_dual_semantic_bits_use_stock_names() -> None:
    """回归：canonical 名取了板块语义，个股行情返回 ``limit_up_count`` 等错误键。"""

    expected = {
        0x5D: "BID2_VOLUME",
        0x5E: "ASK2_VOLUME",
        0x88: "BID5_VOLUME",
        0x8B: "ASK5_VOLUME",
    }
    for value, name in expected.items():
        member = FieldBit._value2member_map_[value]
        assert member.name == name
        assert member.field_name == name.lower()

    # 板块语义仍可通过别名访问
    assert FieldBit.LIMIT_UP_COUNT is FieldBit.BID2_VOLUME
    assert FieldBit.UP_COUNT is FieldBit.BID5_VOLUME


def test_symbol_quotes_returns_stock_volume_keys() -> None:
    """个股报价的盘口量必须是 ``bid2_volume`` 这类键。"""

    bits = [FieldBit.BID2_VOLUME.value, FieldBit.BID5_VOLUME.value]
    bitmap = bytearray(20)
    for b in bits:
        bitmap[b // 8] |= 1 << (b % 8)
    body = bytes(bitmap) + struct.pack("<IH", 1, 1)
    body += struct.pack("<H22s44s", 0, b"600000", "x".encode("gbk"))
    body += struct.pack("<II", 12345, 6789)
    res = SymbolQuotesCmd([(0, "600000")]).parse_response(body)
    assert set(res[0].fields) == {"bid2_volume", "bid5_volume"}


def test_board_members_applies_field_postprocess() -> None:
    """回归：0x122C 漏了 FIELD_POSTPROCESS，AH_CODE 不补零、类型也不一致。"""

    bitmap = bytearray(20)
    bitmap[FieldBit.AH_CODE.value // 8] |= 1 << (FieldBit.AH_CODE.value % 8)
    body = bytes(bitmap) + struct.pack("<IH", 1, 1)
    body += struct.pack("<H22s44s", 0, b"600000", "x".encode("gbk"))
    body += struct.pack("<I", 1)
    res = BoardMembersQuotesCmd(21001).parse_response(body)
    assert res[0].fields["ah_code"] == "00001"


def test_board_members_ah_code_matches_symbol_quotes() -> None:
    """同一个字段在 0x122B / 0x122C 上语义必须一致。"""

    bitmap = bytearray(20)
    bitmap[FieldBit.AH_CODE.value // 8] |= 1 << (FieldBit.AH_CODE.value % 8)
    row = struct.pack("<H22s44s", 0, b"600000", "x".encode("gbk"))
    sym = SymbolQuotesCmd([(0, "600000")]).parse_response(
        bytes(bitmap) + struct.pack("<IH", 1, 1) + row + struct.pack("<I", 600519)
    )
    bmk = BoardMembersQuotesCmd(21001).parse_response(
        bytes(bitmap) + struct.pack("<IH", 1, 1) + row + struct.pack("<I", 600519)
    )
    assert sym[0].fields["ah_code"] == bmk[0].fields["ah_code"] == "600519"


def test_connect_twice_closes_previous_connection() -> None:
    """回归：重复 ``connect()`` 直接覆盖 ``self._conn``，旧 socket 泄漏。"""

    first = FakeSocket(_handshake_bytes())
    second = FakeSocket(_handshake_bytes())
    sockets = [first, second]

    client = MacClient(
        hosts=["10.0.0.1"],
        socket_factory=lambda f, t: sockets.pop(0) if sockets else FakeSocket(b""),
    )
    client.connect()
    assert first.connected and not first.closed

    client.connect()
    assert second.connected
    assert first.closed, "被替换掉的旧连接必须关闭"
    assert not second.closed
    client.close()
