from __future__ import annotations

from collections.abc import Iterator
from contextlib import contextmanager
from io import BytesIO
from zipfile import ZipFile

import pytest

from eltdx.api.resources import ResourceApi
from eltdx.exceptions import ProtocolError, ResourceFormatError
from eltdx.models import FileContentChunk
from eltdx.protocol.constants import TYPE_FILE_CONTENT
from eltdx.stats import parse_tdx_stats_archive


class ResourceTransport:
    def __init__(self, payload: bytes) -> None:
        self.payload = payload
        self.calls: list[dict[str, object]] = []

    def execute(self, command: int, payload: dict[str, object] | None = None):
        assert command == TYPE_FILE_CONTENT
        request = dict(payload or {})
        self.calls.append(request)
        path = str(request["path"])
        offset = int(request["offset"])
        size = int(request["size"])
        content = self.payload[offset : offset + size]
        return FileContentChunk(
            path=path,
            offset=offset,
            request_size=size,
            chunk_len=len(content),
            content=content,
        )


class PinningResourceTransport:
    def __init__(self, payload: bytes) -> None:
        self.pinned = ResourceTransport(payload)
        self.pin_entries = 0
        self.pin_exits = 0

    def execute(
        self,
        command: int,
        payload: dict[str, object] | None = None,
    ) -> FileContentChunk:
        del command, payload
        raise AssertionError("download must execute through the pinned transport")

    @contextmanager
    def pin(self) -> Iterator[ResourceTransport]:
        self.pin_entries += 1
        try:
            yield self.pinned
        finally:
            self.pin_exits += 1


def test_download_file_joins_chunks_and_probes_exact_size_boundary() -> None:
    transport = ResourceTransport(b"abcdefgh")
    resources = ResourceApi(transport)

    assert resources.download_file("sample.bin", chunk_size=4) == b"abcdefgh"
    assert [call["offset"] for call in transport.calls] == [0, 4, 8]
    assert [call["size"] for call in transport.calls] == [4, 4, 4]


def test_download_file_honors_max_bytes() -> None:
    transport = ResourceTransport(b"abcdefgh")
    resources = ResourceApi(transport)

    assert resources.download_file("sample.bin", chunk_size=4, max_bytes=6) == b"abcdef"
    assert [call["size"] for call in transport.calls] == [4, 2]


def test_download_file_uses_one_public_transport_pin() -> None:
    transport = PinningResourceTransport(b"abcdefgh")
    assert ResourceApi(transport).download_file("sample.bin", chunk_size=4) == b"abcdefgh"
    assert transport.pin_entries == 1
    assert transport.pin_exits == 1
    assert [call["offset"] for call in transport.pinned.calls] == [0, 4, 8]


@pytest.mark.parametrize("value", [0, 60001, True, 1.5])
def test_download_file_rejects_invalid_chunk_size(value: object) -> None:
    with pytest.raises(ProtocolError, match="chunk_size"):
        ResourceApi(ResourceTransport(b"abc")).download_file("sample.bin", chunk_size=value)  # type: ignore[arg-type]


def test_read_stats_downloads_and_parses_zhb_zip() -> None:
    payload = _stats_zip(
        [_stat_line(code="000001"), _stat_line(code="600000", market_id=1)],
        [_stat2_line(code="000001"), _stat2_line(code="600000", market_id=1)],
    )
    resources = ResourceApi(ResourceTransport(payload))

    result = resources.read_stats(chunk_size=37)
    stat, stat2 = result.row(0, "1")

    assert result.source_path == "tdx://zhb.zip"
    assert result.stat_count == 2
    assert result.stat2_count == 2
    assert result.stats_date == "20260710"
    assert result.stats_date_coverage == 1.0
    assert stat is not None
    assert stat.beta_60d == pytest.approx(1.25)
    assert stat.pe_ttm == pytest.approx(8.75)
    assert stat.free_float_shares_10k == pytest.approx(200.0)
    assert stat.year_limit_up_days == 9
    assert stat.limit_stat_days == 7
    assert stat.limit_up_count_in_stat_days == 5
    assert stat.limit_up_streak_days == 3
    assert stat2 is not None
    assert stat2.amount_10k == pytest.approx(242501.84)
    assert stat2.seal_amount_10k is None
    assert stat2.prev_amount_10k == pytest.approx(254664.58)
    assert stat2.prev_seal_amount_10k == pytest.approx(851.80)
    assert stat2.prev2_amount_10k == pytest.approx(134819.47)
    assert stat2.prev2_seal_amount_10k == pytest.approx(3468.47)
    assert stat2.open_volume_hand == pytest.approx(11141)
    assert stat2.prev_open_volume_hand == pytest.approx(9513)
    assert stat2.open_amount_10k == pytest.approx(6002.06)
    assert stat2.prev_open_amount_10k == pytest.approx(4291.39)


def test_parse_stats_archive_rejects_missing_member() -> None:
    buffer = BytesIO()
    with ZipFile(buffer, "w") as archive:
        archive.writestr("tdxstat.cfg", _stat_line().encode("gbk"))

    with pytest.raises(ResourceFormatError, match="tdxstat2.cfg"):
        parse_tdx_stats_archive(buffer.getvalue())


def test_parse_stats_archive_rejects_duplicate_security_keys() -> None:
    payload = _stats_zip(
        [_stat_line(), _stat_line()],
        [_stat2_line()],
    )

    with pytest.raises(ResourceFormatError, match="duplicate security keys"):
        parse_tdx_stats_archive(payload)


def test_parse_stats_archive_rejects_invalid_zip() -> None:
    with pytest.raises(ResourceFormatError, match="valid ZIP"):
        parse_tdx_stats_archive(b"not a zip")


def test_parse_stats_archive_maps_non_finite_numbers_to_none() -> None:
    stat = _stat_line().split("|")
    stat[2] = "nan"
    stat[26] = "inf"
    stat2 = _stat2_line().split("|")
    stat2[3] = "-inf"

    result = parse_tdx_stats_archive(_stats_zip(["|".join(stat)], ["|".join(stat2)]))
    row, row2 = result.row(0, "000001")

    assert row is not None and row.beta_60d is None and row.year_limit_up_days is None
    assert row2 is not None and row2.amount_10k is None


def _stats_zip(stat_lines: list[str], stat2_lines: list[str]) -> bytes:
    buffer = BytesIO()
    with ZipFile(buffer, "w") as archive:
        archive.writestr("tdxstat.cfg", "\n".join(stat_lines).encode("gbk"))
        archive.writestr("tdxstat2.cfg", "\n".join(stat2_lines).encode("gbk"))
    return buffer.getvalue()


def _stat_line(*, code: str = "000001", market_id: int = 0) -> str:
    parts = [""] * 35
    parts[0] = str(market_id)
    parts[1] = code
    parts[2] = "1.25"
    parts[3] = "8.75"
    parts[4] = "20260710"
    parts[11] = "200.00"
    parts[26] = "9"
    parts[31] = "7"
    parts[32] = "5"
    parts[33] = "3"
    return "|".join(parts)


def _stat2_line(*, code: str = "000001", market_id: int = 0) -> str:
    parts = [""] * 21
    parts[0] = str(market_id)
    parts[1] = code
    parts[2] = "20260710"
    parts[3] = "242501.84"
    parts[5] = "254664.58"
    parts[6] = "851.80"
    parts[7] = "134819.47"
    parts[8] = "3468.47"
    parts[9] = "11141"
    parts[10] = "9513"
    parts[14] = "6002.06"
    parts[15] = "4291.39"
    return "|".join(parts)
