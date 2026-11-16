# T17 Stage A — bounded archive and source statistics checkpoint

## Stage B implementation checkpoint — 2026-11-16

All twelve M67–M78 public method sources and their lazy TdxClient getters are
now authored. This section supersedes historical notes that root/Cargo/shared
GBK wiring was still missing. T17 source implementation and limited self-review
are complete; no independent review or execution is claimed.

- Registered the single archive/stats decoder in default HQ and exact provisional
  zip0.6.6(deflate only)/crc32fast1.5.0 dependencies. Wrote
  docs/rust-sdk/dependency-decisions.md. No resolution/download/lockfile update,
  compiler, tests or license/advisory verification was run.
- Added types/resources.rs, types/company_info.rs and types/session.rs with
  DownloadResult/Completion, stats/source DTOs, Table, company queries/content,
  handshake/heartbeat/local ping and endpoint-bearing health reports.
- Added services/resources.rs/company_info.rs/session.rs/health.rs and private
  stats_projection.rs. One operation and pin spans every download page. Exact
  range/cap is not EOF; short/empty validated content is. Zero cap avoids I/O;
  strict download rejects uncertain completeness. A compressed-cap-plus-one
  allowance is checked before archive construction; pin is retired before CPU
  decoding, which still checks the same operation deadline/cancellation.
- Added explicit physical request/epoch/slot/generation/receive-sequence source
  diagnostics. Per-response provenance is bounded to65536 pages and128MiB
  retained payload/frame buffers, including ignored protocol tails. This new
  Rust resource guard is documented in the HQ spec and dependency decision;
  it must not leak into Python legacy limits or counts.
- Stats preserves source market/code/date/integer decimals/f64 units even when
  public identity/date/u32/converted units are unavailable. Typed share values
  multiply source ten-thousand shares by10000 in f64 with source provenance;
  money becomes CNY and opening volumes stay Lots. Nullable conversion failure
  records a bounded diagnostic, never fabricates a zero. Main-date/95%/prior-day
  helper policy remains T18.
- Company filenames use bounded reverse lookup of the existing strict GBK table.
  Added an encoded-byte request constructor without changing the protocol legacy
  string constructor. Added a reusable protocol business-content slice, while
  retaining the old text parser signature. Rust name/body replacement loss is
  explicit and byte offsets remain exact. Existing unpriced decoder now admits
  only the additional resource/company request variants, no metadata fetch.
- Local ping is available after close without admission/startup. Handshake and
  heartbeat retain semantic control bytes under Omit; probe explicitly performs
  heartbeat in its original context and reports actual endpoint and duration.
- Authored public resource, company, HQ loopback, stats projection/oversize and
  source-identity loss cases. Updated stats source cases to include the real
  shared encoding module, removing the temporary fixture mapping. Fixture write
  shutdown errors are treated as normal controlled disconnects, matching reads.

Limited self-review corrected the public CodeQuery/list/cursor source-case
signatures, fixture identities and local pong expectation; added nonzero control
bytes/source dates, expired entry, controlled lost-pin/no-fallback and unknown
stats identity/date/overflow projection cases. Found and corrected the existing
CodeService::list context binding to be mutable before its work-limit setter.
The retained raw cap is128MiB, allowing full valid32MiB statistics with both
payload and wire while still bounding oversized trailing bytes. No source review
claim establishes decoder/runtime behavior. Final independent ZIP corpus and
dependency/MSRV/license checks remain E04/final unified validation obligations.

T18 handoff: all HQ service seams are now present. Keep Python zero-cap pin
observation, optional/unlimited download cap, chunk_size and per-request timeout
policy explicit; do not apply new Rust provenance caps to legacy work. Preserve
original finance/stat fields and helper date/coverage rules. T22 reuses the same
decoder/preset and in-context download seam. Update migration evidence only when
the actual Python/native adapter is authored, not on this T17 source completion.
All authored cases remain unexecuted; runtime dependency behavior is provisional.

## Stage A2 — approved shared GBK extraction checkpoint

Controller source-only review approved Stage A `cd6735b` with no findings, then
authorized this narrower shared encoding integration before network Stage B.
The commit containing this section is the Stage A2 checkpoint; its SHA is in
the handoff. The earlier Stage A notes below remain the historical evidence and
limitations, with the following explicit updates:

- The worktree absolute root and both source/destination absolute file paths
  were resolved and checked to remain under
  `C:\Users\ax\.codex\worktrees\rust-sdk-implementation\tdx抓包`
  before native PowerShell `Move-Item` calls. Destination nonexistence was
  checked. No recursive move/delete or cross-shell filesystem operations ran.
