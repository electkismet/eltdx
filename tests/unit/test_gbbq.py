from __future__ import annotations

from pathlib import Path

from eltdx.protocol.frame import ResponseFrame
from eltdx.protocol.model_gbbq import filter_xdxr_items, parse_gbbq_payload


FIXTURE = Path(__file__).resolve().parents[1] / "fixtures" / "gbbq_sz000001.hex"


def test_parse_gbbq_payload() -> None:
    payload_hex = FIXTURE.read_text(encoding="utf-8").strip()
    payload = bytes.fromhex(payload_hex)
    response = ResponseFrame(
        control=0x1C,
        msg_id=6,
        msg_type=0x000F,
        zip_length=len(payload),
        length=len(payload),
        data=payload,
        raw=b"\xB1\xCB\x74\x00",
    )

    parsed = parse_gbbq_payload(response, include_raw=True)

    assert parsed.count == 78
    assert parsed.items[0].code == "sz000001"
    assert parsed.items[0].time.isoformat() == "1990-03-01T15:00:00+08:00"
    assert parsed.items[0].category == 1
    assert parsed.items[0].category_name == "除权除息"
    assert round(parsed.items[0].c2, 2) == 3.56
    assert parsed.items[0].c4 == 1.0
    assert parsed.items[1].category == 5
    assert parsed.items[1].category_name == "股本变化"
    assert parsed.items[1].c3 == 26_500_000.0
    assert round(parsed.items[-1].c1, 2) == 2.36
    assert parsed.items[-1].time.isoformat() == "2025-10-15T15:00:00+08:00"
    assert parsed.raw_payload_hex == payload_hex


def test_filter_xdxr_items() -> None:
    payload_hex = FIXTURE.read_text(encoding="utf-8").strip()
    payload = bytes.fromhex(payload_hex)
    response = ResponseFrame(
        control=0x1C,
        msg_id=7,
        msg_type=0x000F,
        zip_length=len(payload),
        length=len(payload),
        data=payload,
        raw=b"\xB1\xCB\x74\x00",
    )

    parsed = parse_gbbq_payload(response)
    xdxr = filter_xdxr_items(parsed.items)

    assert len(xdxr) == 31
    assert xdxr[0].code == "sz000001"
    assert xdxr[0].time.isoformat() == "1990-03-01T15:00:00+08:00"
    assert xdxr[0].peigujia == 3.56
    assert xdxr[0].peigu == 1.0
    assert xdxr[-1].time.isoformat() == "2025-10-15T15:00:00+08:00"
    assert xdxr[-1].fenhong == 2.36
