# T18 — 完整 HQ helpers 与 Python 回迁

Read first. Authority `docs/rust-sdk/development-plan.md` T18, `hq-api-spec.md` §§3–4,11–14 and F28 onward, `public-api.md` §§6–10, `execution-and-compatibility.md` entire binding/shutdown contracts, COMP-023–027 and relevant HQ cases. T11–T17 must be reviewed; controller gives GO. Default HQ includes all methods in this brief, not only quotes.

2026-11-17 recovery: reports seventh/eighth/ninth sections supersede the initial
pending helper/example notes. All19 helper methods and example/consumer sources
exist (example39c8e3e); first ordinary/delisted native bar migration and unexecuted
contract/differential sources now authored. Resume minutes/ALL remaining HQ native
migrations and evidence/installed-wheel consumer. T18 remains partial; do not rerun
or rewrite completed sources, and do not execute any verification until T01–T30.

Tenth recovery update: ordinary/delisted bars committed b5ed0b5. All five minute
native routes now authored, sharing request/decode boundary and original scalar
projection; original-input market inference, resolver order, negative volume,
raw hex, batch workers and default-date behavior retained. Added10 contract and
7 differential/integration functions plus five source-only manifest entries.
All unexecuted. Resume the OTHER HQ native domains/evidence/installed-wheel
consumer, not minutes; source completeness does not establish compatibility.

## Exact plan scope

New `src/helpers/{mod,market,boards,shortline,profile}.rs` (split focused files as necessary), all HQ helper DTO/query types, `tests/hq_helpers.rs`, `examples/hq_query.rs`. Migrate Python HQ `api`, `helpers`, `equity.py`, `workday.py`, `stats.py`, native bridge and manifest per completed domain; author domain differential/compatibility source tests. Implement all HQ M79–M97 including pure classify_fund_flow M95. Complete all HQ methods left by prior tasks; no exposed placeholder/Unsupported for documented valid supported HQ behavior.

- full_quotes, daily capital/limits, rank, boards/member quotes, shortline/ladder, market_stat and full profile default features. profile uses only HQ quotes/catalog/finance; no invented f10/offline dependency.
- Associate all batch sources by identity; source/date09:25/dual-stat main-date>=95%/previous-day gates before derived output.
- Required main source failures Err. Optional supplements retain successful main with explicit state/diagnostics only where defined. Rust cancellation/total deadline always Err; Python old request timeouts/optional downgrade preserved.
- Default/no-default hq_helpers and compile visibility feature cases source authored; final execution deferred.
- Replace algorithms per domain; preserve aliases/DTOs/cache/custom transport/close-reconnect generations and exception causes. Update manifest with real source/fixture/test node evidence.
- HQ example and independent Python-free public loopback consumer source; all execution deferred until T30 implementation complete. Never announce usable release before unified verification.

## Required helper algorithms

FullQuote snapshots primary, chunks80; optional depth replaces only both-side>=5 levels plus open_amount, not unrelated prices; insufficient depth Unavailable, failure Failed. Shared OperationContext across nested helpers (internal context methods, not public calls resetting entry timeout). Profile stock fields, full original finance/identity/date/raw kept; optional directory/finance fail=>None+diagnostic, zero circulating shares=>None turnover, turnover%=hands*100/shares*100, marketvalue=shares*price. Preserve Python ordered-tuple finance cache including cached failure/partial and key-order behavior; Rust cache records failure/version/expiry, doesn't claim empty success. clear_cache scoped: old client helper clear does not clear workdays until explicit workdays.clear; memory clear does not delete user files.

Rank keeps server ordering and rank starts1 across pages; countSome target=>RequestedWindow, None uses finite pages/records and true EOF; source name enrichment optional. Helper.latest_suspended returns Security rows, CodeService variant IDs. All selection Explicit(empty) stays empty and no market scan; AllAShares explicitly scans.

DailyShares identities+target date from current source, finance total/circulation separate from stat free float (no substitute), units shares and source version. DailyLimits reference most recent unadjusted day before requested trading date, then same-day tag1 `(ref-o)/m` source order, approved effective policy only; no dated authority in T03=>UnknownRule/no fabricated executable limits. Missing reference distinct from unknown rule.

