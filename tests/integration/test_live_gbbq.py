from __future__ import annotations

import os

import pytest

from eltdx import TdxClient


@pytest.mark.integration
def test_live_gbbq() -> None:
    if os.getenv("ELTDX_RUN_LIVE") != "1":
        pytest.skip("set ELTDX_RUN_LIVE=1 to run live tests")

    with TdxClient() as client:
        response = client.get_gbbq("sz000001")
        xdxr = client.get_xdxr("sz000001")

    assert response.count > 0
    assert response.items[0].code == "sz000001"
    assert response.items[-1].time.tzinfo is not None
    assert len(xdxr) > 0
    assert xdxr[0].code == "sz000001"
    assert xdxr[-1].time.tzinfo is not None
