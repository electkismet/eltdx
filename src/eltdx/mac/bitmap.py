"""MAC 协议字段位图编解码。

提供 :class:`FieldBit` 字段位定义、预定义字段集合 :class:`PresetField`、
字段选择器 :class:`FieldSelection`，以及 20 字节请求位图的构建与响应位图解析。

本模块导出 :class:`FieldBit`（另一任务 ``eltdx/ex/mac_client.py`` 依赖的
稳定公开接口）。
"""

from __future__ import annotations

from collections.abc import Iterable, Iterator
from enum import Enum, IntEnum
from typing import TypeAlias

# ── 统一的字段选择类型 ──
Fields: TypeAlias = "FieldBit | PresetField | FieldSelection | Iterable[FieldBit]"


class FieldBit(IntEnum):
    """字段位定义，自带格式（``fmt``）和描述（``desc``），单一数据源。"""

    fmt: str  # 由 __new__ 设置
    desc: str  # 由 __new__ 设置

    def __new__(cls, value: int, fmt: str = "<f", desc: str = "") -> "FieldBit":
        obj = int.__new__(cls, value)
        obj._value_ = value
        obj.fmt = fmt
        obj.desc = desc
        return obj

    @property
    def field_name(self) -> str:
        """返回英文字段名（小写），用于 dict 列名等。"""

        return self.name.lower()

    # ── 基础字段 (0x00-0x07) ──
    PRE_CLOSE = 0x00, "<f", "昨收"
    OPEN = 0x01, "<f", "开盘价"
    HIGH = 0x02, "<f", "最高价"
    LOW = 0x03, "<f", "最低价"
    CLOSE = 0x04, "<f", "收盘价"
    VOL = 0x05, "<I", "成交量"
    VOL_RATIO = 0x06, "<f", "量比"
    AMOUNT = 0x07, "<f", "总金额(元)"

    # ── 扩展字段 (0x08-0x0F) ──
    INSIDE_VOLUME = 0x08, "<I", "内盘"
    OUTSIDE_VOLUME = 0x09, "<I", "外盘"
    TOTAL_SHARES = 0x0A, "<f", "总股数(单位万)"
    FLOAT_SHARES = 0x0B, "<f", "流通股(单位万)"
    EPS = 0x0C, "<f", "每股收益"
    NET_ASSETS = 0x0D, "<f", "净资产"
    SECURITY_TYPE_PRICE = 0x0E, "<f", "证券类型价"
    TOTAL_MARKET_CAP_AB = 0x0F, "<f", "AB股总市值"

    # ── 0x10-0x1F ──
    PE_DYNAMIC = 0x10, "<f", "市盈率(动)"
    BID_PRICE = 0x11, "<f", "买一价"
    ASK_PRICE = 0x12, "<f", "卖一价"
    SERVER_UPDATE_DATE = 0x13, "<I", "服务器更新日期 YYYYMMDD"
    SERVER_UPDATE_TIME = 0x14, "<I", "服务器更新时间 HHMMSS"
    LOT_SIZE_INFO = 0x15, "<I", "未确定"
    BOARD_STRENGTH = 0x16, "<f", "板块强度(涨跌家数差)"
    DIVIDEND_YIELD = 0x17, "<f", "每股股息(元)"
    BID_VOLUME = 0x18, "<I", "买量"
    ASK_VOLUME = 0x19, "<I", "卖量"
    LAST_VOLUME = 0x1A, "<I", "现量"
    TURNOVER = 0x1B, "<f", "换手"
    INDUSTRY = 0x1C, "<I", "行业分类代码"
    INDUSTRY_CHANGE_UP = 0x1D, "<f", "行业涨跌幅"
    STOCK_TAG_FLAGS = 0x1E, "<I", "股票标签位图"
    DECIMAL_POINT = 0x1F, "<I", "数据精度"

    # ── 0x20-0x2F ──
    BUY_PRICE_LIMIT = 0x20, "<f", "涨停价"
    SELL_PRICE_LIMIT = 0x21, "<f", "跌停价"
    PRICE_DECIMAL_INFO = 0x22, "<I", "价格精度标志"
    LOT_SIZE = 0x23, "<I", "所属地区板块/每手股数"
    PRE_IOPV = 0x24, "<f", "昨IOPV"
    SPEED_PCT = 0x25, "<f", "涨速"
    AVG_PRICE = 0x26, "<f", "均价"
    IOPV = 0x27, "<f", "IOPV"
    PE_TTM_VOL_RELATED = 0x28, "<f", "前参考价(美股适用)"
    EX_PRICE_PLACEHOLDER = 0x29, "<f", "前金额参考"
    OPERATING_REVENUE = 0x2A, "<f", "营业收入(万)"
    FLAG_KCB = 0x2B, "<I", "科创板标志"
    FLAG_BJ = 0x2C, "<I", "北交所标志"
    CIRCULATING_CAPITAL_Z = 0x2D, "<f", "流通股本Z（单位：万股）"
    AFTER_HOURS_VOLUME = 0x2E, "<i", "盘后量"

    # ── 0x30-0x3F ──
    PE_TTM = 0x30, "<f", "市盈率TTM"
    PE_STATIC = 0x31, "<f", "市盈率静"
    INDEX_METRIC = 0x37, "<f", "指数指标"
    MAIN_NET_AMOUNT = 0x38, "<f", "今日主力净流入"
    BID_ASK_RATIO = 0x39, "<f", "委比"
    NON_INDEX_FLAG = 0x3A, "<I", "非指数标志"
    CHANGE_20D_PCT = 0x3B, "<f", "20日涨幅%"
    YTD_PCT = 0x3C, "<f", "年初至今%"
    STOCK_CLASS_CODE = 0x3E, "<I", "证券子分类码"
    PERCENT_BASE = 0x3F, "<I", "百分比基底"

    # ── 0x40-0x4F ──
    MTD_PCT = 0x40, "<f", "月初至今%"
    CHANGE_1Y_PCT = 0x41, "<f", "一年涨幅%"
    PREV_CHANGE_PCT = 0x42, "<f", "昨涨幅%"
    CHANGE_3D_PCT = 0x43, "<f", "3日涨幅%"
    CHANGE_60D_PCT = 0x44, "<f", "60日涨幅%"
    CHANGE_5D_PCT = 0x45, "<f", "5日涨幅%"
    CHANGE_10D_PCT = 0x46, "<f", "10日涨幅%"
    PREV2_CHANGE_PCT = 0x47, "<f", "前日涨幅%"
    BID2_PRICE = 0x48, "<f", "买二价"
    ASK2_PRICE = 0x49, "<f", "卖二价"
    AH_CODE = 0x4A, "<I", "对应A/H股code"
    UNKNOWN_CODE = 0x4B, "<I", "少部分有数据"

    # ── 0x50-0x6F ──
    OPEN_AMOUNT = 0x57, "<f", "开盘金额(元)"
    ANNUAL_LIMIT_UP_DAYS = 0x58, "<i", "年涨停天数"
    ACTIVITY = 0x59, "<I", "活跃度"
    DIVIDEND_YIELD_RATE = 0x5B, "<f", "股息率%"
    CONSECUTIVE_UP_DAYS = 0x5C, "<i", "连涨天"
    #: 0x5D / 0x5E / 0x88 / 0x8B 这 4 个位是**双语义**：个股盘口量 vs 板块统计。
    #: 个股语义取 canonical name —— 现有命令（0x122B 个股报价、0x122C 板块成分）
    #: 返回的都是**个股**，用它才不会让 ``quote.fields["bid2_volume"]`` 变成
    #: ``limit_up_count``。查询板块代码时这 4 个键分别表示
    #: 涨停数 / 跌停数 / 上涨家数 / 下跌家数，见对应的板块别名。
    #:
    #: !!! note "为什么 canonical 必须是个股语义"
    #:     ``IntEnum`` 只管一个 canonical 名，``field_name`` 又直接取 ``self.name``，
    #:     所以定义在前的那个名字决定了 dict 的键。早期把板块名写在前面，
    #:     导致个股行情返回 ``limit_up_count`` 这类键 —— 语义错误。
    BID2_VOLUME = 0x5D, "<I", "买二量(个股) / 涨停数(板块)"
    LIMIT_UP_COUNT = 0x5D, "<I", "涨停数(板块)"
    ASK2_VOLUME = 0x5E, "<I", "卖二量(个股) / 跌停数(板块)"
    LIMIT_DOWN_COUNT = 0x5E, "<I", "跌停数(板块)"
    INDUSTRY_SUB = 0x5F, "<I", "行业二级分类"
    AUCTION_BUY_LIMIT = 0x66, "<f", "连续竞价买入上限"
    AUCTION_SELL_LIMIT = 0x67, "<f", "连续竞价卖出下限"
    VOL_SPEED_PCT = 0x68, "<f", "量涨速%"
    SHORT_TURNOVER_PCT = 0x69, "<f", "短换手%"
    AMOUNT_2M = 0x6A, "<f", "2分钟金额(元)"
    MAIN_NET_AMOUNT_COPY = 0x6B, "<f", "今日主力净流入(副本)"
    MAIN_NET_RATIO = 0x6C, "<f", "主力净比%"
    RETAIL_NET_AMOUNT = 0x6D, "<f", "散户单增比"
    MAIN_NET_5M_AMOUNT = 0x6E, "<f", "5分钟主力净额"
    MAIN_NET_3D_AMOUNT = 0x6F, "<f", "近三日主力净额"

    # ── 0x70-0x7F ──
    MAIN_NET_5D_AMOUNT = 0x70, "<f", "近五日主力净额"
    MAIN_NET_10D_AMOUNT = 0x71, "<f", "近十日主买金额(待确定)"
    MAIN_BUY_NET_AMOUNT = 0x72, "<f", "今日主买净额"
    DDX = 0x73, "<f", "DDX"
    DDY = 0x74, "<f", "DDY"
    DDZ = 0x75, "<f", "DDZ"
    DDF = 0x76, "<f", "DDF"
    STOCK_FLAG_A = 0x77, "<f", "个股标志位A"
    STOCK_FLAG_B = 0x78, "<f", "个股标志位B(副本)"
    AUCTION_VOL_RATIO = 0x7A, "<f", "竞价昨比"
    PREV_AMOUNT = 0x7B, "<f", "昨成交额(元)"
    RECENT_INDICATOR = 0x7D, "<f", "近日指标提示"

    # ── 0x80-0x8F ──
    BID3_PRICE = 0x80, "<f", "买三价"
    BID4_PRICE = 0x81, "<f", "买四价"
    BID5_PRICE = 0x82, "<f", "买五价"
    ASK3_PRICE = 0x83, "<f", "卖三价"
    ASK4_PRICE = 0x84, "<f", "卖四价"
    ASK5_PRICE = 0x85, "<f", "卖五价"
    BID3_VOLUME = 0x86, "<I", "买三量"
    BID4_VOLUME = 0x87, "<I", "买四量"
    BID5_VOLUME = 0x88, "<I", "买五量(个股) / 上涨家数(板块)"
    UP_COUNT = 0x88, "<I", "上涨家数(板块)"
    ASK3_VOLUME = 0x89, "<I", "卖三量"
    ASK4_VOLUME = 0x8A, "<I", "卖四量"
    ASK5_VOLUME = 0x8B, "<I", "卖五量(个股) / 下跌家数(板块)"
    DOWN_COUNT = 0x8B, "<I", "下跌家数(板块)"
    BID_ASK_DIFF = 0x8C, "<i", "委差"
    CHANGE_UP_TYPE = 0x8D, "<i", "封板状态"
    SAFETY_SCORE = 0x8E, "<f", "安全分"
    HIGHLIGHT_COUNT = 0x8F, "<f", "亮点数"

    # ── 0x90-0x96: 日内时间涨幅(从昨收算) ──
    CHANGE_AT_1000 = 0x90, "<f", "日内涨幅% 10:00"
    CHANGE_AT_1030 = 0x91, "<f", "日内涨幅% 10:30"
    CHANGE_AT_1100 = 0x92, "<f", "日内涨幅% 11:00"
    CHANGE_AT_1130 = 0x93, "<f", "日内涨幅% 11:30"
    CHANGE_AT_1330 = 0x94, "<f", "日内涨幅% 13:30"
    CHANGE_AT_1400 = 0x95, "<f", "日内涨幅% 14:00"
    CHANGE_AT_1430 = 0x96, "<f", "日内涨幅% 14:30"


