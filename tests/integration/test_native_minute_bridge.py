"""Authored local differential cases; no live protocol provenance is claimed."""
from __future__ import annotations

import _thread
import struct
import threading
import time
from datetime import date

import pytest

from eltdx.api.minutes import MinuteApi
from eltdx.client import TdxClient
from eltdx.exceptions import ProtocolError
from eltdx.protocol.commands import parse_command_response
from eltdx.protocol.frame import ResponseFrame
from eltdx.protocol.unit import normalize_code
from eltdx.transport import PooledSocketTransport, SocketTransport

from loopback_support import ScriptedServer, answer_handshake, read_request, response_bytes, wait_for_peer_close


CODE = "sh603777"
COMMANDS = {"today": 0x0537, "history": 0x0FB4, "recent": 0x0FEB,
            "aux": 0x051B, "sparkline": 0x0FD1}


def _parse(command, data, payload):
    return parse_command_response(command, ResponseFrame(0, 1, command, len(data), len(data), data, b""), payload)


def _directory(decimal, code=CODE):
    record = bytearray(37)
    record[:6] = code[2:].encode("ascii")
    record[6:8] = (100).to_bytes(2, "little")
    record[8:12] = b"TEST"
    record[24:28] = struct.pack("<f", 1.0)
    record[28] = decimal
    record[29:33] = struct.pack("<f", 10.0)
    return b"\x01\x00" + bytes(record)


def _minute_payload(method):
    prefix = {"today": b"\x02\x00\x00\x00",
              "history": b"\x02\x00" + struct.pack("<f", 10.5),
              "recent": b"\x02\x00" + struct.pack("<ff", 10.5, 11.25)}[method]
    # Two prices/averages per wire family; negative volume must remain legal
    # in the old Python profile even though the Rust public service rejects it.
    return prefix + bytes((10, 12, 3, 1, 2, 0x41))


def _sparkline_payload():
    data = bytearray(42)
    data[0] = 2                       # Deliberately different from requested sh.
    data[2:8] = b"920001"
    data[24] = 7                      # Deliberately different from selector1.
    data[26:30] = (123).to_bytes(4, "little")
    data[34:36] = (60).to_bytes(2, "little")
    data[36:40] = struct.pack("<f", 10.0)
    data[40:42] = (2).to_bytes(2, "little")
    return bytes(data) + struct.pack("<ff", 11.0, 12.0)


def _invoke(api, method, *, include_raw, kind="buy_sell_strength", code=CODE):
    kwargs = dict(include_raw=include_raw)
    if method in ("history", "recent"):
        kwargs["trading_date"] = date(2026, 11, 17)
    if method == "aux":
        kwargs["kind"] = kind
    if method == "sparkline":
        kwargs.update(selector=1, window=1)
    return getattr(api, method)(code, **kwargs)


class RecordedMinutes:
    def __init__(self, command, data):
        self.command, self.data = command, data
        self.calls = []

    def execute(self, command, payload):
        assert command == self.command
        self.calls.append((command, payload))
        return _parse(command, self.data, payload)


def _client(transport_type, host):
    return TdxClient(transport=transport_type([host], timeout=2, heartbeat_interval=None,
        **({"pool_size": 1} if transport_type is PooledSocketTransport else {})))


