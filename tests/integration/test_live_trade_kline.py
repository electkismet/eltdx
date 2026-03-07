from __future__ import annotations

import os

import pytest

from eltdx import TdxClient


@pytest.mark.integration
def test_live_history_trade_minute_kline() -> None:
    if os.getenv("ELTDX_RUN_LIVE") != "1":
        pytest.skip("set ELTDX_RUN_LIVE=1 to run live tests")

    with TdxClient() as client:
        trades = client.get_history_trade_day("sz000001", "20260305")
        bars = client.get_history_trade_minute_kline("sz000001", "20260305")

    assert bars.count == 241
    assert bars.items[0].time.strftime("%H:%M") == "09:30"
    assert bars.items[-1].time.strftime("%H:%M") == "15:00"
    assert bars.items[0].time.tzinfo is not None
    assert any(item.volume > 0 for item in bars.items)
    assert sum(item.volume for item in bars.items) == sum(item.volume for item in trades.items if item.price_milli > 0)