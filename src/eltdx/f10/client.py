"""Client for the 7615 TQLEX / F10 HTTP gateway."""

from __future__ import annotations

import json
from collections.abc import Mapping, Sequence
from datetime import date, datetime
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode
from urllib.request import Request, urlopen

from eltdx.exceptions import ProtocolError, TransportError
from eltdx.protocol.unit import split_code

from .models import (
    F10Cell,
    F10Response,
    F10ResultSet,
    LimitBoardLadder,
    LimitBoardLadderRow,
)

DEFAULT_TQLEX_BASE_URL = "http://static.tdx.com.cn:7615/TQLEX"
DEFAULT_LIMIT_BOARD_LADDER_BASE_URL = "http://hot.icfqs.com:7615/TQLEX"
DEFAULT_QSID = "tdx"
LIMIT_BOARD_LADDER_ENTRY = "CWServ.cfg_fx_lbtt"


class F10Client:
    """High-level wrapper around the 7615 F10 / TQLEX gateway.

    The gateway has one physical HTTP endpoint and many logical Entry names.
    ``call`` exposes the raw Entry model; the other methods are thin product
    wrappers that fill the Entry name and parameter order for common pages.
    """

    def __init__(
        self,
        *,
        base_url: str = DEFAULT_TQLEX_BASE_URL,
        limit_board_ladder_base_url: str | None = None,
        timeout: float = 8.0,
        headers: Mapping[str, str] | None = None,
    ) -> None:
        self.base_url = base_url
        self.limit_board_ladder_base_url = (
            DEFAULT_LIMIT_BOARD_LADDER_BASE_URL
            if limit_board_ladder_base_url is None and base_url == DEFAULT_TQLEX_BASE_URL
            else (limit_board_ladder_base_url or base_url)
        )
        self.timeout = timeout
        self.headers = {
            "Content-Type": "application/json",
            "Accept": "application/json",
            "User-Agent": "eltdx/1.0",
        }
        if headers:
            self.headers.update(headers)

    def call(self, entry: str, body: Any | None = None, *, params: Sequence[Any] | None = None) -> F10Response:
        """Call any TQLEX Entry.

        Pass either an explicit JSON body or ``params=[...]``. ``params`` is
        encoded as ``{"Params": [...]}``, matching the CWServ convention.
        """

        if body is not None and params is not None:
            raise ValueError("pass either body or params, not both")
        request_body = {"Params": list(params)} if params is not None else (body if body is not None else {})
        return self._post(entry, request_body)

    def params(self, entry: str, *params: Any) -> F10Response:
        """Call a CWServ Entry with a Params array."""

        return self.call(entry, params=params)

    def limit_up_down_list(
        self,
        start_date: str | int | date | None = None,
        end_date: str | int | date | None = None,
        *,
        include_summary: bool = False,
    ) -> LimitBoardLadder:
        """Query the stock-level limit-up, broken-board and limit-down list.

        The page uses the dedicated ``CWServ.cfg_fx_lbtt`` 7615 Entry.  The
        detail request is ``Params=["1", start, end]``; when requested, the
        market overview is fetched with ``Params=["2", start, end]``.
        """

        selected_start, selected_end = _normalize_limit_board_dates(
            start_date, end_date
        )
        body = {"Params": ["1", selected_start, selected_end]}
        response = self._post(LIMIT_BOARD_LADDER_ENTRY, body)
        summary_rows: tuple[dict[str, Any], ...] = ()
        if include_summary:
            summary = self._post(
                LIMIT_BOARD_LADDER_ENTRY,
                {"Params": ["2", selected_start, selected_end]},
            )
            summary_rows = summary.rows

        rows = tuple(_parse_limit_board_ladder_row(row) for row in response.rows)
        return LimitBoardLadder(
            entry=response.entry,
            request_body=body,
            error_code=response.error_code,
            start_date=selected_start,
            end_date=selected_end,
            rows=rows,
            summary=summary_rows,
            raw=response.raw,
        )

    def limit_board_ladder(
        self,
        start_date: str | int | date | None = None,
        end_date: str | int | date | None = None,
        *,
        include_summary: bool = False,
    ) -> LimitBoardLadder:
        """Compatibility alias for :meth:`limit_up_down_list`."""

        return self.limit_up_down_list(
            start_date,
            end_date,
            include_summary=include_summary,
        )

    def stock_info(self, code: str) -> F10Response:
        """股票基础信息，通常用于页面初始化。"""

        return self.params("CWServ.tdxf10_gg_comreq", "gpquery", _code6(code))

    def business_periods(self, code: str) -> F10Response:
        """主营构成可选报告期。"""

        return self.params("CWServ.tdxf10_gg_comreq", "zygcfx", _code6(code))

    def topic_ids(self, code: str) -> F10Response:
        """股票关联题材 ID 列表。"""

        return self.params("CWServ.tdxf10_gg_comreq", "rdtcgn", _code6(code))

    def company_profile(self, code: str, section: str = "8") -> F10Response:
        """公司概况。section=8 为发行上市信息，section=9 为指数调入调出。"""

        return self.params("CWServ.tdxf10_gg_gsgk", section, _code6(code), "")

    def business_composition(self, code: str, report_date: str | None = None) -> F10Response:
        """主营构成。report_date 不传时先取服务端给出的最新报告期。"""

        code6 = _code6(code)
        selected_date = report_date or _first_value(self.business_periods(code6), "T002")
        if selected_date is None:
            raise ProtocolError(f"no business composition report date for {code6}")
        return self.params("CWServ.tdxf10_gg_jyfx", code6, "zygc", str(selected_date))

    def shareholder_change_plans(
        self,
        code: str,
        *,
        page: int = 1,
        page_size: int = 20,
        filter1: str = "",
        filter2: str = "",
    ) -> F10Response:
        """股东增减持计划。"""

        return self.params(
            "CWServ.tdxf10_gg_gdyj",
            _code6(code),
            "gdzjcjh",
            filter1,
            filter2,
            str(page),
            str(page),
            str(page_size),
        )

    def dividend_financing(self, code: str, section: str = "fh") -> F10Response:
        """分红融资类数据。section=fh 为分红方案历史。"""

        return self.params("CWServ.tdxf10_gg_fhrz", _code6(code), section)

    def allotment_dates(self, code: str) -> F10Response:
        """增发获配明细可选日期。"""

        return self.params("CWServ.tdxf10_gg_fhrz_zfhpmx", "zfpg_bgq", _code6(code), "")

    def allotment_details(self, code: str, date: str) -> F10Response:
        """指定增发日期的获配机构明细。"""

        return self.params("CWServ.tdxf10_gg_fhrz_zfhpmx", "zfpg", _code6(code), date)

    def finance_report(self, code: str, report_type: str = "zcfzb") -> F10Response:
        """财务报表。report_type=zcfzb 为资产负债表。"""

        return self.params("CWServ.tdxf10_gg_cwfx", _code6(code), report_type, "")

    def finance_diagnosis(self, code: str, section: str = "yynl", scope: str = "") -> F10Response:
        """财务诊断。"""

        return self.params("CWServ.tdxf10_gg_cwzd", section, _code6(code), scope)

    def stock_score(self, code: str, section: str = "pf", arg: str = "") -> F10Response:
        """个股总评。section=pf 为综合评分。"""

        return self.params("CWServ.tdxf10_gg_ggzp", section, _code6(code), arg, "")

    def profit_forecast(self, code: str) -> F10Response:
        """盈利预测评级统计。"""

        return self.params("CWServ.tdxf10_gg_ybpj", _code6(code), "ylyctj")

    def ranking_detail(self, code: str, section: str = "scpmdela") -> F10Response:
        """市场排名 / 行业排名明细。"""

        return self.params("CWServ.tdxf10_gg_zxts_rqpm", _code6(code), section)

    def governance(self, code: str, section: str = "wgcl", arg: str = "") -> F10Response:
        """资本运作治理。section=wgcl 为违规处理，dbmx 为担保明细。"""

        return self.params("CWServ.tdxf10_gg_zbyz", section, _code6(code), arg)

    def hot_topics(self, code: str, section: str = "zttzbkz") -> F10Response:
        """热点题材。section=zttzbkz 为板块题材。"""

        return self.params("CWServ.tdxf10_gg_rdtc", _code6(code), section)

    def topic_compare(self, code: str, topic_id: str, section: str = "gndbzfsj", sort_by: str = "zdf") -> F10Response:
        """题材内对比排名。"""

        return self.params("CWServ.tdxf10_gg_rdtc_gndb", section, _code6(code), str(topic_id), sort_by)

    def topic_compare_first(self, code: str, section: str = "gndbzfsj", sort_by: str = "zdf") -> F10Response:
        """用股票第一个题材 ID 查询题材内对比排名。"""

        topic_id = _first_value(self.topic_ids(code), "t001")
        if topic_id is None:
            raise ProtocolError(f"no topic id for {_code6(code)}")
        return self.topic_compare(code, str(topic_id), section=section, sort_by=sort_by)

    def company_news(
        self,
        code: str,
        section: str = "gsyj",
        *,
        keyword: str = "",
        rating: str | int = "0",
        page: int = 1,
        page_size: int = 20,
    ) -> F10Response:
        """公司资讯。section=gsyj 为研报，jgcs 为监管措施。"""

        return self.params("CWServ.tdxf10_gg_gszx", _code6(code), section, keyword, str(rating), str(page), str(page_size))

    def northbound_holding(
        self,
        code: str,
        section: str = "bszj",
        *,
        filter_value: str = "",
        page: int = 1,
        page_size: int = 20,
    ) -> F10Response:
        """沪深股通持股变化。"""

        return self.params(
            "CWServ.tdxf10_gg_zlcc",
            _code6(code),
            section,
            filter_value,
            str(page),
            str(page),
            str(page_size),
        )

    def detail(self, detail_type: str, record_id: str | int) -> F10Response:
        """按记录 ID 查询详情正文。"""

        return self.params("CWServ.tdxf10_gg_idreq", detail_type, str(record_id))

    def cache_list(self, code: str, kind: str = "gg") -> F10Response:
        """新闻 / 公告 / 路演缓存列表。kind=xw、gg、ly。"""

        if kind not in {"xw", "gg", "ly"}:
            raise ValueError("kind must be one of: xw, gg, ly")
        market_id, _, code6 = split_code(code)
        body = {"action": "get", "key": f"{kind}:{market_id}_{code6}", "bin": "1", "qsid": DEFAULT_QSID}
        return self.call("CWSearch.tzx_rcache", body)

    def announcements(self, code: str) -> F10Response:
        """公告列表。"""

        return self.cache_list(code, "gg")

    def news(self, code: str) -> F10Response:
        """新闻列表。"""

        return self.cache_list(code, "xw")

    def roadshows(self, code: str) -> F10Response:
        """路演列表。"""

        return self.cache_list(code, "ly")

    def theme_market(
        self,
        code: str,
        req_id: str | int = "200743",
        *,
        page: int = -1,
        page_size: int = 10,
        zq_num: int | str = 30,
        extra: Mapping[str, Any] | None = None,
    ) -> F10Response:
        """题材概念行情。req_id=200743 为相关板块。"""

        payload = {
            "ReqId": str(req_id),
            "setcode": split_code(code)[0],
            "code": _code6(code),
            "Page": page,
            "PageSize": str(page_size),
            "modname": "mod_tcihq.dll",
        }
        if str(req_id) in {"200742", "200745", "200747"}:
            payload["zq_num"] = str(zq_num)
        if extra:
            payload.update(extra)
        return self.call("HQServ.hq_nlp_tcihq", [payload])

    def valuation(
        self,
        code: str,
        req_id: str | int = "200191",
        *,
        page: int = 0,
        page_size: int = 20,
        extra: Mapping[str, Any] | None = None,
    ) -> F10Response:
        """估值市场数据。req_id=200191 为估值表。"""

        market_id, _, code6 = split_code(code)
        req_id_text = str(req_id)
        payload: dict[str, Any]
        if req_id_text in {"200191", "200192"}:
            payload = {
                "ReqId": req_id_text,
                "Code": f"{code6}|{market_id}",
                "BeginDate": "0",
                "EndDate": "0",
                "Page": str(page),
                "PageSize": str(page_size),
                "modname": "mod_gpsj.dll",
            }
            if req_id_text == "200192":
                payload.setdefault("Type", "0")
                payload["Page"] = str(page if page != 0 else -1)
                payload["PageSize"] = str(page_size if page_size != 20 else 5)
        elif req_id_text == "200135":
            payload = {
                "ReqId": req_id_text,
                "code": code6,
                "setcode": str(market_id),
                "modname": "mod_gpsj.dll",
            }
        elif req_id_text == "200124":
            payload = {
                "ReqId": req_id_text,
                "code": code6,
                "setcode": str(market_id),
                "sjfw": "1Y",
                "sdate": "",
                "edate": "",
                "zb": "0",
                "hqzq": "0",
                "cwppgz": 1,
                "jglx": 0,
                "Page": page if page != 0 else 1,
                "PageSize": str(page_size if page_size != 20 else 300),
                "modname": "mod_gpsj.dll",
            }
        else:
            payload = {
                "ReqId": req_id_text,
                "code": code6,
                "setcode": str(market_id),
                "modname": "mod_gpsj.dll",
            }
        if extra:
            payload.update(extra)
        return self.call("HQServ.hq_nlp_gpsj", [payload])

    def _post(self, entry: str, body: Any) -> F10Response:
        base_url = (
            self.limit_board_ladder_base_url
            if entry == LIMIT_BOARD_LADDER_ENTRY
            else self.base_url
        )
        url = f"{base_url}?{urlencode({'Entry': entry})}"
        data = json.dumps(body, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
        request = Request(url, data=data, headers=self.headers, method="POST")
        try:
            with urlopen(request, timeout=self.timeout) as response:
                raw_bytes = response.read()
        except HTTPError as exc:
            raise TransportError(f"TQLEX HTTP error {exc.code} for {entry}") from exc
        except URLError as exc:
            raise TransportError(f"TQLEX request failed for {entry}: {exc.reason}") from exc
        except TimeoutError as exc:
            raise TransportError(f"TQLEX request timed out for {entry}") from exc

        try:
            raw = json.loads(raw_bytes.decode("utf-8-sig"))
        except (UnicodeDecodeError, json.JSONDecodeError) as exc:
            raise ProtocolError(f"TQLEX returned invalid JSON for {entry}") from exc
        if not isinstance(raw, dict):
            raise ProtocolError(f"TQLEX returned non-object JSON for {entry}")
        return parse_tqlex_response(entry, body, raw)


def parse_tqlex_response(entry: str, request_body: Any, raw: Mapping[str, Any]) -> F10Response:
    """Parse a TQLEX JSON object into stable table models."""

    result_sets_raw = raw.get("ResultSets") or ()
    if not isinstance(result_sets_raw, Sequence) or isinstance(result_sets_raw, (str, bytes, bytearray)):
        raise ProtocolError("TQLEX ResultSets must be an array")

    result_sets = tuple(_parse_result_set(item, index) for index, item in enumerate(result_sets_raw))
    error_code_raw = raw.get("ErrorCode")
    error_code = int(error_code_raw) if error_code_raw is not None else None
    return F10Response(
        entry=entry,
        request_body=request_body,
        error_code=error_code,
        result_sets=result_sets,
        raw=dict(raw),
    )


def _parse_result_set(raw: Any, table_index: int) -> F10ResultSet:
    if not isinstance(raw, Mapping):
        raise ProtocolError("TQLEX ResultSet must be an object")
    columns = _columns(raw)
    content = raw.get("Content") or ()
    if not isinstance(content, Sequence) or isinstance(content, (str, bytes, bytearray)):
        raise ProtocolError("TQLEX ResultSet Content must be an array")

    rows: list[dict[str, Any]] = []
    row_cells: list[tuple[F10Cell, ...]] = []
    for row in content:
        values = _row_values(row)
        cells = tuple(F10Cell(name=_column_name(columns, index), value=value, index=index) for index, value in enumerate(values))
        row_cells.append(cells)
        rows.append(_row_dict(cells))

    return F10ResultSet(
        key=raw.get("ResultSetKey") or f"table{table_index}",
        columns=tuple(columns),
        rows=tuple(rows),
        row_cells=tuple(row_cells),
        raw=dict(raw),
    )


def _columns(raw: Mapping[str, Any]) -> list[str]:
    if "ColName" in raw:
        col_name = raw.get("ColName") or ()
        if not isinstance(col_name, Sequence) or isinstance(col_name, (str, bytes, bytearray)):
            raise ProtocolError("TQLEX ColName must be an array")
        return [str(item) for item in col_name]

    if "ColDes" in raw:
        col_des = raw.get("ColDes") or ()
        if not isinstance(col_des, Sequence) or isinstance(col_des, (str, bytes, bytearray)):
            raise ProtocolError("TQLEX ColDes must be an array")
        names = []
        for item in col_des:
            if isinstance(item, Mapping):
                names.append(str(item.get("Name", "")))
            else:
                names.append(str(item))
        return names

    return []


def _row_values(row: Any) -> list[Any]:
    if isinstance(row, Sequence) and not isinstance(row, (str, bytes, bytearray)):
        return list(row)
    return [row]


def _column_name(columns: Sequence[str], index: int) -> str:
    if index < len(columns) and columns[index]:
        return columns[index]
    return f"col_{index}"


def _row_dict(cells: Sequence[F10Cell]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    counts: dict[str, int] = {}
    for cell in cells:
        count = counts.get(cell.name, 0)
        counts[cell.name] = count + 1
        key = cell.name if count == 0 else f"{cell.name}__{count + 1}"
        result[key] = cell.value
    return result


def _code6(code: str) -> str:
    return split_code(code)[2]


def _first_value(response: F10Response, field_name: str) -> Any | None:
    for row in response.rows:
        if field_name in row:
            return row[field_name]
    return None


def _normalize_limit_board_dates(
    start_date: str | int | date | None,
    end_date: str | int | date | None,
) -> tuple[str, str]:
    """Normalize optional dates to the gateway's ``YYYYMMDD`` format."""

    start = _date_text(start_date)
    end = _date_text(end_date)
    if start is None and end is None:
        today = date.today().strftime("%Y%m%d")
        return today, today
    if start is None:
        start = end
    if end is None:
        end = start
    assert start is not None and end is not None
    if start > end:
        raise ValueError("start_date must be on or before end_date")
    return start, end


def _date_text(value: str | int | date | None) -> str | None:
    if value is None:
        return None
    if isinstance(value, datetime):
        value = value.date()
    if isinstance(value, date):
        return value.strftime("%Y%m%d")
    text = str(value).strip().replace("-", "").replace("/", "")
    if len(text) != 8 or not text.isdigit():
        raise ValueError(f"invalid date: {value!r}")
    try:
        datetime.strptime(text, "%Y%m%d")
    except ValueError as exc:
        raise ValueError(f"invalid date: {value!r}") from exc
    return text


def _parse_limit_board_ladder_row(row: Mapping[str, Any]) -> LimitBoardLadderRow:
    limit_type = _as_int(row.get("ztlb"))
    board_level = _as_int(row.get("lbts"))
    return LimitBoardLadderRow(
        trading_date=row.get("rq"),
        trading_date_value=row.get("rqex"),
        board_level=(
            board_level
            if limit_type == 1 and board_level is not None and board_level > 0
            else None
        ),
        code=_as_text(row.get("ZQDM")),
        market_id=_as_int(row.get("SC")),
        limit_reason=row.get("ztyy"),
        seal_amount=_as_number(row.get("fde")),
        name=row.get("ZQJC"),
        limit_reason_extra=row.get("ztyy2"),
        limit_time=row.get("ztsj"),
        broken_count=_as_int(row.get("kbcs")),
        industry=row.get("sshy"),
        limit_type=limit_type,
        success_rate=row.get("cgl"),
        highest_board_level=_as_int(row.get("zglb")),
        raw=dict(row),
    )


def _as_text(value: Any) -> str | None:
    return None if value is None else str(value)


def _as_int(value: Any) -> int | None:
    if value is None or value == "":
        return None
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


def _as_number(value: Any) -> float | int | None:
    if value is None or value == "":
        return None
    if isinstance(value, bool):
        return int(value)
    if isinstance(value, (int, float)):
        return value
    try:
        number = float(value)
    except (TypeError, ValueError):
        return None
    return int(number) if number.is_integer() else number
