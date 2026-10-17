"""Tests for the market-statistic and intraday fund-flow helpers."""

from __future__ import annotations

import pytest

from eltdx import TdxClient
from eltdx.helpers.market import classify_fund_flow
from eltdx.models import LegacyQuote, TradePage, TradeTick


def _legacy_quote(code: str, *, last_price: float, open_price: float,
                  high_price: float, low_price: float, amount: float = 0.0,
                  total_hand: int = 0) -> LegacyQuote:
    return LegacyQuote(
        exchange="sh",
        market_id=1,
        code=code,
        active1=0,
        last_price=last_price,
        pre_close_price=last_price,
        open_price=open_price,
        high_price=high_price,
        low_price=low_price,
        server_time_raw=0,
        unknown_after_time_raw=0,
        total_hand=total_hand,
        current_hand=0,
        amount=amount,
        amount_raw=0,
        inside_dish=0,
        outer_disc=0,
        unknown_after_outer_raw=0,
        open_amount_raw=0,
        open_amount_yuan=0.0,
        buy_levels=(),
        sell_levels=(),
        trading_status_raw=0,
        tail_metrics_raw=(0, 0, 0, 0),
        rise_speed_raw=None,
        active2=None,
    )


def test_market_stat_decodes_statistic_indices() -> None:
    quotes = [
        _legacy_quote("880005", last_price=300.0, open_price=200.0,
                      high_price=550.0, low_price=50.0, amount=9_000.0, total_hand=123),
        _legacy_quote("880001", last_price=5.0, open_price=5.0, high_price=5.0, low_price=5.0),
        _legacy_quote("880006", last_price=12.0, open_price=4.0, high_price=12.0, low_price=4.0),
    ]

    class FakeTransport:
        def execute(self, command: int, payload=None):
            assert command == 0x053E
            assert payload["codes"] == ["sh880005", "sh880001", "sh880006"]
            return quotes

    stat = TdxClient(transport=FakeTransport()).helpers.market_stat()

    assert stat.up_count == 3000
    assert stat.down_count == 2000
    assert stat.neutral_count == 500
    assert stat.total_count == 5500
    assert stat.suspended_count == 0
    assert stat.total_amount == 9_000.0
    assert stat.total_volume == 123.0
    assert stat.total_market_cap == 5.0 * 1e10
    assert stat.limit_up_count == 120
    assert stat.limit_down_count == 40


def test_market_stat_suspended_count_is_derived() -> None:
    quotes = [
        _legacy_quote("880005", last_price=100.0, open_price=50.0,
                      high_price=200.0, low_price=10.0),
        _legacy_quote("880001", last_price=1.0, open_price=1.0, high_price=1.0, low_price=1.0),
        _legacy_quote("880006", last_price=1.0, open_price=1.0, high_price=1.0, low_price=1.0),
    ]

    class FakeTransport:
        def execute(self, command: int, payload=None):
            return quotes

    stat = TdxClient(transport=FakeTransport()).helpers.market_stat()

    assert stat.up_count == 1000
    assert stat.down_count == 500
    assert stat.neutral_count == 100
    assert stat.total_count == 2000
    assert stat.suspended_count == 400


def test_market_stat_raises_when_server_returns_nothing() -> None:
    class FakeTransport:
        def execute(self, command: int, payload=None):
            return []

    with pytest.raises(RuntimeError):
        TdxClient(transport=FakeTransport()).helpers.market_stat()


def _trade_tick(side: str, price: float, volume: int, *, event_kind: str = "trade") -> TradeTick:
    return TradeTick(
        index=1,
        absolute_index=1,
        time_minutes=9 * 60 + 30,
        time_label="09:30",
        trade_datetime=None,
        price=price,
        price_milli=int(price * 1000),
        volume=volume,
        order_count=1,
        status_raw=0,
        side=side,
        price_delta_raw=0,
        price_acc_raw=0,
        event_kind=event_kind,
    )


def test_classify_fund_flow_buckets_by_amount_and_side() -> None:
    ticks = [
        # amount = price * volume * 100
        _trade_tick("buy", 100.0, 200),    # 2_000_000 -> super_in
        _trade_tick("sell", 100.0, 50),    # 500_000   -> large_out
        _trade_tick("buy", 100.0, 10),     # 100_000   -> medium_in
        _trade_tick("sell", 100.0, 1),     # 10_000    -> small_out
        _trade_tick("neutral", 100.0, 999),  # skipped
        _trade_tick("buy", 100.0, 999, event_kind="auction_snapshot"),  # skipped
    ]

    flow = classify_fund_flow(ticks)

    assert flow.super_in == 2_000_000.0
    assert flow.large_out == 500_000.0
    assert flow.medium_in == 100_000.0
    assert flow.small_out == 10_000.0
    assert flow.large_in == 0.0
    assert flow.medium_out == 0.0
    assert flow.small_in == 0.0
    assert flow.super_out == 0.0
    assert flow.main_net_inflow == 2_000_000.0 - 500_000.0
    assert flow.total_net_inflow == (2_000_000.0 + 100_000.0) - (500_000.0 + 10_000.0)


def test_today_fund_flow_reads_all_today_ticks() -> None:
    page = TradePage(
        exchange="sz",
        market_id=0,
        code="000001",
        start=0,
        request_count=2,
        ticks=(
            _trade_tick("buy", 100.0, 200),
            _trade_tick("sell", 100.0, 1),
        ),
    )

    class FakeTransport:
        def execute(self, command: int, payload=None):
            assert command == 0x0FC5
            # ``all_today`` pages until the server returns an empty page.
            return page if payload["start"] == 0 else _empty_page()

    flow = TdxClient(transport=FakeTransport()).helpers.today_fund_flow("sz000001")

    assert flow.super_in == 2_000_000.0
    assert flow.small_out == 10_000.0
    assert flow.total_net_inflow == 2_000_000.0 - 10_000.0


def _empty_page() -> TradePage:
    return TradePage(
        exchange="sz",
        market_id=0,
        code="000001",
        start=0,
        request_count=0,
        ticks=(),
    )


def test_today_fund_flow_accepts_transport_returning_bare_tick_list() -> None:
    # ``TradeApi.all_today`` passes through a payload without a ``ticks``
    # attribute unchanged, so a plain tick list keeps custom transports working.
    class FakeTransport:
        def execute(self, command: int, payload=None):
            assert command == 0x0FC5
            return [_trade_tick("buy", 50.0, 100)]

    flow = TdxClient(transport=FakeTransport()).helpers.today_fund_flow("sz000001")

    assert flow.large_in == 50.0 * 100 * 100.0


def test_today_fund_flow_returns_zero_when_no_ticks() -> None:
    class FakeTransport:
        def execute(self, command: int, payload=None):
            return _empty_page()

    flow = TdxClient(transport=FakeTransport()).helpers.today_fund_flow("sz000001")

    assert flow.total_net_inflow == 0.0
    assert flow.main_net_inflow == 0.0