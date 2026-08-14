"""Derived helpers built from capital-change records."""

from __future__ import annotations

from datetime import date, datetime

from eltdx.models import (
    CapitalChangeBlock,
    CapitalChangeRecord,
    EquityRecord,
    EquityResponse,
    FactorRecord,
    FactorResponse,
    KlineBar,
    KlineSeries,
    XdxrRecord,
)
from eltdx.protocol.unit import yyyymmdd

EQUITY_CATEGORIES = {2, 3, 5, 7, 8, 9, 10}
VOLUME_UNIT_MULTIPLIERS = {
    "share": 1,
    "shares": 1,
    "stock": 1,
    "hand": 100,
    "hands": 100,
    "lot": 100,
    "lots": 100,
}


def filter_xdxr_records(block: CapitalChangeBlock) -> list[XdxrRecord]:
    return [
        XdxrRecord(
            code=record.full_code,
            date=record.date,
            category=record.category_raw,
            category_name=record.category_name,
            fenhong=round(record.c1_value, 6),
            peigujia=round(record.c2_value, 6),
            songzhuangu=round(record.c3_value, 6),
            peigu=round(record.c4_value, 6),
        )
        for record in block.records
        if record.category_raw == 1
    ]


def filter_equity_records(block: CapitalChangeBlock) -> EquityResponse:
    records = tuple(
        EquityRecord(
            code=record.full_code,
            date=record.date,
            category=record.category_raw,
            category_name=record.category_name,
            float_shares=int(record.c3_value),
            total_shares=int(record.c4_value),
        )
        for record in block.records
        if record.category_raw in EQUITY_CATEGORIES
    )
    return EquityResponse(count=len(records), items=records)


def pick_equity(records: list[EquityRecord] | tuple[EquityRecord, ...], on=None) -> EquityRecord | None:
    target = _normalize_date(on)
    ordered = sorted((record for record in records if record.date is not None), key=lambda record: record.date)
    for record in reversed(ordered):
        assert record.date is not None
        if record.date <= target:
            return record
    return None


def normalize_volume_unit(unit: str) -> int:
    key = str(unit).strip().lower()
    try:
        return VOLUME_UNIT_MULTIPLIERS[key]
    except KeyError as exc:
        raise ValueError(f"invalid volume unit: {unit!r}") from exc


def compute_turnover(equity: EquityRecord | None, volume: int | float, *, unit: str = "hand") -> float:
    if equity is None or equity.float_shares <= 0:
        return 0.0
    shares = float(volume) * normalize_volume_unit(unit)
    return shares / float(equity.float_shares) * 100.0


def build_factor_response(
    day_kline: KlineSeries,
    xdxr_records: list[XdxrRecord] | tuple[XdxrRecord, ...],
    *,
    anchor_date=None,
) -> FactorResponse:
    bars = sorted(day_kline.bars, key=lambda item: item.time)
    xdxr_sorted = sorted((item for item in xdxr_records if item.date is not None), key=lambda item: item.date)
    overrides: dict[date, int | None] = {}

    for xdxr in xdxr_sorted:
        assert xdxr.date is not None
        for bar in bars:
            if bar.time.date() >= xdxr.date:
                overrides[bar.time.date()] = apply_xdxr_to_last_close(bar.last_close_price_milli, xdxr)
                break

    factors: list[FactorRecord] = []
    hfq_cumulative = 1.0
    for bar in bars:
        pre_last_close_milli = overrides.get(bar.time.date(), bar.last_close_price_milli)
        hfq_cumulative *= _hfq_step(bar.last_close_price_milli, pre_last_close_milli)
        factors.append(
            FactorRecord(
                time=bar.time,
                last_close_price=None if bar.last_close_price_milli is None else bar.last_close_price_milli / 1000.0,
                last_close_price_milli=bar.last_close_price_milli,
                pre_last_close_price=None if pre_last_close_milli is None else pre_last_close_milli / 1000.0,
                pre_last_close_price_milli=pre_last_close_milli,
                qfq_factor=1.0,
                hfq_factor=hfq_cumulative,
            )
        )

    if factors:
        qfq_cumulative = 1.0
        for index in range(len(factors) - 1, 0, -1):
            current = factors[index]
            qfq_cumulative *= _qfq_step(current.last_close_price_milli, current.pre_last_close_price_milli)
            factors[index - 1] = _replace_factor_qfq(factors[index - 1], qfq_cumulative)

    resolved_anchor = _normalize_date(anchor_date) if anchor_date not in (None, "") else None
    if resolved_anchor is not None:
        factors = _normalize_qfq_anchor(factors, resolved_anchor)

    return FactorResponse(count=len(factors), items=tuple(factors), anchor_date=resolved_anchor)


def apply_xdxr_to_last_close(last_close_milli: int | None, xdxr: XdxrRecord | None) -> int | None:
    if last_close_milli in (None, 0) or xdxr is None:
        return last_close_milli

    numerator = ((last_close_milli / 1000.0) * 10.0 - xdxr.fenhong) + (xdxr.peigu * xdxr.peigujia)
    denominator = 10.0 + xdxr.songzhuangu + xdxr.peigu
    if denominator == 0:
        return last_close_milli
    return int((numerator / denominator) * 1000.0)


