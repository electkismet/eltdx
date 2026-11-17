"""MAC 协议命令集合（仅同步：请求构造 + 响应解析，不含任何 IO）。

移植自 easy_tdx ``mac/commands/*.py``（15 个命令模块）。为减少文件数量，
本移植将全部命令类集中在本模块，命令类名与字节格式保持一一对应。
"""

from __future__ import annotations

import json
import struct
from sys import modules as _MODULES
from datetime import date, datetime, time
from typing import TypeVar, cast

from .._command import BaseCommand
from ._binary import require_bytes, unpack_from
from .bitmap import (
    FIELD_POSTPROCESS,
    RESP_BITMAP_SIZE,
    Fields,
    PresetField,
    build_bitmap,
    count_set_bits,
    iter_set_bits,
)
from .codec import build_mac_request
from .enums import Adjust, BoardType, FilterType, Period, SortOrder, SortType
from .models import (
    AuctionItem,
    BelongBoardInfo,
    BoardInfo,
    CapitalFlowData,
    FileMeta,
    GoodsItem,
    KlineOffsetInfo,
    MacBar,
    MacMultiTickChart,
    MacMultiTickDay,
    MacQuoteField,
    MacSymbolInfo,
    MacTick,
    MacTickChart,
    MacTransaction,
    ServerSession,
    UnusualItem,
)

T = TypeVar("T")


def _native_exclude_values(values):
    # Preserve the generator-expression object protocol and PEP 479 boundary.
    return (value for value in values)


def _combine_bar_datetime(ymd: int, time_num: int, is_intraday: bool) -> datetime:
    """将日期和可选时间组合为 datetime（日线及以上 ``time_num`` 为 0）。

    ``time_num`` 是**当日秒数**（``09:30:01`` → ``34201``），不是 ``HHMM``，
    因此必须保留秒：早前只取到分钟，两条相邻的秒级 K 线会塌成同一个
    ``09:30:00``，后续按时间排序/去重都会出错。
    """

    from eltdx._native_abi import load_native
    return load_native().mac_combine_datetime(ymd, time_num, is_intraday, _MODULES[__name__])


def _decode_gbk(raw: bytes) -> str:
    from eltdx._native_abi import load_native
    return load_native().mac_decode_gbk(raw)


class SymbolQuotesCmd(BaseCommand[list[MacQuoteField]]):
    """批量获取自定义字段报价（0x122B）。

    Args:
        stocks: ``[(market, code), ...]`` 列表。
        fields: 字段选择，默认 :attr:`PresetField.COMMON`。
    """

    MSG_ID = 0x122B

    def __init__(self, stocks: list[tuple[int, str]], fields: Fields | None = None) -> None:
        from eltdx._native_abi import load_native
        load_native().mac_command_init("quotes", self, (stocks, fields), _MODULES[__name__])

    def build_request(self) -> bytes:
        from eltdx._native_abi import load_native
        return load_native().mac_command_build("quotes", self, _MODULES[__name__])

    def parse_response(self, body: bytes) -> list[MacQuoteField]:
        from eltdx._native_abi import load_native
        return load_native().mac_response_parse("quotes", self, body, _MODULES[__name__])


#: 0x122C 的字段位图只有 16 字节（128 位），其后 4 字节是控制区而非字段位。
#: 实测（2026-10-16）：请求体末 4 字节取 0 / 0x08 / 1 三种值，响应行宽恒为
#: ``68 + 4×字段区置位数``，不受影响 —— 确认它们不参与字段槽位分配。
BOARD_MEMBERS_BITMAP_SIZE = 16


