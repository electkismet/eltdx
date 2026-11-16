# T08 fix round 1 scoped review

Review base `b3371c0`, head `fc90734`, package `task-08-fix1-fc90734.diff`. Source-only; no validation commands executed.

All four original Important findings ADDRESSED:

1. Terminal/cancel race: shared terminal flag is rechecked at registration (`engine.rs:657`) and cancellation consumption (`engine.rs:4801`); terminal reducer marks it before result removal/publication (`engine.rs:4479`).
2. Effective deadline and stale time: worker start checks RequestAttempt deadline (`engine.rs:5128`), SDK samples time after admission locks and checks both deadlines (`retries.rs:81`); pre-start failures remain refundable and enter request-local UnstartedRejected (`engine.rs:5630`, `5735`).
3. Legacy endpoint continuation: unchanged attempt uses continuation authorization (`engine.rs:3819`), and gated continuation reservation carries no repeated extra/recovery or logical wire count (`retries.rs:266`).
4. Annotated first cause: operation stores annotated error (`operation.rs:231`), budget merges missing context (`retries.rs:100`), executor adds known command before retirement (`executor.rs:126`), existing source is retained (`operation.rs:304`), confirmed cleanup returns root (`executor.rs:158`).

## New Important finding in fix diff

`crates/eltdx/src/operation_tests.rs:216` calls `commit_with_clock` on the value returned by `authorization.reserve`, which is `Box<dyn ReservedAttempt>`. The trait exposes start/start_before only; commit_with_clock belongs to concrete Reservation. Construct a concrete mutable test reservation or add a narrow test-only concrete helper. This is a source type-check defect, not an executed compiler result.

Round verdict: Needs fixes. No new Critical or Minor findings. No out-of-scope blockers. Reviewer read the supplied diff and expanded cut-off affected functions only.
