# eltdx-runtime

TDX transport engine with request dispatch, pending execution, pinning and
connection lifecycle. The candidate retains version 3.3.5 and requires Rust
1.89.0. The included non-commercial learning and data research [LICENSE](LICENSE)
applies; the new SDK does not change this project's licensing terms.

Ordinary Rust callers should use the business `eltdx` crate. Its owner,
operation deadline and service contracts sit above this lower-level engine.
Internal Tokio threads do not make a synchronous call publicly async.

The runtime depends on the exact candidate `eltdx-protocol` version, with a
local path for development and a version requirement for package consumers.
This source candidate has no new compile, test, package or registry evidence.
The separate runtime fixture and loom gates must run before release acceptance.
