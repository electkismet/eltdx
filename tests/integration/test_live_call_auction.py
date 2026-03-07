from __future__ import annotations

import os

import pytest

from eltdx import TdxClient


@pytest.mark.integration
def test_live_call_auction() -> None:
    if os.getenv("ELTDX_RUN_LIVE") != "1":
        pytest.skip("set ELTDX_RUN_LIVE=1 to run live tests")

    with TdxClient() as client:
        response = client.get_call_auction("sz000001")

    assert response.count > 0
    assert response.items[0].price > 0
