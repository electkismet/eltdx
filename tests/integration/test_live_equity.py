from __future__ import annotations

import os

import pytest

from eltdx import TdxClient


@pytest.mark.integration
def test_live_equity() -> None:
    if os.getenv("ELTDX_RUN_LIVE") != "1":
        pytest.skip("set ELTDX_RUN_LIVE=1 to run live tests")

    with TdxClient() as client:
        changes = client.get_equity_changes("sz000001")
        equity = client.get_equity("sz000001")
        turnover = client.get_turnover("sz000001", 1000, unit="hand")

    assert changes.count > 0
    assert changes.items[0].code == "sz000001"
    assert equity is not None
    assert equity.float_shares > 0
    assert turnover > 0
