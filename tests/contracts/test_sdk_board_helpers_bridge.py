"""Legacy board cache/parse/compose contract sources for final verification."""
from datetime import date
import json
from pathlib import Path
import struct
from types import SimpleNamespace

import pytest

from eltdx.helpers import boards

DAY = date(2024, 6, 3)
CODE = "sh" + str(600_001)
BOARD = {"board_code": "880001", "board_name": "Example", "full_code": "sh880001",
         "category": 4, "market_id": 1, "membership_key": ""}


class Client(SimpleNamespace):
    pass


def test_board_quote_composition_preserves_selection_order_duplicate_last_and_hooks(tmp_path, monkeypatch):
    client = Client()
    service = boards.BoardService(client, data_dir=tmp_path)
    selected = [BOARD, dict(BOARD, board_code="880002", full_code="sh880002")]
    service._prepare = lambda **kwargs: {"date": DAY, "boards": selected}
    seen = []
    monkeypatch.setattr(boards, "_matches_category", lambda item, category: seen.append(item["board_code"]) or True)
    first, last = Client(full_code="sh880001", last_price=10), Client(full_code="sh880001", last_price=11)
    service._snapshot_batches = lambda codes: [first, last]
    result = service.board_quotes()
    assert seen == ["880001", "880002"]
    assert result.rows[0].quote is last and result.rows[1].quote is None
    assert result.prepared_date == DAY and result.count == 2


def test_board_member_composition_keeps_raw_display_excluded_objects(tmp_path):
    client = Client()
    service = boards.BoardService(client, data_dir=tmp_path)
    service._prepare = lambda **kwargs: {"date": DAY, "boards": [BOARD]}
    raw = [{"market": 1, "code": "600001"}, {"market": 1, "code": "600002"}]
    detail = {"raw_members": raw, "display_members": raw[:1], "excluded_members": raw[1:]}
    service._members_for = lambda board: detail
    quote = Client(full_code=CODE)
    service._snapshot_batches = lambda codes: [quote]
    result = service.board_member_quotes(" SH880001 ")
    assert result.raw_members[0] is raw[0] and result.excluded_members[0] is raw[1]
    assert result.rows[0].full_code == CODE and result.rows[0].quote is quote
    with pytest.raises(ValueError, match="unknown board code"):
        service.board_member_quotes("missing")


def test_board_invalid_category_and_code_fail_before_preparation(tmp_path):
    client = Client()
    service = boards.BoardService(client, data_dir=tmp_path)
    service._prepare = lambda **kwargs: pytest.fail("invalid request reached preparation")
    for category in ["", None, [], {}]:
        with pytest.raises(ValueError, match="unsupported board category"):
            service.board_quotes(category=category)
    for code in [None, "", " "]:
        with pytest.raises(ValueError, match="board_code is required"):
            service.board_member_quotes(code)


def test_board_prepare_memory_hit_skips_disk_and_sources(tmp_path):
    client = Client()
    service = boards.BoardService(client, data_dir=tmp_path / "not-created")
    service._market_date = lambda: DAY
    service._prepared_date = DAY
    service._boards = (BOARD,)
    service._security = {}
    result = service._prepare(refresh=False)
    assert result["boards"] is service._boards
    assert not service._data_dir.exists()


def test_board_prepare_disk_hit_reuses_empty_security_and_skips_download(tmp_path):
    (tmp_path / ".eltdx_board_cache.json").write_text(json.dumps({"last_success_date": DAY.isoformat()}))
    client = Client()
    service = boards.BoardService(client, data_dir=tmp_path)
    service._market_date = lambda: DAY
    service._cache_files_ready = lambda: True
    service._parse_boards = lambda: [BOARD]
    service._load_security_file = lambda: {}
    service._download_missing_or_refresh = lambda **kwargs: pytest.fail("disk hit downloaded")
    result = service._prepare(refresh=False)
    assert result["date"] == DAY and service._security == {}


