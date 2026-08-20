"""Public package interface for eltdx."""

from importlib.metadata import PackageNotFoundError, version

from .client import Client, TdxClient
from .f10 import F10Client, F10Response, F10ResultSet
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
)
from .serialization import to_json, to_jsonable
from .workday import WorkdayService

__all__ = [
    "AuctionData",
    "DailyPriceLimit",
    "DailyPriceLimitTable",
    "DailyShareCapital",
    "DailyShareCapitalTable",
    "Client",
    "F10Client",
    "F10Response",
    "F10ResultSet",
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
    "WorkdayService",
    "__version__",
    "to_json",
    "to_jsonable",
]
try:
    __version__ = version("eltdx")
except PackageNotFoundError:
    __version__ = "0+unknown"
