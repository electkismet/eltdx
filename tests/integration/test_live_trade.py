from __future__ import annotations

import os

import pytest

from eltdx import TdxClient


@pytest.mark.integration
def test_live_trade() -> None:
    if os.getenv("ELTDX_RUN_LIVE") != "1":
        pytest.skip("set ELTDX_RUN_LIVE=1 to run live tests")

    with TdxClient() as client:
        live = client.get_trade("sz000001", count=10)
        hist = client.get_history_trade("sz000001", "20260305", count=10)

    assert live.count > 0
    assert hist.count > 0
    assert live.items[0].time.tzinfo is not None
    assert hist.items[0].time.tzinfo is not None