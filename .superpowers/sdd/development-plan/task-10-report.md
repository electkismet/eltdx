# T10 — metadata, units and clock implementation report

## Status and review base

Stage A checkpoint only. Review base: `2ac38707f2f48b1a3acd248cf9c5de91f18e3429`, filtered to the files listed below. Stage B was authorized while Stage A was being completed; owner/operation/HQ wiring follows this checkpoint and is not claimed by it.

## Authority and implemented sources

Read `task-10-brief.md`, development-plan T10, architecture §7, public-api §§7–8, execution-and-compatibility §4, HQ API §3, and normalization/units/dates policy fixtures. Read the concrete T08 `OperationSponsor` and T09 `PricePrecisionSource`/decode boundary.

Stage A adds immutable versioned metadata snapshots and strict HQ candidate validation, source namespaces and market-qualified identities, terminal-evidence publication, explicit refresh, last-known-good preservation and injected freshness clock. It adds a same-key flight registry with independently checked waiters, owner-free sponsor data, per-sponsor/per-request cached authorizers, unique reservation refund before pre-start reassignment, source-work bounds, stop signals and owned worker join handles. Loader callbacks are crate-private transitional seams; Stage B must connect only the cooperative HQ runtime loader.

`Diagnostic::new(source, stage, status, error_kind, message)` and the five readonly getters exactly match the frozen T21 constructor. `SourceStamp`, `ResultMeta`, `RawData`, and `ValueProvenance` are shared definitions under types. Existing Price/Quantity/Amount/Ratio gain explicit provenance/source representation and checked field-specific conversions. Integer values retain integer scale; source f32/f64 bits remain queryable; JSON serialization explicitly rejects nonfinite floating business values. Clock injection uses UTC epoch +08:00 and actual Gregorian rollovers while retaining local/unknown offset identity.

Authored source cases cover terminal evidence, duplicate/wrong-market identities, half-load failure, immutable old snapshots, conflicting stores, expiry, the original waiter cancelling with a surviving second waiter, pre-start sponsor reassignment, exact large integers/i64::MIN, f32 bits, unknown units, explicit units, Shanghai midnight and leap/month/year rollover. These have not been executed.

## Ownership and timing handoff

Rust source cap is `ClientConfig::request_timeout` from flight creation (default 8 seconds); it is not a new deadline for the waiting operation. A longer OperationOptions cannot extend an already-created flight. The cap provenance is available from FlightControl and timeout error context. Cleanup has one configured budget, to be clipped by the Stage B owner shutdown deadline. Last eligible waiter removal signals source stop. Workers weak-reference store state and do not retain SessionOwner/OperationContext. Existing runtime keeps responsibility for actual retries; the proxy only selects/refunds/reserves/starts sponsorship for that same attempt.

Python legacy must use the explicit `legacy_per_request` policy/compatibility adapter; it must retain original startup, request and prefetch timing, legacy request selection and recovery policy. T12 must not route it through the Rust source total cap, extra32 budget or strict identity inference.

## Stage A files

- `crates/eltdx/src/metadata/{mod,precision,snapshot,store}.rs`
- `crates/eltdx/src/types/{mod,numeric,provenance,time}.rs`
- `crates/eltdx/tests/{metadata,units}.rs`
- this report

## Self-review and remaining integration

Stage A intentionally does not register metadata in the crate root or change owner, operation, runtime, Python bridge, config, client or HQ executor. Full task completion needs Stage B owner lifecycle integration, fixed cooperative HQ source loader, snapshot install/freeze and the no-irrelevant-command-load cases. Shared root and owner paths must preserve the approved T07/T08 state/error gates. A cooperative loader is necessary: an arbitrary blocking user callback cannot be forcibly retired by safe Rust; none will be exposed publicly. No compilation or runtime success is claimed.

## No-execution statement

No tests, test collection, build, check, clippy, doctest, network probe, compilation, AST validation, formatting or diff checks were run. Work consisted of reading source/specification, authoring source, and permitted Git status/log/staging/commit operations with hooks disabled. The final deferred command is `cargo test -p eltdx --test metadata --test units --locked`, only after all T01–T30 implementation code is complete and the controller opens validation.
