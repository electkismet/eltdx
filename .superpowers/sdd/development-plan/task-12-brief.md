# T12 — 第一条跨语言业务链

Read this first: requirements authority is `docs/rust-sdk/development-plan.md` T12; `execution-and-compatibility.md` §§1–7; `public-api.md` §§6–10; `hq-api-spec.md` §§3–5; `compatibility-cases.md` COMP-001–015 and COMP-028/029; frozen Python baseline and current T08/T11 compatibility interfaces. Start only with controller GO after reviewed T11.

## Exact plan scope

Modify `crates/eltdx-python/Cargo.toml` and binding `lib/request/response/transport`, add `service.rs`. Modify Python `src/eltdx/api/{quotes,codes}.py` and `client.py`; author `tests/contracts/test_sdk_quote_bridge.py` and necessary domain compatibility source cases.

Native service calls SDK's hidden compatibility bridge into the shared Rust domain algorithm and returns the original tuple/dataclass projection. Keep Python aliases, exception mapping and per-request timing.

- Same fake/loopback inputs compare frozen old Python facade, public Rust service and new Python facade, including all fields, order, original scalar/raw values and errors.
- Cover GIL release, KeyboardInterrupt, exception `__cause__`, custom transport, and absence of blocking I/O while holding GIL.
- Remove old business algorithm from native normal path. A temporarily retained custom-transport path must be explicit and covered with an exit condition.
- Deferred validation: `python -m pytest -q tests/test_client.py tests/test_helpers.py tests/contracts/test_sdk_quote_bridge.py tests/contracts/test_native_errors_contract.py`, SDK quote source tests, and independent Rust consumer compatibility.

## Binding contracts

Use `eltdx::__python_compat`, no private crate visibility tricks. Normal Rust client has no legacy switch; legacy generation is distinct and owns its existing transport/domain gates. Python has original startup timing then per-wire timeout; no new total 8s/32-extra cap/65535 input restriction or Rust cursor semantics imposed. One shared codes/quotes algorithm; facade should perform input/model/error compatibility only. Do not call public Rust strict methods then undo errors or results in Python.

Reuse the existing Python ABI conversion once. Preserve model constructors, list versus tuple versus dict, raw_payload/record_hex rules and QuoteSnapshot original unknown/tail fields, price resolver and metadata-prefetch semantics. Additional protocol raw preservation fields from T09 are internal only, not added Python dataclass fields. Category count/sort and lenient code aliases preserve old acceptance and error precedence. Snapshot singleton absence produces original MissingCodesError or one MissingCodesWarning and shortened ordered rows in skip mode; corrupted records, extra unknown IDs and timeout remain real failures. COMP-028 duplicate occurrences A/B/A must retain distinct 10/12 A values when returned independently.

Native normal calls must operate the same runtime/connection identities and existing pin/push state rather than silently creating an unrelated SDK Engine per call. Adapt existing Engine to legacy SDK generation/executor if needed, with explicit ownership and shutdown responsibilities. Persist one bridge per Python client/transport generation. Old service proxy may follow new generation after reconnect; old pins/subscriptions do not revive. Preserve close ordinary→money-flow→auction phase sequence and independent deadlines/short-circuit semantics. T15 will migrate push dispatcher; keep old refresh/poll/subscription behavior in the meantime, never create a second consumer queue.

For cancellable native waits, detach GIL before blocking, regularly check signals, request cancellation and confirm cleanup within original policy. Signal exception remains primary; cleanup failure becomes cause as existing native transport does. Cancellation and pin receipt handoff keep RAII ownership until Python holder/model allocation succeeds. Error retains SDK kind/phase/op/command/endpoint/attempt/source and is mapped by binding to original Python type/attributes/cause; SDK never stores PyErr. Callback transport exceptions remain Python objects at binding scope and restore without stringifying.

Custom transport: minimum supported object only needs the existing execute interface, not inheritance or new methods. Prefer callback executor into same algorithm, invoked on controlled caller thread with GIL and no SDK/runtime lock. Reentrant callbacks must not deadlock. Preserve diagnostic non-list envelopes and arbitrary compatible custom objects. If preserving this cannot be done without changing established behavior, keep only the explicit custom-object compatibility path and record exact entry, reason, tests and exit condition in migration manifest/report; ordinary native path still uses Rust service. Do not replace all Python code just to preserve a narrow callback case.

Migration manifest may upgrade only actually implemented public/business entries and must provide concrete root `lib.rs::PublicSymbol` evidence, source_files, exact pytest nodes and fixture IDs per T01. Exported service symbols under `services` can be evidenced by an actual root public entry TdxClient plus service implementation sources; do not invent declarations in comments or claim all HQ migrated because codes/quotes are switched. Freeze baseline independently: do not regenerate goldens/API snapshot to fit drift.

## Workflow

Workspace `C:/Users/ax/.codex/worktrees/rust-sdk-implementation/tdx抓包`, branch `codex/rust-sdk-implementation`. All verification prohibited until all T01–T30 code is complete: no tests/collection/build/check/clippy/doctest/network/compile/AST/format/diff checks/equivalents. Author source only; ordinary review diff generation and git checkpoint commands allowed. No subagents, no push/tag/publish/merge, no controller progress/status edits. Coordinate shared SDK files before edits, commit exact scope with hooks disabled.

Full report `.superpowers/sdd/development-plan/task-12-report.md`: scope and per-entry migration, authored cases, original compatibility provenance, exact changes/commits, no-execution statement, GIL/cancellation/ownership reasoning, remaining adapter paths with exit conditions and self-review concerns. Return compact status/commits/no-tests/concerns/report path. No unmeasured success claims.
