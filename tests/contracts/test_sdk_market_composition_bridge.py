"""Auction/market compatibility sources; run only in final validation phase."""
from datetime import date
from types import SimpleNamespace

import pytest

from eltdx.helpers import core, market

CODE = "sh" + str(600_001)
DAY = date(2024, 6, 3)


class Client(SimpleNamespace):
    pass


def test_auction_current_preserves_source_order_original_objects_and_rounding():
    calls = []
    series = object()
    match = SimpleNamespace(price=11.0, volume=100, trade_amount_yuan=110000.126)
    client = Client(
        session=Client(handshake=lambda: calls.append("handshake") or Client(server_date_1=DAY)),
        auctions=Client(series=lambda code, day: calls.append(("series", code, day)) or series),
        trades=Client(opening_match_today=lambda code: calls.append(("opening", code)) or match),
        quotes=Client(get_snapshots=lambda code: calls.append(("quote", code)) or [Client(pre_close_price=10.0)]),
    )
    result = core.HelperApi(client).auction_data(CODE)
    assert calls == ["handshake", ("series", CODE, None), ("opening", CODE), ("quote", CODE)]
    assert result.series is series and result.snapshot_0925 is match
    assert result.trading_date == DAY and result.open_amount == 110000.13
    assert result.open_change_pct == 10.0


def test_auction_history_uses_first_formal_match_and_page_price_base():
    calls = []
    first = Client(price=12.0, volume=3, trade_amount_yuan=3600.0)
    second = Client(price=13.0, volume=4, trade_amount_yuan=5200.0)
    page = Client(opening_matches=(first, second), price_base_raw_f32=10.12349)
    client = Client(workdays=Client(normalize=lambda day: calls.append(("normalize", day)) or DAY),
                    auctions=Client(series=lambda code, day: calls.append(("series", day)) or "series"),
                    trades=Client(all_history=lambda code, day: calls.append(("history", day)) or page))
    result = core.HelperApi(client).auction_data(CODE, "day")
    assert result.snapshot_0925 is first and result.pre_close_price == 10.123
    assert calls == [("normalize", "day"), ("series", DAY), ("history", DAY)]


def test_auction_explicit_reference_and_disabled_sources_are_preserved():
    client = Client(workdays=Client(normalize=lambda day: DAY))
    result = core.HelperApi(client).auction_data(CODE, DAY, include_series=False,
        include_snapshot=False, include_quote=True, pre_close_price=-1.0)
    assert result.pre_close_price == -1.0
    assert result.snapshot_0925 is None and result.series is None
    assert result.open_price is None and result.open_change_pct is None


def test_auction_history_can_fetch_reference_without_snapshot():
    page = Client(opening_matches=None, price_base_raw_f32=10.0)
    client = Client(workdays=Client(normalize=lambda day: day),
                    trades=Client(all_history=lambda code, day: page))
    result = core.HelperApi(client).auction_data(CODE, DAY, include_series=False, include_snapshot=False)
    assert result.pre_close_price == 10.0 and result.snapshot_0925 is None
    page.price_base_raw_f32 = 0.0
    assert core.HelperApi(client).auction_data(CODE, DAY, include_series=False,
                                              include_snapshot=False).pre_close_price is None


def test_auction_percentage_preserves_custom_numeric_operators():
    seen = []

    class Number:
        def __bool__(self):
            seen.append("truth")
            return True

        def __sub__(self, other):
            seen.append("sub")
            return self

        def __truediv__(self, other):
            seen.append("divide")
            return self

        def __mul__(self, other):
            seen.append(("multiply", other))
            return 23

    assert core._pct(Number(), Number()) == 23
    assert seen == ["truth", "sub", "divide", ("multiply", 100.0)]
    assert core._pct(10, 0) is None and core._pct(None, 10) is None


