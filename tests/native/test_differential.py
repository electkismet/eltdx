"""Exact current protocol golden fixture tests."""

from __future__ import annotations

import os
from pathlib import Path

import pytest

from eltdx.protocol.commands import COMMANDS
from scripts.fixtures.differential import (
    DifferentialCase,
    assert_exact,
    assert_error_case,
    assert_parse_case,
    assert_raw_precision_case,
    assert_request_case,
    discover_cases,
    raw_precision_projection,
)
ROOT = Path(__file__).parents[2]
FIXTURES_ROOT = Path(
    os.environ.get("ELTDX_FIXTURES_ROOT", ROOT / "tests" / "fixtures" / "7709")
)


def _cases() -> list[DifferentialCase]:
    return discover_cases(FIXTURES_ROOT)


ALL_CASES = _cases()


def test_differential_matrix_covers_all_21_commands() -> None:
    cases = ALL_CASES
    assert len(COMMANDS) == 21
    assert {case.command for case in cases} == set(COMMANDS)
    assert len({case.case_id for case in cases}) == len(cases)
    assert all(case.command_code == COMMANDS[case.command].code for case in cases)
    assert all(case.metadata["golden_schema_version"] == 1 for case in cases)
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
def test_native_request_frames_match_golden(case: DifferentialCase) -> None:
    assert_request_case(case)


@pytest.mark.parametrize("case", SUCCESS_CASES, ids=lambda case: case.case_id)
def test_native_parse_results_match_golden(case: DifferentialCase) -> None:
    assert_parse_case(case)


@pytest.mark.parametrize("case", ERROR_CASES, ids=lambda case: case.case_id)
def test_native_errors_match_golden(case: DifferentialCase) -> None:
    assert_error_case(case)


@pytest.mark.parametrize("case", RAW_PRECISION_CASES, ids=lambda case: case.case_id)
def test_native_raw_and_precision_match_golden(case: DifferentialCase) -> None:
    assert_raw_precision_case(case)


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
