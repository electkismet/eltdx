"""7709 command registry.

The registry is the bridge from the human-readable API to the binary command
numbers documented in ``C:\\Users\\ax\\Desktop\\eltdx\\7709``.
"""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True, slots=True)
class CommandSpec:
    code: int
    name: str
    module: str
    method: str
    required_for_1_0: bool
    document: str
    retry_safe: bool = False

    @property
    def hex(self) -> str:
        return f"0x{self.code:04x}"


COMMANDS: dict[str, CommandSpec] = {
    "heartbeat": CommandSpec(0x0004, "heartbeat", "session", "heartbeat", True, "0x0004-心跳保活接口.md", True),
    "handshake": CommandSpec(0x000D, "handshake", "session", "handshake", True, "0x000d-连接握手接口.md", True),
    "capital_changes": CommandSpec(0x000F, "capital_changes", "corporate", "capital_changes", False, "0x000f-股本变迁查询接口.md", True),
    "finance_batch": CommandSpec(0x0010, "finance_batch", "corporate", "finance_batch", False, "0x0010-财务信息批量查询&下发接口.md", True),
    "security_list": CommandSpec(0x044D, "security_list", "codes", "list", True, "0x044d-代码表分页接口.md", True),
    "security_count": CommandSpec(0x044E, "security_count", "codes", "count", True, "0x044e-代码数量接口.md", True),
    "special_limits": CommandSpec(0x0452, "special_limits", "limits", "special", False, "0x0452-特殊品种涨跌停限制表接口.md", True),
    "intraday_aux": CommandSpec(0x051B, "intraday_aux", "minutes", "aux", False, "0x051b-个股分时副图数据接口.md", True),
    "delisted_bars": CommandSpec(0x052B, "delisted_bars", "delisted_bars", "get", False, "0x052b-退市K线日线接口.md", True),
    "klines": CommandSpec(0x052D, "klines", "bars", "get", True, "0x052d-K线周期数据接口.md", True),
    "today_intraday": CommandSpec(0x0537, "today_intraday", "minutes", "today", True, "0x0537-个股当前日分时图接口.md", True),
    "legacy_quotes": CommandSpec(0x053E, "legacy_quotes", "quotes", "legacy", False, "0x053e-旧版批量行情接口.md", True),
    "refresh_stream": CommandSpec(0x0547, "refresh_stream", "quotes", "refresh", True, "0x0547-行情增量刷新推送接口.md", True),
    "category_quotes": CommandSpec(0x054B, "category_quotes", "quotes", "list_by_category", True, "0x054b-分类行情列表分页接口.md", True),
    "snapshots": CommandSpec(0x054C, "snapshots", "quotes", "get", True, "0x054c-显式代码批量行情快照接口.md", True),
    "auction_series": CommandSpec(0x056A, "auction_series", "auctions", "series", False, "0x056a-集合竞价明细接口.md", True),
    "file_content": CommandSpec(0x06B9, "file_content", "resources", "read", False, "0x06b9-服务器文件读取接口.md", True),
    "historical_intraday": CommandSpec(0x0FB4, "historical_intraday", "minutes", "history", True, "0x0fb4-历史分时数据接口.md", True),
    "today_ticks": CommandSpec(0x0FC5, "today_ticks", "trades", "today", True, "0x0fc5-当日成交明细分页接口.md", True),
    "historical_ticks": CommandSpec(0x0FC6, "historical_ticks", "trades", "history", False, "0x0fc6-历史成交明细增强分页接口.md", True),
    "sparkline": CommandSpec(0x0FD1, "sparkline", "minutes", "sparkline", False, "0x0fd1-单标的价格小走势图接口.md", True),
    "recent_intraday": CommandSpec(0x0FEB, "recent_intraday", "minutes", "recent", False, "0x0feb-近期历史分时图接口.md", True),
}


def command_code(name: str) -> int:
    return COMMANDS[name].code


def required_commands() -> list[CommandSpec]:
    return [spec for spec in COMMANDS.values() if spec.required_for_1_0]
