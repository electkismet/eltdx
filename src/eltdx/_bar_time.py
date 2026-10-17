"""K 线时间戳语义对齐（``bar_time``）。

通达信给出的分钟级 bar 时刻是 bar 的**开始**时间（上午最后一根 5 分钟 bar 标
``11:25``、下午第一根标 ``13:00``）；Tushare / 同花顺用的是 bar 的**结束**时间
（同一根应标 ``11:30``）。``bar_time="end"`` 会给分钟级 bar 的时刻加上一个周期
时长来完成对齐。

日线及以上周期不受影响——它们只有日期，没有"一根 bar 跨多久"的问题。

本模块与 ``easy_tdx._df`` 里的同名工具语义一致，但 eltdx 返回的是 dataclass 列表
而非 DataFrame，所以这里直接对 dataclass 做 ``replace``：

* :func:`align_hour_minute_bars` 用于字段分散的模型（扩展行情 ``ExInstrumentBar``
  有 ``hour`` / ``minute``），按 easy_tdx ``_align_minutes_df`` 的 ``% 24`` 语义进位；
* :func:`align_datetime_bars` 用于单一 ``datetime`` 字段的模型（MAC ``MacBar``），
  按 easy_tdx ``_align_datetime_df`` 的语义加真实 ``timedelta``（可跨日）。
"""

from __future__ import annotations

from dataclasses import replace
from datetime import timedelta
from typing import TypeVar

BAR_TIME_START = "start"
BAR_TIME_END = "end"

#: 合法取值。非法值 fail-fast，避免静默按 "end" 处理。
VALID_BAR_TIMES: tuple[str, ...] = (BAR_TIME_START, BAR_TIME_END)

#: 扩展行情 ``KlineCategory`` → 每根 bar 的分钟数（仅分钟级；日线及以上不在此表）。
#: ``0=MIN_5 1=MIN_15 2=MIN_30 3=MIN_60 7=MIN_1 8=MIN_3``
EX_CATEGORY_MINUTES: dict[int, int] = {0: 5, 1: 15, 2: 30, 3: 60, 7: 1, 8: 3}

#: MAC 协议 ``Period`` → 每根 bar 的分钟数（仅分钟级）。
#: ``0=MIN_5 1=MIN_15 2=MIN_30 3=MIN_60 7=MIN_1 8=MINS``
MAC_PERIOD_MINUTES: dict[int, int] = {0: 5, 1: 15, 2: 30, 3: 60, 7: 1, 8: 5}

#: 7709 ``KlinePeriod.raw``（见 Rust ``unit.rs`` 的 ``KlinePeriod``）→ 每根 bar 的分钟数。
#: ``0=5m 1=15m 2=30m 3=60m 7=1m``；``8`` 是 ``{N}m`` 多分钟（N = parameter，单列处理）；
#: ``4=day 5=week 6=month 9={N}d 10=quarter 11=year 13={N}s`` 均不偏移。
KLINES_RAW_MINUTES: dict[int, int] = {0: 5, 1: 15, 2: 30, 3: 60, 7: 1}

_BarT = TypeVar("_BarT")


def check_bar_time(bar_time: str) -> None:
    """校验 ``bar_time`` 取值，非法值立即抛 :class:`ValueError`。"""

    if bar_time not in VALID_BAR_TIMES:
        raise ValueError(
            f"bar_time 必须是 {BAR_TIME_START!r} 或 {BAR_TIME_END!r}，得到: {bar_time!r}"
        )


def category_to_minutes(category: int) -> int | None:
    """扩展行情 ``KlineCategory`` → 每根 bar 的分钟数；日线及以上返回 ``None``。"""

    return EX_CATEGORY_MINUTES.get(int(category))


def period_to_minutes(period: int, times: int = 1) -> int | None:
    """MAC 协议 ``Period`` → 每根 bar 的分钟数；日线及以上 / 秒级返回 ``None``。

    ``Period.MINS``（8）是多分钟周期，需乘以倍数 ``times``；秒级按分钟粒度对齐
    没有意义，返回 ``None``。
    """

    base = MAC_PERIOD_MINUTES.get(int(period))
    if base is None:
        # 4=DAILY 5=WEEKLY 6=MONTHLY 9=DAYS 10=QUARTERLY 11=YEARLY 13=SECONDS 均不偏移
        return None
    if int(period) == 8:  # MINS：多分钟，乘以倍数
        return base * max(int(times), 1)
    return base


