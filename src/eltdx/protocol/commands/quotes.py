"""Quote snapshot command builders and parsers."""

from __future__ import annotations

import struct

from eltdx.exceptions import ProtocolError
from eltdx.models import (
    CategoryQuotePage,
    CategoryQuoteRecord,
    LegacyQuote,
    QuoteLevel,
    QuoteRefreshPage,
    QuoteRefreshRecord,
    QuoteSnapshot,
)
from eltdx.protocol.constants import TYPE_CATEGORY_QUOTES, TYPE_LEGACY_QUOTES, TYPE_REFRESH_STREAM, TYPE_SNAPSHOTS
from eltdx.protocol.frame import RequestFrame, ResponseFrame
from eltdx.protocol.unit import (
    consume_price,
    consume_varint,
    decode_k,
    get_volume,
    little_f32,
    little_u16,
    little_u32,
    milli_to_float,
    normalize_code,
    price_divisor,
    split_code,
)

CATEGORY_ALIASES = {
    "沪深a股": 6,
    "a股": 6,
    "A股": 6,
    "沪深A股": 6,
}

SORT_ALIASES = {
    None: 0,
    "代码": 0x0000,
    "现价": 0x0006,
    "成交额": 0x000A,
    "涨幅": 0x000E,
    "封单额": 0x001C,
    "开盘金额": 0x001D,
    "涨速": 0x002E,
    "短换手": 0x00CC,
    "量涨速": 0x00D0,
    "开盘抢筹": 0x010A,
    "2分钟金额": 0x010C,
    "开盘涨幅": 0x0119,
    "最高涨幅": 0x011A,
    "最低涨幅": 0x011B,
    "回撤": 0x011E,
    "攻击": 0x011F,
}

MAX_REFRESH_CODES = 100


def build_snapshots_frame(payload: dict, msg_id: int) -> RequestFrame:
    codes = _normalize_codes(payload.get("codes", []))
    if len(codes) > 0xFFFF:
        raise ValueError("too many codes")

    data = bytearray(bytes.fromhex("0500000000000000"))
    data.extend(len(codes).to_bytes(2, "little", signed=False))
    for code in codes:
        market_id, _, number = split_code(code)
        data.append(market_id)
        data.extend(number.encode("ascii"))
    return RequestFrame(msg_id=msg_id, msg_type=TYPE_SNAPSHOTS, data=bytes(data))


def build_legacy_quotes_frame(payload: dict, msg_id: int) -> RequestFrame:
    codes = _normalize_codes(payload.get("codes", []))
    if not codes:
        raise ValueError("codes must not be empty")
    if len(codes) > 0xFFFF:
        raise ValueError("too many codes")

    data = bytearray(bytes.fromhex("0500000000000000"))
    data.extend(len(codes).to_bytes(2, "little", signed=False))
    for code in codes:
        market_id, _, number = split_code(code)
        data.append(market_id)
        data.extend(number.encode("ascii"))
    return RequestFrame(msg_id=msg_id, msg_type=TYPE_LEGACY_QUOTES, data=bytes(data))


def build_refresh_stream_frame(payload: dict, msg_id: int) -> RequestFrame:
    codes = _normalize_codes(payload.get("codes", []))
    if len(codes) > MAX_REFRESH_CODES:
        raise ValueError(f"refresh_stream accepts at most {MAX_REFRESH_CODES} codes per request")
    cursors = payload.get("cursors", {}) or {}
    data = bytearray(len(codes).to_bytes(2, "little"))
    for code in codes:
        market_id, _, number = split_code(code)
        cursor = int(cursors.get(code, cursors.get(number, 0)))
        data.append(market_id)
        data.extend(number.encode("ascii"))
        data.extend(cursor.to_bytes(4, "little", signed=False))
    return RequestFrame(msg_id=msg_id, msg_type=TYPE_REFRESH_STREAM, data=bytes(data))


