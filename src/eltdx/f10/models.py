"""Models for the 7615 TQLEX / F10 HTTP gateway."""

from __future__ import annotations

from dataclasses import dataclass, field
from sys import modules as _MODULES
from typing import Any

from eltdx.protocol.unit import ID_TO_MARKET, normalize_code


@dataclass(frozen=True, slots=True)
class F10Cell:
    """A single value with its raw column name and position."""

    name: str
    value: Any
    index: int


@dataclass(frozen=True, slots=True)
class F10ResultSet:
    """One ResultSet table returned by the TQLEX gateway."""

    key: str | None
    columns: tuple[str, ...]
    rows: tuple[dict[str, Any], ...]
    row_cells: tuple[tuple[F10Cell, ...], ...]
    raw: dict[str, Any] = field(repr=False)

    @property
    def count(self) -> int:
        """Number of rows in this result set."""

        return len(self.rows)

    def first(self) -> dict[str, Any] | None:
        """Return the first row, or None when the table is empty."""

        return self.rows[0] if self.rows else None


@dataclass(frozen=True, slots=True)
class F10Response:
    """Parsed response from a 7615 TQLEX Entry."""

    entry: str
    request_body: Any
    error_code: int | None
    result_sets: tuple[F10ResultSet, ...]
    raw: dict[str, Any] = field(repr=False)

    @property
    def ok(self) -> bool:
        """Whether the gateway reported success."""

        return self.error_code in (None, 0)

    @property
    def tables(self) -> tuple[F10ResultSet, ...]:
        """Alias for result_sets, nicer for product-level examples."""

        return self.result_sets

    @property
    def first_table(self) -> F10ResultSet | None:
        """Return the first table, or None when the response has no table."""

        return self.result_sets[0] if self.result_sets else None

    @property
    def rows(self) -> tuple[dict[str, Any], ...]:
        """Rows from the first table. Empty tuple if no table exists."""

        first = self.first_table
        return first.rows if first is not None else ()

    def first_row(self) -> dict[str, Any] | None:
        """Return the first row in the first table."""

        first = self.first_table
        return first.first() if first is not None else None


@dataclass(frozen=True, slots=True)
class LimitBoardLadderRow:
    """One stock row from ``CWServ.cfg_fx_lbtt``.

    The gateway uses short native names (``ZQDM``, ``lbts`` and so on).
    Descriptive attributes are exposed here while ``raw`` keeps every value
    exactly as returned by the server for fields whose meaning can vary by
    client version.
    """

    trading_date: Any | None
    trading_date_value: Any | None
    board_level: int | None
    code: str | None
    market_id: int | None
    limit_reason: Any | None
    seal_amount: float | int | None
    name: Any | None
    limit_reason_extra: Any | None
    limit_time: Any | None
    broken_count: int | None
    industry: Any | None
    limit_type: int | None
    success_rate: Any | None
    highest_board_level: int | None
    raw: dict[str, Any] = field(repr=False)
    market: str | None = field(init=False)
    full_code: str | None = field(init=False)

    def __post_init__(self) -> None:
        """Derive normalized code fields while keeping raw values intact."""

        from eltdx._native_abi import load_native

        load_native().f10_model_identity(self, "ladder", _MODULES[__name__])

    @property
    def trade_date(self) -> Any | None:
        """Return the canonical date value when the server supplied one."""

        return self.trading_date_value or self.trading_date

    @property
    def board(self) -> int | None:
        """Return this stock's board level, when it is a limit-up row."""

        return self.board_level

    @property
    def consecutive_limit_days(self) -> int | None:
        """Return the current consecutive limit-up day count."""

        return self.board_level

    @property
    def status(self) -> str:
        """Return the normalized row category: ``limit_up``, ``broken`` or ``limit_down``."""

        from eltdx._native_abi import load_native

        return load_native().f10_ladder_status(self)

    @property
    def reason(self) -> Any | None:
        """Return the primary reason shown by the client."""

        return self.limit_reason or self.limit_reason_extra

    # Native aliases are useful when comparing this model with the raw
    # ResultSet returned by ``F10Client.call``.
    @property
    def rq(self) -> Any | None:
        return self.trading_date

    @property
    def rqex(self) -> Any | None:
        return self.trading_date_value

    @property
    def zglb(self) -> int | None:
        return self.highest_board_level

    @property
    def lbts(self) -> int | None:
        return self.board_level

    @property
    def ZQDM(self) -> str | None:  # noqa: N802 - native field name
        return self.code

    @property
    def SC(self) -> int | None:  # noqa: N802 - native field name
        return self.market_id

    @property
    def ztyy(self) -> Any | None:
        return self.limit_reason

    @property
    def fde(self) -> float | int | None:
        return self.seal_amount

    @property
    def ZQJC(self) -> Any | None:  # noqa: N802 - native field name
        return self.name

    @property
    def ztyy2(self) -> Any | None:
        return self.limit_reason_extra

    @property
    def ztsj(self) -> Any | None:
        return self.limit_time

    @property
    def kbcs(self) -> int | None:
        return self.broken_count

    @property
    def sshy(self) -> Any | None:
        return self.industry

    @property
    def ztlb(self) -> int | None:
        return self.limit_type

    @property
    def cgl(self) -> Any | None:
        return self.success_rate


