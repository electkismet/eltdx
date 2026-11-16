# T24 Stage A scoped static review

Reviewer `/root/t24_stage_a_review`, base `b6e4beb`, head `83540ec`, package `task-24-stage-a-83540ec.diff`.

Spec compliance: **Issues found**. Code quality: **Needs fixes**.

## Important

- `crates/eltdx/src/ex/commands.rs:82–88,109–111`: `counted()` accepts any bytes beyond declared rows and `finish()` only emits a trailer diagnostic. A bars payload with one declared row and 33 bytes after its 20-byte header succeeds despite the extra partial row. Markets, instruments, quote-list, minutes and transactions share this behavior. Extension specification §1.1 requires partial-record payloads to fail with Protocol. Enforce the complete command layout, permitting only independently supported trailers, and author partial-tail cases.

## Minor

- `crates/eltdx/src/ex/time.rs:5–8`, `commands.rs:240–242`: malformed minute/second errors omit original values. Carry raw minutes/seconds and transaction direction in error context per §3.3.
- `crates/eltdx/src/ex/models.rs:64–70`: InstrumentInfo exposes market/code only through id(), while §1.1 requires same-name readonly getters. Add forwarding market()/code().

## Cross-task handoff

The Stage B page result must truthfully express short ordinary pages with unknown EOF. Actual association rules need captures. Futures source has the documented 34-name/35-slot mismatch. Shared endpoint spelling is a whole-project integration concern, not attributed only to T24. Public client/owner/transport/Cargo/root remain outside Stage A.

The reviewer found all eleven request layouts and principal record widths consistent with source, exact frame caps and zlib checks, scoped sequence leases and raw preservation. Review used only source reads and focused protocol comparisons. No test, compiler, build/check, dependency, network, AST/format/diff check or Git mutation was performed.
