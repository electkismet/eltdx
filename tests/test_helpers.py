from dataclasses import dataclass
from datetime import date

import pytest

from eltdx import HelperApi, TdxClient
from eltdx.f10 import F10Client, parse_tqlex_response
from eltdx.models import (
    AuctionSeries,
    FinanceBatch,
    FinanceRecord,
    QuoteSnapshot,
    SecurityCode,
)


def test_stock_profile_table_combines_quote_security_and_finance() -> None:
    quote = QuoteSnapshot(
        exchange="sz",
        market_id=0,
        code="000001",
        active1=0,
        last_price=12.0,
        pre_close_price=10.0,
        open_price=11.0,
        high_price=12.5,
        low_price=10.8,
        time_raw=0,
        unknown_after_time_raw=0,
        total_hand=5000,
        current_hand=100,
        amount=6_000_000.0,
        amount_raw=0,
        inside_dish=0,
        outer_disc=0,
        unknown_after_outer_raw=0,
        open_amount_raw=0,
        open_amount_yuan=1_000_000.0,
        buy_levels=(),
        sell_levels=(),
        tail_raw=b"",
    )
    security = SecurityCode(
        exchange="sz",
        market_id=0,
        code="000001",
        name="平安银行",
        multiple=1,
        decimal=2,
        previous_close_price=10.0,
        volume_ratio_base=0.0,
        unknown0_raw=b"",
        previous_close_raw=b"",
        unknown3_raw=b"",
        category="a_share",
        category_reason="test",
        board="main",
        board_reason="test",
    )
    finance = _finance_record()

    class FakeTransport:
        def connect(self) -> None:
            pass

        def close(self) -> None:
            pass

        def request(self, command: str) -> str:
            return "pong"

        def execute(self, command: int, payload=None):
            if command == 0x054C:
                return [quote]
            if command == 0x044D:
                return [security] if payload["start"] == 0 else []
            if command == 0x0010:
                return FinanceBatch(records=(finance,))
            raise AssertionError(f"unexpected command: {command:#x}")

    table = TdxClient(transport=FakeTransport()).helpers.stock_profile_table(["000001"])

    assert table.count == 1
    row = table.rows[0]
    assert row.full_code == "sz000001"
    assert row.name == "平安银行"
    assert row.change_pct == 20.0
    assert row.circulating_shares == 100_000_000
    assert row.total_market_value == 2_400_000_000
    assert row.turnover_rate == 0.5


def test_stock_topics_merges_topic_ids_and_hot_topic_details() -> None:
    class FakeF10(F10Client):
        def _post(self, entry, body):
            if body == {"Params": ["rdtcgn", "000034"]}:
                raw = {
                    "ErrorCode": 0,
                    "ResultSets": [{"ColName": ["t001", "t002"], "Content": [["2945", "存储芯片"], ["3001", "AI手机PC"]]}],
                }
            elif body == {"Params": ["000034", "zttzbkz"]}:
                raw = {
                    "ErrorCode": 0,
                    "ResultSets": [
                        {
                            "ColName": ["id", "ztmc", "gld", "rxsj", "ztrq", "ztnr", "arec", "sslb"],
                            "Content": [[2945, "存储芯片", 3, 20260522, 20230519, "涉及存储产品", 4961, 2]],
                        }
                    ],
                }
            else:
                raw = {"ErrorCode": 0, "ResultSets": []}
            return parse_tqlex_response(entry, body, raw)

    client = TdxClient.in_memory()
    client.f10 = FakeF10()

    topics = client.helpers.stock_topics("000034")

    assert topics.count == 2
    assert topics.topics[0].topic_id == "2945"
    assert topics.topics[0].topic_name == "存储芯片"
    assert topics.topics[0].relation_level == 3
    assert topics.topics[0].reason == "涉及存储产品"
    assert topics.topics[1].topic_name == "AI手机PC"


def test_topic_stocks_resolves_topic_name_and_parses_rows() -> None:
    class FakeF10(F10Client):
        def __init__(self) -> None:
            super().__init__()
            self.compare_body = None

        def _post(self, entry, body):
            if body == {"Params": ["rdtcgn", "000034"]}:
                raw = {"ErrorCode": 0, "ResultSets": [{"ColName": ["t001", "t002"], "Content": [["2945", "存储芯片"]]}]}
            elif body == {"Params": ["000034", "zttzbkz"]}:
                raw = {"ErrorCode": 0, "ResultSets": []}
            else:
                self.compare_body = body
                raw = {
                    "ErrorCode": 0,
                    "ResultSets": [
                        {
                            "ColName": ["pm", "zqdm", "zqjc", "sc", "zdf", "zdf_3d", "tjdate"],
                            "Content": [[1, "300975", "商络电子", 0, 19.97, 37.24, 20260528]],
                        }
                    ],
                }
            return parse_tqlex_response(entry, body, raw)

    client = TdxClient.in_memory()
    client.f10 = FakeF10()

    table = client.helpers.topic_stocks("000034", topic_name="存储芯片")

    assert client.f10.compare_body == {"Params": ["gndbzfsj", "000034", "2945", "zdf"]}
    assert table.topic_id == "2945"
    assert table.topic_name == "存储芯片"
    assert table.rows[0].rank == 1
    assert table.rows[0].full_code == "sz300975"
    assert table.rows[0].change_pct == 19.97


