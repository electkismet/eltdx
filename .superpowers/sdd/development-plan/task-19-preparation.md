# T19 preparation: bounded F10 HTTP/TLS transport

Date: 2026-11-16. Preparation only; **no implementation GO**. This report is the only file authored by this preparation task. No source/Cargo/root-module edits, Git mutations, dependency resolution, network access, tests, collection, builds, checks, clippy, doctests, compilation, AST inspection, formatting, or diff checks were performed. Source and already-local registry files were read as text. No HTTP/TLS/dependency behavior is claimed verified.

## Recommendation and decision boundary

Use **low-level Hyper HTTP/1 client connections on owned Tokio I/O, with tokio-rustls/rustls**, a bounded asynchronous DNS adapter, and the existing flate2 decoder. Start with one in-flight connection per F10 owner, a bounded pending queue, no idle connection pool, and no HTTP application retry. This is smaller to reason about than adding a higher-level client and then overriding its connector, retries, header parser, decompressor, and lifecycle. HTTP parsing/chunk framing stay in Hyper; certificate/hostname verification stay in rustls. Do not write a second HTTP or TLS stack.

The concrete network architecture below can enforce caps and retire asynchronous network work. Two initialization contracts remain **unresolved implementation boundary decisions**, not details that can be silently papered over:

1. Default `SystemRoots` discovery uses synchronous OS/file APIs in candidate root loaders; no cancel/cleanup bound has been established for those APIs.
2. A genuinely asynchronous DNS lookup still needs a nameserver configuration. Candidate default system-configuration discovery is synchronous; the resolver's underlying task ownership/cancellation also needs source review once its code is available.

Consequently this preparation does **not** assert that the whole default configuration already satisfies strict end-to-end cancellation. Controller must rule these boundaries when T19 starts. Bundled Mozilla roots, hard-coded public DNS, blocking DNS hidden in Tokio, or a timeout around an unjoined blocking worker are not substitutes for the specified defaults. A runtime `Unsupported` placeholder would not complete T19.

## Requirements retained

- Independent lazy F10 owner; build/config/getters perform no DNS, root loading, TLS, proxy discovery, filesystem work, or background-thread creation. Clone shares this owner; close permanently seals it.
- Defaults remain static `http://static.tdx.com.cn:7615/TQLEX`, hot `http://hot.icfqs.com:7615/TQLEX`, page2 `http://page2.tdx.com.cn:7615/TQLEX`; timeout 8 s; IPv4 preference true; decoded/encoded caps each 16 MiB; header cap 64 KiB; redirects 0; proxy None; SystemRoots; Bounded resolver.
- URL hostname, numeric connection address, HTTP authority, proxy authority, and TLS server name remain separate values. Only `Entry=<percent-encoded entry>` is the query. Requests use compact UTF-8 JSON, including Chinese characters, without automatic QSID injection.
- One `OperationContext` spans admission, lazy startup/configuration, queue wait, resolution, every physical connect, proxy/TLS, send, headers/body, decompression, JSON parsing, pagination/composition, and delivery. T20 calls a context-sharing internal method; it must not call a public F10 method that starts another context.
- HTTP status, connection/TLS, malformed HTTP/UTF-8/JSON, and limits have separate structured diagnostics. Nonzero server business code remains a normal `Response`.

## What the local source actually establishes

Registry root inspected: `C:/Users/ax/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`.

