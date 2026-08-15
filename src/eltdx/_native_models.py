"""Reconstruct public Python models from the private native DTO ABI.

The Rust extension deliberately returns only tuples, lists, scalar values,
bytes, and ``None``.  This module is the single boundary where those values
become the public dataclasses again.
"""

from __future__ import annotations

from datetime import date, datetime, timedelta, timezone
from typing import TYPE_CHECKING, Any

from eltdx.models import (
    AuctionPoint,
    AuctionSeries,
    CapitalChangeBlock,
    CapitalChangeRecord,
    CategoryQuotePage,
    CategoryQuoteRecord,
    FileContentChunk,
    FinanceBatch,
    FinanceRecord,
    HandshakeInfo,
    HeartbeatAck,
    KlineBar,
    KlineSeries,
    LegacyQuote,
    MinuteAuxPoint,
    MinuteAuxSeries,
    MinutePoint,
    MinuteSeries,
    QuoteLevel,
    QuoteRefreshPage,
    QuoteRefreshRecord,
    QuoteSnapshot,
    SecurityCode,
    SparklineSeries,
    SpecialLimitPage,
    SpecialLimitRecord,
    TradePage,
    TradeTick,
)

if TYPE_CHECKING:
    from eltdx.protocol.frame import ResponseFrame


def _tuple(value: Any, name: str, size: int | None = None) -> tuple[Any, ...]:
    if not isinstance(value, tuple):
        raise TypeError(f"native DTO {name} must be a tuple")
    if size is not None and len(value) != size:
        raise ValueError(f"native DTO {name} must contain {size} fields")
    return value


def _list(value: Any, name: str) -> list[Any]:
    if not isinstance(value, list):
        raise TypeError(f"native DTO {name} must be a list")
    return value


def _date(value: Any) -> date | None:
    if value is None:
        return None
    year, month, day = _tuple(value, "date", 3)
    return date(int(year), int(month), int(day))


def _datetime(value: Any) -> datetime | None:
    if value is None:
        return None
    year, month, day, hour, minute, second, offset = _tuple(value, "datetime", 7)
    if offset is None:
        return datetime(int(year), int(month), int(day), int(hour), int(minute), int(second))
    tz = timezone(timedelta(seconds=int(offset)))
    return datetime(int(year), int(month), int(day), int(hour), int(minute), int(second), tzinfo=tz)


def _records(value: Any, name: str, convert: Any) -> tuple[Any, ...]:
    return tuple(convert(item) for item in _tuple(value, name))


def _quote_level(value: Any) -> QuoteLevel:
    return QuoteLevel(*_tuple(value, "quote level", 3))


def _auction_point(value: Any) -> AuctionPoint:
    return AuctionPoint(*_tuple(value, "auction point", 13))


def _capital_record(value: Any) -> CapitalChangeRecord:
    fields = list(_tuple(value, "capital change record", 21))
    fields[5] = _date(fields[5])
    return CapitalChangeRecord(*fields)


def _finance_record(value: Any) -> FinanceRecord:
    fields = list(_tuple(value, "finance record", 42))
    fields[8] = _date(fields[8])
    fields[10] = _date(fields[10])
    return FinanceRecord(*fields)


def _minute_point(value: Any) -> MinutePoint:
    fields = list(_tuple(value, "minute point", 14))
    fields[2] = _datetime(fields[2])
    return MinutePoint(*fields)


def _minute_aux_point(value: Any) -> MinuteAuxPoint:
    return MinuteAuxPoint(*_tuple(value, "minute auxiliary point", 10))


def _kline_bar(value: Any) -> KlineBar:
    fields = list(_tuple(value, "kline bar", 22))
    fields[0] = _datetime(fields[0])
    if fields[0] is None:
        raise ValueError("kline bar time cannot be None")
    return KlineBar(*fields)


def _legacy_quote(value: Any) -> LegacyQuote:
    fields = list(_tuple(value, "legacy quote", 27))
    fields[20] = _records(fields[20], "legacy quote buy levels", _quote_level)
    fields[21] = _records(fields[21], "legacy quote sell levels", _quote_level)
    return LegacyQuote(*fields)


def _category_quote(value: Any) -> CategoryQuoteRecord:
    return CategoryQuoteRecord(*_tuple(value, "category quote record", 39))


def _refresh_quote(value: Any) -> QuoteRefreshRecord:
    fields = list(_tuple(value, "quote refresh record", 24))
    fields[20] = _records(fields[20], "quote refresh buy levels", _quote_level)
    fields[21] = _records(fields[21], "quote refresh sell levels", _quote_level)
    return QuoteRefreshRecord(*fields)


def _trade_tick(value: Any) -> TradeTick:
    fields = list(_tuple(value, "trade tick", 19))
    fields[4] = _datetime(fields[4])
    return TradeTick(*fields)


def _security(value: Any) -> SecurityCode:
    return SecurityCode(*_tuple(value, "security code", 15))


def _response_frame(value: Any) -> ResponseFrame:
    from eltdx.protocol.frame import ResponseFrame

    fields = _tuple(value, "response frame", 8)
    return ResponseFrame(*fields)


