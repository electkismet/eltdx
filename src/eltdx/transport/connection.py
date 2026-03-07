from __future__ import annotations

import socket
import threading
from concurrent.futures import ThreadPoolExecutor

from eltdx.exceptions import ConnectionClosedError, ProtocolError
from eltdx.hosts import DEFAULT_HOSTS
from eltdx.protocol.constants import TYPE_CALL_AUCTION, TYPE_CODE, TYPE_COUNT, TYPE_GBBQ, TYPE_HISTORY_MINUTE, TYPE_HISTORY_TRADE, TYPE_KLINE, TYPE_MINUTE, TYPE_QUOTE, TYPE_TRADE
from eltdx.protocol.frame import ResponseFrame, decode_response, read_response_frame
from eltdx.protocol.model_call_auction import build_call_auction_frame, parse_call_auction_payload
from eltdx.protocol.model_code import build_code_frame, parse_code_payload
from eltdx.protocol.model_connect import build_connect_frame, build_heart_frame
from eltdx.protocol.model_count import build_count_frame, parse_count_payload
from eltdx.protocol.model_gbbq import build_gbbq_frame, filter_xdxr_items, parse_gbbq_payload
from eltdx.protocol.model_kline import build_kline_frame, parse_kline_payload
from eltdx.protocol.model_minute import build_history_minute_frame, build_minute_frame, parse_history_minute_payload, parse_minute_payload
from eltdx.protocol.model_quote import build_quote_frame, parse_quote_payload
from eltdx.protocol.model_trade import build_history_trade_frame, build_trade_frame, parse_history_trade_payload, parse_trade_payload
from eltdx.transport.heartbeat import HeartbeatLoop
from eltdx.transport.reader import ResponseReader
from eltdx.transport.router import ResponseRouter


