# T21 scoped source review

Base `230e566`, reviewed head `9734db3`, package `task-21-review-9734db3.diff`. Static source review only; no commands validated code behavior.

Initial spec verdict: mostly compliant, one Important mismatch. Code-quality verdict: Needs fixes.

- Important: `crates/eltdx/src/offline/block.rs:118` skipped an empty customer-block leaf with a diagnostic, contrary to the brief's requirement to reject empty leaf names. Reject as ResourceFormat while preserving the separate missing-file and empty-block-name skips.
- Associated Minor: `crates/eltdx/tests/offline_blocks.rs:75` codified that empty-leaf skip; update the source case to assert rejection and retain independent skip cases.

The review supported byte layouts and source-value retention, whole-batch encoding before mutation, original-key append ordering, alignment checks, actual byte-count errors, durability reporting and symlink containment. Runtime behavior, GBK generated-table parity, platform paths and pending T10 Diagnostic integration remain unverified.

## Fix round 1

Commit `b6e4beb`; package `task-21-fix1-b6e4beb.diff`, base `9734db3`. Scoped re-review verdict: Approved, both findings ADDRESSED.

`block.rs:118` now rejects an empty leaf with ResourceFormat and record_index before path lookup. The later missing `.blk` and empty block-name skip at line132 remains. The case at `offline_blocks.rs:95` asserts ResourceFormat and row index0, and separate successful cases keep missing-file/empty-name diagnostics. No new breakage or out-of-scope observations were identified. No tests, builds, generators, git commands or equivalent verification were executed by the reviewer.
