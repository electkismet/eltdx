"""Exact Python/Rust differential runner for generated 7709 fixtures."""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from scripts.fixtures.canonical import (
    canonical_exception,
    frame_header,
    from_canonical,
    to_canonical,
)


REQUIRED_CASE_FILES = frozenset(
    {"request.json", "request.bin", "response.bin", "expected.json", "metadata.json"}
)


@dataclass(frozen=True, slots=True)
class DifferentialCase:
    root: Path
    case_id: str
    command: str
    command_code: int
    message_id: int
    request_payload: dict[str, Any]
    request_bytes: bytes
    response_bytes: bytes
    expected: dict[str, Any]
    expected_exception: dict[str, Any] | None
    metadata: dict[str, Any]


def _load_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def discover_cases(fixtures_root: Path) -> list[DifferentialCase]:
    cases = []
    for metadata_path in sorted(fixtures_root.glob("*/*/metadata.json")):
        case_root = metadata_path.parent
        if case_root.parent.name == "baseline":
            continue
        present = {path.name for path in case_root.iterdir() if path.is_file()}
        missing = REQUIRED_CASE_FILES - present
        if missing:
            raise FileNotFoundError(
                f"incomplete differential case {case_root}: missing {', '.join(sorted(missing))}"
            )
        metadata = _load_json(metadata_path)
        _validate_metadata(metadata, case_root)
        payload = from_canonical(_load_json(case_root / "request.json"))
        if not isinstance(payload, dict):
            raise TypeError(f"differential request must decode to dict: {case_root}")
        cases.append(
            DifferentialCase(
                root=case_root,
                case_id=case_root.relative_to(fixtures_root).as_posix(),
                command=metadata["registry_key"],
                command_code=int(metadata["command_code"]),
                message_id=int(metadata["message_id"]),
                request_payload=payload,
                request_bytes=(case_root / "request.bin").read_bytes(),
                response_bytes=(case_root / "response.bin").read_bytes(),
                expected=_load_json(case_root / "expected.json"),
                expected_exception=metadata["expected_exception"],
                metadata=metadata,
            )
        )
    return cases


def _validate_metadata(metadata: dict[str, Any], case_root: Path) -> None:
    if metadata.get("schema_version") != 1:
        raise ValueError(f"unsupported fixture schema in {case_root}")
    if metadata.get("golden_schema_version") != 1:
        raise ValueError(f"unsupported golden schema in {case_root}")
    message_id = metadata.get("message_id")
    if not isinstance(message_id, int) or isinstance(message_id, bool) or not 1 <= message_id <= 0xFFFFFFFF:
        raise ValueError(f"fixture message id must be a fixed nonzero uint32 in {case_root}")


def first_difference(expected: Any, actual: Any, path: str = "$") -> str | None:
    if type(expected) is not type(actual):
        return f"{path}: type {type(expected).__name__} != {type(actual).__name__}"
    if isinstance(expected, dict):
        if set(expected) != set(actual):
            missing = sorted(set(expected) - set(actual))
            extra = sorted(set(actual) - set(expected))
            return f"{path}: keys differ; missing={missing!r}, extra={extra!r}"
        for key in expected:
            difference = first_difference(expected[key], actual[key], f"{path}.{key}")
            if difference is not None:
                return difference
        return None
    if isinstance(expected, list):
        if len(expected) != len(actual):
            return f"{path}: length {len(expected)} != {len(actual)}"
        for index, (expected_item, actual_item) in enumerate(zip(expected, actual, strict=True)):
            difference = first_difference(expected_item, actual_item, f"{path}[{index}]")
            if difference is not None:
                return difference
        return None
    if expected != actual:
        return f"{path}: {expected!r} != {actual!r}"
    return None


def assert_exact(expected: Any, actual: Any, *, label: str) -> None:
    difference = first_difference(expected, actual)
    if difference is not None:
        raise AssertionError(f"{label} differs: {difference}")


