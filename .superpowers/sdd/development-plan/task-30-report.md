# T30 source implementation report

## First checkpoint — native stress sources and evidence reader (stage111)

Base commit43ec6b5. T30 is **partial**, not source-complete or accepted. Current totals:28 source-complete /2 partial(T19/T30) /0 unstarted. The full unlimited goal remains active. No tests, collection, compile/build/check/clippy/doctests, dependency/network, AST/format/diff checks or equivalent execution ran. No subagents, publication, push, tag or merge.

Authored ten explicitly ignored cases across tests/stress.rs, tests/stress_mac.rs and the private cfg(test) src/stress_internal.rs. Full workload/source/evidence/threshold mapping lives in docs/rust-sdk/t30-evidence-index.md. Actual paths include100k wire requests among125k unique outcomes, admitted cancellation/late replies, close/pin/service handles, paused subscription overflow, malformed replies/pagination/download limits, reconnect/retry, real DAT/ZIP caps, F10 HTTP owner workers, MAC-EX catalog/macro cache ownership and actual SDK engine raw/decoded/queue/actor/generation telemetry. None has run.

The HQ loopback helper now assigns physical identities, counts live connections and reaps child JoinHandles during long workloads. Its first error is retained while all children are stopped/joined. Final ordinary regression must cover this shared helper change. SDK archive include adds its two small test helpers because the private library-test module uses them; release-policy.md explains this, production imports unchanged.

New scripts/verification/rust_sdk_stress.py builds before measuring, locates real release test binaries, runs each ignored case in its own process, observes process-tree RSS/threads/FD or handles/TCP/CPU with psutil, retains stdout/events/resource NDJSON and source/blob/lock/toolchain/command/exit/hash provenance. verify/aggregate re-open raw evidence rather than trusting a status field, validate identities/counts/budgets/resources/percentiles and require same clean candidate Linux+Windows. Independent manual rust-sdk-stress.yml preserves failed evidence; ordinary jobs do not execute load. All runner/workflow/dependency APIs are unverified.

Source-read corrections in this checkpoint: fixed cancellation result unwrapping; release fixture before propagating caller error; infinite directory must return LimitExceeded; actually exercise malformed company catalog; connect logical operation ID to physical wire identity; discard stale activation notifications; fixture errors reap all children. These are source edits, not verified fixes.

Remaining T30 source scope: complete performance workloads/runner/exports and old/new same-fixture policy, fuller workload readiness/warmup/configuration and close/cancel timing, final R01–R12 evidence index, rustdoc/examples/Python migration/release/maintenance, and focused source self-review. T19 still needs source completion before global execution gate. Production login/setup evidence remains absent; synthetic rules do not fill it. Do not run any new script yet.

Next recovery: read this report and t30-evidence-index.md, inspect current Git and code, continue remaining T30/T19 implementation. Do not mistake28 source-complete tasks for28 accepted tasks. Canonical docs and package mirrors must stay synchronized. Next checkpoint stage112, not another stage111.

## Second checkpoint — paired performance and workload readiness (stage112)

Base c5584c9. Counts remain28 source-complete /2 partial(T19/T30) /0 unstarted. No compilation, tests, collection, dependency/network, syntax/AST/format/diff checks or equivalent execution ran. Only source/document reads/writes and ordinary Git recovery operations. No agents, push/tag/publish/merge, or goal-controller changes.

Authored measure_candidate.rs using public SDK/std with offline feature: strict in-memory DAT parse, reused HQ loopback, actual connected-owner close, separately named pre-admission cancellation and optional explicit external observation. Python worker imports only installed wheel public APIs and records actual native binary hash. Separate parse and predecoded FinancialRecord construction curves do not infer conversion time by subtracting incompatible Rust/legacy profiles.

