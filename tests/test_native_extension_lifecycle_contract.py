"""Generation/cleanup sources, deliberately unexecuted before the full gate."""
from __future__ import annotations

import gc
import socket
import threading
import weakref
from types import SimpleNamespace

import pytest

from eltdx._native_abi import load_native
from eltdx.exceptions import TransportError
from eltdx.ex import client as ex_client
from eltdx.ex.transport import sync as ex
from eltdx.mac import client as mac_client
from eltdx.mac import transport as mac


class Socket:
    def __init__(self, events=None):
        self.events = events if events is not None else []

    def settimeout(self, timeout):
        self.events.append(("timeout", timeout))

    def connect(self, address):
        self.events.append(("connect", address))

    def sendall(self, data):
        self.events.append(("send", data))

    def shutdown(self, mode):
        self.events.append(("shutdown", mode))

    def close(self):
        self.events.append("close")


def make_connection(monkeypatch, domain, sock=None):
    sock = sock or Socket()
    module = mac if domain == "mac" else ex
    if domain == "mac":
        connection = mac.MacConnection("host", 123, 15, socket_factory=lambda *a: sock, setup_commands=())
    else:
        monkeypatch.setattr(ex.socket, "socket", lambda *a: sock)
        connection = ex.ExTdxConnection("host", 123, 15, send_setup=False)
    connection.connect()
    return module, connection, sock


def response(monkeypatch, module, connection, parse):
    monkeypatch.setattr(module, "parse_mac_header" if module is mac else "parse_header", lambda value: SimpleNamespace(zipsize=0))
    monkeypatch.setattr(module, "decompress_mac_body" if module is mac else "decompress_body", lambda header, raw: b"body")
    connection._recv_exact = lambda count: b"frame"
    return SimpleNamespace(build_request=lambda: b"request", parse_response=parse)


@pytest.mark.parametrize("domain", ["ex", "mac"])
def test_native_extension_close_retires_handle_and_reconnect_gets_new_generation(monkeypatch, domain):
    module, connection, _ = make_connection(monkeypatch, domain)
    old = connection._native_session
    connection.close()
    assert old.retired
    connection.connect()
    assert not connection._native_session.retired
    assert old.generation != connection._native_session.generation
    assert old.retired


@pytest.mark.parametrize("domain", ["ex", "mac"])
def test_native_extension_reentrant_parser_close_does_not_deadlock_or_publish(monkeypatch, domain):
    module, connection, sock = make_connection(monkeypatch, domain)
    old = connection._native_session

    def parse(body):
        connection.close()
        return "stale"

    with pytest.raises(TransportError, match="关闭|替换"):
        connection.execute(response(monkeypatch, module, connection, parse))
    assert old.retired and connection._sock is None and sock.events[-1] == "close"


@pytest.mark.parametrize("domain", ["ex", "mac"])
def test_native_extension_dto_failure_retains_original_exception_and_owner(monkeypatch, domain):
    module, connection, _ = make_connection(monkeypatch, domain)
    old = connection._native_session
    failure = MemoryError("allocation")

    def parse(body):
        raise failure

    with pytest.raises(MemoryError) as raised:
        connection.execute(response(monkeypatch, module, connection, parse))
    assert raised.value is failure and not old.retired
    assert connection.execute(response(monkeypatch, module, connection, lambda body: 7)) == 7
    connection.close()
    assert old.retired


@pytest.mark.parametrize("domain", ["ex", "mac"])
def test_native_extension_keyboard_interrupt_closes_and_retires_captured_resource(monkeypatch, domain):
    module, connection, sock = make_connection(monkeypatch, domain)
    old = connection._native_session
    failure = KeyboardInterrupt()

    def send(data):
        raise failure

    sock.sendall = send
    command = SimpleNamespace(build_request=lambda: b"request")
    with pytest.raises(KeyboardInterrupt) as raised:
        connection.execute(command)
    assert raised.value is failure and connection._sock is None and old.retired
    assert sock.events[-2:] == [("shutdown", socket.SHUT_RDWR), "close"]


def test_native_mac_keyboard_interrupt_during_connect_keeps_previous_generation(monkeypatch):
    _, connection, old_socket = make_connection(monkeypatch, "mac")
    old = connection._native_session
    next_socket = Socket()
    failure = KeyboardInterrupt()

    def connect(address):
        raise failure

    next_socket.connect = connect
    connection._socket_factory = lambda *a: next_socket
    with pytest.raises(KeyboardInterrupt) as raised:
        connection.connect()
    assert raised.value is failure
    assert connection._sock is old_socket and connection._native_session is old and not old.retired
    assert next_socket.events[-2:] == [("shutdown", socket.SHUT_RDWR), "close"]


