"""eltdx.ex（扩展行情 EX 协议栈，同步版）测试。"""

from __future__ import annotations

import dataclasses
import socket
import struct
import zlib

import pytest

from eltdx.ex._hk_transaction import (
    _fetch_all_hk_transactions_sync,
    _fetch_hk_transactions_sync,
    is_hk_stock_market,
)
from eltdx.ex.client import ExTdxClient
from eltdx.ex.commands.base import BaseCommand
from eltdx.ex.commands.get_history_bars_range import GetExHistoryInstrumentBarsRangeCmd
from eltdx.ex.commands.get_instrument_bars import GetExInstrumentBarsCmd
from eltdx.ex.commands.get_instrument_count import GetExInstrumentCountCmd
from eltdx.ex.commands.get_instrument_info import GetExInstrumentInfoCmd
from eltdx.ex.commands.get_instrument_quote import GetExInstrumentQuoteCmd
from eltdx.ex.commands.get_instrument_quote_list import GetExInstrumentQuoteListCmd
from eltdx.ex.commands.get_markets import GetExMarketsCmd
from eltdx.ex.commands.get_minute_time import (
    GetExHistoryMinuteTimeDataCmd,
    GetExMinuteTimeDataCmd,
)
from eltdx.ex.commands.get_transaction import (
    GetExHistoryTransactionDataCmd,
    GetExTransactionDataCmd,
)
from eltdx.ex.commands.login import MacExLoginCmd
from eltdx.ex.frame import HEADER_SIZE, decompress_body, encode_header, parse_header
from eltdx.protocol.decompress import MAX_FRAME_UNZIPPED_BYTES
from eltdx.ex.models import ExInstrumentInfo, ExMarketInfo, ExTransactionRecord
from eltdx.ex.transport import sync as sync_mod
from eltdx.ex.transport.sync import ExTdxConnection
from eltdx.exceptions import ProtocolError, TransportError

# ── 帧编解码 ──────────────────────────────────────────────────────────────


def test_parse_header_reads_magic_and_sizes() -> None:
    raw = encode_header(seq_id=3, method=0x1234, zipsize=5)
    header = parse_header(raw)
    assert header.magic == 7654321
    assert header.seq_id == 3
    assert header.method == 0x1234
    assert header.zipsize == 5
    assert header.unzipsize == 5


def test_decompress_body_uncompressed_roundtrip() -> None:
    body = b"hello world"
    header = parse_header(encode_header(seq_id=0, method=0, zipsize=len(body)))
    assert decompress_body(header, body) == body


def test_decompress_body_zlib_roundtrip() -> None:
    body = b"kline" * 500
    compressed = zlib.compress(body)
    header = parse_header(
        encode_header(seq_id=0, method=0, zipsize=len(compressed), unzipsize=len(body))
    )
    assert decompress_body(header, compressed) == body


def test_parse_header_short_buffer_raises() -> None:
    with pytest.raises(ProtocolError):
        parse_header(b"\x00" * (HEADER_SIZE - 1))


def test_decompress_body_length_mismatch_raises() -> None:
    header = parse_header(encode_header(seq_id=0, method=0, zipsize=10))
    with pytest.raises(ProtocolError):
        decompress_body(header, b"\x00" * 4)


def test_decompress_body_bad_zlib_raises() -> None:
    header = parse_header(encode_header(seq_id=0, method=0, zipsize=4, unzipsize=100))
    with pytest.raises(ProtocolError):
        decompress_body(header, b"\x00" * 4)


# ── build_request 字节级断言 ──────────────────────────────────────────────


