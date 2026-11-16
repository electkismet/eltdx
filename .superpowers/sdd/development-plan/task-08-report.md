# T08 implementation report — operation context and error boundary

## Scope and checkpoints

Implemented the T08 brief against the existing T01–T07 owner/runtime work. Source checkpoint `f837921` contains the initial SDK policy and common-runtime boundary; the final T08 handoff commit contains the compatibility bridge, authored test cases, request-local deadline follow-up, and this report. The controller owns progress/status documents and integration review.

This is an implementation and test-source handoff. No test, collection, build, check, clippy, doctest, formatter, diff check, network probe, compile/AST script, or equivalent verification was run. Compilation and runtime correctness are unconfirmed pending the user-authorized final T01–T30 validation phase. Commits disable hooks with `core.hooksPath=NUL`; nothing was pushed, tagged, published, or merged.

## Implemented behavior

- Added public, private-field `OperationOptions` and irreversible cloneable `CancellationToken`. Options retain `Duration` until a supplied method-entry `Instant`. Explicit zero/past deadlines reject before lazy startup; relative and absolute bounds choose the earlier instant; overflow is `InvalidArgument`. Tokens retain only cancellation state and can outlive a client.
- Added private `OperationContext`, execution profiles, non-Clone `RequestPermit`, child purposes, explicit recovery scopes, work limits, counters, first stop cause, and final-delivery boundary. Rust shares one absolute operation deadline and configured extra-attempt budget; legacy has no fabricated operation-wide deadline or extra-attempt maximum. Domain page limits remain explicit, including unlimited legacy pages.
- Added `OperationBudget` with a weak owner reference and lock order owner -> business token -> budget. Global and local recovery reservations validate transactionally before either debit. A unique reservation is moved to the runtime worker, commits once immediately before connect/send, and refunds only an unstarted reservation on Drop. No policy lock spans I/O, waits, parsing, or callbacks.
- Extended the existing language-neutral runtime submission path with absolute/after-startup timing, optional retry policy and attempt authorization, and unique reservation transfer. Both profiles use the same scheduler and worker loop. Legacy resolves its child deadline once after startup, without resetting it on queueing or retry. Startup polling observes operation cancellation/deadlines without closing unrelated shared startup work.
- Kept finite per-request wire-attempt limits and widened counters to `u32` so a validated `NonZeroU32` client setting is not narrowed to the prior `u8` representation. Rust uses an explicit read-command retry whitelist and rejects protocol/nontransport failures even when a worker calls them retryable. Rust endpoint failover uses the counted retry path; legacy preserves its existing within-attempt endpoint continuation.
- Added pollable cancellation confirmation and engine quarantine after unconfirmed retirement. SDK cleanup is claimed once, uses its separate timeout intersected with any owner shutdown deadline, and does not depend on the business token remaining active. Result delivery rechecks cancellation/close/deadline after receiving a raw reply; services must also call `context.finish()` after decoding/aggregation before final business delivery.
- Preserved structured runtime errors as typed SDK sources with operation/phase/command/host/attempt context. `CancelUnconfirmed.source()` retains the original stop cause; `cleanup_failure()` separately retains cleanup failure. Budget exhaustion restores the real last recoverable cause rather than replacing it with a string-only error.
- Integrated ordinary and exact-engine pinned raw execution in the owner-bound HQ executor. Owner engine registration deduplicates exact engine owners; no runtime starts during construction/registration. Existing T07 exact pin identity, shutdown deadlines, and close failure source handling remain in place.
- Added `#[doc(hidden)] pub mod __python_compat` with a narrow legacy executor/context, child purposes, explicit local recovery limits, optional page limit, raw/pinned execution, and independent owner generation. It adds no PyO3 dependency or feature and cannot switch an existing ordinary Rust client to legacy policy. Python binding orchestration must construct distinct domain executors and close them at their original stages.

## Request-local retirement follow-up

