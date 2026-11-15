"""Contracts for migration counts, frozen Python API, and runtime reflection."""
from __future__ import annotations

import ast
import dataclasses
import importlib
import inspect
import json
import re
import subprocess
from enum import Enum
from pathlib import Path

from eltdx.protocol.commands import COMMANDS

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "tests/contracts/manifests/rust_sdk_migration.json"
BASELINE = ROOT / "docs/rust-sdk/baseline-python-api.json"
RUNTIME_BASELINE = ROOT / "tests/contracts/manifests/python_runtime_reflection.json"
SPECIAL = {
    "__init__", "__enter__", "__exit__", "__getitem__", "__len__", "__iter__",
    "__contains__", "__next__", "__bool__", "__call__", "__aenter__", "__aexit__",
    "__aiter__", "__anext__",
}


def _expr(node: ast.AST | None) -> str | None:
    return ast.unparse(node) if node is not None else None


def _visible(name: str) -> bool:
    return not name.startswith("_") or name in SPECIAL


def _parameter(arg: ast.arg, kind: str, default: ast.AST | None = None, has_default: bool = False) -> dict:
    return {"name": arg.arg, "kind": kind, "annotation": _expr(arg.annotation), "has_default": has_default, "default_expr": _expr(default) if has_default else None}


def _function(node: ast.FunctionDef | ast.AsyncFunctionDef) -> dict:
    args = node.args
    positional = list(args.posonlyargs) + list(args.args)
    first_default = len(positional) - len(args.defaults)
    params = []
    for i, arg in enumerate(positional):
        has_default = i >= first_default
        params.append(_parameter(arg, "positional_only" if i < len(args.posonlyargs) else "positional_or_keyword", args.defaults[i - first_default] if has_default else None, has_default))
    if args.vararg:
        params.append(_parameter(args.vararg, "var_positional"))
    for arg, default in zip(args.kwonlyargs, args.kw_defaults):
        params.append(_parameter(arg, "keyword_only", default, default is not None))
    if args.kwarg:
        params.append(_parameter(args.kwarg, "var_keyword"))
    returns = _expr(node.returns)
    return {"name": node.name, "line": node.lineno, "async": isinstance(node, ast.AsyncFunctionDef), "signature": f"{node.name}({ast.unparse(args)})" + (f" -> {returns}" if returns else ""), "parameters": params, "returns": returns, "decorators": [_expr(item) for item in node.decorator_list]}


def _assignments(body: list[ast.stmt]) -> list[dict]:
    result = []
    for node in body:
        if isinstance(node, ast.Assign):
            for target in node.targets:
                if isinstance(target, ast.Name) and _visible(target.id):
                    result.append({"name": target.id, "line": node.lineno, "value_expr": _expr(node.value)})
    return result


def _class_record(node: ast.ClassDef) -> dict:
    methods = [_function(item) for item in node.body if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)) and _visible(item.name)]
    fields = [{"name": item.target.id, "line": item.lineno, "annotation": _expr(item.annotation), "has_default": item.value is not None, "default_expr": _expr(item.value)} for item in node.body if isinstance(item, ast.AnnAssign) and isinstance(item.target, ast.Name) and _visible(item.target.id)]
    return {"name": node.name, "line": node.lineno, "bases": [_expr(item) for item in node.bases], "keywords": [{"name": item.arg, "value_expr": _expr(item.value)} for item in node.keywords], "decorators": [_expr(item) for item in node.decorator_list], "fields": fields, "methods": methods, "properties": [item["name"] for item in methods if any(d == "property" or d.endswith((".setter", ".getter", ".deleter")) for d in item["decorators"])], "members": _assignments(node.body)}


