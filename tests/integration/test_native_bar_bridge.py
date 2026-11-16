"""Synthetic local differential sources. No live-server provenance is claimed."""
from __future__ import annotations

from pathlib import Path

import pytest

from eltdx.api.bars import BarApi
from eltdx.api.delisted_bars import DelistedBarApi
from eltdx.client import TdxClient
from eltdx.exceptions import ProtocolError
from eltdx.protocol import decode_response
from eltdx.protocol.commands import parse_command_response
from eltdx.protocol.frame import ResponseFrame
from eltdx.transport import PooledSocketTransport, SocketTransport

from loopback_support import ScriptedServer, answer_handshake, read_request, response_bytes, wait_for_peer_close


def _ordinary_payload(rows):
    payload = bytearray(len(rows).to_bytes(2, "little"))
    packed_date = ((2026 - 2004) << 11) | 817
    for hour, minute in rows:
        payload.extend(packed_date.to_bytes(2, "little"))
        payload.extend((hour * 60 + minute).to_bytes(2, "little"))
        payload.extend(b"\x00" * 12)  # four zero price deltas and two volume scalars
    return bytes(payload)


def _delisted_payload():
    return decode_response((Path(__file__).parents[1] / "fixtures" / "7709" /
        "delisted_bars" / "normal" / "response.bin").read_bytes()).data


class RecordedTransport:
    def __init__(self, pages):
        self.pages = pages
        self.offsets = []

    def execute(self, command, payload):
        self.offsets.append(payload["start"])
        data = self.pages[payload["start"]]
        frame = ResponseFrame(0, 1, command, len(data), len(data), data, b"")
        return parse_command_response(command, frame, payload)


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
@pytest.mark.parametrize("include_raw", [False, True])
@pytest.mark.parametrize("delisted", [False, True])
def test_native_bar_collection_matches_old_facade_for_order_raw_and_count(
    transport_type, include_raw, delisted,
) -> None:
    command = 0x052B if delisted else 0x052D
    if delisted:
        data = _delisted_payload()
        # Equal-time duplicates intentionally repeat exact page bytes. The old
        # Python path accepted these; strict Rust detects repeated page evidence.
        pages = {0: data, 20: data, 40: b"\x00\x00"}
        api_type, code = DelistedBarApi, "T000038"
        kwargs = dict(all_pages=True, page_size=800, max_pages=None, include_raw=include_raw)
    else:
        pages = {0: _ordinary_payload([(9, 40), (23, 58)]),
                 2: _ordinary_payload([(9, 30), (9, 35)]), 4: b"\x00\x00"}
        api_type, code = BarApi, "sh600000"
        kwargs = dict(all_pages=True, page_size=3, max_pages=None,
                      include_raw=include_raw, period="5m", bar_time="end")
    reference_transport = RecordedTransport(pages)
    expected = api_type(reference_transport).get(code, **kwargs)
    observed = []

    def handler(connection):
        answer_handshake(connection)
        for offset in pages:
            message_id, kind, body = read_request(connection)
            assert kind == command, "fixed-price OHLC must not prefetch a directory"
            field = 8 if delisted else 12
            actual = int.from_bytes(body[field:field + 2], "little")
            assert actual == offset
            observed.append(actual)
            connection.sendall(response_bytes(message_id, kind, pages[offset]))
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        transport = transport_type([server.host], timeout=2, heartbeat_interval=None,
                                   **({"pool_size": 1} if transport_type is PooledSocketTransport else {}))
        client = TdxClient(transport=transport)
        try:
            actual = (client.delisted_bars if delisted else client.bars).get(code, **kwargs)
            assert actual == expected
            assert actual.request_count == actual.count
            assert observed == reference_transport.offsets
            if not delisted:
                assert actual.raw_payload == b""
                assert actual.bars[-1].time.day == 17
                assert (actual.bars[-1].time.hour, actual.bars[-1].time.minute) == (0, 3)
        finally:
            client.close()
        assert server.wait_for_handlers(1)


@pytest.mark.parametrize("delisted", [False, True])
def test_native_bar_page_limit_keeps_original_runtime_error(delisted) -> None:
    data = _delisted_payload() if delisted else _ordinary_payload([(9, 30)])
    command = 0x052B if delisted else 0x052D

    def handler(connection):
        answer_handshake(connection)
        message_id, kind, _ = read_request(connection)
        assert kind == command
        connection.sendall(response_bytes(message_id, kind, data))
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        client = TdxClient(transport=SocketTransport([server.host], timeout=2, heartbeat_interval=None))
        try:
            api = client.delisted_bars if delisted else client.bars
            name = "delisted_bars" if delisted else "bars"
            with pytest.raises(RuntimeError) as caught:
                api.get("sz000038" if delisted else "sh600000", all_pages=True, max_pages=1)
            assert type(caught.value) is RuntimeError
            assert str(caught.value) == f"{name}.get reached max_pages before the server returned an empty page"
            assert caught.value.__cause__ is None
        finally:
            client.close()
        assert server.wait_for_handlers(1)


def test_native_invalid_bar_time_is_checked_after_first_successful_decode() -> None:
    def handler(connection):
        answer_handshake(connection)
        message_id, kind, _ = read_request(connection)
        assert kind == 0x052D
        connection.sendall(response_bytes(message_id, kind, b"\x00"))
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        client = TdxClient(transport=SocketTransport([server.host], timeout=2, heartbeat_interval=None))
        try:
            with pytest.raises(ProtocolError) as caught:
                client.bars.get("sh600000", bar_time="bad")
            assert caught.value.__cause__ is None
        finally:
            client.close()
        assert server.wait_for_handlers(1)


@pytest.mark.parametrize("delisted", [False, True])
def test_native_bar_empty_history_keeps_first_metadata_and_distinct_raw(delisted) -> None:
    command = 0x052B if delisted else 0x052D

    def handler(connection):
        answer_handshake(connection)
        message_id, kind, _ = read_request(connection)
        assert kind == command
        connection.sendall(response_bytes(message_id, kind, b"\x00\x00"))
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        client = TdxClient(transport=SocketTransport([server.host], timeout=2, heartbeat_interval=None))
        try:
            api = client.delisted_bars if delisted else client.bars
            result = api.get("sz000038" if delisted else "sh600000", start=17,
                             all_pages=True, include_raw=True)
            assert result.start == 17
            assert result.count == result.request_count == 0
            assert result.raw_payload == (b"\x00\x00" if delisted else b"")
        finally:
            client.close()
        assert server.wait_for_handlers(1)


def test_native_invalid_bar_time_after_success_retains_exact_value_error_message() -> None:
    def handler(connection):
        answer_handshake(connection)
        message_id, kind, _ = read_request(connection)
        assert kind == 0x052D
        connection.sendall(response_bytes(message_id, kind, b"\x00\x00"))
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        client = TdxClient(transport=SocketTransport([server.host], timeout=2, heartbeat_interval=None))
        try:
            with pytest.raises(ValueError) as caught:
                client.bars.get("sh600000", bar_time="bad")
            assert str(caught.value) == "bar_time 必须是 'start' 或 'end'，得到: 'bad'"
            assert caught.value.__cause__ is None
        finally:
            client.close()
        assert server.wait_for_handlers(1)