Source review identified deadline expiry after admission/promotion but before initial attempt creation, and expiry while waiting for a retry generation's retirement acknowledgement. These errors formerly propagated through `RuntimeCore` as runtime-fatal results. Initial timeout now releases only the assigned ordinary/pinned request. Retry timeout accepts only its validated reconnect acknowledgement, clears only that request's retry intent, and enters the existing terminal reducer without reconnecting. Promotion recursion is explicitly boxed at the initial rejection branch.

Unstarted pinned cancellation/authorization rejection uses a terminal identity with no generation/message. `PinRegistry` now accepts that case only for the matching engine/slot/lease/request with no bound wire. Commit still requires the same pin identity, active call lease, sequence, FIFO plan, and wire state. Stale unstarted events/plans cannot retire a promoted or replacement call. This is a narrow terminal reconciliation change; no pin lifecycle redesign was introduced.

## T10 integration extension approved by controller

Added owner-free `OperationSponsor` retaining only shared budget/profile, with boundary/deadline access, metadata child authorization, and command retry policy. A T10 shared loader must cache one metadata authorization per sponsor per shared logical request, choose a live sponsor per physical attempt, and retain its own transport deadline/stop state. A sponsor's original deadline must not become the shared flight's transport deadline. No metadata store, snapshot, installation field, or second cache was introduced; T09/T10 own those components.

## Files

- SDK operation/public wiring: `crates/eltdx/src/lib.rs`, `operation.rs`, `__python_compat.rs`.
- SDK policy/error/owner execution: `crates/eltdx/src/policy/mod.rs`, `policy/retries.rs`, `error.rs`, `session.rs`, `hq/executor.rs`.
- Shared runtime: `crates/eltdx-runtime/src/execution.rs`, `request.rs`, `supervisor.rs`, `engine.rs`, `pin.rs`.
- Test sources: `crates/eltdx/src/operation_tests.rs`, `crates/eltdx/tests/operation.rs`, plus inline runtime cases in `execution.rs`, `engine.rs`, and `pin.rs`.
- Report: `.superpowers/sdd/development-plan/task-08-report.md`.

No T09-owned protocol files, SDK `hq/mod.rs`, SDK `hq/decode.rs`, or Cargo manifest were edited by T08. Concurrent controller/T09 changes were excluded from T08 staging.

## Authored test sources — not executed

- Arithmetic/entry: inherited and explicit deadline selection, zero/past rejection, overflow, irreversible cancellation, and token/authorization/sponsor owner lifetime.
- Budget transition code: concurrent contention for the final extra slot, unused and cancelled-before-start refunds, close-versus-start and cancel-versus-start gates, transactional local/global recovery limits, exact normal-page/metadata/retry/fallback accounting, zero-budget legitimate/empty work, and no fabricated legacy 32-attempt cap.
- Work/final boundaries: second-batch cancellation isolation, deterministic profile deadline contrast, page/record/byte overflow, once-only cleanup despite cancelled business token, owner shutdown deadline shortening, typed diagnostic causes, and separate root/cleanup failure retention.
- Shared runtime fixtures: synthetic loopback two-page comparison (80 ms each, 120 ms operation/request timeout), comparing Rust total expiry with the new legacy bridge and the original typed runtime transport; actual local connect failure/failover budget exhaustion with host/attempt source; worker authorization rejection followed by unrelated request success.
- Deterministic `RuntimeCore` cases: ordinary and pinned expiry between admission and initial dispatch; retry deadline expiry during retirement acknowledgement; subsequent request admission remains usable and no unrelated runtime close is requested.
- Pin registry: exact unstarted terminal with FIFO promotion, duplicate/stale terminal plans, and rejection of mismatched request, engine epoch, lease, slot, generation, or message.
- Per-request runtime policy: finite request attempt exhaustion retains the actual network error, strict Rust protocol failure is not retried, and legacy after-startup/absolute timing distinction.
- Public integration: owned Send/Sync options/tokens, language-neutral typed causes, and cross-crate hidden bridge visibility without starting a runtime/network resource.

## Deferred validation and remaining integration concerns

Run the required `cargo test -p eltdx --test operation --locked` only after all T01–T30 implementation is complete. That integration target does not execute the private budget/runtime cases. Final validation must additionally include the SDK library operation unit cases (for example `cargo test -p eltdx --lib operation --locked`) and runtime library cases (`cargo test -p eltdx-runtime --lib --locked`), followed by the plan's workspace checks.

