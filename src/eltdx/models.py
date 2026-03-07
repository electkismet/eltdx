from __future__ import annotations

from dataclasses import dataclass
from datetime import date, datetime


@dataclass(slots=True)
class CallAuctionItem:
    time: datetime
    price: float
    price_milli: int
    match: int
    unmatched: int
    flag: int
    raw_hex: str | None = None


@dataclass(slots=True)
class CallAuctionResponse:
    count: int
    items: list[CallAuctionItem]
    raw_frame_hex: str | None = None
    raw_payload_hex: str | None = None


@dataclass(slots=True)
class MinuteItem:
    time: datetime
    price: float
    price_milli: int
    volume: int


@dataclass(slots=True)
class MinuteResponse:
    count: int
    trading_date: date
    items: list[MinuteItem]
    raw_frame_hex: str | None = None
    raw_payload_hex: str | None = None


@dataclass(slots=True)
class KlineItem:
    time: datetime
    open_price: float
    open_price_milli: int
    high_price: float
    high_price_milli: int
    low_price: float
    low_price_milli: int
    close_price: float
    close_price_milli: int
    last_close_price: float | None
    last_close_price_milli: int | None
    volume: int
    amount: float
    amount_milli: int
    order_count: int | None = None
    up_count: int | None = None
    down_count: int | None = None


@dataclass(slots=True)
class KlineResponse:
    count: int
    items: list[KlineItem]
    raw_frame_hex: str | None = None
    raw_payload_hex: str | None = None


@dataclass(slots=True)
class GbbqItem:
    code: str
    time: datetime
    category: int
    category_name: str | None
    c1: float
    c2: float
    c3: float
    c4: float


@dataclass(slots=True)
class GbbqResponse:
    count: int
    items: list[GbbqItem]
    raw_frame_hex: str | None = None
    raw_payload_hex: str | None = None


@dataclass(slots=True)
class XdxrItem:
    code: str
    time: datetime
    fenhong: float
    peigujia: float
    songzhuangu: float
    peigu: float



@dataclass(slots=True)
class EquityItem:
    code: str
    time: datetime
    category: int
    category_name: str | None
    float_shares: int
    total_shares: int


@dataclass(slots=True)
class EquityResponse:
    count: int
    items: list[EquityItem]

@dataclass(slots=True)
class FactorItem:
    time: datetime
    last_close_price: float | None
    last_close_price_milli: int | None
    pre_last_close_price: float | None
    pre_last_close_price_milli: int | None
    qfq_factor: float
    hfq_factor: float


@dataclass(slots=True)
class FactorResponse:
    count: int
    items: list[FactorItem]


@dataclass(slots=True)
class TradeItem:
    time: datetime
    price: float
    price_milli: int
    volume: int
    status: int
    side: str | None
    order_count: int | None = None


@dataclass(slots=True)
class TradeResponse:
    count: int
    trading_date: date
    items: list[TradeItem]
    raw_frame_hex: str | None = None
    raw_payload_hex: str | None = None


@dataclass(slots=True)
class SecurityCode:
    exchange: str
    code: str
    name: str
    multiple: int
    decimal: int
    last_price: float
    last_price_raw: int

    @property
    def full_code(self) -> str:
        return f"{self.exchange}{self.code}"


@dataclass(slots=True)
class CodePage:
    exchange: str
    start: int
    count: int
    total: int
    items: list[SecurityCode]

    def __iter__(self):
        return iter(self.items)

    def __len__(self) -> int:
        return len(self.items)

    def __getitem__(self, index):
        return self.items[index]


@dataclass(slots=True)
class QuoteLevel:
    buy: bool
    price: float
    price_milli: int
    number: int


@dataclass(slots=True)
class Quote:
    exchange: str
    code: str
    active1: int
    active2: int
    server_time_raw: int
    server_time: datetime | None
    last_price: float
    last_price_milli: int
    open_price: float
    open_price_milli: int
    high_price: float
    high_price_milli: int
    low_price: float
    low_price_milli: int
    last_close_price: float
    last_close_price_milli: int
    total_hand: int
    intuition: int
    amount: float
    inside_dish: int
    outer_disc: int
    buy_levels: list[QuoteLevel]
    sell_levels: list[QuoteLevel]
    rate: float
    call_auction_amount: float | None = None
    call_auction_rate: float | None = None


Code = SecurityCode
MinuteSeries = MinuteResponse
Trade = TradeItem
TradePage = TradeResponse
Kline = KlineItem
KlinePage = KlineResponse


