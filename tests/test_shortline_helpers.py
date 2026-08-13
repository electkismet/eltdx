from __future__ import annotations

from dataclasses import fields, replace
from datetime import date, datetime

import pytest

from eltdx import HelperApi, ShortlineIndicator
from eltdx.exceptions import (
    ResourceFormatError,
    ShortlineIndicatorsNotReadyError,
    TdxStatsDateError,
)
from eltdx.models import (
    HandshakeInfo,
    QuoteLevel,
    QuoteSnapshot,
    SecurityCode,
    TdxStat2Row,
    TdxStatRow,
    TdxStatsResource,
)


METRIC_FIELDS = {
    "beta_60d",
    "pe_ttm",
    "free_float_shares",
    "prev_amount",
    "prev_seal_amount",
    "prev2_seal_amount",
    "prev_open_volume_hand",
    "prev_open_amount",
    "limit_stat_days",
    "limit_up_count_in_stat_days",
    "limit_up_streak_days",
    "year_limit_up_days",
    "free_float_market_value",
    "open_turnover_z",
    "open_prev_amount_ratio",
    "auction_prev_volume_ratio",
    "open_prev_seal_ratio",
    "seal_to_float_ratio",
    "seal_prev_ratio",
    "limit_board_text",
    "ladder_level",
}


def test_shortline_indicator_model_has_exactly_21_metric_fields() -> None:
    model_fields = {field.name for field in fields(ShortlineIndicator)}

    assert len(METRIC_FIELDS) == 21
    assert METRIC_FIELDS <= model_fields


def test_shortline_indicators_validate_resource_options_before_network_calls() -> None:
    client = FakeClient(
        resources=FakeResources(_stats("20260612")),
        handshake=_handshake(10, 0),
    )
    helpers = HelperApi(client)

    with pytest.raises(ValueError, match="stats_path"):
        helpers.shortline_indicators("sz000001", stats_path="")
    with pytest.raises(ValueError, match="refresh_stats"):
        helpers.shortline_indicators("sz000001", refresh_stats=1)  # type: ignore[arg-type]

    assert client.resources.calls == 0
    assert client.quote_calls == 0


def test_shortline_indicators_use_same_day_previous_columns_and_cache() -> None:
    resources = FakeResources(_stats("20260612"))
    client = FakeClient(resources=resources, handshake=_handshake(9, 30))
    helpers = HelperApi(client)

    first = helpers.shortline_indicators("sz000001")
    second = helpers.shortline_indicators("sz000001")
    refreshed = helpers.shortline_indicators("sz000001", refresh_stats=True)
    helpers.clear_cache()
    after_clear = helpers.shortline_indicators("sz000001")

    assert first.stats_date == date(2026, 6, 12)
    assert first.stats_refreshed is True
    assert second.stats_refreshed is False
    assert refreshed.stats_refreshed is True
    assert after_clear.stats_refreshed is True
    assert resources.calls == 3
    row = first.rows[0]
    assert row.alignment_status == "same_day"
    assert row.limit_status == "sealed"
    assert row.beta_60d == 1.2
    assert row.pe_ttm == 15.0
    assert row.free_float_shares == 1_000_000.0
    assert row.prev_amount == 100_000.0
    assert row.prev_seal_amount == 20_000.0
    assert row.prev2_seal_amount == 30_000.0
    assert row.prev_open_volume_hand == 400.0
    assert row.prev_open_amount == 50_000.0
    assert row.limit_stat_days == 7
    assert row.limit_up_count_in_stat_days == 5
    assert row.limit_up_streak_days == 4
    assert row.year_limit_up_days == 13
    assert row.free_float_market_value == 10_000_000.0
    assert row.open_turnover_z == 0.2
    assert row.open_prev_amount_ratio == 20.0
    assert row.auction_prev_volume_ratio == 0.05
    assert row.open_prev_seal_ratio == 100.0
    assert row.seal_to_float_ratio == 10.0
    assert row.seal_prev_ratio == 50.0
    assert row.limit_board_text == "7天5板"
    assert row.ladder_level == 4


def test_shortline_indicators_align_previous_trading_day_columns() -> None:
    resources = FakeResources(_stats("20260611"))
    client = FakeClient(resources=resources, handshake=_handshake(10, 0))

    table = HelperApi(client).shortline_indicators("000001")

    row = table.rows[0]
    assert row.alignment_status == "previous_trading_day"
    assert row.prev_amount == 110_000.0
    assert row.prev_seal_amount == 25_000.0
    assert row.prev2_seal_amount == 20_000.0
    assert row.prev_open_volume_hand == 500.0
    assert row.prev_open_amount == 60_000.0
    assert row.limit_board_text == "8天6板"
    assert row.ladder_level == 5


def test_shortline_indicators_reject_stale_or_disagreeing_resource_dates() -> None:
    stale = FakeClient(
        resources=FakeResources(_stats("20260610")),
        handshake=_handshake(10, 0),
    )
    with pytest.raises(TdxStatsDateError, match="not usable"):
        HelperApi(stale).shortline_indicators("sz000001")

    mismatched = _stats("20260612")
    mismatched = TdxStatsResource(
        stat=mismatched.stat,
        stat2={(0, "000001"): replace(next(iter(mismatched.stat2.values())), stats_date="20260611")},
        source_path=mismatched.source_path,
    )
    client = FakeClient(resources=FakeResources(mismatched), handshake=_handshake(10, 0))
    with pytest.raises(ResourceFormatError, match="dates disagree"):
        HelperApi(client).shortline_indicators("sz000001")


