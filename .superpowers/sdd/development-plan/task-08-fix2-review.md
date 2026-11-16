# T08 fix round 2 scoped review

Fix base `fc90734`, head `d8574bb`, package `task-08-fix2-d8574bb.diff`.

Important source-case mismatch ADDRESSED: `operation_tests.rs:213` obtains a mutable concrete Reservation directly from OperationBudget::reserve; commit_with_clock at line217 now resolves to its concrete method. Local/global recovery-budget participation and refund assertions remain.

No new Critical, Important or Minor breakage. Round accepted by source review. Original four findings were already addressed in round1 and were not reopened. No tests, builds, git commands or other verification were executed; compilation/runtime remain unconfirmed.
