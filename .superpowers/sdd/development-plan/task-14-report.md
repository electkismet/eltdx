# T14 implementation recovery report

Status: all 13 public methods and source self-review authored. Execution and independent review have not occurred. This is an implementation checkpoint, not compilation, runtime or Python compatibility evidence.

Baseline `2576dca`; trade checkpoint `992ba98`. The accompanying completion commit adds auctions, daily money flow, distinct lazy pools, shared physical connection limits, atomic probe accounting, synthetic source cases and public documentation. Next task T15. Goal active without a token budget.

## Method and file coverage

| Methods | Implementation | Public types and source cases |
| --- | --- | --- |
| M41–M50 | `services/trades.rs`: page/collect/columns/collect_columns/opening_match and five many variants | `types/trades.rs`, `tests/trades.rs` |
| M51 | `services/auctions.rs`: series, 056A selector3/start0/limit500, Current/date dispatch | `types/auctions.rs`, `tests/auctions.rs` |
| M52/M100 | `services/money_flow.rs`: daily_many/daily, same private per-code algorithm, 0FFC only | `types/money_flow.rs`, `tests/money_flow.rs` |

Paths above are under `crates/eltdx/src/` for implementation/types and `crates/eltdx/` for tests. Additional shared changes: `client.rs`, `config.rs`, `hosts.rs`, `services/mod.rs`, `types/{mod,provenance}.rs`, `hq/{mod,executor,domains}.rs`, `operation.rs`, `operation_tests.rs`, `policy/retries.rs`; runtime physical permit in `crates/eltdx-runtime/src/engine.rs`. Test server connection counters are in `crates/eltdx/tests/support/hq_server.rs`. Fixture clarification is in `tests/fixtures/sdk/policy/units.json` and corresponding contract source.

## Trades and opening match

Every public entry captures Instant before validation. Single/many/rows/columns share decoded records and a bounded read loop with one frozen date and metadata snapshot; no quote-precision I/O is added. Explicit dates always select historical 0FC6, Current selects 0FC5. Collect/opening/many keep the same source pin. Short pages continue, empty pages prove EOF, exact repeated private wire payloads fail even with Omit, offsets and record indices use checked arithmetic. Aggregation reverses page order and corresponding raw row ranges without sorting rows within a page.

Status8 takes AuctionSnapshot precedence, including09:25; other09:25 rows are OpeningMatch. Legitimate post-close observations remain. Opening-match selects the first semantic match in source scan order; None requires EOF and cap exhaustion fails explicitly. A returned tick retains its source command/endpoint/date/metadata/owner stamp. Typed column arrays retain all row fields, integer price accumulator/delta/denominator, reserved fields and source; tick/to_page are pure checked conversions. Opaque cursor containers intentionally do not derive Serialize.

Today time remains TimeOfDay; history is source-local naive EventTime::local, without an invented UTC or Shanghai timezone conversion. Ordinary protocol trade prices use raw/100, convertible ticks raw/1000; these scales do not come from quote decimal4 metadata. Unknown mixed-asset quantity units remain Unknown, and status8 auction fields without wire evidence remain None.

## Auctions and pools

Auction preserves source order, seconds, exact f32 bits, signed unmatched raw value/direction and reserved byte. Invalid time/nonfinite price/count beyond500 fail; no fixed morning window or hidden quote adjunct request is added. Quantity units remain Unknown.

Ordinary, auction and money-flow connections remain distinct on the same address. New client clones/getters reuse domain pools. Dedicated numeric endpoints have separate optional builder overrides and a packaged default catalog from `src/eltdx/money_flow_servers.json`; ordinary endpoints do not override them. Build/getters create no network resources. Each actual execution registers the engine with SessionOwner before startup.

The new Rust builder opts all engines, including source-bound endpoint engines, into one ConnectionLimit. Default checked4×pool_size; max_connections must be at least pool_size and within semaphore range. A physical TCP attempt holds a permit, its ManagedStream retains it through use, and failed connect/handshake/drop releases it. Exhaustion is LimitExceeded with owner_connections context. Legacy EngineConfig constructors do not opt in, so existing Python connection policy is unchanged.

Money-flow domain admission serializes operations cooperatively under their original deadline/cancellation; work chunks are≤75, with sequential single-code requests. A probe endpoint engine is cached in the existing owner map but explicitly closed after each probe before admission can be released/reused. Failed retirement remains owner-registered; a later successful close can retire it. Request/stop roots and cleanup failures are retained separately, including begin-close errors. Successful retirement does not prematurely claim cleanup time for recoverable source errors; operation stop switches to the already shared cleanup deadline.

## Daily money flow and budgets

Single daily calls use the same private algorithm directly, not a public one-item batch. Many preserves occurrences and input indices. Each code independently considers its primary response and rotates alternate distinct hosts excluding the responding primary. Duplicate occurrences share a per-code scope. The first physical attempt atomically reserves all three applicable counters: code≤3, batch≤min(n,8)×3, and global extra attempts. Failed/unstarted reservations refund all three together; a wire retry consumes global extra without counting another logical probe. Original legacy paths use no new code scope.

Exactly one requested identity block is required; unknown market, foreign/duplicate identity, corrupt/truncated/trailing payload and nonfinite required amounts/ratios are Protocol. A valid40-byte header with zero records is present-empty. An empty byte payload is corruption, not empty. A primary transport error returns directly. Alternate connection failures may try the next bounded source; absent a later nonempty recovery, the real connection error returns rather than false-empty success. A successful later source carries a diagnostic of prior source failure. Protocol/cancellation/deadline/limit failures stop immediately.

Raw21 words and16 bucket values survive Omit. Total amount retains F32 bits; derived amounts retain F64, unknown source units remain Unknown, ratios use PercentagePoints. Invalid date retains date_raw and a diagnostic rather than inventing a trading date. Historical interval fund flow remains T20/F10; no default-feature stub or category22 stable substitute was added.

## Source cases and evidence

Authored cases cover trade row/page/raw order, ordinary/convertible scale, today/history columns and full-field conversion, mixed09:25/status8/post-close, opening EOF/caps, repeated pages/offset/count/malformed time/trailing, many duplicates/cursor/cancellation; auction sequence/seconds/f32 bits/date layout/count/time/nonfinite/raw; money-flow independent A/B empty recovery, per-code3/batch24/duplicate scopes/global tighter budget, invalid identity/empty bytes/date/nonfinite/truncation/trailing, distinct/reused pools, owner cap and cancelable admission. Operation unit source covers atomic three-scope reservation, commit and refund. All are unexecuted synthetic sources.

The previous units fixture mislabeled a convertible /1000 example as equity. It now cites ordinary parse_tick_records and Python trade_amount_yuan, explicitly uses /100 and supplied hands×100 arithmetic. Its narrow synthetic arithmetic scope is recorded; it is not a production oracle for mixed assets/status8. Full real0FFC capture, external market-unit evidence and execution verification remain missing.

## T18 obligations and limits

Python facade and native binding behavior were not changed. T18 must retain native today_batch ValueError("trade batch only supports historical ticks"), execute-only custom transport fallback, original per-request timing, isolated legacy probe counts and legacy raw projection. Current private SDK loops are Rust-facing; adapter integration still must select legacy policy without leaking new Rust caps. No Python compatibility or throughput claim is made from source inspection.

No tests, collection, builds/check/clippy/doctests, network probes, AST/compile, formatting, diff checks or equivalent execution verification ran. No subagents, push, tag, publication or merge. All T01–T30 implementation must be complete before unified validation.
