"""Parsers for statistics files exposed through TDX server resources."""

from __future__ import annotations

from collections import Counter
from collections.abc import Iterable
from io import BytesIO
from math import isfinite
from typing import TypeVar
from zipfile import BadZipFile, ZipFile

from eltdx.exceptions import ResourceFormatError
from eltdx.models import TdxStat2Row, TdxStatRow, TdxStatsResource

MAX_STATS_ZIP_BYTES = 32 * 1024 * 1024
MAX_STATS_ENTRY_BYTES = 32 * 1024 * 1024
MAX_STATS_UNCOMPRESSED_BYTES = 64 * 1024 * 1024
MAX_STATS_ARCHIVE_ENTRIES = 128
REQUIRED_STATS_MEMBERS = ("tdxstat.cfg", "tdxstat2.cfg")

RowT = TypeVar("RowT", TdxStatRow, TdxStat2Row)


def parse_tdx_stats_archive(payload: bytes, *, source_path: str = "zhb.zip") -> TdxStatsResource:
    """Parse ``tdxstat.cfg`` and ``tdxstat2.cfg`` from a TDX ``zhb.zip`` payload."""

    stat_payload, stat2_payload = _read_stats_members(payload)
    if (type(stat_payload) is bytes and type(stat2_payload) is bytes
            and _decode_lines is _ORIGINAL_DECODE_LINES
            and _parse_stat_rows is _ORIGINAL_PARSE_STAT_ROWS
            and _parse_stat2_rows is _ORIGINAL_PARSE_STAT2_ROWS
            and _float_value is _ORIGINAL_FLOAT_VALUE and _int_value is _ORIGINAL_INT_VALUE
            and _text_value is _ORIGINAL_TEXT_VALUE):
        from eltdx.transport.native import native_module

        rows, rows2 = native_module().stats_members(stat_payload, stat2_payload)
        stat_rows = [TdxStatRow(int(market), code, day, *floats,
                               *(None if value is None else int(value) for value in integers))
                     for market, code, day, floats, integers in rows]
        stat2_rows = [TdxStat2Row(int(market), code, day, *values)
                      for market, code, day, values in rows2]
    else:
        stat_rows = list(_parse_stat_rows(_decode_lines(stat_payload)))
        stat2_rows = list(_parse_stat2_rows(_decode_lines(stat2_payload)))
    if not stat_rows or not stat2_rows:
        raise ResourceFormatError(
            "TDX stats resource contains no usable rows in tdxstat.cfg or tdxstat2.cfg"
        )
    _require_unique_keys(stat_rows, member="tdxstat.cfg")
    _require_unique_keys(stat2_rows, member="tdxstat2.cfg")
    return TdxStatsResource(
        stat={row.key: row for row in stat_rows},
        stat2={row.key: row for row in stat2_rows},
        source_path=source_path,
    )


def _read_stats_members(payload: bytes) -> tuple[bytes, bytes]:
    if not isinstance(payload, bytes) or not payload:
        raise ResourceFormatError("TDX stats resource is empty")
    if len(payload) > MAX_STATS_ZIP_BYTES:
        raise ResourceFormatError(f"TDX stats ZIP exceeds {MAX_STATS_ZIP_BYTES} bytes")

    try:
        with ZipFile(BytesIO(payload)) as archive:
            infos = archive.infolist()
            if len(infos) > MAX_STATS_ARCHIVE_ENTRIES:
                raise ResourceFormatError(
                    f"TDX stats ZIP contains too many entries: {len(infos)}"
                )

            names = [info.filename for info in infos]
            missing = [name for name in REQUIRED_STATS_MEMBERS if name not in names]
            if missing:
                raise ResourceFormatError(
                    "TDX stats ZIP is missing files: " + ", ".join(missing)
                )
            duplicated = [name for name in REQUIRED_STATS_MEMBERS if names.count(name) != 1]
            if duplicated:
                raise ResourceFormatError(
                    "TDX stats ZIP contains duplicate files: " + ", ".join(duplicated)
                )

            total_size = 0
            for info in infos:
                if info.flag_bits & 0x1:
                    raise ResourceFormatError(
                        f"TDX stats ZIP contains an encrypted entry: {info.filename}"
                    )
                if info.file_size > MAX_STATS_ENTRY_BYTES:
                    raise ResourceFormatError(
                        f"TDX stats ZIP entry is too large: {info.filename}"
                    )
                total_size += info.file_size
            if total_size > MAX_STATS_UNCOMPRESSED_BYTES:
                raise ResourceFormatError("TDX stats ZIP uncompressed content is too large")

            return archive.read(REQUIRED_STATS_MEMBERS[0]), archive.read(REQUIRED_STATS_MEMBERS[1])
    except ResourceFormatError:
        raise
    except (BadZipFile, OSError, RuntimeError, ValueError) as exc:
        raise ResourceFormatError(f"TDX stats resource is not a valid ZIP: {exc}") from exc


