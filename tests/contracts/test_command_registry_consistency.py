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
    return _contract_test_node(root, reference) is not None


def _pytest_disabled(statements: list[ast.stmt]) -> bool:
    for statement in statements:
        if isinstance(statement, ast.Assign):
            targets = statement.targets
            value = statement.value
        elif isinstance(statement, ast.AnnAssign):
            targets = [statement.target]
            value = statement.value
        else:
            continue
        if any(isinstance(target, ast.Name) and target.id == "__test__" for target in targets):
            if isinstance(value, ast.Constant) and value.value is False:
                return True
    return False


def _pytest_function_disabled(tree: ast.Module, function: ast.FunctionDef | ast.AsyncFunctionDef) -> bool:
    for decorator in function.decorator_list:
        target = decorator.func if isinstance(decorator, ast.Call) else decorator
        if isinstance(target, ast.Name) and target.id == "fixture":
            return True
        if isinstance(target, ast.Attribute) and target.attr == "fixture":
            return True
    for statement in tree.body:
        if not isinstance(statement, ast.Assign) or not isinstance(statement.value, ast.Constant) or statement.value.value is not False:
            continue
        if any(
            isinstance(target, ast.Attribute)
            and target.attr == "__test__"
            and isinstance(target.value, ast.Name)
            and target.value.id == function.name
            for target in statement.targets
        ):
            return True
    return False


def _contract_test_node(root: Path, reference: str) -> tuple[ast.Module, ast.FunctionDef | ast.AsyncFunctionDef] | None:
    target, *parts = reference.split("::")
    path = root / target
    if not path.is_file() or not parts or not _pytest_file(path) or ".." in Path(target).parts:
        return None
    if not path.resolve().is_relative_to(root.resolve()):
        return None
    tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
    if _pytest_disabled(tree.body):
        return None
    current = tree.body
    for index, part in enumerate(parts):
        matches = [
            node for node in current
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef))
            and node.name == part
        ]
        if len(matches) != 1:
            return None
        node = matches[0]
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            if index == len(parts) - 1 and node.name.startswith("test_") and not _pytest_function_disabled(tree, node):
                return tree, node
            return None
        if index != 0 or not node.name.startswith("Test"):
            return None
        if _pytest_disabled(node.body):
            return None
        invalid_constructors = {"__init__", "__new__"}
        has_invalid_constructor = any(
            isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef)) and child.name in invalid_constructors
            for child in node.body
        )
        for child in node.body:
            if isinstance(child, ast.Assign):
                targets = child.targets
            elif isinstance(child, ast.AnnAssign):
                targets = [child.target]
            else:
                continue
            if any(isinstance(target, ast.Name) and target.id in invalid_constructors for target in targets):
                has_invalid_constructor = True
        if has_invalid_constructor:
            return None
        current = node.body
    return None


def _pytest_file(path: Path) -> bool:
    return path.suffix == ".py" and (path.name.startswith("test_") or path.name.endswith("_test.py"))


def _blank_rust_comments_and_literals(text: str) -> str:
    chars = list(text)
    index = 0

    def blank(start: int, end: int) -> None:
        for position in range(start, end):
            if chars[position] not in "\r\n":
                chars[position] = " "

    while index < len(text):
        if text.startswith("//", index):
            end = text.find("\n", index)
            if end == -1:
                end = len(text)
            blank(index, end)
            index = end
            continue
        if text.startswith("/*", index):
            start = index
            index += 2
            depth = 1
            while index < len(text) and depth:
                if text.startswith("/*", index):
                    depth += 1
                    index += 2
                elif text.startswith("*/", index):
                    depth -= 1
                    index += 2
                else:
                    index += 1
            blank(start, index)
            continue

        raw_start = index
        if text.startswith("br", index):
            raw_start += 1
        if text.startswith("r", raw_start):
            quote = raw_start + 1
            while quote < len(text) and text[quote] == "#":
                quote += 1
            if quote < len(text) and text[quote] == '"':
                hashes = text[raw_start + 1:quote]
                terminator = '"' + hashes
                end = text.find(terminator, quote + 1)
                end = len(text) if end == -1 else end + len(terminator)
                blank(index, end)
                index = end
                continue

        if text[index] == '"':
            start = index
            index += 1
            while index < len(text):
                if text[index] == "\\":
                    index += 2
                elif text[index] == '"':
                    index += 1
                    break
                else:
                    index += 1
            blank(start, index)
            continue
        if text[index] == "'":
            start = index
            cursor = index + 1
            if cursor < len(text) and text[cursor] == "\\":
                cursor += 2
                while cursor < len(text) and text[cursor] != "'" and text[cursor] not in "\r\n":
                    cursor += 1
                if cursor < len(text) and text[cursor] == "'":
                    blank(start, cursor + 1)
                    index = cursor + 1
                    continue
            elif cursor + 1 < len(text) and text[cursor + 1] == "'":
                blank(start, cursor + 2)
                index = cursor + 2
                continue
        index += 1
    return "".join(chars)


