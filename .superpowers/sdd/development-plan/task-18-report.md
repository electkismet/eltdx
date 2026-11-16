# T18 — HQ helpers and Python migration recovery report

Status: **partial source implementation only**. T17 source checkpoint88b9036 and
recovery15b1a4d precede this work. Goal is active without token quota. All tests,
builds, compilation/collection/checks, dependency resolution/downloads, network,
formatting/AST/diff checks and equivalent execution remain forbidden until ALL
T01–T30 implementation code is authored. No subagents or publication operations.

## First source checkpoint — M95 pure fund flow

- Read the T18 brief, HQ11–14 service contracts and original helpers/market.py,
  shared trade models, TradeService collection and quote context seams.
- Added helpers/mod.rs and helpers/market.rs with public pure
  classify_fund_flow(ticks) and private boundary-aware calculation seam for the
  eventual M94 network composition. No client, Python or optional feature needed.
- Added typed FundFlow and root/types reexports. Exact source thresholds use
  strict greater-than:1e6/2e5/4e4; amounts are price*hands*100 CNY. OpeningMatch
  and postmarket actual Buy/Sell trades participate; AuctionSnapshot/other sides
  skip. Reject unknown units, nonfinite/negative components and accumulation
  overflow; empty input gives zero buckets/nets. Preserve unique source stamps,
  classified/skipped counts. Added Hash to existing SourceStamp for bounded
  fallible source collection rather than quadratic source comparison.
- Authored internal boundary/threshold/net/units/overflow/cancellation source
  cases and a no-optional-feature public helper source case. Nothing ran.

Source-spec discrepancy corrected during authoring: HQ F37 lists amounts40000,
200000,1000000,1000001 but previously printed net2240001. Their sum is2440001,
matching the explicit Python/source formula. Root/mirror HQ spec and decision
record now fix the addition example only. No goldens regenerated or independent
runtime evidence claimed.

Remaining after first checkpoint: M79–M94/M96–M97 full HQ helpers and shared context/cache/board work;
ALL remaining HQ Python/native bridge migrations, compatibility/custom transport,
actual manifest evidence, examples/consumers and source suites. No HelperService
stub or premature migrated capability was introduced by this first pure function.
Continue from this report and task-18-brief.md; do not repeat T17 implementation.

## Second source checkpoint — shared-context HQ helper services

Committed as **d0f8c2c** (25 files,738 additions). Source task totals remain
18 complete/4 partial/8 unstarted. Post-commit recovery review read CorporateService,
FinanceRecord/FinanceValue, owner/calendar caches and original Python profile/
ordered-tuple finance cache. No profile/cache code has been authored yet. Next
first extract a finance collector accepting &OperationContext without setting
limits; keep the public mutable entry wrapper's limits. Profile must call that
collector after primary full_quotes, not the existing finance_in_context that
would reset limits after requests. New helper cache belongs to shared owner,
records failure/partial/version/expiry, and clear must invalidate in-flight
publication without clearing the owner's calendar. These are next-step notes,
not a claim that M86/M87 are implemented.

Authored, not compiled or executed:

- Registered lazy TdxClient::helpers() and public HelperService. New FullQuote,
  RankQuery, RealtimeRankRow root/types reexports are default-HQ, Python-free.
- M79 full_quotes: freeze all input markets before first decoding; chunks80
  primary snapshots then optional pinned depth. Both sources use the same
  context/deadline/cancellation/extra budget. Identity/occurrence association
  retains input order and duplicates. Main snapshot failures terminate; depth
  failure records Failed, missing/insufficient sides Unavailable. Only bids,
  asks and open_amount change after >=5 levels on both sides. Primary prices,
  wire fields, raw ranges and source stamp stay intact; depth provenance and
  diagnostics remain separate. Shared metadata aggregates actual responses.
- M80/M81 reuse the bounded directory collector for A-shares/ST. M82 returns
  Security records by joining their validated legacy suspension status, unlike
  the CodeService ID-only API. All these compositions own one context.
- M83/M84 reuse extracted MinuteService::aux_in_context, with outer limits
  frozen before the first child. No nested public deadline reset or finish.
- M85 uses optional name-directory enrichment before quote metadata capture;
  paginates within explicit finite pages/records/wire offset, binds later
  pages to the first endpoint, preserves source order and duplicates, numbers
  ranks globally from1. Some(count) yields RequestedWindow; None requires a
  legal empty EOF. No unproven short-page EOF shortcut. Added Table::end()
  optional completion evidence (ordinary company tables return None), recorded
  in HQ spec/decision register and mirrored documents.
- QuoteService has reusable prepare/snapshot/legacy/category/depth context
  seams. Existing public methods still own their own outer context and limits;
  pinned child release does not finish a caller's parent context. CodeService
  exposes crate-only collector/predicates; BatchItem can be consumed internally
  without adding public mutation.
- Source cases: exact depth-field preservation, insufficient/missing sides,
  structured failure diagnostics and foreign identity; lazy empty/closed/zero
  deadline/pre-cancel entry;81-row80/1 chunks with reversed snapshot/depth
  responses; optional malformed depth versus cancellation; rank argument bounds,
  server order/repeated IDs/global ranks, empty EOF versus page cap. Shared
  loopback category fixture is synthetic, not a real capture. All authored only.

