# T10 — metadata, units and clock implementation report

## Status and review base

Stages A and B implementation source is complete and ready for the controller's static task review. No compilation, runtime success or executed acceptance is claimed. Review base remains `2ac38707f2f48b1a3acd248cf9c5de91f18e3429`, restricted to the T10-owned paths below. Stage A commits are `5222e60` and `193daa4`; Stage B is the commit containing this report. Other workers' commits on the shared branch are outside this task's review scope.

Read `task-10-brief.md`, development-plan T10, architecture §7, public-api §§7–8, execution-and-compatibility §4, HQ API §3, and normalization/units/dates fixtures. Integration uses approved T08 OperationSponsor/attempt reservations and T09 context-aware decode_reply. It does not change runtime or protocol source.

## Implemented behavior

- Each SessionOwner owns one isolated MetadataStore and injected ClockHandle. Client/service clones share that owner. Cache keys include protocol family, explicit source/catalog namespace and market; records use strict SecurityId. Complete immutable market snapshots are atomically published with checked monotonic versions, fetched/expiry times and provenance. Combined operation captures preserve all relevant markets under one immutable version.
- Production `MetadataStore::load_hq_market` calls the fixed cooperative HQ security-list loader. It uses context-aware parsing with a no-precision source, never the legacy global decimals table. It requests the next offset after every nonempty page, including short pages, and completes only on an actual empty terminal response. Invalid identities/precision, duplicates, wrong market, excess page size, malformed responses, cancellation and partial failures leave the previous good snapshot intact.
- Same-key callers share a flight but check their own cancellation, owner close and deadline. Original waiter cancellation does not stop a surviving waiter. The proxy caches logical-page authorization per sponsor, refunds an unstarted reservation before reassignment, delegates start_before to the actual authorizer, and preserves legacy endpoint-continuation accounting. Runtime remains the only transport retry loop.
- Production workers capture only Engine, immutable market/timing data and FlightControl. Flight sponsorship has a weak SessionOwner via OperationBudget; publication briefly upgrades a weak StoreInner. There is no worker -> strong SessionOwner or worker -> strong MetadataStore cycle. Generic loading callbacks are private within store.rs; cfg(test) helpers are the only callback seam beyond the fixed production loader.
- Admission and worker registration serialize with shutdown. Active flights and retained workers are bounded at 16; market records/pages and waiter identities are bounded by the selected policy. Rust flights retain aggregate catalog bounds; the legacy metadata policy has no added aggregate record/page ceiling. Stop freezes admission and sets a cleanup deadline. PendingRawExecution is polled cooperatively, then cancelled/confirmed under one deadline clipped by owner shutdown; failure quarantines and reports CancelUnconfirmed with root and cleanup failure. A departing waiter with survivors returns independently; the last waiter waits for bounded source retirement. Final-waiter stop and snapshot publication share a terminal mutex, so only one terminal decision wins.
- SessionOwner registers MetadataStore as OwnerTaskGroup. Explicit close signals tasks before runtime close and confirms joins within the same deadline, then joins its idle cleanup controller after releasing every owner-state guard. Final-owner Drop only seals admission, cancels operations, transfers Engine/task handles and signals an independent cleanup controller; that controller performs bounded retirement without capturing SessionOwner. Engine/task registration and controller startup serialize under owner state, while legacy Engine attachment remains lazy so construction starts no controller thread.
- OperationContext installs an immutable snapshot before first decode; begin_decode marks it frozen and subsequent install fails. HQ request inference loads only quote/refresh and today/historical/recent intraday precision requirements. Category quotes capture all possible HQ markets before parsing. Heartbeat, list/count, klines, trades, auxiliary/sparkline and other commands do not trigger precision loads. Trades' fixed milli-yuan rule is independent of quote decimals.
- Existing numeric names now preserve source value/unit, integer scale, float bit patterns and provenance. Quantity lot/share conversions require an explicit lot size; amount ten-thousand-CNY/CNY and percentage-point/fraction conversions are checked. Unknown unit conversions fail. Float conversion and serde output reject nonfinite business values; diagnostic raw bits remain accessible. Existing exact integer/i64::MIN behavior is retained.
- Clock uses UTC epoch seconds plus +08:00, never host local time. ClientBuilder::clock injects it. SessionOwner::current_shanghai_date and metadata freshness use that clock; Gregorian arithmetic handles day/month/year/leap rollover and preserves naive source-local timestamps.
- Shared Diagnostic constructor/getters match T21 exactly. SourceStamp has only command, endpoint, session_id, metadata_version and trading_date. RawData/ResultMeta and ValueProvenance remain shared DTO support; downstream field projection remains T11+.

## T11 / T12 integration signatures

All following entry points are crate-private unless noted:

