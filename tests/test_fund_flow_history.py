"""7615 TQLEX 历史资金流向（``MoneyFlowApi.history``）的离线测试。

三件事分开验：

1. **协议 → 模型**：用 `_native_models.response_from_dto` 喂一个合成 DTO，
   锁住 category-22 DTO（`FundFlowHistorySeries`，Rust 侧仍在导出）的字段顺序。
2. **API 行为**：用假 7615 F10 客户端驱动 `MoneyFlowApi.history`，验请求形状
   （`CWServ.tdxf10_gg_sj` + `zjlx_sj`）、`days` 校验、客户端 `start` / `count`
   切片、单只/批量返回类型。
3. **接线**：没有 `f10_factory` 时 `history()` 必须直接报错，而不是退回 7709。

全部离线，不建立任何 socket：7709 transport 一旦被调用就直接失败。
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from datetime import date
from typing import Any

import pytest

from eltdx._native_models import response_from_dto
from eltdx.api.bars import BarApi
from eltdx.api.money_flow import F10_HISTORY_ENTRY, MoneyFlowApi
from eltdx.exceptions import TransportError
from eltdx.models import FundFlowHistoryRecord, FundFlowHistorySeries
from eltdx.models.money_flow import FundFlowRangeSeries


def _raw_row(index: int = 0) -> dict[str, Any]:
    """一条 ``CWServ.tdxf10_gg_sj`` 原始行（列名照抄服务端）。"""

    return {
        "N001": 20261013 - index,
        "N002": "000001",
        "N003": "平安银行",
        "N004": -149341409.0,
        "N005": 1234.0,
        "N006": -5678.0,
        "N007": 91011.0,
        "N009": 19910403,
    }


@dataclass
class _FakeResponse:
    rows: tuple[dict[str, Any], ...]
    raw: dict[str, Any] = field(default_factory=dict)


class _FakeF10:
    """假 7615 F10 客户端：只记录 ``params`` 调用并返回预置行。"""

    def __init__(self, rows: Any = None, *, error: Exception | None = None) -> None:
        self.rows = tuple(rows if rows is not None else (_raw_row(),))
        self.error = error
        self.calls: list[tuple[str, tuple[Any, ...]]] = []

    def __call__(self) -> _FakeF10:
        # 充当 f10_factory：MoneyFlowApi 解析出回调后会再调用一次。
        return self

    def params(self, entry: str, *params: Any) -> _FakeResponse:
        self.calls.append((entry, params))
        if self.error is not None:
            raise self.error
        return _FakeResponse(
            rows=self.rows,
            raw={"ErrorCode": 0, "ResultSets": [{"Content": list(self.rows)}]},
        )


class _UnusedTransport:
    """原 7709 通道：新的 ``history()`` 不该再碰它。"""

    def __init__(self) -> None:
        self.closed = False

    def execute(self, command: int, payload: dict):  # pragma: no cover - 不该被调用
        raise AssertionError(
            f"money_flow.history 不应再走 7709 命令 0x{command:04x}"
        )

    def close(self) -> None:
        self.closed = True

    @property
    def pool_size(self) -> int:
        return 1


def _api(f10: _FakeF10 | None = None, *, transport=None) -> MoneyFlowApi:
    """假客户端本身可调用，当作 `f10_factory`；弱引用，必须由调用方保活。"""

    return MoneyFlowApi(
        transport if transport is not None else _UnusedTransport(),
        f10_factory=f10,
    )


# ── 协议 → 模型（category-22 DTO 仍在 Rust 侧导出） ─────────────────────────────


def _record(index: int = 0, *, with_hex: bool = False) -> FundFlowHistoryRecord:
    return FundFlowHistoryRecord(
        date_raw=20261000 + index,
        date=date(2026, 10, 1 + index),
        super_large_in=1.0,
        large_in=2.0,
        medium_in=3.0,
        small_in=4.0,
        super_large_out=5.0,
        large_out=6.0,
        medium_out=7.0,
        small_out=8.0,
        record_hex="ab" * 36 if with_hex else "",
    )


def test_dto_conversion_maps_fields_and_decodes_dates() -> None:
    dto = (
        "history_fund_flow",
        (
            "sz",
            0,
            "000001",
            0,
            5,
            (
                (20261001, (2026, 10, 1), 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, "cd" * 36),
                (20261002, (2026, 10, 2), 9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, "ef" * 36),
            ),
            b"",
        ),
    )
    series = response_from_dto(dto)

    assert isinstance(series, FundFlowHistorySeries)
    assert series.full_code == "sz000001"
    assert series.start == 0
    assert series.request_count == 5
    assert series.count == 2
    first, second = series.records
    assert first.date == date(2026, 10, 1)
    assert first.total_in == pytest.approx(10.0)
    assert first.total_out == pytest.approx(26.0)
    assert first.net == pytest.approx(-16.0)
    assert len(first.record_hex) == 72
    assert second.date == date(2026, 10, 2)
    assert series.raw_payload == b""


def test_dto_conversion_rejects_a_wrong_shape() -> None:
    with pytest.raises(TypeError):
        response_from_dto(("history_fund_flow", ["sz", 0, "000001", 0, 5, (), b""]))
    with pytest.raises(ValueError):
        response_from_dto(("history_fund_flow", ("sz", 0, "000001", 0, 5, (), b"", "extra")))


# ── 请求形状 ──────────────────────────────────────────────────────────────────


def test_history_requests_the_tqlex_channel_with_the_sixty_day_default() -> None:
    f10 = _FakeF10()
    api = _api(f10)

    series = api.history("sz000001")

    assert isinstance(series, FundFlowRangeSeries)
    assert series.code == "sz000001"
    assert series.days == 60
    assert series.codes == ("000001",)
    assert series.count == 1
    assert series.rows[0]["N004"] == -149341409.0
    assert series.rows[0]["N009"] == 19910403
    assert f10.calls == [(F10_HISTORY_ENTRY, ("zjlx_sj", "000001", "60", "000001"))]


@pytest.mark.parametrize("days", [3, 5, 10, 20, 60])
def test_history_passes_the_window_step_to_the_server(days) -> None:
    f10 = _FakeF10()

    series = _api(f10).history("000001", days=days)

    assert series.days == days
    assert f10.calls == [(F10_HISTORY_ENTRY, ("zjlx_sj", "000001", str(days), "000001"))]


def test_history_accepts_the_prefixed_and_bare_code_forms() -> None:
    f10 = _FakeF10()
    api = _api(f10)

    assert api.history("sh600000").codes == ("600000",)
    assert api.history("600000").codes == ("600000",)

    assert [call[1][1] for call in f10.calls] == ["600000", "600000"]


# ── 客户端切片 ────────────────────────────────────────────────────────────────


def test_history_slices_the_series_on_the_client() -> None:
    rows = tuple(_raw_row(index) for index in range(5))
    f10 = _FakeF10(rows)
    api = _api(f10)

    assert api.history("000001").rows == rows
    assert api.history("000001", start=2).rows == rows[2:]
    assert api.history("000001", start=1, count=2).rows == rows[1:3]
    assert api.history("000001", count=3).rows == rows[:3]
    assert api.history("000001", start=9).rows == ()
    # 切片只在客户端做：请求体里的 days 始终不变。
    assert {call[1][2] for call in f10.calls} == {"60"}


# ── 校验 ─────────────────────────────────────────────────────────────────────


@pytest.mark.parametrize("days", [0, 1, 2, 4, 30, 90, True, 3.0, "60", None])
def test_history_rejects_an_unsupported_window(days) -> None:
    f10 = _FakeF10()
    api = _api(f10)
    with pytest.raises(ValueError, match="days must be one of: 3, 5, 10, 20, 60"):
        api.history("sz000001", days=days)


@pytest.mark.parametrize("count", [0, -1, 801, True, 1.5])
def test_history_rejects_an_invalid_count(count) -> None:
    f10 = _FakeF10()
    api = _api(f10)
    with pytest.raises(ValueError, match="count must be between 1 and"):
        api.history("sz000001", count=count)


@pytest.mark.parametrize("start", [-1, 1.5, True, "0"])
def test_history_rejects_an_invalid_start(start) -> None:
    f10 = _FakeF10()
    api = _api(f10)
    with pytest.raises(ValueError, match="start must be a non-negative integer"):
        api.history("sz000001", start=start)


@pytest.mark.parametrize("batch_size", [0, -3, True, 2.5])
def test_history_rejects_an_invalid_batch_size(batch_size) -> None:
    f10 = _FakeF10()
    api = _api(f10)
    with pytest.raises(ValueError, match="batch_size must be a positive integer"):
        api.history("sz000001", batch_size=batch_size)


def test_history_rejects_an_empty_code_sequence() -> None:
    f10 = _FakeF10()
    api = _api(f10)
    with pytest.raises(ValueError, match="codes must not be empty"):
        api.history([])


# ── 通道接线 ──────────────────────────────────────────────────────────────────


def test_history_without_an_f10_factory_raises_before_any_io() -> None:
    """裸用 ``MoneyFlowApi(transport)``（没有 7615 通道）必须直接报错。"""

    transport = _UnusedTransport()
    api = MoneyFlowApi(transport)

    with pytest.raises(RuntimeError, match="7615 F10 通道"):
        api.history("sz000001")
    with pytest.raises(RuntimeError, match="7615 F10 通道"):
        api.history(["sz000001", "sh600000"])
    assert transport.closed is False


def test_history_normalizes_codes_and_returns_a_dict_for_a_sequence() -> None:
    f10 = _FakeF10()
    api = _api(f10)

    result = api.history(["000001", "sh600000"], days=20)

    assert isinstance(result, dict)
    assert sorted(result) == ["sh600000", "sz000001"]
    assert all(isinstance(item, FundFlowRangeSeries) for item in result.values())
    assert result["sh600000"].codes == ("600000",)
    assert result["sz000001"].days == 20
    assert sorted(call[1][1] for call in f10.calls) == ["000001", "600000"]
    assert {call[1][2] for call in f10.calls} == {"20"}


def test_history_keeps_the_raw_json_only_when_asked() -> None:
    f10 = _FakeF10()
    api = _api(f10)

    assert api.history("000001").raw_payload == b""

    payload = api.history("000001", include_raw=True).raw_payload
    assert json.loads(payload.decode("utf-8"))["ErrorCode"] == 0


def test_history_propagates_the_f10_transport_error() -> None:
    # f10_factory 是弱引用：假客户端必须留在局部变量里，否则函数返回后立刻失效。
    f10 = _FakeF10(error=TransportError("gateway down"))
    api = _api(f10)

    with pytest.raises(TransportError, match="gateway down"):
        api.history("sz000001")


# ── 与 K 线 API 的边界 ────────────────────────────────────────────────────────


def test_bar_api_rejects_the_flow_period_and_points_at_the_money_flow_api() -> None:
    """``period='flow'`` 是资金流序列，不是 OHLC，必须显式拒绝而不是返回异构对象。"""

    class _NullTransport:
        def execute(self, command: int, payload: dict):  # pragma: no cover - 不该被调用
            raise AssertionError("bar API must reject the flow period before any I/O")

    api = BarApi(_NullTransport())
    with pytest.raises(ValueError, match="money_flow.history"):
        api.get("sz000001", period="flow")
