# T20 source implementation report

## First checkpoint — concrete endpoint source foundation

Prior committed checkpoint: `a0893db` (T19 bounded F10 retention and shared
connect probes). Goal remains active with no token quota. All execution
validation is deferred until the complete T01–T30 implementation is authored.

Authored `f10/values.rs`, `news.rs`, and `endpoints/{mod,international,bonds,
sectors,reports,news_queries,valuation,market}.rs`; wired exports and offline
`parse_response`. Macros emit distinct public query structs and named methods.
No generic `call` substitution is counted as an implemented convenience API.

- Explicit A-share, US, HK, third-board, bond, industry and futures identities;
  method-scoped sections, pagination selectors, date/window values. Bond
  default market and six-digit HK wire identity preserve their provenance.
- Initial US17/HK20/SB13, bond31, industry13/futures8 endpoint sources; report
  branches and report-period prerequisites; all keep wire parameter order and
  numeric-versus-string distinctions in source.
- Eight home tabs and 84 news channels, second-table extraction, cache request
  bodies, topic prerequisite, theme/valuation specialized wrappers.
- Market report branch table, index/tracking/board collectors and raw-backed
  typed rows. This is still under source review, not a compiled API claim.

Remaining source work, in priority order:

1. Finish market collector lifetime/metadata/raw bounds; typed projection must
   run before the original context finishes. Board keys need cancellable,
   bounded traversal; prerequisites must include retained raw provenance.
2. Complete limit_up_down_list/limit_board_ladder and typed per-date statistics,
   local-date source, unknown units and invalid-number/date diagnostics.
3. Full scoped fields registry from both Python dictionaries, preserving
   unknown fields and the Entry/section/report-type context.
4. Historical money_flow history/range through F10, requested days, ascending
   date order and start/count slicing; never category22 substitution.
5. Complete per-entry/branch route and result test sources; scrutinize report
   date validation, defaults, closed sections and all 157 public entries.
6. T22/T23 and remaining extension/consumer/package/docs implementation.

Status: 19 tasks source-complete, five partial (T19/T20/T22/T24/T25), six not
started. T19 is still partial because source handoff and dependency/platform
evidence are not complete. No tests, collection, build, check, clippy, doctest,
AST/format/diff checks, dependency resolution/download or network verification
ran. No new agents, push, tag, publication or merge.

## Second checkpoint — parent budget and composed provenance

First endpoint foundation committed `5a4185c`. Added one configurable 64 MiB
logical aggregate limit to F10 config/builder. OperationSponsor carries the
parent WorkLimits and charges its existing byte counter while decoded bytes,
retained encoded payload and retained HTTP plaintext are streamed, before
their allocation. No separate operation/deadline/counter is created.

MetaCollector merges physical-response-order source/raw handles with parent
cancellation checks; report/topic prerequisites retain both responses' raw
payloads and diagnostics. Market collection projects each typed row before
finishing the original context, retains cancellation during totals and record
keys, and uses fallible identity-key storage. Corrected the row string helper
lifetime. Business and fund prerequisites select the first row only, validate
the actual Gregorian report date, and reject failed/absent metadata before a
second request. Authored aggregate-limit and two-request raw source cases;
these have not been executed.

The remaining ladder/fields/history/full route cases and later T22–T30 work
remain incomplete. Counts19/5/6 and active unlimited goal unchanged. No test,
build, dependency, network, formatter, AST or other execution validation ran.

## Third checkpoint — limit-board typed source and initial route/result cases

Added both distinct query structs and public methods for limit_up_down_list /
limit_board_ladder. Explicit/repeated/local-clock dates are distinguished;
injected LocalDateClock is lazy, and SystemLocalDateClock uses provisional
chrono0.4.42 rather than changing HQ's Shanghai clock. Date selection is fixed
once inside the public operation; optional summary shares the same parent.

Raw-backed rows preserve every native field; status1/3/0 and unknown remain
distinct, positive type1 levels alone enter per-date counts. Invalid dates /
numbers / unsupported market values are diagnosed without discarding raw.
Stats preserve contradictory counts; promotion ratios outside0..1 are omitted
with diagnostics. Native seal/summary amount units are not advertised as yuan.
Detail and summary business status and original additional result sets remain
independently accessible, with all source/raw metadata aggregated in order.

Authored f10_routes.rs for explicit ladder dates, market aliases, report
periods and pagination bounds; f10_results.rs for nonempty multi-date ladder,
unknowns/invalid fields, summary business error, raw and injected local date,
plus lossless duplicate columns and actual second-table news. HTTP source
cases cover aggregate raw-byte limits and two-request report provenance.
These are initial cases, not complete157-method coverage; none has executed.

Still to implement: full scoped fields registry, historical money flow,
complete endpoint/branch route inventory and remaining result projection
details. Counts19/5/6; all later source stages remain required. No execution
validation, dependency resolution, new agents or publication occurred.
