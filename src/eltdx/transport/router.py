from __future__ import annotations

import queue
import threading

from eltdx.protocol.frame import ResponseFrame


class ResponseRouter:
    def __init__(self) -> None:
        self._pending: dict[int, queue.Queue[ResponseFrame]] = {}
        self._lock = threading.Lock()

    def register(self, msg_id: int) -> queue.Queue[ResponseFrame]:
        message_queue: queue.Queue[ResponseFrame] = queue.Queue(maxsize=1)
        with self._lock:
            self._pending[msg_id] = message_queue
        return message_queue

    def unregister(self, msg_id: int) -> None:
        with self._lock:
            self._pending.pop(msg_id, None)

    def clear(self) -> None:
        with self._lock:
            self._pending.clear()

    def deliver(self, response: ResponseFrame) -> None:
        with self._lock:
            message_queue = self._pending.get(response.msg_id)
        if message_queue is None:
            return
        try:
            message_queue.put_nowait(response)
        except queue.Full:
            pass
