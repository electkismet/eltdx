# T22 isolated Stage A implementation report

Status: Stage A source implementation complete; public registration, shared ZIP integration and all network composition remain Stage B. No compilation or executable verification has occurred. No whole-T22 completion claim is made.

Authority: `task-22-brief.md` staged authorization; development-plan T22; extension-api-spec sections 6–7; public-api raw/resource/error contracts; current T21 offline implementation; Python `src/eltdx/offline/history_financial.py` and `src/eltdx/financial.py`. The controller supplied review base `7a85f5c` for T10 context, not ownership of T10 edits. T17 `cd6735b` has a scoped static Stage A approval, not dependency/build/runtime validation. The later controller ruling in decision-register fixes new Rust financial ZIP total expanded bytes at 256 MiB across all members, separate from the selected-DAT cap.

Owned paths only:

- `crates/eltdx/src/offline/financial.rs`
- `crates/eltdx/tests/financial.rs`
- `.superpowers/sdd/development-plan/task-22-report.md`

Source checkpoint: `d6de324` (`feat(financial): author isolated DAT and list parsing stage A`) contains the initial parser/models/options/tests. The final owned-path checkpoint containing this report and follow-up source review changes is returned to the controller with its exact SHA. Hooks are disabled for both checkpoints. No shared root/offline registration, archive.rs, Cargo, encoding, T10 files, ledger or controller status files were edited.

## Implemented pure API and method coverage

| Required method / component | Stage A source | Stage B remaining work |
| --- | --- | --- |
| `offline::financial::parse_dat(data, report_date, options) -> Result<ReadResult<FinancialRecord>>` | Complete DAT algorithm and T21-compatible result projection | Register module/exports |
| `parse_dat_detailed(...) -> Result<FinancialDatResult>` | Header, completion, index counts, raw payload and provenance-capable result | Decide whether to additionally export detailed API at offline root |
| `parse_dat_for_archive(data, filename, report_date, options)` | Pure Rust date precedence over already-selected DAT | Feed only complete, verified archive content |
| `parse_dat_for_archive_with_boundary(..., &mut FnMut() -> Result<()>)` | Crate-private operation/deadline callback at bounded CPU units | Pass the same HQ operation checkpoint used for list/download/ZIP |
| `parse_file_list(data) -> Result<Vec<FinancialFileInfo>>` | Replacement decoding, source whitespace rules, comma splitting and metadata | Register/export |
| `parse_file_list_with_strict(data, strict)` | Optional rejection for negative/overflow size; default nullable size with row diagnostic | Any network strict preference remains explicit |
| `parse_file_list_with_boundary(..., boundary)` | Crate-private list checkpoints, unchanged error propagation | Use inherited operation in network list service |
| `read_dat_file(path, options) -> Result<FinancialDatResult>` | Direct DAT file loading through T21 `bars::load`, selected byte cap and cancellation | Public `read` must route `.zip` via shared decoder and `.dat` here |
| `offline::financial::read(path, options) -> Result<ReadResult<FinancialRecord>>` | DAT constituent complete | Entire public dispatcher withheld rather than ship a ZIP error stub |
| `FinancialArchiveName`, `FinancialRecordsQuery` | Checked pure remote name/query values | Re-export/use in root financial module |
| `FinancialParseOptions`, `FinancialReadOptions`, `DatSelector` | Private fields/getters/checked options; exact named selector | Connect selectors to shared archive adapter |
| `FinancialHeader`, `FinancialRecord`, `FinancialDatResult`, `FinancialCompletion`, `ReportDateSource`, `ArchiveProvenance`, `FinancialFileInfo` | Source-authoritative local result/model types and getters | Populate archive provenance only from validated shared entry metadata |
| `FinancialService::list_archive_files` | Fixed `FINANCIAL_LIST_PATH = tdxfin/gpcw.txt` and parser ready | Owned HQ service implementation |
| `FinancialService::download_archive` | Remote leaf/value ready | HQ 0x06B9 complete/bounded source download |
| `FinancialService::download_records` | Query, parsers, inherited CPU boundary ready | Same-operation list/download/ZIP/DAT and Complete-only composition |
| `open_calc_client` / `CalcClientConfig` | No network stub introduced | Lazy TdxClient composition, default 120.76.152.87:7709, 8s, probe_hosts=false |
| `DownloadOptions` | No competing partial type introduced | Coordinate existing HQ resource options and max_bytes/chunk/offset budgets |

All mutable DTO fields are private. Getters return references, slices or Copy values. `FinancialArchiveName` accepts a nonempty ASCII remote leaf without separators, colon, NUL/control bytes or surrounding whitespace, reserves the seven-byte `tdxfin/` prefix inside HQ's 300-byte path limit, and never creates a local path.

## DAT semantics and limits

