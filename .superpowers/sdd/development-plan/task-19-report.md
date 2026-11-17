# T19 source progress — F10 foundation

## First checkpoint, 2026-11-17

T18 source gate was committed as `515a13c`. T19 is now in progress. All test,
build, check, network, dependency resolution and execution validation remain
forbidden until T01–T30 implementation is complete. No such execution ran.

Authored `crates/eltdx/src/f10/{mod,config,json,request,response}.rs`, feature
registration, optional serde_json/url declarations and `tests/f10_values.rs`.

- Ordered duplicate raw JSON keys, exact numeric lexemes with checked scalar
  access, BOM/UTF-8, compact Unicode encoding, byte/depth/node limits and
  caller-supplied operation boundaries. Strings alone use serde_json's decoder.
- Private config fields and default three-route selection, no ambient proxy,
  validated header/URL/proxy types, TLS/resolver policies and source limits.
- Mutually exclusive Body/Params requests; typed extra keys reject collisions
  before mutation. Params encoding avoids cloning the complete request tree.
- Ordered multi-table/cell response, duplicate column access and Python suffix
  collision projection, absent/empty/null distinctions, business error status.
  Raw Omit does not clone a complete response tree for retention.
- Authored public and private source cases for these contracts, unexecuted.

## Next work — not implemented yet

1. F10ClientBuilder/F10Client with independent lazy SessionOwner and the common
   OperationContext; live configuration and diagnostic snapshots, close/no reopen.
2. Narrow domain-neutral RequestPermit authorization consumption. Start the
   unique reserved physical attempt at connect/send, sharing existing counters.
3. Choose bounded DNS plus production HTTP/TLS transport after source review;
   implementation must provide per-phase cancellation and confirmed cleanup.
   Hyper/rustls local cached sources were read only; no dependency was selected.
4. Implement streaming header/encoded/decoded caps, gzip/chunks, explicit proxy
   CONNECT, root/hostname verification, IPv4 preference/IPv6 fallback and exact
   source retention. No implicit environment proxy/cookie/retry.
5. All required `f10_http.rs` local HTTP/HTTPS/proxy/fault/close fixture source,
   dependency decision and integration handoff to T20/T23. Do not run them yet.

Current totals: **19 source complete, 4 partial (T19/T22/T24/T25), 7 unstarted
(T20/T23/T26–T30)**. Goal active without token cap. No subagents or publication.
Neither F10 nor the whole SDK is claimed usable/compilable/verified.

## Second checkpoint — owner/configuration and shared authorization

First value-layer checkpoint committed as `8a339c0`. Added `f10/client.rs`:
public F10ClientBuilder/F10Client/F10Diagnostics, pure build validation, all
specified with_* settings, ordered header defaults/overrides and an independent
SessionOwner. Clone shares that owner; close uses the existing confirmed owner
shutdown and cannot reopen it. No network methods have been wired yet.

Added `RequestPermit::into_transport_authorization` as a narrow crate-private
move of the existing Arc authorization. A transport must still reserve/start
physical attempts on that authorization, without constructing an HQ command.
Added unexecuted budget/refund/closed-owner source cases and lazy F10 build,
header order, duplicate header and close source cases.

Continue HTTP/DNS/TLS, actual public connect/call/params, bounded streaming and
all fault fixture sources. Counts19 complete/4 partial/7 unstarted unchanged.
No test/build/check/AST/dependency/format/network execution or subagents.

## Third checkpoint — HTTP/DNS/TLS source and public fixture matrix

Second checkpoint committed as `545dd8b`. Added control.rs/resolver.rs/io.rs/
http.rs/worker.rs and wired public connect/call/params plus the T20
call_in_context seam. Provisional dependencies and limitations are recorded in
dependency-decisions.md, not claimed as verified choices.

- Registered worker/runtime ownership, max4 active jobs, cancellable queue,
  original sponsor/deadline and unique RequestPermit connect authorization.
  Post has no application retry. Address and redirect attempts share counters.
- Async Hickory DNS, numeric IPv4/IPv6 fast path, v4 preference plus v6 fallback,
  explicit proxy CONNECT, rustls hostname/custom/system trust and HTTP/1.1 ALPN.
- Default no environment proxy, cookies or cache. Strict POST-preserving
  redirect handling, independent routes, UTF8 body and no injected QSID.
- Header/encoded/decoded/frame-overhead limits, streamed gzip/deflate and raw
  encoded payload versus actual received HTTP plaintext. Header order adapter
  preserves separated duplicate caller names without changing Hyper body framing.
- Worker done publishes after runtime destruction, join only after finished,
  failed cleanup preserves original cause, and successful pages do not start
  a terminal cleanup deadline on the shared multi-page parent.