No Python facade or migration-manifest status was changed here. M79–M85 have
their initial complete source paths, but T18 is still partial. Next: M86 profile
and shared cache, M87 cache clearing, M88–M94/M96–M97 remaining compositions,
then ALL outstanding HQ Python/native migration, compatibility/consumer/example
sources and exact evidence. T19–T30 remaining work still precedes any execution.
No tests/collection/build/check/clippy/doctest/network/dependency resolution/
AST/format/diff checks ran. Goal remains active without token quota; no agents.

## Third source checkpoint — M86 profile and M87 owner-local cache

Authored source and bounded source-only self-review complete; all execution still
deferred. T18 remains partial, so totals remain18 complete/4 partial/8 unstarted.

- Added ProfileQuery and StockProfile, root/types exports, and
  HelperService::stock_profile_table/clear_cache. One outer context freezes
  aggregate limits, gets required full quotes first, then optional directory and
  finance. Preserves input occurrences, original security/finance values, source
  dates/units/raw and independent Succeeded/Unavailable/Failed/NotRequested states.
- Turnover is percentage points (hands*100/shares*100); values are shares*price
  CNY. Zero/invalid denominators and unsupported units omit the affected derived
  field with diagnostics. Finite raw EPS remains F32, including negative values;
  nonfinite EPS is omitted while the original FinanceRecord remains accessible.
- Extracted CorporateService::finance_collect_in_context, which never resets
  parent limits or finishes the outer operation. Public finance retains its
  original limit wrapper/chunk300; profile uses chunk80/Fallback and keeps
  successful batches when a separate optional batch fails. Cancellation, owner
  close and total deadline are never converted into cached optional success.
- HelperCache belongs to SessionOwner and is shared by cloned clients/services.
  Catalog keys use market, finance keys the full ordered occurrence tuple; both
  include local load date, metadata version and exact RawPolicy. Complete or
  confirmed-missing results expire after900s, failures/partial failures after30s.
  Both source caches are bounded to16 entries/8192 retained row or occurrence
  weights. Oversized results are returned but not retained. Date is local load
  date, never proof of a server trading date; original source stamps are retained.
- Clear invalidates in-flight publication by generation and only clears helper
  memory. It preserves calendar/precision/session/user files. No cache lock spans
  I/O; clock reversal invalidates hits; late older version/load cannot overwrite
  newer retained entries. Catalog uses the actually published version after a
  complete scan, avoiding repeated misses. Finance conservatively reloads once
  after a primary metadata version change.
- Added source cases for units/derived fields/raw/order/duplicates, explicit
  source states, zero denominator, empty/disabled sources, expiry/clear/raw keys,
  80+1 partial finance, in-flight clear/cancellation, published catalog version,
  and calendar preservation. These are synthetic and have never run.

Next: M88–M94/M96–M97 (shares/limits/shortline/ladder/auction/market/day-flow/
boards), complete Python HQ/native migration, manifest evidence, independent
consumer/examples. M95 and M79–M87 must not be reimplemented after recovery.
No Python facade switch or manifest status upgrade occurred in this checkpoint.

The third source checkpoint above was committed as **14f590c** (22 files).

## Fourth source checkpoint — M92–M94 market/auction compositions

Authored source and source-only self-review; no execution. T18 remains partial
and global task totals remain18 complete/4 partial/8 unstarted.

- M93 market_stat: new typed MarketStat and lazy public entry. Required
  sh880005/sh880001/sh880006 rows are associated by identity, with MissingData
  for a legally absent required index, Protocol for foreign/excess identities.
  Existing public legacy strict behavior is retained through a common collector
  with a separate crate-only sparse association mode. Counter prices use
  decimal nearest-even round(value*10), capitalization*1e10 CNY, remaining
  count saturates at0; invalid/overflow/nonfinite required numbers are errors.
  Only the documented pseudo-index amount receives the field-specific CNY
  interpretation; generic legacy amount remains unchanged/unknown.
- M94 today_fund_flow: shared TradeService collector keeps its source pin,
  finite paging, repeated-page detection, cleanup and EOF evidence. Classification
  happens only after EmptyPage, under the same outer deadline/cancellation;
  failures never yield partial buckets. Preserve every page's source/raw/date,
  including empty EOF. Reuses M95; does not duplicate the classifier.
- T14 integration correction: actual A-share ticks now carry Lots using the
  existing classified code identity and PrefixFallback provenance. The ordinary
  A-share hand arithmetic is explicitly documented in the units fixture/Python
  TradeTick.trade_amount_yuan contract. Bonds/indices/other unknown assets and
  status8 auction quantities remain Unknown. This does not claim production
  unit evidence for all asset classes. Unknown actual trade units still cause
  InvalidArgument in the strict CNY classifier.
- M92 auction_data: new AuctionDataQuery/DTO/root exports. Current date comes
  from handshake date maximum, never a local clock substitute. Explicit history
  has no current-date handshake/quote fetch; reference comes from0FC6 price_base.
  Explicit pre_close wins, including zero (undefined percentage then None).
  Opening fields use formal OpeningMatch only, never a nearest09:24 point.
  Requested source failures terminate; disabled sources and source absence have
  separate diagnostics. All disabled explicit history has no startup; current
  still fetches its required source date. Unknown opening units keep raw/price/
  volume but omit CNY amount with diagnostics.
- Session handshake and auction series now have internal caller-context seams;
  TradeService has collect/opening-page variants with a supplied source date.
  Public wrappers retain their own limits/finish; nested collectors never reset
  limits or finish the parent. Current quote metadata is prepared before any
  auction/trade decoding. Helper shares the original DomainPools rather than
  constructing another owner or dedicated connection pool. Original source
  stamps are not relabeled to the aggregate target date.
