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
4. Historical money_flow history/range through F10, requested days, reversal
   of source rows followed by start/count slicing (COMP-050, no sort); never
   category22 substitution. The earlier ascending-order wording was erroneous.
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

Committed as `2a9e05d`; preceding endpoint foundation is `5a4185c`.

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

Resume note: read both Python `fields.py` and `fields_mg_hk.py` in full after
this commit. Next migrate their complete literal registries, dictionary
inheritance/override order, two cash-flow notes and exact scope routing. All
four A-share statement templates require nhytype; bank cash-flow T088 has
only a dynamic-field note and must not inherit the balance-sheet T088 label.
MG/HK/SB labels use their declared scope index and wildcard fallback. The
non-prefix analyst forecast mapping exists in the MG/HK helper registry but
the top Python dispatcher does not route it there; retain this compatibility
distinction separately from the new Rust explicit field resolver. No field
registry source has been written yet, and no runtime was invoked to extract it.

## Fourth checkpoint — full scoped field dictionary source

The preceding resume note is now superseded: `f10/fields.rs` and
`f10/fields/data.rs` are authored. The data file was transcribed by
`tools/generate_f10_fields.ps1`, which reads literal pairs/includes without
importing, evaluating or running either Python module. That authoring command
did run; it performs no AST/build/test/format/runtime/network validation.

All source dictionaries are represented, including include/override order,
four balance/income/cash-flow templates, all MG/HK labels and cash-flow notes.
FieldContext takes Entry/request scope and explicit FieldMetadata for nhytype
and reporttype; conflicting known report families select no known labels.
No schema is inferred from ticker or a globally shared N001/Txxx meaning.
Unknown fields/units remain unknown. Bank cash-flow T088 has only its dynamic
note; other templates keep their own T088 meaning. Literal raw field names and
Python suffixed projections use separate describe functions, so a real raw
field called T039__2 is not silently reinterpreted.

The new Rust explicit resolver also reaches the declared HK analyst-forecast
mapping despite its nonstandard Entry prefix; later Python migration must
retain the original dispatcher behavior. Response::field_context requires
explicit metadata rather than guessing which table contains a template.

Authored three public field test groups spanning all12 A-share statement
templates, duplicate/unknown semantics, conflicting metadata, include order,
MG/HK scope position and wildcard behavior. All remain unexecuted.

Next: historical money_flow history/range through F10; complete the157 named
method/branch request fixture inventory and remaining typed model projection
details; then T22/T23/T24–T30. Counts19/5/6 unchanged, active unlimited goal.
No tests/build/check/collection/AST/format/diff checks, dependency resolution,
network validation, agents or publication ran.

## Fifth checkpoint — F10 historical money-flow service source

The preceding fields checkpoint is committed as `e8392a5`. Added feature-gated
`services/money_flow_history.rs`, reexported its query/window/row/result/service
from both services and f10. F10Client owns a lazy getter; service clones share
its original irreversible close state. No default HQ HTTP dependency is added.

`history` and same-semantics `range` issue exactly one string-param request to
CWServ.tdxf10_gg_sj. Days3/5/10/20/60, usize start, optional nonzero count<=800;
validation precedes requests. Source rows are reversed then sliced without
sorting or start+count overflow. This resolves the incorrect task-list phrase
in favor of original Python, capability matrix and explicit COMP-050 contract.

Rows move from the first table instead of duplicating its result tree; raw
cells, duplicate/unknown fields and numeric lexemes are retained. Gregorian
dates and finite-f64 amount views diagnose invalid raw values; units remain
Unknown. Business error code, additional tables/attributes and actual HTTP raw
stay inspectable. Work limits, cancellation and projection stay inside the
original operation. Python F10 history migration/raw-serialization behavior
is still T23 scope, not silently changed here.

Five public source cases cover route/windows/limits, COMP-050 normal+unsorted
order, raw/business/additional-table/invalid values, empty/outside windows and
zero-request invalid days/shared close. None has been compiled or executed.

Next: full157 endpoint/branch inventory, closed selectors/topic prerequisite
and remaining typed result details, then T22–T30. Counts19/5/6 unchanged;
active unlimited goal, no execution verification/agents/publication.

## Sixth checkpoint — literal route inventory and checked prerequisites

Historical flow committed `da1953f`. Authored f10_route_inventory.rs by reading
the Python endpoint families and frozen specification, without executing them.
It contains literal requests for US17/HK20/SB13/bond31/industry13/futures8,
A-share defaults and shareholder/capital/business/industry/trading branches,
all30 fund sections, all26 market sections/aliases, cache/digest/home tabs and
the independent84-id channel inventory, theme/stage/valuation request variants.
These are source contracts, not measured executed coverage or157 completion.
Open selector families and all remaining typed/pagination details still need
source review and later execution evidence.