- Moved `crates/eltdx/src/offline/gbk_pairs.rs` to
  `crates/eltdx/src/encoding/gbk_pairs.rs`. The actual mapping constants were
  preserved by moving the original file, without parsing/regenerating/replacing
  mapping values. Two comment lines record the move and current generator path;
  original generator/recipe, executable and Python 3.12.14 provenance are kept.
- Moved `crates/eltdx/src/offline/generate_gbk_pairs.py` to
  `crates/eltdx/src/encoding/generate_gbk_pairs.py`. Only the two emitted
  generator/recipe path strings now name `encoding/`. Its codec, iteration,
  sentinel, indexing and generation logic are unchanged. **The generator was
  not run**, and no byte/hash/diff equivalence check was executed. Preservation
  is by the native move and the intentionally comment-only table edit.
- Added `crates/eltdx/src/encoding/mod.rs` with private `gbk_scalar` strict
  two-byte lookup and allocation-free `scan_gbk`. The scanner takes explicit
  `Replace`/`Ignore` policies, reports whether invalid bytes occurred, and emits
  one scalar at a time. Invalid lead/undefined pair consumes only one byte;
  ASCII, including NUL, passes through unchanged; standalone 0x80 is invalid.
- `scan_gbk` has a caller boundary before input, after each 16 KiB of scanned
  input (a two-byte pair may cross by one byte), and before successful return.
  Errors propagate unchanged; already-emitted output belongs to the caller
  and cannot be delivered as complete after a boundary error. Output allocation
  and input limits remain each consumer's responsibility.
- Added only private `mod encoding;` to `crates/eltdx/src/lib.rs`. No archive,
  stats, services or other module registration was made in this checkpoint.
- Removed the old `offline::gbk_pairs` module declaration and changed existing
  `offline::decode_gbk` to delegate to `encoding::scan_gbk(Replace)` using an
  infallible no-op boundary, preserving its signature and prior cancellation
  behavior. Future users can pass real cancellation checks to the shared
  scanner. The old offline wrapper itself has not gained a new public policy.
- `stats.rs` and `archive.rs` remain unchanged. The existing
  `crate::encoding::gbk_scalar` reference is now satisfied by this single table;
  the existing stats scan still owns `Ignore` and per-chunk operation checks.
  No caller-injected production codec, copied mapping table or public encoding
  API was introduced. Stage B may consolidate its scan loop only with explicit
  scope approval; it already shares the strict scalar source.
- Added `crates/eltdx/tests/encoding.rs`: source cases for known strict pairs,
  ASCII/NUL, invalid lead/trail and undefined pairs, standalone 0x80, replacement
  versus ignore with recovery, cancellation before emission/between chunks/at
  final return, empty input, a valid pair straddling a chunk boundary, and the
  public offline industry consumer under the offline feature. All are unrun.
- Controller accepted the financial preset's **all-member cumulative expanded
  256 MiB as a new documented Rust protection**; selected DAT remains separately
  limited to 256 MiB. The protection must not leak into the legacy Python
  selected-DAT path. This resolves the earlier provisional aggregate-policy
  question, without changing archive source or dependency validation status.

Stage A2 has no Cargo changes or dependency fetches, no tests/collection/build/
check/compilation/clippy/doctest/AST/format/diff checks, and no network actions.
Only source reads/edits, the bounded native moves and ordinary Git checkpoint
operations with hooks disabled were used. Source review alone does not confirm
runtime correctness. T17 Stage B, full default-HQ archive/stats registration,
dependencies and public resource/owner integration remain outstanding.

Status: **partial T17 implementation only; authored, not executed or validated**.
Stage A owns only `crates/eltdx/src/archive.rs`, `crates/eltdx/src/stats.rs`,
`crates/eltdx/tests/stats.rs`, and this report. No Stage B public service, Cargo,
root registration, DTO, Python, shared owner/operation or ledger changes are part
of this checkpoint. Reviewed base was d8574bb; the controller's documentation
checkpoint bf94f0b was HEAD when authoring began. The commit containing this
report is the Stage A checkpoint (its SHA is supplied in the handoff).

## Source and method coverage

Read the T17 brief, HQ API §10 and limits/F23–F27, public API download/statistics
contract, decision register §3/E04, Python `stats.py`, resource/company/session/
health APIs, resource dataclasses and resource tests; inspected the existing
operation boundary, error types, market identifiers, resource protocol parser,
and T21 offline GBK implementation. No network requests were made.

- `archive::decode_selected`: crate-private, in-memory, bounded ZIP decoder with
  immutable policy presets, exact raw-name selection and an owner-provided
  `FnMut() -> Result<()>` cancellation/deadline boundary. Every member is streamed
  and CRC checked, including unselected members; only selected bytes are retained.
