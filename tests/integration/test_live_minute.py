from __future__ import annotations

import os
from datetime import timedelta

import pytest

from eltdx import TdxClient
from eltdx.protocol.unit import SHANGHAI_TZ
from datetime import datetime


@pytest.mark.integration
def test_live_minute() -> None:
    if os.getenv("ELTDX_RUN_LIVE") != "1":
        pytest.skip("set ELTDX_RUN_LIVE=1 to run live tests")

    with TdxClient() as client:
        response = client.get_minute("sz000001")
        if response.count == 0:
            current = datetime.now(SHANGHAI_TZ).date()
            for offset in range(1, 8):
                retry_date = current - timedelta(days=offset)
                response = client.get_history_minute("sz000001", retry_date)
                if response.count > 0:
                    break

    assert response.count > 0
    assert response.items[0].price > 0
    assert response.items[0].time.tzinfo is not None