class BoardMembersQuotesCmd(BaseCommand[list[MacQuoteField]]):
    """板块成分报价查询（0x122C）。

    响应格式与 :class:`SymbolQuotesCmd`（0x122B）相同：
    20 字节位图 + 总数 + 行数 + N 条记录，每条 68 字节头 + 活跃字段 x 4 字节。

    !!! note "本命令的请求位图只有 16 字节"
        与 0x122B 不同，0x122C 的请求体是「16 字节字段位图 + 4 字节控制区」，
        控制区承载 ``exclude_flags``，不参与字段槽位分配，因此该命令只能请求
        位 0–127 的字段。请求 :attr:`~PresetField.HANDICAP` 等含 ``>=128`` 位的
        预设会被显式拒绝（见 :func:`~eltdx.mac.bitmap.build_bitmap`）。
    """

    MSG_ID = 0x122C

    def __init__(
        self,
        board_code: int,
        sort_type: SortType = SortType.CHANGE_PCT,
        start: int = 0,
        page_size: int = 80,
        sort_order: SortOrder = SortOrder.NONE,
        fields: Fields = PresetField.NONE,
        exclude_flags: list[FilterType] | None = None,
    ) -> None:
        from eltdx._native_abi import load_native
        load_native().mac_command_init("members", self, (board_code, sort_type, start, page_size, sort_order, fields, exclude_flags), _MODULES[__name__])

    def build_request(self) -> bytes:
        from eltdx._native_abi import load_native
        return load_native().mac_command_build("members", self, _MODULES[__name__])

    def parse_response(self, body: bytes) -> list[MacQuoteField]:
        from eltdx._native_abi import load_native
        return load_native().mac_response_parse("members", self, body, _MODULES[__name__])


class SymbolBarCmd(BaseCommand[list[MacBar]]):
    """获取单只股票的 K 线数据（0x122E，支持复权）。"""

    MSG_ID = 0x122E

    def __init__(
        self,
        market: int,
        code: str,
        period: Period = Period.DAILY,
        start: int = 0,
        count: int = 700,
        fq: Adjust = Adjust.NONE,
        *,
        times: int = 1,
    ) -> None:
        from eltdx._native_abi import load_native
        load_native().mac_command_init("bars", self, (market, code, period, start, count, fq, times), _MODULES[__name__])

    def build_request(self) -> bytes:
        from eltdx._native_abi import load_native
        return load_native().mac_command_build("bars", self, _MODULES[__name__])

    def parse_response(self, body: bytes) -> list[MacBar]:
        from eltdx._native_abi import load_native
        return load_native().mac_response_parse("bars", self, body, _MODULES[__name__])


class SymbolTickChartCmd(BaseCommand[MacTickChart]):
    """获取单日分时图（0x122D）。"""

    MSG_ID = 0x122D

    def __init__(self, market: int, code: str, query_date: date | None = None) -> None:
        from eltdx._native_abi import load_native
        load_native().mac_command_init("minute", self, (market, code, query_date), _MODULES[__name__])

    def build_request(self) -> bytes:
        from eltdx._native_abi import load_native
        return load_native().mac_command_build("minute", self, _MODULES[__name__])

    def parse_response(self, body: bytes) -> MacTickChart:
        from eltdx._native_abi import load_native
        return load_native().mac_response_parse("minute", self, body, _MODULES[__name__])


class TickChartsCmd(BaseCommand[MacMultiTickChart]):
    """获取多日分时图（0x123E，最多 5 天）。"""

    MSG_ID = 0x123E
    _TICK_OFFSET = 71
    _TICK_SIZE = struct.calcsize("<HffHH")
    _TAIL_FMT = "<44sBHf5x2I5ffIf12s2fI"
    _TAIL_SIZE = struct.calcsize(_TAIL_FMT)
    #: 协议里日期 / 昨收固定各 5 个槽位，``days`` 不得超过它。
    MAX_DAYS = 5

    def __init__(
        self,
        market: int,
        code: str,
        start_date: date | None = None,
        days: int = 5,
    ) -> None:
        from eltdx._native_abi import load_native
        load_native().mac_command_init("multi_minute", self, (market, code, start_date, days), _MODULES[__name__])

    def build_request(self) -> bytes:
        from eltdx._native_abi import load_native
        return load_native().mac_command_build("multi_minute", self, _MODULES[__name__])

    def parse_response(self, body: bytes) -> MacMultiTickChart:
        from eltdx._native_abi import load_native
        return load_native().mac_response_parse("multi_minute", self, body, _MODULES[__name__])


