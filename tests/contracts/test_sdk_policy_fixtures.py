"""Frozen policy contracts for the planned standalone Rust SDK."""

from __future__ import annotations

import json
from pathlib import Path


ROOT = Path(__file__).parents[2]
POLICY_ROOT = ROOT / "tests" / "fixtures" / "sdk" / "policy"


def _policy(name: str) -> dict:
    document = json.loads((POLICY_ROOT / name).read_text(encoding="utf-8"))
    assert document["schema_version"] == 1
    assert document["source_revision"]
    assert document["cases"]
    return document


def _cases(name: str) -> dict[str, dict]:
    rows = _policy(name)["cases"]
    cases = {case["case_id"]: case for case in rows}
    assert len(cases) == len(rows)
    for case in rows:
        assert case["python_compat"]
        assert case["rust"]
        assert case["evidence"]["level"]
    return cases


def test_price_limit_profiles_remain_unknown_without_dated_rule_evidence() -> None:
    document = _policy("price_limits.json")
    cases = {case["case_id"]: case for case in document["cases"]}
    assert document["decision"]["rounding"]["python_daily_and_shortline"] == "round(value + 1e-9, 2)"
    assert document["decision"]["rounding"]["python_mac"] == "round(value + 1e-5, 2)"
    assert document["decision"]["rounding"]["rust_approved_profile_must_define_tick_and_rounding_mode"] is True
    assert set(cases) == {
        "main_board_st",
        "registration_board_st",
        "unassigned_30_segment",
        "beijing_ipo_day_two",
        "index_identity",
        "delisting_st",
    }
    for case in cases.values():
        assert case["evidence"]["rule_profile_approved"] is False
        assert case["evidence"]["dated_authoritative_rule_sources"] == []
        assert case["rust"]["limit_status"] == "unknown_rule"
        assert case["rust"]["limit_up_cny"] is None
        assert case["rust"]["limit_down_cny"] is None
        assert set(case["python_compat"]) == {"daily_price_limits", "shortline", "mac"}


def test_code_identity_and_precision_prefetch_are_separate_contracts() -> None:
    cases = _cases("normalization.json")
    assert {
        "bare_sh_fund",
        "bare_beijing_code",
        "integer_alias",
        "market_aliases_and_host_default",
        "extension_identity",
    } <= set(cases)
    for case_id in ("bare_sh_fund", "bare_beijing_code"):
        case = cases[case_id]
        assert case["python_compat"]["normalized_security_id"] == case["rust"]["security_id"]
        assert case["python_compat"]["decimal_prefetch_markets"] == ["bj", "sh", "sz"]
        assert case["rust"]["metadata_market"] == case["rust"]["security_id"][:2]
    assert cases["integer_alias"]["python_compat"]["normalized_security_id"] == "sh600000"
    assert cases["integer_alias"]["rust"]["string_constructor_accepts_non_string"] is False
    aliases = cases["market_aliases_and_host_default"]
    assert aliases["python_compat"]["market_aliases"] == ["sz", "sh", "bj"]
    assert aliases["python_compat"]["hosts"] == ["example.com:7709", "[::1]:7709"]
    assert aliases["rust"]["string_market_alias_constructor"] is False
    assert aliases["rust"]["explicit_socket_address_has_port"] is True
    assert cases["extension_identity"]["rust"]["security_id"] is None


def test_unit_contracts_keep_raw_values_and_scales_per_field() -> None:
    cases = _cases("units.json")
    assert {
        "hq_equity_tick",
        "hq_equity_kline",
        "hq_convertible_bond_kline",
        "hk_transaction",
        "unknown_amount",
    } <= set(cases)
    tick = cases["hq_equity_tick"]["rust"]
    assert tick["price_cny"] == 1255.28
    assert tick["quantity_shares"] == 2300
    assert tick["trade_amount_cny"] == 2887144.0
    assert cases["hq_equity_kline"]["rust"]["volume_lots"] == 20
    assert cases["hq_convertible_bond_kline"]["rust"]["volume_lots"] == 2
    assert cases["hk_transaction"]["rust"]["price_hkd"] == 125.528
    assert cases["unknown_amount"]["rust"]["unit"] == "unknown"
    for case in cases.values():
        assert case["rust"]["raw_value_preserved"] is True


def test_date_contracts_preserve_calendar_validity_source_zone_and_domain_periods() -> None:
    cases = _cases("dates.json")
    assert {
        "gregorian_date_is_not_market_calendar_validation",
        "hq_exchange_local_time",
        "mac_source_local_time",
        "bar_end_crosses_midnight",
        "period_eight_is_domain_specific",
        "implicit_today_source",
    } <= set(cases)
    assert cases["gregorian_date_is_not_market_calendar_validation"]["rust"]["valid"] is True
    assert cases["hq_exchange_local_time"]["rust"]["offset"] == "+08:00"
    assert cases["mac_source_local_time"]["rust"]["timezone"] == "unknown"
    assert cases["bar_end_crosses_midnight"]["rust"]["derived_date"] == "2026-10-13"
    assert cases["period_eight_is_domain_specific"]["rust"]["minutes"] == {
        "hq_parameter_13": 13,
        "ex_category_8": 3,
        "mac_five_minute_times_3": 15,
    }
    assert cases["implicit_today_source"]["rust"]["uses_host_local_date"] is False


def test_legacy_boundaries_keep_python_projection_separate_from_rust_policy() -> None:
    cases = _cases("legacy_boundaries.json")
    assert {
        "today_bulk_native_transport",
        "financial_zip_multiple_dat_names",
        "market_stat_missing_middle_identity",
        "finance_failure_cache",
        "max_pages_none",
        "request_deadline_scope",
    } <= set(cases)
    assert cases["today_bulk_native_transport"]["python_compat"]["error_message"] == (
        "trade batch only supports historical ticks"
    )
    assert cases["financial_zip_multiple_dat_names"]["python_compat"]["selected_member"] == "A.dat"
    assert cases["financial_zip_multiple_dat_names"]["rust"]["without_selector"] == "ambiguous"
    assert cases["market_stat_missing_middle_identity"]["rust"]["missing_identity"] == "sh880001"
    assert cases["finance_failure_cache"]["rust"]["failure_is_complete_cache_entry"] is False
    assert cases["max_pages_none"]["python_compat"]["adds_implicit_page_cap"] is False
    assert cases["request_deadline_scope"]["rust"]["deadline_scope"] == "whole_operation"
    assert cases["request_deadline_scope"]["python_compat"]["two_page_result"] == "success_at_about_10_seconds"
    assert cases["request_deadline_scope"]["rust"]["two_page_result"] == "timeout_at_8_seconds"
