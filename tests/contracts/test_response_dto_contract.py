"""Static contract checks for the Rust response DTO boundary.

This test is deliberately import-free so it can be reviewed before the first
native build.  Runtime value and model-construction checks are performed by the
later unified native rounds.
"""

from __future__ import annotations

import ast
import re
from pathlib import Path


ROOT = Path(__file__).parents[2]
RESPONSE = ROOT / "crates" / "eltdx-python" / "src" / "response.rs"
TRADES = ROOT / "crates" / "eltdx-protocol" / "src" / "commands" / "trades.rs"

EXPECTED_TAGS = {
    "heartbeat",
    "handshake",
    "capital_changes",
    "finance_batch",
    "security_list",
    "security_count",
    "special_limits",
    "intraday_aux",
    "klines",
    "today_intraday",
    "legacy_quotes",
    "refresh_stream",
    "category_quotes",
    "snapshots",
    "auction_series",
    "file_content",
    "historical_intraday",
    "today_ticks",
    "historical_ticks",
    "sparkline",
    "recent_intraday",
}

EXPECTED_MODEL_FIELDS = {
    "AuctionPoint": 13,
    "AuctionSeries": 9,
    "CapitalChangeRecord": 21,
    "CapitalChangeBlock": 6,
    "FinanceRecord": 42,
    "FinanceBatch": 2,
    "SecurityCode": 15,
    "SpecialLimitRecord": 7,
    "SpecialLimitPage": 3,
    "MinutePoint": 14,
    "MinuteSeries": 10,
    "MinuteAuxPoint": 10,
    "MinuteAuxSeries": 7,
    "SparklineSeries": 11,
    "KlineBar": 22,
    "KlineSeries": 14,
    "QuoteLevel": 3,
    "QuoteSnapshot": 23,
    "LegacyQuote": 27,
    "CategoryQuoteRecord": 39,
    "CategoryQuotePage": 9,
    "QuoteRefreshRecord": 24,
    "QuoteRefreshPage": 4,
    "FileContentChunk": 6,
    "TradeTick": 19,
    "TradePage": 9,
    "HeartbeatAck": 4,
    "HandshakeInfo": 12,
}

MODEL_FILES = {
    "AuctionPoint": "auction.py",
    "AuctionSeries": "auction.py",
    "CapitalChangeRecord": "corporate.py",
    "CapitalChangeBlock": "corporate.py",
    "FinanceRecord": "corporate.py",
    "FinanceBatch": "corporate.py",
    "SecurityCode": "security.py",
    "SpecialLimitRecord": "limit.py",
    "SpecialLimitPage": "limit.py",
    "MinutePoint": "minute.py",
    "MinuteSeries": "minute.py",
    "MinuteAuxPoint": "minute.py",
    "MinuteAuxSeries": "minute.py",
    "SparklineSeries": "minute.py",
    "KlineBar": "kline.py",
    "KlineSeries": "kline.py",
    "QuoteLevel": "quote.py",
    "QuoteSnapshot": "quote.py",
    "LegacyQuote": "quote.py",
    "CategoryQuoteRecord": "quote.py",
    "CategoryQuotePage": "quote.py",
    "QuoteRefreshRecord": "quote.py",
    "QuoteRefreshPage": "quote.py",
    "FileContentChunk": "resource.py",
    "TradeTick": "trade.py",
    "TradePage": "trade.py",
    "HeartbeatAck": "session.py",
    "HandshakeInfo": "session.py",
}


def _dataclass_field_counts(path: Path) -> dict[str, int]:
    tree = ast.parse(path.read_text(encoding="utf-8"))
    counts: dict[str, int] = {}
    for node in tree.body:
        if not isinstance(node, ast.ClassDef):
            continue
        fields = [item for item in node.body if isinstance(item, ast.AnnAssign)]
        if fields:
            counts[node.name] = len(fields)
    return counts


def test_response_tags_are_exhaustive() -> None:
    source = RESPONSE.read_text(encoding="utf-8")
    actual = set(re.findall(r'tagged\(\s*py,\s*"([a-z_]+)"', source))
    actual.remove("push")
    assert actual == EXPECTED_TAGS


def test_response_tuple_shapes_and_raw_policy_are_frozen() -> None:
    source = RESPONSE.read_text(encoding="utf-8")
    model_root = ROOT / "src" / "eltdx" / "models"
    for model_name, expected_count in EXPECTED_MODEL_FIELDS.items():
        actual_count = _dataclass_field_counts(model_root / MODEL_FILES[model_name])[model_name]
        assert actual_count == expected_count
        assert model_name in source
    assert "request_date" not in source
    assert "any(py, req.period.name())?" in source
    assert "any(py, req.adjust.as_str())?" in source
    assert "bytes(py, &[])" not in source
    assert 'PyString::new(py, "")' not in source
    assert "fn raw_payload<'py>(py: Python<'py>, _include_raw: bool" in source
    assert "fn record_hex<'py>(py: Python<'py>, _include_raw: bool" in source
    assert "record_hex(py, false" not in source
    assert source.count("any(py, *series_a)?") == 2
    assert source.count("any(py, *series_b)?") == 2
    assert ".map(|v| legacy_quote(py, v, true))" in source
    assert ".map(|v| refresh_quote(py, v, true))" in source
    assert ".map(|v| category_quote(py, v, true))" in source


