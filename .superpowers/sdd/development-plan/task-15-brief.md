# T15 — 订阅、单消费者分发和 gap

Read first. Authority: `docs/rust-sdk/development-plan.md` T15, `public-api.md` §§4–5, `hq-api-spec.md` §§3–5, `execution-and-compatibility.md` §§3,6–7, COMP-033/034, `decision-register.md` §3. Current reviewed owner/operation/metadata/quote APIs are dependencies. Controller gives GO.

## Exact task and complete scope

New `src/subscription/{mod,dispatcher,cursor,lease}.rs`, extend `services/quotes.rs`; new `tests/subscription.rs`. Implement QuoteService::subscribe M24, Subscription/SubscriptionOptions/GapPolicy/PushEvent/PushGap, and PinnedSession::quotes()->PinnedQuoteService<'_> with refresh M23. Unique non-Clone pin/subscription ownership; service getter shares owner and remains lazy.

- Metadata before pin reservation, precheck impossible configured group capacity before taking first pin; groups of100, total entry deadline covers sequential acquisition/activation. Concurrent occupancy still needs bounded acquisition and complete rollback on failure.
- Second-group failure releases first-group and all acquired resources. Failed renewal must not leave active-looking subscription.
- One dispatcher consumes runtime queue; fanout routes by exact engine epoch/slot/TCP generation/security. Consumers cannot steal each other's pushes.
- Cursor is source time-of-day, may roll150000→93000; next renewal uses93000, not max.
- Explicit close/Drop/client close, slow consumer, queue overflow and recovery have meaningful test source.
- Deferred `cargo test -p eltdx --test subscription --locked`; real1000-security online case opt-in only, not default suite.

## Binding behavior

Public shapes follow §5 exactly: SubscriptionOptions default queue1024 events/64MiB, renewalSome7s, gap ReportOnly; setters validate at subscribe before I/O. Both budgets positive, a supported maximum event must fit. Initial establishment OperationOptions is not retained after success. Renewal creates fresh finite independent operation context (ClientConfig default timeout), inherits session/subscription stop, not old setup token/deadline. User next_event options govern only that read wait: timeout or cancellation does not unsubscribe, Err Timeout/Cancelled; Ok(None) only normal own-close/end. Own explicit close then read=>None; parent close/failure=>Closed/root failure. next_event must wake without needing consumer polling to retire pins.

Subscription owns strong owner and pins; dispatcher/renewal workers hold only Weak owner or independent control state, no permanent strong capture/self-cycle. Finite per-operation weak upgrade permitted. Dropping last owner synchronously closes admission and sends stop without blocking join/I/O. Explicit client close confirms all workers/pins under T07 shared absolute shutdown deadline. Reuse T10's owner-registered background stop/join hook if available; no second cleanup state machine. Subscription close is bounded/idempotent, does not close unrelated subscriptions or whole client. No user callback background worker in Rust API; Python callbacks stay existing caller thread at migration.

Dispatcher is unique per Engine/owner queue, including multiple groups and raw compatibility poll/drain/advanced paths. Route all consumption through controlled fanout or explicitly exclusive mode. Never leave old native poll/drain competing with dispatcher. If Python bridge migration is later T18, add a narrow controlled consumer adapter here so T18 can preserve old raw/error projection. Preserve PushOverflowError counts/projection for Python; don't expose new Rust enums directly. Handshake/session snapshots from existing pin stay source-bound.

Queues account actual held event/raw payload memory independently of runtime raw-frame queue (existing runtime default bytes may differ). Gap uses independent reserved control state/coalesced count/range, cannot be dropped because Update queue is full. Event>byte cap reports explicit configuration limit/terminal state. ReportOnly preserves desync status on subsequent best-effort updates; Resynchronize emits Gap then finite snapshot/cursor restore then Resynchronized only on success; Close emits Gap then stops/releases. Protocol generation mismatch/stale old frame cannot update cursor. Reconnect/source loss invalidates old pin and reports gap/error; no silent rebinding of immutable pin identity.

Refresh request single wire<=100, empty is legitimate activation/poll on same pin (unlike ordinary empty batch). PinnedQuoteService borrows pin and cannot escape lifetime or clone into an owned ordinary service. Serial execution uses existing pin runtime path. Temporary depth and subscription setup reuse same decode/projection, immutable metadata snapshot and RawData policy; avoid copied quote parser/algorithms.

## Source cases and workflow

Read `src/eltdx/api/{subscription,quotes}.py`, runtime `push.rs` and Engine::poll_push/drain_push/PinHandle, current session resource tracking. Author deterministic channel/barrier/controlled-clock source cases:101 codes/group100+1/pool2/cold metadata, second activation fails=>2 releases; pool1=>0 pins/activations; dynamic capacity race; two overlapping subscriptions receive independent updates from one poll; stale epoch/slot/gen ignored;150000→93000 then renewal; queue2 plus3 updates=>reachable single gap; cancelled next_event leaves subscription active; setup token cancelled after established does not kill it; renewal uses fresh deadline; parent close without read releases; Drop no owner cycle; resync failure/oversized event explicit. No short sleep synchronization.

Workspace `C:/Users/ax/.codex/worktrees/rust-sdk-implementation/tdx抓包`, branch `codex/rust-sdk-implementation`. Until ALL T01–T30 implementation code complete, NO tests/collection/build/check/clippy/doctest/network/compile/AST/format/diff checks or equivalent. Author source only, ordinary review diff generation and git checkpoints allowed. No subagents, push/tag/publish/merge, or controller ledger/status edits. Coordinate shared session/quote/bridge hooks, commit exact scope with hooks disabled.

Full report `.superpowers/sdd/development-plan/task-15-report.md`: behavior/interfaces/ownership and stop ordering, exact files/commits, authored cases, no-execution, T18 Python poll adapter, self-review concerns. Return compact status/SHAs/no-tests/concerns/path. No compile/runtime claims.