def test_auction_data_combines_series_snapshot_and_open_change() -> None:
    quote = QuoteSnapshot(
        exchange="sz",
        market_id=0,
        code="000001",
        active1=0,
        last_price=11.0,
        pre_close_price=10.0,
        open_price=10.5,
        high_price=11.0,
        low_price=10.2,
        time_raw=0,
        unknown_after_time_raw=0,
        total_hand=100,
        current_hand=10,
        amount=1000.0,
        amount_raw=0,
        inside_dish=0,
        outer_disc=0,
        unknown_after_outer_raw=0,
        open_amount_raw=0,
        open_amount_yuan=900.0,
        buy_levels=(),
        sell_levels=(),
        tail_raw=b"",
    )
    series = AuctionSeries(
        exchange="sz",
        market_id=0,
        code="000001",
        trading_date=None,
        mode_or_selector_raw=0,
        start_raw=0,
        limit_or_count_raw=0,
        points=(),
    )
    from eltdx.models import TradeTick
    snapshot = TradeTick(0, 0, 565, "09:25", None, 11.0, 11000, 123, 1, 2, "neutral", 0, 1100, event_kind="opening_match")

    @dataclass
    class FakeWorkdays:
        def normalize(self, value=None):
            return date(2026, 5, 20)

        def same_day(self, left, right):
            return self.normalize(left) == self.normalize(right)

    class FakeClient:
        workdays = FakeWorkdays()
        def __init__(self):
            self.quote_calls = 0
            self.auctions = type("Auctions", (), {"series": lambda _, code, date=None: series})()
            self.trades = type("Trades", (), {"opening_match_today": lambda _, code: snapshot})()
            self.quotes = type("Quotes", (), {"get_snapshots": lambda _, code: self._quotes(code)})()

        def _quotes(self, code):
            self.quote_calls += 1
            return [quote]

    client = FakeClient()
    helpers = HelperApi(client)
    helpers._current_market_date = lambda: date(2026, 5, 20)
    result = helpers.auction_data("000001")

    assert result.series is series
    assert result.snapshot_0925 is snapshot
    assert result.pre_close_price == 10.0
    assert result.open_price == 11.0
    assert result.open_volume == 123
    assert result.open_amount == 123 * 11.0 * 100
    assert result.open_change_pct == 10.0
    assert client.quote_calls == 1


def test_auction_data_reuses_current_market_date_for_snapshot() -> None:
    class FakeWorkdays:
        @staticmethod
        def normalize(value=None): return date(2026, 5, 20)

    class FakeTrades:
        @staticmethod
        def opening_match_today(code): return None

    class FakeClient:
        workdays = FakeWorkdays()
        trades = FakeTrades()
        session = type("Session", (), {"handshake": staticmethod(lambda: type("Handshake", (), {"server_date_1": date(2026, 5, 20), "server_date_2": date(2026, 5, 20)})())})()

    result = HelperApi(FakeClient()).auction_data(
        "000001",
        include_series=False,
        include_quote=False,
    )

    assert result.trading_date == date(2026, 5, 20)
    assert result.snapshot_0925 is None


def test_auction_data_uses_quote_only_for_current_pre_close() -> None:
    class FakeClient:
        workdays = type("Workdays", (), {"normalize": staticmethod(lambda value=None: date(2026, 5, 20))})()
        auctions = type("Auctions", (), {"series": staticmethod(lambda code: None)})()
        trades = type("Trades", (), {"opening_match_today": staticmethod(lambda code: None)})()

        def __init__(self):
            self.quote_calls = 0
            self.quotes = type("Quotes", (), {"get_snapshots": lambda _, code: self._quotes(code)})()

        def _quotes(self, code):
            self.quote_calls += 1
            return [type("Quote", (), {"pre_close_price": 10.0, "open_price": 11.0, "open_amount_yuan": 123.0})()]

    client = FakeClient()
    helpers = HelperApi(client)
    helpers._current_market_date = lambda: date(2026, 5, 20)
    result = helpers.auction_data("000001", include_series=False)

    assert client.quote_calls == 1
    assert result.pre_close_price == 10.0
    assert result.open_price is None
    assert result.open_amount is None

    result = helpers.auction_data(
        "000001",
        include_series=False,
        include_snapshot=False,
        pre_close_price=9.9,
    )
    assert client.quote_calls == 1
    assert result.pre_close_price == 9.9


