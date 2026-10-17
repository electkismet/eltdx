"""离线数据模型（eltdx.offline 专用，避免依赖网络模型）。

这些模型只描述本地通达信数据文件的结构，字段均为不可变（``frozen`` +
``slots``），与 ``eltdx.models`` 中的网络模型相互独立。
"""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass(frozen=True, slots=True)
class DailyBar:
    """单根日线 K 线（``.day`` 文件）。"""

    year: int
    month: int
    day: int
    open: float
    high: float
    low: float
    close: float
    volume: float
    amount: float
    adjust_flag: int = 0
    raw: bytes = field(default=b"", repr=False, compare=False)

    @property
    def date_int(self) -> int:
        """YYYYMMDD 形式的整数日期。"""

        return self.year * 10000 + self.month * 100 + self.day


@dataclass(frozen=True, slots=True)
class MinBar:
    """单根分钟 K 线（``.lc1`` / ``.lc5`` / ``.5`` 文件）。"""

    year: int
    month: int
    day: int
    hour: int
    minute: int
    open: float
    high: float
    low: float
    close: float
    volume: float
    amount: float
    reserved: int = 0
    raw: bytes = field(default=b"", repr=False, compare=False)

    @property
    def datetime_key(self) -> tuple[int, int, int, int, int]:
        """用于去重/排序的 (年, 月, 日, 时, 分) 元组。"""

        return (self.year, self.month, self.day, self.hour, self.minute)


@dataclass(frozen=True, slots=True)
class ExDailyBar:
    """扩展市场日线（期货/港股等 ``ds/`` 目录下的 ``.day`` 文件）。

    ``amount`` 与 ``hk_stock_amount`` 是**同一个 4 字节槽位**的两种解释：

    * 期货 / 外盘：该槽位是整数成交额 → 用 :attr:`amount`；
    * 港股：该槽位是 ``float32`` 成交额 → 用 :attr:`hk_stock_amount`。

    ``amount`` 同时是写回（:func:`encode_ex_daily_bar`）时使用的那一个，
    因此「读出来再写回去」不会丢字节。港股文件若按整数读该槽位会得到
    IEEE-754 位模式（``1234567.5`` → ``1234613308``），那是 :attr:`amount`，
    不是错误——正确值在 :attr:`hk_stock_amount`。
    """

    year: int
    month: int
    day: int
    open: float
    high: float
    low: float
    close: float
    amount: int
    volume: int
    settlement: float
    hk_stock_amount: float = 0.0
    raw: bytes = field(default=b"", repr=False, compare=False)

    @property
    def date_int(self) -> int:
        """YYYYMMDD 形式的整数日期。"""

        return self.year * 10000 + self.month * 100 + self.day


@dataclass(frozen=True, slots=True)
class BlockInfo:
    """板块文件（``.dat``）中的单个板块。"""

    name: str
    category: int
    count: int
    codes: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class CustomerBlock:
    """通达信自定义板块（``blocknew.cfg`` + ``*.blk``）。"""

    blockname: str
    block_type: str
    codes: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class FinancialFileInfo:
    """财报 zip 文件索引条目（来自 ``tdxfin/gpcw.txt``）。"""

    filename: str
    hash: str
    filesize: int


@dataclass(frozen=True, slots=True)
class FinancialRecord:
    """单只股票的一期历史专业财报记录。"""

    code: str
    market_id: int
    report_date: int
    fields: tuple[float, ...]
