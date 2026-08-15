"""TDX server file API."""

from __future__ import annotations

import operator
from collections.abc import Iterator
from contextlib import contextmanager
from typing import SupportsIndex, cast

from eltdx.exceptions import ProtocolError, TransportError
from eltdx.models import FileContentChunk, TdxStatsResource
from eltdx.protocol.constants import DEFAULT_FILE_CHUNK_SIZE, MAX_FILE_CHUNK_SIZE
from eltdx.stats import MAX_STATS_ZIP_BYTES, parse_tdx_stats_archive
from eltdx.transport import Transport

from .base import ApiBase


class ResourceApi(ApiBase):
    def read(self, path: str, *, offset: int = 0, size: int = DEFAULT_FILE_CHUNK_SIZE):
        """Read one byte range from a file exposed by the TDX server."""

        return self._execute("file_content", path=path, offset=offset, size=size)

    def download_file(
        self,
        path: str,
        *,
        chunk_size: int = DEFAULT_FILE_CHUNK_SIZE,
        max_bytes: int | None = None,
    ) -> bytes:
        """Download a complete server file with repeated ``0x06B9`` requests."""

        chunk_size = _bounded_int(chunk_size, "chunk_size", minimum=1, maximum=MAX_FILE_CHUNK_SIZE)
        if max_bytes is not None:
            max_bytes = _bounded_int(max_bytes, "max_bytes", minimum=0, maximum=0xFFFFFFFF)

        with _pinned_transport(self._transport) as transport:
            reader = ResourceApi(transport)
            expected_host: str | None = None
            offset = 0
            chunks: list[bytes] = []
            while max_bytes is None or offset < max_bytes:
                size = chunk_size if max_bytes is None else min(chunk_size, max_bytes - offset)
                chunk = reader.read(path, offset=offset, size=size)
                if not isinstance(chunk, FileContentChunk):
                    raise ProtocolError("file content request did not return FileContentChunk")
                if chunk.chunk_len != len(chunk.content):
                    raise ProtocolError(
                        "file content chunk length does not match the returned content"
                    )
                if chunk.chunk_len > size:
                    raise ProtocolError(
                        f"file content chunk exceeds requested size: {chunk.chunk_len} > {size}"
                    )

                current_host = getattr(transport, "connected_host", None)
                if expected_host is None:
                    expected_host = current_host
                elif current_host is not None and current_host != expected_host:
                    raise TransportError(
                        "TDX server changed while downloading a multi-chunk file"
                    )

                chunks.append(chunk.content)
                offset += chunk.chunk_len
                if chunk.chunk_len == 0 or chunk.chunk_len < size:
                    break
            return b"".join(chunks)

    def read_stats(
        self,
        path: str = "zhb.zip",
        *,
        chunk_size: int = DEFAULT_FILE_CHUNK_SIZE,
    ) -> TdxStatsResource:
        """Download and parse ``tdxstat.cfg`` and ``tdxstat2.cfg`` from ``zhb.zip``."""

        payload = self.download_file(
            path,
            chunk_size=chunk_size,
            max_bytes=MAX_STATS_ZIP_BYTES + 1,
        )
        return parse_tdx_stats_archive(payload, source_path=f"tdx://{path}")


def _bounded_int(value: object, name: str, *, minimum: int, maximum: int) -> int:
    if isinstance(value, bool):
        raise ProtocolError(f"file content {name} must be an integer")
    try:
        number = operator.index(cast(SupportsIndex, value))
    except (TypeError, ValueError) as exc:
        raise ProtocolError(f"file content {name} must be an integer") from exc
    if not minimum <= number <= maximum:
        raise ProtocolError(
            f"file content {name} must be between {minimum} and {maximum}"
        )
    return number


@contextmanager
def _pinned_transport(transport: Transport) -> Iterator[Transport]:
    pin = getattr(transport, "pin", None)
    if not callable(pin):
        yield transport
        return
    with pin() as pinned:
        yield pinned