class SymbolTransactionCmd(BaseCommand[list[MacTransaction]]):
    """获取逐笔成交数据（0x122F）。"""

    MSG_ID = 0x122F

    def __init__(
        self,
        market: int,
        code: str,
        query_date: date | None = None,
        start: int = 0,
        count: int = 1000,
    ) -> None:
        from eltdx._native_abi import load_native
        load_native().mac_command_init("transactions", self, (market, code, query_date, start, count), _MODULES[__name__])

    def build_request(self) -> bytes:
        from eltdx._native_abi import load_native
        return load_native().mac_command_build("transactions", self, _MODULES[__name__])

    def parse_response(self, body: bytes) -> list[MacTransaction]:
        from eltdx._native_abi import load_native
        return load_native().mac_response_parse("transactions", self, body, _MODULES[__name__])


class SymbolInfoCmd(BaseCommand[MacSymbolInfo]):
    """获取个股简要特征快照（0x122A）。"""

    MSG_ID = 0x122A

    def __init__(self, market: int, code: str) -> None:
        from eltdx._native_abi import load_native
        load_native().mac_command_init("info", self, (market, code), _MODULES[__name__])

    def build_request(self) -> bytes:
        from eltdx._native_abi import load_native
        return load_native().mac_command_build("info", self, _MODULES[__name__])

    def parse_response(self, body: bytes) -> MacSymbolInfo:
        from eltdx._native_abi import load_native
        return load_native().mac_response_parse("info", self, body, _MODULES[__name__])


# ============================================================
# 板块 / 所属板块 / 资金流：0x1231 / 0x1218
# ============================================================

_BOARD_RECORD_FMT = "<H6s16s44sfffH6s16s44sfff"
_BOARD_RECORD_SIZE = struct.calcsize(_BOARD_RECORD_FMT)  # 160


class BoardListCmd(BaseCommand[list[BoardInfo]]):
    """查询板块列表（0x1231）。"""

    MSG_ID = 0x1231

    def __init__(
        self,
        board_type: BoardType = BoardType.ALL,
        start: int = 0,
        page_size: int = 150,
    ) -> None:
        from eltdx._native_abi import load_native
        load_native().mac_command_init("boards", self, (board_type, start, page_size), _MODULES[__name__])

    def build_request(self) -> bytes:
        from eltdx._native_abi import load_native
        return load_native().mac_command_build("boards", self, _MODULES[__name__])

    def parse_response(self, body: bytes) -> list[BoardInfo]:
        from eltdx._native_abi import load_native
        return load_native().mac_response_parse("boards", self, body, _MODULES[__name__])


def _to_float(value: object) -> float:
    """安全转 float（失败返回 0.0）。"""

    from eltdx._native_abi import load_native
    try:
        return load_native().mac_numeric_value(False, value, _MODULES[__name__])
    except (ValueError, TypeError):
        return load_native().mac_numeric_default(False)


def _to_int(value: object) -> int:
    """安全转 int（失败返回 0）。"""

    from eltdx._native_abi import load_native
    try:
        return load_native().mac_numeric_value(True, value, _MODULES[__name__])
    except (ValueError, TypeError):
        return load_native().mac_numeric_default(True)


class SymbolBelongBoardCmd(BaseCommand[list[BelongBoardInfo]]):
    """查询个股所属板块（0x1218，``head_flag=1``）。"""

    MSG_ID = 0x1218
    HEAD_FLAG = 1

    def __init__(self, market: int, code: str) -> None:
        from eltdx._native_abi import load_native
        load_native().mac_command_init("belong", self, (market, code), _MODULES[__name__])

    def build_request(self) -> bytes:
        from eltdx._native_abi import load_native
        return load_native().mac_command_build("belong", self, _MODULES[__name__])

    def parse_response(self, body: bytes) -> list[BelongBoardInfo]:
        from eltdx._native_abi import load_native
        return load_native().mac_response_parse("belong", self, body, _MODULES[__name__])