- Source inspection also corrected CategoryQuote::seal_amount's Quantity
  access to value().as_f64(), matching the actual Quantity API.
- Authored synthetic public cases for reordered/missing/excess pseudo-indices,
  amounts/units/raw, complete/empty/error/cancel/repeated trade pagination,
  A-share versus unknown units, current dedicated-series+trade+quote composition,
  server-versus-local dates, historical source isolation, explicit/zero refs,
  disabled/no-formal-match/invalid/missing-date and failed/cancelled sources.
  No tests, builds, checks or runtime/network probes have run.

Recovery next: M88–M91 daily shares/limits/shortline/ladder, M96–M97 boards,
ALL remaining HQ Python/native migration, manifest evidence, independent
consumer/examples. M79–M87/M92–M95 now have initial source implementations
(13 of19 HQ helper methods); this is not completed T18 or verified functionality.
Do not repeat authored methods; continue from their shared context seams.

Fourth source stage committed as **ab5f8bf** (26 files). Tests/checks/builds/network execution remain forbidden until all T01–T30 implementation code is complete. Next shares/limits/shortline/ladder/boards, then full HQ migration. Shortline must add its stricter datetime/09:25/stat-date gates before reusing the auction source-date seam.

## Fifth source checkpoint — M88/M89 daily capital and limits

Authored source only, following recovery27dbe91. T18 remains partial; task totals
remain18 source-complete/4 partial/8 unstarted. M79–M89 and M92–M95 now have
initial source implementations (15 of19 helper methods). No Python facade or
migration-manifest upgrade, no execution, no agents, no publication operations.

- Added SecuritySelection, DailySharesQuery, DailyLimitsQuery, DailyShareCapital,
  DailyPriceLimit and public root/type exports. AllAShares performs the existing
  strict current directory scan; Explicit preserves occurrences. Explicit(empty)
  returns after admission, without clock/calendar/handshake/catalog/file I/O.
- M88 uses server current source date, observed calendar previous trading day,
  required complete stats ZIP and optional finance. Target date remains the
  server target even when accepted statistics belong to the previous trading
  day. Finance total/circulating shares and stat free float remain independent;
  preserve original units, source updated date, metadata versions/raw and row
  source-date diagnostics. Missing/unaligned free float never substitutes finance.
  Optional finance failure keeps stats-derived fields; cancellation/outer deadline
  still terminate through the shared profile-finance seam.
- Added helpers/stats_alignment.rs: both tables must independently have at least
  95 percent coverage, including undated/invalid-date rows in the denominator,
  use the same main date, and match target or previous observed trading day.
  Disagreement/low coverage/no date are ResourceFormat; an inadmissible common
  day is ResourceFormat with code=stats_date_mismatch for the later legacy
  TdxStatsDateError mapping. No local-clock replacement for server dates.
- Statistics cache reuses owner/generation/TTL/RawPolicy infrastructure. Key
  includes full StatsQuery, target and previous days; local load date is only
  cache context. Only validated success is published; explicit refresh downloads
  anew and failure never silently returns or relabels old data. Bound to2 entries
  and65536 weights (rows plus retained raw bytes/4KiB); oversized delivered but
  not cached. clear invalidates old in-flight stats publication too.
- M89 validates an observed trading date, requires complete unadjusted daily
  source collection, takes maximum source date strictly before target rather than
  trusting row order, detects conflicting closes on that date, and applies only
  same-day tag1 events in original source order using shared `(ref-o)/m`
  coefficients. Keeps original raw/source metadata. Empty/invalid prior reference
  yields MissingReference, independently retaining Unknown rule evidence.
  With a reference, the deliberately empty approved registry yields UnknownRule
  and no upper/lower prices; no guessed10/5/20percent executable rules.
- Extracted resource read_stats_in_context, corporate capital_collect_in_context,
  Kline collect_with_parent_context and calendar load_in_context. These retain
  original service wrappers while nested helper calls never reset consumed
  limits or finish a parent; download/parse/calendar/finance/reference/action
  passes share outer cancellation/deadline/pin cleanup.
- Authored private arithmetic/date/cache cases and public hq_helpers support
  cases for empty/closed/cancelled/expired entry, server-vs-local date, three share
  sources, cache/reload/clear, required dual-stat date failure before finance,
  finance degradation versus cancellation, ordered actions/reference/unknown
  policy/missing reference, and nontrading-date rejection. All remain unexecuted.

Next: M90 shortline/M91 ladder, M96/M97 boards, then all HQ Python/native
migration/evidence and consumer/example sources. Shortline readiness MUST require
usable server_datetime and its09:25 gate before quote/finance work; the new
daily/auction source-date fallback alone is not a readiness proof. Do not repeat
M88/M89 or reset caller budgets in the new internal service seams. Final full
compilation, behavior, compatibility, packaging and performance evidence remains
outstanding until all T01–T30 implementation sources are authored.

Fifth source stage committed as **4823fb8** (25 files). No execution validation.

## Sixth source checkpoint — M90/M91 shortline and ladder

Authored after4823fb8, on2026-11-17. M79–M95 now have initial source
implementations (17 of19 HQ helper methods). T18 remains partial; totals still
18 source-complete/4 partial/8 unstarted. No Python or migration-manifest upgrade.

- Added public ShortlineQuery/LadderQuery/ShortlineTable/ShortlineIndicator and
  StatsAlignment, in types/shortline.rs and crate-root reexports. Nonempty
  ShortlineQuery preserves occurrences. Explicit(empty) ladder admits and returns
  without clock/session/files/catalog I/O; closed/cancelled/expired entry errors
  still apply. Count zero is invalid; final count truncates after selection/sort.
