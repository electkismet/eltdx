"""Local synthetic trade differential sources, authored but not executed."""
from __future__ import annotations

import _thread
import math
import struct
import threading
import time

import pytest

from eltdx import _native_models
from eltdx.api.trades import TradeApi
from eltdx.client import TdxClient
from eltdx.exceptions import ProtocolError
from eltdx.protocol.commands import parse_command_response
from eltdx.protocol.frame import ResponseFrame
from eltdx.transport import PooledSocketTransport, SocketTransport
from eltdx.transport.native import call_native, native_module

from loopback_support import ScriptedServer, answer_handshake, read_request, response_bytes, wait_for_peer_close


CODE = "sh600001"
DATE = "2026-09-11"


def _varint(value):
    magnitude = abs(value)
    first = (magnitude & 0x3F) | (0x40 if value < 0 else 0)
    magnitude >>= 6
    result = bytearray([first | (0x80 if magnitude else 0)])
    while magnitude:
        octet = magnitude & 0x7F
        magnitude >>= 7
        result.append(octet | (0x80 if magnitude else 0))
    return bytes(result)


def _payload(rows=(), *, historical=False, base=12.5):
    data = len(rows).to_bytes(2, "little")
    if historical:
        data += struct.pack("<f", base)
    for minute, delta, volume, orders, status, tail in rows:
        data += minute.to_bytes(2, "little")
        data += b"".join(_varint(value) for value in (delta, volume, orders, status, tail))
    return data


def _parse(command, data, payload):
    return parse_command_response(command, ResponseFrame(0, 1, command, len(data), len(data), data, b""), payload)


class RecordedTrades:
    def __init__(self, pages):
        self.pages = pages
        self.offsets = []

    def execute(self, command, payload):
        self.offsets.append(payload["start"])
        return _parse(command, self.pages[payload["start"]], payload)


class RecordedBulkTrades(RecordedTrades):
    def execute_trade_batch(self, command, payload):
        self.offsets.append(payload["start"])
        # Reproduce the frozen native per-page bulk entry. A custom execute
        # returning TradePage stores datetime objects in private blocks, whereas
        # native bulk preserves lazy date tuples. Compare like entry points.
        dto = call_native(native_module().parse_command_response, command,
                          self.pages[payload["start"]], payload)
        return _native_models.trade_batch_from_dto(dto)


def _client(transport_type, host):
    return TdxClient(transport=transport_type([host], timeout=2, heartbeat_interval=None,
        **({"pool_size": 1} if transport_type is PooledSocketTransport else {})))


def _answer(connection, historical, offset, data):
    message_id, command, body = read_request(connection)
    assert command == (0x0FC6 if historical else 0x0FC5), "trade prices must not trigger metadata requests"
    position = 12 if historical else 8
    assert int.from_bytes(body[position:position + 2], "little") == offset
    connection.sendall(response_bytes(message_id, command, data))
    return body


