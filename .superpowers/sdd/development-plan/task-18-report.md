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
