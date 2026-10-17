"""Public package interface for eltdx."""

from importlib.metadata import PackageNotFoundError, version

from .client import Client, TdxClient
from .f10 import (
    F10Client,
    F10Response,
    F10ResultSet,
    LimitBoardLadder,
    LimitBoardLadderRow,
)
from .helpers import (
    AuctionData,
    DailyPriceLimit,
    DailyPriceLimitTable,
    DailyShareCapital,
    DailyShareCapitalTable,
    HelperApi,
    LimitLadderTable,
    RealtimeRankRow,
    RealtimeRankTable,
    ShortlineIndicator,
    ShortlineIndicatorTable,
    StockProfile,
    StockProfileTable,
    StockTopic,
    StockTopics,
    TopicStock,
    TopicStockTable,
    ThemeStrengthRow,
    ThemeStrengthTable,
    BoardQuoteRow,
    BoardQuoteTable,
    BoardMemberQuoteTable,
    FundFlow,
    MarketStat,
)
from .serialization import to_json, to_jsonable
from .models import BlockMeta, CompanyInfoCategory, Market, TradeBatch
from .workday import WorkdayService
from .ex import ExMarket, ExTdxClient, MacExClient
from .mac import MacClient

__all__ = [
    "TradeBatch",
    "AuctionData",
    "DailyPriceLimit",
    "DailyPriceLimitTable",
    "DailyShareCapital",
    "DailyShareCapitalTable",
    "Client",
    "F10Client",
    "F10Response",
    "F10ResultSet",
    "LimitBoardLadder",
    "LimitBoardLadderRow",
    "HelperApi",
    "LimitLadderTable",
    "RealtimeRankRow",
    "RealtimeRankTable",
    "ShortlineIndicator",
    "ShortlineIndicatorTable",
    "StockProfile",
    "StockProfileTable",
    "StockTopic",
    "StockTopics",
    "TdxClient",
    "TopicStock",
    "TopicStockTable",
    "ThemeStrengthRow",
    "ThemeStrengthTable",
    "BoardQuoteRow",
    "BoardQuoteTable",
    "BoardMemberQuoteTable",
    "BlockMeta",
    "CompanyInfoCategory",
    "ExMarket",
    "ExTdxClient",
    "FundFlow",
    "MacClient",
    "MacExClient",
    "Market",
    "MarketStat",
    "WorkdayService",
    "__version__",
    "to_json",
    "to_jsonable",
]
try:
    __version__ = version("eltdx")
except PackageNotFoundError:
    __version__ = "0+unknown"
