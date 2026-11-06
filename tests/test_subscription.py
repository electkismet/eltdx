"""订阅式实时行情推送（QuoteApi.subscribe / subscribe_hq）测试。"""

from __future__ import annotations

import pytest

from eltdx import TdxClient
from eltdx import hosts as hosts_module
from eltdx.api import QuoteSubscription
from eltdx.hosts import HostProbeResult
from eltdx.models import QuoteRefreshPage, QuoteRefreshRecord
from eltdx.protocol.constants import TYPE_REFRESH_STREAM


@pytest.fixture(autouse=True)
def local_probe_results(tmp_path, monkeypatch):
    monkeypatch.setenv("ELTDX_DATA_DIR", str(tmp_path))
    monkeypatch.setattr(
        hosts_module,
        "probe_host",
        lambda host, **kwargs: HostProbeResult(host=host, ok=True, latency_ms=1.0),
    )


def _record(code: str, price: float) -> QuoteRefreshRecord:
    exchange, number = code[:2], code[2:]
    return QuoteRefreshRecord(
        exchange=exchange,
        market_id=0,
        code=number,
        active=1,
        update_time_raw=153306,
        last_price=price,
        last_close_price=price - 0.1,
        open_price=price - 0.2,
        high_price=price,
        low_price=price - 0.3,
        status_or_reserved_raw=0,
        total_hand=100,
        current_hand=1,
        amount=1000.0,
        amount_raw=0,
        inside_dish=0,
        outer_disc=0,
        unknown_after_outer_raw=0,
        open_amount_raw=0,
        open_amount_yuan=0.0,
        buy_levels=(),
        sell_levels=(),
        tail_raw=b"",
    )


class ScriptedPushTransport:
    """按脚本返回 refresh 首帧与后续 push 帧的最小 transport。"""

    def __init__(self, pushes: list[QuoteRefreshPage | None]) -> None:
        self._pushes = list(pushes)
        self.refresh_calls: list[list[str]] = []
        self.poll_calls: list[float | None] = []

    def connect(self) -> None:
        pass

    def close(self) -> None:
        pass

    def request(self, command: str) -> str:
        return "pong"

    def execute(self, command: int, payload=None):
        assert command == TYPE_REFRESH_STREAM
        codes = list((payload or {}).get("codes") or [])
        self.refresh_calls.append(codes)
        return QuoteRefreshPage(
            tuple(codes),
            tuple(_record(code, 10.0) for code in codes),
            decoded_payload=b"",
        )

    def poll_push(self, timeout: float | None = 0.0, *, parse: bool = False):
        assert parse is True
        self.poll_calls.append(timeout)
        return self._pushes.pop(0) if self._pushes else None


def _client(transport) -> TdxClient:
    return TdxClient(transport=transport)


def test_subscribe_dispatches_initial_snapshot_and_pushes() -> None:
    push = QuoteRefreshPage(
        ("sz000001",),
        (_record("sz000001", 10.5),),
        decoded_payload=b"",
    )
    transport = ScriptedPushTransport([push, None])
    client = _client(transport)

    seen: list[tuple[str, float]] = []

    def on_quote(record) -> None:
        seen.append((record.full_code, record.last_price))

    sub = client.quotes.subscribe(["sh600000", "sz000001"], on_quote)
    assert isinstance(sub, QuoteSubscription)
    assert sub.codes == ("sh600000", "sz000001")

    sub.run(duration=0.0)

    # 首帧快照 2 条 + push 1 条
    assert seen == [
        ("sh600000", 10.0),
        ("sz000001", 10.0),
        ("sz000001", 10.5),
    ]
    assert sub.dispatched == 3
    assert transport.refresh_calls == [["sh600000", "sz000001"]]
    assert sub.stopped is True


def test_run_once_returns_dispatch_count() -> None:
    transport = ScriptedPushTransport([None])
    client = _client(transport)
    sub = client.quotes.subscribe("sz000001", lambda record: None)
    assert sub.run_once() == 0
    assert sub.frames == 1