- M90 requires a date-and-clock server datetime and maximum valid explicit
  handshake source date. No datetime-date substitute for absent source dates.
  Same-target server time before09:25 is DataNotReady before other source work.
  Previous date uses the observed calendar; both statistics files reuse the
  shared inclusive95percent/date alignment/cache gates before quotes or finance.
- New QuoteService prepare_composition_in_context captures all explicit quote
  IDs and, for>=80 occurrences, category's three-market scope before calendar or
  finance starts decoding. Later children retain one immutable capture, parent
  work allowance, cancellation and absolute deadline. KlineService gained a
  single-page parent-context seam, with original standalone limit/finish intact.
- Required full quotes preserve identity/occurrences. Optional finance cache,
  recent six unadjusted daily bars, and category opening-rush failures are
  diagnosed; owner close/cancellation/outer deadline still propagate. Bars are
  fetched once per distinct identity; first-occurrence order determines metadata
  aggregation. Five latest distinct prior source dates are required for the
  hand-volume/(240*5) baseline; missing/zero/conflicting/unknown units yield None.
- Stat2 previous-day mapping uses current amount/seal/opening and previous seal
  as prior fields; same-day uses prior columns. Missing or out-of-range individual
  rows retain source-date diagnostics and None fields without substituting another
  security. Finance circulation and stat free float remain independent. Derived
  percentage-point fields, fractions, shares, hands and CNY are explicitly typed;
  zero denominators/overflow do not produce nonfinite output. Generic quote amount
  currently retains Unknown currency units, so seal_to_amount remains None for
  those sources with an explicit diagnostic; no implicit CNY relabeling. Python
  round6 and legacy amount projection belong to the later compatibility seam.
- Opening-rush sweep only for>=80 occurrences: ascending Code/category6,
  maximum200 pages x80, first endpoint retained, short page not EOF, repeated
  identity-page detection independent of raw policy. Valid matched supplements
  survive an optional later failure; unresolved identities carry Failed with
  error kind and never become confirmed missing. Small input is NotRequested.
- M91 filters Sealed/Touched according to include_touched, sorts level descending,
  seal amount descending and full ID ascending, then truncates. Pure source-day
  progression keeps original counters unchanged and guards u32 overflow. The
  approved policy registry is deliberately empty, so production rows currently
  remain UnknownRule; empty ladder carries explicit unclassified occurrence count
  and is not proof of no limit stocks. Synthetic approved-price cases exercise
  sealed/touched/not-at-limit arithmetic without registering an exchange profile.
- Authored7 private source cases and8 public loopback cases: readiness/09:25,
  stat remapping, ratios/units, missing/unaligned rows, five distinct prior dates,
  synthetic limit classification/progression, final ladder ordering/truncation,
  lazy entry, required resource gates, repeated occurrences/raw, optional failures
  versus cancellation, three-market metadata capture, partial category supplement
  failure and unknown-policy empty ladder. All are source only, unexecuted.

Next: M96/M97 board file parsers/explicit sources/cache/member filtering/quotes,
then ALL HQ Python/native migration, truthful manifest evidence and independent
consumer/example sources. Region must require actual base.dbf; reuse shared GBK
and default-HQ internal bounded parsers, never add a mandatory offline dependency.
No tests, collection, cargo/dependency/build/check/clippy/doctest/format/AST,
network probes, diff-check or equivalent execution occurred. No agents or remote
publication operations. Goal remains active with no token budget.

Sixth source stage committed as **f6a99a8**. All execution validation is deferred.

## Seventh source checkpoint — M96/M97 boards and member quotes

Authored on2026-11-17 afterf6a99a8. All M79–M97 (19 of19 helper methods)
now have initial Rust source implementations. T18 is still partial: no claim of
completed Python/native migration, compilation, behavior or compatibility.
Task totals remain18 source-complete/4 partial/8 unstarted; goal stays active
without a token budget. No subagents or remote publication actions.

- Added types/boards.rs, helpers/board_decode.rs and helpers/boards.rs with
  crate-root exports. BoardId is a six-digit lookup with optional explicit HQ
  market. BoardQuery is lazy and explicit, default Concept/refresh false. Named
  in-memory BoardFile bytes override explicit directories; only five fixed
  filenames are admitted. Each source is capped32MiB, aggregate64MiB. There is
  no ambient path discovery or disk write. Explicit definitions_dir requires
  both definition files; base.dbf must be a real explicit source, never a
  fictitious third HQ download.
- Source preparation loads the same complete fixed source set irrespective of
  requested category, enabling cross-category memory reuse. Missing HQ pair
  members are downloaded through the existing pinned ResourceService. Optional
  counterpart failure keeps an independently complete requested source, retains
  Failed/error_kind diagnostics, and shortens cache expiry. Missing selected
  membership sources are ResourceFormat, not an empty successful board table.
- Internal default-HQ parsers share the existing GBK table, with no dependency
  on optional offline/f10. Definition first occurrence wins. Known category/key
  mappings include X+2/4/6-digit industry depth; unknown mappings are excluded
  with diagnostics rather than guessed Concept. Inferred market is diagnosed.
  infoharbor verifies unique actual counts, rejects repeated sections and
  preserves stable first occurrence. Industry uses column2/5 prefix matching.
  DBF checks header/record spans, duplicate/bounded fields, terminator, truncation,
  deleted records and ASCII selected SC/GPDM/DY fields. Limits:65535 boards,
  one million aggregate concept members,32KiB text lines. Parser loops retain
  the caller's cancellation/deadline boundaries.
