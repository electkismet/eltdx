"""Native equity contracts authored for the final unified verification pass."""
from __future__ import annotations

from dataclasses import replace
from datetime import date
import math

import pytest

from eltdx import equity
from eltdx.api.corporate import CorporateApi
from eltdx.models import CapitalChangeBatch, CapitalChangeBlock, CapitalChangeRecord

CODE = "sh" + str(600_001)


def _event(day=date(2024, 6, 1), values=(0.0, 25.0, 0.0, 2.0), category=1):
    return CapitalChangeRecord("sh", 1, "600001", 0, 20240601, day, category, None,
                               b"", b"", b"", b"", *values, *values)


def _block(events):
    return CapitalChangeBlock("sh", 1, "600001", 1, tuple(events))


def _legacy(changes, monkeypatch, **kwargs):
    with monkeypatch.context() as context:
        original = equity._event_coefficients
        context.setattr(equity, "_event_coefficients", lambda event: original(event))
        return equity.build_adjustment_factor_response(changes, **kwargs)


def test_native_equity_same_day_order_matches_original_composition(monkeypatch):
    changes = _block([_event(), _event(values=(1.0, 0.0, 1.0, 0.0))])
    result = equity.build_adjustment_factor_response(changes)
    assert result == _legacy(changes, monkeypatch)
    assert result.items[0].hfq_offset == pytest.approx(-5.4)
    reversed_result = equity.build_adjustment_factor_response(replace(changes, records=changes.records[::-1]))
    assert reversed_result.items[0].hfq_offset != result.items[0].hfq_offset


@pytest.mark.parametrize("anchor,start", [(None, None), ("2024-05-31", None),
                                         ("2024-06-01", "2024-05-31"), (None, "2024-06-02")])
def test_native_equity_date_windows_and_dated_filter_match_original(anchor, start, monkeypatch):
    changes = _block([_event(date(2024, 6, 2)), _event(), _event(None), _event(category=5)])
    kwargs = dict(anchor_date=anchor, start_date=start)
    assert equity.build_adjustment_factor_response(changes, **kwargs) == _legacy(changes, monkeypatch, **kwargs)


def test_native_equity_empty_records_return_original_identity_and_dates():
    result = equity.build_adjustment_factor_response(_block([]), anchor_date=20240601, start_date="2024-05-01")
    assert result.full_code == CODE and result.items == ()
    assert result.anchor_date == date(2024, 6, 1) and result.start_date == date(2024, 5, 1)


def test_native_equity_nonpositive_multiplier_keeps_exact_error(monkeypatch):
    changes = _block([_event(values=(0.0, 0.0, -10.0, 0.0))])
    for call in (lambda: equity.build_adjustment_factor_response(changes), lambda: _legacy(changes, monkeypatch)):
        with pytest.raises(ValueError, match="^capital-change event produces a non-positive price multiplier$"):
            call()


@pytest.mark.parametrize("values", [(float("nan"), 0.0, 0.0, 0.0),
                                    (0.0, 0.0, float("inf"), 0.0),
                                    (0.0, 0.0, float("nan"), 0.0)])
def test_native_equity_nonfinite_values_keep_legacy_arithmetic(values, monkeypatch):
    changes = _block([_event(values=values)])
    actual = equity.build_adjustment_factor_response(changes).items[0]
    expected = _legacy(changes, monkeypatch).items[0]
    for name in ("qfq_scale", "qfq_offset", "hfq_scale", "hfq_offset"):
        left, right = getattr(actual, name), getattr(expected, name)
        assert math.isnan(left) if math.isnan(right) else left == right


def test_native_equity_preserves_patched_coefficient_extension(monkeypatch):
    calls = []
    monkeypatch.setattr(equity, "_event_coefficients", lambda event: calls.append(event) or (2.0, 3.0))
    result = equity.build_adjustment_factor_response(_block([_event()]))
    assert len(calls) == 2
    assert (result.items[0].qfq_scale, result.items[0].qfq_offset) == (0.5, -1.5)


def test_native_equity_custom_record_subclass_retains_property_evaluation():
    calls = []

    class Record(CapitalChangeRecord):
        def __getattribute__(self, name):
            if name == "c1_value":
                calls.append(name)
            return super().__getattribute__(name)

    record = Record("sh", 1, "600001", 0, 20240601, date(2024, 6, 1), 1, None,
                    b"", b"", b"", b"", *([0.0] * 8))
    equity.build_adjustment_factor_response(_block([record]))
    assert calls == ["c1_value", "c1_value"]


def test_native_equity_corporate_batch_keeps_response_order():
    blocks = (_block([_event()]), replace(_block([]), code="600002"))

    class Transport:
        def execute(self, command, payload):
            assert command == 0x000F
            return CapitalChangeBatch(blocks)

    result = CorporateApi(Transport()).adjustment_factors([CODE, "sh600002"])
    assert [response.full_code for response in result.responses] == [CODE, "sh600002"]
    assert result.responses[1].items == ()