```rust
HqExecutor::prepare_metadata(
    &self, engine: &Engine, context: &OperationContext, config: &ClientConfig,
    namespace: &str, requests: &[CommandRequest],
) -> Result<Arc<MetadataSnapshot>>

HqExecutor::execute_decoded(
    &self, engine: &Engine, context: &OperationContext, config: &ClientConfig,
    namespace: &str, request: CommandRequest, purpose: ChildPurpose,
    recovery: Option<RecoveryLimit>,
) -> Result<DecodedReply>

HqExecutor::decode_received(&self, context: &OperationContext, raw: RawReply)
    -> Result<DecodedReply>

HqExecutor::prepare_legacy_metadata(
    &self, engine: &Engine, context: &OperationContext, namespace: &str,
    markets: &[HqMarket], request_timeout: Duration, cleanup_timeout: Duration,
) -> Result<Arc<MetadataSnapshot>>

MetadataStore::load_hq_market(
    &self, engine: Engine, namespace: &str, market: HqMarket,
    sponsor: OperationSponsor, policy: FlightPolicy, request_timeout: Duration,
    ttl: Duration, refresh: bool,
) -> Result<Arc<MetadataSnapshot>>

MetadataStore::capture(&self, namespace: &str, snapshots: &[Arc<MetadataSnapshot>])
    -> Result<Arc<MetadataSnapshot>>

OperationContext::metadata() -> Result<Option<Arc<MetadataSnapshot>>>
OperationContext::install_metadata(Arc<MetadataSnapshot>) -> Result<()>
OperationContext::begin_decode() -> Result<Arc<MetadataSnapshot>>
SessionOwner::metadata() -> &Arc<MetadataStore>
SessionOwner::current_shanghai_date() -> Result<TradingDate>
SessionOwner::register_task_group(Arc<dyn OwnerTaskGroup>) -> Result<()>
```

T11 must pass all batch/chunk requests together to prepare_metadata before the first response decode, use an actual source/catalog namespace consistently with the Engine, and route every chunk/page/fallback through decode_received. A missing identity/market or T09 `ProtocolError::InvalidData { context: "price precision", .. }` becomes MetadataUnavailable; other parse errors become Protocol with original error source and operation annotations. DecodedReply still contains the actual RawReply identity/endpoint/payload information needed for provenance. Namespace mismatch is InvalidArgument. Existing snapshot cannot be extended after decode starts.

T12 must call the legacy seam using exactly its historical prefetch scope/order and timing. `FlightPolicy::legacy_per_request` omits the Rust source-total cap and aggregate catalog cap, and uses `SubmissionOptions::legacy_after_startup`. Do not route Python through Rust request inference or add extra32/whole-operation 8-second limits. The binding-layer Python source remains downstream; T10 only keeps the crate-internal compatibility shim's Engine registration lazy. Bridge calls exposing this crate-private seam belong to T12. The default HQ helper cache TTL is 24 hours; explicit lower-level load accepts a TTL and refresh flag. Cache expiry does not invalidate a snapshot already captured by an operation.

## Source cases authored, not executed

Existing Stage A sources cover terminal evidence, duplicate/wrong-market identity, half-load failure, old Arc freeze, expiry, independent owner precision, original-waiter cancellation with survivor, sponsor refund, numeric exactness/bits/unknown units, and Shanghai/Gregorian boundaries.

Stage B adds actual loopback-loader source covering two nonempty short pages followed by a terminal empty page and a malformed mid-refresh page retaining the good version; multi-market operation capture and refusal to replace after first decode; no metadata requests for security count; strict single-market request selection; explicit owner stop/join; and last-owner Drop task cleanup. Round-one review fixes add source cases for both stop/publication terminal orders, >65,535 records under legacy policy while Rust remains bounded, preserved `start_before` timeout and no sponsor reassignment after its shared deadline, lazy registration racing close, explicit controller join, and Drop returning while independent cleanup remains blocked on another thread. These cases are authored only; none were executed.

## Owned files

- `crates/eltdx/src/metadata/{mod,loader,precision,snapshot,store}.rs`
- `crates/eltdx/src/types/{mod,numeric,provenance,time}.rs`
- `crates/eltdx/src/{lib,client,config,session,operation,__python_compat}.rs` (the compatibility shim change only preserves lazy owner registration)
- `crates/eltdx/src/hq/executor.rs`
- `crates/eltdx/tests/{metadata,units}.rs`
- this report

No edits to protocol/runtime, binding-layer Python source, future HQ services, EX/offline domain source, or controller progress/ledger.

## Self-review and remaining validation

Source review checked lifecycle ownership, lock order, deadline propagation, atomic publication, single-market and aggregate lookup, and T08 reservation interfaces. Round-one review fixes moved controller startup inside the owner-locked registration transaction, moved explicit controller join after releasing SessionState, and transferred Drop cleanup to an independent controller without a strong owner capture. They also serialize last-waiter stop with publication, select policy-specific catalog bounds, and preserve timeout outcomes through sponsor reservation. Earlier source fixes include worker-registration/close ordering, completion panic scope, late-waiter pruning, cleanup deadline extension, final-waiter unconfirmed cleanup handling, namespace/market capture checks, and the types re-export needed by unit sources. This is source reasoning, not validation evidence. Runtime scheduling, compilation, feature combinations, loopback behavior, cancellation races and serde behavior remain to be verified when the controller opens validation. Downstream HQ/Python public DTO field projection and service selection wiring remain their planned tasks.

## No-execution statement

No tests, test collection, build, check, clippy, doctest, network probe, compilation, AST validation, formatting or diff checks were run. Work used source/specification reads, source authoring and permitted Git status/log/staging/commit operations with hooks disabled. Deferred acceptance command is `cargo test -p eltdx --test metadata --test units --locked`, alongside controller-selected unit/lifecycle/feature coverage, only after all T01–T30 implementation code is complete and validation is explicitly opened.
