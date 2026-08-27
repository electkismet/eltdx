"""Corporate action and finance models."""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import date, datetime


@dataclass(frozen=True, slots=True)
class CapitalChangeRecord:
    exchange: str
    market_id: int
    code: str
    reserved_7: int
    date_raw: int
    date: date | None
    category_raw: int
    category_name: str | None
    c1_raw: bytes
    c2_raw: bytes
    c3_raw: bytes
    c4_raw: bytes
    c1_float: float
    c2_float: float
    c3_float: float
    c4_float: float
    c1_value: float
    c2_value: float
    c3_value: float
    c4_value: float
    record_hex: str = ""

    @property
    def full_code(self) -> str:
        return f"{self.exchange}{self.code}"

    @property
    def time(self) -> datetime | None:
        if self.date is None:
            return None
        return datetime(self.date.year, self.date.month, self.date.day, 15, 0)

    @property
    def category(self) -> int:
        return self.category_raw

    @property
    def c1(self) -> float:
        return self.c1_value

    @property
    def c2(self) -> float:
        return self.c2_value

    @property
    def c3(self) -> float:
        return self.c3_value

    @property
    def c4(self) -> float:
        return self.c4_value


@dataclass(frozen=True, slots=True)
class CapitalChangeBlock:
    exchange: str
    market_id: int
    code: str
    block_count: int
    records: tuple[CapitalChangeRecord, ...]
    raw_payload: bytes = b""

    @property
    def full_code(self) -> str:
        return f"{self.exchange}{self.code}"

    @property
    def count(self) -> int:
        return len(self.records)

    @property
    def items(self) -> tuple[CapitalChangeRecord, ...]:
        return self.records


@dataclass(frozen=True, slots=True)
class CapitalChangeBatch:
    blocks: tuple[CapitalChangeBlock, ...]
    raw_payloads: tuple[bytes, ...] = field(default_factory=tuple)

    @property
    def count(self) -> int:
        return len(self.blocks)

    @property
    def items(self) -> tuple[CapitalChangeBlock, ...]:
        return self.blocks

    @property
    def records(self) -> tuple[CapitalChangeRecord, ...]:
        return tuple(record for block in self.blocks for record in block.records)


@dataclass(frozen=True, slots=True)
class XdxrRecord:
    code: str
    date: date | None
    category: int
    category_name: str | None
    fenhong: float
    peigujia: float
    songzhuangu: float
    peigu: float

    @property
    def time(self) -> datetime | None:
        if self.date is None:
            return None
        return datetime(self.date.year, self.date.month, self.date.day, 15, 0)


@dataclass(frozen=True, slots=True)
class EquityRecord:
    code: str
    date: date | None
    category: int
    category_name: str | None
    float_shares: int
    total_shares: int

    @property
    def time(self) -> datetime | None:
        if self.date is None:
            return None
        return datetime(self.date.year, self.date.month, self.date.day, 15, 0)


@dataclass(frozen=True, slots=True)
class EquityResponse:
    count: int
    items: tuple[EquityRecord, ...]


@dataclass(frozen=True, slots=True)
class FactorRecord:
    time: datetime
    last_close_price: float | None
    last_close_price_milli: int | None
    pre_last_close_price: float | None
    pre_last_close_price_milli: int | None
    qfq_factor: float
    hfq_factor: float


@dataclass(frozen=True, slots=True)
class FactorResponse:
    count: int
    items: tuple[FactorRecord, ...]
    anchor_date: date | None = None


@dataclass(frozen=True, slots=True)
class AdjustmentFactor:
    date: date
    qfq_scale: float
    qfq_offset: float
    hfq_scale: float
    hfq_offset: float

    @property
    def time(self) -> datetime:
        return datetime(self.date.year, self.date.month, self.date.day, 15, 0)


@dataclass(frozen=True, slots=True)
class AdjustmentFactorResponse:
    exchange: str
    market_id: int
    code: str
    anchor_date: date | None
    start_date: date | None
    items: tuple[AdjustmentFactor, ...]

    @property
    def full_code(self) -> str:
        return f"{self.exchange}{self.code}"

    @property
    def count(self) -> int:
        return len(self.items)


@dataclass(frozen=True, slots=True)
class AdjustmentFactorBatch:
    responses: tuple[AdjustmentFactorResponse, ...]

    @property
    def count(self) -> int:
        return len(self.responses)

    @property
    def items(self) -> tuple[AdjustmentFactorResponse, ...]:
        return self.responses


@dataclass(frozen=True, slots=True)
class FinanceRecord:
    exchange: str
    market_id: int
    code: str
    finance_info_raw: bytes
    liu_tong_gu_ben_raw_float: float
    province_raw: int
    industry_raw: int
    updated_date_raw: int
    updated_date: date | None
    ipo_date_raw: int
    ipo_date: date | None
    zong_gu_ben_raw_float: float
    guo_jia_gu_raw_float: float
    fa_qi_ren_fa_ren_gu_raw_float: float
    fa_ren_gu_raw_float: float
    b_gu_raw_float: float
    h_gu_raw_float: float
    eps_raw: float
    zong_zi_chan_raw_float: float
    liu_dong_zi_chan_raw_float: float
    gu_ding_zi_chan_raw_float: float
    wu_xing_zi_chan_raw_float: float
    gu_dong_ren_shu_raw_float: float
    liu_dong_fu_zhai_raw_float: float
    chang_qi_fu_zhai_raw_float: float
    zi_ben_gong_ji_jin_raw_float: float
    jing_zi_chan_raw_float: float
    zhu_ying_shou_ru_raw_float: float
    zhu_ying_li_run_raw_float: float
    ying_shou_zhang_kuan_raw_float: float
    ying_ye_li_run_raw_float: float
    tou_zi_shou_yu_raw_float: float
    jing_ying_xian_jin_liu_raw_float: float
    zong_xian_jin_liu_raw_float: float
    cun_huo_raw_float: float
    li_run_zong_he_raw_float: float
    shui_hou_li_run_raw_float: float
    jing_li_run_raw_float: float
    wei_fen_li_run_raw_float: float
    mei_gu_jing_zi_chan_raw_float: float
    bao_liu_2_raw_float: float
    record_hex: str = ""

    @property
    def full_code(self) -> str:
        return f"{self.exchange}{self.code}"

    @property
    def circulating_shares(self) -> float:
        return self.liu_tong_gu_ben_raw_float * 10000.0

    @property
    def total_shares(self) -> float:
        return self.zong_gu_ben_raw_float * 10000.0

    @property
    def total_assets_yuan(self) -> float:
        return self.zong_zi_chan_raw_float * 1000.0

    @property
    def net_profit_yuan(self) -> float:
        return self.jing_li_run_raw_float * 1000.0


@dataclass(frozen=True, slots=True)
class FinanceBatch:
    records: tuple[FinanceRecord, ...]
    raw_payload: bytes = b""

    @property
    def count(self) -> int:
        return len(self.records)
