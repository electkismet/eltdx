from __future__ import annotations

import os

import pytest

from eltdx import TdxClient


@pytest.mark.integration
def test_live_quote_batching() -> None:
    if os.getenv("ELTDX_RUN_LIVE") != "1":
        pytest.skip("set ELTDX_RUN_LIVE=1 to run live tests")

    with TdxClient() as client:
        sz_codes = client.get_codes("sz", limit=60)
        sh_codes = client.get_codes("sh", limit=60)
        codes = [item.full_code for item in sz_codes.items] + [item.full_code for item in sh_codes.items]
        quotes = client.get_quote(codes)

    assert len(codes) == 120
    assert len(quotes) == len(codes)

    requested = set(codes)
    returned = {f"{quote.exchange}{quote.code}" for quote in quotes}

    assert returned == requested
    assert all(len(quote.buy_levels) == 5 for quote in quotes)
    assert all(len(quote.sell_levels) == 5 for quote in quotes)
    assert all(quote.server_time is None or quote.server_time.tzinfo is not None for quote in quotes)