def test_build_request_bytes() -> None:
    assert GetExMarketsCmd().build_request().hex() == "01024869000102000200f423"
    assert GetExInstrumentCountCmd().build_request().hex() == "01034866000102000200f023"
    assert GetExInstrumentInfoCmd(0, 100).build_request().hex() == "01044867000108000800f523000000006400"
    assert (
        GetExInstrumentQuoteCmd(47, "IFL0").build_request().hex()
        == "0101080202010c000c00fa232f49464c300000000000"
    )
    assert (
        GetExInstrumentQuoteListCmd(47, 3, 0, 80).build_request().hex()
        == "01c1060b00020b000b0000242f0000000050000100"
    )
    assert (
        GetExInstrumentBarsCmd(4, 47, "IFL0", 0, 700).build_request().hex()
        == "0101086a010116001600ff232f49464c3000000000000400010000000000bc02"
    )
    assert (
        GetExMinuteTimeDataCmd(31, "00700").build_request().hex()
        == "0107080001010c000c000b241f303037303000000000"
    )
    assert (
        GetExHistoryMinuteTimeDataCmd(31, "00700", 20240102).build_request().hex()
        == "010130000101100010000c24e6d634011f303037303000000000"
    )
    assert (
        GetExTransactionDataCmd(31, "00700", 0, 1800).build_request().hex()
        == "01010800030112001200fc231f303037303000000000000000000807"
    )
    assert (
        GetExHistoryTransactionDataCmd(31, "00700", 20240102, 0, 1800).build_request().hex()
        == "010130000201160016000624e6d634011f303037303000000000000000000807"
    )


def test_build_request_history_range_seqid() -> None:
    GetExHistoryInstrumentBarsRangeCmd._seqid = 1
    first = GetExHistoryInstrumentBarsRangeCmd(47, "IFL0", 20240101, 20240131).build_request()
    second = GetExHistoryInstrumentBarsRangeCmd(47, "IFL0", 20240101, 20240131).build_request()
    assert first.hex() == "010138920001160016000d242f49464c3000000000000700e5d6340103d73401"
    assert first[1] == 1
    assert second[1] == 2  # 序号自增


def test_build_request_login_bytes() -> None:
    request = MacExLoginCmd().build_request()
    assert request[:12].hex() == "010000000001520052005424"
    assert request[10:12] == b"\x54\x24"  # msg_id 0x2454 小端
    assert len(request) == 92
    assert request[12:].hex().startswith("e5bb1c2fafe52594")


# ── parse_response 合成字节断言 ───────────────────────────────────────────


def test_parse_markets() -> None:
    record = struct.pack("<B32sB2s", 1, b"SH-MAIN", 1, b"SH") + b"\x00" * (64 - 36)
    body = struct.pack("<H", 1) + record
    markets = GetExMarketsCmd().parse_response(body)
    assert len(markets) == 1
    assert markets[0].market == 1
    assert markets[0].category == 1
    assert markets[0].name == "SH-MAIN"
    assert markets[0].short_name == "SH"


def test_parse_instrument_count() -> None:
    body = b"\x00" * 19 + struct.pack("<I", 12345)
    assert GetExInstrumentCountCmd().parse_response(body) == 12345


def test_parse_instrument_info() -> None:
    record = struct.pack("<BB3s9s17s9s", 4, 47, b"\x00\x00\x00", b"IFL0", b"IF-read", b"desc") + b"\x00" * (64 - 40)
    body = struct.pack("<IH", 0, 1) + record
    infos = GetExInstrumentInfoCmd(0, 1).parse_response(body)
    assert len(infos) == 1
    assert infos[0].category == 4
    assert infos[0].market == 47
    assert infos[0].code == "IFL0"
    assert infos[0].name == "IF-read"


def test_parse_quote() -> None:
    fields = struct.pack(
        "<fffffIIIIIIIIIfffffIIIIIfffffIIIII",
        10.0, 11.0, 12.0, 9.0, 11.5,
        0, 0, 100, 50, 0, 30, 20, 0, 70,
        11.4, 11.3, 11.2, 11.1, 11.0,
        1, 2, 3, 4, 5,
        11.6, 11.7, 11.8, 11.9, 12.0,
        6, 7, 8, 9, 10,
    )
    body = struct.pack("<B9s", 47, b"IFL0") + b"\x00" * 4 + fields
    quote = GetExInstrumentQuoteCmd(47, "IFL0").parse_response(body)
    assert quote is not None
    assert quote.code == "IFL0"
    assert quote.pre_close == 10.0
    assert quote.price == 11.5
    assert quote.bid1 == pytest.approx(11.4)
    assert quote.ask5 == 12.0
    assert quote.bid_vol5 == 5


def test_parse_quote_too_short_returns_none() -> None:
    assert GetExInstrumentQuoteCmd(47, "IFL0").parse_response(b"\x00" * 100) is None


