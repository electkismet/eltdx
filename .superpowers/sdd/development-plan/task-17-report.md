# T17 Stage A — bounded archive and source statistics checkpoint

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
