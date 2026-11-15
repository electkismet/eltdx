# T04 Implementation Report

## Changed files

- Added `crates/eltdx/Cargo.toml` with the empty default feature set and the documented `serde`, `offline`, `f10`, `ex`, `mac`, and `full` feature names. The crate depends on the existing protocol and runtime crates and has no PyO3 or HTTP dependency.
- Added `crates/eltdx/src/lib.rs`, `client.rs`, `config.rs`, and `error.rs` for the selected public facade, pure validated builder, shared lazy client owner, and language-neutral errors.
- Added `crates/eltdx/src/types/{mod,security,time,numeric,raw,batch}.rs` for the foundational public types.
- Added `crates/eltdx/tests/config.rs` with deterministic source tests for defaults, invalid config, endpoint ordering, clone/close, strict identity normalization, and date validation.
- Added `crates/eltdx` to the root workspace in `Cargo.toml`.

## Design notes

`build()` validates only in-memory values. The owner contains an atomic terminal close flag and is shared with `Arc`; there is no runtime, thread, resolver, socket, or filesystem initialization in this task. Explicit endpoints remain in caller order and reject empty lists and port zero. Configuration is private and exposed through read-only getters.

`SecurityId` supports strict six-digit ASCII input, explicit HQ market prefixes, and the documented bare-code inference examples. Ambiguous bare bond ranges are rejected instead of guessed. `EventTime` distinguishes Shanghai-local, source-local, trading-date-only, and time-of-day values, with private storage and validated clock constructors. Numeric values retain scaled integers or their original float width and carry explicit quantity, amount, and ratio units.

## Verification

No tests, collection, build, check, clippy, doctest, network validation, or Cargo command was run, as required by the user constraint that T01–T30 implementation code must be complete first. The test source was authored but remains unexecuted; no compilation or test success is claimed.

## Known uncertainty

The source contract identifies some overlapping bond prefixes as ambiguous but does not enumerate a complete approved Rust inference table. The implementation rejects the documented ambiguous bare prefixes and permits explicit markets; additional inference cases should be added only with evidence. The public serde wire shape for non-finite numbers, scaled values, and validated identities is still marked unresolved by the architecture decision register (E06); the current serde derives are provisional and have not been compiled or round-trip validated.

## Fix Round 1

### Changes

- Removed `Deserialize` from `SecurityId`, `TradingDate`, and `EventTime`; their serialized representation remains provisional, while deserialization is deferred until E06 defines a stable schema. `EventTime`'s private internal enum also no longer derives deserialization.
- Added `EventTime::source_kind()`, `date()`, and `clock()` read-only inspection APIs, with a non-exhaustive `EventTimeSource` distinguishing date-only, Shanghai, source-local, and time-of-day values.
- Added `PriceSource` and `RoundingMode`, `Price::source()`, explicit `to_scaled_integer`, and strict `to_scaled_integer_exact`. The exact conversion rejects float sources, overflow, and discarded fractional precision; rounded conversion requires a caller-selected rounding mode and reports range/non-finite failures.
- Expanded authored config tests to cover a live loopback listener remaining unaccepted across successful build/close and failed validation, plus EventTime getters, price conversion behavior, and `sz127001` acceptance. Bare `127001` remains rejected in line with the frozen ruling in `progress.md`.

### Verification and limits

No tests, test collection, build, check, clippy, doctest, network validation, or Cargo command was run. The loopback tests are source-only and unexecuted. They can detect an accidental connection to the configured numeric socket; numeric `SocketAddr` inputs avoid DNS by construction. They do not directly instrument global thread creation or filesystem reads. Those parts of the no-startup contract are supported by source structure: the client owner contains only an atomic close flag, builder validation is in-memory, and neither owns nor calls a runtime, resolver, or filesystem API. No compile or runtime success is claimed.

### Remaining uncertainty

E06 still leaves the serialized schema and non-finite numeric policy unresolved. Serialization derives are retained where currently present, but validated identity/time types intentionally have no deserialization implementation until that policy is approved. Rounded conversion from float sources is explicitly lossy by nature and is only offered with a named rounding mode; strict exact conversion requires scaled-integer provenance.

## Fix Round 2

`Price::to_scaled_integer` now dispatches scaled-integer values to checked integer arithmetic, preserving values above f64's exact-integer range. Scale increases use checked multiplication; scale reductions use integer quotient/remainder rounding for toward-zero, floor, ceiling, half-away-from-zero, and half-to-even, including negative ties and extreme scale reductions. Wire `f32`/`f64` values retain the explicit-rounding floating-point path. Added source-only cases for `9_007_199_254_740_993` at unchanged scale and positive/negative halfway boundaries across the rounding modes. No tests, collection, builds, checks, clippy, doctests, Cargo commands, network validation, or equivalent verification ran; compile and runtime success are not claimed.
