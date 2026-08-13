"""Security code table command builders and parsers."""

from __future__ import annotations

from eltdx.exceptions import ProtocolError
from eltdx.models import SecurityCode
from eltdx.protocol.constants import MAX_CODE_PAGE_SIZE, TYPE_SECURITY_COUNT, TYPE_SECURITY_LIST
from eltdx.protocol.frame import RequestFrame, ResponseFrame
from eltdx.protocol.unit import decode_gbk_text, little_f32, little_u16, market_to_id, normalize_market, yyyymmdd

CODE_RECORD_SIZE = 37


def build_security_count_frame(payload: dict, msg_id: int) -> RequestFrame:
    market_id = market_to_id(payload.get("market", payload.get("market_id", "sz")))
    client_date = yyyymmdd(payload.get("client_date_yyyymmdd", payload.get("client_date")))
    data = market_id.to_bytes(2, "little", signed=False) + client_date.to_bytes(4, "little", signed=False)
    return RequestFrame(msg_id=msg_id, msg_type=TYPE_SECURITY_COUNT, data=data)


def parse_security_count_payload(response: ResponseFrame) -> int:
    if len(response.data) < 2:
        raise ProtocolError("invalid security count payload")
    return little_u16(response.data[:2])


def build_security_list_frame(payload: dict, msg_id: int) -> RequestFrame:
    start = int(payload.get("start", 0))
    limit = int(payload.get("limit", 1600))
    if start < 0 or start > 0xFFFFFFFF:
        raise ValueError("start must be between 0 and 4294967295")
    if limit < 0 or limit > MAX_CODE_PAGE_SIZE:
        raise ValueError(f"limit must be between 0 and {MAX_CODE_PAGE_SIZE}")

    market_id = market_to_id(payload.get("market", payload.get("market_id", "sz")))
    data = (
        market_id.to_bytes(2, "little", signed=False)
        + start.to_bytes(4, "little", signed=False)
        + limit.to_bytes(4, "little", signed=False)
        + b"\x00" * 4
    )
    return RequestFrame(msg_id=msg_id, msg_type=TYPE_SECURITY_LIST, data=data)


def parse_security_list_payload(response: ResponseFrame, request_payload: dict | None = None) -> list[SecurityCode]:
    payload = response.data
    if len(payload) < 2:
        raise ProtocolError("invalid security list payload")

    request_payload = request_payload or {}
    exchange = normalize_market(request_payload.get("market", request_payload.get("market_id", "sz")))
    market_id = market_to_id(exchange)
    count = little_u16(payload[:2])
    expected_length = 2 + count * CODE_RECORD_SIZE
    if len(payload) < expected_length:
        raise ProtocolError("truncated security list payload")

    items: list[SecurityCode] = []
    offset = 2
    for _ in range(count):
        record = payload[offset : offset + CODE_RECORD_SIZE]
        offset += CODE_RECORD_SIZE
        code = _decode_code(record[:6])
        unknown0_raw = record[24:28]
        previous_close_raw = record[29:33]
        full_code = f"{exchange}{code}"
        category, category_reason = classify_security(full_code)
        board, board_reason = classify_board(full_code, category)
        items.append(
            SecurityCode(
                exchange=exchange,
                market_id=market_id,
                code=code,
                name=decode_gbk_text(record[8:24]),
                multiple=little_u16(record[6:8]),
                decimal=record[28],
                previous_close_price=little_f32(previous_close_raw),
                volume_ratio_base=little_f32(unknown0_raw),
                unknown0_raw=unknown0_raw,
                previous_close_raw=previous_close_raw,
                unknown3_raw=record[33:37],
                category=category,
                category_reason=category_reason,
                board=board,
                board_reason=board_reason,
            )
        )
    return items


def _decode_code(data: bytes) -> str:
    try:
        return data.decode("ascii")
    except UnicodeDecodeError as exc:
        raise ProtocolError("invalid security code") from exc


def classify_security(full_code: str) -> tuple[str, str]:
    code = full_code.lower()
    if code.startswith(("sh000", "sh880", "sh881", "sh999", "sz399", "bj899")):
        return "index", "index code prefix"
    if code.startswith(
        (
            "sh510",
            "sh511",
            "sh512",
            "sh513",
            "sh515",
            "sh516",
            "sh517",
            "sh518",
            "sh520",
            "sh560",
            "sh561",
            "sh562",
            "sh563",
            "sh588",
            "sz158",
            "sz159",
        )
    ):
        return "etf", "ETF code prefix"
    if code.startswith(("sh600", "sh601", "sh603", "sh605", "sh688", "sh689")):
        return "a_share", "SSE A-share code prefix"
    if code.startswith(("sz000", "sz001", "sz002", "sz003", "sz004", "sz300", "sz301")):
        return "a_share", "SZSE A-share code prefix"
    if code.startswith("bj92"):
        return "a_share", "BSE listed stock code prefix"
    if code.startswith("sh900") or any(code.startswith(f"sz20{digit}") for digit in range(10)):
        return "b_share", "B-share code prefix"
    if code.startswith("bj810"):
        return "private_convertible_bond", "BSE private convertible bond prefix"
    if code.startswith("bj821"):
        return "bond", "BSE bond sample prefix"
    return "unknown", "no matched code prefix"


def classify_board(full_code: str, category: str | None = None) -> tuple[str, str]:
    code = full_code.lower()
    if category not in {None, "a_share"}:
        return "none", "not an A-share stock"
    if code.startswith(("sh600", "sh601", "sh603", "sh605")):
        return "sse_main_board", "SSE main-board prefix"
    if code.startswith(("sh688", "sh689")):
        return "sse_star_market", "SSE STAR Market prefix"
    if code.startswith(("sz000", "sz001", "sz002", "sz003", "sz004")):
        return "szse_main_board", "SZSE main-board prefix"
    if code.startswith(("sz300", "sz301")):
        return "szse_chinext", "SZSE ChiNext prefix"
    if code.startswith("bj92"):
        return "bse_listed_stock", "BSE listed stock prefix"
    return "none", "no stock board matched"
