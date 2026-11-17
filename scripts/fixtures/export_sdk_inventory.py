"""Write provenance metadata for Rust SDK fixtures without running parsers."""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from scripts.fixtures.differential import discover_cases, discover_sdk_cases  # noqa: E402


LEGACY_FIXTURE_IDS = frozenset(
    {
        "auction_series/normal",
        "block_meta/normal",
        "capital_changes/normal",
        "category_quotes/normal",
        "category_quotes/trailing_byte",
        "company_info_category/normal",
        "company_info_content/normal",
        "delisted_bars/normal",
        "file_content/max_chunk_empty",
        "file_content/normal",
        "finance_batch/normal",
        "handshake/normal",
        "heartbeat/bad_compression",
        "heartbeat/compressed",
        "heartbeat/normal",
        "heartbeat/stale_message",
        "historical_intraday/normal",
        "historical_ticks/normal",
        "intraday_aux/normal",
        "klines/include_raw_false",
        "klines/max_page_empty",
        "klines/normal",
        "legacy_quotes/normal",
        "recent_intraday/include_raw_false",
        "recent_intraday/normal",
        "refresh_stream/normal",
        "security_count/normal",
        "security_list/bj_empty",
        "security_list/normal",
        "security_list/sh_empty",
        "snapshots/etf_index_empty",
        "snapshots/normal",
        "snapshots/truncated_record",
        "sparkline/normal",
        "special_limits/normal",
        "today_intraday/normal",
        "today_ticks/normal",
    }
)

GENERATOR_REVISION = "401dda95dd337ddf1440c18629be37d39d134d2c"
GOLDEN_REVISION = "0fb9d00ebbb2dabc1f40f511919a192003896e36"
REFERENCE_CASE = "delisted_bars/normal"
REFERENCE_SOURCE = "052b-ui-20261005/live-query-example.json"


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _relative(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def _legacy_record(case: Any) -> dict[str, Any]:
    case_root = case.root
    case_id = case.case_id
    is_reference = case_id == REFERENCE_CASE
    source_type = "reference" if is_reference else "synthetic"
    source_revision = "052b-ui-20261005" if is_reference else GENERATOR_REVISION
    request_path = case_root / "request.json"
    response_path = case_root / "response.bin"
    expected_path = case_root / "expected.json"
    source = (
        {
            "reference_id": "052b-ui-20261005",
            "original_path": REFERENCE_SOURCE,
            "available_in_repository": False,
            "construction": "synthetic 7709 frame around referenced record_hex values",
            "frame_generator_revision": GENERATOR_REVISION,
        }
        if is_reference
        else {
            "generator": "scripts/fixtures/materialize_static_inputs.py",
            "generator_revision": GENERATOR_REVISION,
        }
    )
    return {
        "case_id": case_id,
        "source_type": source_type,
        "source_revision": source_revision,
        "input_provenance": source,
        "request": {"path": _relative(request_path), "sha256": _sha256(request_path)},
        "response": {"path": _relative(response_path), "sha256": _sha256(response_path)},
        "expected": {
            "path": _relative(expected_path),
            "sha256": _sha256(expected_path),
            "source_revision": GOLDEN_REVISION,
            "generated_by": "scripts/fixtures/export_current_golden.py",
            "evidence_level": "python_regression_baseline",
            "independent_oracle": False,
        },
        "evidence_level": "python_regression_baseline",
    }


def _sdk_record(fixture_path: Path, raw: dict[str, Any]) -> dict[str, Any]:
    relative_path = _relative(fixture_path)
    digest = _sha256(fixture_path)
    artifact = {"path": relative_path, "sha256": digest}
    return {
        "case_id": raw["case_id"],
        "source_type": raw["source_type"],
        "source_revision": raw["source_revision"],
        "request": {**artifact, "json_pointer": "/request"},
        "response": {**artifact, "json_pointer": "/response"},
        "expected": {
            **artifact,
            "json_pointer": "/expected",
            "evidence_level": raw["evidence_level"],
            "independent_oracle": raw["evidence_level"]
            == "independent_protocol_calculation",
        },
        "evidence_level": raw["evidence_level"],
        "spec_refs": raw["spec_refs"],
    }


def build_inventory() -> dict[str, Any]:
    legacy_cases = discover_cases(ROOT / "tests" / "fixtures" / "7709")
    legacy_ids = {case.case_id for case in legacy_cases}
    if legacy_ids != LEGACY_FIXTURE_IDS:
        missing = sorted(LEGACY_FIXTURE_IDS - legacy_ids)
        unexpected = sorted(legacy_ids - LEGACY_FIXTURE_IDS)
        raise ValueError(f"7709 fixture inventory changed: missing={missing}, unexpected={unexpected}")

    sdk_root = ROOT / "tests" / "fixtures" / "sdk"
    discover_sdk_cases(sdk_root)
    sdk_records = []
    for fixture_path in sorted((sdk_root / "cases").glob("*.json")):
        raw = json.loads(fixture_path.read_text(encoding="utf-8"))
        sdk_records.append(_sdk_record(fixture_path, raw))

    return {
        "schema_version": 1,
        "legacy_cases": [_legacy_record(case) for case in legacy_cases],
        "supplemental_cases": sdk_records,
        "evidence_gaps": [
            {
                "case_id": "money_flow/0x0FFC/nonempty-wire-capture",
                "status": "missing",
                "source_type": "capture",
                "source_revision": None,
                "reason": "The documentation sample contains a truncated record_hex, not the complete 88-byte wire record.",
                "handling": "Do not label the documentation values as a replayable packet; add an independently recorded response when available.",
            },
            {
                "case_id": "klines/0x052D/category22/nonempty-response",
                "status": "empty-only-reported",
                "source_type": "capture",
                "source_revision": GENERATOR_REVISION,
                "reason": "The repository reports reachable-host responses as the two-byte empty payload 0000; no non-empty response is stored.",
                "handling": "Keep category22 experimental and distinct from stable 0x0FFC daily flow and 7615 F10 history.",
            },
            {
                "case_id": "extension/ex/mac/f10/real-response-captures",
                "status": "synthetic-only",
                "source_type": "capture",
                "source_revision": None,
                "reason": "Current SDK fixtures cover selected codec and table contracts with synthetic inputs; they do not contain exchange/HTTP captures for EX response correlation, unknown MAC bits, or duplicate F10 columns.",
                "handling": "Preserve each protocol as a separately labeled experimental evidence domain until dated, source-attributed captures are checked in.",
            },
        ],
        "provenance_policy": {
            "legacy_generator_revision": GENERATOR_REVISION,
            "legacy_expected_revision": GOLDEN_REVISION,
            "legacy_expected_independent_oracle": False,
            "note": "Python-generated expected values are regression baselines only; supplemental contracts identify their evidence level per case.",
        },
    }


def main() -> int:
    output = ROOT / "tests" / "fixtures" / "sdk" / "inventory.json"
    content = json.dumps(build_inventory(), ensure_ascii=False, indent=2) + "\n"
    output.write_text(content, encoding="utf-8", newline="\n")
    print(output.relative_to(ROOT).as_posix())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
