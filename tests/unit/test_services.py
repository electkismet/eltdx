from __future__ import annotations

from datetime import datetime
from unittest.mock import Mock

from eltdx import KlineItem, KlinePage
from eltdx.models import CodePage, GbbqItem, GbbqResponse, SecurityCode
from eltdx.protocol.unit import SHANGHAI_TZ
from eltdx.services import CodesService, GbbqService, WorkdayService


def test_codes_service_wraps_client() -> None:
    client = Mock()
    client.get_codes.return_value = CodePage(exchange="sz", start=0, count=1, total=1, items=[1])
    client.get_codes_all.side_effect = lambda exchange: {
        "sh": [
            SecurityCode("sh", "600000", "stock", 1, 2, 10.0, 10000),
            SecurityCode("sh", "900901", "b_stock", 1, 2, 10.0, 10000),
            SecurityCode("sh", "510050", "50ETF", 1, 2, 10.0, 10000),
            SecurityCode("sh", "999998", "index", 1, 2, 10.0, 10000),
        ],
        "sz": [
            SecurityCode("sz", "000001", "stock", 1, 2, 10.0, 10000),
            SecurityCode("sz", "159001", "ETF", 1, 2, 10.0, 10000),
            SecurityCode("sz", "399001", "index", 1, 2, 10.0, 10000),
        ],
        "bj": [SecurityCode("bj", "920001", "stock", 1, 2, 10.0, 10000)],
    }[exchange]

    service = CodesService(client)

    page = service.get_page("sz", limit=1)

    assert page.total == 1
    assert service.refresh() == 8
    assert service.get("600000").full_code == "sh600000"
    assert service.get_name("sz159001") == "ETF"
    assert [item.full_code for item in service.by_exchange("sz")] == ["sz000001", "sz159001", "sz399001"]
    assert [item.full_code for item in service.stocks()] == ["sh600000", "sh900901", "sz000001", "bj920001"]
    assert [item.full_code for item in service.etfs()] == ["sh510050", "sz159001"]
    assert [item.full_code for item in service.indexes()] == ["sh999998", "sz399001"]
    assert service.get_stocks() == ["sh600000", "sh900901", "sz000001", "bj920001"]
    client.get_codes.assert_called_once_with("sz", start=0, limit=1)


def test_gbbq_service_wraps_adjusted_kline_calls() -> None:
    client = Mock()
    client.get_gbbq.return_value = GbbqResponse(
        count=2,
        items=[
            GbbqItem(
                code="sz000001",
                time=datetime(2026, 3, 5, 15, 0, tzinfo=SHANGHAI_TZ),
                category=1,
                category_name="除权除息",
                c1=0.1,
                c2=0.0,
                c3=0.0,
                c4=0.0,
            ),
            GbbqItem(
                code="sz000001",
                time=datetime(2026, 3, 6, 15, 0, tzinfo=SHANGHAI_TZ),
                category=2,
                category_name="送配股上市",
                c1=0.0,
                c2=0.0,
                c3=1_000_000.0,
                c4=2_000_000.0,
            ),
        ],
    )
    client.get_kline_all.return_value = KlinePage(
        count=2,
        items=[
            KlineItem(
                time=datetime(2026, 3, 5, 15, 0, tzinfo=SHANGHAI_TZ),
                open_price=10.0,
                open_price_milli=10000,
                high_price=10.1,
                high_price_milli=10100,
                low_price=9.9,
                low_price_milli=9900,
                close_price=10.0,
                close_price_milli=10000,
                last_close_price=9.8,
                last_close_price_milli=9800,
                volume=1,
                amount=1.0,
                amount_milli=1000,
            ),
            KlineItem(
                time=datetime(2026, 3, 6, 15, 0, tzinfo=SHANGHAI_TZ),
                open_price=10.2,
                open_price_milli=10200,
                high_price=10.3,
                high_price_milli=10300,
                low_price=10.1,
                low_price_milli=10100,
                close_price=10.2,
                close_price_milli=10200,
                last_close_price=10.0,
                last_close_price_milli=10000,
                volume=1,
                amount=1.0,
                amount_milli=1000,
            ),
        ],
    )
    client.get_kline.return_value = client.get_kline_all.return_value

    service = GbbqService(client)

    assert len(service.items("000001")) == 2
    assert len(service.get_xdxr("sz000001")) == 1
    assert service.get_equity_changes("sz000001").count == 1
    assert service.get_equity("sz000001").float_shares == 1_000_000
    assert service.get_turnover("sz000001", 100, unit="hand") == 1.0
    factors = service.get_factors("sz000001")
    adjusted = service.get_adjusted_kline("sz000001", "day", count=10)

    assert factors.count == 2
    assert adjusted.count == 2
    client.get_gbbq.assert_called_once_with("sz000001", include_raw=False)
    client.get_kline_all.assert_called_once_with("sz000001", "day")
    client.get_kline.assert_called_once_with("sz000001", "day", start=0, count=10, include_raw=False)


