from __future__ import annotations

from datetime import date, datetime
from types import SimpleNamespace

from eltdx import HelperApi
from eltdx.models import (
    CategoryQuotePage,
    CategoryQuoteRecord,
    HandshakeInfo,
    QuoteLevel,
    QuoteSnapshot,
    SecurityCode,
)


def _security(code: str, name: str) -> SecurityCode:
    return SecurityCode(
        exchange=code[:2],
        market_id={"sz": 0, "sh": 1, "bj": 2}[code[:2]],
        code=code[2:],
        name=name,
        multiple=1,
        decimal=2,
        previous_close_price=9.0,
        volume_ratio_base=0.0,
        unknown0_raw=b"",
        previous_close_raw=b"",
        unknown3_raw=b"",
        category="a_share",
        category_reason="test",
        board="main",
        board_reason="test",
    )


def _quote(code: str) -> QuoteSnapshot:
    return QuoteSnapshot(
        exchange=code[:2],
        market_id={"sz": 0, "sh": 1, "bj": 2}[code[:2]],
        code=code[2:],
        active1=0,
        last_price=10.0,
        pre_close_price=9.0,
        open_price=9.5,
        high_price=10.0,
        low_price=9.0,
        time_raw=0,
        unknown_after_time_raw=0,
        total_hand=100,
        current_hand=10,
        amount=100_000.0,
        amount_raw=0,
        inside_dish=0,
        outer_disc=0,
        unknown_after_outer_raw=0,
        open_amount_raw=0,
        open_amount_yuan=95_000.0,
        buy_levels=(QuoteLevel(10.0, 100, 0),),
        sell_levels=(),
        tail_raw=b"",
    )


class _Codes:
    rows = (_security("sz000001", "平安银行"), _security("sh600000", "ST浦发"))

    def latest_stock_list(self, market=None):
        return [row for row in self.rows if market is None or row.exchange == market]

    def latest_st(self, market=None):
        return [row for row in self.latest_stock_list(market) if row.name.startswith("ST")]

    def all_a_shares(self):
        return [row.full_code for row in self.rows]

    def all(self, market):
        return self.latest_stock_list(market)


class _Quotes:
    def get_snapshots(self, codes):
        return [_quote(code) for code in ([codes] if isinstance(codes, str) else codes)]

    def get_depth(self, codes):
        return type("Depth", (), {"records": ()})()

    def list_by_category(self, category, **kwargs):
        records = (
            CategoryQuoteRecord(
                exchange="sz",
                market_id=0,
                code="000001",
                active1=0,
                active2=0,
                last_price=10.0,
                pre_close_price=9.0,
                open_price=9.5,
                high_price=10.0,
                low_price=9.0,
                server_time_raw=0,
                neg_price_raw=0,
                total_hand=100,
                current_hand=10,
                amount=100_000.0,
                amount_raw=0,
                inside_dish=0,
                outer_disc=0,
                after_outer_raw=0,
                open_amount_raw=0,
                open_amount=95_000.0,
                bid1=10.0,
                ask1=10.1,
                bid_vol1=100,
                ask_vol1=50,
                status_or_sort_raw=0,
                rise_speed_raw=0,
                rise_speed=0.0,
                short_turnover_raw=0,
                short_turnover=0.0,
                min2_amount=0.0,
                opening_rush_raw=0,
                opening_rush=1.5,
                extra_pair_raw=b"",
                vol_rise_speed=0.0,
                depth=0.0,
                extra_meta_raw=b"",
                tail_raw=b"",
            ),
        )
        return CategoryQuotePage(6, 14, kwargs.get("start", 0), 1, 0, 0, 0, records)


class _Client:
    codes = _Codes()
    quotes = _Quotes()
    bars = None
    corporate = None

    def __init__(self):
        self.session = type("Session", (), {"handshake": lambda self: _handshake()})()
        self.workdays = type("Workdays", (), {"previous_workday": lambda self, value: date(2026, 8, 19)})()


def _handshake():
    return HandshakeInfo(
        server_datetime=datetime(2026, 8, 20, 10, 0),
        session_minutes_1=(),
        session_minutes_2=(),
        server_date_1=date(2026, 8, 20),
        server_date_2=date(2026, 8, 20),
        server_name="test",
        product_tag="test",
        unknown_time_1_raw=None,
        unknown_time_2_raw=None,
        flags_raw=b"",
        tail_control_raw=b"",
        raw_payload=b"",
    )


def test_common_a_share_helpers_and_rank() -> None:
    helper = HelperApi(_Client())
    assert [item.full_code for item in helper.latest_st()] == ["sh600000"]
    limits = helper.daily_price_limits(["sz000001"], trade_date=date(2026, 8, 20))
    assert limits.rows[0].limit_status == "missing_pre_close"
    rank = helper.realtime_rank(count=1)
    assert rank.count == 1
    assert rank.rows[0].opening_rush == 1.5


def test_daily_price_limits_apply_same_day_ex_right_event_and_use_current_st_rule() -> None:
    from eltdx.helpers.core import _build_daily_price_limit

    security = _security("sh600000", "ST浦发")
    event = SimpleNamespace(c1_value=0.0, c2_value=25.0, c3_value=0.0, c4_value=2.0)
    row = _build_daily_price_limit(
        "sh600000",
        date(2026, 8, 20),
        date(2026, 8, 19),
        security,
        9.0,
        (event,),
    )

    # (9 - (-5)) / 1.2 = 11.666..., then the current main-board ST rule applies +/-10%.
    assert round(row.pre_close or 0.0, 6) == 11.666667
    assert row.limit_ratio_pct == 10.0
    assert row.limit_up_price == 12.83
    assert row.limit_down_price == 10.5
    assert row.limit_rule == "st_main_10pct"
    assert row.pre_close_source == "kline_unadjusted+capital_changes"
