"""Extension connection contracts; authored only until the full source gate."""
from __future__ import annotations

import socket
import sys
from types import SimpleNamespace

import pytest

from eltdx.exceptions import ConnectionClosedError, ProtocolError, ResponseTimeoutError, TransportError
from eltdx.ex.transport import sync as ex
from eltdx.mac import transport as mac


@pytest.mark.parametrize("module,read", [(mac, mac._recv_exact), (ex, ex._recv_exact_sock)])
def test_native_extension_exact_read_keeps_fragment_boundaries_and_requested_remainder(module, read):
    sizes = []
    chunks = iter((b"ab", b"c", b"de"))
    sock = SimpleNamespace(recv=lambda size: sizes.append(size) or next(chunks))
    assert read(sock, 5) == b"abcde" and sizes == [5, 3, 2]


@pytest.mark.parametrize("module,read", [(mac, mac._recv_exact), (ex, ex._recv_exact_sock)])
def test_native_extension_exact_read_keeps_nonpositive_and_overfull_legacy_behavior(module, read):
    sock = SimpleNamespace(recv=lambda size: pytest.fail("zero or negative target cannot read"))
    assert read(sock, 0) == b"" and read(sock, -1) == b""
    assert read(SimpleNamespace(recv=lambda size: b"oversized"), 1) == b"oversized"


@pytest.mark.parametrize("read,error", [(mac._recv_exact, ConnectionClosedError), (ex._recv_exact_sock, TransportError)])
def test_native_extension_exact_read_preserves_empty_chunk_exception(read, error):
    with pytest.raises(error, match="连接被服务器关闭"):
        read(SimpleNamespace(recv=lambda size: b""), 2)


def test_native_extension_read_keeps_live_len_bytes_and_recv_capture(monkeypatch):
    events = []
    output = object()
    lengths = iter((0, 1, 3))
    sock = SimpleNamespace()

    def length(value):
        events.append("len")
        sock.recv = lambda size: pytest.fail("recv captured before second len")
        return next(lengths)

    def recv(size):
        events.append(("recv", size))
        return b"ab"

    def initial_length(value):
        events.append("len")
        monkeypatch.setattr(mac, "len", length)
        return 0

    sock.recv = recv
    monkeypatch.setattr(mac, "len", initial_length, raising=False)
    # after condition, recv is captured; then length mutates it.
    lengths = iter((1, 3))
    monkeypatch.setattr(mac, "bytes", lambda value: events.append(bytes(value)) or output, raising=False)
    assert mac._recv_exact(sock, 3) is output
    assert events == ["len", "len", ("recv", 2), "len", b"ab"]


def test_native_mac_connection_constructor_keeps_assignment_and_false_factory(monkeypatch):
    events = []
    default = object()
    monkeypatch.setattr(mac, "_default_socket_factory", default)

    class Factory:
        def __bool__(self):
            events.append("factory-bool")
            return False

    class Connection(mac.MacConnection):
        def __setattr__(self, key, value):
            events.append(key)
            super().__setattr__(key, value)

    setup = object()
    conn = Connection("host", 7000, 15, socket_factory=Factory(), setup_commands=setup)
    assert conn._socket_factory is default and conn._setup_commands is setup
    assert events == ["host", "port", "timeout", "factory-bool", "_socket_factory", "_setup_commands", "_sock"]


def test_native_ex_connection_constructor_keeps_lazy_first_host_and_setup_short_circuit(monkeypatch):
    events = []
    monkeypatch.setattr(ex, "_first_host", lambda: events.append("first") or "first")

    class Mode:
        def __bool__(self):
            pytest.fail("false setup must skip mode truthiness")

    conn = ex.ExTdxConnection(None, mac_ex_mode=Mode(), send_setup=0)
    assert conn.host == "first" and conn.send_setup == 0 and events == ["first"]
    conn = ex.ExTdxConnection("explicit", mac_ex_mode=True)
    assert conn.host == "explicit" and conn.send_setup is False and events == ["first"]


def test_native_mac_connect_assigns_socket_after_connect_and_preserves_setup_override():
    events = []
    conn = None

    class Socket:
        def settimeout(self, timeout):
            events.append(("timeout", timeout))

        def connect(self, address):
            assert conn._sock is None
            events.append(("connect", address))

        def close(self):
            events.append("close")

    sock = Socket()

    class Connection(mac.MacConnection):
        def _send_setup(self):
            assert self._sock is sock
            events.append("setup")

    conn = Connection("h", 7, 15, socket_factory=lambda *args: sock)
    conn.connect()
    assert conn.connected and events == [("timeout", 15), ("connect", ("h", 7)), "setup"]
    conn.close()
    conn.close()
    assert events[-1] == "close" and not conn.connected


