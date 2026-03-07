from __future__ import annotations

import math
import struct
from datetime import date, datetime, timedelta, timezone

from ..enums import Exchange, KlinePeriod
from ..exceptions import ProtocolError

SHANGHAI_TZ = timezone(timedelta(hours=8), name="Asia/Shanghai")


def _exp2(value: float) -> float:
    exp2 = getattr(math, "exp2", None)
    if exp2 is not None:
        return exp2(value)
    return math.pow(2.0, value)


EXCHANGE_TO_WIRE = {
    Exchange.SZ: 0,
    Exchange.SH: 1,
    Exchange.BJ: 2,
}
WIRE_TO_EXCHANGE = {value: key for key, value in EXCHANGE_TO_WIRE.items()}
PRICING_ETF_PREFIXES = ("15", "16", "50", "51", "52", "53", "56", "58")
SH_A_SHARE_PREFIXES = ("600", "601", "603", "605", "688", "689")
SZ_A_SHARE_PREFIXES = ("000", "001", "002", "003", "300", "301")
BJ_A_SHARE_PREFIXES = ("920",)
SH_B_SHARE_PREFIXES = ("900",)
SZ_B_SHARE_PREFIXES = ("200",)
SH_STOCK_PREFIXES = SH_A_SHARE_PREFIXES + SH_B_SHARE_PREFIXES
SZ_STOCK_PREFIXES = SZ_A_SHARE_PREFIXES + SZ_B_SHARE_PREFIXES
BJ_STOCK_PREFIXES = BJ_A_SHARE_PREFIXES
SH_INDEX_PREFIXES = ("000", "880", "881", "999")
SZ_INDEX_PREFIXES = ("399",)
BJ_INDEX_PREFIXES = ("899",)
SH_ETF_ENTRY_PREFIXES = (
    "510",
    "511",
    "512",
    "513",
    "515",
    "516",
    "517",
    "518",
    "520",
    "560",
    "561",
    "562",
    "563",
    "588",
)
SZ_ETF_ENTRY_PREFIXES = ("159",)
KLINE_PERIOD_ALIASES = {
    KlinePeriod.MINUTE_1: KlinePeriod.MINUTE_1,
    KlinePeriod.MINUTE_5: KlinePeriod.MINUTE_5,
    KlinePeriod.MINUTE_15: KlinePeriod.MINUTE_15,
    KlinePeriod.MINUTE_30: KlinePeriod.MINUTE_30,
    KlinePeriod.MINUTE_60: KlinePeriod.MINUTE_60,
    KlinePeriod.DAY: KlinePeriod.DAY,
    KlinePeriod.WEEK: KlinePeriod.WEEK,
    KlinePeriod.MONTH: KlinePeriod.MONTH,
    KlinePeriod.QUARTER: KlinePeriod.QUARTER,
    KlinePeriod.YEAR: KlinePeriod.YEAR,
    "1m": KlinePeriod.MINUTE_1,
    "1min": KlinePeriod.MINUTE_1,
    "1minute": KlinePeriod.MINUTE_1,
    "minute": KlinePeriod.MINUTE_1,
    "min": KlinePeriod.MINUTE_1,
    "m1": KlinePeriod.MINUTE_1,
    "5m": KlinePeriod.MINUTE_5,
    "5min": KlinePeriod.MINUTE_5,
    "5minute": KlinePeriod.MINUTE_5,
    "m5": KlinePeriod.MINUTE_5,
    "15m": KlinePeriod.MINUTE_15,
    "15min": KlinePeriod.MINUTE_15,
    "15minute": KlinePeriod.MINUTE_15,
    "m15": KlinePeriod.MINUTE_15,
    "30m": KlinePeriod.MINUTE_30,
    "30min": KlinePeriod.MINUTE_30,
    "30minute": KlinePeriod.MINUTE_30,
    "m30": KlinePeriod.MINUTE_30,
    "60m": KlinePeriod.MINUTE_60,
    "60min": KlinePeriod.MINUTE_60,
    "60minute": KlinePeriod.MINUTE_60,
    "1h": KlinePeriod.MINUTE_60,
    "hour": KlinePeriod.MINUTE_60,
    "day": KlinePeriod.DAY,
    "1d": KlinePeriod.DAY,
    "daily": KlinePeriod.DAY,
    "d": KlinePeriod.DAY,
    "week": KlinePeriod.WEEK,
    "1w": KlinePeriod.WEEK,
    "weekly": KlinePeriod.WEEK,
    "w": KlinePeriod.WEEK,
    "month": KlinePeriod.MONTH,
    "1mo": KlinePeriod.MONTH,
    "monthly": KlinePeriod.MONTH,
    "mo": KlinePeriod.MONTH,
    "quarter": KlinePeriod.QUARTER,
    "1q": KlinePeriod.QUARTER,
    "quarterly": KlinePeriod.QUARTER,
    "q": KlinePeriod.QUARTER,
    "year": KlinePeriod.YEAR,
    "1y": KlinePeriod.YEAR,
    "yearly": KlinePeriod.YEAR,
    "y": KlinePeriod.YEAR,
}
INTRADAY_KLINE_PERIODS = {
    KlinePeriod.MINUTE_1,
    KlinePeriod.MINUTE_5,
    KlinePeriod.MINUTE_15,
    KlinePeriod.MINUTE_30,
    KlinePeriod.MINUTE_60,
}


