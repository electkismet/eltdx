from enum import Enum


class Exchange(str, Enum):
    SZ = "sz"
    SH = "sh"
    BJ = "bj"


class KlinePeriod(str, Enum):
    MINUTE_1 = "1m"
    MINUTE_5 = "5m"
    MINUTE_15 = "15m"
    MINUTE_30 = "30m"
    MINUTE_60 = "60m"
    DAY = "day"
    WEEK = "week"
    MONTH = "month"
    QUARTER = "quarter"
    YEAR = "year"


class AdjustMode(str, Enum):
    QFQ = "qfq"
    HFQ = "hfq"