- `ArchiveLimits::stats()`: compressed 32 MiB, 128 entries, each expanded 32 MiB,
  cumulative actual/declared expanded 64 MiB.
- `ArchiveLimits::financial()`: compressed 64 MiB, 64 entries, each expanded
  256 MiB, cumulative expanded 256 MiB. **The aggregate 256 MiB is provisional**:
  the current decision register explicitly specifies per-DAT 256 MiB but does
  not state an archive-wide expanded limit. T22/controller must affirm or revise
  this finite internal preset before integrating it; no public unlimited knob.
- `stats::parse_stats_archive`: required unique `tdxstat.cfg`/`tdxstat2.cfg`, usable
  row requirement, per-table unique `(Python int(market), trimmed code)`, raw
  date strings/source path, source line numbers, all Python dataclass fields,
  nullable finite floats and finite-float integer truncation. Independent table
  dates are accepted. Stats integer source values stay private and have decimal
  compatibility representation plus checked `as_u32` projection; no new public
  number type is introduced.
- Native source units remain explicit in field names: `_10k` monetary/share
  values and `_hand` opening volumes. No guessed conversion, date, security
  identity or local-clock substitution occurs. Unknown market/non-six-digit
  ASCII code is retained with `UnknownIdentity` diagnostics for later projection.
- GBK strict scalar semantics are a **pending shared integration** described
  below. Stats ignores invalid bytes one byte at a time, resumes at the next
  byte and records loss; T21 offline replacement semantics remain separate.

## Proposed dependency, exact evidence and unknowns

Propose workspace `zip = { version = "=0.6.6", default-features = false,
features = ["deflate"] }`; default-HQ `eltdx` must depend on it unconditionally.
Also propose direct workspace `crc32fast = "=1.5.0"` and unconditional `eltdx`
dependency for independent accumulated CRC. No dependency file was edited.

Local source folder search had found no `zip` implementation. The local sparse
registry record at `.cargo/registry/index/index.crates.io-1949cf8c6b5b557f/.cache/3/z/zip`
was read as text only. Its 0.6.6 entry states:

- version 0.6.6, `yanked: false`, `rust_version: 1.59.0` (workspace MSRV 1.89);
- checksum `760394e246e4c28189f19d488c058bf16f564016aefac5d32bb1f3b51d5e9261`;
- `deflate` enables `flate2/rust_backend`; optional flate2 requirement `^1.0.23`,
  default features disabled; existing workspace flate2 is =1.1.2/rust_backend;
- crc32fast requirement `^1.3.2`, compatible by range with existing locked 1.5.0;
- nonoptional byteorder `^1.4.3`; crossbeam-utils `^0.8.8` on specified 32-bit
  architectures. Defaults that we disable include AES, bzip2, time and zstd.

The sparse record does **not** contain license text. Expected zip license is MIT
and expected crc32fast license is MIT OR Apache-2.0 from prior API knowledge;
both remain **unverified proposals**, not source/license acceptance. API use
(`ZipArchive::new/by_index`, `ZipFile::{name_raw,size,compressed_size,crc32}`,
`Read`, test-only `ZipWriter/FileOptions`) is based on known 0.6 APIs, not local
source inspection or compilation. Transitive resolution, target MSRV, exact
license files/advisories, decoder EOF/CRC/trailing compressed-byte behavior,
writer duplicate-name behavior and interoperability are all unverified. No
claim that dependencies are fetched, locked, built or security-reviewed.

0.6.6 is a deliberately narrow provisional API with the existing flate2 backend
and one ZIP implementation. 2.4.2/4.6.1 sparse records were also inspected;
their additional backend/feature/dependency choices were not adopted. There is
no custom deflate implementation or second decompression stack in this patch.

## Bounds, cancellation and cleanup reasoning (not runtime proof)

The compressed-byte cap is checked before ZIP construction. A bounded EOCD
scan checks classic ZIP counts and directory extent, then central records are
count/byte bounded before the production library can allocate its directory.
All exact duplicate raw names and encryption flags/extras are rejected. Local
names, methods, flags, CRC and sizes must match central declarations; bit-3
data descriptors are checked in signed and unsigned form. ZIP64, spanning,
unsupported codecs/flags, prefixes, overlapping records and unlisted local
bytes fail closed. This deliberately supports a stricter classic Store/Deflate
subset than arbitrary Python ZipFile; real fixture compatibility remains an
acceptance item.

Archive member allocations follow validated sizes and `try_reserve_exact`.
Reads use a 16 KiB stack buffer with an additional single-byte EOF probe at
declared/cap boundaries. Checked actual per-entry and aggregate counts precede
any append; actual size and independently accumulated CRC must match. A
ZipFile's bounded compressed region still relies on the proposed library to
reject malformed deflate data; source/runtime review must confirm that it
cannot silently truncate at an advertised size or overlook trailing bytes.