def _source_record(path: str, raw: bytes) -> dict:
    tree = ast.parse(raw.decode("utf-8-sig"), filename=path)
    relative = Path(path).relative_to("src")
    parts = list(relative.with_suffix("").parts)
    if parts[-1] == "__init__":
        parts.pop()
    private = any(part.startswith("_") and part != "__init__.py" for part in Path(path).relative_to("src/eltdx").parts)
    imports = []
    exports = None
    for node in tree.body:
        if isinstance(node, ast.ImportFrom):
            imports.append({"from": node.module, "level": node.level, "names": [{"name": a.name, "asname": a.asname} for a in node.names]})
        elif isinstance(node, ast.Import):
            imports.append({"from": None, "level": 0, "names": [{"name": a.name, "asname": a.asname} for a in node.names]})
        elif isinstance(node, ast.Assign) and any(isinstance(t, ast.Name) and t.id == "__all__" for t in node.targets):
            try:
                value = ast.literal_eval(node.value)
            except (ValueError, TypeError):
                value = None
            exports = {"expression": _expr(node.value), "literal_names": value}
    return {"path": path, "module": ".".join(parts), "source_sha256": __import__("hashlib").sha256(raw).hexdigest(), "scope": "internal_support" if private else "public_candidate", "all": exports, "imports": imports, "functions": [_function(n) for n in tree.body if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef)) and _visible(n.name)], "classes": [_class_record(n) for n in tree.body if isinstance(n, ast.ClassDef) and _visible(n.name)], "aliases": _assignments(tree.body)}


def _git_source(revision: str, path: str) -> bytes:
    return subprocess.check_output(("git", "show", f"{revision}:{path}"), cwd=ROOT)


def _contract_target_exists(root: Path, reference: str) -> bool:
    target, *parts = reference.split("::")
    path = root / target
    if not path.is_file() or not parts or not _pytest_file(path) or ".." in Path(target).parts:
        return False
    if not path.resolve().is_relative_to(root.resolve()):
        return False
    tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
    current = tree.body
    for index, part in enumerate(parts):
        matches = [
            node for node in current
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef))
            and node.name == part
        ]
        if len(matches) != 1:
            return False
        node = matches[0]
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            return index == len(parts) - 1 and node.name.startswith("test_")
        if not node.name.startswith("Test"):
            return False
        if any(
            isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef)) and child.name == "__init__"
            for child in node.body
        ):
            return False
        current = node.body
    return False


def _pytest_file(path: Path) -> bool:
    return path.name.startswith("test_") or path.name.endswith("_test.py")


def _rust_public_symbol_exists(root: Path, reference: str) -> bool:
    source, *symbols = reference.split("::")
    path = root / source
    if len(symbols) != 1 or not source.startswith("crates/eltdx/src/") or path.suffix != ".rs" or not path.is_file():
        return False
    if ".." in Path(source).parts:
        return False
    if not path.resolve().is_relative_to(root.resolve()):
        return False
    text = path.read_text(encoding="utf-8")
    return any(
        re.search(rf"(?m)^\s*pub\s+(?:(?:async|unsafe|const|extern\s+\"[^\"]+\")\s+)*{kind}\s+{re.escape(symbol)}\b", text)
        for symbol in symbols
        for kind in ("fn", "struct", "enum", "trait", "type", "const", "static", "mod", "union")
    )


def _python_module(path: str) -> str:
    relative = Path(path).relative_to("src").with_suffix("")
    parts = list(relative.parts)
    if parts[-1] == "__init__":
        parts.pop()
    return ".".join(parts)


def _test_imports_module(root: Path, reference: str, python_path: str) -> bool:
    target = root / reference.split("::", 1)[0]
    if not target.is_file():
        return False
    module = _python_module(python_path)
    tree = ast.parse(target.read_text(encoding="utf-8"), filename=str(target))
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            if any(alias.name == module or alias.name.startswith(module + ".") for alias in node.names):
                return True
        elif isinstance(node, ast.ImportFrom) and node.module:
            imported = node.module
            if imported == module or imported.startswith(module + "."):
                return True
            if module.startswith(imported + ".") and any(alias.name == module[len(imported) + 1:].split(".")[0] for alias in node.names):
                return True
    return False


def _validate_coverage_entry(root: Path, path: str, entry: dict, frozen: set[str], records: dict[str, dict]) -> None:
    assert _contract_target_exists(root, entry["test_reference"])
    source = entry["coverage_source"]
    if source == "frozen_static_baseline":
        assert path in frozen
    elif source == "migration_manifest":
        assert entry["record_id"] in records
        assert path in records[entry["record_id"]]["source_files"]
    elif source == "compatibility_contract":
        assert _test_imports_module(root, entry["test_reference"], path)
    else:
        raise AssertionError(f"unknown Python module coverage source: {source}")


