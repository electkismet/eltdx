from types import SimpleNamespace

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

    first = service.board_quotes()
    first_code_calls = client.code_calls
    second = service.board_quotes()

    assert first.count == 81
    assert [row.board_code for row in first.rows] == [row.board_code for row in second.rows]
    assert client.code_calls == first_code_calls
    assert len(client.quote_batches) == 4
    assert [len(batch) for batch in client.quote_batches] == [80, 1, 80, 1]