def test_parse_instrument_bars_daily() -> None:
    header = b"\x00" * 18
    count = struct.pack("<H", 1)
    when = struct.pack("<I", 20240102)  # 日线：4 字节 YYYYMMDD
    record = struct.pack("<ffffIIf", 10.0, 12.0, 9.0, 11.0, 0, 500, 0.0)
    bars = GetExInstrumentBarsCmd(4, 47, "IFL0", 0, 700).parse_response(header + count + when + record)
    assert len(bars) == 1
    bar = bars[0]
    assert (bar.open, bar.high, bar.low, bar.close) == (10.0, 12.0, 9.0, 11.0)
    assert (bar.year, bar.month, bar.day, bar.hour, bar.minute) == (2024, 1, 2, 15, 0)
    assert bar.trade == 500
    assert bar.amount == 0.0


def test_parse_history_bars_range() -> None:
    header = b"\x00" * 12
    count = struct.pack("<H", 1)
    # 2024-01-02 09:30 → d1 = (2024-2004)*2048 + 102 = 41062 ; d2 = 570
    d1 = (2024 - 2004) * 2048 + 102
    record = struct.pack("<HHffffIIf", d1, 570, 1.0, 1.2, 0.9, 1.1, 5, 6, 7.0)
    bars = GetExHistoryInstrumentBarsRangeCmd(47, "IFL0", 20240101, 20240131).parse_response(
        header + count + record
    )
    assert len(bars) == 1
    bar = bars[0]
    assert (bar.year, bar.month, bar.day, bar.hour, bar.minute) == (2024, 1, 2, 9, 30)
    assert bar.trade == 6


def test_parse_minute_time() -> None:
    body = struct.pack("<B9sH", 31, b"00700", 1) + struct.pack("<HffII", 570, 3.5, 3.4, 1000, 200)
    bars = GetExMinuteTimeDataCmd(31, "00700").parse_response(body)
    assert len(bars) == 1
    assert (bars[0].hour, bars[0].minute) == (9, 30)
    assert bars[0].price == 3.5
    assert bars[0].open_interest == 200


def test_parse_transaction() -> None:
    body = struct.pack("<B9s4sH", 31, b"00700", b"\x00" * 4, 1) + struct.pack(
        "<HIIiH", 570, 431400, 100, -5, 12345
    )
    records = GetExTransactionDataCmd(31, "00700", 0, 1800).parse_response(body)
    assert len(records) == 1
    rec = records[0]
    assert (rec.hour, rec.minute) == (9, 30)
    assert rec.second == 0  # 12345 % 10000 = 2345 > 59 → 归零
    assert rec.price == 431400
    assert rec.volume == 100
    assert rec.zengcang == -5
    assert rec.nature == 1  # 12345 // 10000


def test_parse_hk_quote_list_rejects_bad_category() -> None:
    with pytest.raises(ValueError):
        GetExInstrumentQuoteListCmd(47, 9, 0, 80)


# ── 港股逐笔两页拼接 ─────────────────────────────────────────────────────


def _record(price: int) -> ExTransactionRecord:
    return ExTransactionRecord(
        hour=9, minute=30, second=15, price=price, volume=100, zengcang=0, nature=0
    )


def test_fetch_hk_transactions_sync_two_pages() -> None:
    calls: list[int] = []

    def execute(cmd: BaseCommand) -> list[ExTransactionRecord]:
        calls.append(cmd.count)
        return [_record(431400)] * cmd.count if cmd.count == 1800 else [_record(431400)] * 3

    assert is_hk_stock_market(31)
    result = _fetch_hk_transactions_sync(execute, 31, "00700", None, 0, 1803)
    assert len(result) == 1803
    assert result[0].price == 431.4
    assert calls == [1800, 3]


def test_fetch_all_hk_transactions_sync_stops_on_short_page() -> None:
    pages = [
        [_record(1)] * 1800,
        [_record(2)] * 5,
    ]

    def execute(cmd: BaseCommand) -> list[ExTransactionRecord]:
        return pages.pop(0) if pages else []

    result = _fetch_all_hk_transactions_sync(execute, 31, "00700", None)
    assert len(result) == 1805


# ── 传输层（fake socket，不联网） ────────────────────────────────────────


