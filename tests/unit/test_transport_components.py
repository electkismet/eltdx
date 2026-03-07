from __future__ import annotations

import threading

from eltdx.protocol.frame import ResponseFrame
from eltdx.transport.heartbeat import HeartbeatLoop
from eltdx.transport.reader import ResponseReader
from eltdx.transport.router import ResponseRouter


def _make_response(msg_id: int = 7) -> ResponseFrame:
    return ResponseFrame(
        control=0x1C,
        msg_id=msg_id,
        msg_type=0x0450,
        zip_length=0,
        length=0,
        data=b"",
        raw=b"",
    )


def test_response_router_registers_and_delivers() -> None:
    router = ResponseRouter()

    message_queue = router.register(7)
    response = _make_response(7)
    router.deliver(response)

    assert message_queue.get_nowait() == response

    router.unregister(7)
    router.clear()


def test_response_reader_decodes_and_routes(monkeypatch) -> None:
    stop_event = threading.Event()
    responses: list[ResponseFrame] = []
    errors: list[str] = []

    def on_response(response: ResponseFrame) -> None:
        responses.append(response)
        stop_event.set()

    def on_error() -> None:
        errors.append("error")

    monkeypatch.setattr("eltdx.transport.reader.read_response_frame", lambda sock: b"raw")
    monkeypatch.setattr("eltdx.transport.reader.decode_response", lambda raw: _make_response(11))

    reader = ResponseReader(stop_event, on_response, on_error)
    reader.run(object())

    assert [response.msg_id for response in responses] == [11]
    assert errors == []


def test_heartbeat_loop_sends_until_stopped() -> None:
    stop_event = threading.Event()
    sent: list[str] = []
    errors: list[str] = []

    def send_heartbeat() -> None:
        sent.append("tick")
        stop_event.set()

    def on_error() -> None:
        errors.append("error")

    heartbeat = HeartbeatLoop(stop_event, 0, send_heartbeat, lambda: True, on_error)
    heartbeat.run()

    assert sent == ["tick"]
    assert errors == []