def test_native_ex_connect_registers_before_blocking_and_consumes_whole_setup(monkeypatch):
    events = []
    conn = ex.ExTdxConnection("h", 7, 15)

    class Socket:
        def settimeout(self, timeout):
            events.append(("timeout", timeout))

        def connect(self, address):
            assert conn._sock is self
            events.append(("connect", address))

        def sendall(self, value):
            events.append(("send", value))

    sock = Socket()
    monkeypatch.setattr(ex.socket, "socket", lambda *args: sock)
    monkeypatch.setattr(ex, "_EX_SETUP_PKG", b"setup")
    monkeypatch.setattr(ex, "parse_header", lambda raw: events.append(("parse", raw)) or SimpleNamespace(zipsize=299))
    conn._recv_exact = lambda size: events.append(("recv", size)) or b"response"
    conn.connect()
    assert events == [("timeout", 15), ("connect", ("h", 7)), ("send", b"setup"), ("recv", 16), ("parse", b"response"), ("recv", 299)]


@pytest.mark.parametrize("domain", ["mac", "ex"])
def test_native_extension_connect_failure_closes_and_preserves_cause(domain, monkeypatch):
    events = []
    failure = OSError("offline")
    conn = None

    def connect(address):
        raise failure

    sock = SimpleNamespace(settimeout=lambda value: None, connect=connect, close=lambda: events.append(sys.exc_info()[1]))
    if domain == "mac":
        conn = mac.MacConnection("h", 7, 15, socket_factory=lambda *args: sock)
    else:
        conn = ex.ExTdxConnection("h", 7, 15)
        monkeypatch.setattr(ex.socket, "socket", lambda *args: sock)
    with pytest.raises(TransportError) as caught:
        conn.connect()
    assert caught.value.__cause__ is failure and events == [failure]
    assert conn._sock is None


def test_native_mac_setup_swallows_response_errors_but_send_error_propagates(monkeypatch):
    events = []
    conn = mac.MacConnection("h", 7, 15, setup_commands=(b"one", b"two"))
    conn._sock = SimpleNamespace(sendall=lambda value: events.append(value))
    conn._recv_exact = lambda size: (_ for _ in ()).throw(TransportError("read failed"))
    conn._send_setup()
    assert events == [b"one", b"two"]
    error = OSError("send failed")
    conn._sock = SimpleNamespace(sendall=lambda value: (_ for _ in ()).throw(error))
    with pytest.raises(OSError) as caught:
        conn._send_setup()
    assert caught.value is error


def test_native_mac_setup_reads_only_positive_payload_lengths(monkeypatch):
    events = []
    conn = mac.MacConnection("h", 7, 15, setup_commands=(b"one", b"two"))
    conn._sock = SimpleNamespace(sendall=lambda value: events.append(("send", value)))
    sizes = iter((0, 3))
    monkeypatch.setattr(mac, "parse_mac_header", lambda raw: SimpleNamespace(zipsize=next(sizes)))
    conn._recv_exact = lambda size: events.append(("recv", size)) or b"raw"
    conn._send_setup()
    assert events == [("send", b"one"), ("recv", 16), ("send", b"two"), ("recv", 16), ("recv", 3)]


def test_native_mac_execute_keeps_timeout_projection_and_parser_outside_io_handler(monkeypatch):
    conn = mac.MacConnection("h", 7, 15)
    failure = socket.timeout("slow")
    conn._sock = SimpleNamespace(sendall=lambda request: (_ for _ in ()).throw(failure))
    command = SimpleNamespace(build_request=lambda: b"request")
    with pytest.raises(ResponseTimeoutError, match="请求超时: slow") as caught:
        conn.execute(command)
    assert caught.value.__cause__ is failure
    conn._sock = SimpleNamespace(sendall=lambda request: None)
    conn._recv_exact = lambda size: b"raw"
    monkeypatch.setattr(mac, "parse_mac_header", lambda raw: SimpleNamespace(zipsize=3))
    monkeypatch.setattr(mac, "decompress_mac_body", lambda header, raw: b"body")
    parse_error = OSError("parser")
    command.parse_response = lambda body: (_ for _ in ()).throw(parse_error)
    with pytest.raises(OSError) as caught:
        conn.execute(command)
    assert caught.value is parse_error


def test_native_ex_execute_holds_original_lock_across_build_wire_decode_and_parse(monkeypatch):
    events = []

    class Lock:
        def __enter__(self):
            events.append("enter")

        def __exit__(self, *args):
            events.append("exit")

    conn = ex.ExTdxConnection("h", mac_ex_mode=True)
    conn._lock = Lock()
    conn._sock = SimpleNamespace(sendall=lambda value: events.append(("send", value)))
    conn._recv_exact = lambda size: events.append(("recv", size)) or b"raw"
    monkeypatch.setattr(ex, "parse_header", lambda raw: SimpleNamespace(zipsize=3))
    monkeypatch.setattr(ex, "decompress_body", lambda header, raw: events.append("decode") or b"body")
    command = SimpleNamespace(build_request=lambda: events.append("build") or b"\x1cpacket",
                              parse_response=lambda body: events.append(("parse", body)) or "result")
    assert conn.execute(command) == "result"
    assert events == ["enter", "build", ("send", b"\x01packet"), ("recv", 16), ("recv", 3), "decode", ("parse", b"body"), "exit"]


