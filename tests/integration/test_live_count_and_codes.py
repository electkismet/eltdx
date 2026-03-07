from __future__ import annotations

import os

import pytest

from eltdx import TdxClient


@pytest.mark.integration
def test_live_count_and_codes() -> None:
    if os.getenv("ELTDX_RUN_LIVE") != "1":
        pytest.skip("set ELTDX_RUN_LIVE=1 to run live tests")

    with TdxClient() as client:
        count = client.get_count("sz")
        codes = client.get_codes("sz", start=0, limit=3)

    assert count > 0
    assert len(codes) == 3
    assert all(item.exchange == "sz" for item in codes)
    assert all(len(item.code) == 6 for item in codes)


@pytest.mark.integration
def test_live_bj_codes_fallback() -> None:
    if os.getenv("ELTDX_RUN_LIVE") != "1":
        pytest.skip("set ELTDX_RUN_LIVE=1 to run live tests")

    with TdxClient() as client:
        count = client.get_count("bj")
        page = client.get_codes("bj", start=0, limit=3)
        all_codes = client.get_codes_all("bj")

    assert count > 0
    assert len(page) == 3
    assert len(all_codes) == count
    assert all(item.exchange == "bj" for item in page)
    assert all(len(item.code) == 6 for item in page)
    assert all(item.name for item in page)
