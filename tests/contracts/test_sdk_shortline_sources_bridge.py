"""Shortline composition/source order cases for the final verification pass."""
from datetime import date, datetime
from types import SimpleNamespace

import pytest

from eltdx.helpers import shortline
from eltdx.exceptions import ShortlineIndicatorsNotReadyError

CODE = "sh" + str(600_001)
DAY = date(2024, 6, 3)


class Client(SimpleNamespace):
    pass


def test_shortline_composition_keeps_source_order_builder_and_table_metadata(monkeypatch):
    calls = []
    client = Client(helpers=SimpleNamespace(full_quotes=lambda codes: calls.append("quotes") or []))
    service = shortline.ShortlineIndicatorService(client)
    context = SimpleNamespace(ready=True, target_trade_date=DAY, previous_trade_date=date(2024, 5, 31))
    stats = SimpleNamespace(source_path="original.zip")
    monkeypatch.setattr(shortline, "_resolve_market_date_context", lambda client: calls.append("context") or context)
    service._stats_resource = lambda *args, **kwargs: (calls.append("stats") or stats, True)
    monkeypatch.setattr(shortline, "_validate_stats_resource_dates", lambda *args, **kwargs: calls.append("dates") or DAY)
    for name, label in [("_security_map", "security"), ("_recent_daily_bars_map", "bars"),
                        ("_finance_map", "finance"), ("_opening_rush_map", "rush")]:
        monkeypatch.setattr(shortline, name, lambda *args, label=label, **kwargs: calls.append(label) or {})
    monkeypatch.setattr(shortline, "_build_indicator", lambda code, **kwargs:
                        calls.append(("row", code, kwargs["context"])) or code)
    table = service.get([CODE, CODE])
    assert table.codes == (CODE,) and table.rows == (CODE,)
    assert table.stats_source_path == "original.zip" and table.stats_refreshed is True
    assert calls == ["context", "stats", "dates", "quotes", "security", "bars", "finance", "rush", ("row", CODE, context)]


def test_shortline_not_ready_stops_before_sources(monkeypatch):
    client = Client()
    service = shortline.ShortlineIndicatorService(client)
    monkeypatch.setattr(shortline, "_resolve_market_date_context", lambda client: SimpleNamespace(ready=False))
    service._stats_resource = lambda *args, **kwargs: pytest.fail("not ready")
    with pytest.raises(ShortlineIndicatorsNotReadyError, match="before the 09:25"):
        service.get(CODE)


def test_shortline_finance_batches_keep_last_record_optional_request_and_parse_errors():
    calls = []
    first = SimpleNamespace(full_code=CODE)
    last = SimpleNamespace(full_code=CODE)

    def fetch(codes):
        calls.append(codes)
        return SimpleNamespace(records=(first, last))

    client = Client(corporate=SimpleNamespace(finance_batch=fetch))
    assert shortline._finance_map(client, [CODE] * 81) == {CODE: last}
    assert [len(part) for part in calls] == [80, 1]
    client.corporate.finance_batch = lambda codes: SimpleNamespace(records=123)
    with pytest.raises(TypeError):
        shortline._finance_map(client, [CODE])

    def fail(codes):
        raise RuntimeError("optional")

    client.corporate.finance_batch = fail
    assert shortline._finance_map(client, [CODE]) == {}


def test_shortline_recent_bars_retains_filter_order_and_first_five():
    bars = [SimpleNamespace(time=datetime(2024, 6, day), volume_lots=day) for day in [3, 2, 1]]
    calls = []
    client = Client(bars=SimpleNamespace(get=lambda *args, **kwargs: calls.append(kwargs) or SimpleNamespace(bars=bars)))
    result = shortline._recent_daily_bars_map(client, [CODE], before=DAY)
    assert result[CODE] == tuple(bars[1:])
    assert calls == [dict(period="day", start=0, count=6, adjust="none")]


def test_opening_rush_does_not_scan_for_small_request_and_keeps_short_page():
    calls = []
    row = SimpleNamespace(full_code=CODE, opening_rush=1.23456789)
    client = Client(quotes=SimpleNamespace(list_by_category=lambda *args, **kwargs:
                                         calls.append(kwargs) or SimpleNamespace(records=(row,))))
    assert shortline._opening_rush_map(client, [CODE]) == {} and calls == []
    assert shortline._opening_rush_map(client, [CODE] * 80) == {CODE: 1.234568}
    assert calls == [dict(sort_by="代码", start=0, count=80, ascending=True)]


def test_shortline_scalar_rounding_missing_zero_and_ratios():
    assert shortline._round(None) is None
    assert shortline._round(1.23456789) == 1.234568
    assert shortline._tenk(1.23456789) == 12345.6789
    assert shortline._multiply(3, 1.23456789) == 3.703704
    assert shortline._safe_ratio(1, 3) == 0.333333
    assert shortline._safe_ratio_pct(1, 3) == 33.333333
    assert shortline._safe_ratio(None, 3) is None
    assert shortline._safe_ratio_pct(1, 0) is None
    assert shortline._change_pct(3, 2) == 50.0
    assert shortline._change_pct(3, 0) is None


