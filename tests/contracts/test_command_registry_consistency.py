"""Contracts for migration counts, manifest records, and Python API baseline reflection."""
from __future__ import annotations

import dataclasses
import importlib
import inspect
import json
import re
from enum import Enum
from pathlib import Path

from eltdx.protocol.commands import COMMANDS

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "tests/contracts/manifests/rust_sdk_migration.json"
BASELINE = ROOT / "docs/rust-sdk/baseline-python-api.json"


def test_manifest_keeps_wire_registry_and_catalog_counts_distinct() -> None:
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    assert manifest["counts"] == {
        "wire_codes": 26,
        "python_registry_entries": 25,
        "catalog_capabilities": 124,
        "static_fixture_cases": 37,
    }
    records = manifest["records"]
    assert len(records) == 124
    required = {"id", "source_files", "feature", "stage", "status", "fixtures", "compatibility", "wire_code"}
    assert all(required <= record.keys() for record in records)
    assert len({record["id"] for record in records}) == 124
    assert any(record["wire_code"] == "0x0ffc" for record in records)
    assert any("category22" in record["compatibility"] for record in records)


def test_registry_and_wire_sets_document_only_allowed_difference() -> None:
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    registry_codes = {spec.code for spec in COMMANDS.values()}
    request_source = "\n".join(path.read_text(encoding="utf-8") for path in (ROOT / "crates/eltdx-protocol/src/commands").rglob("*.rs"))
    codes = {int(value, 16) for value in re.findall(r"TYPE_[A-Z_]+\s*=\s*0x([0-9A-Fa-f]+)", request_source)}
    assert len(registry_codes) == manifest["counts"]["python_registry_entries"]
    assert len(codes) == manifest["counts"]["wire_codes"]
    assert codes - registry_codes == {0x0FFC}
    assert registry_codes - codes == set()
    assert any(entry["left"] == "wire_codes" and entry["right"] == "python_registry_entries" for entry in manifest["allowed_differences"])


def test_static_python_baseline_contains_signatures_defaults_exports_properties_and_fields() -> None:
    baseline = json.loads(BASELINE.read_text(encoding="utf-8"))
    assert baseline["extraction"] == "static_ast_no_import_no_execution"
    assert baseline["summary"]["modules"] == len(baseline["modules"])
    for module in baseline["modules"]:
        assert module["path"].startswith("src/eltdx/")
        assert isinstance(module["source_sha256"], str) and len(module["source_sha256"]) == 64
        exported = module.get("all")
        if exported is not None:
            assert isinstance(exported.get("literal_names"), list)
        for function in module["functions"]:
            assert function["signature"] and "parameters" in function and "returns" in function
            for parameter in function["parameters"]:
                assert {"name", "has_default", "default_expr"} <= parameter.keys()
        for cls in module["classes"]:
            assert {"name", "bases", "decorators", "fields", "methods", "properties"} <= cls.keys()
            for method in cls["methods"]:
                assert method["signature"] and "decorators" in method
            for field in cls["fields"]:
                assert {"name", "annotation"} <= field.keys()
            for prop in cls["properties"]:
                assert prop["name"]


def test_import_time_reflection_records_inheritance_dataclasses_decorators_and_enums() -> None:
    modules = ("eltdx.protocol.frame", "eltdx.protocol.commands.registry", "eltdx.models.enums", "eltdx.models.kline", "eltdx.ex.enums")
    records = []
    for module_name in modules:
        module = importlib.import_module(module_name)
        classes = [obj for _, obj in inspect.getmembers(module, inspect.isclass) if obj.__module__ == module_name]
        assert classes, module_name
        for cls in classes:
            records.append({
                "module": module_name,
                "name": cls.__name__,
                "bases": [base.__name__ for base in cls.__bases__],
                "dataclass": dataclasses.is_dataclass(cls),
                "enum": issubclass(cls, Enum),
                "decorated": bool(getattr(cls, "__dataclass_params__", None) or getattr(cls, "__members__", None)),
            })
    assert any(record["dataclass"] and record["decorated"] for record in records)
    assert any(record["enum"] and record["decorated"] for record in records)
    assert any(record["bases"] for record in records)