def _validate_status_record(root: Path, record: dict, allowed_statuses: set[str], allowed_protocol: set[str], allowed_sdk: set[str]) -> None:
    assert record["status"] in allowed_statuses
    assert record["rust_protocol_status"] in allowed_protocol
    assert record["public_sdk_status"] in allowed_sdk
    if record["status"] == "migrated":
        assert record["public_sdk_status"] == "implemented"
        assert record["sdk_evidence"] and all(
            _rust_public_symbol_exists(root, ref)
            and ref.split("::", 1)[0] in record["source_files"]
            for ref in record["sdk_evidence"]
        )
        assert isinstance(record["compatibility"], str) and _contract_target_exists(root, record["compatibility"])
    if record["rust_protocol_status"] == "wire_supported":
        assert record["status"] in {"planned", "experimental", "unsupported"} or record["public_sdk_status"] == "implemented"


def test_manifest_matches_authoritative_catalog_records() -> None:
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    text = (ROOT / manifest["catalog_source_path"]).read_text(encoding="utf-8")
    catalog = json.loads(text[text.index("{"):text.rfind(";")])
    expected = {item["id"]: item for item in catalog["items"]}
    records = {record["id"]: record for record in manifest["records"]}
    assert len(expected) == 124 and set(records) == set(expected)
    for item_id, item in expected.items():
        record = records[item_id]
        assert record["catalog_source"] == item["source"]
        assert record["catalog_doc"] == item.get("doc")
        assert record["catalog_protocol"] == item.get("protocol")
        assert f"docs/{item['doc']}" in record["source_files"] if item.get("doc") else True


def test_python_module_coverage_accounts_for_every_current_module() -> None:
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    baseline = json.loads(BASELINE.read_text(encoding="utf-8"))
    current = {path.relative_to(ROOT).as_posix() for path in (ROOT / "src/eltdx").rglob("*.py") if "/docs/" not in path.as_posix()}
    frozen = {module["path"] for module in baseline["modules"]}
    coverage = {entry["path"]: entry for entry in manifest["python_module_coverage"]}
    assert frozen <= set(coverage)
    assert current <= frozen | set(coverage)
    allowed_sources = {"frozen_static_baseline", "migration_manifest", "compatibility_contract"}
    records = {record["id"]: record for record in manifest["records"]}
    for path in current:
        entry = coverage[path]
        assert entry["coverage_source"] in allowed_sources
        _validate_coverage_entry(ROOT, path, entry, frozen, records)
    for path in current - frozen:
        assert coverage[path]["coverage_source"] in {"migration_manifest", "compatibility_contract"}


def test_manifest_keeps_wire_registry_and_catalog_counts_distinct() -> None:
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    assert manifest["counts"] == {"wire_codes": 26, "python_registry_entries": 25, "catalog_capabilities": 124, "static_fixture_cases": 37}
    records = manifest["records"]
    required = {"id", "source_files", "feature", "stage", "status", "fixtures", "compatibility", "wire_code"}
    assert len(records) == 124 and all(required <= record.keys() for record in records)
    assert len({record["id"] for record in records}) == 124
    assert any(record["wire_code"] == "0x0ffc" for record in records)
    assert any("category22" in record["compatibility"] for record in records)


def test_registry_and_wire_sets_document_only_allowed_difference() -> None:
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    registry_codes = {spec.code for spec in COMMANDS.values()}
    source = "\n".join(path.read_text(encoding="utf-8") for path in (ROOT / "crates/eltdx-protocol/src/commands").rglob("*.rs"))
    codes = {int(value, 16) for value in re.findall(r"TYPE_[A-Z_]+\s*(?::\s*u16)?\s*=\s*0x([0-9A-Fa-f]+)", source)}
    assert len(registry_codes) == 25 and len(codes) == 26
    assert codes - registry_codes == {0x0FFC} and registry_codes - codes == set()
    assert any(entry["left"] == "wire_codes" and entry["right"] == "python_registry_entries" for entry in manifest["allowed_differences"])


def test_migration_status_vocabulary_enforces_protocol_sdk_boundary() -> None:
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    allowed_statuses = {"planned", "migrated", "compatibility_adapter_retained", "experimental", "unsupported"}
    allowed_protocol = {"none", "wire_supported", "python_helper"}
    allowed_sdk = {"unimplemented", "implemented"}
    assert {key for key in manifest["status_semantics"] if key in allowed_statuses} == allowed_statuses
    for record in manifest["records"]:
        _validate_status_record(ROOT, record, allowed_statuses, allowed_protocol, allowed_sdk)


