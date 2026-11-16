# T14 implementation recovery report

Status: partial implementation; trade rows/columns/opening-match source authored. Auction and daily money-flow services plus dedicated pools are still pending. No completion, compilation, runtime or independent-review claim.

Baseline: `2576dca` on `codex/rust-sdk-implementation`.

## Authored source

- `crates/eltdx/src/types/trades.rs`: full TradeQuery, TradeTick, TradePage, TradeColumns, TradeColumnData and semantic enums. Typed columns retain every row field, including raw price accumulator/delta/denominator, reserved tail and per-tick source. Pure checked tick lookup and page reconstruction.
- `crates/eltdx/src/services/trades.rs`: all ten M41-M50 entry points capture Instant first and share the same internal read loop. Many preserves occurrences and uses one context/date/snapshot/pin; explicit date always selects 0FC6, Current selects 0FC5. No quote metadata I/O or global resolver.
- Lazy `TdxClient::trades()` and module/type exports.
- `ResultMeta::with_trading_date` adds the frozen series date to response and aggregate source stamps.
- `crates/eltdx/tests/trades.rs`: authored synthetic loopback sources for row ordering/raw mapping, today/history columns, convertible milli and ordinary hundredth prices, status8 precedence and first source-order opening match, EOF/cap, repeated page, offset overflow, excessive count, malformed time/trailing data, many duplicates/empty present series, cursor validation, empty no-I/O calls, cancellation and expired entry.

Collections hold a source pin, reject exact repeated wire pages even when diagnostic raw is omitted, advance checked offsets and only infer EOF from empty pages. Pages reverse at aggregation while row-local index and absolute index survive. Opening-match returns None only at verified EOF, and a found tick retains endpoint/command/date/metadata/owner provenance.

## Evidence and compatibility

All new test payloads are synthetic, constructed from source layouts; no new real capture and no independent market-unit oracle is claimed. HQ protocol source explicitly uses /100 for ordinary trade prices and /1000 for convertible-bond ticks. It resets the price accumulator per response page. The SDK retains the exact raw integer and corresponding decimal scale. Quote decimal metadata cannot change this scale.

`tests/fixtures/sdk/policy/units.json::hq_equity_tick` currently labels a /1000 convertible-specific arithmetic example as an equity case while citing the convertible test. This evidence mismatch remains to be reconciled in the completed T14 report/spec checkpoint; do not change the parser or invent a production-price oracle from this fixture. Quantity units without sufficient per-instrument evidence remain Unknown.

Python facade/binding source is unchanged at this checkpoint. T18 must preserve native today_batch rejection, custom execute-only fallback, original per-request timing and original aggregate raw projection. New Rust today columns are supported by the shared typed path. Public row/column containers intentionally do not serialize opaque continuation tokens.

## Resume next

1. Implement lazy owner-shared distinct auction and money-flow pools with authoritative dedicated hosts, configuration injection and a hard owner-wide connection bound; close temporary probe resources deterministically.
2. Implement AuctionQuery/AuctionPoint/AuctionSeries and M51 (preserve f32 bits, seconds and source sequence, reject invalid time).
3. Implement MoneyFlowDay/DailyMoneyFlow and M52/M100; strict response identity and finite numeric checks, raw business fields under Omit, per-code three and per-batch min(n,8)*3 probes charged atomically through the existing attempt authorization.
4. Author corresponding synthetic test sources, inspect all T14 source, reconcile documented precision evidence and save completion/recovery checkpoints.

No tests, collection, builds, cargo check/clippy/doctests, network probes, AST/compile/formatting, diff checks or equivalent verification have run. The user's all-implementation-before-validation constraint remains in force. No subagents, push, tag, publication or merge.
