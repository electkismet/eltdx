from __future__ import annotations

from datetime import datetime

from ..exceptions import ProtocolError
from ..models import GbbqItem, GbbqResponse, XdxrItem
from .constants import TYPE_GBBQ
from .frame import RequestFrame, ResponseFrame
from .unit import SHANGHAI_TZ, decode_code, get_volume, little_f32, little_u16, little_u32

CATEGORY_NAMES = {
    1: "除权除息",
    2: "送配股上市",
    3: "非流通股上市",
    4: "未知股本变动",
    5: "股本变化",
    6: "增发新股",
    7: "股份回购",
    8: "增发新股上市",
    9: "转配股上市",
    10: "可转债上市",
    11: "扩缩股",
    12: "非流通股缩股",
    13: "送认购权证",
    14: "送认沽权证",
}


def build_gbbq_frame(code: str, msg_id: int) -> RequestFrame:
    exchange, number = decode_code(code)
    payload = bytes([0x01, 0x00, exchange]) + number.encode("ascii")
    return RequestFrame(msg_id=msg_id, msg_type=TYPE_GBBQ, data=payload)


def parse_gbbq_payload(response: ResponseFrame, *, include_raw: bool = False) -> GbbqResponse:
    payload = response.data
    if len(payload) < 11:
        raise ProtocolError("invalid gbbq payload")

    count = little_u16(payload[9:11])
    offset = 11
    items: list[GbbqItem] = []

    for _ in range(count):
        if offset + 29 > len(payload):
            raise ProtocolError("truncated gbbq payload")

        exchange = payload[offset]
        code = payload[offset + 1 : offset + 7].decode("ascii")
        yyyymmdd = little_u32(payload[offset + 8 : offset + 12])
        year = yyyymmdd // 10000
        month = (yyyymmdd % 10000) // 100
        day = yyyymmdd % 100
        category = payload[offset + 12]
        offset += 13

        c1 = 0.0
        c2 = 0.0
        c3 = 0.0
        c4 = 0.0

        if category == 1:
            c1 = float(little_f32(payload[offset : offset + 4]))
            c2 = float(little_f32(payload[offset + 4 : offset + 8]))
            c3 = float(little_f32(payload[offset + 8 : offset + 12]))
            c4 = float(little_f32(payload[offset + 12 : offset + 16]))
        elif category in {11, 12}:
            c3 = float(little_f32(payload[offset + 8 : offset + 12]))
        elif category in {13, 14}:
            c1 = float(little_f32(payload[offset : offset + 4]))
            c3 = float(little_f32(payload[offset + 8 : offset + 12]))
        else:
            c1 = get_volume(little_u32(payload[offset : offset + 4])) * 1e4
            c2 = get_volume(little_u32(payload[offset + 4 : offset + 8])) * 1e4
            c3 = get_volume(little_u32(payload[offset + 8 : offset + 12])) * 1e4
            c4 = get_volume(little_u32(payload[offset + 12 : offset + 16])) * 1e4

        items.append(
            GbbqItem(
                code={0: "sz", 1: "sh", 2: "bj"}.get(exchange, "unknown") + code,
                time=datetime(year, month, day, 15, 0, 0, 0, tzinfo=SHANGHAI_TZ),
                category=category,
                category_name=CATEGORY_NAMES.get(category),
                c1=c1,
                c2=c2,
                c3=c3,
                c4=c4,
            )
        )
        offset += 16

    return GbbqResponse(
        count=len(items),
        items=items,
        raw_frame_hex=response.raw.hex() if include_raw else None,
        raw_payload_hex=payload.hex() if include_raw else None,
    )


def filter_xdxr_items(items: list[GbbqItem]) -> list[XdxrItem]:
    return [
        XdxrItem(
            code=item.code,
            time=item.time,
            fenhong=round(item.c1, 2),
            peigujia=round(item.c2, 2),
            songzhuangu=round(item.c3, 2),
            peigu=round(item.c4, 2),
        )
        for item in items
        if item.category == 1
    ]