performance_fixture.py independently creates256x64 float32 DAT and a nonempty standard-library HQ server, with strict14-byte market/start/limit/reserved request validation, monotonic physical connection IDs and full observed traffic. rust_sdk_performance.py builds actual baseline/candidate release wheels with locked dependency inputs/profile/toolchain provenance, installs separately with no-index/binary-only, builds/copies the release Rust worker, alternates12 curves across7 trials with50 warmup+500 samples each, and samples only worker resources. Raw samples/wire/peer failures/logs, build artifacts and original records/lock/source/script/binary hashes are retained. verify re-reads raw evidence and checks snapshots against candidate Git blob contents. All84 default runs remain unexecuted.

Manual rust-sdk-performance.yml separates preparation/measurement/artifact export from quick CI. performance-policy.md documents exact scopes, source/build commands, CPU/RSS sampling limits, uncontrollable external observations and candidate/baseline ratio direction. It makes no speedup or passing claim.

All ten stress paths now author actual warmup, one configuration and final measurement/end/cooldown boundaries. Separate latency observations cover actual admitted cancellation and connected owner close, with raw identity/count rechecking. Fixed malformed company command to0x02cf after reading its production command source and subscription SecurityId construction. Source-only self-review corrected an initial6-byte directory fixture assumption to the actual14-byte request, resource hashes calculated before buffered file close, failed peer records discarded before summary, cleanup after failed close-preparation queries, and source snapshot hashes not tied to actual Git bytes. None is called a verified runtime fix.

Next stage113: finish focused pressure/performance source review (including generation/resource evidence), finalR01–R12 coverage handoff, crate rustdoc/example/feature/error/unit/time/raw/serde explanations, Python migration/release/maintenance documents and strict/link verifier sources. Then close T19 focused source inventory/review/handoff. Only after ALLT01–T30 source scopes are actually complete may unified execution begin. ProductionMAC-EX login and optionalMAC setup identity/order/fence evidence remain absent; synthetic ACKs never fill them. Keep canonical documentation and src/eltdx/docs/rust-sdk mirrors synchronized.

## Third checkpoint — final guides, transport handoff and strict docs (stage113)

Base58f7664. The actual source additions close T19's implementation/fixture/
handoff scope. Current totals29 source-complete /1 partial(T30) /0 unstarted.
No source gate has been opened and no execution verification ran. The unlimited
goal remains active; no agents, publication or goal-controller changes.

Added user-guide.md, python-migration.md, final-acceptance.md, release-notes.md,
maintenance.md and f10-transport-index.md. They explain independent installs,
features, ownership, deadlines/errors/cleanup, ordering/completion, units/time/
raw/serde, preserved Python adapters, current candidate scope and unresolved
production evidence. R01–R12 link real domain indexes, implementations, actual
test targets and required run provenance; the table is navigation, not an
acceptance program that passes merely because files exist.

Crate homepage rustdoc has a non-network lazy-owner doctest and a no_run actual
codes query with explicit endpoint and primary-versus-close error handling.
EX and financial download examples now also report secondary close failure.
The README documents the gated measure_candidate worker. MkDocs has user and
domain guide navigation, and its SDK index now distinguishes original design
baseline from the unverified implementation candidate. The stale status table
is brought forward without altering historical checkpoint records.

rust_sdk_matrix.ps1 authors minimal/default/full doctest and doc runs with
strict RUSTDOCFLAGS, logs the actual flags and restores the original process
environment in finally. The manual SDK workflow adds a separate documentation
job using existing metadata/full mirror, MkDocs strict and built local-link/
fragment checkers; it does not install/build the Python product to build docs.
None of these commands or jobs has run.

Pressure source adds actual actor_slots, live/slot/pool bounds, required live
generation observation and per-slot monotonic epoch/generation. Close requires
zero raw/decoded/live actor/queue; the private test also asserts every remaining
actor is stopped with no pending work. The verifier still independently checks
wire/terminal IDs and inflight late-response isolation. These are authored
conditions, not observed performance or lifecycle results.

