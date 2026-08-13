# 迁移到 eltdx 2.0

eltdx 2.0 移除了 `TdxClient` 上的旧版扁平 `get_*` 兼容入口。安装升级不会失败，但仍调用旧方法的业务代码会收到 `AttributeError`，需要改用模块化 API。

## 原生接口迁移

| 1.x 旧入口 | 2.0 入口 |
| --- | --- |
| `client.get_count(market)` | `client.codes.count(market)` |
| `client.get_codes(market, ...)` | `client.codes.list(market, ...)` |
| `client.get_codes_all(market)` | `client.codes.all(market)` |
| `client.get_quote_depth(codes)` | `client.quotes.get_depth(codes)` |
| `client.get_legacy_quotes(codes)` | `client.quotes.legacy(codes)` |
| `client.read_server_file(path, ...)` | `client.resources.read(path, ...)` |
| `client.get_kline(period, code, ...)` | `client.bars.get(code, period=period, ...)` |
| `client.get_kline_all(period, code, ...)` | `client.bars.all(code, period=period, ...)` |
| `client.get_minute(code)` | `client.minutes.today(code)` |
| `client.get_history_minute(code, date)` | `client.minutes.history(code, date)` |
| `client.get_trades(code, ...)` | `client.trades.today(code, ...)` |
| `client.get_history_trade(code, date, ...)` | `client.trades.history(code, date, ...)` |
| `client.get_trades_all(code)` | `client.trades.all_today(code)` |
| `client.get_history_trade_day(code, date)` | `client.trades.all_history(code, date)` |
| `client.get_call_auction(code)` | `client.auctions.series(code)` |
| `client.get_gbbq(code)` | `client.corporate.capital_changes(code)` |
| `client.get_finance_batch(codes)` | `client.corporate.finance_batch(codes)` |

## Helpers 迁移

| 1.x 旧入口 | 2.0 入口 |
| --- | --- |
| `client.get_quote(codes)` | `client.helpers.full_quotes(codes)` |
| `client.get_auction_0925(code, date)` | `client.trades.opening_match_today(code)` 或 `client.trades.opening_match_history(code, date)` |
| `client.get_xdxr(code)` | `client.helpers.xdxr(code)` |
| `client.get_equity_changes(code)` | `client.helpers.equity_changes(code)` |
| `client.get_equity(code, on=...)` | `client.helpers.equity(code, on=...)` |
| `client.get_turnover(code, volume, ...)` | `client.helpers.turnover(code, volume, ...)` |
| `client.get_factors(code)` | `client.helpers.factors(code)` |
| `client.get_local_adjusted_kline_all(period, code, ...)` | `client.helpers.local_adjusted_kline(code, period=period, ...)` |
| `client.get_adjusted_kline(...)` | `client.bars.get(..., adjust=...)` |
| `client.get_adjusted_kline_all(...)` | `client.bars.all(..., adjust=...)` |

`client.helpers.get_shortline_indicators()` 等带 `get_` 的 Helper 别名也已删除，直接使用 `client.helpers.shortline_indicators()`、`stock_profile_table()`、`stock_topics()`、`topic_stocks()`、`auction_data()` 和 `adjusted_kline()`。

`client.quotes.legacy()` 是原生 `0x053e` 协议接口，`client.resources.read()` 是原生 `0x06b9` 协议接口；它们不是旧版 Python 兼容层，2.0 继续保留。