def test_shortline_indicators_reject_low_dominant_date_coverage() -> None:
    stats = _stats("20260612")
    other = replace(next(iter(stats.stat.values())), code="000002", stats_date="20260611")
    stats = TdxStatsResource(
        stat={**stats.stat, other.key: other},
        stat2=stats.stat2,
        source_path=stats.source_path,
    )
    client = FakeClient(resources=FakeResources(stats), handshake=_handshake(10, 0))

    with pytest.raises(ResourceFormatError, match="coverage is too low"):
        HelperApi(client).shortline_indicators("sz000001")


def test_shortline_indicators_fail_closed_before_auction_ready() -> None:
    resources = FakeResources(_stats("20260611"))
    client = FakeClient(resources=resources, handshake=_handshake(9, 24))

    with pytest.raises(ShortlineIndicatorsNotReadyError, match="09:25"):
        HelperApi(client).shortline_indicators("sz000001")

    assert resources.calls == 0
    assert client.quote_calls == 0


class FakeResources:
    def __init__(self, stats: TdxStatsResource) -> None:
        self.stats = stats
        self.calls = 0

    def read_stats(self, path: str = "zhb.zip") -> TdxStatsResource:
        assert path == "zhb.zip"
        self.calls += 1
        return self.stats


class FakeWorkdays:
    def normalize(self, value=None) -> date:
        if isinstance(value, date):
            return value
        return date(2026, 6, 12)

    def previous_workday(self, value=None, *, include_self: bool = False) -> date:
        assert self.normalize(value) == date(2026, 6, 12)
        assert include_self is False
        return date(2026, 6, 11)


class FakeSession:
    def __init__(self, handshake: HandshakeInfo) -> None:
        self._handshake = handshake

    def handshake(self) -> HandshakeInfo:
        return self._handshake


class FakeClient:
    def __init__(self, *, resources: FakeResources, handshake: HandshakeInfo) -> None:
        self.resources = resources
        self.session = FakeSession(handshake)
        self.transport = None
        self.workdays = FakeWorkdays()
        self.quote_calls = 0
        self.quotes = self
        self.codes = self
        self.helpers = self

    def get_snapshots(self, codes):
        self.quote_calls += 1
        return [_quote()]

    def full_quotes(self, codes):
        return self.get_snapshots(codes)

    def all(self, market: str):
        return [_security()] if market == "sz" else []


def _handshake(hour: int, minute: int) -> HandshakeInfo:
    return HandshakeInfo(
        server_datetime=datetime(2026, 6, 12, hour, minute),
        session_minutes_1=(),
        session_minutes_2=(),
        server_date_1=date(2026, 6, 12),
        server_date_2=date(2026, 6, 12),
        server_name="test",
        product_tag="test",
        unknown_time_1_raw=None,
        unknown_time_2_raw=None,
        flags_raw=b"",
        tail_control_raw=b"",
        raw_payload=b"",
    )


def _stats(stats_date: str) -> TdxStatsResource:
    stat = TdxStatRow(
        market_id=0,
        code="000001",
        stats_date=stats_date,
        beta_60d=1.2,
        pe_ttm=15.0,
        free_float_shares_10k=100.0,
        year_limit_up_days=13,
        limit_stat_days=7,
        limit_up_count_in_stat_days=5,
        limit_up_streak_days=4,
    )
    stat2 = TdxStat2Row(
        market_id=0,
        code="000001",
        stats_date=stats_date,
        amount_10k=11.0,
        seal_amount_10k=2.5,
        prev_amount_10k=10.0,
        prev_seal_amount_10k=2.0,
        prev2_amount_10k=9.0,
        prev2_seal_amount_10k=3.0,
        open_volume_hand=500.0,
        prev_open_volume_hand=400.0,
        open_amount_10k=6.0,
        prev_open_amount_10k=5.0,
    )
    return TdxStatsResource(
        stat={stat.key: stat},
        stat2={stat2.key: stat2},
        source_path="tdx://zhb.zip",
    )


def _quote() -> QuoteSnapshot:
    return QuoteSnapshot(
        exchange="sz",
        market_id=0,
        code="000001",
        active1=0,
        last_price=10.0,
        pre_close_price=9.09,
        open_price=10.0,
        high_price=10.0,
        low_price=9.5,
        time_raw=0,
        unknown_after_time_raw=0,
        total_hand=1000,
        current_hand=10,
        amount=1_000_000.0,
        amount_raw=0,
        inside_dish=0,
        outer_disc=0,
        unknown_after_outer_raw=0,
        open_amount_raw=0,
        open_amount_yuan=20_000.0,
        buy_levels=(QuoteLevel(price=10.0, volume=1000, price_delta_raw=0),),
        sell_levels=(),
        tail_raw=b"",
    )


def _security() -> SecurityCode:
    return SecurityCode(
        exchange="sz",
        market_id=0,
        code="000001",
        name="平安银行",
        multiple=1,
        decimal=2,
        previous_close_price=9.09,
        volume_ratio_base=0.0,
        unknown0_raw=b"",
        previous_close_raw=b"",
        unknown3_raw=b"",
        category="a_share",
        category_reason="test",
        board="main",
        board_reason="test",
    )
