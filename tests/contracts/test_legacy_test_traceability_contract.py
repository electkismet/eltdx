"""Import-free traceability contract for deleting private Python runtime tests."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any


ROOT = Path(__file__).parents[2]
MANIFEST = ROOT / "tests/contracts/manifests/legacy_test_traceability.json"
LEGACY_FILES = {
    "tests/test_actor_support.py",
    "tests/test_frame_stream.py",
    "tests/test_protocol_7709.py",
    "tests/test_push_buffer.py",
    "tests/test_socket_transport.py",
    "tests/test_transport_actor.py",
    "tests/test_transport_actor_regressions.py",
    "tests/test_transport_failover_regressions.py",
    "tests/test_transport_lifecycle.py",
    "tests/test_transport_lifecycle_regressions.py",
    "tests/test_transport_pool.py",
    "tests/test_transport_pool_regressions.py",
    "tests/test_transport_retirement_regressions.py",
    "tests/test_transport_stress.py",
}


def _manifest() -> dict[str, Any]:
    return json.loads(MANIFEST.read_text(encoding="utf-8"))


def test_traceability_covers_the_complete_private_test_inventory() -> None:
    manifest = _manifest()
    entries = manifest["entries"]
    assert manifest["schema_version"] == 1
    assert {entry["legacy_file"] for entry in entries} == LEGACY_FILES
    assert len(entries) == len(LEGACY_FILES)
    assert sum(entry["legacy_test_functions"] for entry in entries) == 422
    assert manifest["policy"]["legacy_test_function_total"] == 422
    assert all(entry["disposition"] == "delete_after_replacement" for entry in entries)
    assert all(entry["behavior_families"] for entry in entries)


def test_every_replacement_path_and_anchor_exists() -> None:
    manifest = _manifest()
    observed_kinds: set[str] = set()
    for entry in manifest["entries"]:
        assert entry["replacements"], entry["legacy_file"]
        for replacement in entry["replacements"]:
            observed_kinds.add(replacement["kind"])
            path = ROOT / replacement["path"]
            assert path.is_file(), (entry["legacy_file"], replacement["path"])
            source = path.read_text(encoding="utf-8")
            assert replacement["anchors"]
            for anchor in replacement["anchors"]:
                assert anchor in source, (entry["legacy_file"], replacement["path"], anchor)
    assert observed_kinds == set(manifest["policy"]["required_replacement_kinds"])


def test_private_implementation_tests_are_absent_from_the_frozen_candidate() -> None:
    manifest = _manifest()
    assert manifest["policy"]["legacy_files_must_be_absent_at_static_freeze"] is True
    for relative in LEGACY_FILES:
        assert not (ROOT / relative).exists(), relative