class SymbolCapitalFlowCmd(BaseCommand[CapitalFlowData | None]):
    """查询个股资金流向（0x1218，``head_flag=2``）。"""

    MSG_ID = 0x1218
    HEAD_FLAG = 2

    def __init__(self, market: int, code: str) -> None:
        from eltdx._native_abi import load_native
        load_native().mac_command_init("capital", self, (market, code), _MODULES[__name__])

    def build_request(self) -> bytes:
        from eltdx._native_abi import load_native
        return load_native().mac_command_build("capital", self, _MODULES[__name__])

    def parse_response(self, body: bytes) -> CapitalFlowData | None:
        from eltdx._native_abi import load_native
        return load_native().mac_response_parse("capital", self, body, _MODULES[__name__])


# ============================================================
# 集合竞价 / 异动 / 服务器信息 / K线偏移：0x123D / 0x1237 / 0x120F / 0x124A
# ============================================================


class SymbolAuctionCmd(BaseCommand[list[AuctionItem]]):
    """查询集合竞价数据（0x123D）。"""

    MSG_ID = 0x123D

    def __init__(self, market: int, code: str, start: int = 0, count: int = 500) -> None:
        from eltdx._native_abi import load_native
        load_native().mac_command_init("auction", self, (market, code, start, count), _MODULES[__name__])

    def build_request(self) -> bytes:
        from eltdx._native_abi import load_native
        return load_native().mac_command_build("auction", self, _MODULES[__name__])

    def parse_response(self, body: bytes) -> list[AuctionItem]:
        from eltdx._native_abi import load_native
        return load_native().mac_response_parse("auction", self, body, _MODULES[__name__])


def _describe_unusual(unusual_type: int, data: bytes) -> tuple[str, str]:
    """根据异动类型解析描述和数值。"""

    from eltdx._native_abi import load_native
    return load_native().mac_unusual_description(unusual_type, data, _MODULES[__name__])


class UnusualCmd(BaseCommand[list[UnusualItem]]):
    """查询异动数据（0x1237，最大 600 条）。"""

    MSG_ID = 0x1237

    def __init__(self, market: int, start: int = 0, count: int = 600) -> None:
        from eltdx._native_abi import load_native
        load_native().mac_command_init("unusual", self, (market, start, count), _MODULES[__name__])

    def build_request(self) -> bytes:
        from eltdx._native_abi import load_native
        return load_native().mac_command_build("unusual", self, _MODULES[__name__])

    def parse_response(self, body: bytes) -> list[UnusualItem]:
        from eltdx._native_abi import load_native
        return load_native().mac_response_parse("unusual", self, body, _MODULES[__name__])


class ServerInfoCmd(BaseCommand[ServerSession]):
    """查询服务器交易时段信息（0x120F）。"""

    MSG_ID = 0x120F

    def build_request(self) -> bytes:
        from eltdx._native_abi import load_native
        return load_native().mac_command_build("server", self, _MODULES[__name__])

    def parse_response(self, body: bytes) -> ServerSession:
        from eltdx._native_abi import load_native
        return load_native().mac_response_parse("server", self, body, _MODULES[__name__])


class KlineOffsetCmd(BaseCommand[KlineOffsetInfo]):
    """查询 K 线数据偏移（0x124A）。"""

    MSG_ID = 0x124A

    def __init__(self, offset: int = 0, count: int = 128000) -> None:
        from eltdx._native_abi import load_native
        load_native().mac_command_init("offsets", self, (offset, count), _MODULES[__name__])

    def build_request(self) -> bytes:
        from eltdx._native_abi import load_native
        return load_native().mac_command_build("offsets", self, _MODULES[__name__])

    def parse_response(self, body: bytes) -> KlineOffsetInfo:
        from eltdx._native_abi import load_native
        return load_native().mac_response_parse("offsets", self, body, _MODULES[__name__])