- `hyper-1.12.0/src/client/conn/http1.rs` exposes **`Builder::max_header_size(usize)`**. Its documentation says status line is included and the value also limits chunked trailers. This is distinct from `max_headers` (count) and `max_buf_size` (connection buffer). `max_buf_size` panics below 8192; do not pass an arbitrary small public header cap to it.
- Hyper's `proto/h1/role.rs` checks the completed head's parsed byte length; `proto/h1/io.rs` also stops an incomplete head at the configured limit. The latter reads/reserves a connection buffer before the next parse. Therefore `max_header_size` is the semantic byte guard, not a promise that every internal allocation is at most H bytes. Use fixed bounded I/O scratch and an independently bounded head gate if exact pre-parser allocation/read boundaries are required.
- Hyper `proto/h1/decode.rs` applies the same configured size to trailers. Its trailer check uses `len >= limit`, so exact inclusive trailer-boundary acceptance needs later evidence; do not advertise an untested exact-boundary equivalence with final-head parsing.
- `http-1.5.0/src/header/map.rs` explicitly says iteration order is arbitrary. Hyper's original-order extension is private, and its `preserve_header_order` builder method is gated by `ffi`. It is not a stable public Rust solution for ordered request serialization.
- `reqwest-0.12.28` supports custom DNS, explicit proxies, rustls, and disabling proxies. Its local client source documents default protocol-NACK retries and exposes `retry(...)`; that must be disabled if it is ever selected. No equivalent public exact response-head byte knob was found in the inspected client API. The blocking facade also obscures task retirement. It is not the minimal choice for these requirements.
- `ureq-2.12.1/src/response.rs` hard-codes a 100 KiB response-header limit and a 100-header limit. Ordered header vectors and injectable DNS are useful, but this version does not supply the required arbitrary configurable head cap or the desired asynchronous retirement contract.
- `tokio-1.53.1/src/net/addr.rs` resolves string addresses through `spawn_blocking` and `std::net::ToSocketAddrs`. `lookup_host`/`TcpStream::connect(hostname)` cannot be the Bounded resolver implementation. Only already-numeric socket addresses reach Tokio connect.
- The local reqwest Hickory adapter calls **`hickory_resolver::TokioResolver::builder_tokio()`**, then sets `LookupIpStrategy::Ipv4AndIpv6`, then `build()`. Its documentation identifies synchronous system DNS configuration discovery. Hickory source itself was not present among the inspected cached crate sources; request-drop and background-task shutdown are unestablished.
- `hyper-rustls-0.27.10/src/config.rs` calls **`rustls_native_certs::load_native_certs()`** synchronously. Its `with_native_roots` helper logs partial load errors and skips certificates that fail parsing when usable roots remain. That helper's permissive error handling does not meet the brief's prohibition on silently accepting root-load failure. Use a direct loader adapter that converts load errors/empty roots into `Connection`, with an explicit ruling for partial stores.

## Minimal transport ownership

`F10Client` owns `Arc<SessionOwner>` independently of HQ, plus a lazy `HttpDriver` control handle. The initial driver has a bounded command queue and owns one Tokio runtime thread, its active request future, connection future, resolver resources, and termination receipt. No automatic cookies, cache, redirects, environment proxy, periodic task, or reconnect loop is added.

The thread starts only inside an admitted operation. Queue waits and start publication use the same context boundary/deadline. Creating the thread and registering its shutdown participant must be one owner-controlled startup transition: if close wins, the unpublished candidate is stopped and joined by its startup owner, never abandoned. The worker holds a weak/control reference rather than a permanent strong owner cycle. Waiting callers/operation guards keep the owner alive.

Poll Hyper's connection driver and request/body futures within the driver's structured task scope. Avoid fire-and-forget `tokio::spawn`; any tasks created by the DNS dependency must be registered and awaited too. End a request by dropping/closing its socket and awaiting its driver termination. No idle pool means no stale pooled-connection retry and no hidden request replay. It costs another TCP/TLS handshake per request; pooling can be considered only with a demonstrated equivalent close contract.

Cancellation races the I/O future against the context's absolute deadline and a bounded context-boundary poll, using the existing runtime signal-poll cadence rather than another deadline calculation. A future drop alone is not a completion receipt: release is acknowledged only after all request-owned connection/DNS tasks and sockets retire. Owner close closes admission first, wakes the driver, stops queued/active work, and uses the one owner shutdown deadline to confirm resource shutdown and thread join. On failed confirmation retain the registration and failure lineage; subsequent cleanup cannot reopen the owner or erase its root failure.

`connect(&OperationOptions)` should initialize the owned driver and perform bounded readiness connection/handshake for the configured base route, then retire that temporary socket. Later idempotent connect can observe the initialized state. It must not POST an invented Entry. Whether readiness covers all configured route origins should be frozen by controller; a base-route readiness check must not claim hot/page2 reachability.

## Request headers: preserve configuration semantics and ordered serialization

