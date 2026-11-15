"""Provenance and evidence contracts for Rust SDK fixtures."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path

from scripts.fixtures.differential import discover_cases, discover_sdk_cases


ROOT = Path(__file__).parents[2]
SDK_ROOT = ROOT / "tests" / "fixtures" / "sdk"
INVENTORY_PATH = SDK_ROOT / "inventory.json"
GENERATOR_REVISION = "401dda95dd337ddf1440c18629be37d39d134d2c"
GOLDEN_REVISION = "0fb9d00ebbb2dabc1f40f511919a192003896e36"


def _inventory() -> dict:
    return json.loads(INVENTORY_PATH.read_text(encoding="utf-8"))


def _check_artifact(artifact: dict, *, pointer_allowed: bool = False) -> None:
    path = ROOT / artifact["path"]
    assert path.is_file(), artifact["path"]
    assert hashlib.sha256(path.read_bytes()).hexdigest() == artifact["sha256"]
    if pointer_allowed:
        assert artifact["json_pointer"] in {"/request", "/response", "/expected"}


def test_legacy_fixture_inventory_maps_all_25_normal_and_12_extra_cases() -> None:
    inventory = _inventory()
    discovered = discover_cases(ROOT / "tests" / "fixtures" / "7709")
    entries = {item["case_id"]: item for item in inventory["legacy_cases"]}

    assert len(discovered) == len(entries) == 37
    assert set(entries) == {case.case_id for case in discovered}
    assert sum(case.case_id.endswith("/normal") for case in discovered) == 25
    assert sum(not case.case_id.endswith("/normal") for case in discovered) == 12

    for case in discovered:
        entry = entries[case.case_id]
        assert entry["request"]["path"].endswith(f"/{case.case_id}/request.json")
        assert entry["response"]["path"].endswith(f"/{case.case_id}/response.bin")
        assert entry["expected"]["path"].endswith(f"/{case.case_id}/expected.json")
        if case.case_id == "delisted_bars/normal":
            assert entry["source_type"] == "reference"
            assert entry["source_revision"] == "052b-ui-20261005"
        else:
            assert entry["source_type"] == "synthetic"
            assert entry["source_revision"] == GENERATOR_REVISION
        assert entry["expected"]["source_revision"] == GOLDEN_REVISION
        assert entry["expected"]["evidence_level"] == "python_regression_baseline"
        assert entry["expected"]["independent_oracle"] is False
        for artifact in (entry["request"], entry["response"], entry["expected"]):
            _check_artifact(artifact)


def test_delisted_fixture_is_a_reference_with_missing_source_not_a_capture() -> None:
    entry = next(
        item for item in _inventory()["legacy_cases"]
        if item["case_id"] == "delisted_bars/normal"
    )
    assert entry["source_type"] == "reference"
    assert entry["input_provenance"]["original_path"] == "052b-ui-20261005/live-query-example.json"
    assert entry["input_provenance"]["available_in_repository"] is False
    assert entry["input_provenance"]["frame_generator_revision"] == GENERATOR_REVISION


def test_supplemental_fixtures_define_both_sdk_profiles_and_provenance() -> None:
    inventory = _inventory()
    fixtures = discover_sdk_cases(SDK_ROOT)
    entries = {item["case_id"]: item for item in inventory["supplemental_cases"]}
    assert len(fixtures) == len(entries) == 7
    assert set(entries) == {fixture.case_id for fixture in fixtures}

    for fixture in fixtures:
        entry = entries[fixture.case_id]
        assert entry["source_type"] == fixture.source_type == "synthetic"
        assert entry["source_revision"] == fixture.source_revision
        assert entry["evidence_level"] == fixture.evidence_level
        assert entry["spec_refs"] == list(fixture.spec_refs)
        for name in ("request", "response", "expected"):
            _check_artifact(entry[name], pointer_allowed=True)
        assert set(fixture.expected) == {"python_compat", "rust"}


def test_evidence_gaps_keep_money_flow_category22_and_extension_paths_distinct() -> None:
    gaps = {item["case_id"]: item for item in _inventory()["evidence_gaps"]}
    assert gaps["money_flow/0x0FFC/nonempty-wire-capture"]["status"] == "missing"
    assert gaps["klines/0x052D/category22/nonempty-response"]["status"] == "empty-only-reported"
    assert gaps["extension/ex/mac/f10/real-response-captures"]["status"] == "synthetic-only"
    assert "0x0FFC" in gaps["klines/0x052D/category22/nonempty-response"]["handling"]