def _invoke(api, historical, *, collect=False, opening=False, bulk=False, **kwargs):
    if opening:
        method = api.opening_match_history if historical else api.opening_match_today
    elif collect:
        method = (api.all_history_batch if historical else api.all_today_batch) if bulk else (
            api.all_history if historical else api.all_today)
    else:
        method = (api.history_batch if historical else api.today_batch) if bulk else (
            api.history if historical else api.today)
    return method(CODE, DATE, **kwargs) if historical else method(CODE, **kwargs)


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
@pytest.mark.parametrize("historical", [False, True])
@pytest.mark.parametrize("include_raw", [False, True])
@pytest.mark.parametrize("code", [CODE, "sh113001"])
def test_native_trade_single_page_matches_legacy_prices_order_and_raw(transport_type, historical, include_raw, code):
    rows = [(565, 12345, -3, 4, 8, -7), (565, -5, 9, 2, 2, 0), (910, 5, 9, 2, 5, 0)]
    data = _payload(rows, historical=historical)
    args = dict(start=17, count=1, include_raw=include_raw)
    command = 0x0FC6 if historical else 0x0FC5
    payload = dict(code=code, **args)
    if historical:
        payload["trading_date"] = DATE
    expected = _parse(command, data, payload)

    def handler(connection):
        answer_handshake(connection)
        _answer(connection, historical, 17, data)
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        client = _client(transport_type, server.host)
        try:
            actual = client.trades.history(code, DATE, **args) if historical else client.trades.today(code, **args)
            assert actual == expected
            assert actual.count == 3 and actual.request_count == 1
            assert actual.ticks[0].is_auction_snapshot and actual.ticks[1].is_opening_match
            assert actual.ticks[0].price == 12345 / (1000 if code == "sh113001" else 100)
            assert bool(actual.ticks[0].record_hex) is include_raw
            assert actual.raw_payload == (data if include_raw else b"")
        finally:
            client.close()
        assert server.wait_for_handlers(1)


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
@pytest.mark.parametrize("historical", [False, True])
@pytest.mark.parametrize("include_raw", [False, True])
def test_native_trade_collection_preserves_short_repeat_pages_first_base_and_page_order(transport_type, historical, include_raw):
    recent = [(571, 1100, 3, 1, 0, 7), (570, -100, 4, 2, 1, 0)]
    same = _payload(recent, historical=historical, base=12.5)
    pages = {0: same, 2: same, 4: _payload([(560, 500, 1, 1, 2, -1)], historical=historical, base=99),
             5: _payload(historical=historical, base=101)}
    kwargs = dict(page_size=4, max_pages=None, include_raw=include_raw)
    reference = RecordedTrades(pages)
    expected = _invoke(TradeApi(reference), historical, collect=True, **kwargs)
    observed = []

    def handler(connection):
        answer_handshake(connection)
        for offset, data in pages.items():
            _answer(connection, historical, offset, data)
            observed.append(offset)
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        client = _client(transport_type, server.host)
        try:
            actual = _invoke(client.trades, historical, collect=True, **kwargs)
            assert actual == expected
            assert actual.request_count == actual.count == 5 and actual.start == 0
            assert [tick.absolute_index for tick in actual.ticks] == [4, 2, 3, 0, 1]
            assert actual.raw_payload == b""
            assert observed == reference.offsets == [0, 2, 4, 5]
            if historical:
                assert actual.price_base_raw_f32 == 12.5
        finally:
            client.close()
        assert server.wait_for_handlers(1)


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
def test_native_trade_bulk_collection_exceeds_page_size_without_tick_objects(transport_type, monkeypatch):
    rows = [(570, 1, 1, 2, 0, 0)] * 1200
    pages = {0: _payload(rows, historical=True), 1200: _payload(rows, historical=True),
             2400: _payload(historical=True)}
    expected = TradeApi(RecordedTrades(pages)).all_history(CODE, DATE, include_raw=True)
    expected_batch = TradeApi(RecordedBulkTrades(pages)).all_history_batch(CODE, DATE, include_raw=True)
    custom_batch = TradeApi(RecordedTrades(pages)).all_history_batch(CODE, DATE, include_raw=True)
    assert custom_batch.to_columns() == expected_batch.to_columns()

    def handler(connection):
        answer_handshake(connection)
        for offset, data in pages.items():
            _answer(connection, True, offset, data)
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        client = _client(transport_type, server.host)
        try:
            with monkeypatch.context() as scoped:
                scoped.setattr(_native_models, "_trade_tick_at", lambda *_args: pytest.fail("bulk constructed a Python tick"))
                batch = client.trades.all_history_batch(CODE, DATE, include_raw=True)
                assert batch.count == batch.request_count == 2400
                assert batch.column("absolute_index")[:2] == (1200, 1201)
                assert batch == expected_batch
            assert batch.to_page() == expected
        finally:
            client.close()
        assert server.wait_for_handlers(1)


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
@pytest.mark.parametrize("historical", [False, True])
def test_native_trade_opening_skips_status_eight_and_stops_on_first_match(transport_type, historical):
    pages = {0: _payload([(565, 1000, 1, 1, 8, 0)], historical=historical),
             1: _payload([(565, 1050, 1, 1, 2, 0), (565, 5, 1, 1, 1, 0)], historical=historical)}
    expected = _invoke(TradeApi(RecordedTrades(pages)), historical, opening=True, include_raw=True)

    def handler(connection):
        answer_handshake(connection)
        for offset, data in pages.items():
            _answer(connection, historical, offset, data)
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        client = _client(transport_type, server.host)
        try:
            result = _invoke(client.trades, historical, opening=True, include_raw=True, max_pages=2)
            assert result == expected
            assert result.is_opening_match and result.absolute_index == 1
            assert result.price == 10.5
        finally:
            client.close()
        assert server.wait_for_handlers(1)


@pytest.mark.parametrize("opening", [False, True])
@pytest.mark.parametrize("historical", [False, True])
def test_native_trade_terminal_empty_page_wins_over_page_limit(opening, historical):
    pages = {0: _payload([(570, 1000, 1, 1, 0, 0)], historical=historical),
             1: _payload(historical=historical)}

    def handler(connection):
        answer_handshake(connection)
        for offset, data in pages.items():
            _answer(connection, historical, offset, data)
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        client = _client(SocketTransport, server.host)
        try:
            result = _invoke(client.trades, historical, collect=not opening, opening=opening, max_pages=2)
            assert (result is None) if opening else result.count == 1
        finally:
            client.close()
        assert server.wait_for_handlers(1)