def test_native_ex_io_error_preserves_outer_handler_and_does_not_wrap_protocol_errors(monkeypatch):
    conn = ex.ExTdxConnection("h", send_setup=False)
    failure = OSError("send")
    conn._sock = SimpleNamespace(sendall=lambda request: (_ for _ in ()).throw(failure))
    command = SimpleNamespace(build_request=lambda: b"request")
    with pytest.raises(TransportError, match="通信错误: send") as caught:
        conn.execute(command)
    assert caught.value.__cause__ is failure
    conn._sock = SimpleNamespace(sendall=lambda request: None)
    conn._recv_exact = lambda size: b"raw"
    error = ProtocolError("bad header")
    monkeypatch.setattr(ex, "parse_header", lambda raw: (_ for _ in ()).throw(error))
    with pytest.raises(ProtocolError) as caught:
        conn.execute(command)
    assert caught.value is error


@pytest.mark.parametrize("domain", ["mac", "ex"])
def test_native_extension_settimeout_failure_stays_outside_connection_handler(domain, monkeypatch):
    failure = OSError("timeout setter")
    sock = SimpleNamespace(settimeout=lambda value: (_ for _ in ()).throw(failure), close=lambda: pytest.fail("setter failure never entered old close handler"))
    if domain == "mac":
        conn = mac.MacConnection("h", 7, 15, socket_factory=lambda *args: sock)
    else:
        conn = ex.ExTdxConnection("h", 7, 15)
        monkeypatch.setattr(ex.socket, "socket", lambda *args: sock)
    with pytest.raises(OSError) as caught:
        conn.connect()
    assert caught.value is failure and conn._sock is None


def test_native_ex_mac_head_rewrite_keeps_empty_standard_and_special_heads(monkeypatch):
    conn = ex.ExTdxConnection("h", mac_ex_mode=True)
    sent = []
    conn._sock = SimpleNamespace(sendall=sent.append)
    conn._recv_exact = lambda size: b"raw"
    monkeypatch.setattr(ex, "parse_header", lambda raw: SimpleNamespace(zipsize=0))
    monkeypatch.setattr(ex, "decompress_body", lambda header, raw: b"")
    for value in [b"", b"\x0cpayload", b"\x02payload", b"\x1cpayload"]:
        conn.execute(SimpleNamespace(build_request=lambda value=value: value, parse_response=lambda body: None))
    assert sent == [b"", b"\x0cpayload", b"\x02payload", b"\x01payload"]


def test_native_mac_connection_receive_captures_module_helper_before_socket_property(monkeypatch):
    events = []

    class Connection(mac.MacConnection):
        reads = 0

        @property
        def _sock(self):
            self.reads += 1
            if self.reads == 2:
                monkeypatch.setattr(mac, "_recv_exact", lambda *args: pytest.fail("helper captured before socket"))
            return "sock"

        @_sock.setter
        def _sock(self, value):
            pass

    conn = Connection("h", 7, 15)
    monkeypatch.setattr(mac, "_recv_exact", lambda sock, count: events.append((sock, count)) or "result")
    assert conn._recv_exact(3) == "result" and events == [("sock", 3)]


@pytest.mark.parametrize("domain", ["mac", "ex"])
def test_native_extension_close_keeps_non_oserror_visible_and_connection_reference(domain):
    conn = mac.MacConnection("h", 7, 15) if domain == "mac" else ex.ExTdxConnection("h", 7, 15)
    failure = ValueError("close hook")
    sock = SimpleNamespace(close=lambda: (_ for _ in ()).throw(failure))
    conn._sock = sock
    with pytest.raises(ValueError) as caught:
        conn.close()
    assert caught.value is failure and conn._sock is sock


def test_native_mac_setup_failure_closes_connection_but_baseexception_keeps_original_boundary():
    failure = RuntimeError("setup")
    closed = []
    sock = SimpleNamespace(settimeout=lambda value: None, connect=lambda address: None, close=lambda: closed.append(True))

    class Connection(mac.MacConnection):
        def _send_setup(self):
            raise failure

    conn = Connection("h", 7, 15, socket_factory=lambda *args: sock)
    with pytest.raises(RuntimeError) as caught:
        conn.connect()
    assert caught.value is failure and closed == [True] and conn._sock is None


def test_native_extension_unconnected_error_precedes_command_build():
    command = SimpleNamespace(build_request=lambda: pytest.fail("unconnected command cannot build"))
    with pytest.raises(TransportError, match="未连接"):
        mac.MacConnection("h", 7, 15).execute(command)
    with pytest.raises(TransportError, match="未连接"):
        ex.ExTdxConnection("h", 7, 15).execute(command)
