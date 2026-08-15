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
    "AuctionSeries": 8,
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