def _rust_root_exports(text: str) -> set[str]:
    code = _blank_rust_comments_and_literals(text)
    exports = set()
    depth = 0
    lines = code.splitlines()
    index = 0
    declaration = re.compile(
        r"^\s*pub\s+(?:(?:async|unsafe|const|extern)\s+)*(?:fn|struct|enum|trait|type|const|static|mod|union)\s+([A-Za-z_]\w*)\b"
    )
    single_reexport = re.compile(
        r"pub\s+use\s+(?:(?:crate|self)::)?(?:[A-Za-z_]\w*::)*([A-Za-z_]\w*)(?:\s+as\s+([A-Za-z_]\w*))?\s*;"
    )
    while index < len(lines):
        line = lines[index]
        at_root = depth == 0
        if at_root:
            match = declaration.match(line)
            if match:
                exports.add(match.group(1))
            if re.match(r"^\s*pub\s+use\b", line):
                statement = [line]
                while ";" not in lines[index] and index + 1 < len(lines):
                    index += 1
                    statement.append(lines[index])
                match = single_reexport.fullmatch(" ".join(" ".join(statement).split()))
                if match:
                    exports.add(match.group(2) or match.group(1))
        depth += line.count("{") - line.count("}")
        index += 1
    return exports


def _rust_public_symbol_exists(root: Path, reference: str) -> bool:
    source, *symbols = reference.split("::")
    path = root / source
    if len(symbols) != 1 or source != "crates/eltdx/src/lib.rs" or not path.is_file():
        return False
    repository_root = root.resolve()
    crate_src = (root / "crates/eltdx/src").resolve()
    resolved_path = path.resolve()
    if not crate_src.is_relative_to(repository_root) or not resolved_path.is_relative_to(crate_src):
        return False
    return symbols[0] in _rust_root_exports(path.read_text(encoding="utf-8"))


def _python_module(path: str) -> str:
    relative = Path(path).relative_to("src").with_suffix("")
    parts = list(relative.parts)
    if parts[-1] == "__init__":
        parts.pop()
    return ".".join(parts)


