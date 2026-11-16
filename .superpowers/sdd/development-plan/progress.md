# SDD ledger — plan: docs/rust-sdk/development-plan.md

## Setup

- Worktree: `C:/Users/ax/.codex/worktrees/rust-sdk-implementation/tdx抓包`
- Branch: `codex/rust-sdk-implementation`
- Plan base: `e36c71e` (`docs(rust-sdk): checkpoint implementation specification`)
- Plan/spec source: `docs/rust-sdk/development-plan.md` and linked rust-sdk specifications.
- User constraint: do not run tests, builds, checks, clippy, doctests, or network validation until all implementation code T01-T30 is complete. Test sources may be authored, but execution is deferred. User explicitly authorized progress commits; publish, tag, push, merge remain unauthorized.
- Tool adaptation: SDD helper scripts run under WSL only with explicit `GIT_DIR` and `GIT_WORK_TREE`, because this Windows worktree's `.git` file contains a Windows gitdir path.

## Preflight: task self-consistency

| Task | Text checked | Result |
|---|---|---|
| T01 | Runner target, 26 wire codes, 25 registry entries, migration manifest, API snapshot | Coherent; preserve separate counts and represent the unregistered money-flow wire path explicitly. |
| T02 | Fixture schema, prior fixtures, independent expected values | Coherent; missing real captures stay labeled synthetic/experimental. |
| T03 | Policy fixtures, evidence levels, Python legacy behavior | Coherent; evidence gaps must produce Unknown/Unsupported rather than guessed rules. |
| T04 | New crate, public types, features, no-network builder | Coherent; base crate must not require Python or optional F10/offline features. |
| T05 | Pin/pending transitions and deterministic race fixtures | Coherent; shared exactly-once terminal transition is the prerequisite for T06. |
| T06 | Raw reply and operation-wide deadline | Coherent; old typed runtime path remains compatible and delegates to one execution loop. |
| T07 | Owner lifetime, close-once behavior, service handles | Coherent; all service handles share one owner and Rust close is terminal. |
| T08 | Options, cancellation, errors, compatibility policy | Coherent; operation-wide Rust policy and per-request Python policy remain distinct. |
| T09 | Parser context and error attribution | Coherent; parser context remains language-neutral and owner-scoped. |
| T10 | Metadata loading, projections, timezone/clock | Coherent; metadata version and units must be validated before projection. |
| T11 | Security lists, batch query, missing data | Coherent; batch order and missing-result contract are shared by services. |
| T12 | First business chain and Python bridge | Coherent; facade replacement follows a working Rust domain and preserves Python surface. |
| T13 | Kline/minute pagination and continuation | Coherent; cursor rules depend on T08-T11 contracts. |
| T14 | Tick, auction, and money-flow behavior | Coherent; experimental category22 remains separate from stable historical F10. |
| T15 | Subscription dispatcher, pin ownership, gap delivery | Coherent; dispatcher must be the sole runtime push consumer. |
| T16 | Corporate data, adjustment, calendar, price-limit policy | Coherent; unresolved exchange rules remain Unknown. |
| T17 | Server resources, ZIP stats, company data | Coherent; one bounded pinned download contract feeds T22. |
| T18 | HQ migration and usable SDK acceptance | Coherent; replace algorithms by domain and retain legacy adapter behavior. |
| T19 | HTTP dependency and bounded F10 transport | Coherent; dependency decision precedes its use and does not affect default HQ. |
| T20 | F10 dynamic results, endpoint variants, historical flow | Coherent; preserve duplicate/ordered cells and distinguish HTTP from service errors. |
| T21 | Offline market data and writes | Coherent; append/replacement and durability semantics remain explicit. |
| T22 | Financial ZIP/DAT behavior | Coherent; reuse T17 decoder and resource service rather than introduce another archive path. |
| T23 | F10/offline helpers and Python migration | Coherent; helper features follow their concrete Rust dependencies. |
| T24 | EX session, frames, command families | Coherent; implement bounded protocol profiles without inferring undocumented correlation rules. |
| T25 | MAC/MAC-EX routing and bitmap | Coherent; profile-specific layout preserves unknown legal bits and raw identifiers. |
| T26 | Extension services and catalogs | Coherent; waits on stable T24/T25 sessions and preserves source provenance. |
| T27 | Extension Python migration and coverage | Coherent; migration status is explicit per API and does not collapse independent clients into HQ. |
| T28 | Consumer and feature/platform matrix | Coherent; Rust consumer is Python-free and wheel consumer is Rust-toolchain-free. |
| T29 | Package metadata/version matrix | Coherent; registry availability is checked only during final packaging verification; no publishing. |
| T30 | Evidence, docs, stress/performance record | Coherent; no performance or stress claims before measurements exist. |