Keep the builder's header input in an ordered vector with a locally checked name/value type. Apply case-insensitive overrides to default `Content-Type`/`Accept` without shuffling unrelated user entries; retain intentional duplicate ordinary fields. Reject CR/LF and ambiguous HTTP framing locally; derive the POST Content-Length from the actual compact UTF-8 payload. Do not rely on HeaderMap insertion behavior for order.

If wire order of duplicate configured request headers must be preserved, add one **bounded outbound head serializer/AsyncWrite adapter** around the Hyper connection. It consumes only Hyper's initial request head, validates its semantic framing against the already-built request, and emits the precomputed ordered head. Body bytes then pass through unchanged. Limit its storage to the checked outgoing head plus a fixed scratch buffer; handle partial writes, flushes, a head/body coalesced write, and cancellation explicitly. Use a known-length body and disable vectored writes initially to keep the adapter small. This is a request-head adapter, not an alternative response/chunk parser, and does not use Hyper private FFI extensions.

Builder order/override semantics are distinct from **raw response wire order**. Parsed response headers need not gain an invented ordering promise. If `RetainWire` is requested, capture the actual accepted HTTP plaintext bytes before Hyper consumes them; reconstructing them from HeaderMap/JSON would not be faithful. Do not label decrypted HTTP capture as TLS ciphertext. Omit does not allocate this capture.

## Exact limits and decoding path

1. Validate all configured caps and checked arithmetic before allocating. Do not preallocate from peer Content-Length, chunk length, gzip ISIZE, or JSON counts. A declared Content-Length exceeding encoded cap can be rejected before body collection, but smaller/absent claims do not bypass streaming accounting.
2. Configure Hyper `max_header_size(H)` and a separately bounded read buffer. Map its TooLarge/head/trailer-limit errors to `LimitExceeded`, keeping malformed/truncated HTTP as `Protocol`. Use a small pre-parser head gate when stricter input allocation boundaries than Hyper's fixed buffer are required. It collects at most H plus a one-byte overflow probe, retains only a fixed suffix across reads, and releases accepted bytes to Hyper after the delimiter. Handle informational and CONNECT heads rather than only the final 200 response.
3. Hyper handles Content-Length/chunked/EOF framing. Count DATA bytes **before content decompression** with checked u64 additions; reject the chunk/frame that would exceed encoded cap before copying it into the response working buffer. Fixed I/O/read-ahead scratch is separately bounded. Do not allocate a body-sized chunk from a declared chunk length.
4. Initial implementation can collect at most the encoded cap, then decode it through a budget-aware in-memory reader that exposes small slices and calls `check_boundary(Decode)` between slices. Reuse flate2 for gzip; use fixed output chunks and check decoded cap before append. This still enforces actual encoded and decoded limits, including a gzip expansion bomb, without another decompression crate. Gzip member/checksum/truncation/trailing-data behavior must be explicitly frozen and later exercised; do not rely on output length alone. Unsupported content encodings fail explicitly rather than delivering compressed text as JSON.
5. Strip only a leading UTF-8 BOM; reject invalid UTF-8, malformed JSON, trailing non-whitespace, and a non-object top level. Preserve ordered object entries/duplicate raw keys and numeric lexemes in F10-owned values. `serde_json::Value`, including its `preserve_order` feature, cannot by itself preserve duplicate object keys. Use a bounded context-aware visitor/raw-token path rather than first constructing an ordinary Value and cloning it.
6. Omit permits temporary bounded decode buffers but does not construct an additional complete retained JSON tree, wire capture, or row hex. RetainPayload shares actual decoded source storage; RetainWire adds actual HTTP source storage. Account retained bytes before allocation/copy and share Bytes/ranges where possible. Chunk extensions/framing can consume bytes without adding DATA, so the encoded DATA cap alone cannot bound wire retention: freeze an explicit checked raw-retention ceiling derived from decoded+encoded+header limits and charge chunk framing to it. Exceeding retention budget must fail, not silently truncate a claimed faithful artifact.

