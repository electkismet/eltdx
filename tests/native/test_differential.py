"""Exact v2.0.5/Python to 3.0/Rust differential fixture tests."""

from __future__ import annotations

import os
from pathlib import Path

import pytest

from eltdx.protocol.commands import COMMANDS
from scripts.fixtures.differential import (
    DifferentialCase,
    applicable_override,
    assert_exact,
    assert_error_case,
    assert_parse_case,
    assert_raw_precision_case,
    assert_request_case,
    discover_cases,
    load_overrides,
    raw_precision_projection,
    target_expected,
    target_request_bytes,
)
from scripts.fixtures.export_v205_baseline import to_canonical


ROOT = Path(__file__).parents[2]
FIXTURES_ROOT = Path(
    os.environ.get("ELTDX_FIXTURES_ROOT", ROOT / "tests" / "fixtures" / "7709")
)
OVERRIDES_PATH = ROOT / "tests" / "contracts" / "manifests" / "differential_overrides.json"


def _cases() -> list[DifferentialCase]:
    return discover_cases(FIXTURES_ROOT)


ALL_CASES = _cases()


def test_differential_matrix_covers_all_21_commands() -> None:
    cases = ALL_CASES
    assert len(COMMANDS) == 21
    assert {case.command for case in cases} == set(COMMANDS)
    assert len({case.case_id for case in cases}) == len(cases)
    assert all(case.command_code == COMMANDS[case.command].code for case in cases)
    assert len({case.metadata["baseline_wheel_sha256"] for case in cases}) == 1
    assert SUCCESS_CASES
    assert ERROR_CASES
    assert RAW_PRECISION_CASES


REQUEST_CASES = [
    case
    for case in ALL_CASES
    if case.expected_exception is None or case.expected_exception.get("phase") != "build"
]
SUCCESS_CASES = [case for case in ALL_CASES if case.expected_exception is None]
ERROR_CASES = [case for case in ALL_CASES if case.expected_exception is not None]
RAW_PRECISION_CASES = [
    case for case in SUCCESS_CASES if raw_precision_projection(case.expected)
]


@pytest.mark.parametrize("case", REQUEST_CASES, ids=lambda case: case.case_id)
def test_native_request_frames_match_v205_fixture(case: DifferentialCase) -> None:
    assert_request_case(case, load_overrides(OVERRIDES_PATH))


@pytest.mark.parametrize("case", SUCCESS_CASES, ids=lambda case: case.case_id)
def test_native_parse_results_match_v205_fixture(case: DifferentialCase) -> None:
    assert_parse_case(case, load_overrides(OVERRIDES_PATH))


@pytest.mark.parametrize("case", ERROR_CASES, ids=lambda case: case.case_id)
def test_native_errors_match_v205_fixture(case: DifferentialCase) -> None:
    assert_error_case(case, load_overrides(OVERRIDES_PATH))


@pytest.mark.parametrize("case", RAW_PRECISION_CASES, ids=lambda case: case.case_id)
def test_native_raw_and_precision_match_v205_fixture(case: DifferentialCase) -> None:
    assert_raw_precision_case(case, load_overrides(OVERRIDES_PATH))


def _tick_case(command: str, baseline_default: int) -> DifferentialCase:
    request = b"header" + baseline_default.to_bytes(2, "little")
    expected = {
        "$type": "dataclass",
        "module": "eltdx.models.trade",
        "qualname": "TradePage",
        "fields": [["request_count", to_canonical(baseline_default)]],
    }
    return DifferentialCase(
        root=Path(command) / "omitted_count",
        case_id=f"{command}/omitted_count",
        command=command,
        command_code=0,
        message_id=1,
        request_payload={"code": "sz000001"},
        request_bytes=request,
        response_bytes=b"",
        expected=expected,
        expected_exception=None,
        metadata={},
    )


@pytest.mark.parametrize(
    ("command", "baseline_default"),
    [("today_ticks", 115), ("historical_ticks", 900)],
)
def test_declared_tick_default_override_is_narrow_and_exact(
    command: str,
    baseline_default: int,
) -> None:
    overrides = load_overrides(OVERRIDES_PATH)
    case = _tick_case(command, baseline_default)
    override = applicable_override(case, overrides)
    assert override is not None
    assert target_request_bytes(case, override) == b"header" + (1800).to_bytes(2, "little")
    expected = target_expected(case, override)
    assert expected["fields"] == [["request_count", to_canonical(1800)]]

    explicit = DifferentialCase(
        root=case.root,
        case_id=case.case_id,
        command=case.command,
        command_code=case.command_code,
        message_id=case.message_id,
        request_payload={**case.request_payload, "count": baseline_default},
        request_bytes=case.request_bytes,
        response_bytes=case.response_bytes,
        expected=case.expected,
        expected_exception=case.expected_exception,
        metadata=case.metadata,
    )
    assert applicable_override(explicit, overrides) is None


def test_exact_comparison_rejects_float_bit_and_container_drift() -> None:
    with pytest.raises(AssertionError, match="f64_bits"):
        assert_exact(
            {"$type": "float", "f64_bits": "0000000000000000", "readable_hex": "0x0.0p+0"},
            {"$type": "float", "f64_bits": "8000000000000000", "readable_hex": "-0x0.0p+0"},
            label="signed zero",
        )
    with pytest.raises(AssertionError, match="type"):
        assert_exact(
            {"$type": "tuple", "items": []},
            {"$type": "list", "items": []},
            label="container",
        )