# 从 FieldBit 自动生成：位值 -> (字段名, 格式, 描述)
FIELD_BITMAP_MAP: dict[int, tuple[str, str, str]] = {
    bit.value: (bit.name.lower(), bit.fmt, bit.desc) for bit in FieldBit
}


# ── 字段后处理钩子 ──
def _post_ah_code(value: int, market: int = 0) -> str:
    """A/H 股代码补齐位数（沪深北 5 位，其他 6 位）。"""

    if not value:
        return ""
    width = 5 if market in (0, 1) else 6
    return str(value).zfill(width)


FIELD_POSTPROCESS: dict[int, object] = {
    0x4A: _post_ah_code,  # AH_CODE: 补齐 0
}


# ── 控制区(位128-159, 4字节) ──
CTRL_BYTE = 0  # 控制字节起始位(152)
CTRL_EXTENDED = 1  # 非0=扩展模式(含北交所等)，0=标准模式(仅A股)

#: 字段位图字节数。实测（2026-10-16）协议字段位为 0..150，落在 20 字节内。
BITMAP_SIZE = 20
#: 响应位图固定前缀长度（= :data:`BITMAP_SIZE`），其后是总数与行数。
RESP_BITMAP_SIZE = 20
#: 控制区字节数，仅在位图宽度为 :data:`BITMAP_SIZE` 时附加在位图之后。
CONTROL_SIZE = 4


