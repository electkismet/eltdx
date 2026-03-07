from __future__ import annotations

from eltdx.protocol.frame import ResponseFrame
from eltdx.protocol.model_count import parse_count_payload


def test_parse_count_payload() -> None:
    payload = bytes.fromhex("d058")
    response = ResponseFrame(
        control=0x1C,
        msg_id=1,
        msg_type=0x044E,
        zip_length=len(payload),
        length=len(payload),
        data=payload,
        raw=b"\xB1\xCB\x74\x00",
    )

    parsed = parse_count_payload(response)

    assert parsed == 22736