class FakeSocket:
    def __init__(self, chunks: list[bytes]) -> None:
        self._chunks = list(chunks)
        self.sent = bytearray()
        self.closed = False
        self.connected_to: tuple[str, int] | None = None

    def settimeout(self, _timeout: float) -> None:
        return None

    def connect(self, addr: tuple[str, int]) -> None:
        self.connected_to = addr

    def sendall(self, data: bytes) -> None:
        self.sent += data

    def recv(self, _n: int) -> bytes:
        return self._chunks.pop(0) if self._chunks else b""

    def close(self) -> None:
        self.closed = True


def _frame(body: bytes, *, compressed: bool = False) -> bytes:
    if compressed:
        payload = zlib.compress(body)
        return encode_header(seq_id=0, method=0, zipsize=len(payload), unzipsize=len(body)) + payload
    return encode_header(seq_id=0, method=0, zipsize=len(body)) + body


def test_execute_sends_request_and_parses_response() -> None:
    body = b"\x00" * 19 + struct.pack("<I", 42)
    frame = _frame(body)
    conn = ExTdxConnection("127.0.0.1")
    conn._sock = FakeSocket([frame[:HEADER_SIZE], frame[HEADER_SIZE:]])
    assert conn.execute(GetExInstrumentCountCmd()) == 42
    assert bytes(conn._sock.sent) == GetExInstrumentCountCmd().build_request()


def test_execute_handles_zlib_body() -> None:
    frame = _frame(b"\x00" * 19 + struct.pack("<I", 7), compressed=True)
    conn = ExTdxConnection("127.0.0.1")
    conn._sock = FakeSocket([frame[:HEADER_SIZE], frame[HEADER_SIZE:]])
    assert conn.execute(GetExInstrumentCountCmd()) == 7


def test_execute_closed_socket_raises() -> None:
    frame = _frame(b"\x00" * 19 + struct.pack("<I", 1))
    conn = ExTdxConnection("127.0.0.1")
    conn._sock = FakeSocket([frame[:HEADER_SIZE], b""])
    with pytest.raises(TransportError):
        conn.execute(GetExInstrumentCountCmd())


def test_execute_without_connect_raises() -> None:
    conn = ExTdxConnection("127.0.0.1")
    with pytest.raises(TransportError):
        conn.execute(GetExInstrumentCountCmd())


class DummyCmd(BaseCommand[bytes]):
    def build_request(self) -> bytes:
        return b"\x1cZZ"

    def parse_response(self, body: bytes) -> bytes:
        return body


def test_mac_ex_mode_rewrites_head_flag() -> None:
    frame = _frame(b"xy")
    conn = ExTdxConnection("127.0.0.1", mac_ex_mode=True)
    conn._sock = FakeSocket([frame[:HEADER_SIZE], frame[HEADER_SIZE:]])
    assert conn.execute(DummyCmd()) == b"xy"
    assert bytes(conn._sock.sent)[0] == 0x01


def test_connect_uses_socket(monkeypatch: pytest.MonkeyPatch) -> None:
    fake = FakeSocket([])
    monkeypatch.setattr(sync_mod.socket, "socket", lambda *_a, **_k: fake)
    conn = ExTdxConnection("1.2.3.4", 7727)
    conn.connect()
    assert fake.connected_to == ("1.2.3.4", 7727)


def test_connect_failure_raises_transport_error(monkeypatch: pytest.MonkeyPatch) -> None:
    def boom(*_a, **_k):
        raise OSError("refused")

    fake = FakeSocket([])
    fake.connect = boom  # type: ignore[method-assign]
    monkeypatch.setattr(sync_mod.socket, "socket", lambda *_a, **_k: fake)
    conn = ExTdxConnection("1.2.3.4", 7727)
    with pytest.raises(TransportError):
        conn.connect()


def test_execute_socket_timeout_raises_transport_error() -> None:
    class TimeoutSocket(FakeSocket):
        def recv(self, _n: int) -> bytes:
            raise socket.timeout("timed out")

    conn = ExTdxConnection("127.0.0.1")
    conn._sock = TimeoutSocket([])
    with pytest.raises(TransportError):
        conn.execute(GetExInstrumentCountCmd())


def test_ex_tdx_client_end_to_end_over_fake_socket(monkeypatch: pytest.MonkeyPatch) -> None:
    body = b"\x00" * 19 + struct.pack("<I", 99)
    frame = _frame(body)
    fake = FakeSocket([frame[:HEADER_SIZE], frame[HEADER_SIZE:]])
    monkeypatch.setattr(sync_mod.socket, "socket", lambda *_a, **_k: fake)
    client = ExTdxClient("1.2.3.4", hosts=["1.2.3.4"])
    client.connect()
    try:
        assert client.get_instrument_count() == 99
    finally:
        client.close()
    assert fake.closed