- M97 requires successful current three-market directory collection, keeps raw,
  display and excluded members in source order, and records the absent reason in
  raw and excluded rows. Only current display members request quotes. Both
  methods join snapshots by identity,80 per wire batch; quote absence stays
  Unavailable, source failures Failed, cancel/close/outer deadline Err. Aggregate
  diagnostics expose confirmed missing counts as well as per-row states. Above
  65535 members, metadata preparation captures all HQ markets instead of
  degrading a local public-batch-size limit into failure for all members.
- Owner cache key includes all explicit source bytes/paths and target date;
  category/refresh are normalized because preparation loads all sources. Stamp
  retains local load day and RawPolicy. Complete successes expire900s, successful
  sources with counterpart failure30s; at most2 entries/131072 weights include
  definitions, parsed concept members and file/raw bytes per4KiB. Oversized data
  is returned without retention. Memory clear also invalidates old in-flight
  publications. Refresh rereads selected sources and never relabels stale data.
  Local/memory/download source dates remain explicitly unproven; prepared_date
  and load_date do not certify file freshness.
- Authored8 public source cases in tests/support/board_cases.rs, registered in
  hq_helpers.rs: lazy/rejected entry, explicit memory and reversed quote identity,
  member filtering/raw facts, corrupt/confirmed-missing/cancelled optional quotes,
  required directory/membership/DBF failures, fixed downloads/cache/refresh/clear,
  partial-source short TTL, cross-category cache and81 members in80+1 batches.
  Added6 parser,1 member-filter and1 cache private source cases. Corrected5
  earlier daily helper authored cases to use the actual Table::rows getter and
  corrected draft identity expectations by constructing typed SecurityIds.

All source cases remain unexecuted. No tests/collection/build/check/clippy/
doctest/dependency resolution/network/AST/format/diff-check commands ran. Next:
ALL HQ Python/native domain adapters, compatibility/differential source cases,
truthful migration manifest evidence, independent consumer and HQ example. Do
not repeat the19 authored helper methods or mark T18 complete before these
remaining requirements and the later unified verification are satisfied.

Seventh source stage committed as **1bb3c1a** (18 files). All execution deferred.

## Eighth source checkpoint — initial pure Rust example and consumer

- Authored crates/eltdx/examples/hq_query.rs. Explicit server/security arguments,
  default HQ only, finite whole-operation timeout, full-quote diagnostics/typed
  units and explicit close on success/failure. The primary business error stays
  primary when cleanup also fails. No server is contacted while authoring.
- Added tests/consumers/rust-sdk with an independent Cargo workspace and one
  direct dependency, eltdx. A std-only local server authors its own framing and
  fixed expected prices without importing protocol/runtime or repository test
  helpers. The consumer covers lazy build/getters/double-close plus actual
  nonempty 0/1/2 directory pages, reversed snapshot rows with repeated input,
  identity/price/source/raw preservation, single connection and service rejection
  after close. It closes the owner and stops/joins the fixture on business failure.
- Consumer README distinguishes a standalone manifest inside the repository from
  actual external/package/registry consumption. The latter, dependency-graph
  inspection, feature/MSRV/platform matrix and installed-wheel consumer evidence
  remain outstanding. No lockfile or dependencies have been resolved, no example
  or consumer has been compiled, executed or used as passing evidence.

Resume with full HQ Python/native domain migrations, authored compatibility
cases and truthful manifest evidence, then installed-wheel consumer source.
Do not count the new Rust consumer as completion of T28. Task totals remain18
source-complete/4 partial/8 unstarted, goal active without quota; all execution
validation remains forbidden until all T01–T30 implementation sources are done.

Eighth source stage committed as **39c8e3e**. All execution deferred.

## Ninth source checkpoint — ordinary and delisted Python/native bars

- Added one shared `services/bar_collection.rs` walker and ordering function.
  Public Rust collectors now use that walker with strict finite caps, repeated
  payload rejection, count checks, unchanged wire-offset errors and raw-row
  permutations. Legacy uses the same loop with its original optional page cap,
  repeated-page acceptance and per-wire execution profile. Short pages continue;
  only an empty response proves EOF. Ordinary pages reverse page order without
  reversing rows; delisted bars sort stably by time, including equal timestamps.
- Added `LegacyOperation::bars`, `LegacyBarSeries` and `LegacyBarTime`. Binding
  requests reuse the old `request::from_python` extractor. The compatibility
  operation neither prefetches a security directory nor pins the full history.
  SDK permits Kline/Delisted requests through the fixed-scale unpriced decoder.
  First response metadata/anchor stay first; ordinary aggregate payload clears,
  delisted aggregate payload stays first, including an empty initial history.
  Aggregate request_count is separate from the validated per-page wire request.
- One timestamp shift mechanism has explicit AdvanceDate and SameDate policy.
  Rust public End keeps elapsed rollover; Python End wraps hour/minute within
  the original date and preserves seconds/timezone/raw/time_raw. Invalid string
  bar_time retains the exact repr message and is raised only after first decode;
  a malformed first response therefore still wins. Existing max_pages RuntimeError
  text/class/cause are preserved by a narrow LimitExceeded projection.
- `NativeEngine.sdk_bars`, service worker and both native Python transports use
  the existing detached-GIL/signal cancellation shell and tuple/model ABI. No
  Python model field was added. TdxClient enables native routing only for exact
  built-in transport types; subclasses retain execute extension behavior.
  Python sequence normalization, stable dedup, input order and pool-bounded
  concurrency remain adapters. Existing BarApi float/bool/unlimited page-limit
  comparisons are normalized without changing their reachable stopping boundary.
