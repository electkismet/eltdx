"""Local adjustment coefficients derived only from capital-change records."""

from __future__ import annotations

from datetime import date

from eltdx.models import (
    AdjustmentFactor,
    AdjustmentFactorResponse,
    CapitalChangeBlock,
    CapitalChangeRecord,
)
from eltdx.protocol.unit import date_from_yyyymmdd, yyyymmdd


def build_adjustment_factor_response(
    changes: CapitalChangeBlock,
    *,
    anchor_date=None,
    start_date=None,
) -> AdjustmentFactorResponse:
    resolved_anchor = _normalize_optional_date(anchor_date, "anchor_date")
    resolved_start = _normalize_optional_date(start_date, "start_date")
    if resolved_anchor is not None and resolved_start is not None and resolved_anchor < resolved_start:
        raise ValueError("anchor_date must not be earlier than start_date")

    events = [
        record
        for record in changes.records
        if record.category_raw == 1
        and record.date is not None
        and (resolved_start is None or record.date >= resolved_start)
    ]
    chronological = sorted(events, key=lambda item: item.date or date.min)
    reverse_chronological = [
        event
        for _, event in sorted(
            enumerate(events),
            key=lambda item: (-(item[1].date or date.min).toordinal(), item[0]),
        )
    ]
    event_dates = sorted({event.date for event in events if event.date is not None})

    factors = []
    for event_date in event_dates:
        qfq_scale, qfq_offset = _qfq_coefficients(
            chronological,
            event_date,
            resolved_anchor,
        )
        hfq_scale, hfq_offset = _hfq_coefficients(
            reverse_chronological,
            event_date,
        )
        factors.append(
            AdjustmentFactor(
                date=event_date,
                qfq_scale=qfq_scale,
                qfq_offset=qfq_offset,
                hfq_scale=hfq_scale,
                hfq_offset=hfq_offset,
            )
        )

    return AdjustmentFactorResponse(
        exchange=changes.exchange,
        market_id=changes.market_id,
        code=changes.code,
        anchor_date=resolved_anchor,
        start_date=resolved_start,
        items=tuple(factors),
    )


def _event_coefficients(event: CapitalChangeRecord) -> tuple[float, float]:
    # Tag 1: c1=D, c2=P, c3=S, c4=R, all expressed per ten shares.
    multiplier = (10.0 + event.c3_value + event.c4_value) / 10.0
    offset = (event.c1_value - event.c4_value * event.c2_value) / 10.0
    if multiplier <= 0:
        raise ValueError("capital-change event produces a non-positive price multiplier")
    return multiplier, offset


def _qfq_coefficients(
    events: list[CapitalChangeRecord],
    factor_date: date,
    anchor_date: date | None,
) -> tuple[float, float]:
    scale = 1.0
    offset = 0.0
    for event in events:
        assert event.date is not None
        if event.date >= factor_date and (anchor_date is None or event.date <= anchor_date):
            multiplier, event_offset = _event_coefficients(event)
            scale /= multiplier
            offset = (offset - event_offset) / multiplier
    return scale, offset


def _hfq_coefficients(
    events: list[CapitalChangeRecord],
    factor_date: date,
) -> tuple[float, float]:
    scale = 1.0
    offset = 0.0
    for event in events:
        assert event.date is not None
        if event.date <= factor_date:
            multiplier, event_offset = _event_coefficients(event)
            scale *= multiplier
            offset = multiplier * offset + event_offset
    return scale, offset


def _normalize_optional_date(value, name: str) -> date | None:
    if value in (None, ""):
        return None
    parsed = date_from_yyyymmdd(yyyymmdd(value))
    if parsed is None:
        raise ValueError(f"invalid {name}: {value!r}")
    return parsed
