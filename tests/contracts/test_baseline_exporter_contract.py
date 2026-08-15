"""Contracts for the deferred v2.0.5 baseline exporter."""

from __future__ import annotations

import json
import math
import struct
import sys
import types
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
    export_fixture_case,
    from_canonical,
    to_canonical,
)
from scripts.fixtures.differential import (
    DifferentialCase,
    assert_request_case,
    parse_actual,
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


def _protocol_probe(monkeypatch: pytest.MonkeyPatch, calls: list[tuple[str, int]]) -> None:
    module = types.ModuleType("eltdx.protocol")

    class Frame:
        control = 0x01
        msg_id = 7
        msg_type = 4
        data = b"payload"
        raw = b"response"

        def to_bytes(self) -> bytes:
            return b"request"

    def build(command: int, _payload: object, _message_id: int) -> Frame:
        calls.append(("build", command))
        return Frame()

    def decode(_raw: bytes) -> Frame:
        return Frame()

    def parse(command: int, _response: object, _payload: object) -> int:
        calls.append(("parse", command))
        return 1

    module.build_command_frame = build  # type: ignore[attr-defined]
    module.decode_response = decode  # type: ignore[attr-defined]
    module.parse_command_response = parse  # type: ignore[attr-defined]
    monkeypatch.setitem(sys.modules, "eltdx.protocol", module)


def test_fixture_export_uses_numeric_command_code(
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
) -> None:
    calls: list[tuple[str, int]] = []
    _protocol_probe(monkeypatch, calls)
    case = tmp_path / "heartbeat" / "normal"
    case.mkdir(parents=True)
    (case / "request.json").write_text(
        json.dumps(to_canonical({})),
        encoding="utf-8",
    )
    (case / "response.bin").write_bytes(b"response")
    (case / "metadata.json").write_text(
        json.dumps(
            {
                "registry_key": "heartbeat",
                "command_code": 4,
                "message_id": 7,
            }
        ),
        encoding="utf-8",
    )

    export_fixture_case(case, {"wheel_sha256": "0" * 64}, force=False)

    assert calls == [("build", 4), ("parse", 4)]
    assert json.loads((case / "metadata.json").read_text(encoding="utf-8"))[
        "expected_exception"
    ] is None


def test_differential_uses_numeric_command_code(monkeypatch: pytest.MonkeyPatch) -> None:
    calls: list[tuple[str, int]] = []
    _protocol_probe(monkeypatch, calls)
    case = DifferentialCase(
        root=Path("heartbeat/normal"),
        case_id="heartbeat/normal",
        command="heartbeat",
        command_code=4,
        message_id=7,
        request_payload={},
        request_bytes=b"request",
        response_bytes=b"response",
        expected=to_canonical(1),
        expected_exception=None,
        metadata={
            "frame_header": to_canonical(
                {
                    "control": 0x01,
                    "message_id": 7,
                    "message_type": 4,
                    "zip_length": 9,
                    "length": 9,
                }
            )
        },
    )

    assert_request_case(case, {})
    assert parse_actual(case) == to_canonical(1)
    assert calls == [("build", 4), ("parse", 4)]


def test_baseline_identity_is_immutable() -> None:
    assert BASELINE_TAG == "v2.0.5"
    assert BASELINE_VERSION == "2.0.5"
    assert BASELINE_COMMIT == "6486a1692dd4aca5339001b2de22e88bb29e16ec"


def test_canonical_json_is_schema_friendly() -> None:
    payload = to_canonical({"nan": float("nan"), "missing": MISSING_VALUE})
    serialized = json.dumps(payload, allow_nan=False)
    assert "NaN" not in serialized
