# T17 Stage A2 scoped static review

Reviewer `/root/t17_stage_a_review`, base `cd6735b`, head `bcd6a8f`, package `task-17-stage-a2-bcd6a8f.diff`.

Spec compliance: **Approved for the A2 changes**. Code quality: **Approved**. No Critical, Important or Minor findings.

The original table and generator were moved into private `encoding/` without regenerating the table. `gbk_scalar` implements strict Python GBK two-byte mappings; the shared scanner supports invalid-byte `Replace` and `Ignore`, one-byte recovery and bounded cancellation callbacks. Existing offline `decode_gbk` delegates using replacement, while stats continues to ignore invalid bytes using the shared scalar table. The root change is private module registration only.

Review evidence: `encoding/mod.rs:21,39,54`, `offline/mod.rs:53`, and task-17-report provenance at line20. No tests, collection, build/check, dependency resolution, network, compiler/AST, format/diff checks or repository writes were performed. This approves source organization/scanner only, not T17 service wiring or runtime behavior.
