"""Dataclass, exception, and serialization contracts frozen at v2.0.5."""

from __future__ import annotations

import importlib
import json
from dataclasses import MISSING, fields, is_dataclass
from datetime import date, datetime
from pathlib import Path
from typing import Any

from eltdx.serialization import to_json, to_jsonable


MANIFEST = Path(__file__).with_name("manifests") / "dataclasses_exceptions.json"


def _load_manifest() -> dict[str, Any]:
    return json.loads(MANIFEST.read_text(encoding="utf-8"))


def _resolve(path: str) -> Any:
    module_name, qualname = path.split(":", 1)
    value: Any = importlib.import_module(module_name)
    for part in qualname.split("."):
        value = getattr(value, part)
    return value


def _default_value(value: Any) -> Any:
    if isinstance(value, bytes):
        return {"$type": "bytes", "hex": value.hex()}
    return value


def _declared_properties(cls: type[Any]) -> list[str]:
    return sorted(name for name, value in vars(cls).items() if isinstance(value, property))


def test_dataclass_shape_matches_v205() -> None:
    manifest = _load_manifest()
    for path, contract in manifest["dataclasses"].items():
        cls = _resolve(path)
        assert is_dataclass(cls), path
        declared_fields = list(fields(cls))
        expected_fields = contract["fields"]
        assert [field.name for field in declared_fields] == expected_fields, path
        assert tuple(cls.__slots__) == tuple(expected_fields), path
        assert cls.__module__ == path.split(":", 1)[0], path

        params = cls.__dataclass_params__
        assert params.frozen is contract.get("frozen", True), path
        assert params.eq is True, path
        assert params.repr is True, path

        defaults = {
            field.name: _default_value(field.default)
            for field in declared_fields
            if field.default is not MISSING
        }
        assert defaults == contract.get("defaults", {}), path
        assert [field.name for field in declared_fields if not field.repr] == contract.get(
            "repr_false_fields", []
        ), path
        assert _declared_properties(cls) == contract.get("properties", []), path


def test_dataclass_fields_remain_annotated() -> None:
    manifest = _load_manifest()
    for path, contract in manifest["dataclasses"].items():
        cls = _resolve(path)
        assert list(cls.__annotations__) == contract["fields"], path
        assert all(annotation is not None for annotation in cls.__annotations__.values()), path


def test_exception_direct_bases_match_v205() -> None:
    manifest = _load_manifest()
    for path, expected_bases in manifest["exceptions"].items():
        exception_type = _resolve(path)
        assert [f"{base.__module__}:{base.__qualname__}" for base in exception_type.__bases__] == expected_bases


def test_serialization_contract_preserves_v205_shapes() -> None:
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
        "path": "docs/index.md",
        "tuple": [1, None],
        "set": [2],
    }
    assert to_jsonable(value) == expected
    assert to_json(value) == json.dumps(expected, ensure_ascii=False)
