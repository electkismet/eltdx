from __future__ import annotations

import json
import time
from urllib.request import Request, urlopen

from .exceptions import ProtocolError
from .models import SecurityCode

BSE_CODES_URL = "https://www.bse.cn/nqhqController/nqhq_en.do?callback=jQuery3710848510589806625_%d"
BSE_CODES_BODY = 'page={page}&type_en=%5B%22B%22%5D&sortfield=hqcjsl&sorttype=desc&xxfcbj_en=%5B2%5D&zqdm='
BSE_HEADERS = {
    "X-Requested-With": "XMLHttpRequest",
    "Content-Type": "application/x-www-form-urlencoded; charset=UTF-8",
    "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/123.0.0.0 Safari/537.36",
}
BSE_MAX_PAGES = 200
BSE_PAGE_DELAY = 0.1


def fetch_bj_codes(timeout: float = 8.0, *, max_pages: int = BSE_MAX_PAGES, page_delay: float = BSE_PAGE_DELAY) -> list[SecurityCode]:
    items: list[SecurityCode] = []
    for page in range(max_pages):
        page_items, last_page = fetch_bj_codes_page(page, timeout=timeout)
        items.extend(page_items)
        if last_page:
            return items
        time.sleep(page_delay)
    raise ProtocolError("bj code source exceeded max pages")


def fetch_bj_codes_page(page: int, *, timeout: float = 8.0) -> tuple[list[SecurityCode], bool]:
    url = BSE_CODES_URL % int(time.time() * 1000)
    body = BSE_CODES_BODY.format(page=page).encode("utf-8")
    request = Request(url, data=body, headers=BSE_HEADERS, method="POST")
    with urlopen(request, timeout=timeout) as response:
        payload = response.read()
    return parse_bj_codes_response(payload)


def parse_bj_codes_response(payload: bytes | str) -> tuple[list[SecurityCode], bool]:
    text = payload.decode("utf-8", errors="ignore") if isinstance(payload, bytes) else str(payload)
    start = text.find("(")
    end = text.rfind(")")
    if start < 0 or end <= start:
        raise ProtocolError(f"invalid bj code response: {text[:120]!r}")

    try:
        data = json.loads(text[start + 1 : end])
    except json.JSONDecodeError as exc:
        raise ProtocolError("invalid bj code json payload") from exc

    if not isinstance(data, list) or not data:
        raise ProtocolError("invalid bj code page payload")

    block = data[0]
    if not isinstance(block, dict):
        raise ProtocolError("invalid bj code page block")

    content = block.get("content")
    if not isinstance(content, list):
        raise ProtocolError("invalid bj code content list")

    items: list[SecurityCode] = []
    for record in content:
        if not isinstance(record, dict):
            raise ProtocolError("invalid bj code record")
        code = str(record.get("hqzqdm", "")).strip()
        if len(code) != 6:
            raise ProtocolError(f"invalid bj code value: {code!r}")
        name = str(record.get("hqzqjc", "")).strip()
        last_price = _coerce_float(record.get("hqzjcj"))
        items.append(
            SecurityCode(
                exchange="bj",
                code=code,
                name=name,
                multiple=0,
                decimal=0,
                last_price=last_price,
                last_price_raw=int(round(last_price * 1000.0)),
            )
        )

    return items, bool(block.get("lastPage"))


def _coerce_float(value) -> float:
    if value in (None, "", "--"):
        return 0.0
    try:
        return float(value)
    except (TypeError, ValueError) as exc:
        raise ProtocolError(f"invalid bj code price: {value!r}") from exc