@dataclass(frozen=True, slots=True)
class LimitBoardLadder:
    """Parsed stock-level limit-up ladder query."""

    entry: str
    request_body: dict[str, Any]
    error_code: int | None
    start_date: str
    end_date: str
    rows: tuple[LimitBoardLadderRow, ...]
    summary: tuple[dict[str, Any], ...]
    raw: dict[str, Any] = field(repr=False)

    @property
    def ok(self) -> bool:
        """Whether the gateway reported success."""

        return self.error_code in (None, 0)

    @property
    def count(self) -> int:
        return len(self.rows)

    @property
    def trade_date(self) -> str | None:
        """Return the queried date for a single-day request."""

        return self.start_date if self.start_date == self.end_date else None

    def first_row(self) -> LimitBoardLadderRow | None:
        """Return the first detail row, or ``None`` when no rows exist."""

        return self.rows[0] if self.rows else None

    @property
    def market_overview(self) -> tuple[dict[str, Any], ...]:
        """Return the optional market totals with descriptive field names.

        The gateway's ``N002``..``N014`` fields are the page-level overview:
        advancing, declining, flat, limit-up, limit-down, turnover amounts,
        and the three leading topics.  The original row is retained as
        ``raw`` in each mapped item.
        """

        from eltdx._native_abi import load_native

        return load_native().f10_market_overview(self, _MODULES[__name__])

    @property
    def overview(self) -> tuple[dict[str, Any], ...]:
        """Alias for :attr:`market_overview`."""

        return self.market_overview

    @property
    def ladder_counts(self) -> dict[int, int]:
        """Return limit-up counts grouped by ``lbts`` for a single-day query.

        For a date range use :attr:`ladder_counts_by_date` to avoid merging
        separate trading days.
        """

        from eltdx._native_abi import load_native

        return load_native().f10_ladder_counts(self, False, _MODULES[__name__])

    @property
    def ladder_counts_by_date(self) -> dict[str, dict[int, int]]:
        """Return actual board counts separately for every trading date."""

        from eltdx._native_abi import load_native

        return load_native().f10_ladder_counts(self, True, _MODULES[__name__])

    @property
    def promotion_rates(self) -> dict[int, float]:
        """Return each board's promotion rate against the immediately lower board."""

        from eltdx._native_abi import load_native

        return load_native().f10_ladder_rates(self, False)

    @property
    def promotion_rates_by_date(self) -> dict[str, dict[int, float]]:
        """Return board promotion rates separately for every trading date."""

        from eltdx._native_abi import load_native

        return load_native().f10_ladder_rates(self, True)


def _infer_market(code: Any) -> str | None:
    """按代码号段推断市场，失败时返回 ``None``（不抛异常）。"""

    from eltdx._native_abi import load_native

    return load_native().f10_infer_market(code, _MODULES[__name__])