def test_market_date_keeps_both_reads_and_requires_server_date():
    seen = []

    class Handshake:
        @property
        def server_date_1(self):
            seen.append(1)
            return DAY

        @property
        def server_date_2(self):
            seen.append(2)
            return date(2024, 6, 4)

    client = Client(session=Client(handshake=lambda: Handshake()))
    assert core.HelperApi(client)._current_market_date() == date(2024, 6, 4)
    assert seen == [1, 2]
    client.session.handshake = lambda: Client()
    with pytest.raises(RuntimeError, match="current market date"):
        core.HelperApi(client)._current_market_date()


def test_first_quote_preserves_scalar_sequence_and_bytes_behavior():
    current = []
    client = Client(quotes=Client(get_snapshots=lambda code: current))
    helper = core.HelperApi(client)
    assert helper._first_quote(CODE) is None
    item = object()
    current = [item, object()]
    assert helper._first_quote(CODE) is item
    current = b"data"
    assert helper._first_quote(CODE) is current
    current = object()
    assert helper._first_quote(CODE) is current


def test_snapshot_batches_keep_80_chunks_and_ignore_scalar_responses():
    calls = []
    item = object()

    def get(codes):
        calls.append(list(codes))
        return [item] if len(calls) == 1 else object()

    client = Client(quotes=Client(get_snapshots=get))
    assert core.HelperApi(client)._snapshot_batches([CODE] * 81) == [item]
    assert [len(codes) for codes in calls] == [80, 1]


def stat_row(**extra):
    return Client(last_price=1.25, open_price=0.25, low_price=0.35,
                  high_price=4.5, amount=123, total_hand=42, **extra)


def test_market_stat_keeps_legacy_position_even_when_middle_source_is_missing():
    breadth = stat_row(full_code="sh880005")
    limits = Client(full_code="sh880006", last_price=7.0, open_price=8.0)
    calls = []
    client = Client(quotes=Client(legacy=lambda codes: calls.append(codes) or [breadth, limits]))
    result = market.fetch_market_stat(client)
    assert result.up_count == 12 and result.down_count == 2 and result.neutral_count == 4
    assert result.total_count == 45 and result.suspended_count == 27
    assert result.total_market_cap == 7e10  # Frozen positional behavior.
    assert result.limit_up_count == 0 and result.limit_down_count == 0
    assert result.total_amount == 123 and result.total_volume == 42.0
    assert calls == [list(market._MARKET_STAT_CODES)]


def test_market_stat_all_three_rows_and_empty_error():
    values = [stat_row(), Client(last_price=3.0), Client(last_price=2.25, open_price=1.25)]
    client = Client(quotes=Client(legacy=lambda codes: values))
    result = market.fetch_market_stat(client)
    assert result.limit_up_count == 22 and result.limit_down_count == 12
    assert result.total_market_cap == 3e10
    values.clear()
    with pytest.raises(RuntimeError, match="880005/880001/880006"):
        market.fetch_market_stat(client)


def test_today_flow_materializes_ticks_and_calls_current_classifier(monkeypatch):
    ticks = [object(), object()]
    client = Client(trades=Client(all_today=lambda code: Client(ticks=iter(ticks))))
    seen = []
    monkeypatch.setattr(market, "classify_fund_flow", lambda values: seen.append(values) or "result")
    assert market.fetch_today_fund_flow(client, CODE) == "result"
    assert seen == [tuple(ticks)]
    client.trades.all_today = lambda code: []
    assert market.fetch_today_fund_flow(client, CODE) == "result"
    assert seen[-1] == ()


def test_latest_suspended_helper_retains_list_order_and_original_rows():
    calls = []
    first = Client(full_code=CODE)
    other = Client(full_code="sh600002")
    client = Client(codes=Client(latest_suspended=lambda m: calls.append(("suspended", m)) or [CODE],
                    latest_stock_list=lambda m: calls.append(("stocks", m)) or [other, first, first]))
    result = core.HelperApi(client).latest_suspended("sh")
    assert isinstance(result, list) and result == [first, first]
    assert calls == [("suspended", "sh"), ("stocks", "sh")]
