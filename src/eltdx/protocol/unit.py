"""Small binary helpers for 7709 command parsers."""

from __future__ import annotations

import math
import struct
from datetime import date, datetime, timedelta, timezone
from typing import Any

from eltdx.exceptions import ProtocolError

MARKET_TO_ID = {"sz": 0, "sh": 1, "bj": 2}
ID_TO_MARKET = {value: key for key, value in MARKET_TO_ID.items()}
PRICING_ETF_PREFIXES = ("15", "16", "50", "51", "52", "53", "56", "58")
SHANGHAI_TZ = timezone(timedelta(hours=8), name="Asia/Shanghai")


def little_u16(data: bytes) -> int:
    return int.from_bytes(data, "little", signed=False)


def little_u32(data: bytes) -> int:
    return int.from_bytes(data, "little", signed=False)


def little_f32(data: bytes) -> float:
    return struct.unpack("<f", data)[0]


def decode_gbk_text(data: bytes) -> str:
    return data.decode("gbk", errors="ignore").replace("\x00", "").strip()


def normalize_market(value: str | int) -> str:
    if isinstance(value, int):
        try:
            return ID_TO_MARKET[value]
        except KeyError as exc:
            raise ProtocolError(f"invalid market id: {value!r}") from exc

    text = str(value).strip().lower()
    aliases = {
        "0": "sz",
        "1": "sh",
        "2": "bj",
        "sza": "sz",
        "sha": "sh",
        "bse": "bj",
        "深市": "sz",
        "沪市": "sh",
        "北交所": "bj",
    }
    text = aliases.get(text, text)
    if text not in MARKET_TO_ID:
        raise ProtocolError(f"invalid market: {value!r}")
    return text


def market_to_id(value: str | int) -> int:
    return MARKET_TO_ID[normalize_market(value)]


def normalize_code(code: str) -> str:
    text = str(code).strip().lower()
    if len(text) == 8 and text[:2] in MARKET_TO_ID and text[2:].isdigit():
        return text
    if len(text) != 6 or not text.isdigit():
        raise ProtocolError(f"invalid code: {code!r}")
    if text.startswith("92"):
        return "bj" + text
    if text.startswith(("6", "9")):
        return "sh" + text
    if text.startswith(("0", "1", "2", "3")):
        return "sz" + text
    if text.startswith("8"):
        return "bj" + text
    raise ProtocolError(f"unable to infer market for code: {code!r}")


def split_code(code: str) -> tuple[int, str, str]:
    full_code = normalize_code(code)
    market = full_code[:2]
    number = full_code[2:]
    return market_to_id(market), market, number


def yyyymmdd(value: str | int | date | datetime | None = None) -> int:
    if value is None:
        return int(date.today().strftime("%Y%m%d"))
    if isinstance(value, datetime):
        return int(value.date().strftime("%Y%m%d"))
    if isinstance(value, date):
        return int(value.strftime("%Y%m%d"))
    if isinstance(value, int):
        return value

    text = str(value).strip().replace("-", "")
    if len(text) != 8 or not text.isdigit():
        raise ProtocolError(f"invalid date: {value!r}")
    return int(text)


def date_from_yyyymmdd(raw: int) -> date | None:
    text = f"{raw:08d}"
    try:
        return datetime.strptime(text, "%Y%m%d").date()
    except ValueError:
        return None


def clean_payload_dict(payload: dict[str, Any] | None) -> dict[str, Any]:
    return dict(payload or {})


def consume_varint(payload: bytes, offset: int) -> tuple[int, int]:
    if offset >= len(payload):
        raise ProtocolError("unexpected end of payload")

    value = 0
    position = offset
    shift = 0
    while True:
        if position >= len(payload):
            raise ProtocolError("unterminated varint")
        byte = payload[position]
        if position == offset:
            value += byte & 0x3F
            shift = 6
        else:
            value += (byte & 0x7F) << shift
            shift += 7
        position += 1
        if byte & 0x80 == 0:
            break
    if payload[offset] & 0x40:
        value = -value
    return value, position


def consume_price(payload: bytes, offset: int) -> tuple[int, int]:
    return consume_varint(payload, offset)


def decode_k(payload: bytes, offset: int) -> tuple[dict[str, int], int]:
    current_delta, offset = consume_price(payload, offset)
    last_close_delta, offset = consume_price(payload, offset)
    open_delta, offset = consume_price(payload, offset)
    high_delta, offset = consume_price(payload, offset)
    low_delta, offset = consume_price(payload, offset)
    current_milli = current_delta * 10
    return {
        "current": current_milli,
        "last_close": (last_close_delta + current_delta) * 10,
        "open": (open_delta + current_delta) * 10,
        "high": (high_delta + current_delta) * 10,
        "low": (low_delta + current_delta) * 10,
    }, offset


def milli_to_float(value: int) -> float:
    return value / 1000.0


def price_divisor(code: str) -> int:
    full_code = normalize_code(code)
    return 10 if full_code[2:].startswith(PRICING_ETF_PREFIXES) else 1


def get_volume(value: int) -> float:
    if value == 0:
        return 0.0

    signed = int.from_bytes(value.to_bytes(4, "big", signed=False), "big", signed=True)
    logpoint = signed >> 24
    hleax = (signed >> 16) & 0xFF
    lheax = (signed >> 8) & 0xFF
    lleax = signed & 0xFF

    base = math.pow(2.0, float(logpoint * 2 - 0x7F))
    if hleax > 0x80:
        high = base * (64.0 + float(hleax & 0x7F)) / 64.0
    else:
        high = base * float(hleax) / 128.0

    scale = 2.0 if hleax & 0x80 else 1.0
    middle = base * float(lheax) / 32768.0 * scale
    low = base * float(lleax) / 8388608.0 * scale
    return base + high + middle + low


def decode_kline_datetime(raw_value: bytes, period_raw: int) -> datetime:
    if len(raw_value) != 4:
        raise ProtocolError("invalid kline time length")

    if period_raw in {0, 1, 2, 3, 7, 8}:
        date_packed = little_u16(raw_value[:2])
        minute_of_day = little_u16(raw_value[2:4])
        year = (date_packed >> 11) + 2004
        month = (date_packed % 2048) // 100
        day = (date_packed % 2048) % 100
        return datetime(
            year,
            month,
            day,
            minute_of_day // 60,
            minute_of_day % 60,
            tzinfo=SHANGHAI_TZ,
        )

    if period_raw == 13:
        epoch = datetime(2003, 12, 31, tzinfo=SHANGHAI_TZ)
        return epoch + timedelta(seconds=little_u32(raw_value))

    raw_date = little_u32(raw_value)
    parsed = date_from_yyyymmdd(raw_date)
    if parsed is None:
        raise ProtocolError(f"invalid kline date: {raw_date}")
    return datetime(parsed.year, parsed.month, parsed.day, 15, 0, tzinfo=SHANGHAI_TZ)


def minute_index_label(index: int) -> str:
    if index < 0:
        raise ProtocolError(f"invalid minute index: {index}")
    if index < 120:
        total_minutes = 9 * 60 + 30 + index + 1
    else:
        total_minutes = 13 * 60 + index - 119
    return f"{total_minutes // 60:02d}:{total_minutes % 60:02d}"


def minute_index_datetime(trading_date: date, index: int) -> datetime:
    label = minute_index_label(index)
    hour, minute = (int(part) for part in label.split(":", 1))
    return datetime(trading_date.year, trading_date.month, trading_date.day, hour, minute, tzinfo=SHANGHAI_TZ)
