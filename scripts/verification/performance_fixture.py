"""Independent, deterministic DAT and HQ wire fixtures for paired measurement."""
from __future__ import annotations
import socket
import socketserver
import struct
import threading


def dat() -> bytes:
    rows, fields = 256, 64
    header = struct.pack("<hIHIII", 0, 20240331, rows, 0, fields * 4, 0)
    index = b"".join(struct.pack("<6sBI", ("%06d" % (600_000 + row)).encode(), 1,
                                  20 + rows * 11 + row * fields * 4) for row in range(rows))
    return header + index + struct.pack("<f", 1.0) * (rows * fields)


def exact(stream, size):
    result = bytearray()
    while len(result) < size:
        block = stream.recv(size - len(result))
        if not block:
            raise EOFError("Incomplete benchmark request")
        result.extend(block)
    return bytes(result)


class Handler(socketserver.BaseRequestHandler):
    def handle(self):
        stream = self.request
        stream.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        stream.settimeout(10)
        try:
            while True:
                first = stream.recv(1)
                if not first:
                    return
                header = first + exact(stream, 11)
                length, repeat, command = struct.unpack_from("<HHH", header, 6)
                if header[0] != 0x0c or length != repeat or length < 2:
                    raise ValueError("Invalid HQ request frame")
                payload = exact(stream, length - 2)
                with self.server.lock:
                    if not hasattr(self, "connection_id"):
                        self.server.connections += 1
                        self.connection_id = self.server.connections
                    self.server.observed.append(dict(connection=self.connection_id, endpoint=list(self.client_address),
                        message_id=struct.unpack_from("<I", header, 1)[0], command=command, payload_hex=payload.hex()))
                if command == 0x000d:
                    reply = bytes(189)
                elif command == 0x0004:
                    reply = bytes(10)
                elif command == 0x044d:
                    if len(payload) != 14:
                        raise ValueError("Unexpected directory request size")
                    market, offset, count, reserved = struct.unpack("<HIII", payload)
                    if market != 1 or offset >= 1000 or count != 1 or reserved != 0:
                        raise ValueError("Unexpected benchmark directory query")
                    row = bytearray(37)
                    row[:6] = ("%06d" % (600_000 + offset)).encode()
                    struct.pack_into("<H", row, 6, 100)
                    row[8:12] = b"TEST"
                    struct.pack_into("<f", row, 24, 1.0)
                    row[28] = 2
                    struct.pack_into("<f", row, 29, 10.0)
                    reply = b"\x01\x00" + row
                else:
                    raise ValueError("Unexpected benchmark command: %04x" % command)
                size = struct.pack("<H", len(reply))
                stream.sendall(b"\xb1\xcb\x74\x00\x01" + header[1:5] + b"\x00" + struct.pack("<H", command) + size + size + reply)
        except BaseException as error:
            with self.server.lock:
                self.server.errors.append(repr(error))


class Peer(socketserver.ThreadingTCPServer):
    allow_reuse_address = False
    daemon_threads = False
    block_on_close = True

    def __init__(self):
        super().__init__(("127.0.0.1", 0), Handler)
        self.lock = threading.Lock()
        self.connections = 0
        self.observed, self.errors = [], []
        self.thread = threading.Thread(target=self.serve_forever, kwargs={"poll_interval": .02})

    def __enter__(self):
        self.thread.start()
        return self

    @property
    def endpoint(self):
        return "%s:%s" % self.server_address

    def __exit__(self, kind, value, traceback):
        self.shutdown()
        self.thread.join(10)
        self.server_close()
        if self.thread.is_alive():
            raise RuntimeError("Benchmark peer failed to stop")
        if self.errors and kind is None:
            raise RuntimeError("Benchmark peer failed: %r" % self.errors)
