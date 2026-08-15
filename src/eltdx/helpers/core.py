"""User-facing scenario helpers.

The helpers in this module intentionally compose existing APIs instead of
parsing protocol frames directly. Low-level command ownership stays in
``client.codes``, ``client.quotes``, ``client.bars`` and ``client.f10``.
"""

from __future__ import annotations

from collections.abc import Iterable, Mapping, Sequence
from dataclasses import dataclass, replace
from typing import TYPE_CHECKING, Any

from eltdx.equity import (
    apply_factors_to_kline,
    build_factor_response,
    compute_turnover,
    filter_equity_records,
    filter_xdxr_records,
    pick_equity,
)
from eltdx.models import QuoteRefreshRecord, QuoteSnapshot
from eltdx.protocol.unit import ID_TO_MARKET, normalize_code

from .shortline import (
    ShortlineIndicator as ShortlineIndicator,
    ShortlineIndicatorService,
    ShortlineIndicatorTable,
)

if TYPE_CHECKING:
    from eltdx.client import TdxClient


@dataclass(frozen=True, slots=True)
class StockProfile:
    full_code: str
    exchange: str
    market_id: int | None
    code: str
    name: str | None
    category: str | None
    board: str | None
    last_price: float | None
    pre_close_price: float | None
    open_price: float | None
    high_price: float | None
    low_price: float | None
    change: float | None
    change_pct: float | None
    volume_hand: int | None
    amount: float | None
    open_amount_yuan: float | None
    circulating_shares: float | None
    total_shares: float | None
    turnover_rate: float | None
    circulating_market_value: float | None
    total_market_value: float | None
    eps: float | None
    ipo_date: Any | None
    updated_date: Any | None
    security: Any | None = None
    quote: Any | None = None
    finance: Any | None = None


@dataclass(frozen=True, slots=True)
class StockProfileTable:
    codes: tuple[str, ...]
    rows: tuple[StockProfile, ...]

    @property
    def count(self) -> int:
        return len(self.rows)


@dataclass(frozen=True, slots=True)
class StockTopic:
    topic_id: str | None
    topic_name: str | None
    relation_level: float | None
    selected_date: Any | None
    topic_date: Any | None
    reason: str | None
    detail_id: str | None
    category_raw: Any | None
    source: str
    raw: dict[str, Any]


@dataclass(frozen=True, slots=True)
class StockTopics:
    code: str
    topics: tuple[StockTopic, ...]

    @property
    def count(self) -> int:
        return len(self.topics)


@dataclass(frozen=True, slots=True)
class TopicStock:
    rank: int | None
    full_code: str | None
    exchange: str | None
    market_id: int | None
    code: str | None
    name: str | None
    change_pct: float | None
    change_pct_3d: float | None
    change_pct_5d: float | None
    change_pct_20d: float | None
    change_pct_60d: float | None
    change_pct_ytd: float | None
    trading_date: Any | None
    raw: dict[str, Any]


@dataclass(frozen=True, slots=True)
class TopicStockTable:
    seed_code: str
    topic_id: str | None
    topic_name: str | None
    sort_by: str
    rows: tuple[TopicStock, ...]

    @property
    def count(self) -> int:
        return len(self.rows)


@dataclass(frozen=True, slots=True)
class AuctionData:
    code: str
    trading_date: Any
    series: Any | None
    auction_records: tuple[Any, ...]
    snapshot_0925: Any | None
    pre_close_price: float | None
    open_price: float | None
    open_volume: int | None
    open_amount: float | None
    open_change_pct: float | None


