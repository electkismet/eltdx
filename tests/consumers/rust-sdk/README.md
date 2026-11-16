# Independent Rust consumer

This is a source-only acceptance fixture for T18/T23/T28. It has not been compiled
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

`f10` (`sdk-f10`) uses a separate std-only HTTP fixture. Six real POSTs cover a
business-error response with duplicate columns and cache metadata, a named
stock endpoint, typed index members, both required topic sources and explicit
topic comparison. Expected Entries and bodies are independent literals. It
checks order/raw/source preservation, that HTTP-only helpers never connect HQ,
and that closing F10 closes the common Rust owner.

`offline` (`sdk-offline`) writes and rereads a caller-owned temporary daily
file, retains negative-zero/raw bytes, round trips records, checks the fixed
original-tail append rule with duplicate rows and verifies partial-tail
rejection leaves bytes intact. It also checks industry duplicate ordering.
Only two named files in the created temporary directory are removed during
cleanup; no recursive deletion is used.

`financial` (`sdk-offline`) constructs a ZIP/DAT without a ZIP dependency in the
consumer, checks exact float bits and archive identity, then performs real HQ
list and archive downloads through the public financial service. The fixture's
stale size and opaque hash do not stand in for complete download evidence.
The result must retain both physical sources and the filename's report date.

After **all T01–T30 implementation sources are complete**, the unified validation
pass can run these commands, initially resolving a new standalone lockfile:

```sh
cargo generate-lockfile --manifest-path tests/consumers/rust-sdk/Cargo.toml
cargo run --manifest-path tests/consumers/rust-sdk/Cargo.toml --locked -- lazy
cargo run --manifest-path tests/consumers/rust-sdk/Cargo.toml --locked -- loopback
cargo run --manifest-path tests/consumers/rust-sdk/Cargo.toml --no-default-features --locked -- loopback
cargo run --manifest-path tests/consumers/rust-sdk/Cargo.toml --no-default-features --features sdk-full --locked -- loopback
cargo run --manifest-path tests/consumers/rust-sdk/Cargo.toml --no-default-features --features sdk-f10 --locked -- f10
cargo run --manifest-path tests/consumers/rust-sdk/Cargo.toml --no-default-features --features sdk-offline --locked -- offline
cargo run --manifest-path tests/consumers/rust-sdk/Cargo.toml --no-default-features --features sdk-offline --locked -- financial
```

T28 must additionally copy this entire consumer outside the repository, replace the
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