class PresetField(Enum):
    """预定义字段集合，支持 ``+`` / ``|`` 链式组合。"""

    NONE = ()
    OHLC = (FieldBit.OPEN, FieldBit.HIGH, FieldBit.LOW, FieldBit.CLOSE)
    BASIC = (
        FieldBit.OPEN,
        FieldBit.HIGH,
        FieldBit.LOW,
        FieldBit.CLOSE,
        FieldBit.PRE_CLOSE,
        FieldBit.VOL,
    )
    QUOTE = (
        FieldBit.BID_PRICE,
        FieldBit.ASK_PRICE,
        FieldBit.BID_VOLUME,
        FieldBit.ASK_VOLUME,
        FieldBit.LAST_VOLUME,
    )
    VOLUME = (FieldBit.VOL, FieldBit.AMOUNT, FieldBit.TURNOVER, FieldBit.VOL_RATIO)
    FUNDAMENTAL = (
        FieldBit.TOTAL_SHARES,
        FieldBit.FLOAT_SHARES,
        FieldBit.EPS,
        FieldBit.NET_ASSETS,
    )
    ENHANCED = (
        FieldBit.OPEN,
        FieldBit.HIGH,
        FieldBit.LOW,
        FieldBit.CLOSE,
        FieldBit.VOL,
        FieldBit.FLOAT_SHARES,
        FieldBit.ACTIVITY,
    )
    AH_CODE_FIELDS = (
        FieldBit.OPEN,
        FieldBit.HIGH,
        FieldBit.LOW,
        FieldBit.CLOSE,
        FieldBit.VOL,
        FieldBit.AH_CODE,
        FieldBit.LOT_SIZE,
        FieldBit.INDUSTRY,
    )
    BOARD_STATS = (
        FieldBit.LIMIT_UP_COUNT,
        FieldBit.LIMIT_DOWN_COUNT,
        FieldBit.UP_COUNT,
        FieldBit.DOWN_COUNT,
    )
    HANDICAP = (
        FieldBit.BID_PRICE,
        FieldBit.BID2_PRICE,
        FieldBit.BID3_PRICE,
        FieldBit.BID4_PRICE,
        FieldBit.BID5_PRICE,
        FieldBit.ASK_PRICE,
        FieldBit.ASK2_PRICE,
        FieldBit.ASK3_PRICE,
        FieldBit.ASK4_PRICE,
        FieldBit.ASK5_PRICE,
        FieldBit.BID_VOLUME,
        FieldBit.BID2_VOLUME,
        FieldBit.BID3_VOLUME,
        FieldBit.BID4_VOLUME,
        FieldBit.BID5_VOLUME,
        FieldBit.ASK_VOLUME,
        FieldBit.ASK2_VOLUME,
        FieldBit.ASK3_VOLUME,
        FieldBit.ASK4_VOLUME,
        FieldBit.ASK5_VOLUME,
    )
    COMMON = (
        FieldBit.PRE_CLOSE,
        FieldBit.OPEN,
        FieldBit.HIGH,
        FieldBit.LOW,
        FieldBit.CLOSE,
        FieldBit.VOL,
        FieldBit.VOL_RATIO,
        FieldBit.AMOUNT,
        FieldBit.TOTAL_SHARES,
        FieldBit.FLOAT_SHARES,
        FieldBit.EPS,
        FieldBit.NET_ASSETS,
        FieldBit.SECURITY_TYPE_PRICE,
        FieldBit.TOTAL_MARKET_CAP_AB,
        FieldBit.PE_DYNAMIC,
        FieldBit.LOT_SIZE_INFO,
        FieldBit.DIVIDEND_YIELD,
        FieldBit.LAST_VOLUME,
        FieldBit.TURNOVER,
        FieldBit.STOCK_TAG_FLAGS,
        FieldBit.DECIMAL_POINT,
        FieldBit.BUY_PRICE_LIMIT,
        FieldBit.SELL_PRICE_LIMIT,
        FieldBit.PRICE_DECIMAL_INFO,
        FieldBit.LOT_SIZE,
        FieldBit.PRE_IOPV,
        FieldBit.SPEED_PCT,
        FieldBit.FLAG_KCB,
        FieldBit.PE_TTM,
        FieldBit.PE_STATIC,
        FieldBit.MAIN_NET_AMOUNT,
        FieldBit.VOL_SPEED_PCT,
        FieldBit.SHORT_TURNOVER_PCT,
        FieldBit.CIRCULATING_CAPITAL_Z,
    )
    DEBUG = (-1, "", "调试用全字段")
    ALL = tuple(FieldBit)

    def __add__(self, other: object) -> "FieldSelection":
        if isinstance(other, FieldBit | PresetField | FieldSelection):
            return FieldSelection(self, other)
        return NotImplemented

    def __or__(self, other: object) -> "FieldSelection":
        return self.__add__(other)

    def __radd__(self, other: object) -> "FieldSelection":
        if isinstance(other, FieldBit | FieldSelection):
            return FieldSelection(other, self)
        return NotImplemented

    def __ror__(self, other: object) -> "FieldSelection":
        return self.__radd__(other)