class HelperApi:
    """Practical helpers for common user questions."""

    def __init__(self, client: TdxClient) -> None:
        self._client = client
        self._shortline = ShortlineIndicatorService(client)
        self._capital_cache: dict[str, Any] = {}
        self._finance_cache: dict[tuple[str, ...], Any] = {}

    def clear_cache(self) -> None:
        self._shortline.clear_cache()
        self._capital_cache.clear()
        self._finance_cache.clear()

    def full_quotes(self, codes: str | Sequence[str]):
        """Return complete quotes by combining ``0x054c`` snapshots with ``0x0547`` depth."""
        full_codes = _code_list(codes)
        if not full_codes:
            return []
        results = []
        for start in range(0, len(full_codes), 80):
            batch = full_codes[start : start + 80]
            page = self._client.quotes.get_snapshots(batch)
            if isinstance(page, (list, tuple)):
                results.extend(self._merge_quote_depths(list(page), batch))
            else:
                return page
        return results

    def _merge_quote_depths(self, snapshots: list[Any], codes: Sequence[str]) -> list[Any]:
        """Merge a 0x0547 refresh page into 0x054c snapshots when five levels exist."""
        if not snapshots or not all(isinstance(item, QuoteSnapshot) for item in snapshots):
            return snapshots
        try:
            refresh = self._client.quotes.get_depth(codes)
        except Exception:
            return snapshots
        records = getattr(refresh, "records", ())
        depth_by_code = {
            record.full_code: record
            for record in records
            if isinstance(record, QuoteRefreshRecord)
            and len(record.buy_levels) >= 5
            and len(record.sell_levels) >= 5
        }
        if not depth_by_code:
            return snapshots
        merged: list[Any] = []
        for snapshot in snapshots:
            depth = depth_by_code.get(snapshot.full_code)
            if depth is None:
                merged.append(snapshot)
                continue
            merged.append(
                replace(
                    snapshot,
                    buy_levels=depth.buy_levels,
                    sell_levels=depth.sell_levels,
                    open_amount_raw=depth.open_amount_raw,
                    open_amount_yuan=depth.open_amount_yuan,
                )
            )
        return merged

    def capital_changes(self, code: str, *, include_raw: bool = False, refresh: bool = False):
        full_code = normalize_code(code)
        if not include_raw and not refresh and full_code in self._capital_cache:
            return self._capital_cache[full_code]
        result = self._client.corporate.capital_changes(full_code, include_raw=include_raw)
        if not include_raw:
            self._capital_cache[full_code] = result
        return result

    def xdxr(self, code: str, *, refresh: bool = False):
        return filter_xdxr_records(self.capital_changes(code, refresh=refresh))

    def equity_changes(self, code: str, *, refresh: bool = False):
        return filter_equity_records(self.capital_changes(code, refresh=refresh))

    def equity(self, code: str, on=None, *, refresh: bool = False):
        return pick_equity(self.equity_changes(code, refresh=refresh).items, on)

    def turnover(self, code: str, volume: int | float, *, on=None, unit: str = "hand", refresh: bool = False) -> float:
        return compute_turnover(self.equity(code, on=on, refresh=refresh), volume, unit=unit)

    def factors(self, code: str, *, anchor_date=None, refresh: bool = False):
        return build_factor_response(
            self._client.bars.all(code, period="day", adjust="none"),
            self.xdxr(code, refresh=refresh),
            anchor_date=anchor_date,
        )

    def local_adjusted_kline(
        self,
        code: str,
        *,
        period: str = "day",
        adjust: str = "qfq",
        anchor_date=None,
        refresh: bool = False,
    ):
        period_key = str(period).strip().lower()
        is_daily_period = period_key in {"day", "1d", "d", "daily"} or (
            isinstance(period, tuple) and len(period) == 2 and tuple(map(int, period)) == (4, 1)
        )
        if not is_daily_period:
            raise ValueError("local_adjusted_kline only supports daily K-lines; use client.bars for other periods")
        adjust_key = str(adjust).strip().lower()
        qfq_modes = {"qfq", "front", "forward", "pre"}
        hfq_modes = {"hfq", "back", "backward", "post"}
        if adjust_key not in qfq_modes | hfq_modes:
            raise ValueError(f"invalid adjust mode: {adjust!r}")
        if anchor_date not in (None, "") and adjust_key in hfq_modes:
            raise ValueError("anchor_date is only supported for qfq")

        base = self._client.bars.all(code, period="day", adjust="none")
        factors = build_factor_response(
            base,
            self.xdxr(code, refresh=refresh),
            anchor_date=anchor_date,
        )
        return apply_factors_to_kline(base, factors, adjust=adjust, anchor_date=anchor_date)

    def shortline_indicators(
        self,
        codes: str | Sequence[str],
        *,
        stats_path: str = "zhb.zip",
        refresh_stats: bool = False,
    ) -> ShortlineIndicatorTable:
        """Return 21 shortline fields with trading-date-safe stats alignment."""

        return self._shortline.get(
            codes,
            stats_path=stats_path,
            refresh_stats=refresh_stats,
        )

    def stock_profile_table(
        self,
        codes: str | Sequence[str],
        *,
        include_security: bool = True,
        include_finance: bool = True,
    ) -> StockProfileTable:
        """Return quote, code-table and finance fields as one table."""

        full_codes = _code_list(codes)
        quote_map = _by_full_code(self.full_quotes(full_codes))
        security_map = self._security_map(full_codes) if include_security else {}
        finance_map = self._finance_map(full_codes) if include_finance else {}
        rows = tuple(
            _build_stock_profile(
                full_code,
                quote_map.get(full_code),
                security_map.get(full_code),
                finance_map.get(full_code),
            )
            for full_code in full_codes
        )
        return StockProfileTable(codes=tuple(full_codes), rows=rows)

    def stock_topics(self, code: str) -> StockTopics:
        """Return all topics for one stock, merging topic IDs and details."""

        full_code = normalize_code(code)
        topic_rows = list(getattr(self._client.f10.topic_ids(full_code), "rows", ()))
        hot_rows = list(getattr(self._client.f10.hot_topics(full_code), "rows", ()))

        merged: dict[str, dict[str, Any]] = {}
        order: list[str] = []

        for row in topic_rows:
            topic_id = _text(_pick(row, "t001", "id", "topic_id"))
            topic_name = _text(_pick(row, "t002", "ztmc", "topic_name"))
            key = _topic_key(topic_id, topic_name)
            if key not in merged:
                merged[key] = {"source": set(), "raw": {}}
                order.append(key)
            merged[key].update({"topic_id": topic_id, "topic_name": topic_name})
            merged[key]["source"].add("topic_ids")
            merged[key]["raw"]["topic_ids"] = dict(row)

        for row in hot_rows:
            topic_id = _text(_pick(row, "id", "t001", "topic_id"))
            topic_name = _text(_pick(row, "ztmc", "t002", "topic_name"))
            key = _topic_key(topic_id, topic_name)
            if key not in merged:
                merged[key] = {"source": set(), "raw": {}}
                order.append(key)
            merged[key].update(
                {
                    "topic_id": topic_id,
                    "topic_name": topic_name,
                    "relation_level": _float(_pick(row, "gld", "relation_level")),
                    "selected_date": _pick(row, "rxsj", "selected_date"),
                    "topic_date": _pick(row, "ztrq", "topic_date"),
                    "reason": _text(_pick(row, "ztnr", "reason")),
                    "detail_id": _text(_pick(row, "arec", "detail_id")),
                    "category_raw": _pick(row, "sslb", "category_raw"),
                }
            )
            merged[key]["source"].add("hot_topics")
            merged[key]["raw"]["hot_topics"] = dict(row)

        topics = tuple(_build_stock_topic(merged[key]) for key in order)
        return StockTopics(code=full_code, topics=topics)

    def topic_stocks(
        self,
        seed_code: str,
        *,
        topic_id: str | int | None = None,
        topic_name: str | None = None,
        sort_by: str = "zdf",
        section: str = "gndbzfsj",
    ) -> TopicStockTable:
        """Return stocks inside a topic using a seed stock and topic ID/name."""

        full_seed = normalize_code(seed_code)
        resolved_id = None if topic_id is None else str(topic_id)
        resolved_name = topic_name
        if resolved_id is None or topic_name is not None:
            matched = self._resolve_topic(full_seed, resolved_id, resolved_name)
            resolved_id = resolved_id or matched.topic_id
            resolved_name = resolved_name or matched.topic_name
        if resolved_id is None:
            raise ValueError("topic_id or topic_name is required when the seed stock has no topic id")

        response = self._client.f10.topic_compare(full_seed, resolved_id, section=section, sort_by=sort_by)
        rows = tuple(_build_topic_stock(row) for row in getattr(response, "rows", ()))
        return TopicStockTable(
            seed_code=full_seed,
            topic_id=resolved_id,
            topic_name=resolved_name,
            sort_by=sort_by,
            rows=rows,
        )

    def auction_data(
        self,
        code: str,
        date=None,
        *,
        include_series: bool = True,
        include_snapshot: bool = True,
        include_quote: bool = True,
        pre_close_price: float | None = None,
    ) -> AuctionData:
        """Aggregate current or historical auction process and opening match data."""

        full_code = normalize_code(code)
        is_history = date is not None
        trading_date = self._client.workdays.normalize(date) if is_history else self._current_market_date()

        series = None
        auction_records: tuple[Any, ...] = ()
        snapshot = None
        historical_page = None
        if is_history:
            if include_series or include_snapshot or (include_quote and pre_close_price is None):
                historical_page = self._client.trades.all_history(full_code, trading_date)
                if include_series:
                    auction_records = tuple(getattr(historical_page, "auction_snapshots", ()))
                if include_snapshot:
                    opening_matches = tuple(getattr(historical_page, "opening_matches", ()))
                    snapshot = opening_matches[0] if opening_matches else None
        else:
            if include_series:
                series = self._client.auctions.series(full_code)
            if include_snapshot:
                snapshot = self._client.trades.opening_match_today(full_code)

        quote = None
        if not is_history and include_quote and pre_close_price is None:
            quote = self._first_quote(full_code)

        resolved_pre_close = pre_close_price
        if resolved_pre_close is None and include_quote:
            if historical_page is not None:
                price_base = getattr(historical_page, "price_base_raw_f32", None)
                resolved_pre_close = (
                    round(float(price_base), 3)
                    if price_base is not None and price_base != 0
                    else None
                )
            elif quote is not None:
                resolved_pre_close = getattr(quote, "pre_close_price", None)

        open_price = None
        open_volume = None
        open_amount = None
        if snapshot is not None:
            open_price = getattr(snapshot, "price", None)
            open_volume = getattr(snapshot, "volume", None)
            open_amount = round(snapshot.trade_amount_yuan, 2)

        return AuctionData(
            code=full_code,
            trading_date=trading_date,
            series=series,
            auction_records=auction_records,
            snapshot_0925=snapshot,
            pre_close_price=resolved_pre_close,
            open_price=open_price,
            open_volume=open_volume,
            open_amount=open_amount,
            open_change_pct=_pct(open_price, resolved_pre_close),
        )

    def adjusted_kline(
        self,
        code: str,
        *,
        period: str = "day",
        adjust: str | None = "qfq",
        anchor_date=None,
        count: int = 800,
        start: int = 0,
        all_pages: bool = False,
        page_size: int = 800,
        max_pages: int | None = 200,
        include_raw: bool = False,
    ):
        """Fetch K-line data with plain adjust arguments."""

        if all_pages:
            return self._client.bars.all(
                code,
                period=period,
                adjust=adjust,
                anchor_date=anchor_date,
                page_size=page_size,
                max_pages=max_pages,
                include_raw=include_raw,
            )
        return self._client.bars.get(
            code,
            period=period,
            adjust=adjust,
            anchor_date=anchor_date,
            start=start,
            count=count,
            include_raw=include_raw,
        )

    def _security_map(self, full_codes: Sequence[str]) -> dict[str, Any]:
        markets = sorted({code[:2] for code in full_codes})
        result: dict[str, Any] = {}
        for market in markets:
            for item in self._client.codes.all(market):
                result[getattr(item, "full_code", f"{item.exchange}{item.code}")] = item
        return result

    def _finance_map(self, full_codes: Sequence[str]) -> dict[str, Any]:
        key = tuple(full_codes)
        batch = self._finance_cache.get(key)
        if batch is None:
            batch = self._client.corporate.finance_batch(full_codes)
            self._finance_cache[key] = batch
        return _by_full_code(getattr(batch, "records", ()))

    def _first_quote(self, full_code: str) -> Any | None:
        quotes = self._client.quotes.get_snapshots(full_code)
        if isinstance(quotes, Sequence) and not isinstance(quotes, (str, bytes, bytearray)):
            return quotes[0] if quotes else None
        return quotes

    def _current_market_date(self):
        handshake = self._client.session.handshake()
        dates = [
            value
            for value in (
                getattr(handshake, "server_date_1", None),
                getattr(handshake, "server_date_2", None),
            )
            if value is not None
        ]
        if not dates:
            raise RuntimeError("server handshake did not provide a current market date")
        return max(dates)

    def _resolve_topic(self, full_seed: str, topic_id: str | None, topic_name: str | None) -> StockTopic:
        topics = self.stock_topics(full_seed).topics
        if topic_id is not None:
            for item in topics:
                if item.topic_id == topic_id:
                    return item
        if topic_name is not None:
            for item in topics:
                if item.topic_name == topic_name:
                    return item
        if topics and topic_id is None and topic_name is None:
            return topics[0]
        raise ValueError(f"topic not found for {full_seed}: {topic_id or topic_name!r}")