- Authored public f10_http.rs source for normal/route/status/JSON/BOM/short
  headers/chunks/compression/byte caps/cancel/close/DNS/queue/TLS/SNI/hostname/
  untrusted root/CONNECT/IPv6/environment isolation/idempotent connect/header
  ordering; no fixture server or test has been started.

T19 remains partial pending focused source review of transport boundary cases,
complete source evidence and remaining fixture matrix. OS trust/config reads
are local synchronous calls inside a registered worker; an unresponsive call
must yield unconfirmed cleanup, never a false successful cancellation. Final
dependency/API/MSRV/platform evidence is absent by user instruction.
Next finish this source review, then T20 all157 entry/query implementations;
do not treat generic call as full F10. Counts19/4/7 unchanged, unlimited active.

Same-stage review correction: worker now retains a shared concurrency lease
through confirmed thread exit, including CancelUnconfirmed paths. Completed
registered tasks are reaped outside the concurrency lock, so leaked capacity
cannot strand later requests. SessionOwner's existing reap method is now
crate-visible; no new owner registry was added. Header override order is emitted
through bounded plaintext header adaptation, and related source cases authored.

## Fourth checkpoint — source boundary follow-up

Third transport checkpoint committed as `6196df2`. Source review found and
addressed the following before continuing the endpoint layer:

- Explicit zlib StreamEnd/Adler32 completion with trailing-byte rejection,
  using the existing flate2 primitive and bounded output blocks. Authored
  success/truncation/trailing cases; gzip concatenated-member/footer cases.
- Concurrent connect callers share a one-slot probe gate, with independent
  waiter cancellation. Probe and worker leases remain owned by the registered
  task until confirmed retirement; no second probe can bypass unconfirmed
  cleanup. Authored overlapping TLS connect and cancelled-waiter cases.
- Params encoding uses one Encoder for wrapper plus all elements, including
  shared node/depth counts; it no longer resets complexity per parameter.
  ParamsQuery stores its CallQuery internally to avoid a full preflight clone.
- Retained request and response JSON use fallible, cancellation-aware copying;
  large strings are copied in bounded blocks. Authored preservation and
  mid-copy cancellation cases. Omit still avoids retaining complete table JSON.
- Response::attributes retains ordered non-table top-level service metadata,
  including HitCache/hitCache and duplicate unknown keys. Earlier duplicate
  ResultSets stay in retained original payload only, not another tree under Omit.
- Authored same-origin 307 POST continuation and cross-origin/method-changing/
  credential/fragment/default-limit redirect rejection cases.

All added source cases remain unexecuted. No dependency resolution, tests,
collection, compilation, formatting, check, clippy, network, AST or equivalent
validation ran. This is a continuation checkpoint, not a passing release or
T19 runtime/dependency gate. Counts19 complete source/4 partial/7 unstarted
remain unchanged. Next finish source evidence/handoff, then T20 all157 methods,
schemas, field registry, news and historical flow. Active goal has no token cap.

## Fifth checkpoint — final transport source handoff (stage113)

Base58f7664. T20/T23 and subsequent domain migrations already have their own
source checkpoints; the older next-task directions above are historical.
T19 implementation, authored fixtures and source handoff are now source-complete.
Totals are29 source-complete /1 partial(T30) /0 unstarted. No dependency/runtime
or acceptance checkbox is promoted. T30 still needs per-service compiled-usage
example sources and final source review before the global execution gate.

Read config/client, worker/control/resolver/http/io and their public fixtures.
The transport index maps exact existing function names to HTTP/TLS/CONNECT,
DNS/cancel/close, header/body/compression caps, ordering, redirect and owner gate
boundaries. Runtime destruction precedes done, actual thread completion precedes
join/lease release, and uncertain cleanup retains registered ownership. Native
root/config reads remain synchronous OS work with explicit CancelUnconfirmed
limitations; no hard-preemption guarantee is invented.

Source review added rejection of repeated Content-Encoding fields rather than
silently using the first. A public fixture source now authors repeated fields,
comma-separated coding chains and unknown br coding, requiring Protocol and
closing/joining before result assertions. Existing zlib/gzip end checks and
single-driver abort/await retirement remain the shared implementations.

New docs/rust-sdk/f10-transport-index.md and dependency-decisions.md record
ownership, source cases, T20/T23 boundaries and precise unexecuted commands.
The dependency pins, availability, lock/API/MSRV/license closure and three OS
trust/DNS/IPv6 behavior remain unverified. Production samples are separate.
No tests/collection/build/check/clippy/doctest, dependency/network, AST/syntax,
format/doc/diff checks or equivalent execution ran. Only source reads/writes,
ordinary review diff and hook-disabled recovery commits. No agents/publication.