def _decode_lines(payload: bytes) -> list[str]:
    return payload.decode("gbk", errors="ignore").splitlines()


def _parse_stat_rows(lines: Iterable[str]) -> Iterable[TdxStatRow]:
    for line in lines:
        parts = line.rstrip("\n\r").split("|")
        if len(parts) < 35:
            continue
        market_id = _int_value(parts[0])
        code = parts[1].strip()
        if market_id is None or not code:
            continue
        yield TdxStatRow(
            market_id=market_id,
            code=code,
            stats_date=_text_value(parts[4]),
            beta_60d=_float_value(parts[2]),
            pe_ttm=_float_value(parts[3]),
            free_float_shares_10k=_float_value(parts[11]),
            year_limit_up_days=_int_value(parts[26]),
            limit_stat_days=_int_value(parts[31]),
            limit_up_count_in_stat_days=_int_value(parts[32]),
            limit_up_streak_days=_int_value(parts[33]),
        )


def _parse_stat2_rows(lines: Iterable[str]) -> Iterable[TdxStat2Row]:
    for line in lines:
        parts = line.rstrip("\n\r").split("|")
        if len(parts) < 21:
            continue
        market_id = _int_value(parts[0])
        code = parts[1].strip()
        if market_id is None or not code:
            continue
        yield TdxStat2Row(
            market_id=market_id,
            code=code,
            stats_date=_text_value(parts[2]),
            amount_10k=_float_value(parts[3]),
            seal_amount_10k=_float_value(parts[4]),
            prev_amount_10k=_float_value(parts[5]),
            prev_seal_amount_10k=_float_value(parts[6]),
            prev2_amount_10k=_float_value(parts[7]),
            prev2_seal_amount_10k=_float_value(parts[8]),
            open_volume_hand=_float_value(parts[9]),
            prev_open_volume_hand=_float_value(parts[10]),
            open_amount_10k=_float_value(parts[14]),
            prev_open_amount_10k=_float_value(parts[15]),
        )


def _require_unique_keys(rows: list[RowT], *, member: str) -> None:
    counts = Counter(row.key for row in rows)
    duplicates = [key for key, count in counts.items() if count > 1]
    if duplicates:
        sample = ", ".join(f"{market_id}:{code}" for market_id, code in duplicates[:5])
        raise ResourceFormatError(
            f"TDX stats resource {member} contains duplicate security keys: {sample}"
        )


def _text_value(value: str) -> str | None:
    text = value.strip()
    return text or None


def _float_value(value: str) -> float | None:
    text = value.strip()
    if not text:
        return None
    try:
        number = float(text)
    except ValueError:
        return None
    return number if isfinite(number) else None


def _int_value(value: str) -> int | None:
    number = _float_value(value)
    if number is None:
        return None
    return int(number)


__all__ = ["parse_tdx_stats_archive"]

_ORIGINAL_DECODE_LINES = _decode_lines
_ORIGINAL_PARSE_STAT_ROWS = _parse_stat_rows
_ORIGINAL_PARSE_STAT2_ROWS = _parse_stat2_rows
_ORIGINAL_FLOAT_VALUE = _float_value
_ORIGINAL_INT_VALUE = _int_value
_ORIGINAL_TEXT_VALUE = _text_value
