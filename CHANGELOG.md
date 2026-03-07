# Changelog

All notable changes to `eltdx` will be documented in this file.

## 0.1.3 - 2026-03-07

Small PyPI page sync release.

### Changed

- refreshed the README badge URL so the PyPI project page picks up the latest version badge
- republished the current public README to keep GitHub and PyPI project pages aligned

## 0.1.2 - 2026-03-07

Small documentation and packaging refresh.

### Changed

- refreshed the PyPI project page to match the latest public README
- replaced the README Python badge with a clearer `Python 3.10+` display
- fixed a corrupted line in `docs/API_REFERENCE.md`

## 0.1.1 - 2026-03-07

Small compatibility update for the first public release.

### Fixed

- restored full Python 3.10 compatibility in protocol numeric decoding
- replaced direct `math.exp2()` usage with a safe fallback for older Python runtimes
- added a regression test to keep the 3.10 code path covered in CI

## 0.1.0 - 2026-03-07

First Python alpha cut of the Tongdaxin client, focused on the protocol and client layer.

### Added

- `TdxClient` as the single public entry point for market data access
- long-lived pooled connections with default `pool_size=2`
- automatic quote batching with default `batch_size=80`
- context manager support via `with TdxClient() as client:`
- dataclass response models for call auction, quote, code, minute, trade, kline, gbbq, xdxr, equity, and factors
- native Python `datetime` and `date` conversion for parsed time fields
- optional raw protocol hex debugging on supported responses via `include_raw=True`
- public frozen API methods:
  - `get_call_auction()`
  - `get_quote()`
  - `get_count()`
  - `get_codes()`
  - `get_codes_all()`
  - `get_stock_codes_all()`
  - `get_etf_codes_all()`
  - `get_index_codes_all()`
  - `get_minute()`
  - `get_trades()`
  - `get_trades_all()`
  - `get_kline()`
  - `get_kline_all()`

### Compatibility

- kept compatibility aliases for `get_history_minute()`, `get_trade()`, `get_trade_all()`, `get_history_trade()`, and `get_history_trade_day()`
- accepted both `get_kline(freq, code)` and `get_kline(code, freq)` during the API freeze transition

### Validation

- unit test suite passes locally
- integration tests remain opt-in through `ELTDX_RUN_LIVE=1`