def parse_snapshots_payload(response: ResponseFrame, request_payload: dict | None = None) -> list[QuoteSnapshot]:
    payload = response.data
    if len(payload) < 4:
        raise ProtocolError("invalid snapshots payload")
    count = little_u16(payload[2:4])
    request_codes = _normalize_codes((request_payload or {}).get("codes", []))
    expected_codes = request_codes[:count] if request_codes else []

    records = _split_records(payload[4:], expected_codes, count)
    return [_parse_snapshot_record(record, expected_codes[index] if index < len(expected_codes) else None) for index, record in enumerate(records)]


def parse_legacy_quotes_payload(response: ResponseFrame, request_payload: dict | None = None) -> list[LegacyQuote]:
    payload = response.data
    if len(payload) < 4:
        raise ProtocolError("invalid legacy quotes payload")

    count = little_u16(payload[2:4])
    offset = 4
    requested_codes = _normalize_codes((request_payload or {}).get("codes", []))
    records: list[LegacyQuote] = []
    for index in range(count):
        next_marker = _legacy_marker(requested_codes[index + 1]) if index + 1 < len(requested_codes) else None
        record, offset = _parse_legacy_quote_record(
            payload,
            offset,
            remaining_records=count - index - 1,
            next_marker=next_marker,
        )
        records.append(record)
    if offset != len(payload):
        raise ProtocolError(f"unexpected trailing legacy quotes payload bytes: {len(payload) - offset}")
    return records


def build_category_quotes_frame(payload: dict, msg_id: int) -> RequestFrame:
    category = normalize_category(payload.get("category", 6))
    sort_type = normalize_sort_type(payload.get("sort_type", payload.get("sort_by", 0)))
    start = _u16(payload.get("start", 0), "start")
    count = _u16(payload.get("count", 80), "count")
    if "sort_reverse" in payload:
        sort_reverse = _u16(payload["sort_reverse"], "sort_reverse")
    else:
        ascending = bool(payload.get("ascending", False))
        sort_reverse = 0 if sort_type == 0 else (2 if ascending else 1)
    filter_raw = _u16(payload.get("filter_raw", 0), "filter_raw")
    data = struct.pack("<9H", category, sort_type, start, count, sort_reverse, 5, filter_raw, 1, 0)
    return RequestFrame(msg_id=msg_id, msg_type=TYPE_CATEGORY_QUOTES, data=data)


def parse_category_quotes_payload(response: ResponseFrame, request_payload: dict | None = None) -> CategoryQuotePage:
    request_payload = request_payload or {}
    payload = response.data
    if len(payload) < 4:
        raise ProtocolError("invalid category quotes payload")

    header = little_u16(payload[:2])
    count = little_u16(payload[2:4])
    offset = 4
    records: list[CategoryQuoteRecord] = []
    for _ in range(count):
        record, offset = _parse_category_quote_record(payload, offset)
        records.append(record)
    if offset != len(payload):
        raise ProtocolError(f"unexpected trailing category quotes payload bytes: {len(payload) - offset}")
    return CategoryQuotePage(
        category=normalize_category(request_payload.get("category", 6)),
        sort_type=normalize_sort_type(request_payload.get("sort_type", request_payload.get("sort_by", 0))),
        start=int(request_payload.get("start", 0)),
        request_count=int(request_payload.get("count", 80)),
        sort_reverse=int(request_payload.get("sort_reverse", 0 if normalize_sort_type(request_payload.get("sort_type", request_payload.get("sort_by", 0))) == 0 else (2 if request_payload.get("ascending", False) else 1))),
        filter_raw=int(request_payload.get("filter_raw", 0)),
        header=header,
        records=tuple(records),
        raw_payload=payload,
    )


