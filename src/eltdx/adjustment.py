from __future__ import annotations

from datetime import date

from .enums import AdjustMode
from .models import FactorItem, FactorResponse, KlineItem, KlineResponse, XdxrItem

ADJUST_MODE_ALIASES = {
    AdjustMode.QFQ: AdjustMode.QFQ,
    AdjustMode.HFQ: AdjustMode.HFQ,
    "qfq": AdjustMode.QFQ,
    "forward": AdjustMode.QFQ,
    "pre": AdjustMode.QFQ,
    "hfq": AdjustMode.HFQ,
    "backward": AdjustMode.HFQ,
    "post": AdjustMode.HFQ,
}


def normalize_adjust_mode(value: str | AdjustMode) -> AdjustMode:
    if isinstance(value, AdjustMode):
        return value
    key = str(value).strip().lower()
    try:
        return ADJUST_MODE_ALIASES[key]
    except KeyError as exc:
        raise ValueError(f"invalid adjust mode: {value!r}") from exc


def apply_xdxr_to_last_close(last_close_milli: int | None, xdxr: XdxrItem | None) -> int | None:
    if last_close_milli in (None, 0) or xdxr is None:
        return last_close_milli

    numerator = ((last_close_milli / 1000.0) * 10.0 - xdxr.fenhong) + (xdxr.peigu * xdxr.peigujia)
    denominator = 10.0 + xdxr.songzhuangu + xdxr.peigu
    if denominator == 0:
        return last_close_milli
    return int((numerator / denominator) * 1000.0)


def _qfq_step(last_close_milli: int | None, pre_last_close_milli: int | None) -> float:
    if last_close_milli in (None, 0) or pre_last_close_milli in (None, 0) or last_close_milli == pre_last_close_milli:
        return 1.0
    return pre_last_close_milli / last_close_milli


def _hfq_step(last_close_milli: int | None, pre_last_close_milli: int | None) -> float:
    if last_close_milli in (None, 0) or pre_last_close_milli in (None, 0) or last_close_milli == pre_last_close_milli:
        return 1.0
    return last_close_milli / pre_last_close_milli


def build_factor_response(day_kline: KlineResponse, xdxr_items: list[XdxrItem]) -> FactorResponse:
    items = sorted(day_kline.items, key=lambda item: item.time)
    xdxr_sorted = sorted(xdxr_items, key=lambda item: item.time)
    overrides: dict[date, int | None] = {}

    for xdxr in xdxr_sorted:
        for kline in items:
            if kline.time.date() >= xdxr.time.date():
                overrides[kline.time.date()] = apply_xdxr_to_last_close(kline.last_close_price_milli, xdxr)
                break

    factors: list[FactorItem] = []
    hfq_cumulative = 1.0
    for item in items:
        pre_last_close_milli = overrides.get(item.time.date(), item.last_close_price_milli)
        hfq_cumulative *= _hfq_step(item.last_close_price_milli, pre_last_close_milli)
        factors.append(
            FactorItem(
                time=item.time,
                last_close_price=item.last_close_price,
                last_close_price_milli=item.last_close_price_milli,
                pre_last_close_price=None if pre_last_close_milli is None else pre_last_close_milli / 1000.0,
                pre_last_close_price_milli=pre_last_close_milli,
                qfq_factor=1.0,
                hfq_factor=hfq_cumulative,
            )
        )

    if factors:
        qfq_cumulative = 1.0
        factors[-1].qfq_factor = 1.0
        for index in range(len(factors) - 1, 0, -1):
            current = factors[index]
            qfq_cumulative *= _qfq_step(current.last_close_price_milli, current.pre_last_close_price_milli)
            factors[index - 1].qfq_factor = qfq_cumulative

    return FactorResponse(count=len(factors), items=factors)


def _adjust_price_milli(value: int | None, factor: float) -> int | None:
    if value is None:
        return None
    return int(round(value * factor))


def apply_factors_to_kline(response: KlineResponse, factors: FactorResponse, mode: str | AdjustMode) -> KlineResponse:
    resolved_mode = normalize_adjust_mode(mode)
    factor_by_day = {item.time.date(): item for item in factors.items}

    adjusted_items: list[KlineItem] = []
    for item in response.items:
        factor_item = factor_by_day.get(item.time.date())
        factor = 1.0 if factor_item is None else (factor_item.qfq_factor if resolved_mode is AdjustMode.QFQ else factor_item.hfq_factor)
        open_milli = _adjust_price_milli(item.open_price_milli, factor)
        high_milli = _adjust_price_milli(item.high_price_milli, factor)
        low_milli = _adjust_price_milli(item.low_price_milli, factor)
        close_milli = _adjust_price_milli(item.close_price_milli, factor)
        base_last_close_milli = item.last_close_price_milli if factor_item is None else factor_item.pre_last_close_price_milli
        last_close_milli = _adjust_price_milli(base_last_close_milli, factor)
        adjusted_items.append(
            KlineItem(
                time=item.time,
                open_price=0.0 if open_milli is None else open_milli / 1000.0,
                open_price_milli=0 if open_milli is None else open_milli,
                high_price=0.0 if high_milli is None else high_milli / 1000.0,
                high_price_milli=0 if high_milli is None else high_milli,
                low_price=0.0 if low_milli is None else low_milli / 1000.0,
                low_price_milli=0 if low_milli is None else low_milli,
                close_price=0.0 if close_milli is None else close_milli / 1000.0,
                close_price_milli=0 if close_milli is None else close_milli,
                last_close_price=None if last_close_milli is None else last_close_milli / 1000.0,
                last_close_price_milli=last_close_milli,
                volume=item.volume,
                amount=item.amount,
                amount_milli=item.amount_milli,
                order_count=item.order_count,
                up_count=item.up_count,
                down_count=item.down_count,
            )
        )

    return KlineResponse(
        count=response.count,
        items=adjusted_items,
        raw_frame_hex=response.raw_frame_hex,
        raw_payload_hex=response.raw_payload_hex,
    )

