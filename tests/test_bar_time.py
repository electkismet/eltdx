"""``bar_time`` 时间戳语义对齐的测试（对应 easy_tdx 的 ``_df._apply_bar_time_align_*``）。

全部离线：客户端方法用 stub 掉 ``_execute`` / ``_fetch_kline_pages`` 的方式驱动，
不建立任何 socket。
"""

from __future__ import annotations

from datetime import datetime

import pytest

from eltdx._bar_time import (
    BAR_TIME_END,
    BAR_TIME_START,
    VALID_BAR_TIMES,
    align_datetime_bars,
    align_hour_minute_bars,
    align_time_bars,
    category_to_minutes,
    check_bar_time,
    kline_period_minutes,
    period_to_minutes,
)
from eltdx.api.bars import BarApi
from eltdx.ex.client import ExTdxClient
from eltdx.ex.models import ExInstrumentBar
from eltdx.mac.client import MacClient
from eltdx.mac.enums import Period
from eltdx.mac.models import MacBar
from eltdx.models.kline import KlineBar, KlineSeries


def _ex_bar(hour: int, minute: int) -> ExInstrumentBar:
    return ExInstrumentBar(
        open=10.0,
        high=11.0,
        low=9.0,
        close=10.5,
        position=1,
        trade=100,
        amount=1000.0,
        year=2026,
        month=10,
        day=12,
        hour=hour,
        minute=minute,
    )


def _mac_bar(when: datetime) -> MacBar:
    return MacBar(
        datetime=when,
        open=10.0,
        high=11.0,
        low=9.0,
        close=10.5,
        vol=100.0,
        amount=1000.0,
    )


def _kline_bar(when: datetime) -> KlineBar:
    return KlineBar(
        time=when,
        open=10.0,
        close=10.5,
        high=11.0,
        low=9.0,
        open_price_milli=10000,
        close_price_milli=10500,
        high_price_milli=11000,
        low_price_milli=9000,
        last_close_price_milli=9900,
        volume_raw=100,
        amount_raw=1000,
        volume_wire_value=100.0,
        volume_lots=1.0,
        amount=1000.0,
        open_delta_raw=0,
        close_delta_raw=500,
        high_delta_raw=1000,
        low_delta_raw=-1000,
    )


def _kline_series(when: datetime, *, raw: int = 0, parameter: int = 1) -> KlineSeries:
    return KlineSeries(
        exchange="sz",
        market_id=0,
        code="000001",
        period_raw=raw,
        period_param_raw=parameter,
        period_name="5m",
        start=0,
        request_count=1,
        adjust_mode_raw=0,
        adjust_mode="none",
        anchor_date_raw=0,
        anchor_date=None,
        bars=(_kline_bar(when),),
    )


# ── 校验与映射表 ────────────────────────────────────────────────────────────


@pytest.mark.parametrize("value", VALID_BAR_TIMES)
def test_check_bar_time_accepts_documented_values(value: str) -> None:
    check_bar_time(value)


def test_check_bar_time_rejects_unknown_value_with_actionable_message() -> None:
    with pytest.raises(ValueError) as excinfo:
        check_bar_time("middle")
    message = str(excinfo.value)
    assert "start" in message
    assert "end" in message
    assert "middle" in message


def test_ex_category_minutes_only_covers_intraday_periods() -> None:
    assert category_to_minutes(0) == 5
    assert category_to_minutes(1) == 15
    assert category_to_minutes(2) == 30
    assert category_to_minutes(3) == 60
    assert category_to_minutes(7) == 1
    assert category_to_minutes(8) == 3
    # 4/5/6/9/10/11/13 等日线及以上、多日、秒级都不偏移
    assert category_to_minutes(4) is None
    assert category_to_minutes(5) is None
    assert category_to_minutes(6) is None
    assert category_to_minutes(13) is None


def test_mac_period_minutes_scales_multi_minute_by_times() -> None:
    assert period_to_minutes(Period.MIN_5) == 5
    assert period_to_minutes(Period.MIN_1) == 1
    # MINS 是多分钟周期，分钟数 = 基数 × times
    assert period_to_minutes(Period.MINS, 1) == 5
    assert period_to_minutes(Period.MINS, 3) == 15
    # times 非法（0 / 负数）时至少按 1 倍
    assert period_to_minutes(Period.MINS, 0) == 5
    assert period_to_minutes(Period.MINS, -2) == 5
    # 日线及以上、DAYS、SECONDS 都不偏移
    assert period_to_minutes(Period.DAILY) is None
    assert period_to_minutes(Period.DAYS, 3) is None
    assert period_to_minutes(Period.SECONDS, 30) is None


