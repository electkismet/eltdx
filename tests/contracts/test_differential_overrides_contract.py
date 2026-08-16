"""Narrow contracts for intentional v2.0.5 differential overrides."""

from __future__ import annotations

from pathlib import Path

from scripts.fixtures.differential import (
    DifferentialCase,
    applicable_override,
    load_overrides,
    target_expected,
    target_request_bytes,
)
from scripts.fixtures.export_v205_baseline import to_canonical


ROOT = Path(__file__).parents[2]
OVERRIDES_PATH = ROOT / "tests" / "contracts" / "manifests" / "differential_overrides.json"


def test_auction_series_trading_date_override_adds_only_model_field() -> None:
    expected = {
        "$type": "dataclass",
        "module": "eltdx.models.auction",
        "qualname": "AuctionSeries",
        "fields": [
            ["exchange", to_canonical("sz")],
            ["code", to_canonical("000001")],
            ["points", to_canonical([])],
        ],
    }
    case = DifferentialCase(
        root=Path("auction_series") / "normal",
        case_id="auction_series/normal",
        command="auction_series",
        command_code=0x056A,
        message_id=1,
        request_payload={"code": "sz000001"},
        request_bytes=b"unchanged-wire-frame",
        response_bytes=b"",
        expected=expected,
        expected_exception=None,
        metadata={},
    )
    override = applicable_override(case, load_overrides(OVERRIDES_PATH))
    assert override is not None
    assert target_request_bytes(case, override) == case.request_bytes
    assert target_expected(case, override)["fields"] == [
        ["exchange", to_canonical("sz")],
        ["code", to_canonical("000001")],
        ["trading_date", to_canonical(None)],
        ["points", to_canonical([])],
    ]

    explicit = DifferentialCase(
        root=case.root,
        case_id=case.case_id,
        command=case.command,
        command_code=case.command_code,
        message_id=case.message_id,
        request_payload={**case.request_payload, "trading_date": "2026-08-14"},
        request_bytes=case.request_bytes,
        response_bytes=case.response_bytes,
        expected=case.expected,
        expected_exception=case.expected_exception,
        metadata=case.metadata,
    )
    assert applicable_override(explicit, load_overrides(OVERRIDES_PATH)) is None
