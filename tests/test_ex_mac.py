"""eltdx.ex MAC-EX 客户端（MacExClient）测试。

依赖并发的 eltdx.mac 子包（codec.build_mac_request / bitmap.FieldBit）；
若尚未就绪则整文件跳过（不视为失败）。
"""

from __future__ import annotations

import struct
from datetime import datetime

import pytest

pytest.importorskip("eltdx.mac.codec")
pytest.importorskip("eltdx.mac.bitmap")

import eltdx.ex.mac_client as mc  # noqa: E402
from eltdx.ex.commands.get_transaction import GetExTransactionDataCmd  # noqa: E402
from eltdx.ex.commands.login import MacExLoginCmd  # noqa: E402
from eltdx.ex.mac_commands import (  # noqa: E402
    DEFAULT_QUOTE_FIELDS,
    ChartSamplingCmd,
    GoodsListCmd,
    Period,
    SymbolBarCmd,
    SymbolQuotesCmd,
    SymbolTransactionCmd,
)
from eltdx.ex.mac_client import MacExClient  # noqa: E402

# 期望字节（由 build_mac_request 独立复算得到）
EXPECTED_LOGIN = (
    "010000000001520052005424"
    "e5bb1c2fafe525941f32c6e5d53dfb415b734cc9cdbf0ac92021bfdd1eb06d22"
    "d008884c1611cb1378f6abd824d899d21f32c6e5d53dfb411f32c6e5d53dfb41"
    "a9325ac935dc0837335a16e4ce17c1bb"
)
EXPECTED_QUOTES = (
    "1c0000000001300030002b12bf0000000000000000000000000000000000000001"
    "001f0030303730300000000000000000000000000000000000"
)
EXPECTED_KLINE = (
    "1c0000000001300030002e122f0049464c30000000000000000000000000000000"
    "00000004000100000000002003000001010001000000000000"
)


# ── 三条字节级断言 ───────────────────────────────────────────────────────


def test_login_request_bytes() -> None:
    assert MacExLoginCmd().build_request().hex() == EXPECTED_LOGIN


def test_goods_quotes_request_bytes() -> None:
    # 与 MacExClient.goods_quotes 默认一致：7 个基础字段（DEFAULT_QUOTE_FIELDS）。
    cmd = SymbolQuotesCmd([(31, "00700")], DEFAULT_QUOTE_FIELDS)
    assert cmd.build_request().hex() == EXPECTED_QUOTES


def test_goods_kline_request_bytes() -> None:
    cmd = SymbolBarCmd(47, "IFL0", Period.DAILY, 0, 800)
    assert cmd.build_request().hex() == EXPECTED_KLINE


# ── MacExClient 命令接线（无网络） ───────────────────────────────────────


def test_client_goods_quotes_and_kline_wire_expected_bytes() -> None:
    captured: list[object] = []
    client = MacExClient()
    client._execute = lambda cmd: captured.append(cmd) or []  # type: ignore[method-assign]

    client.goods_quotes([(31, "00700")])
    assert captured[-1].build_request().hex() == EXPECTED_QUOTES

    client.goods_kline(47, "IFL0")
    assert captured[-1].build_request().hex() == EXPECTED_KLINE


class _FakeConn:
    def __init__(self, *_a, **_k) -> None:
        self.executed: list[object] = []

    def connect(self) -> None:
        return None

    def close(self) -> None:
        return None

    def execute(self, cmd: object) -> bool:
        self.executed.append(cmd)
        return True


def test_client_connect_performs_login(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(mc, "ExTdxConnection", _FakeConn)
    client = MacExClient()
    client.connect()
    assert isinstance(client._conn.executed[-1], MacExLoginCmd)
    assert client._conn.executed[-1].build_request().hex() == EXPECTED_LOGIN


def test_client_transaction_routes_hk_to_ex_protocol(monkeypatch: pytest.MonkeyPatch) -> None:
    seen: list[object] = []
    client = MacExClient()
    client._execute = lambda cmd: seen.append(cmd) or []  # type: ignore[method-assign]
    client.goods_transaction(31, "00700", start=0, count=5)
    assert isinstance(seen[0], GetExTransactionDataCmd)


def test_client_transaction_all_rejects_non_hk() -> None:
    client = MacExClient()
    with pytest.raises(ValueError):
        client.goods_transaction_all(47, "IFL0")


# ── MAC 响应解析（合成字节） ─────────────────────────────────────────────


def test_symbol_quotes_parse_response() -> None:
    bitmap = ((1 << 0) | (1 << 1) | (1 << 5)).to_bytes(16, "little") + b"\x00" * 4
    row = struct.pack("<H22s44s", 31, b"00700", b"TENCENT") + struct.pack("<ffI", 10.0, 11.0, 1000)
    body = bitmap + struct.pack("<IH", 1, 1) + row
    result = SymbolQuotesCmd([(31, "00700")]).parse_response(body)
    assert len(result) == 1
    assert result[0].code == "00700"
    assert result[0].name == "TENCENT"
    assert result[0].fields["pre_close"] == 10.0
    assert result[0].fields["open"] == 11.0
    assert result[0].fields["vol"] == 1000


def test_symbol_bar_parse_response() -> None:
    prefix = struct.pack("<H", 47) + b"IFL0".ljust(22, b"\x00")
    head = struct.pack("<HBHI", 4, 1, 1, 0)
    record = struct.pack("<II7f", 20240102, 0, 10.0, 12.0, 9.0, 11.0, 1234.0, 500.0, 100.0)
    bars = SymbolBarCmd(47, "IFL0", Period.DAILY).parse_response(prefix + head + record)
    assert len(bars) == 1
    assert bars[0].datetime == datetime(2024, 1, 2)
    assert bars[0].open == 10.0
    assert bars[0].close == 11.0
    assert bars[0].vol == 500.0


def test_symbol_transaction_parse_response() -> None:
    prefix = (
        struct.pack("<H", 31)
        + b"00700".ljust(22, b"\x00")
        + struct.pack("<I", 0)
        + b"\x00"
        + struct.pack("<H", 1)
        + struct.pack("<I", 0)
        + struct.pack("<I", 0)
    )
    record = struct.pack("<IfIIH", 34200, 431.4, 100, 1, 0)
    tx = SymbolTransactionCmd(31, "00700").parse_response(prefix + record)
    assert len(tx) == 1
    assert tx[0].vol == 100
    assert tx[0].price == pytest.approx(431.4)


def test_goods_list_parse_response() -> None:
    record = struct.pack("<H23sHIBfffHH", 1, b"IFL0", 2, 3, 4, 1.0, 2.0, 3.0, 5, 6)
    body = struct.pack("<H", 1) + record
    items = GoodsListCmd(47, 0, 600).parse_response(body)
    assert len(items) == 1
    assert items[0].name == "IFL0"
    assert items[0].category == 1


def test_chart_sampling_parse_response() -> None:
    body = b"\x00" * 40 + struct.pack("<H", 2) + struct.pack("<ff", 1.0, 2.0)
    assert ChartSamplingCmd(31, "00700").parse_response(body) == [1.0, 2.0]
