from types import SimpleNamespace

import pytest

from eltdx.helpers.core import HelperApi

from eltdx.helpers.boards import BoardService


def _fixture_files(tmp_path, count=81):
    definitions = "".join(f"板块{i}|88{i:04d}|4|1|0|X\n" for i in range(1, count + 1))
    (tmp_path / "tdxzs.cfg").write_text(definitions, encoding="utf-8")
    (tmp_path / "tdxzs3.cfg").write_text("", encoding="utf-8")
    lines = []
    for i in range(1, count + 1):
        code = f"88{i:04d}"
        members = ",".join(f"0#{j:06d}" for j in range(1, 82))
        lines.append(f"#板块{i},81,{code},20200101,20260101,,\n{members}")
    (tmp_path / "infoharbor_block.dat").write_text("\n".join(lines), encoding="utf-8")
    (tmp_path / "tdxhy.cfg").write_text("", encoding="utf-8")


class FakeClient:
    def __init__(self):
        self.resource_calls = 0
        self.code_calls = 0
        self.quote_batches = []
        self.resources = SimpleNamespace(download_file=self._download)
        self.codes = SimpleNamespace(all=self._codes)
        self.quotes = SimpleNamespace(get_snapshots=self._quotes)
        self.session = SimpleNamespace(handshake=lambda: SimpleNamespace())

    def _download(self, name):
        self.resource_calls += 1
        raise RuntimeError("offline fixture")

    def _codes(self, market):
        self.code_calls += 1
        if market == "sz":
            return [SimpleNamespace(market_id=0, code=f"{j:06d}") for j in range(1, 82)]
        return []

    def _quotes(self, codes):
        self.quote_batches.append(list(codes))
        return [SimpleNamespace(full_code=code, last_price=11.0, pre_close_price=10.0) for code in codes]


def test_board_member_quotes_filters_current_universe_and_batches_80(tmp_path):
    _fixture_files(tmp_path, count=1)
    client = FakeClient()
    service = BoardService(client, data_dir=tmp_path, definitions_dir=tmp_path)

    result = service.board_member_quotes("880001")

    assert result.count == 81
    assert len(result.raw_members) == 81
    assert len(result.display_members) == 81
    assert result.excluded_members == ()
    assert [len(batch) for batch in client.quote_batches] == [80, 1]


def test_board_quotes_preserve_definition_order_and_reuse_daily_cache(tmp_path):
    _fixture_files(tmp_path, count=81)
    client = FakeClient()
    service = BoardService(client, data_dir=tmp_path, definitions_dir=tmp_path)

    first = service.board_quotes(category="概念")
    first_code_calls = client.code_calls
    second = service.board_quotes()

    assert first.count == 81
    assert [row.board_code for row in first.rows] == [row.board_code for row in second.rows]
    assert client.code_calls == first_code_calls
    assert len(client.quote_batches) == 4
    assert [len(batch) for batch in client.quote_batches] == [80, 1, 80, 1]


def _mixed_files(tmp_path):
    _fixture_files(tmp_path, count=1)
    (tmp_path / "tdxzs.cfg").write_text(
        "概念甲|880501|4|1|0|概念甲\n"
        "煤炭|881001|12|1|0|X10\n"
        "煤炭开采|881002|12|1|0|X1001\n"
        "动力煤|881003|12|1|1|X100101\n"
        "风格甲|880801|5|1|0|风格甲\n"
        "地区甲|880201|3|1|0|1\n"
        "行业甲|880301|2|1|0|T0101\n"
        "概念乙|880502|4|1|0|概念乙\n",
        encoding="gb18030",
    )
    (tmp_path / "infoharbor_block.dat").write_text(
        "#GN_概念甲,1,880501,,,,\n0#000001\n"
        "#FG_风格甲,1,880801,,,,\n0#000001\n",
        encoding="gb18030",
    )


@pytest.mark.parametrize(("category", "expected"), [
    ("概念", ["880501", "880502"]),
    ("一级行业", ["881001"]),
    ("二级行业", ["881002"]),
    ("三级行业", ["881003"]),
    ("风格", ["880801"]),
    ("地区", ["880201"]),
    ("880行业", ["880301"]),
    ("全部", ["880501", "881001", "881002", "881003", "880801", "880201", "880301", "880502"]),
])
def test_board_quotes_request_only_selected_category(tmp_path, category, expected):
    _mixed_files(tmp_path)
    client = FakeClient()
    helper = HelperApi(client, board_data_dir=str(tmp_path), board_definitions_dir=str(tmp_path))
    result = helper.board_quotes(category=category)
    assert [r.board_code for r in result.rows] == expected
    assert client.quote_batches == [["sh" + code for code in expected]]


def test_default_concepts_and_category_switch_reuse_cache(tmp_path):
    _mixed_files(tmp_path)
    client = FakeClient()
    service = BoardService(client, data_dir=tmp_path, definitions_dir=tmp_path)
    assert [r.board_code for r in service.board_quotes().rows] == ["880501", "880502"]
    calls = (client.resource_calls, client.code_calls)
    assert service.board_quotes(category="二级行业").count == 1
    assert service.board_member_quotes("880801").count == 1
    assert (client.resource_calls, client.code_calls) == calls
    assert service.board_quotes(category="全部").count == 8


@pytest.mark.parametrize("category", ["unknown", "", None, 12, [], {}])
def test_invalid_category_fails_before_preparation(tmp_path, category):
    client = FakeClient()
    service = BoardService(client, data_dir=tmp_path)
    with pytest.raises(ValueError, match="unsupported board category"):
        service.board_quotes(category=category)
    assert client.resource_calls == client.code_calls == 0
    assert client.quote_batches == []


def test_file_only_fallback_distinguishes_concepts_and_styles(tmp_path):
    _mixed_files(tmp_path)
    (tmp_path / "tdxzs.cfg").write_text("", encoding="gb18030")
    client = FakeClient()
    service = BoardService(client, data_dir=tmp_path, definitions_dir=tmp_path)
    concept = service.board_quotes()
    assert [(r.board_code, r.board_name) for r in concept.rows] == [("880501", "概念甲")]
    assert [r.board_code for r in service.board_quotes(category="风格").rows] == ["880801"]
    assert service.board_quotes(category="二级行业").rows == ()
    assert client.quote_batches == [["sh880501"], ["sh880801"]]
