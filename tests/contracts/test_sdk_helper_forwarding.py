"""Unexecuted compatibility cases for intentionally thin helper aliases."""
from __future__ import annotations

from types import SimpleNamespace
import weakref

import pytest

from eltdx.helpers.core import HelperApi
from eltdx.protocol.unit import normalize_code


class Client:
    pass


@pytest.mark.parametrize("method", ["latest_stock_list", "latest_st"])
@pytest.mark.parametrize("market", [None, "sh", "custom-market"])
def test_helper_directory_alias_preserves_custom_hook_and_result_identity(method, market):
    client = Client()
    calls = []
    result = object()

    def callback(value):
        calls.append(value)
        return result

    client.codes = SimpleNamespace(**{method: callback})
    helper = object.__new__(HelperApi)
    helper._client_ref = weakref.ref(client)
    assert getattr(helper, method)(market) is result
    assert calls == [market]


@pytest.mark.parametrize("method", ["buy_sell_strength", "volume_comparison"])
@pytest.mark.parametrize("raw", [False, True])
def test_helper_minute_alias_keeps_normalization_selector_and_original_result(method, raw):
    client = Client()
    calls = []
    result = object()

    def callback(*args, **kwargs):
        calls.append((args, kwargs))
        return result

    client.minutes = SimpleNamespace(aux=callback)
    helper = object.__new__(HelperApi)
    helper._client_ref = weakref.ref(client)
    code = "sh" + str(600_001)
    assert getattr(helper, method)(code, include_raw=raw) is result
    assert calls == [((normalize_code(code),), {"kind": method, "include_raw": raw})]