@pytest.mark.parametrize("domain", ["ex", "mac"])
def test_native_extension_interrupt_during_dto_leaves_replacement_open(monkeypatch, domain):
    module, connection, old_socket = make_connection(monkeypatch, domain)
    old = connection._native_session
    replacement = Socket()
    failure = KeyboardInterrupt()

    def parse(body):
        if domain == "mac":
            connection._socket_factory = lambda *a: replacement
        else:
            monkeypatch.setattr(ex.socket, "socket", lambda *a: replacement)
        connection.connect()
        raise failure

    with pytest.raises(KeyboardInterrupt) as raised:
        connection.execute(response(monkeypatch, module, connection, parse))
    assert raised.value is failure and old.retired
    assert connection._sock is replacement and not connection._native_session.retired
    assert old_socket.events[-1] == "close" and "close" not in replacement.events


def test_native_extension_signal_cleanup_error_becomes_cause_not_replacement(monkeypatch):
    module, connection, sock = make_connection(monkeypatch, "mac")
    failure, cleanup = KeyboardInterrupt(), ValueError("custom close")

    def send(data):
        raise failure

    def close():
        raise cleanup

    sock.sendall, sock.close = send, close
    with pytest.raises(KeyboardInterrupt) as raised:
        connection.execute(SimpleNamespace(build_request=lambda: b"request"))
    assert raised.value is failure and failure.__cause__ is cleanup
    assert connection._native_session.retired and connection._sock is sock
    # Failed custom cleanup remains owned for a later explicit close.
    sock.close = lambda: None
    connection.close()
    assert connection._sock is None


def test_native_mac_failed_reconnect_preserves_old_generation_and_resource(monkeypatch):
    _, connection, old_socket = make_connection(monkeypatch, "mac")
    old = connection._native_session
    replacement = Socket()

    def fail(address):
        raise OSError("offline")

    replacement.connect = fail
    connection._socket_factory = lambda *a: replacement
    with pytest.raises(TransportError):
        connection.connect()
    assert connection._native_session is old and not old.retired and connection._sock is old_socket
    assert replacement.events[-1] == "close" and "close" not in old_socket.events


@pytest.mark.parametrize("domain", ["ex", "mac"])
def test_native_extension_close_failure_does_not_claim_retirement(monkeypatch, domain):
    _, connection, sock = make_connection(monkeypatch, domain)
    old = connection._native_session

    def close():
        raise ValueError("custom failure")

    sock.close = close
    with pytest.raises(ValueError, match="custom failure"):
        connection.close()
    assert not old.retired and connection._sock is sock


def test_native_extension_closed_holder_releases_socket_reference(monkeypatch):
    _, connection, sock = make_connection(monkeypatch, "mac")
    old = connection._native_session
    reference = weakref.ref(sock)
    connection._socket_factory = lambda *args: None
    connection.close()
    del sock
    gc.collect()
    assert old.retired and reference() is None


def test_native_extension_cycle_is_visible_to_python_gc(monkeypatch):
    _, connection, sock = make_connection(monkeypatch, "mac")
    sock.connection = connection
    connection._socket_factory = lambda *args: None
    reference = weakref.ref(connection)
    del connection, sock
    gc.collect()
    assert reference() is None


def test_native_extension_direct_socket_injection_gets_separate_generation(monkeypatch):
    module, connection, _ = make_connection(monkeypatch, "mac")
    old = connection._native_session
    replacement = Socket()
    connection._sock = replacement
    assert connection.execute(response(monkeypatch, module, connection, lambda body: 3)) == 3
    assert old.retired and connection._native_session.generation != old.generation


def test_native_extension_retired_admission_is_rejected_and_one_shot_commit(monkeypatch):
    module, connection, _ = make_connection(monkeypatch, "mac")
    native = load_native()
    operation = native.extension_operation_begin(connection, None, module)
    marker = object()
    assert native.extension_operation_finish(operation, marker, module) is marker
    with pytest.raises(TransportError):
        native.extension_operation_finish(operation, marker, module)
    operation = native.extension_operation_begin(connection, None, module)
    connection.close()
    with pytest.raises(TransportError):
        native.extension_operation_check(operation, module)
    with pytest.raises(TransportError):
        native.extension_operation_finish(operation, marker, module)


