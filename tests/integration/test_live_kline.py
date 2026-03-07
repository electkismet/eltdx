from __future__ import annotations

import os

import pytest

from eltdx import TdxClient


@pytest.mark.integration
def test_live_kline() -> None:
    if os.getenv("ELTDX_RUN_LIVE") != "1":
        pytest.skip("set ELTDX_RUN_LIVE=1 to run live tests")

    with TdxClient() as client:
        stock = client.get_kline("day", "sz000001", count=5)
        index = client.get_kline("day", "sh000001", count=5, kind="index")
        minute = client.get_kline("1m", "sz000001", count=5)

    assert stock.count == 5
    assert stock.items[-1].close_price > 0
    assert stock.items[-1].time.tzinfo is not None
    assert index.count == 5
    assert index.items[-1].up_count is not None
    assert index.items[-1].down_count is not None
    assert minute.count == 5
    assert minute.items[0].time.tzinfo is not None
