# Rust SDK dependency decisions

## ZIP and CRC — T17, provisional source selection

The default HQ SDK uses one private decoder, `archive::decode_selected`, for
statistics and financial archives. Cargo source declares exact workspace
`zip = 0.6.6` with default features disabled and only `deflate`, and
`crc32fast = 1.5.0`. Both are unconditional SDK dependencies: HQ resource support
does not require the optional offline or F10 feature. The existing flate2 Rust
backend is reused by the selected ZIP dependency; no independent inflater or
filesystem extraction was added.

This is a **provisional, unverified dependency choice**. The local sparse record
read during Stage A declares zip MSRV 1.59.0 (workspace 1.89), an unyanked
0.6.6 entry, and checksum
`760394e246e4c28189f19d488c058bf16f564016aefac5d32bb1f3b51d5e9261`.
Its dependency ranges admit existing flate2 1.1.2 and crc32fast 1.5.0. Expected
license identifiers are MIT for zip and MIT OR Apache-2.0 for crc32fast; exact
license files, advisory state, transitives, target MSRV and APIs have not yet
been verified. No registry resolution, downloads, lockfile regeneration,
compilation or runtime checks have run. Cargo.lock must be resolved and all
requirements verified at the final T01–T30 validation gate before release
readiness can be claimed. A rejected dependency will be replaced at that gate
without weakening the archive limits or introducing a second decoder.

The structural preflight precedes library directory allocation. It admits a
bounded classic Store/Deflate subset, validates central/local declarations and
descriptors, and rejects duplicate raw names, encryption, ZIP64, spanning,
overlapping records and unlisted local data. Streaming checks actual expanded
counts and independently accumulates CRC for every member, including members
not selected for delivery. An operation-supplied boundary is checked between
bounded work units. Library deflate EOF/trailing-byte behavior, interoperability
with independent archives, CRC, malformed streams and limits all await final
validation; authored source cases are not evidence of passing behavior.

| Consumer | Compressed | Entries | Expanded per member | Expanded total |
| --- | ---: | ---: | ---: | ---: |
| HQ statistics | 32 MiB | 128 | 32 MiB | 64 MiB |
| Rust financial archive | 64 MiB | 64 | 256 MiB | 256 MiB |

The financial total is a new Rust protection approved in the implementation
decision register; it must not become a new cap on the legacy Python selected
DAT path. Stats requires both exact unique `tdxstat.cfg` and `tdxstat2.cfg`.
Downloads use a compressed-limit-plus-one allowance, reject oversize before
archive construction, and require transport EOF evidence. This protection is
independent of limits enforced while expanding the archive.

Resource downloads retaining per-response provenance additionally admit at most
65,536 response pages and 128 MiB of retained payload/frame buffers. This bounds metadata/diagnostic overhead for an unusually
small StatsQuery chunk size; reaching it returns LimitExceeded and retires the
pin without claiming EOF. It does not change Python legacy request counts or
limits. Large ignored protocol tails count toward the retained buffer bound as
well as file data; this prevents small content blocks from hiding large raw
allocations. Ordinary byte-only download_file does not retain page metadata. Raw
frames/payloads remain controlled by RawPolicy, and response frames retain the
existing runtime limits. All output reservations are fallible and byte bounded.

## GBK — shared source table

HQ statistics, company information and offline readers share the original
Python 3.12.14 strict GBK scalar table and private scanner. The table was moved,
not regenerated, in Stage A2. Consumers explicitly select replacement or ignore
for invalid source bytes. Company filenames use a bounded reverse lookup of
the same mapping, reject NUL/unrepresentable scalars and enforce encoded byte
limits. The legacy protocol text constructor retains its original encoding
policy; the strict SDK passes encoded bytes through a separate checked
constructor. No second mapping or public caller-injected codec was added.

## F10 JSON, URL and transport — T19 in progress

The optional `f10` feature now declares workspace `serde_json = 1.0.145` and
`url = 2.5.8`. Neither activates the public model `serde` feature. JSON uses the
SDK's ordered `JsonValue`/validated numeric lexeme scanner; `serde_json` only
decodes an individual quoted string, so it cannot collapse duplicate object
members or coerce large numbers to floating point. Request strings use compact
UTF-8. New parsing/encoding work is bounded by bytes, 128 nesting levels and
1,000,000 value nodes; transport callers supply the shared operation boundary.

Local cached `url 2.5.8` metadata declares Rust 1.63 and MIT OR Apache-2.0.
The workspace serde_json pin is reused as declared; its availability, license,
transitive graph, lock and actual MSRV have **not** been validated. No Cargo
resolution, dependency download, compilation or test command ran.

Production transport selection is still open. Local cached source was read
for hyper 1.12.0 (Rust 1.63, MIT), hyper-util 0.1.21 (Rust 1.85, MIT), rustls
0.23.46 (Rust 1.71, Apache-2.0 OR ISC OR MIT), and tokio-rustls 0.26.6 (Rust
1.71, MIT OR Apache-2.0). Hyper's explicit header-size and connection-level
APIs are candidate mechanisms; reading their source is not verification.
No HTTP/TLS dependency is selected or enabled in this checkpoint.

