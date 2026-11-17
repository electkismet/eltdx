"""Synthetic local auction differential sources; all execution is deferred."""
from __future__ import annotations

import struct

import pytest

from eltdx.api.auctions import AuctionApi
from eltdx.client import TdxClient
from eltdx.exceptions import ProtocolError
from eltdx.protocol.commands import parse_command_response
from eltdx.protocol.frame import ResponseFrame
from eltdx.transport import PooledSocketTransport, SocketTransport

from loopback_support import ScriptedServer, answer_handshake, read_request, response_bytes, wait_for_peer_close


CODE = "sh600001"


def _payload(rows):
    return len(rows).to_bytes(2, "little") + b"".join(
        struct.pack("<HfIiBB", minute, price, matched, unmatched, reserved, second)
        for minute, price, matched, unmatched, reserved, second in rows)


def _parse(data, trading_date, include_raw):
    frame = ResponseFrame(0, 1, 0x056A, len(data), len(data), data, b"")
    return parse_command_response(0x056A, frame, dict(code=CODE, trading_date=trading_date, include_raw=include_raw))


def _answer(connection, data, trading_date):
    message_id, command, body = read_request(connection)
    assert command == 0x056A, "auction must not prefetch security metadata"
    assert body[:8] == b"\x01\x00" + CODE[2:].encode("ascii")
    expected_date = 0 if trading_date is None else int(trading_date.replace("-", ""))
    assert int.from_bytes(body[8:12], "little") == expected_date
    assert body[12:] == struct.pack("<IIII", 3, 0, 0, 500)
    connection.sendall(response_bytes(message_id, command, data))


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
@pytest.mark.parametrize("trading_date", [None, "2026-09-11"])
@pytest.mark.parametrize("include_raw", [False, True])
def test_native_auction_preserves_raw_unusual_times_negative_price_and_over_limit(transport_type, trading_date, include_raw):
    rows = [(1500, -1.25, 0xFFFFFFFF, -2147483648, 7, 99),
            (565, 12.345, 100, 0, 0, 0)] + [(565, 10.0, 1, -1, 0, 30)] * 499
    data = _payload(rows)
    expected = _parse(data, trading_date, include_raw)

    def handler(connection):
        answer_handshake(connection)
        _answer(connection, data, trading_date)
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        transport = transport_type([server.host], timeout=2, heartbeat_interval=None,
            **({"pool_size": 1} if transport_type is PooledSocketTransport else {}))
        try:
            result = AuctionApi(transport, native_services=True).series(CODE, trading_date, include_raw=include_raw)
            assert result == expected and result.count == 501
            assert result.points[0].time_label == "25:00:99"
            assert result.points[0].unmatched_volume == 2147483648
            assert bool(result.points[0].record_hex) is include_raw
        finally:
            transport.close()
        assert server.wait_for_handlers(1)


@pytest.mark.parametrize("data", [b"", b"\x01", b"\x01\x00", b"\x00\x00\xff",
                                      _payload([(565, float("nan"), 1, 1, 0, 0)])])
def test_native_auction_protocol_errors_preserve_original_type_text_and_cause(data):
    with pytest.raises(ProtocolError) as expected:
        _parse(data, None, True)

    def handler(connection):
        answer_handshake(connection)
        _answer(connection, data, None)
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        transport = SocketTransport([server.host], timeout=2, heartbeat_interval=None)
        try:
            with pytest.raises(ProtocolError) as actual:
                AuctionApi(transport, native_services=True).series(CODE, include_raw=True)
            assert type(actual.value) is type(expected.value)
            assert str(actual.value) == str(expected.value)
            assert actual.value.__cause__ is expected.value.__cause__ is None
        finally:
            transport.close()
        assert server.wait_for_handlers(1)


def test_native_auction_uses_separate_dedicated_pool_and_recreates_after_close():
    data = _payload([(565, 10, 1, -1, 0, 0)])

    def handler(connection):
        answer_handshake(connection)
        _answer(connection, data, None)
        wait_for_peer_close(connection)

    with ScriptedServer([handler, handler]) as server:
        ordinary = PooledSocketTransport(["127.0.0.1:1"], pool_size=1, timeout=2, heartbeat_interval=None)
        client = TdxClient(transport=ordinary, pool_size=1, timeout=2, heartbeat_interval=None, probe_hosts=False)
        client._dedicated_hosts = (server.host,)
        try:
            assert client.auctions._dedicated_transport is None
            assert client.auctions.series(CODE).count == 1
            first = client.auctions._dedicated_transport
            assert first is not ordinary and ordinary._engine is None
            client.close()
            assert client.auctions._dedicated_transport is None
            assert client.auctions.series(CODE).count == 1
            assert client.auctions._dedicated_transport is not first
        finally:
            client.close()
        assert server.wait_for_handlers(2)


def test_native_auction_single_transport_reopens_its_original_generation():
    data = b"\x00\x00"

    def handler(connection):
        answer_handshake(connection)
        _answer(connection, data, "2026-09-11")
        wait_for_peer_close(connection)

    with ScriptedServer([handler, handler]) as server:
        client = TdxClient(transport=SocketTransport([server.host], timeout=2, heartbeat_interval=None))
        try:
            for _ in range(2):
                assert client.auctions.series(CODE, "2026-09-11").count == 0
                assert client.auctions._dedicated_transport is None
                client.close()
        finally:
            client.close()
        assert server.wait_for_handlers(2)