class FieldSelection:
    """字段选择器，支持 ``PresetField`` + ``FieldBit`` 组合（保序去重）。"""

    __slots__ = ("_fields",)

    def __init__(self, *parts: "FieldBit | PresetField | FieldSelection") -> None:
        seen: set[FieldBit] = set()
        result: list[FieldBit] = []
        for part in parts:
            if isinstance(part, PresetField):
                source: Iterable[FieldBit] = part.value
            elif isinstance(part, FieldBit):
                source = (part,)
            else:
                source = part._fields
            for bit in source:
                if bit not in seen:
                    seen.add(bit)
                    result.append(bit)
        self._fields: tuple[FieldBit, ...] = tuple(result)

    def __add__(self, other: object) -> "FieldSelection":
        if isinstance(other, FieldBit | PresetField | FieldSelection):
            return FieldSelection(self, other)
        return NotImplemented

    def __or__(self, other: object) -> "FieldSelection":
        return self.__add__(other)

    def __radd__(self, other: object) -> "FieldSelection":
        if isinstance(other, FieldBit | PresetField):
            return FieldSelection(other, self)
        return NotImplemented

    def __ror__(self, other: object) -> "FieldSelection":
        return self.__radd__(other)

    def __iter__(self) -> Iterator[FieldBit]:
        return iter(self._fields)

    def __len__(self) -> int:
        return len(self._fields)

    def __bool__(self) -> bool:
        return bool(self._fields)

    def __contains__(self, item: object) -> bool:
        return item in self._fields

    def __repr__(self) -> str:
        names = [bit.name for bit in self._fields]
        return f"FieldSelection([{', '.join(names)}])"