Stats scans GBK input, line boundaries, pipe columns and numeric text in bounded
CPU chunks. Cancellation is observed before/after archive construction/opening,
reads, table work and final return. A single library call, allocator call,
standard float parse, comparison/hash or string copy is not preemptible; its
input is nevertheless byte bounded. Recheck after that work before successful
delivery. The eventual service must supply the *same* operation deadline and
cancellation state from download through decoding, using its Decode phase.

Rows and identities use checked counts plus fallible reservation, with no new
arbitrary row cap. Natural maximum row counts derive from admitted source bytes
and minimum usable line lengths (stat 36 bytes; stat2 22). Text reserve is at
most 3× member bytes for Unicode expansion. The source-byte bounds imply finite
but potentially substantial DTO/key/string overhead; this is not a claim that
the 64 MiB expanded-payload cap bounds all heap overhead to 64 MiB. Diagnostics
retain at most 128 details and accurately count omitted details.

All state is local and drops on any error. The parser owns no session/pin and
cannot itself establish EOF or retire a pin. **Stage B must prove pin cleanup**
and ensure a `LimitReached`/truncated transport result is never called complete.

## Authored, unrun cases

`tests/stats.rs` includes the private source modules directly, without adding a
public parsing API. It has a tiny test-only `encoding::gbk_scalar` fixture shim
for the exact GBK bytes used in these cases; it is not a production GBK table or
a substitute for later full scalar/codec integration tests.

Cases cover stored/deflated source rows and every field, distinct source dates,
missing/repeated members, empty usable tables, duplicate identities, blank/
invalid/NaN/infinite numeric values, signed fractional truncation, underscore
and GBK full-width digits, >i64 finite integer, unknown identity retention,
invalid GBK loss/resumption, source line separators, capped diagnostics with
exact omitted count, encryption, bad CRC, truncated ZIP, local/central mismatch,
unselected member CRC, compressed oversize before selector/decoder, declared
single/aggregate expansion caps, dishonest expansion, exact 32 MiB entry,
damaged deflate, aliased local offsets, entry-count preset differences, signed/
unsigned descriptors, and cancellation/timeout boundaries. Expected values are
literal source expectations; ordinary fixtures use the single production ZIP
writer, with deliberate byte mutations for faults. Independent external ZIP
fixtures and corpus interoperability still belong to final validation.

## Stage B and cross-task handoff

1. Register `mod archive; mod stats;` in the default-HQ crate root and add the
   exact proposed workspace/crate dependencies after controller approval. Do
   not resolve or fetch them while the all-T01–T30 execution prohibition applies.
2. Under separate explicit integration authorization, move T21's *single*
   source-derived GBK table/generator into a crate-private shared encoding area.
   Supply `crate::encoding::gbk_scalar(u8,u8)->Option<char>` with strict Python
   GBK mappings, rejecting invalid lead/trail pairs. Offline and stats must
   share the scalar scanner/table while choosing replacement versus ignore
   policies separately. No production fallback or caller-injected decoder is
   left in stats. Current stats module cannot compile until this seam exists.
3. Implement resources/company/session/health and public DTO/query/getter wiring
   in Stage B: bounded complete/M+1 stats download, one pinned source and owner
   cleanup; checked public units/counts/dates/identities; preserved metadata,
   raw policies/control fields; GBK company byte offsets/encoding; local ping and
   explicit heartbeat/probe. Preserve Python behavior in the later bridge.
4. T18 consumes the raw fields, keeps explicit main-date/95%/prior-trading-date
   rules and uses source identity joins. Raw parsing does not implement those
   helper gates or silently filter unknown identities.
5. T22 reuses `ArchiveLimits::financial` and `decode_selected`; no second ZIP
   implementation. T22 owns .dat choice/ambiguity, DAT validation and the
   provisional cumulative financial expanded cap decision above.
6. Later integration tests must exercise nonempty public-client requests,
   download cap0/exact6/cap7/short/empty/malformed/host-change behavior, company
   byte offsets, pin retirement, final close and shared deadline. None is
   represented as implemented or validated by the Stage A parser source tests.

## Execution prohibition and review status

No tests, test collection, compilation, cargo check/build/metadata/resolution,
clippy, doctests, AST/format/diff checks, network fetches or equivalent execution
were run. The explicit user/controller prohibition overrides normal TDD red/
green and verification workflows. Source cases were authored first, then
implementation and manual source review. Only source reads/writes and ordinary
git status/log/add/commit with hooks disabled were used. This is **not full T17
completion**, a passing test claim or approval to proceed with Stage B.
