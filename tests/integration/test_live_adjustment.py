from __future__ import annotations

import os

import pytest

from eltdx import TdxClient


@pytest.mark.integration
def test_live_adjustment() -> None:
    if os.getenv("ELTDX_RUN_LIVE") != "1":
        pytest.skip("set ELTDX_RUN_LIVE=1 to run live tests")

    with TdxClient() as client:
        raw = client.get_kline("day", "sz000001", count=10)
        factors = client.get_factors("sz000001")
        qfq = client.get_adjusted_kline("day", "sz000001", count=10, adjust="qfq")
        qfq_all = client.get_adjusted_kline_all("day", "sz000001", adjust="qfq")

    assert raw.count == 10
    assert factors.count > 0
    assert factors.items[-1].qfq_factor == 1.0
    assert qfq.count == raw.count
    assert qfq.items[-1].close_price_milli == raw.items[-1].close_price_milli
    assert qfq_all.count == factors.count
    assert qfq_all.items[-1].time.tzinfo is not None