def _minute_series(value: Any) -> MinuteSeries:
    fields = list(_tuple(value, "minute series", 10))
    fields[3] = _date(fields[3])
    fields[4] = _records(fields[4], "minute points", _minute_point)
    return MinuteSeries(*fields)


def _trade_page(value: Any) -> TradePage:
    fields = list(_tuple(value, "trade page", 9))
    fields[5] = _records(fields[5], "trade ticks", _trade_tick)
    fields[6] = _date(fields[6])
    return TradePage(*fields)


def response_from_dto(dto: Any) -> Any:
    """Convert one ``(tag, payload)`` native response DTO."""

    tag, payload = _tuple(dto, "response", 2)
    if not isinstance(tag, str):
        raise TypeError("native response tag must be a string")

    if tag == "heartbeat":
        fields = list(_tuple(payload, tag, 4))
        fields[2] = _date(fields[2])
        return HeartbeatAck(*fields)
    if tag == "handshake":
        fields = list(_tuple(payload, tag, 12))
        fields[0] = _datetime(fields[0])
        fields[3] = _date(fields[3])
        fields[4] = _date(fields[4])
        return HandshakeInfo(*fields)
    if tag == "capital_changes":
        exchange, market_id, code, block_count, records, raw_payload = _tuple(payload, tag, 6)
        return CapitalChangeBlock(
            exchange,
            market_id,
            code,
            block_count,
            _records(records, tag, _capital_record),
            raw_payload,
        )
    if tag == "finance_batch":
        records, raw_payload = _tuple(payload, tag, 2)
        return FinanceBatch(_records(records, tag, _finance_record), raw_payload)
    if tag == "security_list":
        return [_security(item) for item in _list(payload, tag)]
    if tag == "security_count":
        if not isinstance(payload, int):
            raise TypeError("native security count must be an integer")
        return payload
    if tag == "special_limits":
        start_index, records, raw_payload = _tuple(payload, tag, 3)
        converted = _records(records, tag, lambda item: SpecialLimitRecord(*_tuple(item, tag, 7)))
        return SpecialLimitPage(start_index, converted, raw_payload)
    if tag == "intraday_aux":
        exchange, market_id, code, selector_raw, kind, points, raw_payload = _tuple(payload, tag, 7)
        return MinuteAuxSeries(
            exchange,
            market_id,
            code,
            selector_raw,
            kind,
            _records(points, tag, _minute_aux_point),
            raw_payload,
        )
    if tag == "klines":
        fields = list(_tuple(payload, tag, 14))
        fields[11] = _date(fields[11])
        fields[12] = _records(fields[12], tag, _kline_bar)
        return KlineSeries(*fields)
    if tag in {"today_intraday", "historical_intraday", "recent_intraday"}:
        return _minute_series(payload)
    if tag == "legacy_quotes":
        return [_legacy_quote(item) for item in _list(payload, tag)]
    if tag == "refresh_stream":
        requested_codes, records, decoded_payload, raw_payload = _tuple(payload, tag, 4)
        return QuoteRefreshPage(
            tuple(_tuple(requested_codes, tag)),
            _records(records, tag, _refresh_quote),
            decoded_payload,
            raw_payload,
        )
    if tag == "category_quotes":
        fields = list(_tuple(payload, tag, 9))
        fields[7] = _records(fields[7], tag, _category_quote)
        return CategoryQuotePage(*fields)
    if tag == "snapshots":
        snapshots = []
        for item in _list(payload, tag):
            fields = list(_tuple(item, tag, 23))
            fields[20] = _records(fields[20], tag, _quote_level)
            fields[21] = _records(fields[21], tag, _quote_level)
            snapshots.append(QuoteSnapshot(*fields))
        return snapshots
    if tag == "auction_series":
        exchange, market_id, code, mode, start, limit, points, raw_payload = _tuple(payload, tag, 8)
        return AuctionSeries(
            exchange,
            market_id,
            code,
            mode,
            start,
            limit,
            _records(points, tag, _auction_point),
            raw_payload,
        )
    if tag == "file_content":
        return FileContentChunk(*_tuple(payload, tag, 6))
    if tag in {"today_ticks", "historical_ticks"}:
        return _trade_page(payload)
    if tag == "sparkline":
        fields = list(_tuple(payload, tag, 11))
        fields[8] = tuple(_tuple(fields[8], tag))
        return SparklineSeries(*fields)
    raise ValueError(f"unsupported native response tag: {tag}")


def push_frame_from_dto(dto: Any) -> tuple[int, int, int, str, ResponseFrame, bool]:
    """Convert the metadata-bearing native push DTO without parsing it."""

    tag, payload = _tuple(dto, "push", 2)
    if tag != "push":
        raise ValueError("native push DTO has an invalid tag")
    epoch, slot_id, generation, connected_host, frame, parse = _tuple(payload, tag, 6)
    return epoch, slot_id, generation, connected_host, _response_frame(frame), parse


__all__ = ["push_frame_from_dto", "response_from_dto"]