class TdxConnection:
    def __init__(self, hosts: list[str], timeout: float = 8.0, heartbeat_interval: float = 30.0) -> None:
        self._hosts = list(hosts or DEFAULT_HOSTS)
        self._timeout = timeout
        self._heartbeat_interval = heartbeat_interval
        self._lock = threading.Lock()
        self._socket: socket.socket | None = None
        self._connected_host: str | None = None
        self._router = ResponseRouter()
        self._stop_event = threading.Event()
        self._executor = ThreadPoolExecutor(max_workers=2)
        self._reader = ResponseReader(self._stop_event, self._router.deliver, self._mark_disconnected)
        self._heartbeat = HeartbeatLoop(self._stop_event, heartbeat_interval, self._send_heartbeat, self._is_connected, self._mark_disconnected)
        self._msg_id = 1

    def connect(self) -> None:
        with self._lock:
            if self._is_connected():
                return
            self._stop_event.clear()
            self._connect_socket()
            self._executor.submit(self._reader.run, self._socket)
            self._handshake()
            self._executor.submit(self._heartbeat.run)

    def close(self) -> None:
        with self._lock:
            self._stop_event.set()
            self._router.clear()
            if self._socket is not None:
                try:
                    self._socket.close()
                finally:
                    self._socket = None
                    self._connected_host = None

    @property
    def connected_host(self) -> str | None:
        return self._connected_host

    def request_count(self, exchange):
        response = self._request(build_count_frame(exchange, self._next_msg_id()), TYPE_COUNT)
        return parse_count_payload(response)

    def request_codes(self, exchange, start: int):
        response = self._request(build_code_frame(exchange, start, self._next_msg_id()), TYPE_CODE)
        return parse_code_payload(exchange, response)

    def request_gbbq(self, code: str, *, include_raw: bool = False):
        response = self._request(build_gbbq_frame(code, self._next_msg_id()), TYPE_GBBQ)
        return parse_gbbq_payload(response, include_raw=include_raw)

    def request_xdxr(self, code: str):
        return filter_xdxr_items(self.request_gbbq(code).items)

    def request_minute(self, code: str, *, include_raw: bool = False):
        response = self._request(build_minute_frame(code, self._next_msg_id()), TYPE_MINUTE)
        return parse_minute_payload(response, include_raw=include_raw)

    def request_history_minute(self, code: str, trading_date, *, include_raw: bool = False):
        response = self._request(build_history_minute_frame(code, trading_date, self._next_msg_id()), TYPE_HISTORY_MINUTE)
        return parse_history_minute_payload(trading_date, response, include_raw=include_raw)

    def request_kline(self, period, code: str, start: int, count: int, *, kind: str = "stock", include_raw: bool = False):
        response = self._request(build_kline_frame(period, code, start, count, self._next_msg_id()), TYPE_KLINE)
        return parse_kline_payload(period, code, response, kind=kind, include_raw=include_raw)

    def request_trade(self, code: str, start: int, count: int, *, include_raw: bool = False):
        response = self._request(build_trade_frame(code, start, count, self._next_msg_id()), TYPE_TRADE)
        return parse_trade_payload(code, None, response, include_raw=include_raw)

    def request_history_trade(self, code: str, trading_date, start: int, count: int, *, include_raw: bool = False):
        response = self._request(build_history_trade_frame(code, trading_date, start, count, self._next_msg_id()), TYPE_HISTORY_TRADE)
        return parse_history_trade_payload(code, trading_date, response, include_raw=include_raw)

    def request_call_auction(self, code: str, *, include_raw: bool = False):
        response = self._request(build_call_auction_frame(code, self._next_msg_id()), TYPE_CALL_AUCTION)
        return parse_call_auction_payload(code, response, include_raw=include_raw)

    def request_quotes(self, codes: list[str]):
        response = self._request(build_quote_frame(codes, self._next_msg_id()), TYPE_QUOTE)
        return parse_quote_payload(codes, response)

    def _request(self, frame, expected_type: int) -> ResponseFrame:
        self.connect()
        with self._lock:
            if not self._is_connected():
                raise ConnectionClosedError("connection is not open")
        try:
            return self._request_once(frame, expected_type)
        except (OSError, ConnectionClosedError, ProtocolError):
            self._mark_disconnected()
            self.connect()
            return self._request_once(frame, expected_type)

    def _request_once(self, frame, expected_type: int) -> ResponseFrame:
        message_queue = self._router.register(frame.msg_id)
        try:
            self._send_frame(frame)
            response = message_queue.get(timeout=self._timeout)
        except Exception as exc:
            raise ConnectionClosedError("request timed out") from exc
        finally:
            self._router.unregister(frame.msg_id)

        if response.msg_type != expected_type:
            raise ProtocolError(f"unexpected response type: {response.msg_type:#x}, expected {expected_type:#x}")
        return response

    def _handshake(self) -> None:
        frame = build_connect_frame(self._next_msg_id())
        self._request_once(frame, frame.msg_type)

    def _connect_socket(self) -> None:
        last_error: OSError | None = None
        for host in self._hosts:
            address, port = host.split(":", 1)
            sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            sock.settimeout(self._timeout)
            try:
                sock.connect((address, int(port)))
            except OSError as exc:
                sock.close()
                last_error = exc
                continue
            sock.settimeout(None)
            self._socket = sock
            self._connected_host = host
            return
        raise ConnectionClosedError("unable to connect to any host") from last_error

    def _send_frame(self, frame) -> None:
        if self._socket is None:
            raise ConnectionClosedError("connection is not open")
        self._socket.sendall(frame.to_bytes())

    def _send_heartbeat(self) -> None:
        self._send_frame(build_heart_frame(self._next_msg_id()))

    def _mark_disconnected(self) -> None:
        self.close()

    def _is_connected(self) -> bool:
        return self._socket is not None and not self._stop_event.is_set()

    def _next_msg_id(self) -> int:
        value = self._msg_id
        self._msg_id += 1
        return value