@dataclass(frozen=True, slots=True)
class IndexMember:
    """指数成份股一行（Entry ``CWServ.skef10_zs_cfgsj_cfgjcsj``）。

    服务端只返回 ``N001`` 代码、``N002`` 名称、``N003`` 纳入日期、``N004`` 市场标记
    （``"1"`` 沪市 / ``"0"`` 深市）。**服务端不返回成份权重**，需要权重必须走
    通达信客户端（见 TQ ``get_index_weight``）。
    """

    code: str | None
    name: str | None
    market_id: int | None
    join_date: Any | None
    raw: dict[str, Any] = field(repr=False)
    market: str | None = field(init=False)
    full_code: str | None = field(init=False)

    def __post_init__(self) -> None:
        from eltdx._native_abi import load_native

        load_native().f10_model_identity(self, "standard", _MODULES[__name__])


@dataclass(frozen=True, slots=True)
class IndexMemberTable:
    """某个指数的成份股清单。``total`` 是服务端报告的总数（分页前）。"""

    index_code: str
    total: int | None
    rows: tuple[IndexMember, ...]

    @property
    def count(self) -> int:
        return len(self.rows)

    @property
    def codes(self) -> tuple[str, ...]:
        from eltdx._native_abi import load_native

        return load_native().f10_model_sequence(self, "code", False, _MODULES[__name__])

    @property
    def full_codes(self) -> tuple[str, ...]:
        from eltdx._native_abi import load_native

        return load_native().f10_model_sequence(self, "full_code", False, _MODULES[__name__])


@dataclass(frozen=True, slots=True)
class IndexTrackingProduct:
    """跟踪某个指数的产品一行（Entry ``CWServ.skef10_zs_jbxx_gzbd``）。

    已确认字段：``N001`` 代码、``N002`` 名称、``N003`` 基金规模（元）、``N005``
    涨跌幅（%）、``N006`` 成交额（元）、``N008`` 成立日期、``N009`` 基金经理、
    ``N010`` 基金公司、``N011`` 单位净值、``N012`` 管理费率（%）。

    ``N004`` 与 ``N007`` 服务端未标注含义，这里**不猜测**，只保留在 ``raw`` 里。
    """

    code: str | None
    name: str | None
    market_id: Any | None
    scale_yuan: float | None
    change_pct: float | None
    amount_yuan: float | None
    inception_date: Any | None
    manager: str | None
    company: str | None
    nav: float | None
    fee_rate_pct: float | None
    raw: dict[str, Any] = field(repr=False)
    market: str | None = field(init=False)
    full_code: str | None = field(init=False)

    def __post_init__(self) -> None:
        from eltdx._native_abi import load_native

        load_native().f10_model_identity(self, "tracking", _MODULES[__name__])