- Header is the exact 20-byte `<hIHIII>` layout: signed unknown, raw date, u16 max_count, unknown1, report_size, unknown2. Unknowns and raw date are available regardless of RawPolicy. Index layout is 11 bytes (`6s`, market u8, absolute u32 offset).
- New Rust direct DAT has a 256 MiB maximum; callers can lower it. Per-record fields <=4096; total decoded fields <=4*1024*1024. Options reject zero, values above source limits, and allocation-unrepresentable values. Header/index multiplication, offset addition and total-field addition use checked arithmetic; report_size not divisible by four and exceeded field budgets are ResourceFormat. Byte input excess is LimitExceeded. Field allocations follow size checks. Full retained payload allocation occurs only after all DAT checks complete.
- Empty bytes are complete empty data with no header. Nonempty <20-byte input is Incomplete plus diagnostic (strict rejects). report_size=0 is successful empty, even if index count is nonzero, preserving source behavior. Nonzero reports inspect only full index entries. Out-of-range/truncated indexes are skipped/broken with diagnostics and Incomplete; strict_index rejects those faults. Empty-code and zero-offset entries remain omitted with explicit incomplete facts, including under strict mode (strict applies to malformed ranges/truncation).
- Repeated previously accepted valid offsets fail in both policies, matching current Python. New Rust additionally rejects record/header-index overlap and non-identical record range overlap. A sorted range map permits nonoverlapping out-of-order data while returning rows in index order. Invalid offsets are not added to accepted ranges.
- Code decoding uses shared ASCII replacement then trailing NUL removal; market stays arbitrary u8. Every float uses `WireFloat::from_bits`; NaN payloads, infinity, signed zero and subnormals remain exact. No field dictionary, unit or financial metric name is guessed.
- New Rust date precedence is explicit TradingDate > first valid eight-digit filename match > header. Only the filename leaf is scanned. Direct local DAT reads use header unless explicitly overridden. Invalid header integers remain available and are diagnosed even when an override is valid. Invalid effective legacy overrides are also diagnosed. No clock/today substitution exists.
- `ReadResult` has no completion/header/provenance slots. `parse_dat` preserves its specified signature and exposes any omissions in diagnostics. `FinancialDatResult` is an additive detailed wrapper containing explicit Complete/Incomplete, consumed and omitted indexes, optional header, date source, shared raw payload and optional archive provenance. Stage B `download_records` must consult detailed completion rather than treat any `Ok(ReadResult)` as complete.
- RetainPayload and local RetainWire keep one shared Arc payload with row ranges; local DAT has no fabricated network frame. Omit keeps only required raw bits/header/ranges. Stage B owns retaining actual archive/wire sources. `ArchiveProvenance.entry_name` keeps original bytes so lossy-decoded names do not become identity.
- DAT CPU callbacks run before parsing, at each index, during retained payload copying (64 KiB units), while attaching shared row payloads, and before return. List callbacks run around parsing and each row. Local file loading reuses T21 metadata checks, bounded chunk reads, growth probe and cancellation, without network operations or implicit eight-second deadlines.

## List and Python compatibility seam

List parser uses UTF-8 replacement, Python-style source whitespace trim, newline split, comma split, fewer-than-three/noninteger skip, and ignores extra columns. Filename/hash fields preserve internal/field whitespace after line trimming. Sizes accept signed decimal and between-digit underscores. Negative nonzero or >u64 yields filesize=None plus a per-row diagnostic; strict mode rejects. Negative zero is zero. Both raw size text and unverified hash are retained; no MD5/authenticity or actual-file-size verification is claimed.

The doc-hidden `python_legacy` submodule is a concrete T23 handoff, not a new selector in Rust options:

- `parse_dat(data, nonzero_override)` shares the same decoding algorithm but does not enforce new direct-DAT byte cap and does not introduce Rust's metadata/range overlap rule. It keeps existing Python per-row/total field protections and repeated-valid-offset rejection, old short/truncated/invalid index omission, and raw integer dates. Python adapter projects rows to its original result shape and ignores enhanced Rust diagnostics.
- `first_filename_date` returns the first unvalidated eight-digit filename text as integer; nonzero override takes precedence, zero uses header. New Rust validates filename dates separately.
- `file_list_candidates` retains original three text columns so the Python binding can call Python `int(text)` and catch ValueError, preserving arbitrary-precision negatives and Unicode decimal digits. The convenience `python_legacy::parse_file_list` returns normalized ASCII decimal strings and is not a substitute for Python `int` for Unicode decimal input.
- T23 must use the legacy candidates plus Python int for exact list behavior; preserve original Python exception/result projection and independent request deadlines. No Python source or bridge has yet been migrated.
- Legacy local direct-DAT reading still needs a deliberately uncapped byte loader in the compatibility bridge; do not call new public `read_dat_file` (which correctly enforces new Rust limits).
- Legacy ZIP still selects the first distinct `.dat` in archive order and retains selected-DAT 256 MiB and entries64 protections. New Rust RequireSingle or all-member expansion/validation rules must not be retrofitted.

## Exact Stage B archive and service seams