@pytest.mark.parametrize("limit", [1, True, 1.25])
@pytest.mark.parametrize("opening", [False, True])
def test_native_trade_page_limit_preserves_original_error_and_request_count(limit, opening):
    count = math.ceil(limit)
    data = _payload([(570, 1000, 1, 1, 0, 0)], historical=True)

    def handler(connection):
        answer_handshake(connection)
        for offset in range(count):
            _answer(connection, True, offset, data)
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        client = _client(SocketTransport, server.host)
        try:
            with pytest.raises(RuntimeError, match="^trade pagination reached max_pages before an empty page$"):
                _invoke(client.trades, True, collect=not opening, opening=opening, max_pages=limit)
        finally:
            client.close()
        assert server.wait_for_handlers(1)


def test_native_trade_offset_overflow_occurs_before_next_wire_request():
    data = _payload([(570, 1, 1, 1, 0, 0)] * 1800)

    def handler(connection):
        answer_handshake(connection)
        for offset in range(0, 65536, 1800):
            _answer(connection, False, offset, data)
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        client = _client(SocketTransport, server.host)
        try:
            with pytest.raises(ValueError, match="^start must be between 0 and 65535$"):
                client.trades.all_today(CODE, max_pages=None)
        finally:
            client.close()
        assert server.wait_for_handlers(1)


@pytest.mark.parametrize("historical", [False, True])
def test_native_trade_protocol_errors_keep_legacy_type_text_and_cause(historical):
    data = b"\x01\x00"
    with pytest.raises(ProtocolError) as expected:
        _invoke(TradeApi(RecordedTrades({0: data})), historical)

    def handler(connection):
        answer_handshake(connection)
        _answer(connection, historical, 0, data)
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        client = _client(SocketTransport, server.host)
        try:
            with pytest.raises(ProtocolError) as actual:
                _invoke(client.trades, historical)
            assert type(actual.value) is type(expected.value)
            assert str(actual.value) == str(expected.value)
            assert actual.value.__cause__ is expected.value.__cause__ is None
        finally:
            client.close()
        assert server.wait_for_handlers(1)


def test_native_today_trade_retains_legacy_time_label_outside_one_day():
    data = _payload([(1500, 1000, 1, 1, 0, 0)])
    expected = TradeApi(RecordedTrades({0: data})).today(CODE)

    def handler(connection):
        answer_handshake(connection)
        _answer(connection, False, 0, data)
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        client = _client(SocketTransport, server.host)
        try:
            result = client.trades.today(CODE)
            assert result == expected and result.ticks[0].time_label == "25:00"
        finally:
            client.close()
        assert server.wait_for_handlers(1)


def test_native_historical_trade_keeps_nonfinite_legacy_base():
    data = _payload([(570, 1000, 1, 1, 0, 0)], historical=True, base=float("nan"))

    def handler(connection):
        answer_handshake(connection)
        _answer(connection, True, 0, data)
        _answer(connection, True, 1, _payload(historical=True))
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        client = _client(SocketTransport, server.host)
        try:
            result = client.trades.all_history(CODE, DATE)
            assert math.isnan(result.price_base_raw_f32) and result.count == 1
        finally:
            client.close()
        assert server.wait_for_handlers(1)


def test_native_trade_custom_date_attributes_are_evaluated_for_each_page():
    reads = []

    class ChangingDate:
        @property
        def year(self):
            reads.append("year")
            return 2026

        @property
        def month(self):
            reads.append("month")
            return 9

        @property
        def day(self):
            reads.append("day")
            return 10 + reads.count("day")

    def handler(connection):
        answer_handshake(connection)
        first = _answer(connection, True, 0, _payload([(570, 1000, 1, 1, 0, 0)], historical=True))
        second = _answer(connection, True, 1, _payload(historical=True))
        assert int.from_bytes(first[:4], "little") == 20260911
        assert int.from_bytes(second[:4], "little") == 20260912
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        client = _client(SocketTransport, server.host)
        try:
            result = client.trades.all_history(CODE, ChangingDate())
            assert result.count == 1 and result.trading_date.isoformat() == DATE
            assert reads == ["year", "month", "day", "year", "month", "day"]
        finally:
            client.close()
        assert server.wait_for_handlers(1)


def test_native_trade_collection_interrupt_cleans_current_request():
    requested = threading.Event()

    def handler(connection):
        answer_handshake(connection)
        _answer(connection, True, 0, _payload([(570, 1000, 1, 1, 0, 0)], historical=True))
        _, command, body = read_request(connection)
        assert command == 0x0FC6 and int.from_bytes(body[12:14], "little") == 1
        requested.set()
        wait_for_peer_close(connection)

    with ScriptedServer([handler]) as server:
        client = _client(SocketTransport, server.host)

        def interrupt():
            assert requested.wait(timeout=3)
            time.sleep(0.05)
            _thread.interrupt_main()

        interrupter = threading.Thread(target=interrupt, daemon=True)
        interrupter.start()
        try:
            with pytest.raises(KeyboardInterrupt):
                client.trades.all_history(CODE, DATE)
        finally:
            client.close()
            interrupter.join(timeout=3)
        assert not interrupter.is_alive()
        assert server.wait_for_handlers(1)