Remaining requirements before selecting the transport: a genuinely bounded
asynchronous DNS resolver (no detached blocking getaddrinfo), system/custom
root loading at first network use, explicit CONNECT and environment isolation,
address-family fallback with hostname TLS, streaming compressed/decoded caps,
and cancellation/close retiring every owned task. It must consume the existing
`RequestPermit`/`AttemptAuthorization` instead of inventing a second operation
budget. T19 is incomplete until this path and all required source cases exist;
final execution evidence remains deferred until all implementation is complete.

### T19 third source checkpoint — provisional transport is now authored

The previous unselected-transport paragraph is historical. F10 now declares
the following optional source choices; **none is resolved, downloaded, locked,
compiled, license-audited or exercised in this implementation session**:

| Dependency | Pin/features | Role and expected metadata |
| --- | --- | --- |
| hyper | 1.12.0, client/http1 only | HTTP parser/framing; cached metadata Rust 1.63, MIT |
| hyper-util | 0.1.21, tokio only | Tokio I/O bridge; cached metadata Rust 1.85, MIT |
| http-body-util | 0.1.5 | streaming response frames, fixed request body; metadata pending |
| rustls | 0.23.46, std/ring/tls12, defaults off | certificate/hostname TLS; cached metadata Rust 1.71, Apache-2.0 OR ISC OR MIT |
| tokio-rustls | 0.26.6, ring/tls12, defaults off | cancellable TLS handshake; cached metadata Rust 1.71, MIT OR Apache-2.0 |
| rustls-native-certs | 0.8.2 | system trust loading at first TLS use; pin/API/MSRV/license provisional |
| hickory-resolver | 0.24.4, tokio-runtime/system-config, defaults off | asynchronous DNS without blocking getaddrinfo; pin/API/MSRV/license provisional |
| flate2 | workspace 1.1.2, rust_backend | same existing inflater for streaming gzip/deflate, no extra ZIP stack |
| rcgen | 0.13.2, crypto/ring, dev only | authored local TLS certificate fixture; pin/API/MSRV/license provisional |

One synchronous public call starts an owner-registered worker with its own
current-thread Tokio runtime, capped at four live workers per F10 owner.
The worker's DNS/TCP/TLS/HTTP futures all use the original operation sponsor;
no blocking DNS task is detached. Physical connect attempts reserve/commit the
existing RequestPermit authorization. Failed addresses and redirects preserve
one increasing attempt number. There is no application retry after a POST.
The worker drops its runtime before publishing done, and callers wait for the
thread to finish before joining. Owner close stops and joins registered work.

System DNS configuration and native trust loading are synchronous local OS
operations inside the registered worker. Cancellation can terminate waiting
and return CancelUnconfirmed if one of those OS calls fails to retire before
the cleanup deadline; success must never claim the thread exited without the
join. Their platform behavior, including Windows trust-store loading, remains
an explicit final verification concern. No build/getter reads either source.

Hyper enforces response header bytes; streamed body frames enforce encoded
bytes, and the inflater's checked writer enforces decoded bytes. A separate
bounded observer retains actual HTTP plaintext (TLS ciphertext excluded) and
bounds chunk/header overhead. RetainPayload stores encoded body bytes;
RetainWire additionally stores received HTTP plaintext. Ordered caller headers
are emitted through a bounded first-header-block adapter because HeaderMap
alone groups separated duplicate names. All of these contracts still require
the authored public-client fixtures and final feature/platform/MSRV runs.

### T19 fourth source checkpoint — transport boundary follow-up

The zlib path now uses the already-selected flate2 `Decompress` primitive with
8 KiB output chunks, requiring `StreamEnd` and rejecting bytes after the stream.
This makes checksum/truncated-end handling explicit without a second inflater
dependency. Gzip retains MultiGzDecoder so concatenated members remain valid;
fixture source covers both concatenated members and missing footer bytes.

Explicit connect probes share a cancellable single-slot gate in addition to
the owner's four-worker cap. Both leases stay with a registered worker when a
caller cannot confirm cleanup. JSON Params now share one document-wide node,
depth and byte budget; request/response retention checks cancellation during
copying. Unknown top-level service metadata and repeated HitCache keys remain
available through Response::attributes even when whole-body retention is off.

No candidate API, compile, dependency graph, runtime fixture, formatter or
platform check has been executed. These remain implementation source changes,
not a dependency-selection validation result.

### T20 local civil-date dependency (provisional)

`chrono = 0.4.42`, default features disabled, `clock/std`, optional under `f10`,
implements the ladder's call-time operating-system local civil date. HQ's
fixed Shanghai clock is not substituted for Python's `date.today()` contract.
The public `LocalDateClock` allows an explicitly injected deterministic date;
construction and explicit-date calls never invoke it. The crate pin, API,
MSRV, license, timezone discovery and Windows/Linux/macOS behavior remain
unresolved and unverified until the final dependency/platform phase. No
dependency download or metadata command has run for this declaration.
