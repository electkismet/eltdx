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

Remaining: M79–M94/M96–M97 full HQ helpers and shared context/cache/board work;
ALL remaining HQ Python/native bridge migrations, compatibility/custom transport,
actual manifest evidence, examples/consumers and source suites. No HelperService
stub or premature migrated capability was introduced by this first pure function.
Continue from this report and task-18-brief.md; do not repeat T17 implementation.
