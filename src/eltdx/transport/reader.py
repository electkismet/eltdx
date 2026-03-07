from __future__ import annotations

import socket
import threading
from collections.abc import Callable

from eltdx.protocol.frame import ResponseFrame, decode_response, read_response_frame


class ResponseReader:
    def __init__(
        self,
        stop_event: threading.Event,
        on_response: Callable[[ResponseFrame], None],
        on_error: Callable[[], None],
    ) -> None:
        self._stop_event = stop_event
        self._on_response = on_response
        self._on_error = on_error

    def run(self, sock: socket.socket) -> None:
        while not self._stop_event.is_set():
            try:
                raw = read_response_frame(sock)
                response = decode_response(raw)
            except Exception:
                self._on_error()
                return
            self._on_response(response)
