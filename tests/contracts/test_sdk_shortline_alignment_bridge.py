"""Shared shortline date/field alignment contracts, not yet executed."""
from datetime import date
from types import SimpleNamespace

import pytest

from eltdx.helpers import shortline
from eltdx.exceptions import ResourceFormatError, TdxStatsDateError

DAY = date(2024, 6, 3)
PREVIOUS = date(2024, 5, 31)


def test_dominant_date_counts_missing_rows_in_coverage_and_uses_later_tie():
    row = lambda value: SimpleNamespace(stats_date=value)
    values = [row("20240603")] * 19 + [row(None)]
    assert shortline._dominant_date_and_coverage(iter(values)) == ("20240603", 0.95)
    assert shortline._dominant_date_and_coverage([row("20240531"), row("20240603")]) == ("20240603", 0.5)
    assert shortline._dominant_date_and_coverage([row(None), row("")]) == (None, 0.0)


def test_dominant_date_materializes_before_reading_and_keeps_double_property_access():
    seen = []

    class Row:
        @property
        def stats_date(self):
            seen.append("read")
            return "20240603"

    def rows():
        seen.append("yield")
        yield Row()
        seen.append("done")

    assert shortline._dominant_date_and_coverage(rows()) == ("20240603", 1.0)
    assert seen == ["yield", "done", "read", "read"]


def test_stats_date_error_precedence_and_exact_coverage_message():
    values = lambda value, n=20: {i: SimpleNamespace(stats_date=value) for i in range(n)}
    resource = SimpleNamespace(stat={}, stat2=values("20240603"))
    with pytest.raises(ResourceFormatError, match="has no dominant date"):
        shortline._validate_stats_resource_dates(resource, target=DAY, previous=PREVIOUS)
    resource.stat = values("20240531", 1)
    with pytest.raises(ResourceFormatError, match="dates disagree"):
        shortline._validate_stats_resource_dates(resource, target=DAY, previous=PREVIOUS)
    resource.stat = values("20240603")
    resource.stat[0].stats_date = None
    assert shortline._validate_stats_resource_dates(resource, target=DAY, previous=PREVIOUS) == DAY
    resource.stat[1].stats_date = None
    with pytest.raises(ResourceFormatError, match=r"tdxstat.cfg=90.00%, tdxstat2.cfg=100.00%"):
        shortline._validate_stats_resource_dates(resource, target=DAY, previous=PREVIOUS)
    resource.stat = values("20240604")
    resource.stat2 = values("20240604")
    with pytest.raises(TdxStatsDateError, match="not usable for the target session"):
        shortline._validate_stats_resource_dates(resource, target=DAY, previous=PREVIOUS)


def test_stat2_alignment_preserves_field_identity_and_previous_day_remap():
    fields = ["amount_10k", "seal_amount_10k", "prev_amount_10k", "prev_seal_amount_10k",
              "prev2_seal_amount_10k", "open_volume_hand", "open_amount_10k",
              "prev_open_volume_hand", "prev_open_amount_10k"]
    data = {name: object() for name in fields}
    row = SimpleNamespace(stats_date="20240603", **data)
    same = shortline._aligned_stat2(row, target=DAY, previous=PREVIOUS)
    assert same["status"] == "same_day"
    assert same["prev_amount_10k"] is row.prev_amount_10k
    row.stats_date = "20240531"
    old = shortline._aligned_stat2(row, target=DAY, previous=PREVIOUS)
    assert old["status"] == "previous_trading_day"
    assert old["prev_amount_10k"] is row.amount_10k
    assert old["prev_seal_amount_10k"] is row.seal_amount_10k
    assert old["prev2_seal_amount_10k"] is row.prev_seal_amount_10k
    assert old["prev_open_volume_hand"] is row.open_volume_hand
    assert old["prev_open_amount_10k"] is row.open_amount_10k


def test_stat2_unaligned_does_not_read_unused_columns_and_keeps_empty_hook(monkeypatch):
    row = SimpleNamespace(stats_date="20240604")
    assert shortline._aligned_stat2(row, target=DAY, previous=PREVIOUS)["status"] == "stats_date_unaligned"
    assert shortline._aligned_stat2(None, target=DAY, previous=PREVIOUS)["status"] == "stats_row_missing"
    monkeypatch.setattr(shortline, "_empty_alignment", lambda status: {"hook": status})
    assert shortline._aligned_stat2(row, target=DAY, previous=PREVIOUS) == {"hook": "stats_date_unaligned"}


def test_stats_cache_reuses_valid_value_and_failed_refresh_keeps_old_value(monkeypatch):
    class Client(SimpleNamespace):
        pass

    old = object()
    replacement = object()
    calls = []
    client = Client(resources=SimpleNamespace(read_stats=lambda path: calls.append(path) or replacement))
    service = shortline.ShortlineIndicatorService(client)
    service._stats_cache["stats"] = old
    monkeypatch.setattr(shortline, "_stats_resource_is_usable", lambda *args, **kwargs: True)
    assert service._stats_resource("stats", refresh=False, target=DAY, previous=PREVIOUS) == (old, False)
    assert calls == []
    error = ResourceFormatError("bad replacement")

    def invalid(*args, **kwargs):
        raise error

    monkeypatch.setattr(shortline, "_validate_stats_resource_dates", invalid)
    with pytest.raises(ResourceFormatError) as caught:
        service._stats_resource("stats", refresh=True, target=DAY, previous=PREVIOUS)
    assert caught.value is error and service._stats_cache["stats"] is old
    monkeypatch.setattr(shortline, "_validate_stats_resource_dates", lambda *args, **kwargs: DAY)
    assert service._stats_resource("stats", refresh=True, target=DAY, previous=PREVIOUS) == (replacement, True)


def test_stats_validation_keeps_parser_cause_and_custom_dominant_hook(monkeypatch):
    resource = SimpleNamespace(stat={0: object()}, stat2={0: object()})
    monkeypatch.setattr(shortline, "_dominant_date_and_coverage", lambda rows: ("invalid", 1.0))
    with pytest.raises(ResourceFormatError) as caught:
        shortline._validate_stats_resource_dates(resource, target=DAY, previous=PREVIOUS)
    assert isinstance(caught.value.__cause__, ValueError)
    assert "'invalid'" in str(caught.value)


def test_stats_usable_only_downgrades_resource_format_subclasses(monkeypatch):
    def invalid(*args, **kwargs):
        raise ResourceFormatError("bad")

    monkeypatch.setattr(shortline, "_validate_stats_resource_dates", invalid)
    assert not shortline._stats_resource_is_usable(object(), target=DAY, previous=PREVIOUS)

    def interrupted(*args, **kwargs):
        raise KeyboardInterrupt()

    monkeypatch.setattr(shortline, "_validate_stats_resource_dates", interrupted)
    with pytest.raises(KeyboardInterrupt):
        shortline._stats_resource_is_usable(object(), target=DAY, previous=PREVIOUS)
