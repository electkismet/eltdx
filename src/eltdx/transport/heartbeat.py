from __future__ import annotations

import threading
from collections.abc import Callable


class HeartbeatLoop:
    def __init__(
        self,
        stop_event: threading.Event,
        interval: float,
        send_heartbeat: Callable[[], None],
        is_connected: Callable[[], bool],
        on_error: Callable[[], None],
    ) -> None:
        self._stop_event = stop_event
        self._interval = interval
        self._send_heartbeat = send_heartbeat
        self._is_connected = is_connected
        self._on_error = on_error

    def run(self) -> None:
        while not self._stop_event.wait(self._interval):
            if not self._is_connected():
                return
            try:
                self._send_heartbeat()
            except Exception:
                self._on_error()
                return
