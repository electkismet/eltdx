# T07 implementation report

## Scope

Implemented the SDK owner lifecycle foundation and absolute runtime close deadline entry point.

## Changes

- Replaced the client's atomic closed flag with an `Arc<SessionOwner>` held through the owner-bound HQ executor. Client clones therefore close the same lifecycle owner.
- Added one mutex-protected phase/admission state and active-operation guards. Close establishes the gate before shutdown work; later admission and Engine registration are rejected.
- Added first-close absolute deadline handling and generation-scoped completion for concurrent closers. Every registered Engine receives the same deadline; the existing runtime `Engine::close()` and `begin_close()` remain default-compatible wrappers.
- Preserved failed-close root cause. An explicit later cleanup attempt gets the configured cleanup timeout, retains registered engines, and still reports the original failure even if the cleanup attempt confirms resource shutdown.
- Kept owner construction and close-before-start free of runtime creation or network work. Dropping the final executor/owner releases its Engine fields; runtime Engine Drop retains its existing best-effort close signal.
- Added owner-bound `hq::executor` as the narrow future service/operation boundary. Python per-domain shutdown behavior remains outside this SDK owner and is reserved for the later Python binding tasks.
- Authored lifecycle source cases for the shared close gate, concurrent closers, owner retention by a service-like executor handle, clone terminal behavior, and close-before-connect using a loopback listener.

## Review follow-up

- Each admitted operation now has an owner-signalled cancellation token. Entering `Closing` broadcasts cancellation, and guards expose `is_cancelled` / `wait_cancelled` for synchronous operation paths; the Engine deadline close still terminates runtime wire work.
- Added the non-Clone public `PinnedSession` wrapper with a private constructor, owner registration, explicit idempotent release, and best-effort release request on Drop. Its owner registration is removed when the guard releases; registered pins are released before Engine shutdown. Subscription implementations can use this same resource-registration boundary as they are added rather than adding another lifecycle state machine.
- Updated the concurrent-close unit source case to attach and start a real Engine runtime configured against a loopback endpoint, so the close race includes an owned runtime resource. The existing lifecycle integration source remains focused on client-level behavior.

## Deferred integration

No business service or Engine creation path exists in `eltdx` yet. `SessionOwner::register_engine` and the crate-private `PinnedSession::new` are the controlled attachment points for later executor work; no Engine is created during build or close of an unstarted client. Subscription remains to be implemented with the same owner child-resource lifecycle.

## Review follow-up, round 2

- Replaced both SDK uses of unbounded `PinHandle::close` with one owner helper. An explicit pin close establishes a fixed local `close_timeout` cap; an owner-driven pin close receives the owner's existing absolute deadline. Every wait iteration takes the earlier local/owner deadline, so concurrent owner close can shorten the explicit wait and a later cleanup attempt cannot extend it.
- Pending pin retirement is polled without blocking, then the existing owner condition variable is waited with `min(remaining, SIGNAL_POLL_INTERVAL)`. The Closing transition notifies that variable under the state lock. No owner state lock is held while polling the runtime, and owner close continues to every Engine even if any pin close fails. Each Engine still gets the same absolute owner deadline.
- Kept `SessionState.pins: Vec<PinHandle>` as the tracking structure and added no independent resource lifecycle state machine. A successful explicit pin close unregisters and clears the local guard. A failed close retains both guard and registration; Drop sends best-effort close but does not unregister. Owner close removes a registration only after confirmed pin retirement, preserving failed resources for cleanup retries.
- Added runtime `PendingPinClose::wait_until` and `PinHandle::close_until` as bounded entry points. The original `wait`, `close`, and `begin_close` wrappers retain their prior behavior. A pending deadline failure does not consume or overwrite its retirement receipt, allowing later confirmation.
- SDK close error mapping now keeps the runtime kind, context, and source. The shared close-failure snapshot retains that runtime source when reproducing the first error for concurrent callers or cleanup retries.
- Authored, but did not execute, source cases for a pending receipt expiring and later confirming, runtime error preservation, explicit pin deadline caps, concurrent owner deadline shortening, local cap preservation during a longer cleanup deadline, and loopback pin Drop retention / explicit-close-versus-owner races. The loopback cases use a real handshake and observe socket EOF after owner shutdown.

Static source inspection only was used in this round. No tests, collections, builds, checks, clippy, doctests, network validation, diff checks, or equivalent execution was performed. Runtime and compilation behavior remain unverified until the final T01–T30 validation phase.

## Review follow-up, round 3

- Added the narrow `PinHandle::belongs_to_engine` identity predicate using `Arc::ptr_eq`. It exposes no Engine reference and makes no inference from stopped/starting lifecycle states. Following a successful `Engine::close_until`, the owner removes only the pins issued by that exact Engine and wakes its existing condition variable.
- Added `PinHandle::same_reservation` to compare both Engine owner identity and `PinIdentity`; all owner pin-registration lookups and individual unregister operations use that predicate. Separate pools can safely reuse local counters. Two additional real-loopback source cases create two independent Engines with equal local pin identities and cover individual pin release and full Engine reconciliation without deleting the other Engine's pin; both runtimes are subsequently closed.
- The SDK pin-close helper recognizes an absent owner registration as already confirmed. When beginning or waiting for runtime pin close fails, it rechecks the registration under the owner lock before deciding whether to return success; a `ConnectionClosed` error alone is never treated as release confirmation. Explicit close of a guard after owner-confirmed shutdown is therefore idempotent without sending a new request to the stopped runtime.
- Resource confirmation is now independent of the outward first-error lineage: `cleanup_confirmed` requires all Engine closes in the attempt to succeed, no remaining registered pins, and zero active operations. A failed pin receipt remains the reported root error even if subsequent full Engine shutdown confirms retirement. Once cleanup is confirmed, repeated owner close returns the original structured failure without starting unnecessary cleanup generations.
- Kept the existing pin vector and owner lifecycle state machine. A private `close_with_pin_closer` seam lets source cases withhold a pin confirmation receipt while using the real remaining owner close implementation; no public testing API or production shutdown policy was added.
- Added source cases using a real loopback Engine and pin: a real pin-close command with a deliberately withheld receipt times out, the actual Engine close/join reconciles pins and confirms cleanup, later owner close preserves kind/context/runtime source, and explicit guard close succeeds. Additional source cases cover ordinary explicit close after confirmed owner shutdown and retained registration after a stopped Engine error, plus a runtime identity-predicate case distinguishing two otherwise equal Engine configurations.

This round used read-only source inspection and source edits only. No test/build/check/collection/doctest/clippy/network/diff validation or equivalent execution was performed; all new cases remain unexecuted until T01–T30 implementation is complete.

## Verification

Per the user instruction, no tests, test collection, cargo build/check/clippy/doctest, network validation, diff checks, or equivalent verification commands were run. Test source was authored only. No compilation or runtime success is claimed.