## Preflight: shared interfaces

| Tasks | Producer/consumer boundary | Finding and ruling |
|---|---|---|
| T01 ↔ T02 | Migration manifest, registry coverage, fixture inventory | Freeze manifest identity/schema in T01; T02 consumes it and does not duplicate registry truth. |
| T04 ↔ T07 ↔ T08 | Client, owner, error, operation options | Define shared types and close contract once; service tasks extend these types without parallel versions. |
| T05 ↔ T06 | Runtime engine, pending/pin, raw reply, deadline | Complete pin terminal ownership first; add deadline/raw reply through the same execution engine. |
| T09 ↔ T10 | Parser context, metadata snapshot, numeric projection | Protocol owns language-neutral parse values; SDK owns client-scoped metadata and projections. |
| T11 ↔ T12 ↔ T18 | Batch/query/service and Python facade | T11 fixes shared batch/missing semantics; each migration switches only a Rust-backed domain. |
| T12 ↔ T18 ↔ T23 ↔ T27 | Native service bridge and public Python API | Keep the public facade stable; bridge only to implemented Rust domains and retain explicit legacy policies. |
| T13 ↔ T14 ↔ T15 | Time/cursor, owner, metadata, push dispatch | Share continuation and owner contracts; one dispatcher consumes runtime pushes. |
| T16 ↔ T18 | Corporate, calendar, helper projections | Use approved policy evidence; unresolved limit rules remain Unknown through Python compatibility adapter. |
| T17 ↔ T22 ↔ T23 | Resource download, ZIP/DAT, financial helpers | T17 defines bounded pinned source and archive decision; T22/T23 reuse it. |
| T19 ↔ T20 ↔ T23 | F10 transport/results/helpers | T19 is the only HTTP transport; downstream code preserves ordered/raw results and feature dependencies. |
| T21 ↔ T22 ↔ T23 | Offline files, DAT, financial/helper APIs | Shared archive/limits and Python legacy write semantics are frozen before facade migration. |
| T24 ↔ T26 ↔ T27 | EX session/catalog/Python adapter | Session and profile are authoritative; catalog and adapter do not create a second transport. |
| T25 ↔ T26 ↔ T27 | MAC bitmap/routing/services/Python adapter | One bitmap/profile source feeds services and compatibility projections; MAC-EX uses its own login route. |
| T28 ↔ T29 ↔ T30 | Feature graph, packages, evidence/docs | Consumer matrix constrains package metadata; final docs report only artifacts/evidence actually produced. |

## Rulings

