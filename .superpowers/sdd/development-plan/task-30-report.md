# T30 source implementation report

## First checkpoint — native stress sources and evidence reader (stage111)

Base commit43ec6b5. T30 is **partial**, not source-complete or accepted. Current totals:28 source-complete /2 partial(T19/T30) /0 unstarted. The full unlimited goal remains active. No tests, collection, compile/build/check/clippy/doctests, dependency/network, AST/format/diff checks or equivalent execution ran. No subagents, publication, push, tag or merge.

Authored ten explicitly ignored cases across tests/stress.rs, tests/stress_mac.rs and the private cfg(test) src/stress_internal.rs. Full workload/source/evidence/threshold mapping lives in docs/rust-sdk/t30-evidence-index.md. Actual paths include100k wire requests among125k unique outcomes, admitted cancellation/late replies, close/pin/service handles, paused subscription overflow, malformed replies/pagination/download limits, reconnect/retry, real DAT/ZIP caps, F10 HTTP owner workers, MAC-EX catalog/macro cache ownership and actual SDK engine raw/decoded/queue/actor/generation telemetry. None has run.

The HQ loopback helper now assigns physical identities, counts live connections and reaps child JoinHandles during long workloads. Its first error is retained while all children are stopped/joined. Final ordinary regression must cover this shared helper change. SDK archive include adds its two small test helpers because the private library-test module uses them; release-policy.md explains this, production imports unchanged.

New scripts/verification/rust_sdk_stress.py builds before measuring, locates real release test binaries, runs each ignored case in its own process, observes process-tree RSS/threads/FD or handles/TCP/CPU with psutil, retains stdout/events/resource NDJSON and source/blob/lock/toolchain/command/exit/hash provenance. verify/aggregate re-open raw evidence rather than trusting a status field, validate identities/counts/budgets/resources/percentiles and require same clean candidate Linux+Windows. Independent manual rust-sdk-stress.yml preserves failed evidence; ordinary jobs do not execute load. All runner/workflow/dependency APIs are unverified.

Source-read corrections in this checkpoint: fixed cancellation result unwrapping; release fixture before propagating caller error; infinite directory must return LimitExceeded; actually exercise malformed company catalog; connect logical operation ID to physical wire identity; discard stale activation notifications; fixture errors reap all children. These are source edits, not verified fixes.

Remaining T30 source scope: complete performance workloads/runner/exports and old/new same-fixture policy, fuller workload readiness/warmup/configuration and close/cancel timing, final R01–R12 evidence index, rustdoc/examples/Python migration/release/maintenance, and focused source self-review. T19 still needs source completion before global execution gate. Production login/setup evidence remains absent; synthetic rules do not fill it. Do not run any new script yet.

Next recovery: read this report and t30-evidence-index.md, inspect current Git and code, continue remaining T30/T19 implementation. Do not mistake28 source-complete tasks for28 accepted tasks. Canonical docs and package mirrors must stay synchronized. Next checkpoint stage112, not another stage111.