def milli_to_float(value: int) -> float:
    return value / 1000.0


def little_f32(data: bytes) -> float:
    return struct.unpack("<f", data)[0]


def little_u16(data: bytes) -> int:
    return int.from_bytes(data, "little", signed=False)


def little_u32(data: bytes) -> int:
    return int.from_bytes(data, "little", signed=False)


def little_i16(data: bytes) -> int:
    return int.from_bytes(data, "little", signed=True)


uint16_le = little_u16
uint32_le = little_u32


def normalize_exchange(exchange: str | Exchange | int) -> Exchange:
    if isinstance(exchange, Exchange):
        return exchange
    if isinstance(exchange, int):
        try:
            return WIRE_TO_EXCHANGE[exchange]
        except KeyError as exc:
            raise ProtocolError(f"invalid exchange: {exchange!r}") from exc

    value = str(exchange).strip().lower()
    try:
        return Exchange(value)
    except ValueError as exc:
        raise ProtocolError(f"invalid exchange: {exchange!r}") from exc


def exchange_to_wire(exchange: str | Exchange | int) -> int:
    return EXCHANGE_TO_WIRE[normalize_exchange(exchange)]


def wire_to_exchange(value: int) -> Exchange:
    try:
        return WIRE_TO_EXCHANGE[value]
    except KeyError as exc:
        raise ProtocolError(f"invalid wire exchange: {value!r}") from exc


def decode_gbk_text(data: bytes) -> str:
    return data.decode("gbk", errors="ignore").replace("\x00", "").strip()


def normalize_trading_date(value: str | date | datetime | None = None) -> tuple[str, date]:
    if value is None:
        today = datetime.now(SHANGHAI_TZ).date()
        return today.strftime("%Y%m%d"), today

    if isinstance(value, datetime):
        current = value if value.tzinfo is None else value.astimezone(SHANGHAI_TZ)
        trading_day = current.date()
        return trading_day.strftime("%Y%m%d"), trading_day

    if isinstance(value, date):
        return value.strftime("%Y%m%d"), value

    text = str(value).strip()
    for fmt in ("%Y%m%d", "%Y-%m-%d"):
        try:
            trading_day = datetime.strptime(text, fmt).date()
            return trading_day.strftime("%Y%m%d"), trading_day
        except ValueError:
            pass
    raise ProtocolError(f"invalid trading date: {value!r}")


def clock_minutes_to_datetime(trading_date: str | date | datetime, total_minutes: int) -> datetime:
    _, trading_day = normalize_trading_date(trading_date)
    if total_minutes < 0 or total_minutes >= 24 * 60:
        raise ProtocolError(f"invalid clock minutes: {total_minutes}")

    hour = total_minutes // 60
    minute = total_minutes % 60
    return datetime(
        trading_day.year,
        trading_day.month,
        trading_day.day,
        hour,
        minute,
        0,
        0,
        tzinfo=SHANGHAI_TZ,
    )


def minute_index_to_datetime(trading_date: str | date | datetime, index: int) -> datetime:
    _, trading_day = normalize_trading_date(trading_date)
    if index < 0:
        raise ProtocolError(f"invalid minute index: {index}")

    if index < 120:
        total_minutes = 9 * 60 + 30 + index + 1
    else:
        total_minutes = 13 * 60 + index - 119

    return clock_minutes_to_datetime(trading_day, total_minutes)


def add_prefix(code: str) -> str:
    code = code.strip().lower()
    if len(code) == 8 and code[:2] in {"sh", "sz", "bj"}:
        return code
    if len(code) != 6:
        raise ProtocolError(f"invalid code: {code!r}")
    if code.startswith("6"):
        return "sh" + code
    if code.startswith("0") or code.startswith("30"):
        return "sz" + code
    if code.startswith("92"):
        return "bj" + code
    if code.startswith("15") or code.startswith("16"):
        return "sz" + code
    if code.startswith(PRICING_ETF_PREFIXES[2:]):
        return "sh" + code
    if code.startswith(SZ_INDEX_PREFIXES):
        return "sz" + code
    if code.startswith(("880", "881", "999")):
        return "sh" + code
    if code.startswith("399"):
        return "sz" + code
    if code.startswith("000") or code == "999999":
        return "sh" + code
    if code.startswith("899"):
        return "bj" + code
    raise ProtocolError(f"unable to infer exchange for code: {code!r}")