def test_filter_codes_drops_foreign_records() -> None:
    push = QuoteRefreshPage(
        ("sz000001", "bj430047"),
        (_record("sz000001", 11.0), _record("bj430047", 99.0)),
        decoded_payload=b"",
    )
    transport = ScriptedPushTransport([push, None])
    client = _client(transport)
    seen: list[str] = []
    sub = client.quotes.subscribe(["sz000001"], lambda r: seen.append(r.full_code))
    sub.run(duration=0.0)
    assert seen == ["sz000001", "sz000001"]  # 首帧 + push 中的 sz000001


def test_stop_from_callback_terminates_run() -> None:
    transport = ScriptedPushTransport([None] * 50)
    client = _client(transport)
    holder: dict[str, QuoteSubscription] = {}

    def on_quote(record) -> None:
        holder["sub"].stop()

    sub = client.quotes.subscribe("sz000001", on_quote)
    holder["sub"] = sub
    sub.run(duration=5.0)
    assert sub.stopped is True
    # 首帧派发时即停止，后续一次 poll 都不发生
    assert transport.poll_calls == []


def test_on_error_receives_callback_exception() -> None:
    push = QuoteRefreshPage(
        ("sz000001",),
        (_record("sz000001", 12.0),),
        decoded_payload=b"",
    )
    transport = ScriptedPushTransport([push, None])
    client = _client(transport)
    errors: list[BaseException] = []

    def boom(record) -> None:
        raise RuntimeError("callback failed")

    sub = client.quotes.subscribe("sz000001", boom, on_error=errors.append)
    sub.run(duration=0.0)
    assert len(errors) == 2  # 首帧 + push 各一次
    assert str(errors[0]) == "callback failed"
    assert sub.dispatched == 2


class ExplodingPollTransport:
    """``poll_push`` 持续抛错的 transport，用于验证轮询异常的兜底路径。"""

    def __init__(self, error: BaseException | None = None) -> None:
        self._error = error if error is not None else TimeoutError("poll timed out")
        self.poll_calls = 0

    def connect(self) -> None:
        pass

    def close(self) -> None:
        pass

    def request(self, command: str) -> str:
        return "pong"

    def execute(self, command: int, payload=None):
        assert command == TYPE_REFRESH_STREAM
        codes = list((payload or {}).get("codes") or [])
        return QuoteRefreshPage(tuple(codes), (), decoded_payload=b"")

    def poll_push(self, timeout: float | None = 0.0, *, parse: bool = False):
        assert parse is True
        self.poll_calls += 1
        raise self._error


def test_on_error_receives_poll_exception_instead_of_raising() -> None:
    """回归：`poll_push` 出错此前直接向上抛，`on_error` 一次都收不到。"""

    transport = ExplodingPollTransport(TimeoutError("poll timed out"))
    client = _client(transport)
    errors: list[BaseException] = []
    sub = client.quotes.subscribe("sz000001", lambda record: None, on_error=errors.append)

    assert sub.run_once() == 0  # 不再抛错，本轮按空帧处理
    assert len(errors) == 1  # on_error 收到一次
    assert isinstance(errors[0], TimeoutError)
    assert sub.frames == 0  # 出错不计帧


def test_poll_exception_does_not_break_the_run_loop() -> None:
    """轮询异常按暂时性处理：交给 `on_error` 后继续下一轮，`run()` 不退出。

    若异常直接弹出循环，`on_error` 只会收到 1 次；收到 2 次说明循环继续了。
    """

    transport = ExplodingPollTransport()
    client = _client(transport)
    errors: list[BaseException] = []

    def handler(exc: BaseException) -> None:
        errors.append(exc)
        if len(errors) >= 2:
            sub.stop()

    sub = client.quotes.subscribe("sz000001", lambda record: None, on_error=handler)
    sub.run()

    assert len(errors) == 2
    assert transport.poll_calls == 2


def test_poll_exception_still_propagates_without_on_error() -> None:
    """未设置 `on_error` 时保留向上抛错的行为，不静默吞掉。"""

    transport = ExplodingPollTransport(TimeoutError("poll timed out"))
    client = _client(transport)
    sub = client.quotes.subscribe("sz000001", lambda record: None)
    with pytest.raises(TimeoutError):
        sub.run_once()


