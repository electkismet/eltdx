# T14 — 逐笔、竞价、开盘撮合与金额流

Read first. Authority: `docs/rust-sdk/development-plan.md` T14; `hq-api-spec.md` §§3–4,7; `public-api.md` §§6–9; `decision-register.md` §3; relevant COMP cases, especially today_batch compatibility and money-flow empty-probe limits. Read reviewed operation/metadata/pagination/decode APIs before editing. Controller supplies GO.

## Exact task

New SDK `src/services/{trades,auctions,money_flow}.rs`, DTO/query modules and lazy getters; new `tests/{trades,auctions,money_flow}.rs`. Typed records and column views, source-based opening-match meaning and dedicated HQ daily money flow.

- All-trades reverses page order but preserves row order within a page; retains mixed kinds and legitimate post-close trades.
- New Rust supports both today/history columns. Reproduce Python today_batch rejection and keep it until separately approved; shared native migration must not silently fix it.
- Auction keeps server sequence with no invented fixed time window;09:25 opening-match origin is traceable.
- Money-flow empty-result policy permits at most3 probes per code, per batch min(n,8)*3 capped24, all also under tighter shared operation budget; one code's emptiness says nothing about another.
- Historical interval money flow is enabled by F10 T20; experimental category22 is not a stable history substitute.
- Deferred `cargo test -p eltdx --test trades --test auctions --test money_flow --locked`.

## Complete domain behavior

Implement all HQ M41–M52 and M100. TradeQuery(date=Current,start=0,count=1800,pages={1800,100,180000}) dispatches Today0FC5 versus Historical0FC6 explicitly, with typed date. TradePage/TradeColumns and batch variants preserve index/absolute_index, original side/status, event kind, unknown tail, auction fields only when provided, milliyuan trade price rule independent of quote decimal metadata. Status8 is AuctionSnapshot even at09:25; other09:25 records are OpeningMatch, not a whole guessed morning window. Never remove postmarket observations or sort within pages. Opening match scans bounded shared pages and selects by semantic kind/source, not string prefix alone.

Reuse T11/T13 pagination/cursor/identity contracts: source-aware pin, fixed resolved date/snapshot per operation, checked offset/absolute-index, short page continues, empty proves EOF unless command explicitly proves more, no-progress/repeated-page/cap failure explicit. Many preserves duplicates/index; legal empty historical series is present empty. Many with cursor rejected including len1. Empty ordinary batch has no I/O. Columns and row results must derive from same shared decoded records, no separate parser/transport. Public entry Instant captured before validation; cancellation and total deadline cut every page/chunk/probe.

AuctionQuery date=Current; request profile/selector follows056A wire and echoes. AuctionSeries retains source-local sequence and legitimate out-of-window records; index/time/price/matched/unmatched/side/unknown scalars preserve units/raw bits per HQ DTO. Quote/current/pre-close adjunct choices belong the later helper, do not add invisible extra quote requests here.

MoneyFlowService daily/daily_many is0FFC only. Source-specific response blocks strictly validate requested identities; preserve raw numeric amounts/ratios/unknowns and exact source unit/provenance. A legitimate empty response may use dedicated bounded probing, but transport/protocol corruption never becomes empty. Charge fallback/empty probes via T08 authorization and local policy atomically; retries consume shared extra too. Record why a value remains unavailable; do not copy another security's result. No unconstrained retries or per-probe renewed operation deadline. Python legacy retains original isolated per-code/per-batch counting/request timing; hooks must support T18 adapter without new Rust caps leaking into Python.

Use immutable metadata only where command requires it, no global resolver reads/writes. Share retained Bytes/ranges at page response level; preserve integer and f32 bits with Omit too. Public DTOs private fields/getters under `types`, services under `services`; no protocol structs wholesale exposed. New optional F10 method cannot exist as stub under default features; T20 adds it under f10 when implementation exists. category22 stays documented experimental advanced path only.

## Sources, report, workflow

Read Python `api/{trades,auctions,money_flow}.py`, matching models, protocol `commands/{trades,auctions,money_flow,history_fund_flow}.rs`, fixture inventory and known missing capture labels. Author synthetic independent source cases for mixed09:25/status8, late trades, reverse pages, empty after short, repeated page, history columns, today_batch old failure, no metadata-dependent trade rescaling, per-code3/batch24/external budget exhaustion, corrupt amount block, unknown identity, cancel and resource cleanup. Do not fabricate real capture provenance.

Workspace `C:/Users/ax/.codex/worktrees/rust-sdk-implementation/tdx抓包`, branch `codex/rust-sdk-implementation`. All execution verification forbidden until all T01–T30 code complete: tests/collection/build/check/clippy/doctest/network/compile/AST/format/diff checks/equivalents. Source authoring, ordinary review diff generation and git checkpoints allowed. No subagents/push/tag/publish/merge; no controller ledger/status edits. Coordinate shared client/query/cursor files; commit only owned files with hooks disabled.

Full report `.superpowers/sdd/development-plan/task-14-report.md`: method coverage, exact files/commits, authored cases and provenance, no-execution statement, T18 projection hooks, self-review/concerns. Return compact status/SHAs/no-tests/concerns/report path. No compile/runtime claims.