def normalize_fields(fields: "Fields") -> FieldSelection:
    """将任意字段选择形式归一化为 :class:`FieldSelection`。"""

    if fields is None:
        return FieldSelection()
    if isinstance(fields, FieldSelection):
        return fields
    if isinstance(fields, PresetField):
        return FieldSelection(*fields.value)
    if isinstance(fields, FieldBit):
        return FieldSelection(fields)
    return FieldSelection(*fields)


def build_bitmap(
    fields: "Fields",
    *,
    width: int = BITMAP_SIZE,
) -> bytearray:
    """将字段选择编码为 ``width`` 字节的**字段位图**。

    Args:
        fields: 字段选择。
        width: 位图字节数。默认 ``20``（160 位）；``0x122C`` 这类把末 4 字节
            用作 ``exclude_flags`` 的命令传 ``16``。

    !!! note "协议实测（2026-10-16，MAC 主机 121.36.248.138）"
        逐位探测 0x122B 的结果：

        ==================  ==========  ==================================
        请求置位            记录槽位   说明
        ==================  ==========  ==================================
        0 / 4 / 128 / 129   各 +1       字段位，各占 4 字节槽
        151 / 152 / 159     不变       保留位，服务端忽略
        ==================  ==========  ==================================

        即**字段位范围是 0–150**（恰为 :class:`FieldBit` 最大值 ``0x96``），
        位 151–159 为保留区（``CTRL_BYTE`` 落在位 152）。所以 0x122B 的请求位图
        就是 **20 字节 / 160 位**，不需要额外附加控制区。

        !!! warning "早期实现为什么错"
            旧版把这个 20 字节当成「16 字节字段位图 + 4 字节控制区」，
            于是 ``build_bitmap`` 用 ``bitmap_int.to_bytes(16, "little")`` 编码 ——
            任何 ``>=128`` 的字段直接抛
            ``OverflowError: int too big to convert``。
            ``PresetField.HANDICAP``（含 BID3/4/5、ASK3/4/5）、
            ``BOARD_STATS``、``ALL`` 一用就炸，而它们引用的
            ``BID3_PRICE(0x80)`` 等位本来就是合法的字段位。

    Raises:
        ValueError: 选中了超出 ``width * 8`` 的字段位，或 ``width`` 非法。
    """

    if not 1 <= width <= BITMAP_SIZE:
        raise ValueError(f"width must be between 1 and {BITMAP_SIZE}, got {width}")
    if isinstance(fields, PresetField) and fields is PresetField.DEBUG:
        return bytearray(b"\xff" * width)
    selection = normalize_fields(fields)
    limit = width * 8
    bitmap_int = 0
    for bit in selection:
        if bit.value >= limit:
            raise ValueError(
                f"字段 {bit.name}(0x{bit.value:02X}) 超出 {width} 字节位图"
                f"（最多 {limit} 位，即 0..{limit - 1}）"
            )
        bitmap_int |= 1 << bit.value
    return bytearray(bitmap_int.to_bytes(width, "little"))