- Temporary callback adapters explicitly remain for custom transports/untyped
  responses and nonstandard scalar objects/evaluation semantics. Manifest
  records7709-kline/7709-delisted-bars include real method/source paths, exact
  authored pytest nodes, fixtures, reasons and exit conditions. Their migration
  statuses are deliberately not promoted before the complete HQ evidence pass.
- Added2 private ordering source cases,9 contract functions and5 parametrized
  local integration functions. The integration source compares the existing
  custom facade with the native public client for ordinary/delisted, raw true/
  false and Socket/Pooled transports. It includes repeated equal-time delisted
  pages, short-page offsets, same-date midnight wrap, empty raw/metadata,
  RuntimeError limits, invalid bar_time ordering and exact error message.

No tests, collection, build/check/clippy/doctest, dependency resolution/download,
network probe, AST/compile, formatting, diff whitespace check or equivalent
execution was performed. These are authored sources only, not compile/runtime
success or verified compatibility. T18 remains partial; totals18 complete
source/4 partial/8 unstarted. Next: minutes and all remaining HQ native domains,
the full truthful migration evidence pass and installed-wheel consumer sources,
then T19–T30 and remaining Stage B work. Goal active without a token quota.

Ninth source stage committed as **b5ed0b5**. All execution deferred.

## Tenth source checkpoint — Python/native minute domain

- Added shared `fetch_minute_reply` in `services/minutes.rs`. Both public Rust
  MinuteService and the legacy operation use one domain admission/request/decode
  boundary and response-variant check for today/history/recent/aux/sparkline.
  Strict Rust identity, selector, count and nonnegative-volume assertions stay
  in its typed projection. Legacy retains original protocol scalars, f32
  previous-close/open/base/prices and negative volumes; it does not acquire
  those new assertions or an aggregate whole-query timeout.
- `LegacyOperation::minutes`, `NativeEngine.sdk_minutes`, the detached-GIL
  service worker and Socket/Pooled transport adapters now connect all five
  ordinary native MinuteApi entry points. The original request extractor and
  tuple/model projection are reused; no Python dataclass field was added.
  Recent's omitted date is still resolved by the original binding on each
  request, including each batch member.
- Price resolvers retain pre-request and post-result invocation, the original
  input string and their return projection. Metadata preparation uses market
  inference on that original input, not the normalized request identity. Bare
  019xxx still selects sz for metadata while its wire identity is sh; no extra
  sh scan is introduced. Completed CodeApi metadata publication is reused.
  Aux and sparkline do not prepare metadata or invoke the price resolver.
- Sequence normalization, first-occurrence dedup, ordered dict output and
  Python worker selection stay adapters. Explicit batch_size is not capped to
  pool capacity; omitted batch_size uses original capacity. Single-security
  methods continue to ignore this parameter. Custom transports and transport
  subclasses retain their execute extension and arbitrary response escape.
- Explicit parse contexts preserve row ranges without eagerly generating hex.
  The legacy minute seam now hydrates minute points and both auxiliary layouts
  from retained payload ranges when include_raw=True. The same missing raw
  hydration was identified in the preceding ordinary/delisted Kline seam and
  corrected before alignment/page aggregation, retaining first-page raw rules.
- Legacy HQ decode errors now carry the original RuntimeError conversion so
  Python receives the existing protocol text, class, context and empty cause;
  public Rust continues its strict structured decoding error path. Existing
  run_legacy signal cancellation and cleanup are reused rather than starting
  a second execution or signal lifecycle.
- Authored10 contract functions in test_sdk_minute_bridge.py, including all
  routes, resolver order/projection/failure, unpriced aliases, first-occurrence
  keys, explicit/default workers, ignored single batch_size and custom execute.
  Authored7 local differential/integration functions in
  test_native_minute_bridge.py: both transport types and raw policies; all three
  priced wire families at decimal2/3; negative volume; both auxiliary layouts
  and unknown selector; mismatched sparkline identity/echo/window; exact protocol
  exceptions; default-date selector; original bare-bond inference; and signal
  cancellation with confirmed cleanup. These are unexecuted source obligations.
- Added truthful source-only authored_evidence for the five minute capability
  records, with actual Rust/Python seams, fixture paths, exact authored pytest
  nodes and temporary custom-adapter exit conditions. Migration statuses remain
  planned/unimplemented until the complete evidence pass, not falsely verified.

No tests, collection, builds/checks/clippy/doctests, dependency resolution or
downloads, network probes, compilation/AST, formatting, whitespace diff checks
or equivalent execution ran. T18 remains partial; totals18 complete source/
4 partial/8 unstarted. Resume the remaining HQ native domains (other quotes and
codes, trades, auction/money flow, corporate/equity/calendar, resources/stats/
company/session/health, helpers and remaining subscription integration), the
manifest-wide evidence pass and installed-wheel consumer before T19–T30 and
remaining Stage B. Do not rewrite the already authored19 helper methods, bars,
minutes or example/consumer sources. Active goal has no token limit.

Tenth source stage committed as **ac99b99** (21 files). All execution deferred.

## Eleventh source checkpoint — session and company information

- Added shared domain request/decode boundaries `fetch_session_reply` and
  `fetch_company_reply`. Public Rust services and the Python legacy operation
  use those same functions. Rust health.probe already delegates to session's
  heartbeat_in_context and therefore also reaches the shared session boundary.