def test_migration_evidence_rejects_protocol_only_missing_or_unsafe_sources(tmp_path: Path) -> None:
    source = tmp_path / "crates/eltdx/src/lib.rs"
    source.parent.mkdir(parents=True)
    source.write_text("pub struct TdxClient;\npub(crate) fn internal() {}\n", encoding="utf-8")
    test_file = tmp_path / "tests" / "test_contract.py"
    test_file.parent.mkdir(parents=True)
    test_file.write_text("def test_compatibility(): pass\n", encoding="utf-8")
    status = {"status": "migrated", "rust_protocol_status": "wire_supported", "public_sdk_status": "implemented", "sdk_evidence": ["crates/eltdx/src/lib.rs::TdxClient"], "source_files": ["crates/eltdx/src/lib.rs"], "compatibility": "tests/test_contract.py::test_compatibility"}
    _validate_status_record(tmp_path, status, {"planned", "migrated"}, {"none", "wire_supported"}, {"unimplemented", "implemented"})
    assert _rust_public_symbol_exists(tmp_path, "crates/eltdx/src/lib.rs::TdxClient")
    assert not _rust_public_symbol_exists(tmp_path, "crates/eltdx/src/lib.rs::internal")
    assert not _rust_public_symbol_exists(tmp_path, "crates/eltdx/src/../src/lib.rs::TdxClient")
    assert not _rust_public_symbol_exists(tmp_path, "crates/eltdx/src/lib.rs::Missing")
    status["source_files"] = []
    try:
        _validate_status_record(tmp_path, status, {"planned", "migrated"}, {"none", "wire_supported"}, {"unimplemented", "implemented"})
    except AssertionError:
        pass
    else:
        raise AssertionError("migration evidence outside source_files must be rejected")


def test_pytest_reference_requires_exact_collectable_test_path(tmp_path: Path) -> None:
    test_file = tmp_path / "test_contract.py"
    test_file.write_text(
        "class TestGroup:\n"
        "    def test_exact(self): pass\n"
        "def helper(): pass\n"
        "def test_exact(self): pass\n",
        encoding="utf-8",
    )
    assert _contract_target_exists(tmp_path, "test_contract.py::TestGroup::test_exact")
    assert not _contract_target_exists(tmp_path, "test_contract.py::test_exact")
    assert not _contract_target_exists(tmp_path, "test_contract.py::TestGroup::helper")
    assert not _contract_target_exists(tmp_path, "contract.py::TestGroup::test_exact")
    uncollectable = tmp_path / "test_uncollectable.py"
    uncollectable.write_text("class TestNotCollected:\n    def __init__(self): pass\n    def test_case(self): pass\n", encoding="utf-8")
    assert not _contract_target_exists(tmp_path, "test_uncollectable.py::TestNotCollected::test_case")


def test_python_module_evidence_requires_source_association(tmp_path: Path) -> None:
    test_file = tmp_path / "test_module_contract.py"
    test_file.write_text("import eltdx.offline.models\n\ndef test_model(): pass\n", encoding="utf-8")
    assert _test_imports_module(tmp_path, "test_module_contract.py::test_model", "src/eltdx/offline/models.py")
    assert not _test_imports_module(tmp_path, "test_module_contract.py::test_model", "src/eltdx/offline/paths.py")
    reference = "test_module_contract.py::test_model"
    records = {"sdk-models": {"source_files": ["src/eltdx/offline/models.py"]}}
    _validate_coverage_entry(tmp_path, "src/eltdx/offline/models.py", {"coverage_source": "migration_manifest", "record_id": "sdk-models", "test_reference": reference}, set(), records)
    try:
        _validate_coverage_entry(tmp_path, "src/eltdx/offline/paths.py", {"coverage_source": "migration_manifest", "record_id": "sdk-models", "test_reference": reference}, set(), records)
    except AssertionError:
        pass
    else:
        raise AssertionError("migration coverage must bind the Python path to record source_files")
    _validate_coverage_entry(tmp_path, "src/eltdx/offline/models.py", {"coverage_source": "compatibility_contract", "test_reference": reference}, set(), records)
    try:
        _validate_coverage_entry(tmp_path, "src/eltdx/offline/paths.py", {"coverage_source": "compatibility_contract", "test_reference": reference}, set(), records)
    except AssertionError:
        pass
    else:
        raise AssertionError("compatibility coverage must import its Python module")
    _validate_coverage_entry(tmp_path, "src/eltdx/offline/models.py", {"coverage_source": "frozen_static_baseline", "test_reference": reference}, {"src/eltdx/offline/models.py"}, records)