def test_native_extension_admitted_domains_are_independent(monkeypatch):
    module, first, _ = make_connection(monkeypatch, "mac")
    _, second, _ = make_connection(monkeypatch, "mac")
    native = load_native()
    one = native.extension_operation_begin(first, None, module)
    two = native.extension_operation_begin(second, None, module)
    first.close()
    with pytest.raises(TransportError):
        native.extension_operation_check(one, module)
    assert native.extension_operation_finish(two, 5, module) == 5
    second.close()


def test_native_ex_high_level_replacement_retires_old_handle_after_success(monkeypatch):
    sock = Socket()
    monkeypatch.setattr(ex.socket, "socket", lambda *args: sock)
    client = ex_client.ExTdxClient(hosts=["a"], probe_on_connect=False)
    client._conn.send_setup = False
    client._conn.connect()
    old_connection = client._conn
    old = old_connection._native_session
    candidate = ex.ExTdxConnection("a", 7727, 15, send_setup=False)
    candidate.connect()
    monkeypatch.setattr(ex_client, "ExTdxConnection", lambda *a: candidate)
    client.connect()
    assert old.retired and client._conn is candidate and not candidate._native_session.retired
    with pytest.raises(TransportError):
        old_connection.execute(SimpleNamespace(build_request=lambda: b"stale"))


def test_native_mac_high_level_replacement_closes_old_connection_after_publish(monkeypatch):
    client = mac_client.MacClient(hosts=["a"], socket_factory=lambda *a: Socket())
    _, old_connection, _ = make_connection(monkeypatch, "mac")
    _, candidate, _ = make_connection(monkeypatch, "mac")
    client._conn = old_connection
    old = old_connection._native_session
    candidate.connect = lambda: None
    monkeypatch.setattr(mac_client, "MacConnection", lambda *a, **k: candidate)
    client.connect()
    assert old.retired and old_connection._sock is None and client._conn is candidate


def test_native_extension_close_during_receive_discards_late_reply(monkeypatch):
    module, connection, sock = make_connection(monkeypatch, "ex")
    entered, released = threading.Event(), threading.Event()
    failure = []
    reads = []
    old = connection._native_session

    def receive(count):
        reads.append(count)
        if len(reads) == 1:
            entered.set()
            assert released.wait(5)
        return b"response"

    connection._recv_exact = receive
    monkeypatch.setattr(ex, "parse_header", lambda data: SimpleNamespace(zipsize=0))
    command = SimpleNamespace(build_request=lambda: b"request", parse_response=lambda data: pytest.fail("late reply must not parse"))

    def execute():
        try:
            connection.execute(command)
        except BaseException as error:
            failure.append(error)

    worker = threading.Thread(target=execute)
    worker.start()
    assert entered.wait(5)
    try:
        connection.close()
        assert old.retired
    finally:
        released.set()
        worker.join(5)
    assert not worker.is_alive() and len(failure) == 1 and isinstance(failure[0], TransportError)


@pytest.mark.parametrize("domain", ["ex", "mac"])
def test_native_extension_new_admission_allocation_failure_closes_candidate(monkeypatch, domain):
    sock = Socket()
    native = load_native()
    failure = MemoryError("new operation")

    def fail(*args):
        raise failure

    if domain == "mac":
        connection = mac.MacConnection("a", 1, 15, socket_factory=lambda *args: sock)
        monkeypatch.setattr(native, "extension_attempt_begin", fail)
    else:
        connection = ex.ExTdxConnection("a", 1, 15, send_setup=False)
        monkeypatch.setattr(ex.socket, "socket", lambda *args: sock)
        monkeypatch.setattr(native, "extension_operation_begin", fail)
    with pytest.raises(MemoryError) as raised:
        connection.connect()
    assert raised.value is failure and connection._sock is None and sock.events[-1] == "close"


@pytest.mark.parametrize("domain", ["ex", "mac"])
def test_native_extension_shutdown_attribute_failure_still_attempts_close(monkeypatch, domain):
    module, connection, _ = make_connection(monkeypatch, domain)
    failure, lookup = KeyboardInterrupt(), ValueError("shutdown lookup")

    class ShutdownFailure(Socket):
        @property
        def shutdown(self):
            raise lookup

        def sendall(self, data):
            raise failure

    sock = ShutdownFailure()
    connection._sock = sock
    with pytest.raises(KeyboardInterrupt) as raised:
        connection.execute(SimpleNamespace(build_request=lambda: b"request"))
    assert raised.value is failure and failure.__cause__ is lookup
    assert sock.events == ["close"]
    assert connection._native_session.retired
    connection.close()
    # Conservative retention may retry a custom object when its shutdown
    # lookup failed even if its first close returned successfully.
    assert not connection._native_cleanup_sockets