Shortline: handshake dates maximum target; actual server datetime same target and<09:25=>DataNotReady before quote/finance work. Calendar supplies previous actual trading day. Each stat main-date>=95% inclusive and both tables same main date, only target/previous accepted; ResourceFormat/TdxStatsDate compatibility exceptions distinct. Missing individual row=>RowMissing/None. Previous-day stat2 remaps amount→previous_amount, seal→previous_seal, previous_seal→previous2_seal, open→previous_open. Same-day uses prior columns. Preserve exact units/formulas HQ §12: hands=open_yuan/open_price/100, free-float value=shares*last, percentage vs ratio distinction, zero denomNone;5 prior daily bar hand volumes/(240*5), insufficient5=>None. Opening-rush code-sorted category sweep only n>=80, max200x80; small request no whole-market sweep. Bars/finance/open-rush are optional with diagnostics, cancel/deadline not swallowed. Python round6 preserved. Ladder filters sealed/touched flag, final sort ladder desc/seal amount desc/id asc, count truncates final list; source-day-specific streak progression and text.

AuctionData current source date, explicit pre_close preferred; historical quote reference from0FC6 price_base not today's snapshot. Only formal OpeningMatch, no09:24 nearest substitute. Disabled all sources gives date+None fields without fake data. MarketStat required ids880005/880001/880006 identity association; Rust missing middle=>MissingData, Python preserves old positional bug until separately approved. Multipliers/rounding exactly HQ §13. classify flow ignores AuctionSnapshot but includes actual OpeningMatch/postmarket Buy/Sell; yuan=price*hands*100, strict thresholds>1e6/>2e5/>4e4, equal boundary lower bucket; unknown units/nonfinite/negative Err, empty totals0; full day pagination failure never partial totals.

Boards HQ default internal decoders: tdxzs.cfg/tdxzs3.cfg/infoharbor headers+unique member counts, industry tdxhy.cfg columns/key prefixes, DBF bounded headers/record/field/deleted/encoding. Category mappings 880industry2/concept4/style5/region3, industry X+2/4/6digits. Region requires actual base.dbf; old download list doesn't fetch it, don't pretend available. Stable first appearance member dedup with raw/display/excluded facts retained and current-catalog filtering, batch80 identity join. Optional quote failures diagnosed; directory parse failure Err. Rust BoardQuery explicit dirs or memory only, no C-drive/cwd/env scan or implicit writes at build. Python original env/cwd/C-drive search and disk caching/order retained through compatibility adapter; do not mark stale cache as today's/fresh without evidence. Clear memory doesn't delete disk.

## Python/native completion

Every HQ facade now calls shared Rust business on native normal path, including bars/minutes/trades/auctions/dailyflow/corporate/equity/calendar/resources/stats/company/session/health/helpers/subscription. Input normalization/defaults/aliases/model properties stay adapters; no copied Python business algorithm in ordinary native route. Explicit temporary custom-object adapters need manifest reason/tests/exit condition. Reuse T12 generation/bridge, T15 dispatcher (no second runtime push consumer). Old today_batch rejection, ordinary-vs-delisted raw aggregate, optional request-timeout downgrade, price-limit conflicts, market_stat positional defect, finance failure cache and unbound-workday approximation preserved. Additive protocol fields not new Python dataclass fields. GIL detach for I/O, signal cancellation/confirmed cleanup, primary exception and __cause__, RAII resource handoff survive DTO allocation failure. Rust public consumer remains PyO3-free.

Migration evidence truthful per124 capability records, root public symbol/source files and exact collectable pytest node/fixtures. No upgrading F10/EX/MAC/offline entries. Frozen snapshot/golden data not regenerated to hide API/behavior drift. All HQ requirement IDs mapped in report with actual source endpoints. Docs/mirrors reflect code-only/unverified status, optional features remain upcoming.

## Workflow and report

Workspace `C:/Users/ax/.codex/worktrees/rust-sdk-implementation/tdx抓包`, branch `codex/rust-sdk-implementation`. Until ALL T01–T30 code complete NO tests/collection/build/check/clippy/doctest/network/compile/AST/format/diff checks/equivalents. Author all suites/examples/consumer sources without running. Ordinary source reads/review diff generation/git checkpoints allowed. No subagents/push/tag/publish/merge/controller status edits; coordinate broad shared scope, commit domain checkpoints with hooks disabled so recovery is durable.

Report `.superpowers/sdd/development-plan/task-18-report.md`: all HQ M methods and Python entry mapping, algorithms/provenance/cache/optional errors, exact commits/files and authored cases/consumer, migration evidence/retained adapter list, no-execution, self-review/concerns. Return compact status/SHAs/no-tests/concerns/path; no usable/verified release claim before final evidence.
