# T08 task review — b3371c0

Spec compliance: issues found. Task quality: Needs fixes. No Critical findings. Four Important findings below; no additional Minor findings. Source review only, no execution/validation/mutations. Future domain entry/finish, T10 snapshots/flights and actual Python COMP differential behavior stay later work, not blockers for this task.

## Important findings (verbatim issue identities for fix loop)

1. **Cancellation can miss an already published terminal result and falsely quarantine the shared engine.** `crates/eltdx-runtime/src/engine.rs:935`: begin_cancel checks result_rx then registers cancellation without a shared terminal-publication handshake. Completion can remove/publish between those steps; subsequent cancel_request sees no pending request while Running and stores a pre-admission cancellation whose ID will never submit again. Retirement was already successful yet confirmation never completes, producing CancelUnconfirmed and engine quarantine that affects unrelated requests. Carry shared terminal/admission state through cancellation registration; completed requests acknowledge immediately. Add deterministic unrun completion-versus-cancel case.

2. **Worker start does not validate the effective attempt deadline before committing its reservation.** `crates/eltdx-runtime/src/engine.rs:5556,5662`, `crates/eltdx/src/policy/retries.rs:63`: permit checks operation deadline but RequestAttempt deadline may be a shorter fair slice. Expiry before worker execution commits counters then start_connect Timeout escapes execute_work, worker Stopped triggers runtime fatal; an ordinary expiry closes shared work and counts an attempt that never connected. Also gate samples time before admission locks so commit can use stale time. Recheck effective deadline at worker start, sample actual commit time after gate locks, pre-start expiry goes through unstarted rejection/refund/request-local terminal.

3. **Legacy endpoint continuation consumes the same logical recovery allowance again.** `crates/eltdx/src/policy/retries.rs:223`, `crates/eltdx-runtime/src/engine.rs:4248`: ContinueEndpoint redispatches unchanged attempt1 and re-reserves/starts RecoveryLimit. With local max1, connect failure at endpoint1 blocks endpoint2 as legacy_recovery_limit although same logical recovery child. Track once-only logical recovery consumption or explicit continuation distinction, so legacy endpoint cycling cannot debit it twice.

4. **The common cancellation/timeout path discards its structured annotations.** `crates/eltdx/src/policy/retries.rs:76`, `crates/eltdx/src/hq/executor.rs:126`: budget.check stores unannotated root; context.check_boundary annotates only returned clone; retire_cancelled context.stop returns earlier unannotated root. Confirmed cleanup then returns no operation/id/session/phase/command. Store/return annotated first cause consistently and include known command without replacing original kind/source.

## Strengths and focused checks

Transactional global/local reservation and unstarted Drop refunds (`retries.rs:98`), common profiles/scheduler and after-startup legacy timing (`runtime/execution.rs:14`, `engine.rs:1573`), typed source and separate cleanup error (`error.rs:87`), exact unstarted pin identity/sequence/lease/FIFO checks (`pin.rs:485`), meaningful source transition cases (`operation_tests.rs:73`).

Named checks only: execute_work expanded because diff cut required response branch, then Slot::start_connect/EndpointRotation::next/Stopped for timeout fatal path; process_notifications/cancel_request/event-loop/current_interrupt for terminal race; changed timing/attempt API callers in runtime/Python for signature risk; unchanged RuntimeError::context at error.rs:163 for available phase fields. No broad review, git commands, tests, collection, build/check/clippy/doctest/network/compile/AST/format/diff checks or equivalent validation.

Review package `task-08-review-b3371c0.diff`, base `98e8558`, task-owned paths only. Fix round1 must address only these findings and introduced breakage; broader review remains end-of-plan.