- Authored LegacyOperation::{session,company_info}, the existing detached-GIL
  worker shell with retained payloads and unchanged tuple projection, native
  engine/transport entry points and exact built-in client routing. Neither
  domain prefetches a security directory, creates another network executor or
  resets the per-request compatibility timeout to a whole-operation timeout.
- Session handshake/heartbeat keep raw payload unconditionally, original
  naive server datetime, control fields, invalid-date None projection and
  runtime Slot session snapshots. Runtime's normal close/reopen retains the
  Python NativeEngine holder while creating a new runtime epoch; no adapter
  assumes transport._engine is cleared or permits a stale pin to revive.
- Company queries keep explicit market precedence over a conflicting prefixed
  code, the original filename encoding/extraction and byte offsets. Legacy
  text keeps malformed-GBK ignore, removal of every NUL and trim, including text
  after an embedded NUL. The new Rust projection retains its separate lossy
  diagnostics and range checks; legacy does not acquire sum-overflow or
  requested-length assertions and still accepts otherwise valid trailing data.
- Local session.ping deliberately remains the transport.request adapter. It is
  callable without loading/constructing the native engine, including after
  close, and user request overrides keep working. This small local diagnostic
  adapter is recorded explicitly rather than changing Python startup behavior.
  Custom transports and transport subclasses retain execute and untyped results.
- Authored7 contract and4 local differential/integration functions covering
  both built-in transports: routes, result/exception identity, local ping,
  custom extension points, nonempty handshake/heartbeat raw/control fields,
  actual session snapshots, invalid dates, company GBK/NUL/byte offsets and
  conflicting market, exact protocol failure class/text/cause, and Python
  close/reopen epochs. Added source-only evidence to four capability records;
  migration states remain unchanged. These sources have not been collected,
  compiled or executed.

No tests/collection/build/check/clippy/doctest/network/dependency/AST/format/diff
checks or equivalent verification ran. T18 is still partial and overall counts
remain18 source complete/4 partial/8 unstarted. Resume directory count/list and
selection/statistics, then the other HQ/native domains, whole manifest evidence
and installed-wheel consumer. This checkpoint does not complete health/probe
ranking migration or the entire Python lifecycle audit. Goal active, no quota.

Eleventh source stage committed as **2cbcd53**. All execution deferred.

## Twelfth source checkpoint — directory requests and selection

- Added `fetch_directory_reply` as the count/list request/decode boundary for
  public Rust CodeService and Python LegacyOperation.directory. The existing
  shared `walk_directory` still owns scanning and pagination. Both built-in
  Python transports expose the original extractor/tuple projection through
  sdk_security_request; arbitrary custom transport results and metadata sinks
  retain the old adapter. A list limit of zero still sends a request, and the
  old binding still supplies its local current date for count.
- Added shared CodeSelection for stock/A-share/ETF/index selection and ST names.
  Pure native selection returns indices into the original Python list, retaining
  row object identity, order and duplicate occurrences. It does not initialize
  a network Engine. Unicode uppercase and Python's extra strip characters are
  retained for legacy ST matching; typed Rust keeps its existing ASCII policy.
  latest_st remains a name-only filter after latest_stock_list, preserving a
  user override that supplies a non-A-share row. Cross-market order stays
  sh, sz, bj. latest_suspended is deferred to the remaining quote integration.
- Legacy SecurityList decode invokes the original protocol parser so each
  parsed row still registers its precision in the old process registry used
  by unmigrated execute commands. Earlier rows retain that side effect if a
  later row fails. Strict Rust still uses immutable context snapshots and its
  separate row identity/duplicate/count checks; legacy acquires none of these.
- Custom row/container/field behavior stays an explicit adapter. all_* ID
  comprehensions retain category then full_code per row, including exception
  order and fields with user formatting hooks; native IDs require exact string
  identifiers. Counts retain original sum/equality behavior for non-bool custom
  equality. Empty lists avoid loading the pure selector. This source review
  corrected a draft select-then-project path that changed custom property order.
- Authored11 contract functions,7 local differential/integration functions and
  one private Rust selection case. Coverage includes both native transports,
  zero limits, duplicate/over-limit rows, raw fields/trailing data, date source,
  market order, row identity, Unicode ST, pure no-engine selection, original
  protocol exceptions, precision registry publication (including later-row
  failure), arbitrary sink results and custom attribute/equality ordering.
  Two manifest records now carry actual source/fixture/exact-node evidence and
  adapter reasons/exit conditions; their migration statuses remain unchanged.

All sources remain uncollected, uncompiled and unexecuted. No tests, builds,
checks, dependency resolution/download, network probes, AST, formatting, diff
whitespace checks or equivalent verification ran. T18 stays partial and overall
counts remain18 source complete/4 partial/8 unstarted. Resume remaining quote
entry points/latest_suspended, then trades/auctions/dailyflow/corporate/equity/
calendar/resources/stats/health/helpers/subscription, complete manifest evidence
and installed-wheel consumer. Do not rewrite already authored domain/helper
sources. Goal stays active without a token limit; no subagents or publication.

Twelfth source stage committed as **6fd1e05**. All execution deferred.

## Thirteenth source checkpoint — remaining quote requests and pinned refresh

- Added shared `fetch_quote_reply` for legacy quotes, category rows and refresh.
  Public Rust legacy/category/depth/borrowed refresh and CodeService suspended
  collection use the same domain boundary as LegacyOperation.quotes. The Rust
  strict projections retain their existing identity/window/quantity checks.
  A shared suspended_status predicate keeps the 0x20 rule in one location.