def test_kline_period_minutes_uses_param_for_multi_minute() -> None:
    assert kline_period_minutes(0) == 5
    assert kline_period_minutes(1) == 15
    assert kline_period_minutes(2) == 30
    assert kline_period_minutes(3) == 60
    assert kline_period_minutes(7) == 1
    # raw=8 是 "{N}m"：N 直接就是 parameter，不是基数×倍数
    assert kline_period_minutes(8, 13) == 13
    assert kline_period_minutes(8, 1) == 1
    # 日线及以上 / 多日 / 秒级都不偏移
    assert kline_period_minutes(4) is None
    assert kline_period_minutes(9, 3) is None
    assert kline_period_minutes(13, 30) is None


# ── 分离字段（扩展行情）路径 ─────────────────────────────────────────────────


def test_hour_minute_start_returns_same_objects_untouched() -> None:
    bars = [_ex_bar(11, 25)]
    out = align_hour_minute_bars(bars, delta_minutes=5, bar_time=BAR_TIME_START)
    assert out is bars


def test_hour_minute_end_shifts_to_right_endpoint() -> None:
    bars = [_ex_bar(11, 25), _ex_bar(13, 0), _ex_bar(14, 55)]
    out = align_hour_minute_bars(bars, delta_minutes=5, bar_time=BAR_TIME_END)
    assert [(b.hour, b.minute) for b in out] == [(11, 30), (13, 5), (15, 0)]
    # 原列表不被就地修改
    assert [(b.hour, b.minute) for b in bars] == [(11, 25), (13, 0), (14, 55)]


def test_hour_minute_end_wraps_hour_within_the_day() -> None:
    """跨小时进位用 % 24，与 easy_tdx `_align_minutes_df` 一致。"""

    out = align_hour_minute_bars([_ex_bar(23, 58)], delta_minutes=5, bar_time=BAR_TIME_END)
    assert (out[0].hour, out[0].minute) == (0, 3)


def test_hour_minute_non_intraday_period_is_not_shifted() -> None:
    bars = [_ex_bar(0, 0)]
    assert align_hour_minute_bars(bars, delta_minutes=None, bar_time=BAR_TIME_END) is bars
    assert align_hour_minute_bars(bars, delta_minutes=0, bar_time=BAR_TIME_END) is bars
    assert align_hour_minute_bars(bars, delta_minutes=-5, bar_time=BAR_TIME_END) is bars


def test_hour_minute_empty_list_is_fine() -> None:
    assert align_hour_minute_bars([], delta_minutes=5, bar_time=BAR_TIME_END) == []


def test_invalid_bar_time_raises_even_when_period_is_not_intraday() -> None:
    """fail-fast：不能因为「日线不偏移」就静默接受拼错的 bar_time。"""

    with pytest.raises(ValueError):
        align_hour_minute_bars([_ex_bar(0, 0)], delta_minutes=None, bar_time="END")
    with pytest.raises(ValueError):
        align_datetime_bars([], delta_minutes=None, bar_time="end ")


# ── datetime（MAC）路径 ─────────────────────────────────────────────────────


def test_datetime_start_returns_same_objects_untouched() -> None:
    bars = [_mac_bar(datetime(2026, 10, 12, 11, 25))]
    assert align_datetime_bars(bars, delta_minutes=5, bar_time=BAR_TIME_START) is bars


def test_datetime_end_shifts_and_can_cross_a_day() -> None:
    """MAC 路径加真实 timedelta，因此可以跨日（与 7709 路径不同）。"""

    bars = [
        _mac_bar(datetime(2026, 10, 12, 11, 25)),
        _mac_bar(datetime(2026, 10, 12, 23, 30)),
    ]
    out = align_datetime_bars(bars, delta_minutes=60, bar_time=BAR_TIME_END)
    assert out[0].datetime == datetime(2026, 10, 12, 12, 25)
    assert out[1].datetime == datetime(2026, 10, 13, 0, 30)
    # 原对象不变
    assert bars[0].datetime == datetime(2026, 10, 12, 11, 25)


def test_time_bars_wrap_within_the_same_day() -> None:
    """7709 路径按 easy_tdx `_align_minutes_df` 语义：当天内 % 24 回绕，不滚到次日。

    两条路径的差异只在「跨零点」这种 A 股不可能出现的时刻上体现，
    但既然要逐条对齐 easy_tdx，这里就把差异钉住。
    """

    bars = [
        _kline_bar(datetime(2026, 10, 12, 11, 25)),
        _kline_bar(datetime(2026, 10, 12, 23, 58)),
    ]
    out = align_time_bars(bars, delta_minutes=5, bar_time=BAR_TIME_END, field="time")
    assert out[0].time == datetime(2026, 10, 12, 11, 30)
    # 23:58 + 5min → 次日 00:03 会被回绕成**同日** 00:03
    assert out[1].time == datetime(2026, 10, 12, 0, 3)