def apply_factors_to_kline(
    response: KlineSeries,
    factors: FactorResponse,
    adjust: str = "qfq",
    *,
    anchor_date=None,
) -> KlineSeries:
    key = str(adjust).strip().lower()
    if key not in {"qfq", "front", "forward", "pre", "hfq", "back", "backward", "post"}:
        raise ValueError(f"invalid adjust mode: {adjust!r}")
    factor_by_day = {item.time.date(): item for item in factors.items}
    use_qfq = key in {"qfq", "front", "forward", "pre"}
    if anchor_date not in (None, "") and not use_qfq:
        raise ValueError("anchor_date is only supported for qfq")
    resolved_anchor = _normalize_date(anchor_date) if anchor_date not in (None, "") else factors.anchor_date if use_qfq else None
    bars = tuple(_adjust_bar(bar, factor_by_day.get(bar.time.date()), use_qfq=use_qfq) for bar in response.bars)
    return KlineSeries(
        exchange=response.exchange,
        market_id=response.market_id,
        code=response.code,
        period_raw=response.period_raw,
        period_param_raw=response.period_param_raw,
        period_name=response.period_name,
        start=response.start,
        request_count=response.request_count,
        adjust_mode_raw=response.adjust_mode_raw,
        adjust_mode=f"local_{'qfq' if use_qfq else 'hfq'}",
        anchor_date_raw=yyyymmdd(resolved_anchor) if resolved_anchor is not None else response.anchor_date_raw,
        anchor_date=resolved_anchor if resolved_anchor is not None else response.anchor_date,
        bars=bars,
        raw_payload=response.raw_payload,
    )


def _adjust_bar(bar: KlineBar, factor: FactorRecord | None, *, use_qfq: bool) -> KlineBar:
    if factor is None:
        return bar
    multiplier = factor.qfq_factor if use_qfq else factor.hfq_factor
    last_close_base = factor.pre_last_close_price_milli if factor.pre_last_close_price_milli is not None else bar.last_close_price_milli
    open_milli = _adjust_price_milli(bar.open_price_milli, multiplier)
    close_milli = _adjust_price_milli(bar.close_price_milli, multiplier)
    high_milli = _adjust_price_milli(bar.high_price_milli, multiplier)
    low_milli = _adjust_price_milli(bar.low_price_milli, multiplier)
    last_close_milli = _adjust_price_milli(last_close_base, multiplier)
    return KlineBar(
        time=bar.time,
        open=open_milli / 1000.0,
        close=close_milli / 1000.0,
        high=high_milli / 1000.0,
        low=low_milli / 1000.0,
        open_price_milli=open_milli,
        close_price_milli=close_milli,
        high_price_milli=high_milli,
        low_price_milli=low_milli,
        last_close_price_milli=last_close_milli,
        volume_raw=bar.volume_raw,
        amount_raw=bar.amount_raw,
        volume_wire_value=bar.volume_wire_value,
        volume_lots=bar.volume_lots,
        amount=bar.amount,
        open_delta_raw=bar.open_delta_raw,
        close_delta_raw=bar.close_delta_raw,
        high_delta_raw=bar.high_delta_raw,
        low_delta_raw=bar.low_delta_raw,
        up_count=bar.up_count,
        down_count=bar.down_count,
        record_hex=bar.record_hex,
    )


def _adjust_price_milli(value: int | None, factor: float) -> int:
    if value is None:
        return 0
    return int(round(value * factor))


def _qfq_step(last_close_milli: int | None, pre_last_close_milli: int | None) -> float:
    if last_close_milli in (None, 0) or pre_last_close_milli in (None, 0) or last_close_milli == pre_last_close_milli:
        return 1.0
    return pre_last_close_milli / last_close_milli


def _hfq_step(last_close_milli: int | None, pre_last_close_milli: int | None) -> float:
    if last_close_milli in (None, 0) or pre_last_close_milli in (None, 0) or last_close_milli == pre_last_close_milli:
        return 1.0
    return last_close_milli / pre_last_close_milli


def _replace_factor_qfq(item: FactorRecord, qfq_factor: float) -> FactorRecord:
    return FactorRecord(
        time=item.time,
        last_close_price=item.last_close_price,
        last_close_price_milli=item.last_close_price_milli,
        pre_last_close_price=item.pre_last_close_price,
        pre_last_close_price_milli=item.pre_last_close_price_milli,
        qfq_factor=qfq_factor,
        hfq_factor=item.hfq_factor,
    )


def _normalize_qfq_anchor(factors: list[FactorRecord], anchor_date: date) -> list[FactorRecord]:
    anchor = None
    for item in factors:
        if item.time.date() <= anchor_date:
            anchor = item
        else:
            break
    if anchor is None:
        raise ValueError("anchor_date is earlier than the first available trade date")
    if anchor.qfq_factor == 0:
        raise ValueError("anchor_date qfq factor is zero")
    return [_replace_factor_qfq(item, item.qfq_factor / anchor.qfq_factor) for item in factors]


def _normalize_date(value) -> date:
    raw = yyyymmdd(value)
    text = f"{raw:08d}"
    return date(int(text[:4]), int(text[4:6]), int(text[6:8]))