# ============================================================
# 缩略采样 / 商品列表 / 文件：0x254D / 0x2562 / 0x1215 / 0x1217
# ============================================================


class ChartSamplingCmd(BaseCommand[list[float]]):
    """获取分时缩略采样价格点（0x254D，返回 240 个 float）。"""

    MSG_ID = 0x254D
    _CODE_LEN = 22
    _RESPONSE_HEADER_SIZE = 42  # H(2) + 22s(22) + 9*H(18)

    def __init__(self, market: int, code: str) -> None:
        from eltdx._native_abi import load_native
        load_native().mac_command_init("sampling", self, (market, code), _MODULES[__name__])

    def build_request(self) -> bytes:
        from eltdx._native_abi import load_native
        return load_native().mac_command_build("sampling", self, _MODULES[__name__])

    def parse_response(self, body: bytes) -> list[float]:
        from eltdx._native_abi import load_native
        return load_native().mac_response_parse("sampling", self, body, _MODULES[__name__])


class GoodsListCmd(BaseCommand[list[GoodsItem]]):
    """获取扩展市场（期货/期权等）商品列表（0x2562）。"""

    MSG_ID = 0x2562
    MAX_COUNT = 1000
    _RECORD_SIZE = 48
    _RECORD_FMT = "<H23sHIBfffHH"

    def __init__(self, market: int, start: int = 0, count: int = 600) -> None:
        from eltdx._native_abi import load_native
        load_native().mac_command_init("goods", self, (market, start, count), _MODULES[__name__])

    def build_request(self) -> bytes:
        from eltdx._native_abi import load_native
        return load_native().mac_command_build("goods", self, _MODULES[__name__])

    def parse_response(self, body: bytes) -> list[GoodsItem]:
        from eltdx._native_abi import load_native
        return load_native().mac_response_parse("goods", self, body, _MODULES[__name__])


_FILENAME_LEN = 70
_FILENAME_PAD = 30


class FileListCmd(BaseCommand[FileMeta]):
    """查询远程文件元信息（0x1215）。"""

    MSG_ID = 0x1215

    def __init__(self, filename: str, offset: int = 0) -> None:
        from eltdx._native_abi import load_native
        load_native().mac_command_init("file_meta", self, (filename, offset), _MODULES[__name__])

    def build_request(self) -> bytes:
        from eltdx._native_abi import load_native
        return load_native().mac_command_build("file_meta", self, _MODULES[__name__])

    def parse_response(self, body: bytes) -> FileMeta:
        from eltdx._native_abi import load_native
        return load_native().mac_response_parse("file_meta", self, body, _MODULES[__name__])


class FileDownloadCmd(BaseCommand[bytes]):
    """分段下载远程文件内容（0x1217）。"""

    MSG_ID = 0x1217

    def __init__(
        self,
        filename: str,
        index: int = 1,
        offset: int = 0,
        size: int = 30000,
    ) -> None:
        from eltdx._native_abi import load_native
        load_native().mac_command_init("file_download", self, (filename, index, offset, size), _MODULES[__name__])

    def build_request(self) -> bytes:
        from eltdx._native_abi import load_native
        return load_native().mac_command_build("file_download", self, _MODULES[__name__])

    def parse_response(self, body: bytes) -> bytes:
        from eltdx._native_abi import load_native
        return load_native().mac_response_parse("file_download", self, body, _MODULES[__name__])


__all__ = [
    "BaseCommand",
    "BoardListCmd",
    "BoardMembersQuotesCmd",
    "ChartSamplingCmd",
    "FileDownloadCmd",
    "FileListCmd",
    "GoodsListCmd",
    "KlineOffsetCmd",
    "ServerInfoCmd",
    "SymbolAuctionCmd",
    "SymbolBarCmd",
    "SymbolBelongBoardCmd",
    "SymbolCapitalFlowCmd",
    "SymbolInfoCmd",
    "SymbolQuotesCmd",
    "SymbolTickChartCmd",
    "SymbolTransactionCmd",
    "TickChartsCmd",
    "UnusualCmd",
]
