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

## Deferred integration

No business service or Engine creation path exists in `eltdx` yet. `SessionOwner::register_engine` is the controlled attachment point for later executor work; no Engine is created during build or close of an unstarted client. Public `PinnedSession` and subscription types remain with their scheduled service/subscription implementation tasks rather than being exposed as empty placeholder types.

## Verification

Per the user instruction, no tests, test collection, cargo build/check/clippy/doctest, network validation, diff checks, or equivalent verification commands were run. Test source was authored only. No compilation or runtime success is claimed.