def decode_code(code: str) -> tuple[int, str]:
    full_code = add_prefix(code)
    exchange = full_code[:2]
    if exchange == "sz":
        return 0, full_code[2:]
    if exchange == "sh":
        return 1, full_code[2:]
    if exchange == "bj":
        return 2, full_code[2:]
    raise ProtocolError(f"invalid exchange: {full_code!r}")


def is_etf(code: str) -> bool:
    full_code = add_prefix(code)
    return full_code[2:].startswith(PRICING_ETF_PREFIXES)


def price_divisor(code: str) -> int:
    return 10 if is_etf(code) else 1


def is_index(code: str) -> bool:
    full_code = add_prefix(code)
    number = full_code[2:]
    if full_code.startswith("sh"):
        return number.startswith(SH_INDEX_PREFIXES)
    if full_code.startswith("sz"):
        return number.startswith(SZ_INDEX_PREFIXES)
    if full_code.startswith("bj"):
        return number.startswith(BJ_INDEX_PREFIXES)
    return False


def is_a_share_entry(code: str) -> bool:
    full_code = add_prefix(code)
    number = full_code[2:]
    if full_code.startswith("sh"):
        return number.startswith(SH_A_SHARE_PREFIXES)
    if full_code.startswith("sz"):
        return number.startswith(SZ_A_SHARE_PREFIXES)
    if full_code.startswith("bj"):
        return number.startswith(BJ_A_SHARE_PREFIXES)
    return False


def is_b_share_entry(code: str) -> bool:
    full_code = add_prefix(code)
    number = full_code[2:]
    if full_code.startswith("sh"):
        return number.startswith(SH_B_SHARE_PREFIXES)
    if full_code.startswith("sz"):
        return number.startswith(SZ_B_SHARE_PREFIXES)
    return False


def is_stock_entry(code: str) -> bool:
    return is_a_share_entry(code) or is_b_share_entry(code)


def is_etf_entry(code: str, name: str | None = None) -> bool:
    full_code = add_prefix(code)
    number = full_code[2:]
    normalized_name = (name or "").upper()
    if "ETF" not in normalized_name and "ＥＴＦ" not in (name or ""):
        return False
    if full_code.startswith("sh"):
        return number.startswith(SH_ETF_ENTRY_PREFIXES)
    if full_code.startswith("sz"):
        return number.startswith(SZ_ETF_ENTRY_PREFIXES)
    return False


def normalize_kline_period(value: str | KlinePeriod) -> KlinePeriod:
    if isinstance(value, KlinePeriod):
        return value

    key = str(value).strip().lower()
    try:
        return KLINE_PERIOD_ALIASES[key]
    except KeyError as exc:
        raise ProtocolError(f"invalid kline period: {value!r}") from exc


def decode_kline_datetime(raw_value: bytes, period: str | KlinePeriod) -> datetime:
    resolved_period = normalize_kline_period(period)
    if resolved_period in INTRADAY_KLINE_PERIODS:
        year_month_day = little_u16(raw_value[:2])
        hour_minute = little_u16(raw_value[2:4])
        year = (year_month_day >> 11) + 2004
        month = (year_month_day % 2048) // 100
        day = (year_month_day % 2048) % 100
        hour = hour_minute // 60
        minute = hour_minute % 60
        return datetime(year, month, day, hour, minute, 0, 0, tzinfo=SHANGHAI_TZ)

    year_month_day = little_u32(raw_value)
    year = year_month_day // 10000
    month = (year_month_day % 10000) // 100
    day = year_month_day % 100
    return datetime(year, month, day, 15, 0, 0, 0, tzinfo=SHANGHAI_TZ)


def fix_kline_times(items, *, now: datetime | None = None):
    if not items:
        return items

    current = now or datetime.now(SHANGHAI_TZ)
    if current.tzinfo is None:
        current = current.replace(tzinfo=SHANGHAI_TZ)
    else:
        current = current.astimezone(SHANGHAI_TZ)

    start = current.replace(hour=13, minute=0, second=0, microsecond=0)
    end = current.replace(hour=15, minute=0, second=0, microsecond=0)
    last_time = items[-1].time
    if last_time < start or last_time > end:
        return items

    for item in items[-120:]:
        if item.time.hour == 13 and item.time.minute == 0 and item.time.date() == current.date():
            item.time = item.time.replace(hour=11, minute=30)
    return items