The exact scope of H needs a documented choice: the local Hyper setting is a per-head/per-trailer-block limit, not one cumulative counter over CONNECT + arbitrary 1xx heads + final head + trailers. If the contract requires a cumulative cap, extend the small head/trailer observer and shared accounting; do not claim Hyper's setting already provides it. Informational-response processing remains under the same deadline even when each head is individually small.

## DNS, hostname TLS, proxy, and family fallback

- Resolve the connection peer to bounded A/AAAA results through a production asynchronous DNS implementation; retain the URL hostname for HTTP authority and `rustls::pki_types::ServerName`. Feed only numeric addresses to Tokio. Cache, if any, must be bounded and owner-scoped; the minimal version needs none.
- With IPv4 preference true, prefer IPv4 candidates and preserve each family's resolver order. A failed IPv4 connect must reach IPv6 while budget remains. To prevent one black-holed IPv4 candidate consuming the entire operation, use a bounded two-candidate race with a small documented fallback delay (proposed 250 ms, not yet a frozen API default). Each started candidate gets its own shared authorization; losers are closed and awaited before completion. No family-global resolver override is used.
- For an explicit HTTP proxy, resolve/connect the proxy, not the origin directly. HTTP targets use absolute-form request targets; HTTPS targets use authority-form CONNECT followed by origin TLS over the tunnel. Keep origin Host/SNI/certificate verification through all address-family choices. If HTTPS proxy URLs are accepted, perform an outer proxy TLS handshake using the proxy name, then CONNECT and inner origin TLS; never silently treat an HTTPS proxy as plaintext. Unsupported proxy schemes must be rejected by the checked ProxyUrl constructor, not discovered during business execution.
- The minimal proxy needs no environment lookup. HTTP_PROXY/HTTPS_PROXY/ALL_PROXY/NO_PROXY do not participate in routing. Origin DNS performed by a remote proxy is outside local DNS cancellation control; closing the local tunnel stops SDK-owned work, without claiming cancellation of remote infrastructure.
- Configure rustls with explicit per-client provider (`ring`), safe verification, SNI enabled, no early data, and HTTP/1.1 ALPN. Numeric URL hosts use IP SAN verification; do not replace a hostname by its chosen IP for verification. Custom roots arrive as bounded owned certificate bytes, not a file path that build secretly reads. Trust modes should explicitly state whether custom roots replace or augment system roots.

### Exact initialization limitations and feasible options

`rustls_native_certs::load_native_certs()` and Hickory's `TokioResolver::builder_tokio()` system-configuration step have no established interruption/retirement guarantee here. Putting either in `spawn_blocking`, dropping its JoinHandle, wrapping it in `timeout`, or timing out a completion receiver does not prove it stopped. Calling it directly on the sole driver thread can prevent that thread from observing cancel/close. Loading roots at build would violate laziness. Local source also shows the native-roots helper's partial-error behavior is too permissive for silent use.

Feasible paths for controller to assess are: explicit/preloaded numeric nameserver configuration and preloaded trust material, with their provenance exposed; bounded platform adapters (for example Windows `DnsQueryEx`/`DnsCancelQuery` with completion ownership, requiring later platform-source review); or an explicitly owned cancellable helper process for unavoidable OS/configuration discovery with kill/wait confirmation. Preloaded system-root snapshots can preserve system trust provenance, but obtaining the default snapshot lazily is still a separate boundary that needs a solution. Explicit custom roots/DNS are useful supported modes, not a justification to weaken default SystemRoots/Bounded. No platform adapter or helper executable was implemented or validated here.

## Narrow shared interfaces needed when implementation receives GO

Current source references: `crates/eltdx/src/operation.rs`, `policy/retries.rs`, `session.rs`; `crates/eltdx-runtime/src/execution.rs`.

**Context construction.** `OperationContext::rust_public` currently takes HQ `ClientConfig`, although it reads only request timeout, wire attempt limit, extra budget, cleanup timeout. Factor a crate-private immutable `OperationDefaults` carrying those four values and let the existing HQ constructor delegate. F10 supplies its checked timeout and a one-application-send policy; T24 can reuse the same construction seam. Do not synthesize HQ endpoints/configuration or expose a second public OperationOptions.