- Ruling: The installed SDD task-brief helper cannot parse this plan's `T01` heading because it only matches `Task 1`; create a manually transcribed T01 brief from the authoritative plan and review it against that section — cost if wrong: a requirement could be omitted from the delegated task, so the full plan section remains the review authority.
- Ruling: User's explicit no-test-until-T01-T30-complete instruction overrides the plan's normal RED/GREEN and per-task verification steps; author tests as specified, run none before the final implementation checkpoint — cost if wrong: integration defects may remain latent until the unified verification pass.
- Ruling: T01 must keep wire-code count (26), command-registry count (25), and capability count (124) independent; 0x0FFC stays a protocol-only money-flow path and 0x052D category22 stays a variant — cost if wrong: manifest coverage and user-visible support reporting could misstate protocol behavior.
- Ruling: Validate `baseline-python-api.json` source hashes against raw Git-tree bytes at its declared `source_revision`, and compare current source by normalized AST/API shape; do not regenerate the frozen baseline merely to match Windows CRLF working-tree bytes (`core.autocrlf=true`) — cost if wrong: line-ending conversion could be mistaken for source drift or a real API change could be masked.
- Ruling: Where exchange or server evidence is missing, implement explicit Unknown/Unsupported behavior rather than inferred semantics — cost if wrong: a real supported capability may be withheld until evidence is added.
- Ruling: Preserve Python's public surface and per-request compatibility policy while sharing algorithms behind the bridge; Rust gets its documented stricter operation policy — cost if wrong: Python regression or divergence between product surfaces.
- Ruling: T03 has no dated authoritative source approving any disputed price-limit profile; all six Rust policy cases remain `UnknownRule`, while each Python entry point keeps its own source-reviewed projection — cost if wrong: a supported rule may remain unavailable until authoritative evidence is recorded.
- Ruling: Preserve code normalization and metadata-prefetch as distinct Python compatibility paths; Rust metadata lookup follows strict normalized `SecurityId` and owner scope — cost if wrong: changing the Python prefetch request set or sharing identity state can alter old request ordering/results.
- Ruling: Keep bare `127` codes ambiguous in `SecurityId` inference, even though Shenzhen convertible bonds include `127` prefixes; accept an explicit market such as `sz127001`. The identity spec permits inference only where the bare code is unambiguous, and T03 records this overlap — cost if wrong: rejecting all such bare IDs withholds a valid inference, while guessing Shenzhen can misidentify a Shanghai security.
- Ruling: Accept the T04 no-startup evidence as sufficient for this crate foundation: loopback source tests cover configured socket connections for successful build/close and invalid configuration, while the current builder/owner source contains no runtime, thread, resolver, or filesystem initialization to instrument. Defer direct cross-platform thread/filesystem probes unless those startup paths are added — cost if wrong: a later side-effect path could evade the loopback regression test and would then need its own deterministic instrumentation.

## Task log