def decode_quote_datetime(raw_value: int, *, now: datetime | None = None) -> datetime | None:
    current = now or datetime.now(SHANGHAI_TZ)
    if current.tzinfo is None:
        current = current.replace(tzinfo=SHANGHAI_TZ)
    else:
        current = current.astimezone(SHANGHAI_TZ)

    text = str(raw_value)
    if len(text) == 8:
        hour = int(text[0:2])
        minute = int(text[2:4])
        second = int(text[4:6])
        microsecond = int(text[6:8]) * 10_000
    elif len(text) == 9:
        hour = int(text[0:2])
        minute = int(text[2:4])
        second = int(text[4:6])
        microsecond = int(text[6:9]) * 1_000
    else:
        return None

    if hour > 23 or minute > 59 or second > 59:
        return None

    return current.replace(
        hour=hour,
        minute=minute,
        second=second,
        microsecond=microsecond,
    )


def consume_varint(payload: bytes, offset: int) -> tuple[int, int]:
    if offset >= len(payload):
        raise ProtocolError("unexpected end of payload")

    value = 0
    position = offset
    shift = 0

    while True:
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
        if position >= len(payload):
            raise ProtocolError("unterminated varint")

    if payload[offset] & 0x40:
        value = -value
    return value, position


def consume_price(payload: bytes, offset: int) -> tuple[int, int]:
    return consume_varint(payload, offset)


def decode_k(payload: bytes, offset: int) -> tuple[dict[str, int], int]:
    close_delta, offset = consume_price(payload, offset)
    last_delta, offset = consume_price(payload, offset)
    open_delta, offset = consume_price(payload, offset)
    high_delta, offset = consume_price(payload, offset)
    low_delta, offset = consume_price(payload, offset)

    close_milli = close_delta * 10
    last_milli = (last_delta + close_delta) * 10
    open_milli = (open_delta + close_delta) * 10
    high_milli = (high_delta + close_delta) * 10
    low_milli = (low_delta + close_delta) * 10

    return {
        "close": close_milli,
        "last": last_milli,
        "open": open_milli,
        "high": high_milli,
        "low": low_milli,
    }, offset


def get_volume(value: int) -> float:
    if value == 0:
        return 0.0

    signed = int.from_bytes(value.to_bytes(4, "big", signed=False), "big", signed=True)
    logpoint = signed >> 24
    hleax = (signed >> 16) & 0xFF
    lheax = (signed >> 8) & 0xFF
    lleax = signed & 0xFF

    dw_ecx = logpoint * 2 - 0x7F
    dw_edx = logpoint * 2 - 0x86
    dw_esi = logpoint * 2 - 0x8E
    dw_eax = logpoint * 2 - 0x96

    magnitude = abs(dw_ecx)
    dbl_xmm6 = math.pow(2.0, float(magnitude))
    if dw_ecx < 0:
        dbl_xmm6 = 1.0 / dbl_xmm6

    if hleax > 0x80:
        dbl_xmm0 = math.pow(2.0, float(dw_edx)) * 128.0
        dbl_xmm0 += float(hleax & 0x7F) * math.pow(2.0, float(dw_edx + 1))
        dbl_xmm4 = dbl_xmm0
    else:
        if dw_edx >= 0:
            dbl_xmm0 = math.pow(2.0, float(dw_edx)) * float(hleax)
        else:
            dbl_xmm0 = (1.0 / math.pow(2.0, float(dw_edx))) * float(hleax)
        dbl_xmm4 = dbl_xmm0

    dbl_xmm3 = math.pow(2.0, float(dw_esi)) * float(lheax)
    dbl_xmm1 = math.pow(2.0, float(dw_eax)) * float(lleax)
    if hleax & 0x80:
        dbl_xmm3 *= 2.0
        dbl_xmm1 *= 2.0
    return dbl_xmm6 + dbl_xmm4 + dbl_xmm3 + dbl_xmm1


def get_volume2(value: int) -> float:
    if value == 0:
        return 0.0

    signed = int.from_bytes(value.to_bytes(4, "big", signed=False), "big", signed=True)
    logpoint = signed >> 24
    hleax = (signed >> 16) & 0xFF
    lheax = (signed >> 8) & 0xFF
    lleax = signed & 0xFF

    dbl_xmm6 = _exp2(float(logpoint * 2 - 0x7F))
    if hleax > 0x80:
        dbl_xmm4 = dbl_xmm6 * (64.0 + float(hleax & 0x7F)) / 64.0
    else:
        dbl_xmm4 = dbl_xmm6 * float(hleax) / 128.0

    scale = 2.0 if hleax & 0x80 else 1.0
    dbl_xmm3 = dbl_xmm6 * float(lheax) / 32768.0 * scale
    dbl_xmm1 = dbl_xmm6 * float(lleax) / 8388608.0 * scale
    return dbl_xmm6 + dbl_xmm4 + dbl_xmm3 + dbl_xmm1