def test_static_python_baseline_matches_authoritative_source_revision() -> None:
    baseline = json.loads(BASELINE.read_text(encoding="utf-8"))
    assert baseline["source_revision"] == "af471f1adf38757f6aa0188968db8110eb23f6ad"
    expected = {}
    for module in baseline["modules"]:
        raw = _git_source(baseline["source_revision"], module["path"])
        expected[module["path"]] = _source_record(module["path"], raw)
        assert module == expected[module["path"]]
    assert baseline["summary"] == {"modules": 134, "classes": 231, "module_functions": 247, "methods": 724, "annotated_class_fields": 1220}


def _without_locations(value):
    if isinstance(value, dict):
        return {key: _without_locations(item) for key, item in value.items() if key not in {"line", "source_sha256"}}
    if isinstance(value, list):
        return [_without_locations(item) for item in value]
    return value


def test_current_worktree_ast_contract_matches_frozen_snapshot() -> None:
    baseline = json.loads(BASELINE.read_text(encoding="utf-8"))
    for module in baseline["modules"]:
        current = _source_record(module["path"], (ROOT / module["path"]).read_bytes())
        assert _without_locations(current) == _without_locations(module)


def test_runtime_reflection_matches_versioned_artifact() -> None:
    artifact = json.loads(RUNTIME_BASELINE.read_text(encoding="utf-8"))
    assert artifact["schema_version"] == 1 and artifact["extraction"] == "runtime_import_reflection"
    for module_record in artifact["modules"]:
        module = importlib.import_module(module_record["module"])
        actual = {name: cls for name, cls in inspect.getmembers(module, inspect.isclass) if cls.__module__ == module_record["module"]}
        assert set(actual) == {record["name"] for record in module_record["classes"]}
        for record in module_record["classes"]:
            cls = actual[record["name"]]
            assert record["bases"] == [base.__module__ + "." + base.__qualname__ for base in cls.__bases__]
            assert record["mro"] == [base.__module__ + "." + base.__qualname__ for base in cls.__mro__]
            assert record["signature"] == str(inspect.signature(cls))
            assert record["dataclass"]["is_dataclass"] == dataclasses.is_dataclass(cls)
            expected_decorators = set(record["decorators"])
            assert ("dataclasses.dataclass" in expected_decorators) == dataclasses.is_dataclass(cls)
            assert ("enum.Enum" in expected_decorators) == issubclass(cls, Enum)
            if dataclasses.is_dataclass(cls):
                fields = dataclasses.fields(cls)
                recorded = record["dataclass"]["fields"]
                assert [field["name"] for field in recorded] == [field.name for field in fields]
                for saved, field in zip(recorded, fields):
                    assert saved["type"] == str(field.type)
                    assert saved["default"] == (None if field.default is dataclasses.MISSING else repr(field.default))
                    assert saved["default_factory"] == (None if field.default_factory is dataclasses.MISSING else repr(field.default_factory))
            if issubclass(cls, Enum):
                assert record["enum"]["members"] == {name: value.value for name, value in cls.__members__.items()}
            properties = {name: value for name, value in cls.__dict__.items() if isinstance(value, property)}
            assert {item["name"] for item in record["properties"]} == set(properties)
            for item in record["properties"]:
                getter = properties[item["name"]].fget
                assert item["getter"] == (str(inspect.signature(getter)) if getter else None)


def test_runtime_artifact_covers_reflection_categories() -> None:
    artifact = json.loads(RUNTIME_BASELINE.read_text(encoding="utf-8"))
    classes = [record for module in artifact["modules"] for record in module["classes"]]
    assert any(record["dataclass"]["is_dataclass"] for record in classes)
    assert any(record["enum"] is not None for record in classes)
    assert any(record["bases"] for record in classes)
    assert all("signature" in record and "mro" in record and "decorators" in record for record in classes)
