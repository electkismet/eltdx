"""F10-local address ordering, without changing process-wide socket functions."""

from __future__ import annotations

import socket
from http.client import HTTPConnection, HTTPSConnection
from urllib.request import HTTPHandler, HTTPSHandler, build_opener


def _connect_ipv4_first(address, timeout=socket._GLOBAL_DEFAULT_TIMEOUT, source_address=None):
    host, port = address
    records = socket.getaddrinfo(host, port, 0, socket.SOCK_STREAM)
    # Stable ordering preserves the resolver's order within each address family.
    records = sorted(records, key=lambda row: row[0] != socket.AF_INET)
    last_error = None
    for family, socktype, proto, _, sockaddr in records:
        sock = None
        try:
            sock = socket.socket(family, socktype, proto)
            if timeout is not socket._GLOBAL_DEFAULT_TIMEOUT:
                sock.settimeout(timeout)
            if source_address:
                sock.bind(source_address)
            sock.connect(sockaddr)
            return sock
        except OSError as exc:
            last_error = exc
            if sock is not None:
                sock.close()
    if last_error is not None:
        raise last_error
    raise OSError("getaddrinfo returns an empty list")


class _IPv4HTTPConnection(HTTPConnection):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self._create_connection = _connect_ipv4_first


class _IPv4HTTPSConnection(HTTPSConnection):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        # HTTPSConnection retains Host, proxy CONNECT, SNI and TLS verification.
        self._create_connection = _connect_ipv4_first


class _IPv4HTTPHandler(HTTPHandler):
    def http_open(self, req):
        return self.do_open(_IPv4HTTPConnection, req)


class _IPv4HTTPSHandler(HTTPSHandler):
    def https_open(self, req):
        return self.do_open(_IPv4HTTPSConnection, req, context=self._context)


def open_ipv4_first(request, *, timeout):
    # Keep urllib's environment proxies, redirects and HTTP error handling.
    # A request-local opener avoids shared mutable state between F10 callers.
    opener = build_opener(_IPv4HTTPHandler(), _IPv4HTTPSHandler())
    return opener.open(request, timeout=timeout)