Remaining T30 source work is explicit: verification.md requires every public
service to have an ordinary compileable usage example and a nonempty local
behavior case. Homepage examples and general integration inventory alone do not
satisfy this per-service documentation condition. Author the missing examples,
tie each to existing real loopback functions, then perform final source review
of T01–T30 deliverables and synchronize mirrors. Only then may the root record
the global source gate and start unified dependency/build/test/docs execution.
All dependency/API/MSRV/license/platform, package/compatibility, resource and
performance outcomes remain unverified. MAC-EX production login and optional
MAC setup identity/order/fence evidence remain missing.

## Fourth checkpoint — service examples and E06 serialization contracts (stage114)

Basefd548ca,2026-11-17. Completed the remaining authored T30 deliverables:
six canonical example guides copied byte-for-byte into crate src/guide and
feature-gated include_str rustdoc; service-examples.md maps18 public services,
four standalone clients and local financial reading to actual nonempty fixture
functions. Sixteen HQ snippets include the public pin query; other guides use
actual checked F10/financial/EX/MAC/MAC-EX APIs. Examples borrow configured
owners and explain explicit outer cleanup, original units/dates/raw/completion,
per-public-call relative deadlines and the synthetic/production profile limit.

Authored a new nonempty public pinned refresh fixture preserving request ID,
source cursor, raw payload/frame and explicit pin/owner close. Authored seven
serde_contract functions covering representation/scale/extreme i64/provenance,
f32/f64 negative zero, three nonfinite forms per width with unmodified raw bits,
display/source units, JSON byte sequences and absent-vs-empty wire, all five
time representations, checked constructors and explicit Unknown-vs-foreign
variants. Serde guide adds one generic serializer usage and separate compile-
fail contracts for SecurityId/TradingDate/EventTime Deserialize absence. The
JSON dependency for these tests is dev-only; production serde selection remains
independent. Tests/doc examples have NOT been compiled, collected or executed.

Extended the existing metadata checker to six include byte pairs; it proves
only synchronization. Matrix now authors standalone serde integration plus
minimal/default/serde/f10/full strict doctest/doc. Updated actual navigation,
user/maintenance/final acceptance/verification indexes and package mirrors.
These are real source edits, not another file-existence acceptance program.

Final focused source handoff reviewed T30 brief/plan, R01-R12 final index,
service APIs and samples, T28/T29 sources/reports, feature/dependency boundaries,
E01-E10 decisions and existing T19/pressure/performance handoffs. No remaining
promised implementation source gap was identified; missing independent external
evidence stays explicit rather than being invented or replaced by synthetic
fixtures. Source search found no todo!/unimplemented! body in SDK/bindings or
consumers; that search is not proof of correctness or completed test coverage.

Current count30 source-complete/0 partial/0 unstarted. ALL execution acceptance
is still pending; no test/collection/build/check/clippy/doctest, dependency or
network probe, compiler/AST/format/diff-check/equivalent command ran. Ordinary
source/document/Git operations only. Existing historical reports keep their
then-current counts. No subagents/push/tag/publish/merge/goal-controller edits.

Next root action: commit this source checkpoint with hooks disabled, then record
the global source-complete gate and begin unified verification in the order in
final-acceptance.md. Discover actual tools, resolve/lock dependencies, retain raw
failures, fix without reducing feature/API/test scope, and checkpoint repairs.
Uncompiled dependencies/API/MSRV/licenses/platforms, package/compatibility/
resource/performance outcomes and production profiles remain unverified. The
unlimited goal remains active until requirement-by-requirement actual acceptance.

## Global source gate — root checkpoint (stage115)

Root saved source checkpoint23bf1c83718e83d20e9c5ba9b919e9dfac3e2db4 and
observed a clean Git worktree. All authored T01-T30 source scopes are complete;
under the user's instruction, unified execution can now begin. No compile/test/
dependency/network/format/document result exists at the time of this record.
See docs/rust-sdk/unified-validation.md for subsequent actual run outcomes.
Begin tool/lock preparation then correctness/compatibility/package/docs and
pressure/performance; do not reduce scope or invent missing external evidence.
Goal remains active without a token quota; no publishing or goal-state edits.
