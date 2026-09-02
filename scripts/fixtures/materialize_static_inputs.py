"""Materialize one reviewed 7709 fixture input case without importing eltdx."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import struct
import zlib
from dataclasses import dataclass
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_ROOT = ROOT / "tests" / "fixtures" / "7709"
REQUIRED_FILES = frozenset({"request.json", "response.bin", "metadata.json"})


@dataclass(frozen=True, slots=True)
class Case:
    command: str
    code: int
    message_id: int
    request: dict[str, Any]
    response_payload: bytes
    push_generation: int | None = None
    push_host: str | None = None
    case_id: str = "normal"
    wire_payload: bytes | None = None
    decoded_length: int | None = None
    response_message_id: int | None = None


def _signed_varint(value: int) -> bytes:
    sign = 0x40 if value < 0 else 0
    remaining = abs(value)
    first = (remaining & 0x3F) | sign
    remaining >>= 6
    if remaining:
        first |= 0x80
    output = [first]
    while remaining:
        byte = remaining & 0x7F
        remaining >>= 7
        if remaining:
            byte |= 0x80
        output.append(byte)
    return bytes(output)


def _handshake_payload() -> bytes:
    payload = bytearray(189)
    payload[1:3] = (2026).to_bytes(2, "little")
    payload[3:9] = bytes((27, 5, 30, 10, 0, 0))
    payload[42:46] = (20260527).to_bytes(4, "little")
    payload[50:54] = (20260527).to_bytes(4, "little")
    payload[68:152] = b"fixture-7709".ljust(84, b"\x00")
    payload[160:189] = b"fixture-product".ljust(29, b"\x00")
    return bytes(payload)


def _security_list_payload() -> bytes:
    record = (
        b"000001"
        + (100).to_bytes(2, "little")
        + bytes.fromhex("c6bdb0b2d2f8d0d0").ljust(16, b"\x00")
        + struct.pack("<f", 3956.656494)
        + b"\x02"
        + struct.pack("<f", 10.99)
        + bytes.fromhex("67316825")
    )
    return (1).to_bytes(2, "little") + record


def _legacy_quotes_payload() -> bytes:
    record = bytearray(b"\x00" + b"000001" + (7).to_bytes(2, "little"))
    for value in (1014, -14, -1, 6, -10, 103000, -1014, 1000, 15):
        record.extend(_signed_varint(value))
    record.extend((12345678).to_bytes(4, "little"))
    for value in (400, 600, 0, 100):
        record.extend(_signed_varint(value))
    for values in (
        (-1, 0, 320, 428),
        (-2, 1, 118, 260),
        (-3, 2, 94, 136),
        (-4, 3, 87, 92),
        (-5, 4, 66, 71),
    ):
        for value in values:
            record.extend(_signed_varint(value))
    record.extend((0x20).to_bytes(2, "little"))
    for value in (1, -2, 3, -4):
        record.extend(_signed_varint(value))
    record.extend((21).to_bytes(2, "little", signed=True))
    record.extend((8).to_bytes(2, "little"))
    return (0x0701).to_bytes(2, "little") + (1).to_bytes(2, "little") + record


def _capital_changes_payload() -> bytes:
    record = (
        b"\x00"
        + b"000001"
        + b"\x00"
        + (20260511).to_bytes(4, "little")
        + bytes([15])
        + struct.pack("<ffff", 0.0, 0.0, 3.5, 0.0)
    )
    return (1).to_bytes(2, "little") + b"\x00" + b"000001" + b"\x01\x00" + record


def _finance_batch_payload() -> bytes:
    info = struct.pack("<fHHII30f", 100.0, 1, 2, 20260425, 19910403, *([0.0] * 30))
    return b"\x01\x00\x00" + b"000001" + info


def _sparkline_payload() -> bytes:
    header = (
        b"\x00\x00"
        + b"000001"
        + b"\x00" * 16
        + b"\x01\x00"
        + (0).to_bytes(4, "little")
        + (0).to_bytes(4, "little")
        + (60).to_bytes(2, "little")
        + struct.pack("<f", 10.0)
        + (2).to_bytes(2, "little")
    )
    return header + struct.pack("<ff", 10.0, 10.1)


def _cases() -> dict[str, Case]:
    snapshot_record = bytes.fromhex(
        "00303030303031e61185115b5c005fa4a3cf0ec51187e9aa01bfe40e40afb44eb0994298cf6800"
        "b8df094100901381c3011614120010004091fc4c000000000000000000000000ca0b9f409ffa84c200"
        "00000000000000000000000000000000000000000000e611"
    )
    recent = b"\x01\x00" + struct.pack("<ff", 10.0, 10.1) + bytes.fromhex("0a0b0c")
    historical_ticks = b"\x01\x00" + struct.pack("<f", 35.5) + bytes.fromhex("50030a14030500")
    cases = (
        Case("heartbeat", 0x0004, 0x30000001, {}, bytes.fromhex("000000000000a8263501")),
        Case("handshake", 0x000D, 0x30000002, {}, _handshake_payload()),
        Case(
            "capital_changes",
            0x000F,
            0x30000003,
            {"code": "sz000001", "include_raw": True},
            _capital_changes_payload(),
        ),
        Case(
            "finance_batch",
            0x0010,
            0x30000004,
            {"codes": ["sz000001"], "include_raw": True},
            _finance_batch_payload(),
        ),
        Case(
            "security_list",
            0x044D,
            0x30000005,
            {"market": "sz", "start": 0, "limit": 1600},
            _security_list_payload(),
        ),
        Case(
            "security_count",
            0x044E,
            0x30000006,
            {"market": "sz", "client_date_yyyymmdd": 20260519},
            bytes.fromhex("f55a"),
        ),
        Case(
            "special_limits",
            0x0452,
            0x30000007,
            {"start_index": 2},
            b"\x01\x00\x00" + (123054).to_bytes(4, "little") + struct.pack("<ff", 212.531, 141.687),
        ),
        Case(
            "intraday_aux",
            0x051B,
            0x30000008,
            {"code": "sz000988", "kind": "buy_sell_strength", "include_raw": True},
            bytes.fromhex("01000506"),
        ),
        Case(
            "klines",
            0x052D,
            0x30000009,
            {"code": "sz300308", "period": "day", "start": 0, "count": 420, "include_raw": True},
            b"\x00\x00",
        ),
        Case(
            "today_intraday",
            0x0537,
            0x3000000A,
            {"code": "sz000988", "include_raw": True},
            b"\x00\x00\x00\x00",
        ),
        Case(
            "legacy_quotes",
            0x053E,
            0x3000000B,
            {"codes": ["sz000001"]},
            _legacy_quotes_payload(),
        ),
        Case(
            "refresh_stream",
            0x0547,
            0x3000000C,
            {"codes": ["sz000001"], "cursors": {}},
            bytes.fromhex("9393"),
            push_generation=1,
            push_host="127.0.0.1:7709",
        ),
        Case(
            "category_quotes",
            0x054B,
            0x3000000D,
            {"category": 6, "sort_by": None, "start": 0, "count": 80, "ascending": False},
            b"\x00\x00\x00\x00",
        ),
        Case(
            "snapshots",
            0x054C,
            0x3000000E,
            {"codes": ["sz000001"]},
            b"\x00\x00\x01\x00" + snapshot_record,
        ),
        Case(
            "auction_series",
            0x056A,
            0x3000000F,
            {"code": "sz000988", "include_raw": True},
            b"\x01\x00" + bytes.fromhex("2b02b81e2243080a0000810900000000"),
        ),
        Case(
            "file_content",
            0x06B9,
            0x30000010,
            {"path": "zhb.zip", "offset": 10, "size": 30000},
            b"\x06\x00\x00\x00abc123\xaa\xbb",
        ),
        Case(
            "historical_intraday",
            0x0FB4,
            0x30000011,
            {"code": "sz300308", "trading_date": 20260511, "include_raw": True},
            b"\x00\x00" + struct.pack("<f", 10.0),
        ),
        Case(
            "today_ticks",
            0x0FC5,
            0x30000012,
            {"code": "sz000001", "start": 0, "count": 1800, "include_raw": True},
            bytes.fromhex("010050030a14030000"),
        ),
        Case(
            "historical_ticks",
            0x0FC6,
            0x30000013,
            {
                "code": "sz300308",
                "trading_date": 20260511,
                "start": 0,
                "count": 1800,
                "include_raw": True,
            },
            historical_ticks,
        ),
        Case(
            "sparkline",
            0x0FD1,
            0x30000014,
            {"code": "sz000001", "selector": 1, "window": 20, "include_raw": True},
            _sparkline_payload(),
        ),
        Case(
            "recent_intraday",
            0x0FEB,
            0x30000015,
            {"code": "sz300308", "trading_date": 20260511, "include_raw": True},
            recent,
        ),
    )
    return {case.command: case for case in cases}


CASES = _cases()


def _extra_cases() -> dict[str, Case]:
    heartbeat = bytes.fromhex("000000000000a8263501")
    cases = (
        Case(
            "heartbeat",
            0x0004,
            0x31000001,
            {},
            heartbeat,
            case_id="compressed",
            wire_payload=zlib.compress(heartbeat),
        ),
        Case(
            "heartbeat",
            0x0004,
            0x31000002,
            {},
            b"",
            case_id="bad_compression",
            wire_payload=bytes.fromhex("789c00"),
            decoded_length=10,
        ),
        Case(
            "heartbeat",
            0x0004,
            0x31000003,
            {},
            heartbeat,
            case_id="stale_message",
            response_message_id=0x31000004,
        ),
        Case(
            "security_list",
            0x044D,
            0x31000004,
            {"market": "sh", "start": 0, "limit": 1600},
            b"\x00\x00",
            case_id="sh_empty",
        ),
        Case(
            "security_list",
            0x044D,
            0x31000005,
            {"market": "bj", "start": 0, "limit": 1600},
            b"\x00\x00",
            case_id="bj_empty",
        ),
        Case(
            "snapshots",
            0x054C,
            0x31000006,
            {"codes": ["sh510300", "sh000001"]},
            b"\x00\x00\x00\x00",
            case_id="etf_index_empty",
        ),
        Case(
            "snapshots",
            0x054C,
            0x31000007,
            {"codes": ["sz000001"]},
            b"\x00\x00\x01\x00\x00\x30\x30\x30",
            case_id="truncated_record",
        ),
        Case(
            "category_quotes",
            0x054B,
            0x31000008,
            {"category": 6, "sort_by": None, "start": 0, "count": 80, "ascending": False},
            b"\x00\x00\x00\x00\xff",
            case_id="trailing_byte",
        ),
        Case(
            "klines",
            0x052D,
            0x31000009,
            {"code": "sz300308", "period": "day", "start": 0, "count": 800, "include_raw": True},
            b"\x00\x00",
            case_id="max_page_empty",
        ),
        Case(
            "klines",
            0x052D,
            0x3100000A,
            {"code": "sz300308", "period": "day", "start": 0, "count": 420, "include_raw": False},
            b"\x00\x00",
            case_id="include_raw_false",
        ),
        Case(
            "recent_intraday",
            0x0FEB,
            0x3100000B,
            {"code": "sz300308", "trading_date": 20260511, "include_raw": False},
            b"\x00\x00" + struct.pack("<ff", 10.0, 10.1),
            case_id="include_raw_false",
        ),
        Case(
            "file_content",
            0x06B9,
            0x3100000C,
            {"path": "zhb.zip", "offset": 0, "size": 60000},
            b"\x00\x00\x00\x00",
            case_id="max_chunk_empty",
        ),
    )
    return {f"{case.command}/{case.case_id}": case for case in cases}


EXTRA_CASES = _extra_cases()
SOURCE_CASES = {f"{command}/normal": case for command, case in CASES.items()} | EXTRA_CASES


def _canonical(value: Any) -> dict[str, Any]:
    if value is None:
        return {"$type": "none"}
    if isinstance(value, bool):
        return {"$type": "bool", "value": value}
    if isinstance(value, int):
        return {"$type": "int", "value": str(value)}
    if isinstance(value, str):
        return {"$type": "str", "value": value}
    if isinstance(value, bytes):
        return {"$type": "bytes", "hex": value.hex()}
    if isinstance(value, tuple):
        return {"$type": "tuple", "items": [_canonical(item) for item in value]}
    if isinstance(value, list):
        return {"$type": "list", "items": [_canonical(item) for item in value]}
    if isinstance(value, dict):
        return {
            "$type": "dict",
            "items": [[_canonical(key), _canonical(item)] for key, item in value.items()],
        }
    raise TypeError(f"unsupported static request value: {type(value).__name__}")


def _response_frame(case: Case) -> bytes:
    wire_payload = case.wire_payload if case.wire_payload is not None else case.response_payload
    decoded_length = (
        case.decoded_length if case.decoded_length is not None else len(case.response_payload)
    )
    if len(wire_payload) > 0xFFFF or decoded_length > 0xFFFF:
        raise ValueError(f"fixture payload exceeds uint16 response length: {case.command}")
    message_id = (
        case.response_message_id
        if case.response_message_id is not None
        else case.message_id
    )
    return (
        b"\xb1\xcb\x74\x00"
        + b"\x00"
        + message_id.to_bytes(4, "little")
        + b"\x00"
        + case.code.to_bytes(2, "little")
        + len(wire_payload).to_bytes(2, "little")
        + decoded_length.to_bytes(2, "little")
        + wire_payload
    )


def materialize_case(case_key: str, fixture_root: Path = DEFAULT_ROOT) -> Path:
    case = SOURCE_CASES[case_key]
    fixture_root = fixture_root.resolve()
    target = fixture_root / case.command / case.case_id
    if target.exists():
        raise FileExistsError(f"refusing to overwrite fixture case: {target}")
    target.parent.mkdir(parents=True, exist_ok=True)
    temporary = target.with_name(f".{target.name}.tmp-{os.getpid()}")
    if temporary.exists():
        raise FileExistsError(f"stale fixture temporary directory exists: {temporary}")
    temporary.mkdir()
    request = _canonical(case.request)
    metadata = {
        "schema_version": 1,
        "golden_schema_version": 1,
        "registry_key": case.command,
        "command_code": case.code,
        "message_id": case.message_id,
        "request_context": request,
        "push_generation": case.push_generation,
        "push_host": case.push_host,
        "expected_exception": None,
    }
    try:
        (temporary / "request.json").write_text(
            json.dumps(request, ensure_ascii=True, indent=2) + "\n",
            encoding="utf-8",
        )
        (temporary / "response.bin").write_bytes(_response_frame(case))
        (temporary / "metadata.json").write_text(
            json.dumps(metadata, ensure_ascii=True, indent=2) + "\n",
            encoding="utf-8",
        )
        temporary.replace(target)
    except BaseException:
        shutil.rmtree(temporary, ignore_errors=True)
        raise
    return target


def materialize(command: str, fixture_root: Path = DEFAULT_ROOT) -> Path:
    return materialize_case(f"{command}/normal", fixture_root)


def main() -> int:
    parser = argparse.ArgumentParser(description="Materialize one reviewed 7709 fixture case")
    choices = parser.add_mutually_exclusive_group(required=True)
    choices.add_argument("--command", choices=sorted(CASES))
    choices.add_argument("--case", choices=sorted(EXTRA_CASES))
    args = parser.parse_args()
    case_key = args.case if args.case is not None else f"{args.command}/normal"
    print(materialize_case(case_key))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