# ── _raw：记录原始字节必须无损留存（与 easy_tdx 同语义） ─────────────────────


def _compact_zipday(year: int, month: int, day: int) -> int:
    return ((year - 2004) << 11) + month * 100 + day


def test_instrument_bars_raw_is_exact_record_bytes() -> None:
    """分钟级 K 线：_raw == 日期时间 4 字节 + 28 字节行情。"""

    dt = struct.pack("<HH", _compact_zipday(2024, 1, 2), 9 * 60 + 30)
    quote = struct.pack("<ffffIIf", 1.1, 1.2, 1.0, 1.15, 5, 6, 1.1)
    body = b"\x00" * 18 + struct.pack("<H", 1) + dt + quote

    bars = GetExInstrumentBarsCmd(0, 31, "00700").parse_response(body)

    assert len(bars) == 1
    assert len(bars[0]._raw) == 32
    assert bars[0]._raw == dt + quote
    assert (bars[0].year, bars[0].month, bars[0].day) == (2024, 1, 2)
    assert (bars[0].hour, bars[0].minute) == (9, 30)


def test_history_bars_raw_is_exact_record_bytes() -> None:
    """日线：_raw == 32 字节定长记录。"""

    record = struct.pack("<HHffffIIf", 3212, 570, 1.5, 1.6, 1.4, 1.55, 10, 20, 1.5)
    body = b"\x00" * 12 + struct.pack("<H", 1) + record

    bars = GetExHistoryInstrumentBarsRangeCmd(0, "000001", 20200101, 20201231).parse_response(body)

    assert len(bars) == 1
    assert bars[0]._raw == record
    assert len(bars[0]._raw) == 32


def test_minute_bar_raw_is_exact_record_bytes() -> None:
    record = struct.pack("<HffII", 570, 1.2, 1.21, 100, 42)
    body = struct.pack("<B9sH", 0, b"000001\x00\x00\x00", 1) + record

    bars = GetExMinuteTimeDataCmd(0, "000001").parse_response(body)

    assert len(bars) == 1
    assert bars[0]._raw == record
    assert len(bars[0]._raw) == 18


def test_transaction_raw_is_exact_record_bytes() -> None:
    record = struct.pack("<HIIiH", 570, 1200, 3, 7, 10020)
    body = struct.pack("<B9s4sH", 0, b"000001\x00\x00\x00", b"\x00" * 4, 1) + record

    records = GetExTransactionDataCmd(0, "000001").parse_response(body)

    assert len(records) == 1
    assert records[0]._raw == record
    assert len(records[0]._raw) == 16


def test_instrument_info_raw_is_whole_64_byte_slot() -> None:
    record = struct.pack(
        "<BB3s9s17s9s",
        1,
        2,
        b"\x00" * 3,
        b"000001\x00\x00\x00",
        b"desc\x00" * 4,
        b"n\x00" * 4,
    )
    body = struct.pack("<IH", 0, 1) + record + b"\x00" * 24

    infos = GetExInstrumentInfoCmd(0).parse_response(body)

    assert len(infos) == 1
    assert infos[0]._raw == record + b"\x00" * 24
    assert len(infos[0]._raw) == 64


def test_markets_raw_is_whole_64_byte_slot() -> None:
    record = struct.pack("<B32sB2s", 1, b"\x00" * 32, 31, b"HK")
    body = struct.pack("<H", 1) + record + b"\x00" * 28

    markets = GetExMarketsCmd().parse_response(body)

    assert len(markets) == 1
    assert markets[0]._raw == record + b"\x00" * 28
    assert len(markets[0]._raw) == 64


def test_instrument_quote_raw_is_whole_response() -> None:
    """五档行情是单条记录：_raw 覆盖整条响应（超出定长部分也保留）。"""

    payload = struct.pack(
        "<fffffIIIIIIIIIfffffIIIIIfffffIIIII",
        *([1.0] * 5 + [1] * 9 + [2.0] * 5 + [2] * 5 + [3.0] * 5 + [3] * 5),
    )
    body = struct.pack("<B9s", 31, b"00700\x00\x00\x00\x00") + b"\x00" * 4 + payload

    quote = GetExInstrumentQuoteCmd(31, "00700").parse_response(body)

    assert quote._raw == body
    assert len(quote._raw) == 150
    assert quote.code == "00700"