- Task 1: fix round 1; initial commit `b6c2ea6` has six review findings. Critical: typed Rust command constants are not matched; runtime reflection is not persisted; baseline hashes are unchecked. Important: node lookup is not pytest collection; manifest does not compare to catalog IDs/source mappings; API baseline only checks shape. Working-tree hash mismatches may be caused by CRLF conversion and must be compared to raw Git-tree bytes. No tests or builds run.
- Controller finding for fix round 1: the manifest labels some current SDK capabilities `migrated` before the new `crates/eltdx` public SDK/business layer exists. The fix must define migration-status semantics and separate already-existing Rust protocol support from migration of the public/business capability.
- Task 1: fix round 1 commits `5a096aa` and `c0e2c1c`; scoped re-review found two Important contract gaps and one coverage question to resolve before T02: the runtime reflection contract does not compare all stored decorator/property/dataclass details; the manifest does not assert that public SDK migration status is supported by Rust SDK implementation rather than only protocol wire support; newly added Python modules need an explicit coverage route beyond the frozen module list. No tests, collection, builds, checks, clippy, doctests, or network validation run.
- Task 1: fix round 2 in progress from base `c0e2c1c`; original implementer resumed to close the three review items. User constraint remains binding: author contract tests as needed, execute none until T01-T30 implementation code is complete.
- Task 1: fix round 2 commit `02de834`; scoped re-review: decorator/property/dataclass comparisons ADDRESSED; migration evidence model and extensible module-coverage routes NOT ADDRESSED. Controller also found `current` module paths are absolute while baseline/coverage paths are repository-relative, making the new path-set assertion inconsistent. No tests or collection run.
- Task 1: fix round 3 commit `edacc482`; added separate public SDK evidence, migration-positive fixture shape, relative path normalization, extensible coverage source validation, and concrete test target checks. Re-review pending. No tests, collection, builds/checks, clippy, doctests, or network validation run.
- Task 1: round 3 re-review FAILED. Findings: SDK evidence was only truthy and did not resolve to public implementation in `crates/eltdx`; compatibility references accepted arbitrary same-name AST nodes rather than exact pytest-collectable `test_*` paths; module coverage sources were not individually bound to the snapshot, migration record/source_files, or corresponding Python module. Path normalization is accepted. Round 4 is scoped to these three findings, T01 contract regressions, and status documentation. Do not run pytest or collection, cargo test/build/check/clippy/doctest, network validation, AST compilation scripts, or `git diff --check`. No Rust crate is present yet, so no capability may be upgraded to migrated in this round.
- Task 1: fix round 4 commit `8cc1a0b`; added source-level positive/negative contract cases and validators for crate-relative public Rust symbols, exact pytest node hierarchy, and explicit Python module imports; manifest policy records evidence formats. No pytest/collection, cargo test/build/check/clippy/doctest, network validation, AST compilation scripts, or `git diff --check` run, per user instruction. Scoped re-review remains pending.
- Task 1: round 4 scoped re-review FAILED with three Important findings: Rust source-text search could accept private-parent declarations and paths outside the crate source root; pytest file matching accepted non-`.py` files and did not reject `__new__`; module import coverage searched unrelated test/helper nodes, and migration coverage did not require a valid record status. Round 5 commits `2eb0618` and `6f7cd82` narrow Rust evidence to root `lib.rs` declarations/single-symbol re-exports, bind imports to module scope or the exact test body (excluding nested helpers), and check pytest naming/class/fixture/disabled-node/status rules. Static review considers the findings closed. No validation commands run; T01 implementation is complete with final verification deferred until T01–T30 code is complete.
- Task 2 (`ea4721c`): added `tests/fixtures/sdk/inventory.json` with per-case paths and SHA-256 hashes for all 37 committed 7709 inputs/responses/expected values. The 36 generated cases are synthetic; `delisted_bars/normal` references `052b-ui-20261005/live-query-example.json`, which is absent, and its frame wrapper is synthetic. All existing expected values are explicitly labeled Python regression baselines generated at `0fb9d00`; none are independent oracles. Added seven language-neutral cases for COMP-028, COMP-030, COMP-042, COMP-047, COMP-052, COMP-056, and malformed 7709 compression; their Python and Rust expectations are separate and source/evidence levels are recorded. Added explicit gaps for a complete 0x0FFC capture, non-empty category22, and real EX/MAC/F10 responses; no missing capture was fabricated. Authored the inventory contract test and SDK fixture discovery/projection helpers. Only the inventory export script was executed to write hashes; no tests, collection, builds, checks, clippy, doctests, or network validation were run. T02 implementation is complete; final verification remains deferred until T01–T30 implementation is complete.
- Task 3: complete (commit `8d11baa`). Added policy fixtures for price limits, normalization, units, dates, and legacy cross-domain boundaries; expanded `public-api.md` and `capability-matrix.md`; updated T03 requirements and fixture README; authored `tests/contracts/test_sdk_policy_fixtures.py`. Source-reviewed Python conflicts include main-board ST (10% daily helper vs 5% shortline/MAC), registration-board ST (20% daily/MAC vs 5% shortline), unassigned 30-prefix, BSE IPO day two, index identity, and delisting ST. Since no dated authoritative rule sources are checked in, Rust expected limits remain `UnknownRule` and Python paths remain distinct. The fixtures separately record `today_batch`, 510300/430047 prefetch differences, multi-DAT selection, missing market-stat identity, finance failure cache, `max_pages=None`, deadlines, units, timezone, and bar-date rollover. No tests, collection, build/check/clippy/doctest, network validation, or `git diff --check` were run.
- Task 4: initial implementation committed as `1777e4f`; scoped review found unresolved validated deserialization on `SecurityId`/`TradingDate`/`EventTime`, missing `EventTime` inspection APIs, missing `Price` provenance/rounding conversion, and incomplete source coverage for no-startup side effects. The review's proposed acceptance of bare `127` conflicts with the identity contract; ruling above preserves ambiguity and requires explicit `sz` for Shenzhen. No test/build/check/Cargo/network validation has run.
- Task 4: fix round 1 committed as `4cb8511`; removed unchecked identity/time deserialization, added EventTime inspection and Price source/conversion APIs, extended loopback side-effect source tests, and added explicit `sz127001` coverage. Static scoped re-review pending from `1777e4f..4cb8511` in `review-1777e4f..4cb8511.diff`. No tests, collection, builds/checks, clippy, doctests, Cargo commands, or network validation run.
- Task 4: round 1 scoped re-review found findings 1 and 2 addressed; finding 3 remains open because `Price::to_scaled_integer` converts `ScaledInteger` through `f64` and can corrupt values above exact f64 integer precision. Finding 4 is partially supported by loopback tests; source structure was checked and has no runtime/thread/filesystem startup path, so the remaining direct probes are deferred under the ruling above. No tests or other verification ran.
- Task 4: fix round 2 committed as `4b6a524`; scaled-integer rounding uses checked integer arithmetic with authored large-value/tie/overflow cases. Scoped re-review from `4cb8511..4b6a524` marked the precision-loss finding ADDRESSED with no new breakage. All tests and verification remain forbidden until T01-T30 implementation code is complete.
- Task 4: minor (deferred): no source case explicitly covers `i64::MIN`; the scoped reviewer inspected the `i128` reduction arithmetic and found no issue. Carry this coverage note into the final whole-branch review.
- Task 4: complete (commits `f7b02fc..4b6a524`, scoped reviews clean after two fix rounds; final verification deferred until T01-T30 implementation is complete).
- Task 5 complete (`3ba3902`): private RAII delivery transfers pin identity to the public handle, drop/send-failure close through existing supervisor release transitions, and opened-reservation tracking plus a shared result-terminal marker prevent late success/failure cancellation from becoming stale pre-admission state. Inline source cases cover pending drop, receiver loss, cancellation before success, between open and send, racing with failed result publication, success before cancellation, fail-closed confirmation timeout, exactly-once release, and later pin reuse. The public loopback target covers cancel-after-success and subsequent pin admission. See `task-05-report.md`. No tests, test collection, build/check/clippy/doctest, network validation, `git diff --check`, or equivalent verification ran; no compilation or runtime success is claimed.
- Task 6: implementation `4bf47d7` plus fix `561dcf5`; scoped review round 1 found one Important issue: an expired coalesced connect waiter published failure to shared completion and aborted runtime. Fix round 1 was re-reviewed from `4bf47d7..561dcf5`; the finding is ADDRESSED, with no new Critical/Important breakage. Each waiter now receives its own timeout, shared completion remains owned by the physical attempt, and cleanup waits for the attempt terminal without closing the runtime. T06 is complete; final verification is deferred. No tests, collection, build/check/clippy/doctest, network validation, AST scripts, diff checks, or equivalent commands ran.
- Ruling: T07 must expose a runtime close operation accepting the owner's absolute `Instant` deadline and pass that same deadline through every owned pool; the existing public Engine close uses a fixed 1-second timeout and cannot honor configured `ClientConfig::close_timeout` or a shared multi-pool budget — cost if wrong: custom shutdown budgets are ignored or sequential pools can exceed the owner deadline.