@dataclass(frozen=True, slots=True)
class IndexTrackingTable:
    """跟踪某个指数的全部产品（ETF / LOF / 场外基金等）。"""

    index_code: str
    total: int | None
    rows: tuple[IndexTrackingProduct, ...]

    @property
    def count(self) -> int:
        return len(self.rows)

    @property
    def etfs(self) -> tuple[IndexTrackingProduct, ...]:
        """按代码号段粗筛交易所内 ETF（51/56/58/15/16/159 开头）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_model_sequence(self, "etfs", False, _MODULES[__name__])


@dataclass(frozen=True, slots=True)
class BoardMember:
    """板块成份股的一行**分红记录**（Entry ``CWServ.skef10_bk_getinfo``，section ``board_members``）。

    已确认字段：``N001`` 代码、``N002`` 名称、``sc`` 市场（``1`` 沪市 / ``0`` 深市）、
    ``N003`` 分红方案、``N004`` 进度、``N008`` 报告期。

    **一行 = 一条分红记录，不是一只股票**：同一证券的中报 / 年报分红各占一行
    （实测 ``880472`` 证券板块共 67 行、44 只证券，其中 21 只有两行）。
    行序按公告日期（``N005``）降序；**同日记录的服务端次序不稳定**，跨页拼接时
    请用 :meth:`eltdx.f10.client.F10Client.board_members` 的按记录去重翻页，
    或直接用 ``page_size=100`` 单页取全。

    ``N005`` / ``N006`` / ``N007`` 服务端未标注含义，按 A 股分红实施流程命名为
    ``announce_date`` / ``record_date`` / ``ex_date``，**均为推断**，生产使用前请用
    行内 ``raw`` 复核。
    """

    code: str | None
    name: str | None
    market_id: int | None
    dividend_plan: str | None
    progress: str | None
    announce_date: Any | None
    record_date: Any | None
    ex_date: Any | None
    report_period: Any | None
    raw: dict[str, Any] = field(repr=False)
    market: str | None = field(init=False)
    full_code: str | None = field(init=False)

    def __post_init__(self) -> None:
        from eltdx._native_abi import load_native

        load_native().f10_model_identity(self, "standard", _MODULES[__name__])

    @property
    def record_key(self) -> tuple:
        """记录身份：同一证券的同一条分红记录在跨页抖动里会重复出现，按此去重。"""

        from eltdx._native_abi import load_native

        return load_native().f10_board_record_key(self)


@dataclass(frozen=True, slots=True)
class BoardMemberTable:
    """某个板块的分红记录清单（按记录去重、保留首次出现顺序）。

    ``total`` 是服务端报告的**记录总数**（第二个 ResultSet 的 ``zs`` 列，实测
    ``880472`` 为 67），不是证券只数；``rows`` 一行一条分红记录，同一证券可有多行
    （中报 / 年报）。``count`` 是去重后实际拿到的记录数——服务端同日次序不稳定时
    可能略小于 ``total``（翻页在“整页无新记录”时提前停止）。只要证券清单用
    :attr:`stocks` / :attr:`full_codes`。
    """

    board_code: str
    total: int | None
    rows: tuple[BoardMember, ...]

    @property
    def count(self) -> int:
        return len(self.rows)

    @property
    def stocks(self) -> tuple[str, ...]:
        """去重后的证券代码（首次出现顺序）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_model_sequence(self, "code", True, _MODULES[__name__])

    @property
    def full_codes(self) -> tuple[str, ...]:
        """去重后的完整代码（首次出现顺序）。"""

        from eltdx._native_abi import load_native

        return load_native().f10_model_sequence(self, "full_code", True, _MODULES[__name__])


@dataclass(frozen=True, slots=True)
class IpoEntry:
    """新股一行（Entry ``CWServ.skef10_dpzx_xgfx``，参数 ``002``）。

    已确认字段：``N001`` 日期、``N002`` 代码、``N003`` 名称、``N006`` 发行价（元）、
    ``N008`` 实际募资总额（元）。

    ``N004`` / ``N005`` / ``N007`` 服务端未标注含义。``N004`` 数值区间与新股发行
    市盈率一致，``N007`` 与新股申购页同一数值（疑似网上申购上限，单位万股）——
    两者均为**推断**，使用前请用 ``raw`` 复核。
    """

    apply_date: Any | None
    code: str | None
    name: str | None
    market_id: int | None
    issue_pe: float | None
    extra_pct: float | None
    issue_price: float | None
    apply_limit: float | None
    raise_amount: float | None
    raw: dict[str, Any] = field(repr=False)
    market: str | None = field(init=False)
    full_code: str | None = field(init=False)

    def __post_init__(self) -> None:
        from eltdx._native_abi import load_native

        load_native().f10_model_identity(self, "standard", _MODULES[__name__])

    @property
    def raise_amount_yi(self) -> float | None:
        """募资总额，单位亿元。"""

        from eltdx._native_abi import load_native

        return load_native().f10_raise_amount(self)


@dataclass(frozen=True, slots=True)
class IpoCalendar:
    """近期新股列表。"""

    rows: tuple[IpoEntry, ...]

    @property
    def count(self) -> int:
        return len(self.rows)

    @property
    def full_codes(self) -> tuple[str, ...]:
        from eltdx._native_abi import load_native

        return load_native().f10_model_sequence(self, "full_code", False, _MODULES[__name__])