def test_workday_service_normalizes_dates() -> None:
    service = WorkdayService()

    value = datetime(2026, 3, 7, 9, 30, tzinfo=SHANGHAI_TZ)

    assert service.text(value) == "20260307"
    assert service.normalize(value).isoformat() == "2026-03-07"
    assert service.same_day("2026-03-07", "20260307") is True


def test_workday_service_weekday_fallback_without_client() -> None:
    service = WorkdayService()

    assert service.is_workday("2026-03-06") is True
    assert service.is_workday("2026-03-07") is False
    assert [day.isoformat() for day in service.range("2026-03-06", "2026-03-09")] == [
        "2026-03-06",
        "2026-03-09",
    ]
    assert service.next_workday("2026-03-06").isoformat() == "2026-03-09"
    assert service.previous_workday("2026-03-09").isoformat() == "2026-03-06"


def test_workday_service_builds_calendar_from_index_kline() -> None:
    client = Mock()
    client.get_kline_all.return_value = KlinePage(
        count=3,
        items=[
            KlineItem(
                time=datetime(2026, 3, 5, 15, 0, tzinfo=SHANGHAI_TZ),
                open_price=10.0,
                open_price_milli=10000,
                high_price=10.1,
                high_price_milli=10100,
                low_price=9.9,
                low_price_milli=9900,
                close_price=10.0,
                close_price_milli=10000,
                last_close_price=9.8,
                last_close_price_milli=9800,
                volume=1,
                amount=1.0,
                amount_milli=1000,
            ),
            KlineItem(
                time=datetime(2026, 3, 6, 15, 0, tzinfo=SHANGHAI_TZ),
                open_price=10.0,
                open_price_milli=10000,
                high_price=10.1,
                high_price_milli=10100,
                low_price=9.9,
                low_price_milli=9900,
                close_price=10.0,
                close_price_milli=10000,
                last_close_price=9.8,
                last_close_price_milli=9800,
                volume=1,
                amount=1.0,
                amount_milli=1000,
            ),
            KlineItem(
                time=datetime(2026, 3, 9, 15, 0, tzinfo=SHANGHAI_TZ),
                open_price=10.0,
                open_price_milli=10000,
                high_price=10.1,
                high_price_milli=10100,
                low_price=9.9,
                low_price_milli=9900,
                close_price=10.0,
                close_price_milli=10000,
                last_close_price=9.8,
                last_close_price_milli=9800,
                volume=1,
                amount=1.0,
                amount_milli=1000,
            ),
        ],
    )

    service = WorkdayService(client=client)

    assert service.refresh() == 3
    assert service.is_workday("2026-03-06") is True
    assert service.is_workday("2026-03-07") is False
    assert [day.isoformat() for day in service.range("2026-03-05", "2026-03-09")] == [
        "2026-03-05",
        "2026-03-06",
        "2026-03-09",
    ]
    assert service.next_workday("2026-03-06").isoformat() == "2026-03-09"
    assert service.previous_workday("2026-03-09").isoformat() == "2026-03-06"
    assert service.next_workday("2026-03-06", include_self=True).isoformat() == "2026-03-06"
    assert service.previous_workday("2026-03-06", include_self=True).isoformat() == "2026-03-06"
    assert [day.isoformat() for day in service.iter_days("2026-03-05", "2026-03-09", descending=True)] == [
        "2026-03-09",
        "2026-03-06",
        "2026-03-05",
    ]
    client.get_kline_all.assert_called_once_with("sh000001", "day", kind="index")