## Execution state

| Task | Status |
|---|---|
| T01 | Implementation complete; final verification deferred until all T01–T30 implementation is complete |
| T02 | Implementation complete: fixture provenance, hashes, supplemental cases, and evidence gaps recorded; final verification deferred |
| T03 | Implementation complete: policy fixtures, compatibility decisions, public docs, and test source; commit `8d11baa`; final verification deferred |
| T04 | Implementation complete; validated identities/time, lazy configuration, source-aware event time, price conversion; final verification deferred |
| T05 | Implementation and source cases committed in `3ba3902`; final verification deferred until T01-T30 implementation is complete |
| T06 | Implementation and scoped review complete (`4bf47d7..561dcf5`); final verification deferred |
| T07 | Implementation checkpoint committed; owner lifecycle, absolute close deadline, cleanup lineage, and lifecycle source cases; scoped review pending |
| T08 | Not started |
| T09 | Not started |
| T10 | Not started |
| T11 | Not started |
| T12 | Not started |
| T13 | Not started |
| T14 | Not started |
| T15 | Not started |
| T16 | Not started |
| T17 | Not started |
| T18 | Not started |
| T19 | Not started |
| T20 | Not started |
| T21 | Not started |
| T22 | Not started |
| T23 | Not started |
| T24 | Not started |
| T25 | Not started |
| T26 | Not started |
| T27 | Not started |
| T28 | Not started |
| T29 | Not started |
| T30 | Not started |
