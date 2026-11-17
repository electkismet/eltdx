# Financial implementation and recovery index

This is a source map and contract, not a passing build or runtime report.
All code and examples below await the final T01–T30 execution gate.

## Rust entry points

Enable `eltdx`'s `offline` feature. The financial module uses the ordinary HQ
resource service and its owner; it does not require `f10`, Python, EX or MAC.

| API | Implementation | Purpose |
| --- | --- | --- |
| `TdxClient::financial()` | `client.rs` | Owned handle sharing the client's close state |
| `financial::open_calc_client(&CalcClientConfig)` | `financial.rs` | Lazy construction, default `120.76.152.87:7709`, ordinary eight-second timeout, no probe |
| `FinancialService::list_archive_files(options)` | `financial.rs` | Complete HQ download of `tdxfin/gpcw.txt`, then list parsing |
| `list_archive_files_detailed(options)` | `financial.rs` | Same list with actual HQ source/raw metadata |
| `download_archive(filename, download, options)` | `financial.rs` | HQ `0x06B9` download of checked `tdxfin/<leaf>`; capped output remains `LimitReached` |
| `download_records(query, options)` | `financial.rs` | Complete list/download/ZIP/DAT composition returning the specified `ReadResult` |
| `download_records_detailed(query, options)` | `financial.rs` | Adds header, completion, archive identity, raw archive/DAT and network metadata |
| `download_records_with_options(query, read, download, options)` | `financial.rs` | Caller-supplied smaller limits; query date/selector and operation raw policy take precedence |
| `offline::financial::parse_file_list` | `offline/financial.rs` | Pure list parsing, invalid signed/overflow sizes retained with diagnostics |
| `parse_dat` / `parse_dat_detailed` | `offline/financial.rs` | Checked pure DAT parsing, optional explicit report date |
| `parse_dat_for_archive` | `offline/financial.rs` | Explicit date, then remote filename date, then raw header |
| `read` / `read_detailed` | `offline/financial.rs` | Local path dispatch: case-insensitive `.zip`, otherwise direct DAT |
| `parse_zip` / `parse_zip_detailed` | `offline/financial.rs` | In-memory local archive semantics; selected member name never supplies the date |

Query, options, provenance and record fields are private with getters and
checked builders. `DatSelector::Named` is also validated when accepted by an
options/query builder, because the enum variant can be constructed directly.

## Resource and data contracts

`DownloadOptions` is one shared type, re-exported at the crate root and under
`financial`. Defaults are 64 MiB, 30,000 bytes per chunk, offset zero and
4,096 chunks. HQ chunk size cannot exceed 60,000, and offset plus byte cap
must fit the wire's `u32`. An expected size is a hint only. A full-sized final
chunk does not prove EOF even when it matches that hint. Zero cap performs no
network work and returns empty `LimitReached` from `download_archive`; a
complete-records request rejects that state. Archive operations reject a
nonzero offset. Configured chunk limits use the existing HQ pin/cleanup loop.

The records composition uses one operation deadline, cancellation state and
request budget through list, archive, ZIP and DAT. A smaller read archive cap
also narrows the source download cap. The list is complete before parsing;
its hash and size remain unauthenticated metadata. Absence of a matching list
row does not invent a missing archive or prevent an explicit download.
Noncomplete downloads never enter the decoder, and incomplete DAT indexes
never produce a successful complete-records result.

The shared `archive.rs` performs bounded structural preflight before creating
the sole ZIP decoder. Metadata-based selection returns an exact entry index;
the same index is compared and read by the ZIP library. It never extracts
paths to disk. Strict financial bounds are 64 MiB compressed, 64 entries and
256 MiB total expanded across all members. The selected DAT separately obeys
the caller's lower DAT cap. Every member's declared and actual lengths/CRC
are checked. Encrypted/duplicate/unsupported entries and malformed headers
are rejected. Directory attributes and slash names do not become candidates.
DAT suffix matching is case-sensitive. RequireSingle rejects multiple
distinct candidates; Named selects one exact candidate after whole-archive
validation. Error cases remain distinct from valid empty DAT data.

DAT limits are 256 MiB, 4,096 fields per record and 4 Mi decoded float values.
Header/index/range arithmetic is checked before allocation. Default malformed
index handling records omissions and `Incomplete`; strict-index mode rejects.
Duplicate accepted offsets and overlapping metadata/record ranges are errors.
Values retain original f32 bits, including NaN, infinities and signed zero.
Fields have no invented universal units or named financial meaning.

Local ZIP/DAT reads use the header unless explicitly overridden. Network
records choose explicit query date, then the first valid eight-digit date in
the remote leaf, then the header. Filename scanning uses the frozen Unicode
15.0.0 decimal-digit table. Invalid dates remain raw and diagnosed. The
calculation server's date is never replaced with today's date.

Detailed results distinguish DAT raw bytes, compressed ZIP raw bytes and
actual HQ frame/payload metadata. `ReadResult` retains row diagnostics but
has no provenance/completion slots; consumers needing those use the detailed
API. No hash-verification claim is made.

## Source evidence and consumers

| Source | Intended evidence after final execution |
| --- | --- |
| `crates/eltdx/tests/financial.rs` | DAT layouts/limits/bit preservation, dates, list parsing, local I/O, ZIP selection/provenance/CRC/encryption/limits |
| `crates/eltdx/tests/financial_service.rs` | Actual HQ resource paths, lazy/closed owner, zero cap, EOF versus cap, complete composition, stale metadata, deadline/cancellation |
| private financial source cases | Inherited errors, Unicode dates and UTF-8 replacement across cancellation chunks |
| existing `tests/resources.rs` and `tests/stats.rs` | Regression coverage of the shared resource loop and ZIP decoder |
| `examples/financial_read.rs` | Explicit local-path reader |
| `examples/financial_download.rs` | Explicit endpoint/remote-leaf download with owner cleanup |
| `tests/consumers/rust-sdk/src/financial.rs` | One production dependency, std-only hand-authored ZIP/DAT, actual HQ list/archive downloads, raw signed zero, filename date and shared close |
| `tests/consumers/python-wheel/consumer.py` | Installed public Python models, actual HQ list/archive download and local DAT/ZIP header-date behavior without Rust build tools |

No listed command has run. At the final execution gate, include:

```text
cargo test -p eltdx --features offline --test financial --test financial_service --test resources --test stats --locked
cargo build -p eltdx --features offline --examples --locked
cargo run --manifest-path tests/consumers/rust-sdk/Cargo.toml --features sdk-offline -- financial
```

The independent consumer must also run against a packaged/copied SDK as part
of T28/T29. A source path consumer alone does not establish package contents,
MSRV/platform support or independence from the repository layout.

## Python source handoff completed in T23

T23 native binding sources retain the existing Python API and custom resource-client hooks. Its
first-DAT-in-archive-order rule, Python runtime `int`/Unicode `re`, arbitrary
signed file sizes and integer date overrides differ intentionally from the
strict Rust API. Legacy direct DAT reads do not acquire the new total-byte
cap or strict overlap policy. Legacy ZIP does not acquire new all-member
validation/expansion rules. Those differences require explicit adapters over
the shared algorithms; calling the strict public service from Python is not
a compatibility implementation. Original exceptions and partial-download
behavior remain part of the authored Python tests. Implementation mappings
are in [T23 migration index](t23-migration-index.md) and
`tests/contracts/manifests/rust_sdk_migration.json`; none has been executed.
