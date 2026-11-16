# T13 — K线、退市K线与分钟线分页

Read first. Binding authority: `docs/rust-sdk/development-plan.md` T13, `hq-api-spec.md` §§3–4,6, `public-api.md` §§6–9, `decision-register.md` §3, COMP-030/031 and policy date/unit fixtures. Read current reviewed T08–T11 operation, metadata, raw/cursor helpers before edits. Start after controller GO.

## Exact plan task

New SDK `src/services/{klines,minutes}.rs`, extend shared `types/time.rs` as needed, add domain types/query modules and lazy client getters; new `tests/{klines,minutes}.rs`. Stable validated queries/options, single page versus complete series, page raw mapping, protocol period/calendar distinctions.

- Author short/empty/repeated pages, max_pages/no progress/start offset/total-deadline source cases.
- Cover week/month/quarter/year, category8, HQ time behavior distinct from EX/MAC.
- Preserve different ordinary versus delisted legacy raw aggregation projection.
- Invalid dates/counts/trailing data follow parser contracts.
- Deferred commands `cargo test -p eltdx --test klines --test minutes --locked`.

## Full service scope

Implement HQ M25–M40 plus M98–M99 with exact signatures/getters/defaults from HQ §6: Kline page/collect and many variants, delisted page/collect and many variants; Minute today/history/recent and many, aux/buy_sell_strength/volume_comparison and sparkline. No stubs for valid typed inputs. KlineQuery uses period=Day, start=0, count=800 (1..800), adjust=None, anchor=None, kind=Auto, bar_time=Start, pages={800,200,160000}; Delisted same bounds but only day/no adjustment and no misleading period fields. MinuteQuery date=Current. Sparkline selector=1/window=20, selector echo retained; unsupported semantic selector is explicit Unsupported. Shared DTOs and query types go `eltdx::types`, services `eltdx::services`, not redundant root exports.

Use one operation and one immutable metadata snapshot for every page/chunk/fallback. Capture entry Instant before validation. Ordinary continuation token matching checks protocol/command/id/date/period/adjust/kind/bar_time/page size/owner generation/endpoint/metadata version and necessary accumulators before submission. Current resolves once; cross-day token invalid. Start setters clear continuation; other semantic changes lead to validation error rather than silently dropping token. Many rejects cursor even len=1; collect with cursor returns only remaining pages. Same active-source pin is held/retired through multi-page source-sensitive work, source loss remains PinLost; no switching source and appending under stale cursor. Token itself holds no pin or owner lease. Reuse existing T11 cursor helpers; coordinate shared changes.

Ordinary collect: advance by actual rows with checked wire u16 offset, continue on short nonempty pages, empty page proves EOF. Reverse page order, keep order within each page; do not sort/deduplicate legitimate same-time records. Delisted collect: stable time ascending, preserve equal-time source order. Max pages/records/offset overflow before proven EOF is LimitExceeded; no truncation to ordinary success. Source cases use COMP-030 two short pages at offsets0/2 then empty, final order [09:30,09:35,09:40,09:45]. Detect repeated page/no progress with explicit reason, not by treating same price/time alone as duplicate. Raw retains per-page payload/wire/ranges, not one artificial frame. Legacy ordinary aggregated raw empty; delisted keeps first page raw. Put compatibility projection in shared domain result so T18 binding need not reimplement page ordering.

Price/quantity/amount projection uses preserved integers, scale/f32 bits and exact T10 provenance; never invert a rounded float into alleged wire raw. KlineBar includes index-up/down only when actually present. Period numeric/custom categories8/9, seconds13, day/week/month/quarter/year remain distinct; category22 is not OHLC. BarTime::End offsets minute periods only, with real Gregorian rollover including leap/month/year boundaries and exchange-local time; daily/weekly/monthly/quarterly/yearly/seconds remain unchanged. Invalid period arguments/anchor combinations fail before networking. A local time without source date/offset remains explicitly naive rather than UTC/host-today.

Three minute formats0537/0FB4/0FEB keep their own base/average/raw fields; no generic reinterpretation. Fix trading date once via source/clock policy. Aux cumulative values retain f32 source bits and unknown units where no evidence. Sparkline validates selector echo and bounded lengths; unsupported/malformed differs. Empty *_many returns empty without metadata/network, input accounting/duplicates follow T11, legal empty per-security series is Present(empty) rather than ConfirmedMissing. Entire operation cancellation/deadline stops, including optional/fallback paths; bounded cleanup releases pin despite cancelled business token.

## Source anchors, source cases and workflow

Read Python `api/{bars,delisted_bars,minutes}.py`, `_bar_time.py`, relevant models and protocol `commands/{klines,delisted_bars,minutes}.rs`. Golden inputs stay unchanged; author independent small payload/expected cases with provenance. Include extreme offsets, response count overflow, malformed/truncated time fields, retained unknown tails, epoch invalidation, metadata-refresh cursor invalidation, changed page size, cancelled second page, valid empty history, duplicates and many occurrence order. Test SOURCE only.

Workspace `C:/Users/ax/.codex/worktrees/rust-sdk-implementation/tdx抓包`, branch `codex/rust-sdk-implementation`. Until all T01–T30 implementation complete: NO tests/collection/build/check/clippy/doctest/network/compile/AST/format/diff checks or equivalent verification. Ordinary review diff generation and git checkpoints allowed; commit exact scope with hooks disabled. No subagents/push/tag/publish/merge; no controller ledger/status edits. Coordinate shared client/types/cursor/metadata interfaces with controller.

Report `.superpowers/sdd/development-plan/task-13-report.md`: complete implementation/method coverage, files/commits, authored source cases, no-execution, self-review and T18 compatibility hooks. Return compact status/commits/no-tests/concerns/path. No runtime/compile claims.