- NativeEngine/NativePin expose sdk_quotes; both built-in Python transports and
  PinnedTransportProxy project the unchanged tuple ABI. NativePin retains the
  same LegacyExecutor/Engine identity as its creating NativeEngine, and sends
  through the existing pin with no independent reservation or second consumer.
  CodeApi.latest_suspended calls sdk_suspended in its original80-row chunks;
  empty lists make no requests and results retain server order/foreign identities
  just as the old facade did. The old request extractor remains authoritative.
- Python resolver calls still occur before request extraction and after model
  projection. No implicit second metadata scan was added: no-resolver QuoteApi,
  pinned refresh and latest_suspended retain their old no-prefetch behavior.
  Standard client category queries retain bj/sh/sz precision scan order, and
  subscriptions load metadata before reserving the pool's pins. Standard group
  refresh APIs now use the Rust boundary; custom API/transport subclasses keep
  their old execute route. Callback/poll orchestration remains pending T18 work.
- The legacy quote decoder deliberately uses the original protocol parser for
  these three request families. This preserves process-registry precision,
  saturating scales, negative/duplicate/foreign rows, optional tails, record
  hex and the complete refresh XOR-decoded/raw byte views. A first draft of the
  source tried immutable preparation and later raw hydration; review replaced
  it with the narrower original-parser compatibility boundary. Direct Rust
  calls still decode against their captured immutable metadata snapshot.
- Added9 contract functions and9 local differential/integration functions.
  Authored cases cover built-in/custom routing, original resolver order and
  failure, copy/merge of cursor inputs, count0/start65535 category semantics,
  duplicate/negative/raw rows at decimal2/3/9, no-resolver no-prefetch, exact
  protocol exception type/text/cause, pinned refresh/closed-proxy guard,
  subscription prefetch before pool1 pin, empty refresh byte views, cursor-only
  identities,80-row suspended chunks and pinned KeyboardInterrupt cleanup.
  Three capability records now have real source/fixture/exact-node evidence and
  explicit retained adapters; migration statuses remain unpromoted.

No tests, collection, build/check/clippy/doctest, dependency resolution/download,
network probe, AST/compile/format/diff-check or equivalent verification ran.
These are authored sources only, not compile/runtime/compatibility evidence.
T18 remains partial and counts remain18 source complete/4 partial/8 unstarted.
Next: trades/auctions/dailyflow/corporate/equity/calendar/resources/stats/health,
all helper/remaining subscription migration, complete manifest and installed-wheel
consumer, then remaining T19–T30/Stage B. Goal active, no quota or subagents.

Thirteenth source stage committed as **d3c1215**. All execution deferred.

## Fourteenth source checkpoint — trade pages, collection and opening matches

- Added shared TradePager and fetch_trade_reply for strict Rust and legacy Python
  today/historical ticks. Both advance by actual rows and reverse pages without
  reversing rows within a page. Strict Rust retains response-count/finite-base
  validation, repeated-page rejection, cursor order and u16 continuation limits;
  legacy preserves repeated/over-count pages, original max_pages RuntimeError,
  next-request start-range ValueError and no aggregate deadline or history pin.
- NativeEngine/LegacyOperation/service and both Python transports expose one-page,
  collection, historical bulk and opening-match routes. TdxClient enables them
  only for the already supported exact built-in transports. Intrinsic tick units
  need no metadata prefetch. Legacy decoder uses the original parser, preserving
  cent/milli prices, optional record text, raw payload, nonfinite historical base,
  out-of-day today labels and historical naive datetime validation.
- Aggregation preserves first-response metadata/base and an independent usize
  request_count (which may exceed legal per-page counts), clears aggregate raw,
  and retains each original historical bulk block including the empty terminal
  block. A private historical_trade_batch DTO keeps equality/materialization
  behavior without constructing Python TradeTick objects. The existing today
  bulk rejection is retained. Opening finds the first formal match, skips status8
  even at09:25, terminates before a page-limit failure and retains only its current
  page rather than all earlier search rows.
- Exact builtin code/date/page-size/raw/max-page arguments use the shared native
  collector. Custom scalar/date objects and instance/class replacements of public
  page methods keep per-page Python extraction and callbacks. Existing batch
  normalization/dedup/order/capacity remains. all_history(date=None) still chooses
  today, while all_history_batch(None) still reaches historical date extraction.
  Float ceil/NaN/infinity/large max-page semantics remain as in the old loop.
- Authored14 contract functions,12 local integration/differential functions and
  3 private Rust cases. Sources cover single/row/bulk routes, custom extraction
  and method overrides, equal-time/repeated/short/over-count pages, initial base,
  raw/units/date semantics, aggregate counts/blocks/no Python rows, empty-vs-limit
  ordering, offset overflow, first opening match, protocol exceptions and request
  cleanup after KeyboardInterrupt. Four capability records have source-only
  evidence, retained adapters and exit conditions; no migration status promoted.

No tests, collection, build/check/clippy/doctest, dependency resolution/download,
network probes, AST/compile/format/diff-check or equivalent validation ran. Source
reads and ordinary Git diff/status only; these changes have no compile/runtime
evidence. T18 remains partial; totals18 source complete/4 partial/8 unstarted.
Resume auctions/dailyflow/corporate/equity/calendar/resources/stats/health and ALL
helper/remaining subscription migration, complete manifest and installed-wheel
consumer, then T19–T30/remaining Stage B. Goal active without token cap; no agents
or publication. Preserve this checkpoint rather than rebuilding prior domains.
