"""eltdx.cli 命令行测试（全部离线，不依赖网络与 native 扩展）。"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import pytest

from eltdx import cli
from eltdx.mac import Adjust, Period
from eltdx.offline import DailyBar, write_daily_bar


# ── 参数解析辅助函数 ────────────────────────────────────────────────────


def test_codes_expands_prefix_and_separators() -> None:
    assert cli._codes(["sz000001", "sh600519"]) == ["sz000001", "sh600519"]
    assert cli._codes(["sz000001,sh600519"]) == ["sz000001", "sh600519"]
    assert cli._codes(["SH 600519"]) == ["SH", "600519"]
    assert cli._codes(["  ", "000001"]) == ["000001"]


def test_single_code_rejects_multiple() -> None:
    assert cli._single_code(["sz000001"]) == "sz000001"
    with pytest.raises(ValueError):
        cli._single_code(["sz000001", "sh600519"])


def test_codes_accepts_bare_string_from_optional_positional() -> None:
    """``nargs="?"`` 位置参数是裸字符串，必须当一个代码，不能按字符展开。"""
    assert cli._codes("sh000016") == ["sh000016"]
    assert cli._codes("sz000001,sh600519") == ["sz000001", "sh600519"]
    assert cli._codes(None) == []
    assert cli._single_code("sh000016") == "sh000016"
    with pytest.raises(ValueError):
        cli._single_code(None)




def test_mac_stocks_accepts_three_notations() -> None:
    assert cli._mac_stocks(["SZ 000001,SH 600000"]) == [(0, "000001"), (1, "600000")]
    assert cli._mac_stocks(["sz000001"]) == [(0, "000001")]
    assert cli._mac_stocks(["600519"]) == [(1, "600519")]
    assert cli._mac_stocks(["000001", "600519"]) == [(0, "000001"), (1, "600519")]


def test_mac_market_parses_names_and_numbers() -> None:
    assert cli._mac_market("SZ") == 0
    assert cli._mac_market("sh") == 1
    assert cli._mac_market("BJ") == 2
    assert cli._mac_market("1") == 1
    with pytest.raises(ValueError):
        cli._mac_market("XX")


def test_infer_mac_market_uses_library_normalisation() -> None:
    assert cli._infer_mac_market("000001") == 0
    assert cli._infer_mac_market("600519") == 1
    assert cli._infer_mac_market("830799") == 2


@pytest.mark.parametrize(
    ("text", "expected"),
    [
        ("day", Period.DAILY),
        ("1D", Period.DAILY),
        ("daily", Period.DAILY),
        ("5m", Period.MIN_5),
        ("5min", Period.MIN_5),
        ("60m", Period.MIN_60),
        ("hour", Period.MIN_60),
        ("week", Period.WEEKLY),
        ("month", Period.MONTHLY),
        ("quarter", Period.QUARTERLY),
        ("year", Period.YEARLY),
        ("minute", Period.MIN_1),
    ],
)
def test_period_maps_to_enum(text: str, expected: Period) -> None:
    assert cli._period(text) is expected


def test_period_rejects_unknown() -> None:
    with pytest.raises(ValueError):
        cli._period("yearly-ish")


@pytest.mark.parametrize(
    ("text", "expected"),
    [("none", Adjust.NONE), ("", Adjust.NONE), ("bfq", Adjust.NONE),
     ("QFQ", Adjust.QFQ), ("hfq", Adjust.HFQ)],
)
def test_adjust_maps_to_enum(text: str, expected: Adjust) -> None:
    assert cli._adjust(text) is expected


def test_adjust_rejects_unknown() -> None:
    with pytest.raises(ValueError):
        cli._adjust("sideways")


def test_trade_date_accepts_both_formats() -> None:
    assert cli._trade_date(None) is None
    assert cli._trade_date("20260102") == cli._trade_date("2026-01-02")
    assert cli._int_date("20260102") == 20260102
    with pytest.raises(ValueError):
        cli._trade_date("01/02/2026")


# ── 输出渲染 ────────────────────────────────────────────────────────────


def test_rows_flattens_dict_of_dicts_and_lists() -> None:
    assert cli._rows({"sz000001": {"close": 1.0}}) == [{"code": "sz000001", "close": 1.0}]
    assert cli._rows([1, 2]) == [{"value": 1}, {"value": 2}]
    assert cli._rows("x") == [{"value": "x"}]
    assert cli._rows({"a": 1}) == [{"a": 1}]


def test_render_table_handles_empty_and_rows() -> None:
    assert cli._render_table([]) == "(无数据)"
    table = cli._render_table([{"a": 1, "b": "xy"}, {"a": 2, "b": "z"}])
    assert "| a " in table
    assert "2 行" in table


def test_cell_truncates_and_formats() -> None:
    assert cli._cell(None) == ""
    assert cli._cell(True) == "true"
    assert cli._cell(1.25) == "1.25"
    assert cli._cell(1.0) == "1"
    assert cli._cell("abcdef", limit=4) == "abc…"
    assert cli._cell([1, 2]) == "[1,2]"


def _ns(**kwargs: object) -> argparse.Namespace:
    base: dict[str, object] = {"output": "json", "indent": None}
    base.update(kwargs)
    return argparse.Namespace(**base)


def test_emit_json_jsonl_table_csv(capsys: pytest.CaptureFixture[str]) -> None:
    payload = [{"code": "000001", "close": 1.5}, {"code": "000002", "close": 2.5}]

    assert cli.emit(_ns(output="json"), payload) == 0
    assert json.loads(capsys.readouterr().out) == payload

    assert cli.emit(_ns(output="jsonl"), payload) == 0
    lines = capsys.readouterr().out.strip().splitlines()
    assert [json.loads(line)["code"] for line in lines] == ["000001", "000002"]

    assert cli.emit(_ns(output="table"), payload) == 0
    assert "000001" in capsys.readouterr().out

    assert cli.emit(_ns(output="csv"), payload) == 0
    csv_text = capsys.readouterr().out
    assert csv_text.splitlines()[0] == "code,close"
    assert "000002,2.5" in csv_text


def test_fail_writes_stderr_and_returns_nonzero(capsys: pytest.CaptureFixture[str]) -> None:
    assert cli.fail("boom") == 1
    assert "boom" in capsys.readouterr().err


# ── 解析器 / 入口 ───────────────────────────────────────────────────────


def test_build_parser_exposes_all_commands() -> None:
    parser = cli.build_parser()
    actions = [a for a in parser._actions if isinstance(a, argparse._SubParsersAction)]
    assert actions, "缺少子命令"
    commands = set(actions[0].choices)
    assert {"version", "quote", "kline", "ex", "mac", "macex", "fund", "offline"} <= commands


def test_main_version_is_offline_and_succeeds(capsys: pytest.CaptureFixture[str]) -> None:
    assert cli.main(["version"]) == 0
    payload = json.loads(capsys.readouterr().out)
    assert "version" in payload and "python" in payload


def test_main_without_command_prints_help(capsys: pytest.CaptureFixture[str]) -> None:
    assert cli.main([]) == 0
    assert "场外基金" in capsys.readouterr().out


def test_main_help_exits_zero() -> None:
    with pytest.raises(SystemExit) as excinfo:
        cli.main(["--help"])
    assert excinfo.value.code == 0


def test_main_rejects_unknown_option() -> None:
    with pytest.raises(SystemExit) as excinfo:
        cli.main(["fund", "nav", "--bogus", "016947"])
    assert excinfo.value.code == 2


def test_main_reattaches_positional_after_options(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    """`macex quote --market 33 016947` 这类写法要能识别到 code。"""

    seen: dict[str, object] = {}

    class _FakeMacEx:
        def goods_quotes(self, stocks: list[tuple[int, str]]) -> list[object]:
            seen["stocks"] = stocks
            return []

        def close(self) -> None:
            return None

    monkeypatch.setattr(cli, "_mac_ex", lambda args: _FakeMacEx())
    assert cli.main(["macex", "quote", "--market", "33", "016947"]) == 0
    assert seen["stocks"] == [(33, "016947")]
    capsys.readouterr()


def test_main_reports_client_errors_via_fail(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    monkeypatch.setattr(cli, "_mac_ex", lambda args: (_ for _ in ()).throw(ValueError("bad code")))
    assert cli.main(["fund", "nav", "016947"]) == 1
    assert "bad code" in capsys.readouterr().err


# ── 无需网络的子命令 ────────────────────────────────────────────────────


def test_fund_markets_lists_open_end_fund(capsys: pytest.CaptureFixture[str]) -> None:
    assert cli.main(["fund", "markets"]) == 0
    rows = json.loads(capsys.readouterr().out)
    markets = {row["market"]: row["name"] for row in rows}
    assert markets[33] == "开放式基金"
    assert markets[34] == "货币基金"


def test_fund_nav_requires_a_code(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    class _FakeMacEx:
        def goods_quotes(self, stocks: list[tuple[int, str]]) -> list[object]:
            return []

        def close(self) -> None:
            return None

    monkeypatch.setattr(cli, "_mac_ex", lambda args: _FakeMacEx())
    assert cli.main(["fund", "nav"]) == 1
    assert "至少一个基金代码" in capsys.readouterr().err


def test_ex_markets_uses_local_enum_without_touching_the_network(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    """默认不应联系服务器：该命令在所有内置主站上只会阻塞到超时。"""

    def _boom(args: argparse.Namespace) -> object:
        raise AssertionError("默认路径不应创建连接")

    monkeypatch.setattr(cli, "_ex", _boom)
    assert cli.main(["ex", "markets"]) == 0
    captured = capsys.readouterr()
    rows = json.loads(captured.out)
    by_market = {row["market"]: row["name"] for row in rows}
    assert by_market[33] == "OPEN_END_FUND"
    assert by_market[47] == "CFFEX_FUTURES"
    assert captured.err == ""


def test_ex_markets_probe_server_falls_back_to_local_enum(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    """显式 --probe-server 时仍先问服务器；失败则回退到本地 ExMarket 枚举。"""

    def _boom(args: argparse.Namespace) -> object:
        raise RuntimeError("no markets")

    monkeypatch.setattr(cli, "_ex", _boom)
    assert cli.main(["ex", "markets", "--probe-server"]) == 0
    captured = capsys.readouterr()
    rows = json.loads(captured.out)
    by_market = {row["market"]: row["name"] for row in rows}
    assert by_market[33] == "OPEN_END_FUND"
    assert by_market[47] == "CFFEX_FUTURES"
    assert "回退" in captured.err


def test_offline_day_reads_a_written_file(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    path = tmp_path / "sz000001.day"
    bars = [
        DailyBar(year=2026, month=6, day=5, open=10.0, high=10.5, low=9.9,
                 close=10.2, volume=1000.0, amount=10200.0, adjust_flag=0),
        DailyBar(year=2026, month=6, day=6, open=10.2, high=10.8, low=10.1,
                 close=10.6, volume=1200.0, amount=12720.0, adjust_flag=0),
    ]
    write_daily_bar(path, bars)

    assert cli.main(["offline", "day", "--file", str(path)]) == 0
    assert len(json.loads(capsys.readouterr().out)) == 2

    assert cli.main(["offline", "day", "--file", str(path), "--last"]) == 0
    assert json.loads(capsys.readouterr().out)["close"] == 10.6

    assert cli.main(["offline", "day", "--file", str(path), "--count", "1"]) == 0
    assert len(json.loads(capsys.readouterr().out)) == 1


def test_offline_missing_file_reports_error(capsys: pytest.CaptureFixture[str]) -> None:
    assert cli.main(["offline", "day", "--file", "does-not-exist.day"]) == 1
    assert "文件不存在" in capsys.readouterr().err


def test_offline_requires_file_or_market_and_code(capsys: pytest.CaptureFixture[str]) -> None:
    assert cli.main(["offline", "day"]) == 1
    assert "--file" in capsys.readouterr().err

# ── kline：`nargs="?"` 的裸字符串代码 ───────────────────────────────────


class _RecordingBars:
    def __init__(self) -> None:
        self.calls: list[tuple] = []

    def get(self, code: object, **kwargs: object) -> list:
        self.calls.append((code, kwargs))
        return []


def _record_bars(monkeypatch: pytest.MonkeyPatch) -> _RecordingBars:
    bars = _RecordingBars()

    class _FakeTdx:
        def __init__(self) -> None:
            self.bars = bars

        def close(self) -> None:
            return None

    monkeypatch.setattr(cli, "_tdx", lambda args: _FakeTdx())
    return bars


def test_kline_single_positional_code_is_not_split_into_characters(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """回归：`eltdx kline sh000016 --period day` 曾把代码按字符拆成 8 个。"""
    bars = _record_bars(monkeypatch)
    assert cli.main(["kline", "sh000016", "--period", "day", "--count", "5"]) == 0
    (code, kwargs), = bars.calls
    assert code == "sh000016"
    assert kwargs["period"] == "day"
    assert kwargs["count"] == 5


def test_kline_accepts_code_after_options(monkeypatch: pytest.MonkeyPatch) -> None:
    bars = _record_bars(monkeypatch)
    assert cli.main(["kline", "--period", "day", "sh000016"]) == 0
    assert [code for code, _ in bars.calls] == ["sh000016"]


def test_kline_rejects_multiple_codes(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    bars = _record_bars(monkeypatch)
    assert cli.main(["kline", "sh000016", "sz000001"]) == 1
    assert "只接受一个代码" in capsys.readouterr().err
    assert bars.calls == []

# ── ex：`nargs="*"` 的代码列表不能裸传给单码接口 ────────────────────────


class _RecordingEx:
    def __init__(self) -> None:
        self.calls: list[tuple] = []

    def get_instrument_quote(self, market: int, code: object) -> list:
        self.calls.append(("quote", market, code))
        return []

    def get_instrument_bars(
        self, category: int, market: int, code: object, start: int, count: int
    ) -> list:
        self.calls.append(("bars", category, market, code, start, count))
        return []

    def close(self) -> None:
        return None


def test_ex_passes_code_as_single_string(monkeypatch: pytest.MonkeyPatch) -> None:
    """回归：`eltdx ex quote --market 33 IFL0` 曾把 ``['IFL0']`` 裸传给只收 str 的接口。"""
    fake = _RecordingEx()
    monkeypatch.setattr(cli, "_ex", lambda args: fake)
    assert cli.main(["ex", "quote", "--market", "33", "IFL0"]) == 0
    assert fake.calls == [("quote", 33, "IFL0")]
    assert cli.main(["ex", "bars", "--market", "33", "--category", "2", "IFL0"]) == 0
    assert fake.calls[-1] == ("bars", 2, 33, "IFL0", 0, 20)


def test_ex_rejects_multiple_codes(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    fake = _RecordingEx()
    monkeypatch.setattr(cli, "_ex", lambda args: fake)
    assert cli.main(["ex", "quote", "--market", "33", "IFL0", "IFL1"]) == 1
    assert "只接受一个代码" in capsys.readouterr().err
    assert fake.calls == []
