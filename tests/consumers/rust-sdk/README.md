# Independent Rust consumer

This is a source-only acceptance fixture for T18/T28. It has not been compiled
or executed. Its only direct dependency is the public `eltdx` library. The
fixture does not import protocol/runtime crate APIs, Python, PyO3 or repository
test helpers, and does not start subprocesses.

`lazy` obtains service handles and closes an unstarted client twice. `loopback`
starts a std-only server bound to `127.0.0.1`, exercises a nonempty three-page
directory collection and a nonempty snapshot request with duplicate input and
reversed response ordering, checks source/raw retention, closes the client and
checks rejection after close. The fixture server uses authored byte layouts and
fixed expected prices, rather than deriving expectations with the SDK parser.
It is a synthetic HQ contract, not a real-server capture.

After **all T01–T30 implementation sources are complete**, the unified validation
pass can run these commands, initially resolving a new standalone lockfile:

```sh
cargo generate-lockfile --manifest-path tests/consumers/rust-sdk/Cargo.toml
cargo run --manifest-path tests/consumers/rust-sdk/Cargo.toml --locked -- lazy
cargo run --manifest-path tests/consumers/rust-sdk/Cargo.toml --locked -- loopback
cargo run --manifest-path tests/consumers/rust-sdk/Cargo.toml --no-default-features --locked -- loopback
cargo run --manifest-path tests/consumers/rust-sdk/Cargo.toml --no-default-features --features sdk-full --locked -- loopback
```

T28 must additionally copy this consumer outside the checkout, replace the
`eltdx` path with the selected candidate's absolute path, resolve its own
lockfile, and execute with a separate target directory in an environment without
Python. Check its normal/build dependency graph for Python discovery, then
repeat against unpacked packages and eventually published registry candidates.
Merely having `[workspace]` in this checked-in fixture does not prove external
package completeness, registry availability, MSRV or absence of Python on the
executing machine. CI integration, feature/platform matrices and corresponding
evidence remain pending under T28/T29.

The `hq_query` example in the SDK package is separate: it requires an explicit
server address and security arguments when run, and performs an actual query.
It uses the default HQ feature set, a finite whole-operation timeout, typed units,
diagnostics and explicit cleanup. It has not been run against a live server.