def kline_period_minutes(raw: int, parameter: int = 1) -> int | None:
    """7709 ``KlinePeriod``（``raw`` + ``parameter``）→ 每根 bar 的分钟数。

    日线及以上、多日、秒级都返回 ``None``（不偏移）。取 ``KlineSeries``
    已解析出的 ``period_raw`` / ``period_param_raw`` 即可，无需重复实现
    「周期名 → 编号」的映射，避免与 Rust 侧漂移。
    """

    if int(raw) == 8:  # "{N}m" 多分钟：N 就是 parameter
        return max(int(parameter), 1)
    return KLINES_RAW_MINUTES.get(int(raw))


def _needs_align(bar_time: str, delta_minutes: int | None) -> bool:
    """是否真的需要偏移；``"start"`` 与"非分钟级"都原样返回。"""

    if bar_time == BAR_TIME_START:
        return False
    check_bar_time(bar_time)
    return delta_minutes is not None and delta_minutes > 0


def align_hour_minute_bars(
    bars: list[_BarT],
    *,
    delta_minutes: int | None,
    bar_time: str,
) -> list[_BarT]:
    """对字段分散（``hour`` / ``minute``）的 bar 列表应用对齐。

    用 :func:`dataclasses.replace` 重建，保持 dataclass 不可变语义；跨小时自动进位
    （``% 24``，与 easy_tdx ``_align_minutes_df`` 一致）。收盘 bar 不会跨日，故不处理跨日。
    """

    if not _needs_align(bar_time, delta_minutes):
        return bars
    assert delta_minutes is not None  # _needs_align 已保证
    result: list[_BarT] = []
    for bar in bars:
        total = bar.hour * 60 + bar.minute + delta_minutes  # type: ignore[attr-defined]
        result.append(
            replace(bar, hour=(total // 60) % 24, minute=total % 60)  # type: ignore[type-var]
        )
    return result


def align_time_bars(
    bars: list[_BarT],
    *,
    delta_minutes: int | None,
    bar_time: str,
    field: str = "time",
) -> list[_BarT]:
    """对只有单一 ``datetime`` 字段、但需按**当天内回绕**对齐的 bar 列表应用对齐。

    用于 7709 的 ``KlineBar``：easy_tdx 的 ``get_security_bars`` / ``get_index_bars``
    走的是 ``has_time_columns=True`` 分支（``_align_minutes_df``），即先平移
    ``hour``/``minute`` 再拼 datetime，跨小时进位用 ``% 24``、**不回绕到次日**。
    这里保持同样的语义，以便与 easy_tdx 逐条对齐（MAC 的 ``get_stock_kline`` 走的是
    :func:`align_datetime_bars`，那条路径加真实 ``timedelta``、可以跨日）。
    """

    if not _needs_align(bar_time, delta_minutes):
        return bars
    assert delta_minutes is not None
    result: list[_BarT] = []
    for bar in bars:
        when = getattr(bar, field)  # type: ignore[attr-defined]
        total = when.hour * 60 + when.minute + delta_minutes
        result.append(
            replace(
                bar,
                **{field: when.replace(hour=(total // 60) % 24, minute=total % 60)},
            )  # type: ignore[type-var]
        )
    return result


def align_datetime_bars(
    bars: list[_BarT],
    *,
    delta_minutes: int | None,
    bar_time: str,
    field: str = "datetime",
) -> list[_BarT]:
    """对只有单一时刻字段的 bar 列表应用对齐（MAC / 7709 K 线路径）。

    ``field`` 指定时刻字段名：MAC 的 ``MacBar`` 用 ``datetime``，7709 的
    ``KlineBar`` 用 ``time``。

    与 :func:`align_hour_minute_bars` 不同，这里加的是真实 ``timedelta``，因此**可以跨日**
    ——与 easy_tdx ``_align_datetime_df`` 一致（MAC 走的是这条路径）。
    """

    if not _needs_align(bar_time, delta_minutes):
        return bars
    assert delta_minutes is not None
    delta = timedelta(minutes=delta_minutes)
    result: list[_BarT] = []
    for bar in bars:
        result.append(
            replace(bar, **{field: getattr(bar, field) + delta})  # type: ignore[type-var]
        )
    return result


__all__ = [
    "BAR_TIME_END",
    "BAR_TIME_START",
    "EX_CATEGORY_MINUTES",
    "KLINES_RAW_MINUTES",
    "MAC_PERIOD_MINUTES",
    "VALID_BAR_TIMES",
    "align_datetime_bars",
    "align_hour_minute_bars",
    "align_time_bars",
    "category_to_minutes",
    "check_bar_time",
    "kline_period_minutes",
    "period_to_minutes",
]
