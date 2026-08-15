from __future__ import annotations

import json
import zlib
from pathlib import Path

import pytest

from scripts.fixtures.materialize_static_inputs import (
    CASES,
    EXTRA_CASES,
    REQUIRED_FILES,
    SOURCE_CASES,
    materialize,
    materialize_case,
)


ROOT = Path(__file__).parents[2]
FIXTURE_ROOT = ROOT / "tests" / "fixtures" / "7709"
EXPECTED_CODES = {
    "heartbeat": 0x0004,
    "handshake": 0x000D,
    "capital_changes": 0x000F,
    "finance_batch": 0x0010,
    "security_list": 0x044D,
    "security_count": 0x044E,
    "special_limits": 0x0452,
    "intraday_aux": 0x051B,
    "klines": 0x052D,
    "today_intraday": 0x0537,
    "legacy_quotes": 0x053E,
    "refresh_stream": 0x0547,
    "category_quotes": 0x054B,
    "snapshots": 0x054C,
    "auction_series": 0x056A,
    "file_content": 0x06B9,
    "historical_intraday": 0x0FB4,
    "today_ticks": 0x0FC5,
    "historical_ticks": 0x0FC6,
    "sparkline": 0x0FD1,
    "recent_intraday": 0x0FEB,
}
EXPECTED_EXTRA_CASES = {
    "heartbeat/compressed",
    "heartbeat/bad_compression",
    "heartbeat/stale_message",
    "security_list/sh_empty",
    "security_list/bj_empty",
    "snapshots/etf_index_empty",
    "snapshots/truncated_record",
    "category_quotes/trailing_byte",
    "klines/max_page_empty",
    "klines/include_raw_false",
    "recent_intraday/include_raw_false",
    "file_content/max_chunk_empty",
}


def test_static_input_definitions_cover_all_21_commands() -> None:
    assert {name: case.code for name, case in CASES.items()} == EXPECTED_CODES
    assert len({case.message_id for case in CASES.values()}) == 21
    assert all(case.response_payload for case in CASES.values())
    assert set(EXTRA_CASES) == EXPECTED_EXTRA_CASES
    normal_cases = {f"{command}/normal" for command in EXPECTED_CODES}
    assert set(SOURCE_CASES) == normal_cases | EXPECTED_EXTRA_CASES
    assert len({case.message_id for case in SOURCE_CASES.values()}) == len(SOURCE_CASES)
    assert all(
        case_key == f"{case.command}/{case.case_id}"
        for case_key, case in SOURCE_CASES.items()
    )


def test_supplemental_response_identities_and_compression_seeds() -> None:
    response_identities = {
        (
            case.response_message_id
            if case.response_message_id is not None
            else case.message_id,
            case.code,
        )
        for case in SOURCE_CASES.values()
    }
    assert len(response_identities) == len(SOURCE_CASES)

    compressed = EXTRA_CASES["heartbeat/compressed"]
    assert compressed.wire_payload is not None
    assert compressed.wire_payload != compressed.response_payload
    assert zlib.decompress(compressed.wire_payload) == compressed.response_payload

    malformed = EXTRA_CASES["heartbeat/bad_compression"]
    assert malformed.wire_payload is not None
    assert malformed.decoded_length == len(compressed.response_payload)
    with pytest.raises(zlib.error):
        zlib.decompress(malformed.wire_payload)

    stale = EXTRA_CASES["heartbeat/stale_message"]
    assert stale.response_message_id == 0x31000004
    assert stale.response_message_id != stale.message_id


def test_normal_materializer_wrapper_keeps_normal_target(tmp_path: Path) -> None:
    target = materialize("heartbeat", tmp_path)
    assert target == tmp_path.resolve() / "heartbeat" / "normal"


@pytest.mark.parametrize("case_key", sorted(SOURCE_CASES))
def test_materializer_writes_one_identity_bound_case(tmp_path: Path, case_key: str) -> None:
    case = SOURCE_CASES[case_key]
    target = materialize_case(case_key, tmp_path)
    assert target == tmp_path.resolve() / case.command / case.case_id
    assert {path.name for path in target.iterdir()} == REQUIRED_FILES
    request = json.loads((target / "request.json").read_text(encoding="utf-8"))
    metadata = json.loads((target / "metadata.json").read_text(encoding="utf-8"))
    response = (target / "response.bin").read_bytes()
    assert request["$type"] == "dict"
    assert metadata["registry_key"] == case.command
    assert metadata["command_code"] == case.code
    assert metadata["message_id"] == case.message_id
    assert response[:4] == b"\xb1\xcb\x74\x00"
    expected_message = (
        case.response_message_id
        if case.response_message_id is not None
        else case.message_id
    )
    expected_wire = case.wire_payload if case.wire_payload is not None else case.response_payload
    expected_decoded = (
        case.decoded_length if case.decoded_length is not None else len(case.response_payload)
    )
    assert int.from_bytes(response[5:9], "little") == expected_message
    assert int.from_bytes(response[10:12], "little") == case.code
    assert int.from_bytes(response[12:14], "little") == len(expected_wire)
    assert int.from_bytes(response[14:16], "little") == expected_decoded
    assert len(response) == 16 + len(expected_wire)
    assert response[16:] == expected_wire
    assert "baseline_wheel_sha256" not in metadata
    assert "frame_header" not in metadata
    with pytest.raises(FileExistsError):
        materialize_case(case_key, tmp_path)


def test_repository_fixture_source_inventory_is_complete() -> None:
    directories = {path.name for path in FIXTURE_ROOT.iterdir() if path.is_dir()}
    assert directories == set(EXPECTED_CODES)
    for case_key, case in SOURCE_CASES.items():
        case_root = FIXTURE_ROOT / case.command / case.case_id
        assert {path.name for path in case_root.iterdir()} == REQUIRED_FILES
    forbidden = {
        path.relative_to(FIXTURE_ROOT).as_posix()
        for path in FIXTURE_ROOT.rglob("*")
        if path.is_file() and path.name in {"request.bin", "expected.json"}
    }
    assert not forbidden