def test_subscribe_hq_is_alias() -> None:
    client = _client(ScriptedPushTransport([]))
    sub = client.quotes.subscribe_hq("sz000001", lambda record: None)
    assert isinstance(sub, QuoteSubscription)


def test_subscribe_rejects_empty_and_non_callable() -> None:
    client = _client(ScriptedPushTransport([]))
    with pytest.raises(ValueError):
        client.quotes.subscribe([], lambda record: None)
    with pytest.raises(ValueError):
        client.quotes.subscribe("   ", lambda record: None)
    with pytest.raises(TypeError):
        client.quotes.subscribe("sz000001", "not-callable")


def test_activate_returns_first_page_and_reactivates() -> None:
    transport = ScriptedPushTransport([])
    client = _client(transport)
    sub = client.quotes.subscribe(["sz000001"], lambda record: None)
    page = sub.activate()
    assert page.count == 1
    assert transport.refresh_calls == [["sz000001"]]
    sub.activate()
    assert transport.refresh_calls == [["sz000001"], ["sz000001"]]


def test_timeout_is_forwarded_to_poll_push() -> None:
    transport = ScriptedPushTransport([None])
    client = _client(transport)
    sub = client.quotes.subscribe("sz000001", lambda record: None, timeout=0.25)
    sub.run_once()
    assert transport.poll_calls == [0.25]


@pytest.mark.parametrize("interval", ["default", None, 1.0])
def test_run_dispatches_periodic_refresh_without_server_push(monkeypatch, interval) -> None:
    from dataclasses import replace
    from eltdx.api import subscription

    clock = [0.0]
    monkeypatch.setattr(subscription.time, "monotonic", lambda: clock[0])

    class UpdatingTransport(ScriptedPushTransport):
        def execute(self, command, payload=None):
            page = super().execute(command, payload)
            return replace(page, records=(_record("sz000001", 10.0 + len(self.refresh_calls)),))

        def poll_push(self, timeout=0.0, *, parse=False):
            clock[0] += 1.0
            return super().poll_push(timeout, parse=parse)

    transport = UpdatingTransport([])
    seen = []
    kwargs = {} if interval == "default" else {"reactivate_interval": interval}
    sub = _client(transport).quotes.subscribe("sz000001", lambda r: seen.append(r.last_price), **kwargs)
    sub.run(duration=9.0)
    assert len(transport.refresh_calls) == (2 if interval == "default" else (1 if interval is None else 9))
    assert seen == [11.0 + i for i in range(len(transport.refresh_calls))]


def test_renewal_uses_most_recently_received_server_time(monkeypatch) -> None:
    from dataclasses import replace
    from eltdx.api import subscription
    clock = [0.0]
    monkeypatch.setattr(subscription.time, "monotonic", lambda: clock[0])

    class CursorTransport(ScriptedPushTransport):
        def __init__(self):
            super().__init__([QuoteRefreshPage(("sz000001",), (
                replace(_record("sz000001", 11.0), update_time_raw=102912),
                replace(_record("sz000001", 10.5), update_time_raw=102906),
            ), b"")])
            self.payloads = []

        def execute(self, command, payload=None):
            self.payloads.append(payload)
            return super().execute(command, payload)

    transport = CursorTransport()
    # Initial server time precedes both pushes.
    original = transport.execute
    def execute(command, payload=None):
        page = original(command, payload)
        return replace(page, records=(replace(page.records[0], update_time_raw=102851),))
    transport.execute = execute
    sub = _client(transport).quotes.subscribe("sz000001", lambda r: None)
    sub.activate()
    sub.run_once(timeout=0)
    clock[0] = 7.0
    sub.run_once(timeout=0)
    assert transport.payloads[0]["cursors"] == {"sz000001": 0}
    assert transport.payloads[1]["cursors"] == {"sz000001": 102906}