def test_raw_is_excluded_from_compare_and_repr() -> None:
    """_raw 不参与相等比较、也不出现在 repr（与 easy_tdx 的 field 参数一致）。"""

    record = struct.pack("<HffII", 570, 1.2, 1.21, 100, 42)
    body = struct.pack("<B9sH", 0, b"000001\x00\x00\x00", 1) + record

    bar = GetExMinuteTimeDataCmd(0, "000001").parse_response(body)[0]
    other = dataclasses.replace(bar, _raw=b"totally different")

    assert bar == other
    assert "_raw" not in repr(bar)


def test_raw_defaults_to_empty_bytes() -> None:
    """_raw 有默认值，因此按位置/关键字构造旧代码不破。"""

    assert ExInstrumentInfo(1, 2, "000001", "n", "d")._raw == b""
    assert ExMarketInfo(31, 1, "香港", "HK")._raw == b""



def test_decompress_body_rejects_zip_bomb_before_allocating() -> None:
    """扩展行情帧同样要有解压输出上限。

    8 KiB 压缩体声明解出 8 MiB，旧实现会先把 8 MiB 分配出来才报错。
    """

    bomb = zlib.compress(b"\x00" * (8 * 1024 * 1024), 9)
    header = parse_header(
        encode_header(
            seq_id=0, method=0, zipsize=len(bomb), unzipsize=MAX_FRAME_UNZIPPED_BYTES
        )
    )
    with pytest.raises(ProtocolError, match="解压超限"):
        decompress_body(header, bomb)


def test_decompress_body_accepts_body_just_under_limit() -> None:
    body = b"\x01" * (MAX_FRAME_UNZIPPED_BYTES - 100)
    compressed = zlib.compress(body, 9)
    header = parse_header(
        encode_header(seq_id=0, method=0, zipsize=len(compressed), unzipsize=len(body))
    )
    assert decompress_body(header, compressed) == body


# --------------------------------------------------------------------------- #
# 区间 K 线请求序号回绕（第 256 次崩溃回归）
# --------------------------------------------------------------------------- #


def test_history_range_seqid_wraps_after_255() -> None:
    """回归：序号只占 1 字节，必须在 255 后回绕。

    早期实现让类级 ``_seqid`` 单调递增并直接 ``struct.pack("<B", ...)``，第 257 次
    构造请求就抛 ``struct.error``；因为状态在类上，新建客户端也恢复不了。
    """

    GetExHistoryInstrumentBarsRangeCmd._seqid = 253
    seqids = [
        GetExHistoryInstrumentBarsRangeCmd(47, "IFL0", 20240101, 20240131).build_request()[1]
        for _ in range(6)
    ]
    assert seqids == [253, 254, 255, 1, 2, 3]


def test_history_range_seqid_survives_many_calls() -> None:
    """远超 256 次调用也不应报错，且序号始终落在 1..255。"""

    GetExHistoryInstrumentBarsRangeCmd._seqid = 1
    seqids = [
        GetExHistoryInstrumentBarsRangeCmd(47, "IFL0", 20240101, 20240131).build_request()[1]
        for _ in range(600)
    ]
    assert min(seqids) == GetExHistoryInstrumentBarsRangeCmd._SEQID_MIN
    assert max(seqids) == GetExHistoryInstrumentBarsRangeCmd._SEQID_MAX


def test_history_range_seqid_never_uses_zero() -> None:
    """0 留作「无序号」哨兵，回绕时跳过。"""

    GetExHistoryInstrumentBarsRangeCmd._seqid = GetExHistoryInstrumentBarsRangeCmd._SEQID_MAX
    seqid = GetExHistoryInstrumentBarsRangeCmd(47, "IFL0", 1, 2).build_request()[1]
    assert seqid == GetExHistoryInstrumentBarsRangeCmd._SEQID_MAX
    seqid = GetExHistoryInstrumentBarsRangeCmd(47, "IFL0", 1, 2).build_request()[1]
    assert seqid == 1