def _code_list(codes: str | Sequence[str]) -> list[str]:
    if isinstance(codes, str):
        return [normalize_code(codes)]
    return [normalize_code(code) for code in codes]


def _by_full_code(items: Any) -> dict[str, Any]:
    result: dict[str, Any] = {}
    iterable: Iterable[Any]
    if isinstance(items, Mapping):
        iterable = items.values()
    else:
        iterable = items or ()
    for item in iterable:
        full_code = getattr(item, "full_code", None)
        if full_code is not None:
            result[str(full_code)] = item
    return result


def _build_stock_profile(full_code: str, quote: Any | None, security: Any | None, finance: Any | None) -> StockProfile:
    exchange = full_code[:2]
    code = full_code[2:]
    market_id = getattr(quote, "market_id", None)
    if market_id is None:
        market_id = getattr(security, "market_id", None)
    if market_id is None:
        market_id = getattr(finance, "market_id", None)

    last_price = getattr(quote, "last_price", None)
    circulating_shares = getattr(finance, "circulating_shares", None)
    total_shares = getattr(finance, "total_shares", None)
    volume_hand = getattr(quote, "total_hand", None)

    return StockProfile(
        full_code=full_code,
        exchange=exchange,
        market_id=market_id,
        code=code,
        name=getattr(security, "name", None),
        category=getattr(security, "category", None),
        board=getattr(security, "board", None),
        last_price=last_price,
        pre_close_price=getattr(quote, "pre_close_price", None),
        open_price=getattr(quote, "open_price", None),
        high_price=getattr(quote, "high_price", None),
        low_price=getattr(quote, "low_price", None),
        change=getattr(quote, "change", None),
        change_pct=getattr(quote, "change_pct", None),
        volume_hand=volume_hand,
        amount=getattr(quote, "amount", None),
        open_amount_yuan=getattr(quote, "open_amount_yuan", None),
        circulating_shares=circulating_shares,
        total_shares=total_shares,
        turnover_rate=_turnover_rate(volume_hand, circulating_shares),
        circulating_market_value=_market_value(last_price, circulating_shares),
        total_market_value=_market_value(last_price, total_shares),
        eps=getattr(finance, "eps_raw", None),
        ipo_date=getattr(finance, "ipo_date", None),
        updated_date=getattr(finance, "updated_date", None),
        security=security,
        quote=quote,
        finance=finance,
    )


