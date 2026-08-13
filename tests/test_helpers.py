from dataclasses import dataclass
from datetime import date

from eltdx import HelperApi, TdxClient
from eltdx.f10 import F10Client, parse_tqlex_response
from eltdx.models import (
    Auction0925Result,
    AuctionSeries,
    FinanceBatch,
    FinanceRecord,
    QuoteSnapshot,
    SecurityCode,
)
from eltdx.protocol.constants import MAX_TRADE_PAGE_SIZE


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
        mode_or_selector_raw=0,
        start_raw=0,
        limit_or_count_raw=0,
        points=(),
    )
    snapshot = Auction0925Result(
        code="sz000001",
        trading_date=date(2026, 5, 20),
        has_auction_0925=True,
        price=11.0,
        price_milli=11000,
        volume=123,
        amount=135300.0,
        status=2,
        side="neutral",
        pages_used=1,
        source_mode="history_ticks_scan",
    )

    @dataclass
    class FakeWorkdays:
        def normalize(self, value=None):
            return date(2026, 5, 20)

        def same_day(self, left, right):
            return self.normalize(left) == self.normalize(right)

    class FakeClient:
        workdays = FakeWorkdays()
        def __init__(self):
            self.auctions = type("Auctions", (), {"series": lambda _, code: series})()
            self.quotes = type(
                "Quotes",
                (),
                {
                    "get_snapshots": lambda _, code: [quote],
                    "get_depth": lambda _, code: type("Page", (), {"records": ()})(),
                },
            )()

    helpers = HelperApi(FakeClient())
    helpers._current_market_date = lambda: date(2026, 5, 20)
    helpers._auction_0925 = lambda code, trading_date, **kwargs: snapshot
    result = helpers.auction_data("000001", "2026-05-20")

    assert result.series is series
    assert result.snapshot_0925 is snapshot
    assert result.pre_close_price == 10.0
    assert result.open_price == 11.0
    assert result.open_volume == 123
    assert result.open_amount == 135300.0
    assert result.open_change_pct == 10.0


def test_auction_data_reuses_current_market_date_for_snapshot() -> None:
    from eltdx.models import TradePage

    calls = []

    class FakeWorkdays:
        @staticmethod
        def normalize(value=None):
            return date(2026, 5, 20)

    class FakeSession:
        @staticmethod
        def handshake():
            calls.append("handshake")
            return type(
                "Handshake",
                (),
                {"server_date_1": date(2026, 5, 20), "server_date_2": date(2026, 5, 20)},
            )()

    class FakeTrades:
        @staticmethod
        def today(code, *, start, count):
            calls.append(("today", code, start, count))
            return TradePage(
                exchange="sz",
                market_id=0,
                code="000001",
                start=start,
                request_count=count,
                ticks=(),
            )

    class FakeClient:
        workdays = FakeWorkdays()
        session = FakeSession()
        trades = FakeTrades()

    result = HelperApi(FakeClient()).auction_data(
        "000001",
        include_series=False,
        include_quote=False,
    )

    assert calls == ["handshake", ("today", "sz000001", 0, MAX_TRADE_PAGE_SIZE)]
    assert result.trading_date == date(2026, 5, 20)
    assert result.snapshot_0925.source_mode == "today_ticks_no_0925"


def test_auction_data_does_not_use_current_quote_for_history_date() -> None:
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
    snapshot = Auction0925Result(
        code="sz000001",
        trading_date=date(2026, 5, 19),
        has_auction_0925=True,
        price=11.0,
        price_milli=11000,
        volume=123,
        amount=135300.0,
        status=2,
        side="neutral",
        pages_used=1,
        source_mode="history_ticks_scan",
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
            self.quotes = type(
                "Quotes",
                (),
                {
                    "get_snapshots": lambda _, code: [quote],
                    "get_depth": lambda _, code: type("Page", (), {"records": ()})(),
                },
            )()

    helpers = HelperApi(FakeClient())
    helpers._current_market_date = lambda: date(2026, 5, 20)
    helpers._auction_0925 = lambda code, trading_date, **kwargs: snapshot
    result = helpers.auction_data("000001", "2026-05-19")

    assert result.series is None
    assert result.pre_close_price is None
    assert result.open_price == 11.0
    assert result.open_amount == 135300.0
    assert result.open_change_pct is None

    helpers = HelperApi(FakeClient())
    helpers._current_market_date = lambda: date(2026, 5, 20)
    helpers._auction_0925 = lambda code, trading_date, **kwargs: snapshot
    with_base = helpers.auction_data("000001", "2026-05-19", pre_close_price=10.0)
    assert with_base.open_change_pct == 10.0


def test_adjusted_kline_helper_forwards_plain_arguments() -> None:
    class FakeBars:
        def __init__(self):
            self.called = None

        def get(self, *args, **kwargs):
            self.called = ("get", args, kwargs)
            return "one-page"

        def all(self, *args, **kwargs):
            self.called = ("all", args, kwargs)
            return "all-pages"

    class FakeClient:
        def __init__(self):
            self.bars = FakeBars()
            self.called = None

    client = FakeClient()
    client.helpers = HelperApi(client)

    assert client.helpers.adjusted_kline("000001", period="week", adjust="hfq", count=20) == "one-page"
    assert client.bars.called == ("get", ("000001",), {"period": "week", "adjust": "hfq", "anchor_date": None, "start": 0, "count": 20, "include_raw": False})
    assert client.helpers.adjusted_kline("000001", period="day", all_pages=True, page_size=100) == "all-pages"
    assert client.bars.called[0] == "all"
    assert client.bars.called[1] == ("000001",)
    assert client.bars.called[2]["period"] == "day"
    assert client.bars.called[2]["page_size"] == 100


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