def test_shortline_scalar_preserves_denominator_conversion_order():
    calls = []

    class Number:
        def __init__(self, name, value):
            self.name, self.value = name, value

        def __float__(self):
            calls.append(self.name)
            return self.value

    assert shortline._safe_ratio(Number("numerator", 1.0), Number("denominator", 2.0)) == 0.5
    assert calls == ["denominator", "numerator", "denominator"]


def test_shortline_volume_ratio_requires_five_and_keeps_twice_read():
    seen = []

    class Bar:
        @property
        def volume_lots(self):
            seen.append("volume")
            return 240.0

    assert shortline._open_volume_ratio(20, [Bar()] * 4) is None
    seen.clear()
    assert shortline._open_volume_ratio(20, [Bar()] * 6) == 20.0
    assert seen == ["volume"] * 10


def test_shortline_locked_amount_keeps_first_level_and_negative_values():
    quote = SimpleNamespace(buy_levels=[SimpleNamespace(price=-2.0, volume=3), object()])
    assert shortline._locked_amount(quote) == -600.0
    assert shortline._locked_amount(SimpleNamespace(buy_levels=None)) is None


def test_shortline_indicator_uses_native_composition_with_original_numeric_hooks(monkeypatch):
    context = SimpleNamespace(target_trade_date=DAY, previous_trade_date=date(2024, 5, 31))
    stat = SimpleNamespace(stats_date="20240603", free_float_shares_10k=100.0,
                           limit_stat_days=7, limit_up_count_in_stat_days=5,
                           limit_up_streak_days=3, year_limit_up_days=10, beta_60d=1.2, pe_ttm=15.0)
    stat2 = SimpleNamespace(stats_date="20240603", prev_amount_10k=10.0, prev_seal_amount_10k=2.0,
                            prev2_seal_amount_10k=3.0, prev_open_volume_hand=400.0, prev_open_amount_10k=5.0)
    quote = SimpleNamespace(last_price=11.0, open_price=10.0, pre_close_price=10.0,
                            open_amount_yuan=20000.0, amount=100000.0,
                            buy_levels=[SimpleNamespace(price=11.0, volume=1000)])
    stats = SimpleNamespace(row=lambda *args: (stat, stat2))
    monkeypatch.setattr(shortline, "_limit_status", lambda *args: "sealed")
    row = shortline._build_indicator(CODE, quote=quote, security=None,
        recent_daily_bars=[SimpleNamespace(volume_lots=240)] * 5, finance=None,
        opening_rush=2.0, stats=stats, context=context)
    assert row.free_float_shares == 1000000.0
    assert row.prev_amount == 100000.0 and row.open_volume_hand == 20.0
    assert row.ladder_level == 3 and row.open_volume_ratio == 20.0
    assert row.opening_rush == 2.0 and row.limit_board_text == "7天5板"


def test_shortline_legacy_limit_policy_preserves_st_before_board_and_new_names():
    assert shortline._price_limit_ratio("sh688001", "ST example") == 5.0
    assert shortline._price_limit_ratio("bj430001", "ST example") == 5.0
    assert shortline._price_limit_ratio(CODE, "N example") is None
    assert shortline._price_limit_ratio("sh688001", "ordinary") == 20.0
    assert shortline._price_limit_ratio(CODE, "ordinary") == 10.0


def test_shortline_legacy_limit_status_sealed_touched_and_missing():
    assert shortline._limit_status(CODE, None, None) == "unknown"
    quote = SimpleNamespace(pre_close_price=10.0, last_price=11.0, high_price=11.0,
                            buy_levels=[SimpleNamespace(price=11.0, volume=0)])
    assert shortline._limit_status(CODE, quote, None) == "sealed"
    quote.last_price = 10.0
    assert shortline._limit_status(CODE, quote, None) == "touched"
    quote.high_price = 10.0
    assert shortline._limit_status(CODE, quote, None) == "none"
    assert shortline._limit_status(CODE, quote, "N example") == "none"
    quote.pre_close_price = 0
    assert shortline._limit_status(CODE, quote, None) == "none"


def test_shortline_board_progress_preserves_prior_date_and_positive_floor():
    stat = SimpleNamespace(limit_up_streak_days=3)
    assert shortline._ladder_level(stat, stats_date=DAY, target=DAY,
                                  previous=date(2024, 5, 31), limit_status="sealed") == 3
    assert shortline._ladder_level(stat, stats_date=date(2024, 5, 31), target=DAY,
                                  previous=date(2024, 5, 31), limit_status="sealed") == 4
    assert shortline._ladder_level(stat, stats_date=DAY, target=DAY,
                                  previous=date(2024, 5, 31), limit_status="touched") is None
    assert shortline._limit_board_text("7", 5) == "7天5板"
    assert shortline._limit_board_text(0, 5) is None