**Permit consumption.** Add a crate-private consuming method such as `RequestPermit::into_transport(self) -> TransportPermit`. The non-Clone result carries operation/child identity, original deadline policy, and the **same Arc<dyn AttemptAuthorization>**, with a narrow method that returns the existing non-Clone `Box<dyn ReservedAttempt>`. Do not create another authorization/budget or fake a `CommandRequest`. Preserve `into_submission(&CommandRequest)` for HQ unchanged in behavior.

The driver calls authorization `check_boundary`, then `reserve(physical_ordinal, last_failure)`, and moves that exact reservation to the candidate. It calls `ReservedAttempt::start(now)` immediately before the actual connect/send attempt crosses its first-I/O boundary. Drop before start refunds the reservation; started failures remain counted. Connect, CONNECT, origin TLS, and sending on that same socket do not each debit a separate attempt.

Separate one **HTTP application send** from physical address candidate attempts: no automatic POST replay, but IPv4-to-IPv6 reconnect/fallback consumes a new physical ordinal/shared extra authorization. The underlying `ChildAuthorization` already charges `attempt > 1` as extra. Blindly applying HQ's one-wire-attempt retry limit would incorrectly prevent mandatory family fallback. Freeze this small descriptor/semantics with controller; do not turn family fallback into unmetered retries or introduce another retry budget. Once origin request bytes are sent, return the actual failure rather than replaying POST. Candidate admission under zero extra budget may legitimately stop fallback with `LimitExceeded` and the prior failure.

**Driver view.** Expose an owner-free context view with original absolute deadline, boundary check, and cancellation/close observation. Existing `sponsor().deadline()` can provide the same clock; a bounded boundary-poll loop works with the current public token. An async notification can be added later if it preserves gate ordering. The SDK token and owner gate remain authoritative at reservation start; a cached boolean is insufficient. Preserve the existing `claim_cleanup`/`cleanup_remaining` path and cleanup-failure error structure, including CancelUnconfirmed.

**Owner lifecycle.** `SessionOwner` currently registers only HQ Engines and pins; it cannot confirm an arbitrary HTTP runtime thread. Add one crate-private shutdown participant registration, for example `register_shutdown_participant(Arc<dyn ShutdownParticipant>)`, with bounded non-I/O `request_stop()` and `confirm_stopped_until(absolute_owner_deadline)` operations. Registration/publication must be atomic with the open admission gate; invoke stopping/waiting outside the owner mutex. A receipt must mean active sockets/tasks retired and thread join completed, not merely an abort request. Retain unconfirmed participants after a close failure and include them in `cleanup_confirmed`; keep existing failed-close lineage. A startup-specific owner must retain a thread that loses the registration race and confirm it before dropping its operation guard. This is the smallest lifecycle extension for an independently owned HTTP driver; no general-purpose task framework is needed.

These are later coordinated shared changes only. This task did not edit/reopen T08 or its current review gate.

## Provisional dependency inventory

The following exact versions and package metadata were read from already-cached manifests. Package `rust-version` values are declarations, **not an MSRV build result**, and do not establish the transitive graph's compatibility. None is a resolved/approved F10 dependency decision.

| Crate | Local exact version | Declared Rust | Declared license | Proposed use/features |
| --- | --- | --- | --- | --- |
| hyper | 1.12.0 | 1.63 | MIT | default-features=false; client,http1; exact public max_header_size is a reason for this candidate version |
| hyper-util | 0.1.21 | 1.85 | MIT | default-features=false; tokio only for TokioIo; no client-legacy/pool/system-proxy |
| http | 1.5.0 | 1.57.0 | MIT OR Apache-2.0 | checked HTTP names/values/URI; ordered vector remains authoritative |
| http-body-util | 0.1.5 | 1.61 | MIT | default-features=false; known-length body and frame iteration; never unbounded collect |
| rustls | 0.23.46 | 1.71 | Apache-2.0 OR ISC OR MIT | default-features=false; std,ring,tls12; per-config provider |
| tokio-rustls | 0.26.6 | 1.71 | MIT OR Apache-2.0 | default-features=false; ring,tls12; nonblocking handshake/socket I/O |
| rustls-pki-types | 1.16.0 (also 1.15.1 present) | 1.60 for 1.16.0 | MIT OR Apache-2.0 | owned ServerName/certificate bytes if directly needed |
| url | 2.5.8 | 1.63 | MIT OR Apache-2.0 | checked URL parsing and Entry query encoding; transitive IDNA graph unresolved |
| serde_json | 1.0.145 | 1.61 | MIT OR Apache-2.0 | existing workspace pin; internal raw_value/number path; independent of public serde feature |
| flate2 | 1.1.2 | 1.67.0 | MIT OR Apache-2.0 | existing workspace pin; rust_backend; reuse gzip support |
| tokio | 1.53.1 | 1.71 | MIT | existing workspace pin/features; numeric connect only |
| reqwest (alternative) | 0.12.28 | 1.64.0 | MIT OR Apache-2.0 | not recommended for strict header/lifecycle/order requirements above |
| ureq (alternative) | 2.12.1 | 1.71 | MIT OR Apache-2.0 | not recommended; hard-coded head limits/synchronous lifecycle |