def test_board_prepare_keeps_memory_publication_before_failed_metadata_write(tmp_path, monkeypatch):
    client = Client()
    service = boards.BoardService(client, data_dir=tmp_path)
    security = {(1, "600001"): Client(name="one")}
    service._market_date = lambda: DAY
    service._download_missing_or_refresh = lambda **kwargs: None
    service._parse_boards = lambda: [BOARD]
    service._load_security_from_044d = lambda: security
    service._write_security_file = lambda rows: None
    original = Path.write_text

    def write(path, *args, **kwargs):
        if path.name == ".eltdx_board_cache.json":
            raise OSError("metadata full")
        return original(path, *args, **kwargs)

    monkeypatch.setattr(Path, "write_text", write)
    with pytest.raises(OSError, match="metadata full"):
        service._prepare(refresh=True)
    assert service._prepared_date == DAY and service._boards == (BOARD,)
    assert service._security is security


def test_board_download_only_downgrades_request_failures(tmp_path):
    calls = []

    def download(name):
        calls.append(name)
        if name == "infoharbor_block.dat":
            raise ValueError("optional request")
        return bytearray(b"payload")

    client = Client(resources=Client(download_file=download))
    service = boards.BoardService(client, data_dir=tmp_path)
    service._download_missing_or_refresh(refresh=True)
    assert calls == list(boards.BOARD_FILES)
    assert (tmp_path / "tdxhy.cfg").read_bytes() == b"payload"
    calls.clear()
    service._download_missing_or_refresh(refresh=False)
    assert calls == ["infoharbor_block.dat"]


def test_board_definitions_keep_first_occurrence_and_infoharbor_fallback(tmp_path):
    (tmp_path / "tdxzs.cfg").write_text("First|880001|4|1|0|X\nLater|880001|5|0|0|Y\n", encoding="utf-8")
    (tmp_path / "tdxzs3.cfg").write_text("Third|880001|2|1|0|Z\n", encoding="utf-8")
    (tmp_path / "infoharbor_block.dat").write_text("#GN_ignored,1,880001\n#FG_fallback,0,880002\n", encoding="utf-8")
    client = Client()
    service = boards.BoardService(client, data_dir=tmp_path, definitions_dir=tmp_path)
    rows = service._parse_boards()
    assert [(row["board_code"], row["board_name"], row["category"]) for row in rows] == [
        ("880001", "First", 4), ("880002", "fallback", 5)]


def test_board_members_deduplicate_filter_and_keep_cache_object(tmp_path):
    (tmp_path / "infoharbor_block.dat").write_text("#GN_one,2,880001\n1#600001,1#600001,0#000001\n", encoding="utf-8")
    client = Client()
    service = boards.BoardService(client, data_dir=tmp_path)
    service._security = {(1, "600001"): object()}
    result = service._members_for(BOARD)
    assert result["raw_members"] == [{"market": 1, "code": "600001"}, {"market": 0, "code": "000001"}]
    assert result["display_members"] == [{"market": 1, "code": "600001"}]
    assert result["excluded_members"][0]["reason"] == "absent_from_current_security_list"
    assert service._members_for(BOARD) is result


def test_board_members_preserve_last_header_validation_behavior(tmp_path):
    # Legacy checks the expected count remaining after the final header.
    (tmp_path / "infoharbor_block.dat").write_text("#GN_one,9,880001\n1#600001\n#GN_other,0,880002\n", encoding="utf-8")
    client = Client()
    service = boards.BoardService(client, data_dir=tmp_path)
    assert len(service._members_for(BOARD)["raw_members"]) == 1
    service.clear_cache()
    (tmp_path / "infoharbor_block.dat").write_text("#GN_one,9,880001\n1#600001\n", encoding="utf-8")
    with pytest.raises(ValueError, match="incomplete board membership"):
        service._members_for(BOARD)


