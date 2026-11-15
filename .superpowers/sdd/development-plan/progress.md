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

## Task log

- Task 1: fix round 1; initial commit `b6c2ea6` has six review findings. Critical: typed Rust command constants are not matched; runtime reflection is not persisted; baseline hashes are unchecked. Important: node lookup is not pytest collection; manifest does not compare to catalog IDs/source mappings; API baseline only checks shape. Working-tree hash mismatches may be caused by CRLF conversion and must be compared to raw Git-tree bytes. No tests or builds run.
- Controller finding for fix round 1: the manifest labels some current SDK capabilities `migrated` before the new `crates/eltdx` public SDK/business layer exists. The fix must define migration-status semantics and separate already-existing Rust protocol support from migration of the public/business capability.
- Task 1: fix round 1 commits `5a096aa` and `c0e2c1c`; scoped re-review found two Important contract gaps and one coverage question to resolve before T02: the runtime reflection contract does not compare all stored decorator/property/dataclass details; the manifest does not assert that public SDK migration status is supported by Rust SDK implementation rather than only protocol wire support; newly added Python modules need an explicit coverage route beyond the frozen module list. No tests, collection, builds, checks, clippy, doctests, or network validation run.
- Task 1: fix round 2 in progress from base `c0e2c1c`; original implementer resumed to close the three review items. User constraint remains binding: author contract tests as needed, execute none until T01-T30 implementation code is complete.
- Task 1: fix round 2 commit `02de834`; scoped re-review: decorator/property/dataclass comparisons ADDRESSED; migration evidence model and extensible module-coverage routes NOT ADDRESSED. Controller also found `current` module paths are absolute while baseline/coverage paths are repository-relative, making the new path-set assertion inconsistent. No tests or collection run.
- Task 1: fix round 3 commit `edacc482`; added separate public SDK evidence, migration-positive fixture shape, relative path normalization, extensible coverage source validation, and concrete test target checks. Re-review pending. No tests, collection, builds/checks, clippy, doctests, or network validation run.
- Task 1: round 3 re-review FAILED. Findings: SDK evidence was only truthy and did not resolve to public implementation in `crates/eltdx`; compatibility references accepted arbitrary same-name AST nodes rather than exact pytest-collectable `test_*` paths; module coverage sources were not individually bound to the snapshot, migration record/source_files, or corresponding Python module. Path normalization is accepted. Round 4 is scoped to these three findings, T01 contract regressions, and status documentation. Do not run pytest or collection, cargo test/build/check/clippy/doctest, network validation, AST compilation scripts, or `git diff --check`. No Rust crate is present yet, so no capability may be upgraded to migrated in this round.
- Task 1: fix round 4 commit `8cc1a0b`; added source-level positive/negative contract cases and validators for crate-relative public Rust symbols, exact pytest node hierarchy, and explicit Python module imports; manifest policy records evidence formats. No pytest/collection, cargo test/build/check/clippy/doctest, network validation, AST compilation scripts, or `git diff --check` run, per user instruction. Scoped re-review remains pending.

## Execution state

| Task | Status |
|---|---|
| T01 | Round 4 committed as `8cc1a0b`; scoped re-review pending; verification forbidden |
| T02 | Not started |
| T03 | Not started |
| T04 | Not started |
| T05 | Not started |
| T06 | Not started |
| T07 | Not started |
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
