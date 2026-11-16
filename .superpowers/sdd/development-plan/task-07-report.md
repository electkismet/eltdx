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

## Verification

Per the user instruction, no tests, test collection, cargo build/check/clippy/doctest, network validation, diff checks, or equivalent verification commands were run. Test source was authored only. No compilation or runtime success is claimed.