def test_board_industry_members_keep_prefix_and_source_order(tmp_path):
    (tmp_path / "tdxhy.cfg").write_text("1|600001|T01|||X1001\n0|000001|T02|||X100101\n", encoding="utf-8")
    client = Client()
    service = boards.BoardService(client, data_dir=tmp_path)
    result = service._members_for(dict(BOARD, category=12, membership_key="X10"))
    assert [row["code"] for row in result["raw_members"]] == ["600001", "000001"]
    assert boards._matches_category(dict(BOARD, category="12", membership_key="X1001"), "二级行业")
    assert not boards._matches_category(dict(BOARD, category=12, membership_key="X1001"), "一级行业")


def test_board_security_load_keeps_per_market_failure_and_original_row(tmp_path):
    row = Client(code="600001", market_id=1)
    calls = []

    def all_codes(market):
        calls.append(market)
        if market == "sz":
            raise ValueError("unavailable")
        return [row] if market == "sh" else []

    client = Client(codes=Client(all=all_codes))
    service = boards.BoardService(client, data_dir=tmp_path)
    assert service._load_security_from_044d() == {(1, "600001"): row}
    assert calls == ["sz", "sh", "bj"]


def test_board_text_and_security_json_keep_read_fallbacks(tmp_path):
    path = tmp_path / "text"
    path.write_bytes("Example".encode("gb18030"))
    assert boards._read_text(path) == "Example"
    assert boards._read_text(tmp_path / "missing") == ""
    path.write_text("[]", encoding="utf-8")
    assert boards._read_json(path) == {}
    client = Client()
    service = boards.BoardService(client, data_dir=tmp_path)
    (tmp_path / "security_list.json").write_text('[{"market":"1","code":600001,"name":"one"}]', encoding="utf-8")
    result = service._load_security_file()
    assert result[(1, "600001")]["name"] == "one"
    (tmp_path / "security_list.json").write_text("invalid", encoding="utf-8")
    assert service._load_security_file() is None


def test_board_dbf_members_parse_region_filter_and_ignore_deleted(tmp_path):
    widths = [("SC", 1), ("GPDM", 6), ("DY", 2)]
    header_size, record_size = 32 + 32 * len(widths) + 1, 10
    data = bytearray(header_size)
    struct.pack_into("<IHH", data, 4, 3, header_size, record_size)
    for index, (name, width) in enumerate(widths):
        start = 32 + index * 32
        data[start:start + len(name)] = name.encode("ascii")
        data[start + 16] = width
    data[-1] = 13
    data.extend(b" 1600001 1" + b"*1600002 1" + b" 0600003 2")
    path = tmp_path / "base.dbf"
    path.write_bytes(data)
    assert boards._read_dbf_members(path, "1") == {(1, "600001")}


def test_board_quote_and_full_code_keep_zero_and_unknown_market_fields():
    quote = Client(last_price=0.0, pre_close_price=0.0, amount=-1, total_hand=3)
    row = boards.BoardService._quote_row(dict(BOARD, full_code="xx600001", market_id=7), quote)
    assert row.change == 0.0 and row.change_pct is None and row.market_id == 7
    assert row.quote is quote
    assert boards.BoardService._full_security_code(1, 1) == "sh000001"


def test_board_snapshot_batches_preserves_overridden_size(tmp_path, monkeypatch):
    seen = []
    client = Client(quotes=Client(get_snapshots=lambda codes: seen.append(list(codes)) or []))
    service = boards.BoardService(client, data_dir=tmp_path)
    monkeypatch.setattr(boards, "BOARD_BATCH_SIZE", 2)
    service._snapshot_batches([CODE] * 5)
    assert [len(batch) for batch in seen] == [2, 2, 1]


def test_board_date_fallback_keeps_exception_boundary(tmp_path, monkeypatch):
    class Clock:
        @staticmethod
        def today():
            return DAY

    # No server value belongs to this custom date class, so use its fallback.
    monkeypatch.setattr(boards, "date", Clock)
    client = Client(session=Client(handshake=lambda: Client(server_date_1=DAY)))
    service = boards.BoardService(client, data_dir=tmp_path)
    assert service._market_date() == DAY

    def interrupt():
        raise KeyboardInterrupt("cancelled")

    client.session.handshake = interrupt
    with pytest.raises(KeyboardInterrupt, match="cancelled"):
        service._market_date()
