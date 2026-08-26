"""Local adjustment coefficients derived from capital-change records."""

from __future__ import annotations

from datetime import date

from eltdx.models import (
    AdjustmentFactor,
    AdjustmentFactorResponse,
    CapitalChangeBlock,
    CapitalChangeRecord,
    KlineSeries,
)
from eltdx.protocol.unit import date_from_yyyymmdd, yyyymmdd


def build_adjustment_factor_response(
    day_kline: KlineSeries,
    changes: CapitalChangeBlock,
    *,
    anchor_date=None,
) -> AdjustmentFactorResponse:
    bars = sorted(day_kline.bars, key=lambda item: item.time)
    if not bars:
        return AdjustmentFactorResponse(
            exchange=day_kline.exchange,
            market_id=day_kline.market_id,
            code=day_kline.code,
            anchor_date=None,
            first_trading_date=None,
            items=(),
        )

    first_trading_date = bars[0].time.date()
    resolved_anchor = _resolve_anchor_date(bars, anchor_date)
    effective_anchor = resolved_anchor or bars[-1].time.date()
    events = [
        record
        for record in changes.records
        if record.category_raw == 1
        and record.date is not None
        and record.date >= first_trading_date
    ]
    qfq_events = sorted(events, key=lambda item: item.date or date.min)
    hfq_events = [
        event
        for _, event in sorted(
            enumerate(events),
            key=lambda item: (-(item[1].date or date.min).toordinal(), item[0]),
        )
    ]

    factors = []
    for bar in bars:
        bar_date = bar.time.date()
        qfq_scale, qfq_offset = _qfq_coefficients(qfq_events, bar_date, effective_anchor)
        hfq_scale, hfq_offset = _hfq_coefficients(hfq_events, first_trading_date, bar_date)
        factors.append(
            AdjustmentFactor(
                date=bar_date,
                qfq_scale=qfq_scale,
                qfq_offset=qfq_offset,
                hfq_scale=hfq_scale,
                hfq_offset=hfq_offset,
            )
        )
    return AdjustmentFactorResponse(
        exchange=day_kline.exchange,
        market_id=day_kline.market_id,
        code=day_kline.code,
        anchor_date=resolved_anchor,
        first_trading_date=first_trading_date,
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
    bar_date: date,
    anchor_date: date,
) -> tuple[float, float]:
    scale = 1.0
    offset = 0.0
    for event in events:
        assert event.date is not None
        if bar_date < event.date <= anchor_date:
            multiplier, event_offset = _event_coefficients(event)
            scale /= multiplier
            offset = (offset - event_offset) / multiplier
    return scale, offset


def _hfq_coefficients(
    events: list[CapitalChangeRecord],
    first_trading_date: date,
    bar_date: date,
) -> tuple[float, float]:
    scale = 1.0
    offset = 0.0
    for event in events:
        assert event.date is not None
        if first_trading_date <= event.date <= bar_date:
            multiplier, event_offset = _event_coefficients(event)
            scale *= multiplier
            offset = multiplier * offset + event_offset
    return scale, offset


def _resolve_anchor_date(bars, value) -> date | None:
    if value in (None, ""):
        return None
    target = date_from_yyyymmdd(yyyymmdd(value))
    if target is None:
        raise ValueError(f"invalid anchor_date: {value!r}")
    resolved = None
    for bar in bars:
        if bar.time.date() <= target:
            resolved = bar.time.date()
        else:
            break
    if resolved is None:
        raise ValueError("anchor_date is earlier than the first available trade date")
    return resolved