def _test_imports_module(root: Path, reference: str, python_path: str) -> bool:
    resolved = _contract_test_node(root, reference)
    if resolved is None:
        return False
    tree, test_node = resolved
    module = _python_module(python_path)
    imports = [node for node in tree.body if isinstance(node, (ast.Import, ast.ImportFrom))]
    pending = list(test_node.body)
    while pending:
        node = pending.pop()
        if isinstance(node, (ast.Import, ast.ImportFrom)):
            imports.append(node)
        elif not isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            pending.extend(ast.iter_child_nodes(node))
    for node in imports:
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
        record = records[entry["record_id"]]
        assert record["id"] == entry["record_id"]
        assert record["status"] in {"planned", "migrated", "compatibility_adapter_retained", "experimental", "unsupported"}
        assert path in record["source_files"]
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
    source.write_text(
        "pub struct TdxClient;\n"
        "pub(crate) fn internal() {}\n"
        "mod private {\n"
        "    pub struct Hidden;\n"
        "    pub struct Exported;\n"
        "    pub struct Original;\n"
        "}\n"
        "pub use private::Exported;\n"
        "pub use private::Original as PublicAlias;\n"
        "/* pub struct CommentOnly; */\n"
        "const DOC: &str = r#\"\n"
        "pub struct StringOnly;\n"
        "\"#;\n",
        encoding="utf-8",
    )
    nested_source = tmp_path / "crates/eltdx/src/client.rs"
    nested_source.write_text("pub struct NestedOnly;\n", encoding="utf-8")
    test_file = tmp_path / "tests" / "test_contract.py"
    test_file.parent.mkdir(parents=True)
    test_file.write_text("def test_compatibility(): pass\n", encoding="utf-8")
    status = {"status": "migrated", "rust_protocol_status": "wire_supported", "public_sdk_status": "implemented", "sdk_evidence": ["crates/eltdx/src/lib.rs::TdxClient"], "source_files": ["crates/eltdx/src/lib.rs"], "compatibility": "tests/test_contract.py::test_compatibility"}
    _validate_status_record(tmp_path, status, {"planned", "migrated"}, {"none", "wire_supported"}, {"unimplemented", "implemented"})
    assert _rust_public_symbol_exists(tmp_path, "crates/eltdx/src/lib.rs::TdxClient")
    assert _rust_public_symbol_exists(tmp_path, "crates/eltdx/src/lib.rs::Exported")
    assert _rust_public_symbol_exists(tmp_path, "crates/eltdx/src/lib.rs::PublicAlias")
    assert not _rust_public_symbol_exists(tmp_path, "crates/eltdx/src/lib.rs::internal")
    assert not _rust_public_symbol_exists(tmp_path, "crates/eltdx/src/lib.rs::Hidden")
    assert not _rust_public_symbol_exists(tmp_path, "crates/eltdx/src/lib.rs::CommentOnly")
    assert not _rust_public_symbol_exists(tmp_path, "crates/eltdx/src/lib.rs::StringOnly")
    assert not _rust_public_symbol_exists(tmp_path, "crates/eltdx/src/client.rs::NestedOnly")
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
    nested_class_file = tmp_path / "test_nested_class.py"
    nested_class_file.write_text(
        "class TestOuter:\n"
        "    class TestInner:\n"
        "        def test_case(self): pass\n",
        encoding="utf-8",
    )
    assert not _contract_target_exists(tmp_path, "test_nested_class.py::TestOuter::TestInner::test_case")
    uncollectable = tmp_path / "test_uncollectable.py"
    uncollectable.write_text("class TestNotCollected:\n    def __init__(self): pass\n    def test_case(self): pass\n", encoding="utf-8")
    assert not _contract_target_exists(tmp_path, "test_uncollectable.py::TestNotCollected::test_case")
    uncollectable_new = tmp_path / "test_uncollectable_new.py"
    uncollectable_new.write_text("class TestNotCollected:\n    def __new__(cls): return super().__new__(cls)\n    def test_case(self): pass\n", encoding="utf-8")
    assert not _contract_target_exists(tmp_path, "test_uncollectable_new.py::TestNotCollected::test_case")
    uncollectable_assigned_new = tmp_path / "test_uncollectable_assigned_new.py"
    uncollectable_assigned_new.write_text("class TestNotCollected:\n    __new__ = lambda cls: object.__new__(cls)\n    def test_case(self): pass\n", encoding="utf-8")
    assert not _contract_target_exists(tmp_path, "test_uncollectable_assigned_new.py::TestNotCollected::test_case")
    wrong_suffix = tmp_path / "test_contract.txt"
    wrong_suffix.write_text("def test_case(): pass\n", encoding="utf-8")
    assert not _contract_target_exists(tmp_path, "test_contract.txt::test_case")
    fixture_file = tmp_path / "test_fixture.py"
    fixture_file.write_text("@pytest.fixture\ndef test_case(): pass\n", encoding="utf-8")
    assert not _contract_target_exists(tmp_path, "test_fixture.py::test_case")
    disabled_file = tmp_path / "test_disabled.py"
    disabled_file.write_text("__test__ = False\ndef test_case(): pass\n", encoding="utf-8")
    assert not _contract_target_exists(tmp_path, "test_disabled.py::test_case")
    disabled_class_file = tmp_path / "test_disabled_class.py"
    disabled_class_file.write_text("class TestNotCollected:\n    __test__ = False\n    def test_case(self): pass\n", encoding="utf-8")
    assert not _contract_target_exists(tmp_path, "test_disabled_class.py::TestNotCollected::test_case")


def test_python_module_evidence_requires_source_association(tmp_path: Path) -> None:
    test_file = tmp_path / "test_module_contract.py"
    test_file.write_text(
        "def test_model():\n"
        "    def helper():\n"
        "        import eltdx.offline.paths\n"
        "    import eltdx.offline.models\n"
        "\n"
        "def test_other():\n"
        "    import eltdx.offline.paths\n",
        encoding="utf-8",
    )
    assert _test_imports_module(tmp_path, "test_module_contract.py::test_model", "src/eltdx/offline/models.py")
    assert not _test_imports_module(tmp_path, "test_module_contract.py::test_model", "src/eltdx/offline/paths.py")
    module_scope_file = tmp_path / "module_scope_test.py"
    module_scope_file.write_text("import eltdx.offline.paths\n\ndef test_paths(): pass\n", encoding="utf-8")
    assert _test_imports_module(tmp_path, "module_scope_test.py::test_paths", "src/eltdx/offline/paths.py")
    reference = "test_module_contract.py::test_model"
    records = {"sdk-models": {"id": "sdk-models", "status": "planned", "source_files": ["src/eltdx/offline/models.py"]}}
    _validate_coverage_entry(tmp_path, "src/eltdx/offline/models.py", {"coverage_source": "migration_manifest", "record_id": "sdk-models", "test_reference": reference}, set(), records)
    try:
        _validate_coverage_entry(tmp_path, "src/eltdx/offline/paths.py", {"coverage_source": "migration_manifest", "record_id": "sdk-models", "test_reference": reference}, set(), records)
    except AssertionError:
        pass
    else:
        raise AssertionError("migration coverage must bind the Python path to record source_files")
    records["sdk-models"]["status"] = "invalid"
    try:
        _validate_coverage_entry(tmp_path, "src/eltdx/offline/models.py", {"coverage_source": "migration_manifest", "record_id": "sdk-models", "test_reference": reference}, set(), records)
    except AssertionError:
        pass
    else:
        raise AssertionError("migration coverage must reference a valid migration status")
    records["sdk-models"]["status"] = "planned"
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