def test_multislot_subscription_keeps_groups_on_their_original_connections(monkeypatch) -> None:
    from contextlib import contextmanager
    from eltdx.api import subscription
    clock = [0.0]
    monkeypatch.setattr(subscription.time, "monotonic", lambda: clock[0])

    class Pool(ScriptedPushTransport):
        pool_size = 2
        def __init__(self):
            super().__init__([])
            self.pins = []
            self.closed = []
            self.calls = []

        @contextmanager
        def pin(self):
            index = len(self.pins)
            pool = self
            class Pin:
                def execute(self, command, payload=None):
                    pool.calls.append((index, payload))
                    return pool.execute(command, payload)
            pin = Pin()
            self.pins.append(pin)
            try:
                yield pin
            finally:
                self.closed.append(index)

        def poll_push(self, timeout=0.0, *, parse=False):
            clock[0] += 1.0
            return super().poll_push(timeout, parse=parse)

    pool = Pool()
    codes = [f"sz{i:06}" for i in range(1, 201)]
    seen = []
    sub = _client(pool).quotes.subscribe(codes, lambda r: seen.append(r.full_code))
    sub.run(duration=9)
    assert len(pool.pins) == 2
    assert [slot for slot, _ in pool.calls] == [0, 1, 0, 1]
    assert [len(payload["codes"]) for _, payload in pool.calls] == [100]*4
    assert pool.calls[0][1]["codes"] == pool.calls[2][1]["codes"]
    assert pool.calls[1][1]["codes"] == pool.calls[3][1]["codes"]
    assert len(seen) == 400
    assert sorted(pool.closed) == [0, 1]


@pytest.mark.parametrize("failure", ["initial", "callback"])
def test_subscription_releases_pin_on_initial_failure(failure) -> None:
    from contextlib import contextmanager

    class Pool(ScriptedPushTransport):
        pool_size = 1
        released = False
        @contextmanager
        def pin(self):
            try:
                yield self
            finally:
                self.released = True

        def execute(self, command, payload=None):
            if failure == "initial":
                raise RuntimeError("initial failed")
            return super().execute(command, payload)

    pool = Pool([])
    def callback(record):
        raise RuntimeError("callback failed")
    sub = _client(pool).quotes.subscribe("sz000001", callback)
    with pytest.raises(RuntimeError, match="failed"):
        sub.run()
    assert pool.released
    assert sub.stopped


def test_insufficient_slots_fails_before_sending_requests() -> None:
    transport = ScriptedPushTransport([])
    sub = _client(transport).quotes.subscribe([f"sz{i:06}" for i in range(1, 102)], lambda r: None)
    with pytest.raises(ValueError, match="2 pool slots"):
        sub.activate()
    assert transport.refresh_calls == []


@pytest.mark.parametrize("value", [0, -1, float("nan"), float("inf")])
def test_invalid_renewal_interval_is_rejected(value) -> None:
    with pytest.raises(ValueError):
        _client(ScriptedPushTransport([])).quotes.subscribe("sz000001", lambda r: None, reactivate_interval=value)

@pytest.mark.parametrize("error", [RuntimeError, KeyboardInterrupt])
def test_partial_pin_acquisition_releases_previous_connections(error) -> None:
    from contextlib import contextmanager

    class Pool(ScriptedPushTransport):
        pool_size = 2
        acquired = 0
        released = 0

        @contextmanager
        def pin(self):
            if self.acquired:
                raise error("pin failed")
            self.acquired += 1
            try:
                yield self
            finally:
                self.released += 1

    pool = Pool([])
    sub = _client(pool).quotes.subscribe([f"sz{i:06}" for i in range(1, 102)], lambda r: None)
    with pytest.raises(error, match="pin failed"):
        sub.activate()
    assert pool.released == 1
    assert pool.refresh_calls == []

def test_server_cursors_follow_new_session_after_previous_close_snapshot() -> None:
    from dataclasses import replace

    transport = ScriptedPushTransport([])
    payloads = []
    original = transport.execute
    def execute(command, payload=None):
        payloads.append(payload)
        return original(command, payload)
    transport.execute = execute
    sub = _client(transport).quotes.subscribe("sz000001", lambda r: None)
    try:
        sub.activate()
        assert sub._cursors["sz000001"] == 153306
        # An overnight renewal can still return the previous closing value.
        sub.activate()
        assert payloads[-1]["cursors"] == {"sz000001": 153306}
        # The next session's morning cursor must replace that closing value.
        sub._remember([replace(_record("sz000001", 11.0), update_time_raw=93001)])
        sub.activate()
        assert payloads[-1]["cursors"] == {"sz000001": 93001}
    finally:
        sub.close()