def parse_refresh_stream_payload(response: ResponseFrame, request_payload: dict | None = None) -> QuoteRefreshPage:
    request_payload = request_payload or {}
    raw_payload = response.data
    decoded = bytes(byte ^ 0x93 for byte in raw_payload)
    if len(decoded) < 2:
        raise ProtocolError("invalid refresh stream payload")

    refresh_count = little_u16(decoded[:2])
    requested_codes = tuple(_normalize_codes(request_payload.get("codes", [])))
    if refresh_count == 0:
        if len(decoded) != 2:
            raise ProtocolError(f"unexpected trailing empty refresh bytes: {len(decoded) - 2}")
        return QuoteRefreshPage(requested_codes=requested_codes, records=(), decoded_payload=decoded, raw_payload=raw_payload)

    records_raw = _split_refresh_records(decoded[2:], requested_codes, refresh_count)
    records = tuple(_parse_refresh_record(record) for record in records_raw)
    return QuoteRefreshPage(
        requested_codes=requested_codes,
        records=records,
        decoded_payload=decoded,
        raw_payload=raw_payload,
    )


def _normalize_codes(codes) -> list[str]:
    if isinstance(codes, str):
        codes = [codes]
    return [normalize_code(code) for code in list(codes)]


def _split_records(data: bytes, expected_codes: list[str], count: int) -> list[bytes]:
    if count == 0:
        return []
    if not expected_codes:
        raise ProtocolError("snapshot parser requires request codes to split variable records")
    if len(expected_codes) < count:
        raise ProtocolError("snapshot response count exceeds request code count")

    starts: list[int] = []
    search_from = 0
    for code in expected_codes[:count]:
        market_id, _, number = split_code(code)
        marker = bytes([market_id]) + number.encode("ascii")
        position = data.find(marker, search_from)
        if position < 0:
            raise ProtocolError(f"snapshot record marker not found: {code}")
        starts.append(position)
        search_from = position + 7

    records: list[bytes] = []
    for index, start in enumerate(starts):
        end = starts[index + 1] if index + 1 < len(starts) else len(data)
        records.append(data[start:end])
    return records