def test_shortline_price_tolerance_and_nan_keep_legacy_comparisons():
    assert shortline._price_close(1, 1.005, tolerance=0.0051)
    assert not shortline._price_close(None, 1)
    assert not shortline._price_close(float("nan"), 1)
    assert shortline._price_at_or_above(0.995, 1)
    assert not shortline._price_at_or_above(None, 1)


def test_ladder_filters_and_orders_before_validating_count():
    from eltdx.helpers.core import HelperApi

    client = Client()
    helper = HelperApi(client)
    rows = [SimpleNamespace(full_code="sh600003", limit_status="touched", ladder_level=5, seal_amount=1),
            SimpleNamespace(full_code=CODE, limit_status="sealed", ladder_level=3, seal_amount=100),
            SimpleNamespace(full_code="sh600002", limit_status="sealed", ladder_level=3, seal_amount=200)]
    calls = []
    helper.shortline_indicators = lambda codes: calls.append(codes) or SimpleNamespace(rows=rows, target_trade_date=DAY)
    result = helper.limit_ladder([CODE], include_touched=True, count=2)
    assert [row.full_code for row in result.rows] == ["sh600003", "sh600002"]
    with pytest.raises(ValueError, match="count must be positive"):
        helper.limit_ladder([CODE], count=0)
    assert len(calls) == 2


def test_shortline_date_context_uses_latest_handshake_date_and_server_ready_time():
    handshake = SimpleNamespace(server_date_1=DAY, server_date_2=date(2024, 6, 4),
                                server_datetime=datetime(2024, 6, 4, 9, 24))
    client = Client(transport=SimpleNamespace(last_handshake=handshake),
                    workdays=SimpleNamespace(previous_workday=lambda target: DAY))
    context = shortline._resolve_market_date_context(client)
    assert context.target_trade_date == date(2024, 6, 4) and context.previous_trade_date == DAY
    assert not context.ready
    handshake.server_datetime = datetime(2024, 6, 4, 9, 25)
    assert shortline._resolve_market_date_context(client).ready
    handshake.server_datetime = datetime(2024, 6, 3, 9, 24)
    assert shortline._resolve_market_date_context(client).ready


def test_shortline_handshake_candidates_precede_session_fallback():
    expected = object()
    client = Client(transport=SimpleNamespace(_transports=[SimpleNamespace(last_handshake=None),
                                                         SimpleNamespace(last_handshake=expected)]),
                    session=SimpleNamespace(handshake=lambda: pytest.fail("fallback")))
    assert shortline._client_handshake_info(client) is expected
    client.transport._transports = []
    client.session.handshake = lambda: expected
    assert shortline._client_handshake_info(client) is expected


def test_shortline_row_date_prefers_second_stat_and_preserves_parse_cause():
    assert shortline._row_stats_date(SimpleNamespace(stats_date="invalid"),
                                    SimpleNamespace(stats_date="20240603")) == DAY
    assert shortline._row_stats_date(SimpleNamespace(stats_date="20240603"), None) == DAY
    assert shortline._row_stats_date(None, None) is None
    with pytest.raises(shortline.ResourceFormatError) as caught:
        shortline._row_stats_date(None, SimpleNamespace(stats_date="invalid"))
    assert isinstance(caught.value.__cause__, ValueError)


def test_shortline_mapping_keeps_original_last_duplicate_and_missing_codes():
    first = SimpleNamespace(full_code=CODE)
    last = SimpleNamespace(full_code=CODE)
    assert shortline._by_full_code([first, SimpleNamespace(), last]) == {CODE: last}
    assert shortline._by_full_code(None) == {}


def test_shortline_market_context_preserves_missing_handshake_errors():
    with pytest.raises(shortline.TdxStatsDateError, match="without a TDX handshake"):
        shortline._resolve_market_date_context(Client())
    handshake = SimpleNamespace(server_datetime=None)
    client = Client(transport=SimpleNamespace(last_handshake=handshake))
    with pytest.raises(shortline.TdxStatsDateError, match="usable server datetime"):
        shortline._resolve_market_date_context(client)
    handshake.server_datetime = datetime(2024, 6, 3, 10, 0)
    with pytest.raises(shortline.TdxStatsDateError, match="usable target trading day"):
        shortline._resolve_market_date_context(client)
    handshake.server_date_1 = DAY
    client.workdays = SimpleNamespace(previous_workday=lambda day: None)
    with pytest.raises(shortline.TdxStatsDateError, match="previous trading day for 2024-06-03"):
        shortline._resolve_market_date_context(client)
