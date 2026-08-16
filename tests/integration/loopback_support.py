"""Deterministic standard-library loopback support for native runtime tests."""

from __future__ import annotations

import socket
import threading
from collections.abc import Callable, Sequence


ConnectionHandler = Callable[[socket.socket], None]


class ScriptedServer:
    def __init__(self, handlers: Sequence[ConnectionHandler]) -> None:
        self._handlers = tuple(handlers)
        self._listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self._acceptor: threading.Thread | None = None
        self._workers: list[threading.Thread] = []
        self._condition = threading.Condition()
        self._accepted = 0
        self._finished = 0
        self._closing = False
        self.errors: list[BaseException] = []
        self.host = ""

    def __enter__(self) -> ScriptedServer:
        self._listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self._listener.bind(("127.0.0.1", 0))
        self._listener.listen()
        address, port = self._listener.getsockname()
        self.host = f"{address}:{port}"
        self._acceptor = threading.Thread(
            target=self._serve,
            name="eltdx-native-loopback-acceptor",
            daemon=True,
        )
        self._acceptor.start()
        return self

    def __exit__(self, exc_type, exc, tb) -> None:
        with self._condition:
            self._closing = True
        self._listener.close()
        if self._acceptor is not None:
            self._acceptor.join(timeout=3)
        for worker in self._workers:
            worker.join(timeout=3)
        alive = [worker.name for worker in self._workers if worker.is_alive()]
        if exc_type is None and alive:
            raise AssertionError(f"loopback workers did not stop: {alive!r}")
        if exc_type is None and self.errors:
            raise AssertionError(f"loopback server failed: {self.errors!r}")

    @property
    def accepted_count(self) -> int:
        with self._condition:
            return self._accepted

    def wait_for_connections(self, count: int, timeout: float = 3.0) -> bool:
        with self._condition:
            return self._condition.wait_for(
                lambda: self._accepted >= count,
                timeout=timeout,
            )

    def wait_for_handlers(self, count: int, timeout: float = 3.0) -> bool:
        with self._condition:
            return self._condition.wait_for(
                lambda: self._finished >= count,
                timeout=timeout,
            )

    def _serve(self) -> None:
        try:
            for index, handler in enumerate(self._handlers):
                connection, _ = self._listener.accept()
                connection.settimeout(3)
                with self._condition:
                    self._accepted += 1
                    self._condition.notify_all()
                worker = threading.Thread(
                    target=self._run_handler,
                    args=(connection, handler),
                    name=f"eltdx-native-loopback-{index}",
                    daemon=True,
                )
                self._workers.append(worker)
                worker.start()
        except OSError as error:
            with self._condition:
                closing = self._closing
            if not closing:
                self.errors.append(error)
        except BaseException as error:
            self.errors.append(error)

    def _run_handler(
        self,
        connection: socket.socket,
        handler: ConnectionHandler,
    ) -> None:
        try:
            with connection:
                handler(connection)
        except BaseException as error:
            self.errors.append(error)
        finally:
            with self._condition:
                self._finished += 1
                self._condition.notify_all()


def read_exact(connection: socket.socket, size: int) -> bytes:
    output = bytearray()
    while len(output) < size:
        chunk = connection.recv(size - len(output))
        if not chunk:
            raise EOFError("connection closed before the expected bytes arrived")
        output.extend(chunk)
    return bytes(output)


def read_request(connection: socket.socket) -> tuple[int, int, bytes]:
    header = read_exact(connection, 12)
    if header[0] != 0x0C:
        raise AssertionError(f"invalid request prefix: {header[0]:#x}")
    first_length = int.from_bytes(header[6:8], "little")
    second_length = int.from_bytes(header[8:10], "little")
    if first_length != second_length or first_length < 2:
        raise AssertionError("invalid request length fields")
    return (
        int.from_bytes(header[1:5], "little"),
        int.from_bytes(header[10:12], "little"),
        read_exact(connection, first_length - 2),
    )


def response_bytes(message_id: int, message_type: int, payload: bytes) -> bytes:
    size = len(payload).to_bytes(2, "little")
    return (
        b"\xb1\xcb\x74\x00"
        + b"\x00"
        + message_id.to_bytes(4, "little")
        + b"\x00"
        + message_type.to_bytes(2, "little")
        + size
        + size
        + payload
    )


def handshake_payload() -> bytes:
    payload = bytearray(189)
    payload[1:3] = (2026).to_bytes(2, "little")
    payload[3:9] = bytes((15, 8, 30, 10, 0, 0))
    payload[42:46] = (20260815).to_bytes(4, "little")
    payload[50:54] = (20260815).to_bytes(4, "little")
    payload[68:152] = b"native-loopback".ljust(84, b"\x00")
    payload[160:189] = b"eltdx-3-runtime".ljust(29, b"\x00")
    return bytes(payload)


def answer_handshake(connection: socket.socket, payload: bytes | None = None) -> None:
    message_id, message_type, request_payload = read_request(connection)
    if message_type != 0x000D or request_payload != b"\x01":
        raise AssertionError("expected one native handshake request")
    connection.sendall(
        response_bytes(
            message_id,
            message_type,
            handshake_payload() if payload is None else payload,
        )
    )


def wait_for_peer_close(connection: socket.socket) -> None:
    if connection.recv(1) != b"":
        raise AssertionError("expected native runtime to close the Slot socket")