def _parse_snapshot_record(record: bytes, expected_code: str | None) -> QuoteSnapshot:
    if len(record) < 9:
        raise ProtocolError("truncated snapshot record")
    market_id = record[0]
    exchange = {0: "sz", 1: "sh", 2: "bj"}.get(market_id, "unknown")
    code = record[1:7].decode("ascii")
    active1 = little_u16(record[7:9])
    offset = 9

    prices, offset = decode_k(record, offset)
    time_raw, offset = consume_varint(record, offset)
    unknown_after_time_raw, offset = consume_varint(record, offset)
    total_hand, offset = consume_varint(record, offset)
    current_hand, offset = consume_varint(record, offset)
    if offset + 4 > len(record):
        raise ProtocolError("truncated snapshot amount")
    amount_raw = little_u32(record[offset : offset + 4])
    amount = get_volume(amount_raw)
    offset += 4
    inside_dish, offset = consume_varint(record, offset)
    outer_disc, offset = consume_varint(record, offset)
    unknown_after_outer_raw, offset = consume_varint(record, offset)
    open_amount_raw, offset = consume_varint(record, offset)

    full_code = expected_code or f"{exchange}{code}"
    divisor = price_divisor(full_code)
    current_milli = prices["current"]
    buy_levels: list[QuoteLevel] = []
    sell_levels: list[QuoteLevel] = []
    # Live 0x054c responses only expose a stable bid1/ask1 group at this
    # position. Bytes after the first level are an extension area on current
    # servers; parsing them as levels 2-5 produces invalid prices/volumes.
    for _ in range(1):
        bid_delta, offset = consume_price(record, offset)
        ask_delta, offset = consume_price(record, offset)
        bid_vol, offset = consume_varint(record, offset)
        ask_vol, offset = consume_varint(record, offset)
        bid_milli = (current_milli + bid_delta * 10) // divisor
        ask_milli = (current_milli + ask_delta * 10) // divisor
        buy_levels.append(QuoteLevel(price=milli_to_float(bid_milli), volume=bid_vol, price_delta_raw=bid_delta))
        sell_levels.append(QuoteLevel(price=milli_to_float(ask_milli), volume=ask_vol, price_delta_raw=ask_delta))

    tail_raw = record[offset:]
    return QuoteSnapshot(
        exchange=exchange,
        market_id=market_id,
        code=code,
        active1=active1,
        last_price=milli_to_float(prices["current"] // divisor),
        pre_close_price=milli_to_float(prices["last_close"] // divisor),
        open_price=milli_to_float(prices["open"] // divisor),
        high_price=milli_to_float(prices["high"] // divisor),
        low_price=milli_to_float(prices["low"] // divisor),
        time_raw=time_raw,
        unknown_after_time_raw=unknown_after_time_raw,
        total_hand=total_hand,
        current_hand=current_hand,
        amount=amount,
        amount_raw=amount_raw,
        inside_dish=inside_dish,
        outer_disc=outer_disc,
        unknown_after_outer_raw=unknown_after_outer_raw,
        open_amount_raw=open_amount_raw,
        open_amount_yuan=float(open_amount_raw * 100),
        buy_levels=tuple(buy_levels),
        sell_levels=tuple(sell_levels),
        tail_raw=tail_raw,
    )


def _parse_legacy_quote_record(
    payload: bytes,
    offset: int,
    *,
    remaining_records: int,
    next_marker: bytes | None,
) -> tuple[LegacyQuote, int]:
    start = offset
    if len(payload) < offset + 9:
        raise ProtocolError("truncated legacy quote record header")

    market_id = payload[offset]
    exchange = {0: "sz", 1: "sh", 2: "bj"}.get(market_id)
    if exchange is None:
        raise ProtocolError(f"invalid legacy quote market id: {market_id!r}")
    try:
        code = payload[offset + 1 : offset + 7].decode("ascii")
    except UnicodeDecodeError as exc:
        raise ProtocolError("invalid legacy quote code") from exc
    if len(code) != 6 or not code.isdigit():
        raise ProtocolError(f"invalid legacy quote code: {code!r}")
    active1 = little_u16(payload[offset + 7 : offset + 9])
    offset += 9
    price_scale = 100.0 * price_divisor(f"{exchange}{code}")

    close_raw, offset = consume_varint(payload, offset)
    pre_close_diff_raw, offset = consume_varint(payload, offset)
    open_diff_raw, offset = consume_varint(payload, offset)
    high_diff_raw, offset = consume_varint(payload, offset)
    low_diff_raw, offset = consume_varint(payload, offset)
    server_time_raw, offset = consume_varint(payload, offset)
    unknown_after_time_raw, offset = consume_varint(payload, offset)
    total_hand, offset = consume_varint(payload, offset)
    current_hand, offset = consume_varint(payload, offset)

    if len(payload) < offset + 4:
        raise ProtocolError("truncated legacy quote amount")
    amount_raw = little_u32(payload[offset : offset + 4])
    amount = get_volume(amount_raw)
    offset += 4

    inside_dish, offset = consume_varint(payload, offset)
    outer_disc, offset = consume_varint(payload, offset)
    unknown_after_outer_raw, offset = consume_varint(payload, offset)
    open_amount_raw, offset = consume_varint(payload, offset)

    buy_levels: list[QuoteLevel] = []
    sell_levels: list[QuoteLevel] = []
    for _ in range(5):
        bid_delta_raw, offset = consume_varint(payload, offset)
        ask_delta_raw, offset = consume_varint(payload, offset)
        bid_volume, offset = consume_varint(payload, offset)
        ask_volume, offset = consume_varint(payload, offset)
        buy_levels.append(
            QuoteLevel(price=(close_raw + bid_delta_raw) / price_scale, volume=bid_volume, price_delta_raw=bid_delta_raw)
        )
        sell_levels.append(
            QuoteLevel(price=(close_raw + ask_delta_raw) / price_scale, volume=ask_volume, price_delta_raw=ask_delta_raw)
        )

    if len(payload) < offset + 2:
        raise ProtocolError("truncated legacy quote trading status")
    trading_status_raw = little_u16(payload[offset : offset + 2])
    offset += 2

    tail_metrics: list[int] = []
    for _ in range(4):
        value, offset = consume_varint(payload, offset)
        tail_metrics.append(value)

    rise_speed_raw = None
    active2 = None
    if remaining_records:
        has_short_tail = next_marker is not None and payload.startswith(next_marker, offset)
        has_long_tail = next_marker is not None and payload.startswith(next_marker, offset + 4)
        if has_long_tail or (not has_short_tail and not _is_legacy_record_marker(payload, offset)):
            if len(payload) < offset + 4:
                raise ProtocolError("truncated legacy quote tail")
            rise_speed_raw = int.from_bytes(payload[offset : offset + 2], "little", signed=True)
            active2 = little_u16(payload[offset + 2 : offset + 4])
            offset += 4
    elif offset < len(payload):
        if len(payload) != offset + 4:
            raise ProtocolError("truncated legacy quote tail")
        rise_speed_raw = int.from_bytes(payload[offset : offset + 2], "little", signed=True)
        active2 = little_u16(payload[offset + 2 : offset + 4])
        offset += 4

    return (
        LegacyQuote(
            exchange=exchange,
            market_id=market_id,
            code=code,
            active1=active1,
            last_price=close_raw / price_scale,
            pre_close_price=(close_raw + pre_close_diff_raw) / price_scale,
            open_price=(close_raw + open_diff_raw) / price_scale,
            high_price=(close_raw + high_diff_raw) / price_scale,
            low_price=(close_raw + low_diff_raw) / price_scale,
            server_time_raw=server_time_raw,
            unknown_after_time_raw=unknown_after_time_raw,
            total_hand=total_hand,
            current_hand=current_hand,
            amount=amount,
            amount_raw=amount_raw,
            inside_dish=inside_dish,
            outer_disc=outer_disc,
            unknown_after_outer_raw=unknown_after_outer_raw,
            open_amount_raw=open_amount_raw,
            open_amount_yuan=float(open_amount_raw) * 100.0,
            buy_levels=tuple(buy_levels),
            sell_levels=tuple(sell_levels),
            trading_status_raw=trading_status_raw,
            tail_metrics_raw=tuple(tail_metrics),
            rise_speed_raw=rise_speed_raw,
            active2=active2,
            record_hex=payload[start:offset].hex(),
        ),
        offset,
    )


def _is_legacy_record_marker(payload: bytes, offset: int) -> bool:
    if len(payload) < offset + 9 or payload[offset] not in {0, 1, 2}:
        return False
    return payload[offset + 1 : offset + 7].isdigit()


def _legacy_marker(code: str) -> bytes:
    market_id, _, number = split_code(code)
    return bytes([market_id]) + number.encode("ascii")


def _parse_category_quote_record(payload: bytes, offset: int) -> tuple[CategoryQuoteRecord, int]:
    record_start = offset
    if offset + 9 > len(payload):
        raise ProtocolError("truncated category quote record header")
    market_id = payload[offset]
    exchange = {0: "sz", 1: "sh", 2: "bj"}.get(market_id, "unknown")
    code = payload[offset + 1 : offset + 7].decode("ascii")
    active1 = little_u16(payload[offset + 7 : offset + 9])
    offset += 9

    close_raw, offset = consume_price(payload, offset)
    pre_close_diff, offset = consume_price(payload, offset)
    open_diff, offset = consume_price(payload, offset)
    high_diff, offset = consume_price(payload, offset)
    low_diff, offset = consume_price(payload, offset)
    server_time_raw, offset = consume_varint(payload, offset)
    neg_price_raw, offset = consume_varint(payload, offset)
    total_hand, offset = consume_varint(payload, offset)
    current_hand, offset = consume_varint(payload, offset)
    if offset + 4 > len(payload):
        raise ProtocolError("truncated category quote amount")
    amount_raw = little_u32(payload[offset : offset + 4])
    amount = get_volume(amount_raw)
    offset += 4
    inside_dish, offset = consume_varint(payload, offset)
    outer_disc, offset = consume_varint(payload, offset)
    after_outer_raw, offset = consume_varint(payload, offset)
    open_amount_raw, offset = consume_varint(payload, offset)
    bid1_diff, offset = consume_price(payload, offset)
    ask1_diff, offset = consume_price(payload, offset)
    bid_vol1, offset = consume_varint(payload, offset)
    ask_vol1, offset = consume_varint(payload, offset)

    if offset + 56 > len(payload):
        raise ProtocolError("truncated category quote tail")
    tail = payload[offset : offset + 56]
    offset += 56
    (
        status_or_sort_raw,
        rise_speed_raw,
        short_turnover_raw,
        min2_amount,
        opening_rush_raw,
        extra_pair_raw,
        vol_rise_speed,
        depth,
        extra_meta_raw,
        active2,
    ) = struct.unpack("<Hhhfh10sff24sH", tail)

    full_code = f"{exchange}{code}"
    return (
        CategoryQuoteRecord(
            exchange=exchange,
            market_id=market_id,
            code=code,
            active1=active1,
            active2=active2,
            last_price=_quote_price(close_raw, full_code),
            pre_close_price=_quote_price(close_raw + pre_close_diff, full_code),
            open_price=_quote_price(close_raw + open_diff, full_code),
            high_price=_quote_price(close_raw + high_diff, full_code),
            low_price=_quote_price(close_raw + low_diff, full_code),
            server_time_raw=server_time_raw,
            neg_price_raw=neg_price_raw,
            total_hand=total_hand,
            current_hand=current_hand,
            amount=amount,
            amount_raw=amount_raw,
            inside_dish=inside_dish,
            outer_disc=outer_disc,
            after_outer_raw=after_outer_raw,
            open_amount_raw=open_amount_raw,
            open_amount=float(open_amount_raw * 100),
            bid1=_quote_price(close_raw + bid1_diff, full_code),
            ask1=_quote_price(close_raw + ask1_diff, full_code),
            bid_vol1=bid_vol1,
            ask_vol1=ask_vol1,
            status_or_sort_raw=status_or_sort_raw,
            rise_speed_raw=rise_speed_raw,
            rise_speed=rise_speed_raw / 100.0,
            short_turnover_raw=short_turnover_raw,
            short_turnover=short_turnover_raw / 100.0,
            min2_amount=min2_amount,
            opening_rush_raw=opening_rush_raw,
            opening_rush=opening_rush_raw / 100.0,
            extra_pair_raw=extra_pair_raw,
            vol_rise_speed=vol_rise_speed,
            depth=depth,
            extra_meta_raw=extra_meta_raw,
            tail_raw=tail,
            record_hex=payload[record_start:offset].hex(),
        ),
        offset,
    )


def _split_refresh_records(data: bytes, requested_codes: tuple[str, ...], count: int) -> list[bytes]:
    if count <= 0:
        return []
    if count == 1:
        return [data]

    markers = []
    for code in requested_codes:
        market_id, _, number = split_code(code)
        markers.append(bytes([market_id]) + number.encode("ascii"))
    if not markers:
        raise ProtocolError("refresh parser needs request codes to split multiple records")

    starts = [0]
    search_from = 7
    while len(starts) < count:
        found_positions = [position for marker in markers if (position := data.find(marker, search_from)) >= 0]
        if not found_positions:
            raise ProtocolError("refresh record marker not found")
        next_start = min(found_positions)
        starts.append(next_start)
        search_from = next_start + 7

    records: list[bytes] = []
    for index, start in enumerate(starts):
        end = starts[index + 1] if index + 1 < len(starts) else len(data)
        records.append(data[start:end])
    return records


def _parse_refresh_record(record: bytes) -> QuoteRefreshRecord:
    if len(record) < 9:
        raise ProtocolError("truncated refresh record")
    market_id = record[0]
    exchange = {0: "sz", 1: "sh", 2: "bj"}.get(market_id, "unknown")
    code = record[1:7].decode("ascii")
    active = little_u16(record[7:9])
    offset = 9

    prices, offset = decode_k(record, offset)
    full_code = f"{exchange}{code}"
    current_milli = prices["current"]
    if offset + 4 > len(record):
        raise ProtocolError("truncated refresh update time")
    update_time_raw = little_u32(record[offset : offset + 4])
    offset += 4
    status_or_reserved_raw, offset = consume_varint(record, offset)
    total_hand, offset = consume_varint(record, offset)
    current_hand, offset = consume_varint(record, offset)
    if offset + 4 > len(record):
        raise ProtocolError("truncated refresh amount")
    amount_raw = little_u32(record[offset : offset + 4])
    amount = get_volume(amount_raw)
    offset += 4
    inside_dish, offset = consume_varint(record, offset)
    outer_disc, offset = consume_varint(record, offset)
    unknown_after_outer_raw, offset = consume_varint(record, offset)
    open_amount_raw, offset = consume_varint(record, offset)

    divisor = price_divisor(full_code)
    buy_levels: list[QuoteLevel] = []
    sell_levels: list[QuoteLevel] = []
    for _ in range(5):
        buy_delta_raw, offset = consume_price(record, offset)
        sell_delta_raw, offset = consume_price(record, offset)
        buy_volume, offset = consume_varint(record, offset)
        sell_volume, offset = consume_varint(record, offset)
        buy_price = milli_to_float((current_milli + buy_delta_raw * 10) // divisor)
        sell_price = milli_to_float((current_milli + sell_delta_raw * 10) // divisor)
        buy_levels.append(QuoteLevel(price=buy_price, volume=buy_volume, price_delta_raw=buy_delta_raw))
        sell_levels.append(QuoteLevel(price=sell_price, volume=sell_volume, price_delta_raw=sell_delta_raw))

    return QuoteRefreshRecord(
        exchange=exchange,
        market_id=market_id,
        code=code,
        active=active,
        update_time_raw=update_time_raw,
        last_price=milli_to_float(prices["current"] // divisor),
        last_close_price=milli_to_float(prices["last_close"] // divisor),
        open_price=milli_to_float(prices["open"] // divisor),
        high_price=milli_to_float(prices["high"] // divisor),
        low_price=milli_to_float(prices["low"] // divisor),
        status_or_reserved_raw=status_or_reserved_raw,
        total_hand=total_hand,
        current_hand=current_hand,
        amount=amount,
        amount_raw=amount_raw,
        inside_dish=inside_dish,
        outer_disc=outer_disc,
        unknown_after_outer_raw=unknown_after_outer_raw,
        open_amount_raw=open_amount_raw,
        open_amount_yuan=float(open_amount_raw * 10),
        buy_levels=tuple(buy_levels),
        sell_levels=tuple(sell_levels),
        tail_raw=record[offset:],
        record_hex=record.hex(),
    )


def normalize_category(value) -> int:
    if isinstance(value, int):
        return value
    text = str(value).strip()
    if text in CATEGORY_ALIASES:
        return CATEGORY_ALIASES[text]
    return int(text, 0)


def normalize_sort_type(value) -> int:
    if isinstance(value, int):
        return value
    if value in SORT_ALIASES:
        return SORT_ALIASES[value]
    text = str(value).strip()
    if text in SORT_ALIASES:
        return SORT_ALIASES[text]
    return int(text, 0)


def _quote_price(raw_value: int, full_code: str) -> float:
    return milli_to_float((raw_value * 10) // price_divisor(full_code))


def _u16(value, name: str) -> int:
    parsed = int(value)
    if parsed < 0 or parsed > 0xFFFF:
        raise ValueError(f"{name} must be between 0 and 65535")
    return parsed