def build_exclude_flags(exclude_flags: int = 0) -> bytes:
    """构建 0x122C 请求体末尾的 4 字节排除标志区。

    只有板块成分报价（0x122C）使用它 —— 该命令把位图之后的 4 字节用作
    ``exclude_flags``，因此**不支持位 128 及以上**的字段（实例化
    ``BoardMembersQuotesCmd`` 时若选中高位字段会被显式拒绝）。
    0x122B 没有这块控制区，其 20 字节位图全部是字段位。
    """

    return exclude_flags.to_bytes(CONTROL_SIZE, "little")


#: 合法字段位的上界（不含）。实测字段位为 0–150，位 151–159 是保留区
#: （``CTRL_BYTE`` 落在位 152），服务端既不回显也不为其分配记录槽位。
FIELD_BIT_LIMIT = max(int(bit) for bit in FieldBit) + 1


def iter_set_bits(bitmap_bytes: bytes) -> Iterator[tuple[int, FieldBit | None, str | None]]:
    """按位序升序遍历所有**置位**槽位，产出 ``(槽位序号, 字段, 格式)``。

    !!! warning "槽位序号必须按全部置位位计数，不能按已知字段计数"
        线上响应可能置位本模块尚未定义的位。这些未知字段在线上**同样占用 4 字节
        槽位**，所以记录宽度和每个已知字段的偏移都要按「这是第几个置位位」来算。
        早期实现只返回已知字段，用它算行宽会让行宽偏小，后续已知字段读到的是
        邻居槽位的值 —— 静默返回错价（实测 ``close`` 读出未知位的 ``9.99``）。

    未知位产出 ``(序号, None, None)``，调用方据此跳过取值但**必须**计入偏移。

    位 151–159 是保留区（实测不占槽位），会被忽略 —— 否则一个保留位置位就会
    凭空多出一个槽位，把所有字段整体推错。
    """

    bitmap_int = _field_bits(bitmap_bytes)
    slot = 0
    while bitmap_int:
        lowbit = bitmap_int & -bitmap_int
        bit_pos = lowbit.bit_length() - 1
        bitmap_int ^= lowbit
        field = FieldBit._value2member_map_.get(bit_pos)
        if isinstance(field, FieldBit):
            yield slot, field, field.fmt
        else:
            yield slot, None, None
        slot += 1