def test_auction_data_skips_current_snapshot_when_disabled() -> None:
    class FakeWorkdays:
        @staticmethod
        def normalize(value=None):
            return date(2026, 5, 20)

    class FakeTrades:
        @staticmethod
        def opening_match_today(code):
            raise AssertionError("opening_match_today must not be called")

    class FakeClient:
        workdays = FakeWorkdays()
        trades = FakeTrades()

    helpers = HelperApi(FakeClient())
    helpers._current_market_date = lambda: date(2026, 5, 20)

    result = helpers.auction_data(
        "000001",
        include_series=False,
        include_snapshot=False,
        include_quote=False,
    )

    assert result.snapshot_0925 is None


def test_auction_data_does_not_use_current_quote_for_history_date() -> None:
    from eltdx.models import TradePage, TradeTick
    auction = TradeTick(
        0,
        0,
        564,
        "09:24",
        None,
        10.9,
        10900,
        100,
        -20,
        8,
        "unknown_8",
        0,
        1090,
        event_kind="auction_snapshot",
    )
    snapshot = TradeTick(0, 0, 565, "09:25", None, 11.0, 11000, 123, 1, 2, "neutral", 0, 1100, event_kind="opening_match")
    page = TradePage(
        exchange="sz",
        market_id=0,
        code="000001",
        start=0,
        request_count=2,
        ticks=(auction, snapshot),
        trading_date=date(2026, 5, 19),
        price_base_raw_f32=10.0,
    )

    @dataclass
    class FakeWorkdays:
        def normalize(self, value=None):
            if value is None:
                return date(2026, 5, 20)
            return date(2026, 5, 19)

        def same_day(self, left, right):
            return self.normalize(left) == self.normalize(right)

    class FakeClient:
        workdays = FakeWorkdays()
        def __init__(self):
            self.history_calls = 0
            self.series_calls = []
            self.auctions = type("Auctions", (), {"series": lambda _, code, trading_date=None: self._series(code, trading_date)})()
            self.trades = type("Trades", (), {"all_history": lambda _, code, trading_date: self._history(code, trading_date)})()

        def _series(self, code, trading_date):
            self.series_calls.append((code, trading_date))
            return "historical-series"

        def _history(self, code, trading_date):
            self.history_calls += 1
            return page

    client = FakeClient()
    helpers = HelperApi(client)
    result = helpers.auction_data("000001", "2026-05-19")

    assert result.series == "historical-series"
    assert result.snapshot_0925 is snapshot
    assert result.pre_close_price == 10.0
    assert result.open_price == 11.0
    assert result.open_amount == 135300.0
    assert result.open_change_pct == 10.0
    assert client.history_calls == 1
    assert client.series_calls == [("sz000001", date(2026, 5, 19))]

    helpers = HelperApi(FakeClient())
    with_base = helpers.auction_data("000001", "2026-05-19", pre_close_price=10.0)
    assert with_base.open_change_pct == 10.0

    helpers = HelperApi(FakeClient())
    without_base = helpers.auction_data("000001", "2026-05-19", include_quote=False)
    assert without_base.pre_close_price is None
    assert without_base.open_change_pct is None


def _finance_record() -> FinanceRecord:
    return FinanceRecord(
        exchange="sz",
        market_id=0,
        code="000001",
        finance_info_raw=b"",
        liu_tong_gu_ben_raw_float=10000.0,
        province_raw=0,
        industry_raw=0,
        updated_date_raw=20260520,
        updated_date=date(2026, 5, 20),
        ipo_date_raw=19910403,
        ipo_date=date(1991, 4, 3),
        zong_gu_ben_raw_float=20000.0,
        guo_jia_gu_raw_float=0.0,
        fa_qi_ren_fa_ren_gu_raw_float=0.0,
        fa_ren_gu_raw_float=0.0,
        b_gu_raw_float=0.0,
        h_gu_raw_float=0.0,
        eps_raw=1.23,
        zong_zi_chan_raw_float=0.0,
        liu_dong_zi_chan_raw_float=0.0,
        gu_ding_zi_chan_raw_float=0.0,
        wu_xing_zi_chan_raw_float=0.0,
        gu_dong_ren_shu_raw_float=0.0,
        liu_dong_fu_zhai_raw_float=0.0,
        chang_qi_fu_zhai_raw_float=0.0,
        zi_ben_gong_ji_jin_raw_float=0.0,
        jing_zi_chan_raw_float=0.0,
        zhu_ying_shou_ru_raw_float=0.0,
        zhu_ying_li_run_raw_float=0.0,
        ying_shou_zhang_kuan_raw_float=0.0,
        ying_ye_li_run_raw_float=0.0,
        tou_zi_shou_yu_raw_float=0.0,
        jing_ying_xian_jin_liu_raw_float=0.0,
        zong_xian_jin_liu_raw_float=0.0,
        cun_huo_raw_float=0.0,
        li_run_zong_he_raw_float=0.0,
        shui_hou_li_run_raw_float=0.0,
        jing_li_run_raw_float=0.0,
        wei_fen_li_run_raw_float=0.0,
        mei_gu_jing_zi_chan_raw_float=0.0,
        bao_liu_2_raw_float=0.0,
    )