TopicCompareQuery now requires a valid topic id at construction and updates.
topic_compare_first skips unusable ids, stops on failed prerequisites, and
copies the selected arbitrary-length id with cancellation/aggregate charging.
First-id strict behavior is intentionally distinct from Python's original
first-row _first_value conversion, which T23 must preserve. FundReportQuery
checks a stored DateOrWindow on both date and section setters, eliminating
the setter-order bypass. Added success/two-source/failure source cases.

Counts19/5/6; goal active unlimited. No tests/collection/build/check/AST/format/
diff-check/dependency/network validation, agents or publication occurred.

## Seventh checkpoint — typed market results and pagination provenance

The route inventory is committed as `5b67774`. Continued source-only work on
market.rs: all three collectors now retain each page Response after moving
the first-table rows into the typed result. Extra tables, columns, business
status, attributes and optional raw bytes survive with original source order.
IPO likewise retains its Response instead of discarding non-row content.

The new Rust total selector reads only explicit `zs` cells after the first
table. Invalid values are diagnosed; conflicting valid totals in a response
are ignored. The original Python first-integer scan remains a T23 legacy
requirement. First nonzero business code stops collection, retaining partial
rows and returning BusinessError completion/ok=false/code rather than fake
EndOfData. Transport/protocol failures and resource caps remain errors.

Added checked Gregorian date views alongside all raw getters, finite-number
and market validation diagnostics, per-field unit/evidence descriptors,
ordered convenience views and explicit heuristic ETF filtering. Descriptors
distinguish repository source mappings, inferred meanings and unknown fields;
they do not claim any new live evidence. N005 IPO is never labeled percent,
N007 IPO scale remains unknown, tracking N004/N007 stay unknown. All row
projection and owned result allocation remains inside the original context.

Seven public source case groups cover nonempty two-page data/unrelated numeric
tables, business failure with retained data, board record identity/unique code
views, tracking field units and invalid numbers, IPO raw/business/additional
table retention, limits/no-progress, and conflicting total/RequestedPage.
None was compiled or executed. No Python source was changed in this stage.

T20 remains partial pending the rest of the named-entry/branch binding audit.
Next complete that source coverage and continue T22/T23/T24–T30. Counts19/5/6,
active unlimited goal; no execution checks, agents, push/tag/publish/merge.

## Eighth checkpoint — public entry invocation and recovery inventory

Typed market result checkpoint is `fc2e7cb`. Added f10_named_methods.rs with
148 single-request named public invocations: international50, bonds31,
industry/futures21, reports23, news/valuation21, generic2. Synthetic local
HTTP source cases assert actual route/body, preserved duplicate/unknown
fields and extra tables, raw bytes, alternating business status and closed
owner rejection. Expected bodies use each query's to_call, so only the
separate literal inventory establishes independent wire expectations.

Extended literal inventory with open-section pass-through, closed-selector
rejections and nondefault dates/filters/pages/arguments. Added dependent
fund allocation sources for all three report families, empty/null/invalid/
failed prerequisite rejection, and explicit business report dates. Fixed
test-source scalar expectations for numeric MG pagination and Self-returning
with_board_market by reading their implementations, without execution.

Added f10-implementation-index.md to enumerate157 original entry points,
their source ownership and evidence responsibilities. It also records the
T23 legacy behavior boundaries, including Python's local-today HK forecast
defaults and leap-day behavior. Closed registry documentation now includes
the existing industry/futures families; production selector code unchanged.

All source cases remain uncompiled and unexecuted. Counts19/5/6 unchanged;
T20 final static source review remains before T22/T23 and T24–T30. Goal active
without token cap; no execution verification, agents or publication.

## Ninth checkpoint — source scope closed, execution remains deferred

Public invocation/inventory checkpoint: `7b21857`. Final static pass confirmed
the157 named entry inventory, source result/field/history implementations and
paired wire/invocation cases. Added cancellation/expired-deadline zero-I/O
cases for prerequisite, collection, IPO and ladder entry points. Corrected
the two allotment entry labels and mandatory details date in the specification
to match actual Python source/literal cases; corrected the old brief's history
sorting phrase to COMP-050 reverse-only. No production request was changed.

T20 source scope is complete. This is not compilation or acceptance: all
authored cases remain unexecuted, dependencies unresolved and platform/MSRV
behavior unverified. Totals20 source complete/4 partial/6 unstarted. Next T22
shared ZIP integration/local dispatcher/HQ financial service, then all T23–T30
and the deferred T19 execution gates. Goal active unlimited; no agents or
publication, and no execution checks occurred.