def test_time_bars_start_is_noop_and_invalid_raises() -> None:
    bars = [_kline_bar(datetime(2026, 10, 12, 11, 25))]
    assert align_time_bars(bars, delta_minutes=5, bar_time=BAR_TIME_START, field="time") is bars
    with pytest.raises(ValueError):
        align_time_bars(bars, delta_minutes=5, bar_time="nope", field="time")


# ── 客户端接线 ──────────────────────────────────────────────────────────────


def test_ex_get_instrument_bars_applies_bar_time(monkeypatch: pytest.MonkeyPatch) -> None:
    client = ExTdxClient()
    bars = [_ex_bar(11, 25)]
    monkeypatch.setattr(client, "_execute", lambda *_a, **_k: bars)

    # 默认 start：原样返回，连对象都不换
    assert client.get_instrument_bars(0, 0, "000001") is bars

    out = client.get_instrument_bars(0, 0, "000001", bar_time="end")
    assert [(b.hour, b.minute) for b in out] == [(11, 30)]

    # 非分钟级 category（4 = 日线）即使要 end 也不偏移
    assert client.get_instrument_bars(4, 0, "000001", bar_time="end") is bars


def test_ex_get_history_instrument_bars_range_rejects_end(monkeypatch: pytest.MonkeyPatch) -> None:
    client = ExTdxClient()
    bars = [_ex_bar(11, 25)]
    monkeypatch.setattr(client, "_execute", lambda *_a, **_k: bars)

    with pytest.raises(ValueError) as excinfo:
        client.get_history_instrument_bars_range(0, "000001", 20260101, 20260131, bar_time="end")
    assert "bar_time='end'" in str(excinfo.value)

    # start 正常透传
    assert client.get_history_instrument_bars_range(0, "000001", 20260101, 20260131) is bars

    # 非法值同样 fail-fast
    with pytest.raises(ValueError):
        client.get_history_instrument_bars_range(
            0, "000001", 20260101, 20260131, bar_time="nope"
        )


def test_mac_get_stock_kline_applies_bar_time(monkeypatch: pytest.MonkeyPatch) -> None:
    client = MacClient()
    bars = [_mac_bar(datetime(2026, 10, 12, 11, 25))]
    monkeypatch.setattr(client, "_fetch_kline_pages", lambda *_a, **_k: bars)

    assert client.get_stock_kline(0, "000001") is bars

    out = client.get_stock_kline(0, "000001", Period.MIN_5, bar_time="end")
    assert out[0].datetime == datetime(2026, 10, 12, 11, 30)

    # 日线不偏移
    assert client.get_stock_kline(0, "000001", Period.DAILY, bar_time="end") is bars


class _RecordingTransport:
    """记录调用并返回固定 KlineSeries 的假 transport。"""

    def __init__(self, series: KlineSeries) -> None:
        self.series = series
        self.calls: list[tuple[int, dict]] = []

    def execute(self, code: int, payload: dict):  # noqa: ANN001, ANN201
        self.calls.append((code, payload))
        return self.series


def test_bars_get_applies_bar_time_from_resolved_period() -> None:
    transport = _RecordingTransport(_kline_series(datetime(2026, 10, 12, 11, 25), raw=0))
    api = BarApi(transport)

    assert api.get("sz000001").bars[0].time == datetime(2026, 10, 12, 11, 25)

    page = api.get("sz000001", bar_time="end")
    assert page.bars[0].time == datetime(2026, 10, 12, 11, 30)
    # 除 bars 外的元数据必须原样保留
    assert page.period_raw == 0
    assert page.period_param_raw == 1
    assert page.code == "000001"


def test_bars_get_uses_period_param_for_multi_minute() -> None:
    """raw=8 时分钟数取自 period_param_raw（"13m" → 13 分钟）。"""

    series = _kline_series(datetime(2026, 10, 12, 9, 30), raw=8, parameter=13)
    api = BarApi(_RecordingTransport(series))
    page = api.get("sz000001", period="13m", bar_time="end")
    assert page.bars[0].time == datetime(2026, 10, 12, 9, 43)


def test_bars_get_does_not_shift_daily_bars() -> None:
    series = _kline_series(datetime(2026, 10, 12, 0, 0), raw=4)
    api = BarApi(_RecordingTransport(series))
    page = api.get("sz000001", period="day", bar_time="end")
    assert page.bars[0].time == datetime(2026, 10, 12, 0, 0)


def test_bars_get_rejects_invalid_bar_time() -> None:
    api = BarApi(_RecordingTransport(_kline_series(datetime(2026, 10, 12, 9, 30))))
    with pytest.raises(ValueError):
        api.get("sz000001", bar_time="invalid")
