"""MAC 协议栈（0x1c 帧头，msg_id ``0x12xx`` / ``0x25xx``）。

移植自 easy_tdx 的 ``easy_tdx.mac`` **同步** 子集：批量报价、板块排行、K 线
（全周期 + 复权）、单日/多日分时、缩略采样、逐笔、个股快照、板块列表、所属
板块、资金流、集合竞价、异动、交易时段、K 线偏移、文件、商品列表等。

仅提供同步客户端 :class:`MacClient`；零第三方依赖，公开接口返回
``list[dataclass]``。异步版本（``AsyncMacClient``）不在本移植范围内。
"""

from __future__ import annotations

from .adjust import (
    XdxrEvent,
    apply_forward_adjust,
    compute_forward_factor,
    has_bad_prices,
)
from .bitmap import (
    FieldBit,
    FieldSelection,
    PresetField,
    build_bitmap,
    get_active_fields,
)
from .client import MacClient, convert_board_code
from .codec import build_mac_request, decompress_mac_body, parse_mac_header
from .enums import (
    Adjust,
    BoardType,
    Category,
    ExBoardType,
    ExMarket,
    FilterType,
    Period,
    SortOrder,
    SortType,
)
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
from .transport import MacConnection, ping_mac_all, ping_mac_host

__all__ = [
    "Adjust",
    "AuctionItem",
    "BelongBoardInfo",
    "BoardInfo",
    "BoardType",
    "CapitalFlowData",
    "Category",
    "ExBoardType",
    "ExMarket",
    "FieldBit",
    "FieldSelection",
    "FileMeta",
    "FilterType",
    "GoodsItem",
    "KlineOffsetInfo",
    "MacBar",
    "MacClient",
    "MacConnection",
    "MacMultiTickChart",
    "MacMultiTickDay",
    "MacQuoteField",
    "MacSymbolInfo",
    "MacTick",
    "MacTickChart",
    "MacTransaction",
    "Period",
    "PresetField",
    "ServerSession",
    "SortOrder",
    "SortType",
    "UnusualItem",
    "XdxrEvent",
    "apply_forward_adjust",
    "build_bitmap",
    "build_mac_request",
    "compute_forward_factor",
    "convert_board_code",
    "decompress_mac_body",
    "get_active_fields",
    "has_bad_prices",
    "parse_mac_header",
    "ping_mac_all",
    "ping_mac_host",
]