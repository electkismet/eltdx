"""Legacy subscription contracts; authored only until all implementation ends."""
import sys
from contextlib import contextmanager
from types import SimpleNamespace

import pytest

from eltdx.api import subscription
from eltdx.api.quotes import QuoteApi
from eltdx.models.quote import QuoteRefreshPage, QuoteRefreshRecord

CODE = "sh" + str(600_001)
OTHER = "sh" + str(600_002)


def record(code=CODE, cursor=150000):
    return QuoteRefreshRecord(
        exchange=code[:2], market_id=1, code=code[2:], active=1,
        update_time_raw=cursor, last_price=10.0, last_close_price=9.0,
        open_price=9.5, high_price=10.0, low_price=9.0,
        status_or_reserved_raw=0, total_hand=100, current_hand=1,
        amount=1000.0, amount_raw=0, inside_dish=0, outer_disc=0,
        unknown_after_outer_raw=0, open_amount_raw=0, open_amount_yuan=0.0,
        buy_levels=(), sell_levels=(), tail_raw=b"",
    )


class Transport:
    pool_size = 1

    def __init__(self, pushes=()):
        self.pushes = iter(pushes)
        self.calls = []

    def execute(self, command, payload):
        self.calls.append(("refresh", payload))
        return QuoteRefreshPage(tuple(payload["codes"]), (), b"")

    def poll_push(self, *, timeout, parse):
        self.calls.append(("poll", timeout, parse))
        value = next(self.pushes, None)
        if isinstance(value, BaseException):
            raise value
        return value


def handle(transport=None, callback=None, **kwargs):
    transport = transport or Transport()
    return subscription.QuoteSubscription(QuoteApi(transport), [CODE], callback or (lambda row: None), **kwargs)


def test_dispatch_remembers_all_cursors_before_first_callback_and_allows_next_morning():
    observed = []
    sub = handle(callback=lambda row: observed.append((row, sub._cursors[CODE])))
    afternoon, morning = record(), record(cursor=93000)
    assert sub._dispatch((afternoon, morning)) == 2
    assert observed == [(afternoon, 93000), (morning, 93000)]
    assert sub.dispatched == 2


def test_dispatch_filters_by_real_record_type_and_original_identity():
    seen = []
    sub = handle(callback=seen.append)
    wanted, foreign = record(), record(OTHER)
    assert sub._dispatch((SimpleNamespace(full_code=CODE), foreign, wanted)) == 1
    assert seen == [wanted] and seen[0] is wanted
    assert OTHER not in sub._cursors
    sub._filter_codes = False
    assert sub._dispatch((foreign,)) == 1 and seen[-1] is foreign


def test_dispatch_keeps_two_full_code_reads_and_generator_exhaustion():
    reads = []

    class Observable(QuoteRefreshRecord):
        @property
        def full_code(self):
            reads.append("code")
            return CODE

    base = record()
    value = Observable(**{name: getattr(base, name) for name in base.__dataclass_fields__})
    sub = handle()
    sub._remember([value])
    assert reads == ["code", "code"]
    assert sub._dispatch(iter([value])) == 0
    assert sub.dispatched == 0


def test_callback_error_has_active_python_exception_and_increments_before_call():
    error = ValueError("callback")
    observed = []

    def callback(row):
        assert sub.dispatched == 1
        raise error

    def on_error(value):
        observed.append((value, sys.exc_info()[1]))

    sub = handle(callback=callback, on_error=on_error)
    assert sub._dispatch((record(),)) == 1
    assert observed == [(error, error)]


@pytest.mark.parametrize("error", [ValueError("callback"), KeyboardInterrupt()])
def test_callback_error_identity_propagates_without_fallback(error):
    def callback(row):
        raise error

    sub = handle(callback=callback)
    with pytest.raises(type(error)) as raised:
        sub._dispatch((record(),))
    assert raised.value is error and sub.dispatched == 1


def test_callback_keyboard_interrupt_bypasses_on_error():
    errors = []
    failure = KeyboardInterrupt()

    def callback(row):
        raise failure

    sub = handle(callback=callback, on_error=errors.append)
    with pytest.raises(KeyboardInterrupt) as raised:
        sub._dispatch((record(),))
    assert raised.value is failure and not errors


def test_refresh_failure_still_schedules_next_renewal(monkeypatch):
    monkeypatch.setattr(subscription.time, "monotonic", lambda: 20.0)
    failure = RuntimeError("refresh")

    def refresh(*args, **kwargs):
        raise failure

    sub = handle()
    sub._group_apis = [SimpleNamespace(refresh=refresh)]
    sub._next_renew = [None]
    with pytest.raises(RuntimeError) as raised:
        sub._refresh_group(0)
    assert raised.value is failure and sub._next_renew == [27.0]


def test_refresh_group_lookup_fails_before_finally_schedule(monkeypatch):
    sub = handle()
    monkeypatch.setattr(subscription.time, "monotonic", lambda: pytest.fail("clock read"))
    with pytest.raises(IndexError):
        sub._refresh_group(1)
    assert sub._next_renew == []


def test_refresh_disabled_interval_avoids_clock_read_and_keeps_cursor_map(monkeypatch):
    calls = []
    sub = handle(reactivate_interval=None)
    sub._cursors[CODE] = 93000
    page = QuoteRefreshPage((CODE,), (record(cursor=93001),), b"raw")
    sub._group_apis = [SimpleNamespace(refresh=lambda codes, **kwargs: calls.append((codes, kwargs)) or page)]
    sub._next_renew = [2.0]
    monkeypatch.setattr(subscription.time, "monotonic", lambda: pytest.fail("clock read"))
    assert sub._refresh_group(0) is page
    assert calls == [([CODE], {"cursors": {CODE: 93000}})]
    assert sub._cursors[CODE] == 93001 and sub._next_renew == [None]