def test_native_mac_provisional_failed_signal_cleanup_retains_retry_after_replacement(monkeypatch):
    _, connection, old_socket = make_connection(monkeypatch, "mac")
    old = connection._native_session
    candidate = Socket()
    failure, cleanup = KeyboardInterrupt(), ValueError("provisional close")

    def connect(address):
        raise failure

    def close():
        candidate.events.append("failed-close")
        raise cleanup

    candidate.connect, candidate.close = connect, close
    connection._socket_factory = lambda *args: candidate
    with pytest.raises(KeyboardInterrupt) as raised:
        connection.connect()
    assert raised.value is failure and failure.__cause__ is cleanup
    assert connection._sock is old_socket and connection._native_session is old and not old.retired
    assert connection._native_cleanup_sockets[0][0] is candidate

    replacement = Socket()
    connection._socket_factory = lambda *args: replacement
    connection.connect()
    assert old.retired and connection._sock is replacement
    candidate.close = lambda: candidate.events.append("retry-close")
    connection.close()
    assert replacement.events[-1] == "close" and candidate.events[-1] == "retry-close"
    assert not connection._native_cleanup_sockets


def test_native_mac_provisional_cleanup_record_survives_repeated_explicit_close_failure(monkeypatch):
    _, connection, _ = make_connection(monkeypatch, "mac")
    candidate = Socket()

    def connect(address):
        raise KeyboardInterrupt()

    def close():
        raise ValueError("retry still fails")

    candidate.connect, candidate.close = connect, close
    connection._socket_factory = lambda *args: candidate
    with pytest.raises(KeyboardInterrupt):
        connection.connect()
    for _ in range(2):
        with pytest.raises(ValueError, match="retry still fails"):
            connection.close()
        assert connection._sock is None
        assert connection._native_cleanup_sockets[0][0] is candidate
        assert connection._native_cleanup_sockets[0][1] is False
    candidate.close = lambda: None
    connection.close()
    assert not connection._native_cleanup_sockets


def test_native_mac_provisional_cleanup_retry_is_safe_when_callback_reenters_close(monkeypatch):
    _, connection, _ = make_connection(monkeypatch, "mac")
    candidate = Socket()

    def connect(address):
        raise KeyboardInterrupt()

    def close_failure():
        raise ValueError("first close")

    candidate.connect, candidate.close = connect, close_failure
    connection._socket_factory = lambda *args: candidate
    with pytest.raises(KeyboardInterrupt):
        connection.connect()
    retries = []

    def close_reentrant():
        retries.append("close")
        connection.close()

    candidate.close = close_reentrant
    connection.close()
    assert retries == ["close"] and not connection._native_cleanup_sockets


def test_native_extension_current_failed_cleanup_is_not_closed_twice_on_retry(monkeypatch):
    module, connection, sock = make_connection(monkeypatch, "mac")

    def send(data):
        raise KeyboardInterrupt()

    def failure():
        raise ValueError("first close")

    sock.sendall, sock.close = send, failure
    with pytest.raises(KeyboardInterrupt):
        connection.execute(SimpleNamespace(build_request=lambda: b"request"))
    calls = []
    sock.close = lambda: calls.append("close")
    connection.close()
    assert calls == ["close"] and not connection._native_cleanup_sockets


def test_native_extension_retained_cleanup_reference_is_released_after_explicit_close(monkeypatch):
    _, connection, _ = make_connection(monkeypatch, "mac")
    candidate = Socket()

    def connect(address):
        raise KeyboardInterrupt()

    def fail():
        raise ValueError("candidate close")

    candidate.connect, candidate.close = connect, fail
    connection._socket_factory = lambda *args: candidate
    with pytest.raises(KeyboardInterrupt):
        connection.connect()
    reference = weakref.ref(candidate)
    candidate.close = lambda: None
    connection._socket_factory = lambda *args: None
    connection.close()
    del candidate
    gc.collect()
    assert reference() is None and not connection._native_cleanup_sockets
