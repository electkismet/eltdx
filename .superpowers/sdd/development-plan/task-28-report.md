# T28 implementation report

## Stage 1 — external consumers and executable matrix sources (stage109)

Base55814ea,2026-11-17. Authored standalone extension.rs using only std/public
eltdx: EX independent92-byte setup plus markets/count/directory/quote and raw;
MAC all3 setup packets/nonempty selected fields/unknown slot/raw; MAC-EX exact
login/head rewrite/default7 fields with explicit synthetic-only reply rules.
Fragmented responses, bounded peer accept/read/write, close on operation error,
repeat close, peer EOF and closed-operation rejection are source requirements.
No direct bytes/protocol/runtime/binding imports were added to the consumer.

Added rust_sdk_matrix.ps1:32 explicit feature subsets/default/full,1.89/stable,
production closure trees, protocol/runtime/SDK tests, doc/doctest/examples,
clippy/fmt, external directory consumer with independent lock/target; command
logs/exit/time/hash and candidate/worktree/toolchain/lock metadata are retained.
Added Rust-only shell/container consumption disabled-module negative and
positive checks requiring known rustc errors, never dependency failure.
Manual rust-sdk.yml wires Linux/Windows/macOS x2 toolchains, subsets, separate
loom and Rust-only container, gated on explicit implementation completion.
Shell source LF is fixed through .gitattributes for Windows checkouts.

Added consume_python_wheel.py and fresh consumer jobs separate from builders.
Binary-only/no-index installation outside checkout, pip check, public nonempty
HQ/F10/offline/financial/EX/MAC/MAC-EX queries, native bytes equal wheel bytes,
ABI/tag/version/hash and failure/timeout logs are authored. Linux uses a Python
slim image and asserts tools absent before venv; Windows/macOS assert process
PATH isolation only. A dedicated compiler-free desktop image is still needed
for the stronger physical-absence claim; reports state that limit explicitly.
No workflow was dispatched. No tests/build/check/compiler/AST/format/dependency
resolution/install/network/equivalent verification executed. Candidate container
digests and real platform results remain final execution evidence, not authored
facts. Production MAC-EX login/optional MAC setup gaps remain unchanged.

T28 source scope complete; all execution acceptance boxes remain open. Counts
27 source-complete/1 partial(T19)/2 unstarted(T29-T30). Next finish package and
version/content/registry scripts, stress/performance/maintenance and T19 source
handoff; only then unified validation. Goal active unlimited. No agents, push,
tags, publication, merge or controller-state edits.
