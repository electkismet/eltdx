# T06 implementation report

## Scope

Implemented the T06 runtime deadline and raw reply bridge in `crates/eltdx-runtime`.

## Changes

- Added `execution.rs` with public `SubmissionOptions` carrying a caller-owned absolute `Instant` and public `RawReply` carrying the owned request, `ResponseFrame` (including raw wire bytes), request id, engine epoch, slot id, TCP generation, and endpoint host.
- Added raw pending and synchronous execution APIs on `Engine` and `PinHandle` (`begin_execute_raw_with_options`, `execute_raw_with_options`) plus absolute-deadline pin and connect entry points.
- Kept the existing typed `execute(CommandRequest)` path as a wrapper over the shared raw execution mechanism; legacy requests create their deadline only after runtime startup, preserving the historical boundary.
- Propagated one absolute request deadline through runtime startup, admission, queueing, slot connect, send, retry, receive, and terminal delivery. Retry attempts continue to use the original deadline.
- Preserved the raw frame until delivery and perform legacy typed parsing only in `PendingExecution`, after the shared runtime path has released request ownership. Raw delivery, parser failures, and dropped results therefore use the existing terminal/retirement cleanup path.
- Added `Deadline::from_instant` and a `RequestDeadlinePolicy` bridge for absolute versus legacy-after-startup timing.
- Added loopback source cases covering expired-before-start submission, raw frame/context delivery, typed compatibility and parser failure cleanup, queued deadline expiry, and a reconnect/retry that is held past the original absolute deadline.
- Added asynchronous cleanup for an explicit connect caller whose own deadline expires: the caller gets a bounded timeout, the shared runtime is asked to close, and a cleanup helper finalizes the host connect attempt after runtime exit. Coalesced callers still observe the shared physical attempt result while their waits remain bounded by their own deadlines.

## Source-review notes

The explicit connect path remains deadline-aware because the public API contract requires pin/connect options. The physical coalesced connect batch is governed by the creator's deadline; joining callers keep independent wait bounds. Deadline expiry now requests runtime close and schedules host-attempt cleanup so the host does not remain indefinitely in `Starting` solely because the last pending waiter returned early.

## Checkpoint commit

`4bf47d7` — `feat(runtime): add absolute deadlines and raw replies`

## Verification

Per the user instruction, verification was intentionally deferred until all T01–T30 implementation code is complete. No tests, cargo commands, build/check/clippy/doctest commands, network checks, AST scripts, diff checks, or equivalent verification were run for this checkpoint.

## Remaining concerns

- The deadline cleanup helper relies on the runtime exit completion and a best-effort background thread; a runtime that cannot publish exit completion would require the later shutdown work to report the failure lineage.
- The loopback tests use the repository's existing generated identifier-format method spelling and should be compiled only in the later full-repository verification pass.

## Review fix

- Recheck the caller's connect deadline after a bounded completion wait returns without a shared terminal. If that caller's deadline expired during the wait, return its timeout and schedule the cleanup watcher before consulting runtime exit, so the caller cannot publish a runtime-exit error into the shared completion.
- Keep cleanup attached to the shared physical attempt. The watcher waits for the shared terminal or runtime exit, finalizes host state, and exits after successful completion; waiter timeout does not request runtime close.
- Updated the source-only loopback case to wait concurrently on short- and long-deadline coalesced callers, then confirm the short caller times out, the long caller succeeds, and the Engine remains usable.
- Verification remains deferred per instruction until T01-T30 implementation code is complete. No test, build, check, lint, doctest, network validation, AST, diff, or equivalent verification command was run for this fix.
