"""Authored resource-domain contracts; execution waits for implementation completion."""
from __future__ import annotations

from contextlib import contextmanager
from types import SimpleNamespace

import pytest

from eltdx.api.resources import ResourceApi
from eltdx.exceptions import ProtocolError, TransportError
from eltdx.models import FileContentChunk
from eltdx.transport import PooledSocketTransport, SocketTransport
from eltdx.transport.native import native_module


class Reader:
    def __init__(self, data, hosts=(None,)):
        self.data, self.hosts = data, hosts
        self.calls = []
        self._transport = SimpleNamespace(connected_host=None)

    def read(self, path, *, offset, size):
        self._transport.connected_host = self.hosts[min(len(self.calls), len(self.hosts) - 1)]
        self.calls.append((path, offset, size))
        content = self.data[offset:offset + size]
        return FileContentChunk(path, offset, size, len(content), content)


def download(reader, maximum=None):
    return native_module().download_resource(reader, "sample.bin", 4, maximum,
                                             FileContentChunk, ProtocolError, TransportError)


def test_native_resource_exact_multiple_probes_eof_and_cap_stops_without_probe():
    reader = Reader(b"abcdefgh")
    assert download(reader) == b"abcdefgh"
    assert reader.calls == [("sample.bin", 0, 4), ("sample.bin", 4, 4), ("sample.bin", 8, 4)]
    reader = Reader(b"abcdefgh")
    assert download(reader, 6) == b"abcdef"
    assert reader.calls == [("sample.bin", 0, 4), ("sample.bin", 4, 2)]
    reader = Reader(b"abcdefgh")
    assert download(reader, 0) == b"" and reader.calls == []


def test_native_resource_host_none_adopts_first_concrete_and_rejects_later_change():
    reader = Reader(b"abcdefghijklmnop", (None, "h1", None, "h2"))
    with pytest.raises(TransportError, match="^TDX server changed while downloading a multi-chunk file$"):
        download(reader)
    assert len(reader.calls) == 4


@pytest.mark.parametrize("response,message", [
    (object(), "file content request did not return FileContentChunk"),
    (FileContentChunk("x", 0, 4, 2, b"x"), "file content chunk length does not match the returned content"),
    (FileContentChunk("x", 0, 4, 5, b"12345"), "file content chunk exceeds requested size: 5 > 4"),
])
def test_native_resource_preserves_chunk_errors(response, message):
    reader = SimpleNamespace(read=lambda *_args, **_kwargs: response, _transport=SimpleNamespace())
    with pytest.raises(ProtocolError) as actual:
        download(reader)
    assert str(actual.value) == message


def test_native_resource_preserves_original_query_exception():
    error = TransportError("original request failure")

    def read(*_args, **_kwargs):
        raise error

    with pytest.raises(TransportError) as actual:
        download(SimpleNamespace(read=read))
    assert actual.value is error


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
@pytest.mark.parametrize("command,call,payload", [
    (0x06B9, lambda api: api.read("sample.bin", offset=8, size=4), dict(path="sample.bin", offset=8, size=4)),
    (0x02C5, lambda api: api.block_meta("block.dat"), dict(filename="block.dat")),
])
def test_native_resource_single_requests_use_shared_domain(transport_type, command, call, payload, monkeypatch):
    transport = transport_type(["127.0.0.1:7709"], heartbeat_interval=None)
    calls, response = [], object()
    monkeypatch.setattr(transport, "sdk_resource", lambda *args: calls.append(args) or response)
    assert call(ResourceApi(transport, native_services=True)) is response
    assert calls == [(command, payload)]


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
def test_native_resource_execute_override_remains_an_adapter(transport_type, monkeypatch):
    transport = transport_type(["127.0.0.1:7709"], heartbeat_interval=None)
    calls = []

    def execute(command, payload):
        calls.append(payload)
        return FileContentChunk(payload["path"], payload["offset"], payload["size"], 0, b"")

    monkeypatch.setattr(transport, "execute", execute)
    monkeypatch.setattr(transport, "sdk_resource", lambda *_: pytest.fail("override bypassed"))
    # Pooled pin acquisition is separately observable; use one read here.
    assert ResourceApi(transport, native_services=True).read("sample.bin").content == b""
    assert len(calls) == 1


def test_native_resource_zero_cap_still_observes_public_pin():
    events = []
    transport = PooledSocketTransport(["127.0.0.1:7709"], heartbeat_interval=None)

    @contextmanager
    def pin():
        events.append("enter")
        try:
            yield transport
        finally:
            events.append("exit")

    transport.pin = pin
    assert ResourceApi(transport, native_services=True).download_file("sample.bin", max_bytes=0) == b""
    assert events == ["enter", "exit"] and transport._engine is None