def _handler(command, data, *, directory=None, directory_market=1, observed=None, request_seen=None):
    def serve(connection):
        answer_handshake(connection)
        if directory is not None:
            for offset, reply in ((0, directory), (1, b"\x00\x00")):
                message_id, kind, body = read_request(connection)
                assert kind == 0x044D
                assert int.from_bytes(body[:2], "little") == directory_market
                assert int.from_bytes(body[2:6], "little") == offset
                connection.sendall(response_bytes(message_id, kind, reply))
        message_id, kind, body = read_request(connection)
        assert kind == command, "there must be no extra directory scan before the minute request"
        if observed is not None:
            observed.append(body)
        if request_seen is not None:
            request_seen.set()
        if data is None:
            wait_for_peer_close(connection)
            return
        connection.sendall(response_bytes(message_id, kind, data))
        wait_for_peer_close(connection)
    return serve


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
@pytest.mark.parametrize("include_raw", [False, True])
@pytest.mark.parametrize("decimal", [2, 3])
@pytest.mark.parametrize("method", ["today", "history", "recent"])
def test_native_minutes_match_old_parser_prices_negative_volume_and_raw(
    transport_type, include_raw, decimal, method,
) -> None:
    command, data, directory = COMMANDS[method], _minute_payload(method), _directory(decimal)
    # The original parser reads the legacy precision registry. Populate it
    # independently of the SDK owner's immutable metadata snapshot.
    _parse(0x044D, directory, {"market": "sh", "start": 0, "limit": 800})
    expected = _invoke(MinuteApi(RecordedMinutes(command, data)), method, include_raw=include_raw)
    with ScriptedServer([_handler(command, data, directory=directory)]) as server:
        client = _client(transport_type, server.host)
        try:
            actual = _invoke(client.minutes, method, include_raw=include_raw)
            assert actual == expected
            assert actual.points[-1].volume == -1
            assert actual.volume_sum == 2
            assert actual.raw_payload == (data if include_raw else b"")
            assert bool(actual.points[0].record_hex) is include_raw
            assert actual.points[0].price == (0.1 if decimal == 2 else 0.01)
        finally:
            client.close()
        assert server.wait_for_handlers(1)


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
@pytest.mark.parametrize("include_raw", [False, True])
@pytest.mark.parametrize("kind", ["buy_sell_strength", "volume_comparison", 255])
def test_native_minute_aux_keeps_both_layouts_unknown_selector_and_raw(transport_type, include_raw, kind) -> None:
    data = b"\x01\x00" + (struct.pack("<ff", 12.5, 3.25)
        if kind == "volume_comparison" else bytes((0x41, 7)))
    expected = _invoke(MinuteApi(RecordedMinutes(0x051B, data)), "aux", include_raw=include_raw, kind=kind)
    with ScriptedServer([_handler(0x051B, data)]) as server:
        client = _client(transport_type, server.host)
        try:
            actual = _invoke(client.minutes, "aux", include_raw=include_raw, kind=kind)
            assert actual == expected
            assert bool(actual.points[0].record_hex) is include_raw
        finally:
            client.close()
        assert server.wait_for_handlers(1)


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
@pytest.mark.parametrize("include_raw", [False, True])
def test_native_sparkline_keeps_legacy_mismatched_identity_echo_and_window(transport_type, include_raw) -> None:
    data = _sparkline_payload()
    expected = _invoke(MinuteApi(RecordedMinutes(0x0FD1, data)), "sparkline", include_raw=include_raw)
    with ScriptedServer([_handler(0x0FD1, data)]) as server:
        client = _client(transport_type, server.host)
        try:
            actual = _invoke(client.minutes, "sparkline", include_raw=include_raw)
            assert actual == expected
            assert actual.code == "920001" and actual.full_code != CODE
            assert actual.selector_echo == 7
            assert actual.count == 2 and actual.window_or_count_raw == 1
        finally:
            client.close()
        assert server.wait_for_handlers(1)


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
@pytest.mark.parametrize("method", list(COMMANDS))
def test_native_minute_protocol_failure_keeps_original_text_class_and_cause(transport_type, method) -> None:
    command = COMMANDS[method]
    with pytest.raises(ProtocolError) as reference:
        _invoke(MinuteApi(RecordedMinutes(command, b"\x00")), method, include_raw=True)
    directory = _directory(2) if method in ("today", "history", "recent") else None
    with ScriptedServer([_handler(command, b"\x00", directory=directory)]) as server:
        client = _client(transport_type, server.host)
        try:
            with pytest.raises(ProtocolError) as caught:
                _invoke(client.minutes, method, include_raw=True)
            assert type(caught.value) is type(reference.value)
            assert str(caught.value) == str(reference.value)
            assert caught.value.__cause__ is reference.value.__cause__ is None
        finally:
            client.close()
        assert server.wait_for_handlers(1)


def test_native_recent_default_date_uses_original_binding_date_selector() -> None:
    observed = []
    data, directory = _minute_payload("recent"), _directory(2)
    with ScriptedServer([_handler(0x0FEB, data, directory=directory, observed=observed)]) as server:
        client = _client(SocketTransport, server.host)
        try:
            before = date.today()
            result = client.minutes.recent(CODE)
            after = date.today()
            assert result.trading_date in (before, after)
            assert result.date_selector_raw == 0xFED62304 - result.trading_date.toordinal()
            assert int.from_bytes(observed[0][:4], "little") == result.date_selector_raw
        finally:
            client.close()
        assert server.wait_for_handlers(1)


def test_native_minute_precision_market_inference_precedes_code_normalization() -> None:
    # Resolver inference and the legacy wire extractor both select sz here.
    # The standalone Python normalization helper selects sh for this bond;
    # native preparation must preserve the legacy path without a sh scan.
    data, directory = _minute_payload("today"), _directory(3, "sz019001")
    _parse(0x044D, directory, {"market": "sz", "start": 0, "limit": 800})
    expected = _invoke(MinuteApi(RecordedMinutes(0x0537, data)), "today",
                       include_raw=True, code="019001")
    with ScriptedServer([_handler(0x0537, data, directory=directory, directory_market=0)]) as server:
        client = _client(SocketTransport, server.host)
        try:
            actual = client.minutes.today("019001", include_raw=True)
            assert actual == expected
            assert actual.full_code == expected.full_code == "sz019001"
            assert normalize_code("019001") == "sh019001"
        finally:
            client.close()
        assert server.wait_for_handlers(1)


def test_native_minute_keyboard_interrupt_waits_for_request_cancellation() -> None:
    request_seen = threading.Event()
    handler = _handler(0x0537, None, directory=_directory(2), request_seen=request_seen)
    with ScriptedServer([handler]) as server:
        client = _client(SocketTransport, server.host)

        def interrupt():
            assert request_seen.wait(timeout=3)
            time.sleep(0.05)
            _thread.interrupt_main()

        interrupter = threading.Thread(target=interrupt, daemon=True)
        interrupter.start()
        try:
            with pytest.raises(KeyboardInterrupt) as caught:
                client.minutes.today(CODE)
            assert caught.value.__cause__ is None
        finally:
            interrupter.join(timeout=1)
            client.close()
        assert not interrupter.is_alive()
        assert server.wait_for_handlers(1)
