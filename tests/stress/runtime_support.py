"""Bounded loopback server for native runtime stress and resource tests."""

from __future__ import annotations

import socket
import struct
import threading
import time
from contextlib import suppress


TYPE_HANDSHAKE = 0x000D
TYPE_HEARTBEAT = 0x0004
TYPE_FILE_CONTENT = 0x06B9
CONTENT = struct.Struct("<IIII")
PATH = "eltdx-native-stress.bin"


class NativeStressServer:
    def __init__(
        self,
        *,
        close_after_response: bool = False,
        response_delay: float = 0.0,
        push_frames_per_response: int = 0,
    ) -> None:
        if not 0 <= push_frames_per_response <= 1_024:
            raise ValueError("push_frames_per_response must be between 0 and 1024")
        self.close_after_response = close_after_response
        self.response_delay = response_delay
        self.push_frames_per_response = push_frames_per_response
        self._listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self._listener.settimeout(0.2)
        self._acceptor: threading.Thread | None = None
        self._stop = threading.Event()
        self._condition = threading.Condition()
        self._connections: set[socket.socket] = set()
        self._workers: set[threading.Thread] = set()
        self._active_requests = 0
        self.accepted = 0
        self.requests = 0
        self.max_active_requests = 0
        self.errors: list[BaseException] = []
        self.host = ""

    def __enter__(self) -> NativeStressServer:
        self._listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self._listener.bind(("127.0.0.1", 0))
        self._listener.listen()
        address, port = self._listener.getsockname()
        self.host = f"{address}:{port}"
        self._acceptor = threading.Thread(
            target=self._accept,
            name="eltdx-native-stress-acceptor",
            daemon=True,
        )
        self._acceptor.start()
        return self

    def __exit__(self, exc_type, exc, tb) -> None:
        self._stop.set()
        self._listener.close()
        with self._condition:
            connections = tuple(self._connections)
            workers = set(self._workers)
        for connection in connections:
            with suppress(OSError):
                connection.shutdown(socket.SHUT_RDWR)
            connection.close()
        if self._acceptor is not None:
            self._acceptor.join(timeout=5)
        acceptor_alive = self._acceptor is not None and self._acceptor.is_alive()
        with self._condition:
            workers.update(self._workers)
        for worker in workers:
            worker.join(timeout=5)
        with self._condition:
            stopped = self._condition.wait_for(lambda: not self._workers, timeout=5)
            alive = sorted(worker.name for worker in self._workers)
            errors = tuple(self.errors)
        if exc_type is None and (not stopped or alive):
            raise AssertionError(f"native stress workers did not stop: {alive!r}")
        if exc_type is None and acceptor_alive:
            raise AssertionError("native stress acceptor did not stop")
        if exc_type is None and errors:
            raise AssertionError(f"native stress server failed: {errors!r}")

    def wait_for_idle(self, timeout: float = 10.0) -> bool:
        with self._condition:
            return self._condition.wait_for(lambda: self._active_requests == 0, timeout=timeout)

    def wait_for_active(self, count: int, timeout: float = 10.0) -> bool:
        with self._condition:
            return self._condition.wait_for(
                lambda: self._active_requests >= count,
                timeout=timeout,
            )

    def wait_for_no_workers(self, timeout: float = 10.0) -> bool:
        with self._condition:
            return self._condition.wait_for(lambda: not self._workers, timeout=timeout)

    @property
    def live_workers(self) -> int:
        with self._condition:
            return sum(worker.is_alive() for worker in self._workers)

    def _accept(self) -> None:
        while not self._stop.is_set():
            try:
                connection, _ = self._listener.accept()
            except TimeoutError:
                continue
            except OSError:
                return
            connection.settimeout(10)
            with self._condition:
                self.accepted += 1
                connection_id = self.accepted
                self._connections.add(connection)
            worker = threading.Thread(
                target=self._serve,
                args=(connection, connection_id),
                name=f"eltdx-native-stress-{connection_id}",
                daemon=True,
            )
            with self._condition:
                self._workers.add(worker)
            worker.start()

    def _serve(self, connection: socket.socket, connection_id: int) -> None:
        current = threading.current_thread()
        try:
            with connection:
                while not self._stop.is_set():
                    message_id, message_type, payload = _read_request(connection)
                    if message_type == TYPE_HANDSHAKE:
                        if payload != b"\x01":
                            raise AssertionError("invalid native stress handshake request")
                        connection.sendall(
                            _response(message_id, message_type, _handshake_payload())
                        )
                        continue
                    if message_type == TYPE_HEARTBEAT:
                        if payload:
                            raise AssertionError("invalid native stress heartbeat request")
                        heartbeat = b"\x00" * 6 + (20260815).to_bytes(4, "little")
                        connection.sendall(_response(message_id, message_type, heartbeat))
                        continue
                    if message_type != TYPE_FILE_CONTENT:
                        raise AssertionError(f"unexpected stress command: {message_type:#x}")
                    token, requested_size = _file_request(payload)
                    if requested_size != CONTENT.size:
                        raise AssertionError(
                            "stress request size does not match the identity payload"
                        )
                    with self._condition:
                        self.requests += 1
                        request_sequence = self.requests
                        self._active_requests += 1
                        self.max_active_requests = max(
                            self.max_active_requests,
                            self._active_requests,
                        )
                    try:
                        if self.response_delay:
                            time.sleep(self.response_delay)
                        checksum = token ^ connection_id ^ request_sequence
                        content = CONTENT.pack(token, connection_id, request_sequence, checksum)
                        body = len(content).to_bytes(4, "little") + content
                        push_message_ids: set[int] = set()
                        for push_index in range(self.push_frames_per_response):
                            push_message_id = (message_id + push_index + 1) & 0xFFFF_FFFF
                            while (
                                push_message_id == 0
                                or push_message_id == message_id
                                or push_message_id in push_message_ids
                            ):
                                push_message_id = (push_message_id + 1) & 0xFFFF_FFFF
                            push_message_ids.add(push_message_id)
                            connection.sendall(
                                _response(push_message_id, message_type, body)
                            )
                        connection.sendall(_response(message_id, message_type, body))
                    finally:
                        with self._condition:
                            self._active_requests -= 1
                            self._condition.notify_all()
                    if self.close_after_response:
                        return
        except (EOFError, OSError, TimeoutError):
            return
        except BaseException as error:
            with self._condition:
                self.errors.append(error)
        finally:
            with self._condition:
                self._connections.discard(connection)
                self._workers.discard(current)
                self._condition.notify_all()