The synthetic comparison uses the original typed runtime transport, not executed old/new Python wheels. COMP-009/010/013 recovery ordering, domain close behavior, and actual Python differential fixtures still require the planned binding/compatibility phase. The legacy bridge exposes the necessary separate owner/domain mechanism but does not implement Python facade exception projection or domain orchestration.

Later service methods must capture entry before validation, pass it to `begin_context_at`, freeze domain work limits before the first child, use one context through metadata/pages/fallback, preserve recoverable causes, and call `finish` after the final CPU/decode/aggregate boundary. T10 owns metadata snapshot installation and shared-flight sponsorship integration. All real loopback timing and race assertions remain unexecuted and may require fixture tuning during final validation.

Existing legacy endpoint continuation behavior was retained; the new request-local timeout handling targets initial dispatch and the counted retry path. The repository contains pre-existing unusual rendered identifiers in unrelated source; T08 did not broaden into repository-wide identifier cleanup.

## Scoped review fix round 1

Addressed the four Important findings in `task-08-review.md` against `b3371c0` without broadening the T08 surface:

1. Raw-result cancellation now carries a shared terminal flag into the control registration. The runtime marks the request terminal in its single terminal reducer before removing/publishing the pending result. Cancellation registration and snapshot processing both recheck that flag; a terminal already committed acknowledges immediately and cannot become a never-submitted pre-cancel or trigger shared-engine quarantine. A deterministic runtime source case registers cancellation, publishes the result before processing the control snapshot, then confirms retirement with no close/pre-cancel state.
2. Worker start now checks `RequestAttempt.deadline` before committing the unique reservation. The SDK reservation samples time after owner/token/budget admission locks are acquired, and validates both operation and effective attempt deadlines at that commit point. Deadline rejection drops/refunds global and local reservations and enters the existing request-local `UnstartedRejected` terminal path. A defensive `Slot::start_connect` timeout also becomes a request terminal event rather than a worker error. Deterministic unrun sources inject a post-lock clock value at the attempt deadline and exercise the runtime worker-start gate; they confirm the start callback is not run, no attempt/local/global debit remains, and the local allowance can be reserved again.
3. Same-attempt legacy `ContinueEndpoint` dispatch now asks for a dedicated continuation reservation. It gates on owner/token/operation state but does not reserve another logical recovery and does not increment logical wire-attempt counters. Its effective attempt deadline is still rechecked by the same start gate. A deterministic source case consumes a `maximum = 1` recovery once, starts multiple endpoint continuations, and confirms another logical recovery remains rejected.
4. Operation cancellation/deadline is annotated before becoming the stored first root. Subsequent observations merge missing phase/command/operation/session context without replacing the original kind or source; a runtime diagnostic source is attached only when no first source exists. Confirmed retirement returns this stored cause, including the known command. The focused source case passes a typed runtime timeout through annotation, stored stop, and successful retirement delivery, then asserts its kind, source, operation/session/phase/command/host/attempt fields survive.

No verification was run for this scoped fix round: tests, collection, build, check, clippy, doctest, network, compile/AST, formatting, diff inspection, or equivalent remain deferred until T01–T30 completion. The four added regressions are authored source only; no runtime-success claim is made.

## Scoped review fix round 2

Corrected the injected-clock regression case identified against `fc90734`: it now obtains a concrete mutable `Reservation` directly from `OperationBudget::reserve(true, authorization.recovery, None)`, then invokes the existing concrete `commit_with_clock` method. The prior test called that method on `Box<dyn ReservedAttempt>`, whose trait API does not expose it. The test retains both local/global reservation and refund assertions; production admission gates and runtime trait semantics are unchanged.

Only `crates/eltdx/src/operation_tests.rs` and this report changed for round 2. No tests, collection, build, check, clippy, doctest, network, compile/AST, format/diff checks, or equivalent verification were executed. This is a source correction, not a compilation or runtime-success claim.