def test_run_once_caps_poll_at_next_renewal_and_counts_none_frames(monkeypatch):
    transport = Transport()
    sub = handle(transport, timeout=10.0)
    sub._next_renew = [4.0, None, 5.0]
    monkeypatch.setattr(subscription.time, "monotonic", lambda: 2.0)
    assert sub.run_once() == 0
    assert transport.calls == [("poll", 2.0, True)] and sub.frames == 1


def test_run_once_renewal_then_push_counts_both_and_keeps_order(monkeypatch):
    first, second = record(cursor=93000), record(cursor=93001)
    transport = Transport([QuoteRefreshPage((CODE,), (second,), b"")])
    seen = []
    sub = handle(transport, seen.append)
    sub._next_renew = [0.0]

    def refresh(index):
        sub._next_renew[index] = 7.0
        return QuoteRefreshPage((CODE,), (first,), b"")

    monkeypatch.setattr(sub, "_refresh_group", refresh)
    monkeypatch.setattr(subscription.time, "monotonic", lambda: 1.0)
    assert sub.run_once() == 2 and seen == [first, second]
    assert sub._cursors[CODE] == 93001 and sub.frames == 1


def test_poll_error_runs_on_error_in_exception_scope_and_does_not_count_frame():
    failure = OSError("poll")
    seen = []
    sub = handle(Transport([failure]), on_error=lambda error: seen.append((error, sys.exc_info()[1])))
    assert sub.run_once() == 0 and sub.frames == 0
    assert seen == [(failure, failure)]


def test_stopped_run_once_ignores_bad_timeout_and_makes_no_calls():
    transport = Transport()
    sub = handle(transport)
    sub.stop()
    assert sub.run_once(timeout=-1) == 0 and transport.calls == []


def test_stop_during_renewal_dispatch_prevents_poll(monkeypatch):
    transport = Transport()
    sub = handle(transport, callback=lambda row: sub.stop())
    sub._next_renew = [0.0]
    monkeypatch.setattr(subscription.time, "monotonic", lambda: 1.0)
    monkeypatch.setattr(sub, "_refresh_group", lambda index: QuoteRefreshPage((CODE,), (record(),), b""))
    assert sub.run_once() == 1 and transport.calls == []


def test_activation_merge_error_does_not_release_successfully_prepared_pins(monkeypatch):
    sub = handle()
    sub._groups = ((CODE,), (OTHER,))
    released = []
    monkeypatch.setattr(sub, "_prepare_connections", lambda: None)
    monkeypatch.setattr(sub, "_refresh_group", lambda index: object())
    monkeypatch.setattr(sub, "_release_connections", lambda: released.append(True))
    with pytest.raises(AttributeError):
        sub.activate()
    assert released == []


@pytest.mark.parametrize("failure", [RuntimeError("activation"), KeyboardInterrupt()])
def test_activation_refresh_failure_releases_and_preserves_error(failure, monkeypatch):
    sub = handle()
    released = []

    def refresh(index):
        raise failure

    monkeypatch.setattr(sub, "_prepare_connections", lambda: None)
    monkeypatch.setattr(sub, "_refresh_group", refresh)
    monkeypatch.setattr(sub, "_release_connections", lambda: released.append(True))
    with pytest.raises(type(failure)) as raised:
        sub.activate()
    assert released == [True] and raised.value is failure


def test_zero_duration_run_still_polls_once_and_closes(monkeypatch):
    transport = Transport()
    sub = handle(transport)
    monkeypatch.setattr(subscription.time, "monotonic", lambda: 1.0)
    sub.run(duration=0.0)
    assert transport.calls[-1] == ("poll", 0.0, True)
    assert sub.frames == 1 and sub.stopped and sub._group_apis == []


def test_run_interrupt_stops_and_clears_state_before_stack_close():
    transport = Transport([KeyboardInterrupt()])
    sub = handle(transport)
    observed = []

    class Stack:
        def close(self):
            observed.append((sub.stopped, sub._pin_stack, sub._group_apis, sub._next_renew, sub._poll_api))

    sub._pin_stack = Stack()
    with pytest.raises(KeyboardInterrupt):
        sub.run()
    assert observed == [(True, None, [], [], sub._api)]


def test_group_pins_load_metadata_before_acquisition_and_release_in_reverse():
    events = []

    class Pool(Transport):
        pool_size = 2

        @contextmanager
        def pin(self):
            index = len([event for event in events if event[0] == "pin"])
            events.append(("pin", index))
            try:
                yield Transport()
            finally:
                events.append(("release", index))

    def resolve(*args):
        events.append(("metadata", args))

    api = QuoteApi(Pool(), price_resolver=resolve)
    codes = ["sz" + str(index).zfill(6) for index in range(1, 102)]
    sub = subscription.QuoteSubscription(api, codes, lambda row: None)
    sub.activate()
    assert events[0][0] == "metadata" and events[1:3] == [("pin", 0), ("pin", 1)]
    assert len(sub._group_apis) == 2 and sub._poll_api._transport is api._transport
    sub.close()
    assert events[-2:] == [("release", 1), ("release", 0)]