`archive.rs` remains unchanged. T17 `decode_selected(payload, ArchiveLimits, select(&[u8]), boundary)` already uses same index for validation and reading, bounded metadata, CRC and actual-byte checks. Its returned `ArchiveMember { name, contents }` lacks entry index and declared/actual sizes. `ArchiveLimits::financial()` is a private fixed preset with no lower caller-limit constructor. Required narrow adaptation:

1. Shared archive metadata must retain `{name: Vec<u8>, index: usize, declared_size: u64, actual_size: u64, is_directory: bool}` on the same checked entry used by `by_index`. Do not locate an entry by name after validating another entry.
2. Add checked per-use lower overrides for compressed bytes, entries and selected-DAT expansion. Keep new Rust aggregate expanded total <=256 MiB across all members and selected DAT independently <=min(256 MiB, caller max_dat_bytes). Both declared and streaming actual counts enforce those bounds before allocation/growth. Existing fixed total256 is consistent with controller ruling but smaller caller limits are not yet expressible.
3. Shared selection must inspect all candidate metadata before retaining a DAT: ignore directories; case-sensitive `.dat`; reject duplicate DAT names; RequireSingle rejects multiple distinct DAT names with candidates; Named exact match; missing -> MissingData. Selection belongs to shared integration, not a second ZIP parser in this module.
4. Preserve separate Python legacy policy: first distinct DAT, current duplicate DAT/selected encryption/CRC checks, no new all-member expanded budget or unselected-member rejection. The current decoder validates/rejects all member duplicates/encryption and inflates all entries, so it cannot silently become the legacy route without explicit policy adaptation. Reuse one decoder implementation with policy differences, not copied ZIP code.
5. Shared adapter constructs `ArchiveProvenance::new(...)`, attaches via `FinancialDatResult::with_archive`, and calls `parse_dat_for_archive_with_boundary` with inherited HQ operation. Detailed result completion must be Complete for a complete-records API.
6. Register `pub mod financial;` in `offline/mod.rs`; re-export specified offline models/options (and decide additive detailed types); add root `financial` module under offline feature and owned `TdxClient::financial()` getter. Do not require F10, HTTP, EX or MAC.
7. Add `financial::read(path, options)` with `.zip` routing through shared decoder, lowercase extension selection matching local source, and default header date even for local ZIP filenames; `.dat`/other direct input uses `read_dat_file(...).into_records()`. The current `read_dat_file` is intentionally explicitly DAT-only.
8. HQ `download_archive(filename, download, operation)` propagates max_bytes to 0x06B9 source, validates finite chunk (default30000, max60000), max_chunks4096, u32 wire offset and checked offset progress. max_bytes0 returns LimitReached without I/O after validation. Nonzero start offset is range-only. Same pin and operation budget span every chunk and cleanup. No `LimitReached` bytes enter archive decoding.
9. `download_records(query, operation)` inherits one operation through list, source download, shared ZIP and pure DAT. It checks Complete and origin, cancellation and deadline remain terminal, no fallback to empty result. `list_archive_files` uses `tdxfin/gpcw.txt`. `open_calc_client` is lazy; build/getter do not connect or probe.

## Authored cases, self-review and evidence limits

`tests/financial.rs` contains synthetic source cases for: exact NaN/sign/subnormal bits; raw policies/header fields/market255/ASCII replacement/ranges; empty versus short header versus report_size0; hostile huge headers and 4096 field boundary; direct-byte exact/smaller cap; small total budget and full default total-field boundary; truncated indexes and strict rejection; out-of-range offsets and diagnostics; duplicate/overlap and out-of-order ranges; metadata overlap legacy difference; omission counts; explicit/archive/header dates and invalid-date retention; Python first-invalid filename date; list replacement/negative/overflow/underscores/negative0/strict behavior; option validation; remote name length/selector checks; local DAT date/byte limit/I/O errors; cancellation before file open.

Additional `#[cfg(test)]` source cases in the module cover: inherited callback Timeout/Cancelled/Closed unchanged; archive provenance identity/size fields; legacy direct-DAT cap bypass with a lowered test cap. No input fixture requires a network source. ZIP CRC/encryption/duplicates/candidates/limits and HQ source cap/pin cleanup/lazy client cases remain Stage B, since their implementations and shared seams are not yet present.

Self-review read source against contracts and fixed: an initially invalid index-context string conversion; negative-zero size handling; delayed retained payload allocation until header/index/field checks finish; missing effective-legacy-date diagnostic; deadline/cancellation callbacks through DAT and list CPU loops; cancellation while assigning row payloads. Normal source reads and owned-path Git checkpointing only were used. Source files remain unregistered by design, so authored integration cases cannot yet be collected successfully. Unused private Stage B seam warnings are possible until integration.

Explicitly not run: tests or test collection, build/check/clippy/doctest, cargo metadata/resolution, dependency download, network, compile/AST/format/diff checks or equivalent validation. No claim about compilation, dependency API/MSRV/license, runtime behavior, passing cases or performance. Deferred command after all T01–T30 implementation and Stage B completion: `cargo test -p eltdx --features offline --test financial --locked`.