def test_hot_record_dtos_use_flat_fixed_stride_aggregates() -> None:
    response_source = RESPONSE.read_text(encoding="utf-8")
    models_source = (ROOT / "src" / "eltdx" / "_native_models.py").read_text(
        encoding="utf-8"
    )
    level_body = response_source.split("fn level", 1)[1].split("\nfn ", 1)[0]
    assert "tuple_array(" in level_body
    assert "vec![" not in level_body
    for function_name in ("extend_trade_tick", "extend_quote_snapshot"):
        body = response_source.split(f"fn {function_name}", 1)[1].split("\nfn ", 1)[0]
        assert "fields.extend([" in body
        assert "tuple_array(" not in body
        assert "vec![" not in body
    assert "const SNAPSHOT_STRIDE: usize = 27;" in response_source
    assert "const TRADE_TICK_STRIDE: usize = 19;" in response_source
    assert "quote_snapshots(py, &values)?" in response_source
    assert response_source.count("trade_ticks(py, &value.ticks, req.include_raw)?") == 2
    assert "_SNAPSHOT_STRIDE = 27" in models_source
    assert "_TRADE_TICK_STRIDE = 19" in models_source
    assert "def _flat_records(" in models_source
    today_tick_body = models_source.split("def _today_trade_ticks", 1)[1].split(
        "\ndef ", 1
    )[0]
    assert "zip(*((iterator,) * _TRADE_TICK_STRIDE), strict=True)" in today_tick_body
    assert "starmap(TradeTick, records)" in today_tick_body
    tick_body = models_source.split("def _trade_tick_at", 1)[1].split("\ndef ", 1)[0]
    assert "return TradeTick(" in tick_body
    assert "_tuple(" not in tick_body
    trade_page_body = models_source.split("def _trade_page", 1)[1].split("\ndef ", 1)[0]
    assert "if fields[6] is None:" in trade_page_body
    assert "parsed_ticks = _today_trade_ticks(ticks)" in trade_page_body
    assert "parsed_ticks = tuple(" in trade_page_body
    snapshot_body = models_source.split("def _quote_snapshot_at", 1)[1].split("\ndef ", 1)[0]
    assert "return QuoteSnapshot(" in snapshot_body
    assert "QuoteLevel(fields[offset + 20]" in snapshot_body
    assert "QuoteLevel(fields[offset + 23]" in snapshot_body
    assert "_records(" not in snapshot_body


def test_trade_semantic_names_are_borrowed_and_response_local() -> None:
    response_source = RESPONSE.read_text(encoding="utf-8")
    trades_source = TRADES.read_text(encoding="utf-8")
    assert "pub fn canonical_name(&self) -> Cow<'static, str>" in trades_source
    for name in ("buy", "sell", "neutral"):
        assert f'Cow::Borrowed("{name}")' in trades_source
    assert 'Cow::Owned(format!("status_{value}"))' in trades_source

    names_body = response_source.split("struct TradeSemanticNames", 1)[1].split(
        "struct TradeTickObjects", 1
    )[0]
    assert "HashMap" not in names_body
    assert names_body.count("clone_ref(py)") == 6
    for name in (
        "buy",
        "sell",
        "neutral",
        "trade",
        "opening_match",
        "auction_snapshot",
    ):
        assert f'{name}: any(py, "{name}")?' in names_body

    tick_body = response_source.split("fn extend_trade_tick", 1)[1].split(
        "\nfn ", 1
    )[0]
    assert "objects.semantic_names.side(py, &value.side)?" in tick_body
    assert "objects.semantic_names.event_kind(py, value.event_kind)" in tick_body
    assert "any(py, value.side.canonical_name())?" not in tick_body
    assert "any(py, value.event_kind.canonical_name())?" not in tick_body

    aggregate_body = response_source.split("fn trade_ticks", 1)[1].split("\nfn ", 1)[0]
    assert aggregate_body.count("TradeTickObjects::new(py)?") == 1
    assert "extend_trade_tick(py, &mut fields, value, include_raw, &mut objects)?" in (
        aggregate_body
    )


def test_trade_tick_repeated_objects_are_shared_and_response_local() -> None:
    response_source = RESPONSE.read_text(encoding="utf-8")
    trades_source = TRADES.read_text(encoding="utf-8")

    assert "pub time_label: Arc<str>" in trades_source
    assert "pub record_hex: Arc<str>" in trades_source
    parser_body = trades_source.split("fn parse_tick_records", 1)[1].split(
        "\npub fn minute_of_day_label", 1
    )[0]
    assert "last_time_label: Option<(u16, Arc<str>)>" in parser_body
    assert "last_record: Option<(&[u8], Arc<str>)>" in parser_body
    assert parser_body.count("Arc::clone(") == 4
    tests_body = trades_source.split("#[cfg(test)]", 1)[1]
    assert tests_body.count("Arc::ptr_eq(") == 2

    objects_body = response_source.split("struct TradeTickObjects", 1)[1].split(
        "\nfn any", 1
    )[0]
    for field in ("last_time_minutes", "last_time_label", "last_record_hex"):
        assert field in objects_body
    assert objects_body.count("Arc::ptr_eq(") == 2
    assert "HashMap" not in objects_body

    tick_body = response_source.split("fn extend_trade_tick", 1)[1].split(
        "\nfn ", 1
    )[0]
    assert "u32::from(value.index) == value.absolute_index" in tick_body
    assert "index.clone_ref(py)" in tick_body
    assert "objects.time_minutes(py, value.time_minutes)?" in tick_body
    assert "objects.time_label(py, &value.time_label)" in tick_body
    assert "objects.record_hex(py, include_raw, &value.record_hex)" in tick_body
    assert "any(py, value.time_label.as_str())?" not in tick_body
    assert "\n        record_hex(py, include_raw, &value.record_hex)," not in tick_body
