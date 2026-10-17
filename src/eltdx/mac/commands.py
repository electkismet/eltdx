"""MAC 协议命令集合（仅同步：请求构造 + 响应解析，不含任何 IO）。

移植自 easy_tdx ``mac/commands/*.py``（15 个命令模块）。为减少文件数量，
本移植将全部命令类集中在本模块，命令类名与字节格式保持一一对应。
"""

from __future__ import annotations

import json
import struct
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


def _combine_bar_datetime(ymd: int, time_num: int, is_intraday: bool) -> datetime:
    """将日期和可选时间组合为 datetime（日线及以上 ``time_num`` 为 0）。"""

    year = ymd // 10000
    month = (ymd % 10000) // 100
    day = ymd % 100
    if is_intraday and time_num:
        hour = time_num // 3600
        minute = (time_num % 3600) // 60
        return datetime(year, month, day, hour, minute)
    return datetime(year, month, day)


def _decode_gbk(raw: bytes) -> str:
    return raw.decode("gbk", errors="ignore").replace("\x00", "")


class SymbolQuotesCmd(BaseCommand[list[MacQuoteField]]):
    """批量获取自定义字段报价（0x122B）。

    Args:
        stocks: ``[(market, code), ...]`` 列表。
        fields: 字段选择，默认 :attr:`PresetField.COMMON`。
    """

    MSG_ID = 0x122B

    def __init__(self, stocks: list[tuple[int, str]], fields: Fields | None = None) -> None:
        if not stocks:
            raise ValueError("stocks 不能为空")
        self._stocks = stocks
        self._fields: Fields = PresetField.COMMON if fields is None else fields
        self._bitmap = bytes(build_bitmap(self._fields))

    def build_request(self) -> bytes:
        body = bytearray(self._bitmap)
        body += struct.pack("<H", len(self._stocks))
        for market, code in self._stocks:
            body += struct.pack("<H22s", market, code.encode("gbk"))
        return build_mac_request(self.MSG_ID, bytes(body))

    def parse_response(self, body: bytes) -> list[MacQuoteField]:
        pos = RESP_BITMAP_SIZE
        (_total_stocks, row_count) = unpack_from("<IH", body, pos, "symbol_quotes header")
        pos += 6

        # 行宽按**全部置位位**算，未知字段在线上也占 4 字节槽位；不能只数已知字段。
        # 位图读完整 20 字节：实测字段位为 0–150，其中 128–150 也在 [16:20] 内。
        resp_bitmap = body[:RESP_BITMAP_SIZE]
        slots = list(iter_set_bits(resp_bitmap))
        row_len = 68 + 4 * count_set_bits(resp_bitmap)

        results: list[MacQuoteField] = []
        for _ in range(row_count):
            row_end = pos + row_len
            if row_end > len(body):
                break
            row_data = body[pos:row_end]
            pos = row_end

            (market, code_raw, name_raw) = unpack_from(
                "<H22s44s", row_data, 0, "symbol_quotes row"
            )
            fields_dict: dict[str, object] = {}
            for slot, field_bit, fmt in slots:
                if field_bit is None:
                    # 未知字段：槽位照占，但没有可命名的键。
                    continue
                value_bytes = row_data[68 + slot * 4 : 68 + (slot + 1) * 4]
                (value,) = struct.unpack(fmt, value_bytes)
                post_fn = FIELD_POSTPROCESS.get(field_bit.value)
                if post_fn is not None:
                    value = cast("object", post_fn(value, market))
                fields_dict[field_bit.field_name] = value

            results.append(
                MacQuoteField(
                    market=market,
                    code=_decode_gbk(code_raw),
                    name=_decode_gbk(name_raw),
                    fields=fields_dict,
                )
            )

        return results


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
        self._board_code = board_code
        self._sort_type = sort_type
        self._start = start
        self._page_size = page_size
        self._sort_order = sort_order
        self._fields = fields
        self._exclude_flags = exclude_flags or []

    def build_request(self) -> bytes:
        body = struct.pack(
            "<I9xHIHBB",
            self._board_code,
            int(self._sort_type),
            self._start,
            self._page_size,
            int(self._sort_order),
            0,
        )
        # 位图只有 16 字节：高位字段会在这里被显式拒绝，而不是被静默截断。
        bitmap = build_bitmap(self._fields, width=BOARD_MEMBERS_BITMAP_SIZE)
        body += bytes(bitmap)
        b1 = sum(int(f.value) for f in self._exclude_flags)
        body += struct.pack("<BBBB", 0, b1, 0, 1)
        return build_mac_request(self.MSG_ID, body)

    def parse_response(self, body: bytes) -> list[MacQuoteField]:
        resp_bitmap = body[:RESP_BITMAP_SIZE]
        (_total, row_count) = unpack_from("<IH", body, RESP_BITMAP_SIZE, "board_members header")

        # 响应位图读完整 20 字节（与 0x122B 一致），否则服务端回显的高位字段会被漏掉。
        active_slots = list(iter_set_bits(resp_bitmap))
        row_len = 68 + count_set_bits(resp_bitmap) * 4

        results: list[MacQuoteField] = []
        for i in range(row_count):
            row_start = 26 + i * row_len
            market_raw = unpack_from("<H", body, row_start, f"board_members row[{i}] market")[0]
            code_raw = body[row_start + 2 : row_start + 24]
            name_raw = body[row_start + 24 : row_start + 68]

            fields_dict: dict[str, object] = {}
            for slot, field_bit, fmt in active_slots:
                if field_bit is None:
                    # 未知字段槽位照占，但不产出键。
                    continue
                val_bytes = body[
                    row_start + 68 + slot * 4 : row_start + 68 + (slot + 1) * 4
                ]
                if len(val_bytes) < 4:
                    break
                (value,) = struct.unpack(fmt, val_bytes)
                # 与 SymbolQuotesCmd 保持一致：AH_CODE 这类字段要按同一规则转换，
                # 否则同一个字段在两个命令里返回的类型/语义不同。
                post_fn = FIELD_POSTPROCESS.get(field_bit.value)
                if post_fn is not None:
                    value = cast("object", post_fn(value, market_raw))
                fields_dict[field_bit.field_name] = value

            results.append(
                MacQuoteField(
                    market=market_raw,
                    code=code_raw.decode("gbk", errors="replace").rstrip("\x00"),
                    name=name_raw.decode("gbk", errors="replace").rstrip("\x00"),
                    fields=fields_dict,
                )
            )

        return results


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
        # ``times`` 放在最后且 keyword-only：历史上它插在 period 之后，使
        # ``SymbolBarCmd(47, "IFL0", Period.DAILY, 0, 800)`` 这类位置传参把
        # start/count 错位到 times/start。与 MAC-EX 侧共用同一实现后必须保持
        # 位置参数顺序稳定。
        self._market = market
        self._code = code
        self._period = period
        self._times = times
        self._start = start
        self._count = count
        self._fq = fq

    def build_request(self) -> bytes:
        # 帧体布局（27 字节参数区）：market H、code 22s、period H、times H、
        # start I、count H、fq H、4 个标志字节、H、4s。
        #
        # 注意：这里必须是 **4** 个标志字节（``bbbb``）。历史上漏写成 3 个
        # （``bbb``）导致帧体少 1 字节，服务端对任何标的都只回 1 字节空体，
        # ``parse_response`` 随即抛 ProtocolError —— 即
        # ``MacClient.get_stock_kline`` 全线不可用的根因。
        body = struct.pack(
            "<H22sHHIHHbbbbH4s",
            self._market,
            self._code.encode("gbk"),
            int(self._period),
            self._times,
            self._start,
            self._count,
            int(self._fq),
            1,
            1,
            0,
            1,
            0,
            b"",
        )
        return build_mac_request(self.MSG_ID, body)

    def parse_response(self, body: bytes) -> list[MacBar]:
        (_category_flag, _flag, count, _start) = unpack_from(
            "<HBHI", body, 24, "symbol_bar header"
        )
        count = min(count, max(0, (len(body) - 33) // 36))

        # 日内周期才带 time_num：分钟级（< DAILY）、1 分钟、多分钟，以及**秒级**。
        # 漏掉 SECONDS 会让秒级 K 线的 time_num 被丢弃，时间退化成当天 00:00。
        is_intraday = (
            self._period < Period.DAILY
            or self._period == Period.MIN_1
            or self._period == Period.MINS
            or self._period == Period.SECONDS
        )

        results: list[MacBar] = []
        for i in range(count):
            offset = 33 + i * 36
            if offset + 36 > len(body):
                break
            (ymd, time_num, open_, high, low, close, amount, vol, float_shares) = unpack_from(
                "<II7f", body, offset, f"symbol_bar bar[{i}]"
            )
            if ymd < 19900101 or ymd > 20991231:
                continue
            results.append(
                MacBar(
                    datetime=_combine_bar_datetime(ymd, time_num, is_intraday),
                    open=open_,
                    high=high,
                    low=low,
                    close=close,
                    vol=vol,
                    amount=amount,
                    float_shares=float_shares,
                )
            )
        return results


class SymbolTickChartCmd(BaseCommand[MacTickChart]):
    """获取单日分时图（0x122D）。"""

    MSG_ID = 0x122D

    def __init__(self, market: int, code: str, query_date: date | None = None) -> None:
        self._market = market
        self._code = code
        if query_date is not None:
            self._ymd = query_date.year * 10000 + query_date.month * 100 + query_date.day
        else:
            self._ymd = 0

    def build_request(self) -> bytes:
        body = struct.pack(
            "<H22sI5H",
            self._market,
            self._code.encode("gbk"),
            self._ymd,
            1,
            0,
            0,
            0,
            0,
        )
        return build_mac_request(self.MSG_ID, body)

    def parse_response(self, body: bytes) -> MacTickChart:
        (market, code_raw, _query_date, _reserved, _ref_price, count) = unpack_from(
            "<H22sIBfH", body, 0, "tick_chart header"
        )

        ticks: list[MacTick] = []
        for i in range(count):
            offset = 35 + i * 18
            (minutes, price, avg, vol, momentum) = unpack_from(
                "<HffIf", body, offset, f"tick_chart tick[{i}]"
            )
            ticks.append(
                MacTick(
                    time=time(minutes // 60 % 24, minutes % 60),
                    price=price,
                    avg=avg,
                    vol=vol,
                    momentum=momentum,
                )
            )

        tail_offset = 35 + count * 18
        (
            name_raw,
            _decimal,
            _category,
            _vol_unit,
            _date_raw,
            _time_raw,
            pre_close,
            open_,
            high,
            low,
            close,
            _momentum_tail,
            vol,
            amount,
            _tail_pad2,
            turnover,
            avg_tail,
            _industry,
        ) = unpack_from("<44sBHf5x2I5ffIf12s2fI", body, tail_offset, "tick_chart tail")

        return MacTickChart(
            market=market,
            code=_decode_gbk(code_raw),
            name=_decode_gbk(name_raw),
            pre_close=pre_close,
            open=open_,
            high=high,
            low=low,
            close=close,
            vol=int(vol),
            amount=amount,
            turnover=turnover,
            avg=avg_tail,
            charts=ticks,
        )


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
        # 响应头固定 ``<5I`` + ``<5f``（各 5 个槽位），days 超范围会让解析越界。
        # 在构造期就拒绝，比等到 parse_response 抛 IndexError 更早也更明确。
        if not 1 <= days <= self.MAX_DAYS:
            raise ValueError(f"days 必须在 1..{self.MAX_DAYS} 之间，收到 {days}")
        self._market = market
        self._code = code
        if start_date is not None:
            self._ymd = start_date.year * 10000 + start_date.month * 100 + start_date.day
        else:
            self._ymd = 0
        self._days = days

    def build_request(self) -> bytes:
        body = struct.pack(
            "<H22sIHH6x",
            self._market,
            self._code.encode("gbk"),
            self._ymd,
            self._days,
            1,
        )
        return build_mac_request(self.MSG_ID, body)

    def parse_response(self, body: bytes) -> MacMultiTickChart:
        (market, code_raw) = unpack_from("<H22s", body, 0, "tick_charts header")

        date_ints = unpack_from("<5I", body, 24, "tick_charts dates")
        pre_close_floats = unpack_from("<5f", body, 44, "tick_charts pre_closes")
        (count, _send_last, page_size, total) = unpack_from(
            "<HBHH", body, 64, "tick_charts header2"
        )
        # 响应头只给了 5 个日期 / 昨收槽位：服务端回 count>5（或 page_size=0 导致
        # 后续整除）都不能直接采信。天数改为受两个上界约束：
        #   1) 前导有效日期槽的个数 —— 空槽的 ymd=0 会让 date(0,0,0) 直接抛错；
        #   2) ``MAX_DAYS``。
        leading_days = 0
        for value in date_ints:
            if not value:
                break
            leading_days += 1
        count = max(0, min(count, leading_days, self.MAX_DAYS))
        page_size = max(1, page_size)

        max_ticks = max(0, (len(body) - self._TICK_OFFSET - self._TAIL_SIZE) // self._TICK_SIZE)
        actual_total = min(total, max_ticks)
        if actual_total >= count * page_size:
            tick_counts = [page_size] * count
        else:
            complete_days = min(count - 1, actual_total // page_size) if count else 0
            first_day_count = actual_total - complete_days * page_size
            empty_days = count - complete_days - 1
            tick_counts = [first_day_count] + [page_size] * complete_days + [0] * empty_days

        days: list[MacMultiTickDay] = []
        tick_index = 0
        for d, tick_count in enumerate(tick_counts):
            ticks: list[MacTick] = []
            for t in range(tick_count):
                offset = self._TICK_OFFSET + tick_index * self._TICK_SIZE
                (minutes, price, avg, vol, _tick_reserved) = unpack_from(
                    "<HffHH", body, offset, f"tick_charts tick[{d}][{t}]"
                )
                tick_index += 1
                ticks.append(
                    MacTick(
                        time=time(minutes // 60 % 24, minutes % 60),
                        price=price,
                        avg=avg,
                        vol=vol,
                    )
                )

            ymd = date_ints[d]
            days.append(
                MacMultiTickDay(
                    date=date(ymd // 10000, (ymd % 10000) // 100, ymd % 100),
                    pre_close=pre_close_floats[d],
                    ticks=ticks,
                )
            )

        tail_offset = self._TICK_OFFSET + actual_total * self._TICK_SIZE
        (
            name_raw,
            _decimal,
            _category,
            _vol_unit,
            _date_raw,
            _time_raw,
            pre_close,
            open_,
            high,
            low,
            close,
            _momentum,
            vol,
            amount,
            _tail_pad2,
            turnover,
            avg,
            _industry,
        ) = unpack_from(self._TAIL_FMT, body, tail_offset, "tick_charts tail")

        return MacMultiTickChart(
            market=market,
            code=_decode_gbk(code_raw),
            name=_decode_gbk(name_raw),
            pre_close=pre_close,
            open=open_,
            high=high,
            low=low,
            close=close,
            vol=int(vol),
            amount=amount,
            turnover=turnover,
            avg=avg,
            charts=days,
        )


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
        self._market = market
        self._code = code
        if query_date is not None:
            self._ymd = query_date.year * 10000 + query_date.month * 100 + query_date.day
        else:
            self._ymd = 0
        self._start = start
        self._count = count

    def build_request(self) -> bytes:
        body = struct.pack(
            "<H22sIIH10x",
            self._market,
            self._code.encode("gbk"),
            self._ymd,
            self._start,
            self._count,
        )
        return build_mac_request(self.MSG_ID, body)

    def parse_response(self, body: bytes) -> list[MacTransaction]:
        (count,) = unpack_from("<H", body, 29, "transaction count")

        results: list[MacTransaction] = []
        for i in range(count):
            offset = 39 + i * 18
            (time_sec, price, volume, trade_count, bs_flag) = unpack_from(
                "<IfIIH", body, offset, f"transaction item[{i}]"
            )
            results.append(
                MacTransaction(
                    time=time(time_sec // 3600, time_sec % 3600 // 60, time_sec % 60),
                    price=price,
                    vol=volume,
                    trade_count=trade_count,
                    bs_flag=bs_flag,
                )
            )
        return results


class SymbolInfoCmd(BaseCommand[MacSymbolInfo]):
    """获取个股简要特征快照（0x122A）。"""

    MSG_ID = 0x122A

    def __init__(self, market: int, code: str) -> None:
        self._market = market
        self._code = code

    def build_request(self) -> bytes:
        body = struct.pack("<H22sI12x", self._market, self._code.encode("gbk"), 1)
        return build_mac_request(self.MSG_ID, body)

    def parse_response(self, body: bytes) -> MacSymbolInfo:
        (market, code_raw, name_raw) = unpack_from("<H22s44s", body, 8, "symbol_info identity")
        (
            date_raw,
            time_raw,
            activity,
            pre_close,
            open_,
            high,
            low,
            close,
            momentum,
            vol,
            amount,
            inside_volume,
            outside_volume,
        ) = unpack_from("<III5ffIfII", body, 96, "symbol_info core")

        (_decimal, _a, _b, _c, _vr, turnover, avg) = unpack_from(
            "<HIf20xI3f", body, 148, "symbol_info extra"
        )

        dt = datetime(
            date_raw // 10000,
            (date_raw % 10000) // 100,
            date_raw % 100,
            time_raw // 10000,
            (time_raw % 10000) // 100,
            time_raw % 100,
        )

        return MacSymbolInfo(
            market=market,
            code=_decode_gbk(code_raw),
            name=_decode_gbk(name_raw),
            time=dt,
            activity=activity,
            pre_close=pre_close,
            open=open_,
            high=high,
            low=low,
            close=close,
            momentum=momentum,
            vol=int(vol),
            amount=amount,
            inside_volume=inside_volume,
            outside_volume=outside_volume,
            turnover=turnover,
            avg=avg,
        )

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
        self._board_type = board_type
        self._start = start
        self._page_size = page_size

    def build_request(self) -> bytes:
        body = struct.pack(
            "<HHBBHH8x",
            self._page_size,
            int(self._board_type),
            0,
            0,
            self._start,
            1,
        )
        return build_mac_request(self.MSG_ID, body)

    def parse_response(self, body: bytes) -> list[BoardInfo]:
        count_all, _total = unpack_from("<HH", body, 0, "board_list header")
        count = count_all // 2  # 服务器返回 board_info + symbol_info 各一份

        results: list[BoardInfo] = []
        for i in range(count):
            offset = 4 + i * _BOARD_RECORD_SIZE
            (
                market,
                code_raw,
                _pad1,
                name_raw,
                price,
                rise_speed,
                pre_close,
                symbol_market,
                symbol_code_raw,
                _pad2,
                symbol_name_raw,
                symbol_price,
                symbol_rise_speed,
                symbol_pre_close,
            ) = unpack_from(_BOARD_RECORD_FMT, body, offset, f"board_list record[{i}]")

            results.append(
                BoardInfo(
                    market=market,
                    code=code_raw.decode("gbk", errors="replace").rstrip("\x00"),
                    name=name_raw.decode("gbk", errors="replace").rstrip("\x00"),
                    price=price,
                    rise_speed=rise_speed,
                    pre_close=pre_close,
                    symbol_market=symbol_market,
                    symbol_code=symbol_code_raw.decode("gbk", errors="replace").rstrip("\x00"),
                    symbol_name=symbol_name_raw.decode("gbk", errors="replace").rstrip("\x00"),
                    symbol_price=symbol_price,
                    symbol_rise_speed=symbol_rise_speed,
                    symbol_pre_close=symbol_pre_close,
                )
            )
        return results


def _to_float(value: object) -> float:
    """安全转 float（失败返回 0.0）。"""

    try:
        return float(cast("float", value))
    except (ValueError, TypeError):
        return 0.0


def _to_int(value: object) -> int:
    """安全转 int（失败返回 0）。"""

    try:
        return int(float(cast("float", value)))
    except (ValueError, TypeError):
        return 0


class SymbolBelongBoardCmd(BaseCommand[list[BelongBoardInfo]]):
    """查询个股所属板块（0x1218，``head_flag=1``）。"""

    MSG_ID = 0x1218
    HEAD_FLAG = 1

    def __init__(self, market: int, code: str) -> None:
        self._market = market
        self._code = code

    def build_request(self) -> bytes:
        body = struct.pack(
            "<H8s16x21s",
            self._market,
            self._code.encode("gbk"),
            b"Stock_GLHQ",
        )
        return build_mac_request(self.MSG_ID, body, head_flag=self.HEAD_FLAG)

    def parse_response(self, body: bytes) -> list[BelongBoardInfo]:
        if len(body) < 27:
            return []
        python_list: list[list[object]] = json.loads(body[27:].decode("gbk", errors="replace"))
        results: list[BelongBoardInfo] = []
        for row in python_list:
            n = len(row)
            if n not in (9, 13):
                continue
            results.append(
                BelongBoardInfo(
                    board_type=_to_int(row[0]),
                    market=_to_int(row[1]),
                    board_code=str(row[2]),
                    board_name=str(row[3]),
                    close=_to_float(row[4]) if n > 4 and row[4] else 0.0,
                    pre_close=_to_float(row[5]) if n > 5 and row[5] else 0.0,
                )
            )
        return results


class SymbolCapitalFlowCmd(BaseCommand[CapitalFlowData | None]):
    """查询个股资金流向（0x1218，``head_flag=2``）。"""

    MSG_ID = 0x1218
    HEAD_FLAG = 2

    def __init__(self, market: int, code: str) -> None:
        self._market = market
        self._code = code

    def build_request(self) -> bytes:
        body = struct.pack(
            "<H8s16x21s",
            self._market,
            self._code.encode("gbk"),
            b"Stock_ZJLX",
        )
        return build_mac_request(self.MSG_ID, body, head_flag=self.HEAD_FLAG)

    def parse_response(self, body: bytes) -> CapitalFlowData | None:
        if len(body) < 27:
            return None
        python_list: list[list[object]] = json.loads(body[27:].decode("gbk"))
        if len(python_list) < 2:
            return None

        today_data = python_list[0]
        five_days_data = python_list[1]

        main_in = _to_float(today_data[0]) if len(today_data) > 0 else 0.0
        main_out = _to_float(today_data[1]) if len(today_data) > 1 else 0.0
        retail_in = _to_float(today_data[2]) if len(today_data) > 2 else 0.0
        retail_out = _to_float(today_data[3]) if len(today_data) > 3 else 0.0
        mid_net_5d = _to_float(five_days_data[4]) if len(five_days_data) > 4 else 0.0
        large_net_5d = _to_float(five_days_data[3]) if len(five_days_data) > 3 else 0.0

        return CapitalFlowData(
            date="",
            main_in=main_in,
            main_out=main_out,
            main_net=main_in - main_out,
            small_in=retail_in,
            small_out=retail_out,
            small_net=retail_in - retail_out,
            mid_in=0.0,
            mid_out=0.0,
            mid_net=mid_net_5d,
            large_in=0.0,
            large_out=0.0,
            large_net=large_net_5d,
        )


# ============================================================
# 集合竞价 / 异动 / 服务器信息 / K线偏移：0x123D / 0x1237 / 0x120F / 0x124A
# ============================================================


class SymbolAuctionCmd(BaseCommand[list[AuctionItem]]):
    """查询集合竞价数据（0x123D）。"""

    MSG_ID = 0x123D

    def __init__(self, market: int, code: str, start: int = 0, count: int = 500) -> None:
        self._market = market
        self._code = code
        self._start = start
        self._count = count

    def build_request(self) -> bytes:
        body = struct.pack(
            "<H22sII10x",
            self._market,
            self._code.encode("gbk"),
            self._start,
            self._count,
        )
        return build_mac_request(self.MSG_ID, body)

    def parse_response(self, body: bytes) -> list[AuctionItem]:
        (_market, _code, count) = unpack_from("<H22sI", body, 0, "auction header")
        items: list[AuctionItem] = []
        for i in range(count):
            offset = 36 + i * 16
            if offset + 16 > len(body):
                break
            (time_sec, price, matched, unmatched) = unpack_from(
                "<IfIi", body, offset, f"auction item[{i}]"
            )
            items.append(
                AuctionItem(
                    time=time(time_sec // 3600, (time_sec % 3600) // 60, time_sec % 60),
                    price=price,
                    matched=matched,
                    unmatched=unmatched,
                )
            )
        return items


def _describe_unusual(unusual_type: int, data: bytes) -> tuple[str, str]:
    """根据异动类型解析描述和数值。"""

    if len(data) < 13:
        return "", ""
    v1, v2, v3, v4 = struct.unpack_from("<B2fI", data)

    if unusual_type == 0x03:
        desc = f"主力{'买入' if v1 == 0x00 else '卖出'}"
        val = f"{v2:.2f}/{v3:.2f}"
    elif unusual_type == 0x04:
        desc = "加速拉升"
        val = f"{v2 * 100:.2f}%"
    elif unusual_type == 0x05:
        desc = "加速下跌"
        val = ""
    elif unusual_type == 0x06:
        desc = "低位反弹"
        val = f"{v2 * 100:.2f}%"
    elif unusual_type == 0x07:
        desc = "高位回落"
        val = f"{v2 * 100:.2f}%"
    elif unusual_type == 0x08:
        desc = "撑杆跳高"
        val = f"{v2 * 100:.2f}%"
    elif unusual_type == 0x09:
        desc = "平台跳水"
        val = f"{v2 * 100:.2f}%"
    elif unusual_type == 0x0A:
        desc = f"单笔冲{'跌' if v2 < 0 else '涨'}"
        val = f"{v2 * 100:.2f}%"
    elif unusual_type == 0x0B:
        direction = "平" if v3 == 0 else "跌" if v3 < 0 else "涨"
        desc = f"区间放量{direction}"
        val = f"{v2:.1f}倍" + ("" if v3 == 0 else f"{v3 * 100:.2f}%")
    elif unusual_type == 0x0C:
        desc = "区间缩量"
        val = ""
    elif unusual_type == 0x10:
        desc = "大单托盘"
        val = f"{v4:.2f}/{v3:.2f}"
    elif unusual_type == 0x11:
        desc = "大单压盘"
        val = f"{v2:.2f}/{v3:.2f}"
    elif unusual_type == 0x12:
        desc = "大单锁盘"
        val = ""
    elif unusual_type == 0x13:
        desc = "竞价试买"
        val = f"{v2:.2f}/{v3:.2f}"
    elif unusual_type == 0x14:
        direction = "涨" if v1 == 0x00 else "跌"
        if len(data) >= 10:
            sub_type, v2_alt, v3_alt = struct.unpack_from("<Bff", data, 1)
        else:
            sub_type, v2_alt, v3_alt = 0, 0.0, 0.0
        if sub_type == 0x01:
            desc = f"逼近{direction}停"
        elif sub_type == 0x02:
            desc = f"封{direction}停板"
        elif sub_type == 0x04:
            desc = f"封{direction}大减"
        elif sub_type == 0x05:
            desc = f"打开{direction}停"
        else:
            desc = f"涨跌停({direction})"
        val = f"{v2_alt:.2f}/{v3_alt:.2f}"
    else:
        desc = f"异动类型{unusual_type:#04x}"
        val = ""

    return desc, val


class UnusualCmd(BaseCommand[list[UnusualItem]]):
    """查询异动数据（0x1237，最大 600 条）。"""

    MSG_ID = 0x1237

    def __init__(self, market: int, start: int = 0, count: int = 600) -> None:
        self._market = market
        self._start = start
        self._count = min(count, 600)

    def build_request(self) -> bytes:
        body = struct.pack(
            "<HH2xH2xH5H",
            self._market,
            self._start,
            self._count,
            1,
            200,
            30,
            40,
            50,
            200,
        )
        return build_mac_request(self.MSG_ID, body)

    def parse_response(self, body: bytes) -> list[UnusualItem]:
        (count,) = unpack_from("<H", body, 0, "unusual count")

        results: list[UnusualItem] = []
        for i in range(count):
            offset = 2 + i * 32
            if offset + 32 > len(body):
                break
            market, code_raw, _z0, unusual_type, _z1, index, _z2 = unpack_from(
                "<H6sBBBHH", body, offset, f"unusual record[{i}]"
            )
            desc, value = _describe_unusual(unusual_type, body[offset + 15 : offset + 28])
            hour, minute_sec = unpack_from("<BH", body, offset + 29, f"unusual time[{i}]")
            results.append(
                UnusualItem(
                    index=index,
                    market=market,
                    code=code_raw.decode("gbk", errors="replace").rstrip("\x00"),
                    name="",
                    time=time(hour, minute_sec // 100, minute_sec % 100),
                    desc=desc,
                    value=value,
                    unusual_type=unusual_type,
                )
            )

        binary_length = 2 + count * 32
        text_bytes = body[binary_length:]
        text_list = text_bytes.decode("gbk", errors="ignore").strip(",").split(",")

        populated: list[UnusualItem] = []
        for i, item in enumerate(results):
            name = text_list[i] if i < len(text_list) else ""
            populated.append(
                UnusualItem(
                    index=item.index,
                    market=item.market,
                    code=item.code,
                    name=name,
                    time=item.time,
                    desc=item.desc,
                    value=item.value,
                    unusual_type=item.unusual_type,
                )
            )
        return populated


class ServerInfoCmd(BaseCommand[ServerSession]):
    """查询服务器交易时段信息（0x120F）。"""

    MSG_ID = 0x120F

    def build_request(self) -> bytes:
        header = bytes.fromhex("04002d31")
        body = header + b"\x00" * 8 + b"\x00\x27\x06\x0e" + b"\x00" * 52
        return build_mac_request(self.MSG_ID, body)

    def parse_response(self, body: bytes) -> ServerSession:
        if len(body) < 87:
            return ServerSession(today="", last_trading_day="")

        pos = 0
        _count = unpack_from("<H", body, pos, "server_info count")[0]
        pos += 2 + 8 + 3 + 9  # count + flags + tag + reserved

        def _parse_date(p: int) -> tuple[str, int]:
            d = unpack_from("<I", body, p, "server_info date")[0]
            return f"{d // 10000}-{d % 10000 // 100:02d}-{d % 100:02d}", p + 4

        def _parse_session(p: int) -> tuple[list[dict[str, object]], int]:
            vals = unpack_from("<8H", body, p, "server_info session")
            sessions: list[dict[str, object]] = []
            for i in range(0, 8, 2):
                sessions.append(
                    {
                        "open": f"{vals[i] // 60}:{vals[i] % 60:02d}",
                        "close": f"{vals[i + 1] // 60}:{vals[i + 1] % 60:02d}",
                    }
                )
            return sessions, p + 16

        today, pos = _parse_date(pos)
        pos += 4  # ts1
        sessions_1, pos = _parse_session(pos)
        sessions_2, pos = _parse_session(pos)
        pos += 1  # flag byte
        last_trading_day, pos = _parse_date(pos)
        pos += 4  # ts2

        market_param_1 = 0
        market_param_2 = 0
        if pos + 8 <= len(body):
            market_param_1 = unpack_from("<I", body, pos, "server_info param1")[0]
            pos += 4
            market_param_2 = unpack_from("<I", body, pos, "server_info param2")[0]

        return ServerSession(
            today=today,
            last_trading_day=last_trading_day,
            sessions_1=sessions_1,
            sessions_2=sessions_2,
            market_param_1=market_param_1,
            market_param_2=market_param_2,
        )


class KlineOffsetCmd(BaseCommand[KlineOffsetInfo]):
    """查询 K 线数据偏移（0x124A）。"""

    MSG_ID = 0x124A

    def __init__(self, offset: int = 0, count: int = 128000) -> None:
        self._offset = offset
        self._count = count

    def build_request(self) -> bytes:
        body = struct.pack("<II5x", self._offset, self._count)
        return build_mac_request(self.MSG_ID, body)

    def parse_response(self, body: bytes) -> KlineOffsetInfo:
        if len(body) < 8:
            return KlineOffsetInfo(total=0, returned=0)
        total = struct.unpack(">I", body[:4])[0]  # total 字段为大端序
        returned = struct.unpack("<I", body[4:8])[0]
        return KlineOffsetInfo(total=total, returned=returned)


# ============================================================
# 缩略采样 / 商品列表 / 文件：0x254D / 0x2562 / 0x1215 / 0x1217
# ============================================================


class ChartSamplingCmd(BaseCommand[list[float]]):
    """获取分时缩略采样价格点（0x254D，返回 240 个 float）。"""

    MSG_ID = 0x254D
    _CODE_LEN = 22
    _RESPONSE_HEADER_SIZE = 42  # H(2) + 22s(22) + 9*H(18)

    def __init__(self, market: int, code: str) -> None:
        self.market = market
        self.code = code

    def build_request(self) -> bytes:
        raw_code = self.code.encode("gbk")
        padded = (raw_code + b"\x00" * self._CODE_LEN)[: self._CODE_LEN]
        body = struct.pack("<H22sHH9x", self.market, padded, 1, 20)
        return build_mac_request(self.MSG_ID, body)

    def parse_response(self, body: bytes) -> list[float]:
        if len(body) < self._RESPONSE_HEADER_SIZE:
            return []
        require_bytes(body, 0, self._RESPONSE_HEADER_SIZE, "ChartSamplingCmd header")
        (count,) = unpack_from("<H", body, 40, "chart_sampling count")
        prices: list[float] = []
        for i in range(count):
            pos = self._RESPONSE_HEADER_SIZE + i * 4
            require_bytes(body, pos, 4, f"ChartSamplingCmd price[{i}]")
            (price,) = unpack_from("<f", body, pos, f"chart_sampling price[{i}]")
            prices.append(price)
        return prices


class GoodsListCmd(BaseCommand[list[GoodsItem]]):
    """获取扩展市场（期货/期权等）商品列表（0x2562）。"""

    MSG_ID = 0x2562
    MAX_COUNT = 1000
    _RECORD_SIZE = 48
    _RECORD_FMT = "<H23sHIBfffHH"

    def __init__(self, market: int, start: int = 0, count: int = 600) -> None:
        if count > self.MAX_COUNT:
            raise ValueError(f"count 不能超过 {self.MAX_COUNT}，当前: {count}")
        self.market = market
        self.start = start
        self.count = count
        self.total: int = 0

    def build_request(self) -> bytes:
        body = struct.pack("<HII", self.market, self.start, self.count)
        return build_mac_request(self.MSG_ID, body)

    def parse_response(self, body: bytes) -> list[GoodsItem]:
        require_bytes(body, 0, 2, "GoodsListCmd header")
        (total,) = unpack_from("<H", body, 0, "GoodsListCmd total")
        self.total = total
        items: list[GoodsItem] = []
        for i in range(total):
            offset = 2 + i * self._RECORD_SIZE
            require_bytes(body, offset, self._RECORD_SIZE, f"GoodsListCmd record[{i}]")
            category, raw_name, u, index, switch, v1, v2, v3, c1, c2 = unpack_from(
                self._RECORD_FMT, body, offset, f"GoodsListCmd record[{i}]"
            )
            items.append(
                GoodsItem(
                    name=raw_name.decode("gbk", errors="replace").rstrip("\x00"),
                    category=category,
                    u=u,
                    index=index,
                    switch=switch,
                    code=[v1, v2, v3],
                    c1=c1,
                    c2=c2,
                )
            )
        return items


_FILENAME_LEN = 70
_FILENAME_PAD = 30


class FileListCmd(BaseCommand[FileMeta]):
    """查询远程文件元信息（0x1215）。"""

    MSG_ID = 0x1215

    def __init__(self, filename: str, offset: int = 0) -> None:
        self.filename = filename
        self.offset = offset

    def build_request(self) -> bytes:
        raw_name = self.filename.encode("gbk")
        padded = (raw_name + b"\x00" * _FILENAME_LEN)[:_FILENAME_LEN]
        body = struct.pack("<I", self.offset) + padded + b"\x00" * _FILENAME_PAD
        return build_mac_request(self.MSG_ID, body)

    def parse_response(self, body: bytes) -> FileMeta:
        require_bytes(body, 0, 4 + 4 + 1 + 32, "FileListCmd")
        offset, size, flag = unpack_from("<IIb", body, 0, "FileListCmd meta")
        hash_str = body[9:41].decode("ascii", errors="replace").rstrip("\x00")
        return FileMeta(offset=offset, size=size, flag=flag, hash=hash_str)


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
        self.filename = filename
        self.index = index
        self.offset = offset
        self.size = size

    def build_request(self) -> bytes:
        raw_name = self.filename.encode("gbk")
        padded = (raw_name + b"\x00" * _FILENAME_LEN)[:_FILENAME_LEN]
        body = (
            struct.pack("<III", self.index, self.offset, self.size)
            + padded
            + b"\x00" * _FILENAME_PAD
        )
        return build_mac_request(self.MSG_ID, body)

    def parse_response(self, body: bytes) -> bytes:
        if len(body) < 8:
            return b""
        return body[8:]


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