def _turnover_rate(volume_hand: int | None, circulating_shares: float | None) -> float | None:
    if volume_hand is None or not circulating_shares:
        return None
    return volume_hand * 100.0 / circulating_shares * 100.0


def _market_value(price: float | None, shares: float | None) -> float | None:
    if price is None or shares is None:
        return None
    return price * shares


def _build_stock_topic(data: dict[str, Any]) -> StockTopic:
    return StockTopic(
        topic_id=data.get("topic_id"),
        topic_name=data.get("topic_name"),
        relation_level=data.get("relation_level"),
        selected_date=data.get("selected_date"),
        topic_date=data.get("topic_date"),
        reason=data.get("reason"),
        detail_id=data.get("detail_id"),
        category_raw=data.get("category_raw"),
        source="+".join(sorted(data.get("source", ()))),
        raw=dict(data.get("raw", {})),
    )


def _build_topic_stock(row: Mapping[str, Any]) -> TopicStock:
    market_id = _int(_pick(row, "sc", "market_id"))
    exchange = ID_TO_MARKET.get(market_id) if market_id is not None else None
    code = _text(_pick(row, "zqdm", "code"))
    full_code = f"{exchange}{code}" if exchange and code else None
    if full_code is None and code:
        try:
            full_code = normalize_code(code)
            exchange = full_code[:2]
            market_id = {"sz": 0, "sh": 1, "bj": 2}.get(exchange)
        except Exception:
            full_code = None

    return TopicStock(
        rank=_int(_pick(row, "pm", "rank")),
        full_code=full_code,
        exchange=exchange,
        market_id=market_id,
        code=code,
        name=_text(_pick(row, "zqjc", "name")),
        change_pct=_float(_pick(row, "zdf", "change_pct")),
        change_pct_3d=_float(_pick(row, "zdf_3d", "change_pct_3d")),
        change_pct_5d=_float(_pick(row, "zdf_5d", "change_pct_5d")),
        change_pct_20d=_float(_pick(row, "zdf_20d", "change_pct_20d")),
        change_pct_60d=_float(_pick(row, "zdf_60d", "change_pct_60d")),
        change_pct_ytd=_float(_pick(row, "zdf_ys", "change_pct_ytd")),
        trading_date=_pick(row, "tjdate", "trading_date"),
        raw=dict(row),
    )


def _topic_key(topic_id: str | None, topic_name: str | None) -> str:
    if topic_id:
        return f"id:{topic_id}"
    if topic_name:
        return f"name:{topic_name}"
    return "unknown"


def _pick(row: Mapping[str, Any], *names: str) -> Any | None:
    for name in names:
        if name in row:
            return row[name]
    return None


def _text(value: Any | None) -> str | None:
    if value is None:
        return None
    text = str(value).strip()
    return text or None


def _int(value: Any | None) -> int | None:
    if value is None or value == "":
        return None
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


def _float(value: Any | None) -> float | None:
    if value is None or value == "":
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def _pct(price: float | None, base: float | None) -> float | None:
    if price is None or not base:
        return None
    return (price - base) / base * 100.0