`ring-0.17.14` is locally present and declares Rust 1.66.0. It includes native build/assembly sources; its full license file and platform build-tool closure were not reviewed here. Do not infer that choosing ring eliminates native build prerequisites or establishes the complete license closure.

Additional required candidates are deliberately **not assigned invented exact pins**:

- `rustls-native-certs`: local reqwest 0.12.28 requests `0.8.0` (semver range), hyper-rustls 0.27.10 requests `0.8`; a local sparse-index cache entry exists, but no crate source was available in the inspected source cache. Exact selected version/MSRV/license/platform behavior are unverified. Its caller establishes the synchronous API/partial-error issue, not the loader's implementation details.
- `hickory-resolver`: local reqwest requests `0.25` with its async adapter shown above. No resolver/proto source was available in the inspected cache; exact pin, MSRV/license/features, system configuration path, caps, and abort/join contract are unverified. Consider its protocol-level/explicit-configuration path if the high-level resolver hides unowned spawned tasks; do not implement a home-grown DNS parser.
- `httparse-1.10.1` is present, MIT OR Apache-2.0, no rust-version in the inspected manifest. Hyper already uses it. A direct dependency is only justified if the bounded outgoing-head adapter needs parsing; a manual chunk decoder is not proposed.
- `hyper-rustls-0.27.10` was inspected to understand native roots; it is not needed in the proposed low-level tokio-rustls connector. `rustls-pemfile-2.2.0` is present, but accepting DER/custom certificate bytes may avoid an extra direct dependency. Their MSRV/full closure were not inspected for selection.

Workspace MSRV remains 1.89 and workspace unsafe code remains forbidden. No Cargo.toml/Cargo.lock was edited or resolved. Native adapters requiring project-owned unsafe code would need an approved safe wrapper/dependency, not a local lint bypass.

## Handoff and remaining evidence

After controller GO, T19 should first settle the root/configuration discovery boundary, freeze exact candidate/feature choices and the shared permit/owner hooks, then author the public-client fixtures from its brief. Needed evidence includes ordered duplicate request headers, exact/over-limit head/trailers, informational/CONNECT heads, Content-Length/chunked/EOF and gzip bombs, hostname/SNI/custom roots/invalid certificate, proxy/environment isolation, black-holed and failed IPv4 to IPv6, cancellation/confirmed close during every stage, and T20 composition under one context. The source-only fixtures are not written by this preparation task.

T20 should consume `call_with_context(&OperationContext, &CallQuery)` or an equivalent narrow internal method; it owns route registry/typed endpoints and dynamic table projection. T23 must retain Python's URL quirks, custom `_post`, per-wire timeout scope, legacy absence of new body/head caps, and exception mapping. ZIP remains T17's shared decoder; this proposal introduces none.

Outstanding controller decisions: strict default SystemRoots/system DNS configuration discovery; DNS task ownership contract; header-limit scope/exact boundary semantics; physical-candidate charging versus one POST send; readiness origin coverage; bounded ordered request-head adapter; and exact raw-retention accounting. Exact dependency graph/MSRV/license/platform behavior and all HTTP/TLS/cancellation/limit behavior remain **unexecuted and unverified**.
