"""Current dataclass, exception, and serialization invariants."""

from __future__ import annotations

import json
from dataclasses import is_dataclass
from datetime import date, datetime
from pathlib import Path
from eltdx.serialization import to_json, to_jsonable


def test_current_client_is_a_slots_dataclass() -> None:
    from eltdx import TdxClient

    assert is_dataclass(TdxClient)
    assert "money_flow" in TdxClient.__annotations__
    assert "helpers" in TdxClient.__annotations__
    assert tuple(TdxClient.__slots__)


def test_current_exception_hierarchy_is_stable() -> None:
    from eltdx.exceptions import EltdxError, ResponseTimeoutError

    assert issubclass(ResponseTimeoutError, EltdxError)


def test_serialization_contract_preserves_current_shapes() -> None:
    value = {
        "date": date(2026, 8, 14),
        "datetime": datetime(2026, 8, 14, 9, 25),
        "bytes": b"\x00\xff",
        "path": Path("docs/index.md"),
        "tuple": (1, None),
        "set": {2},
    }
    expected = {
        "date": "2026-08-14",
        "datetime": "2026-08-14T09:25:00",
        "bytes": "00ff",
        "path": str(Path("docs/index.md")),
        "tuple": [1, None],
        "set": [2],
    }
    assert to_jsonable(value) == expected
    assert to_json(value) == json.dumps(expected, ensure_ascii=False)