def _read_exact(connection: socket.socket, size: int) -> bytes:
    output = bytearray()
    while len(output) < size:
        chunk = connection.recv(size - len(output))
        if not chunk:
            raise EOFError("stress connection closed")
        output.extend(chunk)
    return bytes(output)


def _read_request(connection: socket.socket) -> tuple[int, int, bytes]:
    header = _read_exact(connection, 12)
    if header[0] != 0x0C:
        raise AssertionError(f"invalid stress request prefix: {header[0]:#x}")
    first_length = int.from_bytes(header[6:8], "little")
    second_length = int.from_bytes(header[8:10], "little")
    if first_length != second_length or first_length < 2:
        raise AssertionError("invalid stress request length")
    return (
        int.from_bytes(header[1:5], "little"),
        int.from_bytes(header[10:12], "little"),
        _read_exact(connection, first_length - 2),
    )


def _file_request(payload: bytes) -> tuple[int, int]:
    if len(payload) != 308:
        raise AssertionError(f"invalid stress file request length: {len(payload)}")
    token = int.from_bytes(payload[:4], "little")
    requested_size = int.from_bytes(payload[4:8], "little")
    path = payload[8:].split(b"\x00", 1)[0]
    if path != PATH.encode("ascii"):
        raise AssertionError(f"invalid stress request path: {path!r}")
    return token, requested_size


def _response(message_id: int, message_type: int, payload: bytes) -> bytes:
    size = len(payload).to_bytes(2, "little")
    return (
        b"\xb1\xcb\x74\x00\x00"
        + message_id.to_bytes(4, "little")
        + b"\x00"
        + message_type.to_bytes(2, "little")
        + size
        + size
        + payload
    )


def _handshake_payload() -> bytes:
    payload = bytearray(189)
    payload[1:3] = (2026).to_bytes(2, "little")
    payload[3:9] = bytes((15, 8, 30, 10, 0, 0))
    payload[42:46] = (20260815).to_bytes(4, "little")
    payload[50:54] = (20260815).to_bytes(4, "little")
    payload[68:152] = b"native-stress".ljust(84, b"\x00")
    payload[160:189] = b"eltdx-3-stress".ljust(29, b"\x00")
    return bytes(payload)
