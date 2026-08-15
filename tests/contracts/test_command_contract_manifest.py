"""Structural contracts for all native 7709 commands and canonical fixtures."""

from __future__ import annotations

import json
from pathlib import Path

from eltdx.protocol.commands import COMMANDS


ROOT = Path(__file__).parents[2]
MANIFEST = Path(__file__).with_name("manifests") / "command_contracts.json"
CANONICAL_SCHEMA = ROOT / "tests" / "fixtures" / "7709" / "canonical_fixture.schema.json"


def _load(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def test_command_manifest_covers_exact_registry() -> None:
    manifest = _load(MANIFEST)
    commands = manifest["commands"]
    assert len(commands) == 21
    assert set(commands) == set(COMMANDS)
    assert len({contract["code"] for contract in commands.values()}) == 21

    for name, contract in commands.items():
        spec = COMMANDS[name]
        assert contract["code"] == spec.code
        assert contract["retry_safe"] is spec.retry_safe is True
        assert contract["module"] == spec.module
        assert contract["method"] == spec.method
        assert contract["push"] in {"response_only", "response_or_push"}
        assert isinstance(contract["include_raw"], bool)
        assert isinstance(contract["fields"], dict)
        assert isinstance(contract["request_context"], list)
        assert contract["response"]


def test_command_fields_define_types_defaults_and_aliases() -> None:
    commands = _load(MANIFEST)["commands"]
    for name, contract in commands.items():
        for field_name, field in contract["fields"].items():
            assert field["type"], (name, field_name)
            assert not (field.get("required") and ("default" in field or "default_hex" in field))
            if "minimum" in field and "maximum" in field:
                assert field["minimum"] <= field["maximum"]
            assert len(field.get("aliases", [])) == len(set(field.get("aliases", [])))


def test_canonical_schema_has_all_required_tags_and_metadata() -> None:
    schema = _load(CANONICAL_SCHEMA)
    definitions = schema["$defs"]
    required_tags = {"dataclass", "tuple", "list", "bytes", "date", "datetime", "float", "none", "missing"}
    assert required_tags <= set(definitions)
    assert definitions["float"]["properties"]["f64_bits"]["pattern"] == "^[0-9a-f]{16}$"
    assert definitions["dataclass"]["required"] == ["$type", "module", "qualname", "fields"]
    metadata_required = set(definitions["metadata"]["required"])
    assert {"baseline_wheel_sha256", "registry_key", "command_code", "message_id", "request_context", "frame_header", "push_generation", "push_host", "expected_exception"} <= metadata_required
