"""Contracts for the deferred v2.0.5 baseline exporter."""

from __future__ import annotations

import json
import math
import struct
from dataclasses import dataclass
from datetime import date, datetime, timezone
from pathlib import Path

import pytest

from scripts.fixtures.export_v205_baseline import (
    BASELINE_COMMIT,
    BASELINE_TAG,
    BASELINE_VERSION,
    MISSING_VALUE,
    _fixture_cases,
    annotation_shape,
    canonical_exception,
    from_canonical,
    to_canonical,
)


@dataclass(frozen=True, slots=True)
class Sample:
    value: tuple[int, bytes]


@dataclass(frozen=True, slots=True)
class FloatSample:
    price_raw_float: float
    public_price: float


@pytest.mark.parametrize(
    "value",
    [
        None,
        True,
        -123,
        "深市",
        b"\x00\xff",
        date(2026, 8, 14),
        datetime(2026, 8, 14, 9, 25),
        datetime(2026, 8, 14, 9, 25, tzinfo=timezone.utc, fold=1),
        (1, "two"),
        [1, None],
        {"code": "sz000001", "count": 1800},
    ],
)
def test_request_canonical_roundtrip(value: object) -> None:
    assert from_canonical(to_canonical(value)) == value


@pytest.mark.parametrize(
    "value",
    [0.0, -0.0, 1.0, math.inf, -math.inf, float("nan")],
)
def test_float_canonical_form_preserves_all_f64_bits(value: float) -> None:
    canonical = to_canonical(value)
    reconstructed = from_canonical(canonical)
    assert canonical["f64_bits"] == struct.pack(">d", value).hex()
    assert struct.pack(">d", reconstructed) == struct.pack(">d", value)


def test_raw_f32_fields_record_wire_bits_without_changing_public_f64() -> None:
    canonical = to_canonical(FloatSample(1.25, 1.25))
    raw_value = canonical["fields"][0][1]
    public_value = canonical["fields"][1][1]
    assert raw_value["wire_f32_bits"] == struct.pack(">f", 1.25).hex()
    assert "wire_f32_bits" not in public_value


def test_dataclass_canonical_form_preserves_field_order_and_container_types() -> None:
    canonical = to_canonical(Sample((1, b"\x02")))
    assert canonical == {
        "$type": "dataclass",
        "module": __name__,
        "qualname": "Sample",
        "fields": [
            [
                "value",
                {
                    "$type": "tuple",
                    "items": [
                        {"$type": "int", "value": "1"},
                        {"$type": "bytes", "hex": "02"},
                    ],
                },
            ]
        ],
    }


def test_missing_and_none_are_distinct() -> None:
    assert to_canonical(MISSING_VALUE) == {"$type": "missing"}
    assert to_canonical(None) == {"$type": "none"}


def test_exception_snapshot_preserves_type_message_context_and_cause() -> None:
    try:
        try:
            raise ValueError("inner")
        except ValueError as cause:
            error = RuntimeError("outer")
            error.context = {"code": "sz000001"}  # type: ignore[attr-defined]
            raise error from cause
    except RuntimeError as captured:
        snapshot = canonical_exception(captured, phase="parse")

    assert snapshot["phase"] == "parse"
    assert snapshot["type"] == "builtins:RuntimeError"
    assert snapshot["message"] == "outer"
    assert from_canonical(snapshot["context"]) == {"code": "sz000001"}
    assert snapshot["cause"]["type"] == "builtins:ValueError"
    assert snapshot["cause"]["message"] == "inner"


def test_annotation_shape_normalizes_union_and_generics() -> None:
    assert annotation_shape(list[str | None]) == {
        "kind": "generic",
        "origin": {"kind": "type", "path": "builtins:list"},
        "args": [
            {
                "kind": "union",
                "args": [
                    {"kind": "type", "path": "builtins:str"},
                    {"kind": "type", "path": "builtins:None"},
                ],
            }
        ],
    }


def test_fixture_discovery_is_command_case_ordered(tmp_path: Path) -> None:
    for relative in ("today_ticks/normal", "handshake/compressed"):
        case = tmp_path / relative
        case.mkdir(parents=True)
        (case / "metadata.json").write_text("{}", encoding="utf-8")
    (tmp_path / "baseline" / "v2.0.5").mkdir(parents=True)
    (tmp_path / "baseline" / "v2.0.5" / "metadata.json").write_text("{}", encoding="utf-8")

    assert [path.relative_to(tmp_path).as_posix() for path in _fixture_cases(tmp_path)] == [
        "handshake/compressed",
        "today_ticks/normal",
    ]


def test_baseline_identity_is_immutable() -> None:
    assert BASELINE_TAG == "v2.0.5"
    assert BASELINE_VERSION == "2.0.5"
    assert BASELINE_COMMIT == "6486a1692dd4aca5339001b2de22e88bb29e16ec"


def test_canonical_json_is_schema_friendly() -> None:
    payload = to_canonical({"nan": float("nan"), "missing": MISSING_VALUE})
    serialized = json.dumps(payload, allow_nan=False)
    assert "NaN" not in serialized