def _assert_bytes(expected: bytes, actual: bytes, *, label: str) -> None:
    if expected == actual:
        return
    maximum = min(len(expected), len(actual))
    offset = next((index for index in range(maximum) if expected[index] != actual[index]), maximum)
    raise AssertionError(
        f"{label} differs at byte {offset}: expected_len={len(expected)}, actual_len={len(actual)}, "
        f"expected={expected[offset:offset + 16].hex()}, actual={actual[offset:offset + 16].hex()}"
    )


def assert_request_case(case: DifferentialCase) -> None:
    from eltdx.protocol import build_command_frame

    frame = build_command_frame(case.command_code, dict(case.request_payload), case.message_id)
    _assert_bytes(
        case.request_bytes,
        frame.to_bytes(),
        label=f"{case.case_id} request frame",
    )
    assert_exact(
        case.metadata["frame_header"],
        to_canonical(frame_header(frame)),
        label=f"{case.case_id} request header",
    )


def parse_actual(case: DifferentialCase) -> dict[str, Any]:
    from eltdx.protocol import decode_response, parse_command_response

    response = decode_response(case.response_bytes)
    if response.msg_id != case.message_id:
        raise ValueError(
            f"response message id {response.msg_id} does not match fixed fixture id {case.message_id}"
        )
    if response.msg_type != case.command_code:
        raise ValueError(
            f"response message type {response.msg_type} does not match command {case.command_code}"
        )
    parsed = parse_command_response(case.command_code, response, dict(case.request_payload))
    return to_canonical(parsed)


def assert_parse_case(case: DifferentialCase) -> dict[str, Any]:
    actual = parse_actual(case)
    assert_exact(case.expected, actual, label=f"{case.case_id} parsed value")
    return actual


def assert_error_case(case: DifferentialCase) -> None:
    from eltdx.protocol import build_command_frame

    expected_exception = case.expected_exception
    if expected_exception is None:
        raise AssertionError(f"fixture has no expected exception: {case.case_id}")
    phase = expected_exception.get("phase")
    try:
        if phase == "build":
            build_command_frame(case.command_code, dict(case.request_payload), case.message_id)
        elif phase == "parse":
            assert_request_case(case)
            parse_actual(case)
        else:
            raise AssertionError(f"unsupported expected exception phase {phase!r} for {case.case_id}")
    except Exception as error:
        actual_exception = canonical_exception(error, phase=phase)
        assert_exact(expected_exception, actual_exception, label=f"{case.case_id} {phase} exception")
        return
    raise AssertionError(f"expected {phase} exception was not raised for {case.case_id}")


def raw_precision_projection(value: Any, path: str = "$") -> list[tuple[str, Any]]:
    result: list[tuple[str, Any]] = []
    if isinstance(value, dict):
        kind = value.get("$type")
        if kind in {"float", "bytes", "date", "datetime"}:
            result.append((path, value))
            return result
        if kind == "dataclass":
            for name, item in value["fields"]:
                child_path = f"{path}.{name}"
                nested = raw_precision_projection(item, child_path)
                if "raw" in name and not nested:
                    result.append((child_path, item))
                result.extend(nested)
            return result
        for key, item in value.items():
            result.extend(raw_precision_projection(item, f"{path}.{key}"))
    elif isinstance(value, list):
        for index, item in enumerate(value):
            result.extend(raw_precision_projection(item, f"{path}[{index}]"))
    return result


def assert_raw_precision_case(case: DifferentialCase) -> None:
    expected = raw_precision_projection(case.expected)
    actual = raw_precision_projection(parse_actual(case))
    if not expected:
        raise AssertionError(f"fixture has no raw/precision values: {case.case_id}")
    assert_exact(expected, actual, label=f"{case.case_id} raw/precision projection")


def run_case(case: DifferentialCase) -> None:
    if case.expected_exception is not None:
        assert_error_case(case)
        return
    assert_request_case(case)
    assert_parse_case(case)