def _field_bits(bitmap_bytes: bytes) -> int:
    """取出位图里**合法字段位**的部分，屏蔽 151–159 保留区。"""

    mask = (1 << FIELD_BIT_LIMIT) - 1
    return int.from_bytes(bitmap_bytes, "little") & mask


def count_set_bits(bitmap_bytes: bytes) -> int:
    """统计字段置位数 —— 即单条记录占用的槽位个数（每槽位 4 字节）。

    保留位（151–159）不计入。
    """

    return _field_bits(bitmap_bytes).bit_count()


def get_active_fields(bitmap_bytes: bytes) -> list[tuple[FieldBit, str]]:
    """从响应位图解析活跃字段（按位序升序），返回 ``(FieldBit, fmt)`` 列表。

    !!! warning "只用于「字段集合已知且完整」的场景"
        未知置位字段会被丢弃，因此**不能**用它的长度去算记录行宽或字段偏移 ——
        那正是 MAC 报价静默错位的根因。需要按槽位解析时用 :func:`iter_set_bits`
        与 :func:`count_set_bits`。
    """

    return [(field, fmt) for _, field, fmt in iter_set_bits(bitmap_bytes) if field is not None]


__all__ = [
    "BITMAP_SIZE",
    "CONTROL_SIZE",
    "CTRL_BYTE",
    "CTRL_EXTENDED",
    "FIELD_BITMAP_MAP",
    "FIELD_BIT_LIMIT",
    "FIELD_POSTPROCESS",
    "FieldBit",
    "FieldSelection",
    "Fields",
    "PresetField",
    "RESP_BITMAP_SIZE",
    "build_bitmap",
    "build_exclude_flags",
    "count_set_bits",
    "get_active_fields",
    "iter_set_bits",
    "normalize_fields",
]
