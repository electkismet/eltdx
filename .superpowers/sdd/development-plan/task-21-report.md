# T21 implementation report

Status: all 35 nonfinancial offline functions and their local models are authored. Four requested integration test sources and deterministic internal writer-failure cases are authored. No compilation or validation result is claimed. T22 retains the three financial functions and its financial models.

Base supplied by controller: `230e566`. Recovery source checkpoint: `f921d81` (`feat(offline): author isolated local formats and source algorithms`). The final source/data/test/report checkpoint is the commit containing this report and is returned to the controller separately. No other agent's files were staged or committed. Git hooks were disabled for checkpoints. No push, tag, publication, or merge was performed.

## Authority and source mapping

Read `task-21-brief.md`, development-plan T21, extension-api-spec §§1/6, hq-api-spec §3, COMP-037–041, units/dates policy fixtures, all specified Python offline modules, and the existing Python GBBQ inverse fixture recipe. Each Rust function below maps the corresponding Python source algorithm; helpers centralize the repeated record I/O, filtering, key, and durability behavior.

| # | Python exported function | Rust function | Python source |
| --- | --- | --- | --- |
| 1 | detect_tdx_home | offline::paths::detect_tdx_home | offline/paths.py |
| 2 | resolve_vipdoc | offline::paths::resolve_vipdoc | offline/paths.py |
| 3 | market_to_exchange | offline::paths::market_to_exchange | offline/paths.py |
| 4 | read_daily_bar | offline::daily::read | offline/daily_bar.py |
| 5 | encode_daily_bar | offline::daily::encode | offline/daily_bar.py |
| 6 | find_daily_bar_file | offline::paths::find_daily | offline/daily_bar.py |
| 7 | detect_security_type | offline::daily::detect_security_type | offline/daily_bar.py |
| 8 | security_coefficients | offline::daily::security_coefficients | offline/daily_bar.py |
| 9 | write_daily_bar | offline::daily::write | offline/write_daily.py |
| 10 | append_daily_bar | offline::daily::append | offline/write_daily.py |
| 11 | get_last_bar_date | offline::daily::last_date | offline/write_daily.py |
| 12 | read_min_bar | offline::minute::read_lc | offline/min_bar.py |
| 13 | read_5min_bar | offline::minute::read_5 | offline/min_bar.py |
| 14 | encode_min_bar | offline::minute::encode_lc | offline/min_bar.py |
| 15 | encode_5min_bar | offline::minute::encode_5 | offline/min_bar.py |
| 16 | find_lc1_bar_file | offline::paths::find_lc1 | offline/min_bar.py |
| 17 | find_lc5_bar_file | offline::paths::find_lc5 | offline/min_bar.py |
| 18 | find_5min_bar_file | offline::paths::find_5 | offline/min_bar.py |
| 19 | write_min_bar | offline::minute::write_lc | offline/write_min_bar.py |
| 20 | append_min_bar | offline::minute::append_lc | offline/write_min_bar.py |
| 21 | write_5min_bar | offline::minute::write_5 | offline/write_min_bar.py |
| 22 | append_5min_bar | offline::minute::append_5 | offline/write_min_bar.py |
| 23 | get_last_min_bar_datetime | offline::minute::last_lc_time | offline/write_min_bar.py |
| 24 | get_last_5min_bar_datetime | offline::minute::last_5_time | offline/write_min_bar.py |
| 25 | read_ex_daily_bar | offline::ex_daily::read | offline/ex_daily_bar.py |
| 26 | encode_ex_daily_bar | offline::ex_daily::encode | offline/ex_daily_bar.py |
| 27 | write_ex_daily_bar | offline::ex_daily::write | offline/write_ex_daily.py |
| 28 | append_ex_daily_bar | offline::ex_daily::append | offline/write_ex_daily.py |
| 29 | get_last_ex_bar_date | offline::ex_daily::last_date | offline/write_ex_daily.py |
| 30 | read_block | offline::block::read | offline/block.py |
| 31 | read_customer_block | offline::block::read_custom | offline/block.py |
| 32 | read_industry_cfg | offline::industry::read | offline/industry.py |
| 33 | parse_tdxhy_cfg | offline::industry::parse | offline/industry.py |
| 34 | read_gbbq | offline::gbbq::read | offline/gbbq.py |
| 35 | ensure_aligned_for_append | offline::alignment::ensure_aligned | offline/alignment.py |

Required nonfinancial models are `DailyBar`, `MinBar`, `ExDailyBar`, `BlockInfo`, `CustomerBlock`, and `GbbqRecord`. Additional local value types are `LocalCode`, `Exchange`, `TimeOfDay`, `MinuteKey`, `RecordKey`, `WireFloat`, `Coefficients`, `SecurityType`, `CoefficientDecision`, `Evidence`, `IndustryCodes`, and `IndustryMap`. Options/results/models have private fields and read-only accessors; construction checks the relevant numeric/date/path boundaries. `TradingDate`, cancellation, errors, and raw policy reuse existing shared SDK types.

`ReadResult` consumes `crate::types::Diagnostic` directly. The constructor is the controller/T10-frozen `Diagnostic::new(source, stage, status, error_kind, message)`, with `EnrichmentStatus::Unavailable` for ignored tails, coefficient fallback, and skipped customer-block records. No competing global Diagnostic or ResultMeta was introduced.

## Layouts, units, and exact source values

| Format | Layout | Authority retained independently of RawPolicy |
| --- | --- | --- |
| Daily | 32 bytes, `<IIIIIfII>` | original OHLC/volume u32 slots and source coefficients, amount f32 bits, adjust_flag u32 |
| LC1/LC5 | 32 bytes, `<HHfffffII>` | four original f32 price bits, amount f32 bits, original volume u32, reserved u32 |
| .5 | 32 bytes, `<HHIIIIfII>` | four original u32 price slots, amount f32 bits, original volume u32, reserved u32 |
| Local EX day | 32 bytes, `<IffffIIf>` | all float32 bits, original amount u32 dual slot, volume u32, settlement bits |

`WireFloat::as_f32/as_f64` are views over authoritative bits; NaNs, infinities, and negative zero are not replaced by zero. EX `amount()` returns the original u32 and `hk_stock_amount()` reinterprets those same bits. Encoding never derives HK amount from a scalar cast. Decoded daily/minute integer source slots are used directly on same-format encoding, including minute volume. Optional raw 32-byte copies are present only under RetainPayload/RetainWire; mandatory source integers, f32 bits, flags, and reserved fields remain under Omit.

Daily coefficients exactly follow the source table:

| Security | Price | Volume |
| --- | --- | --- |
| SH_A | .01 | .01 |
| SH_B | .001 | .01 |
| SH_INDEX | .01 | 1 |
| SH_FUND | .001 | 1 |
| SH_BOND | .001 | 1 |
| SZ_A | .01 | .01 |
| SZ_B | .01 | .01 |
| SZ_INDEX | .01 | 1 |
| SZ_FUND | .001 | .01 |
| SZ_BOND | .001 | .01 |

Prefix classification preserves every source branch, including bonds, fund variants, and index prefixes. Known filename-based classification has Inferred evidence. Unknown uses .01/.01 with Unknown evidence, an explicit `is_fallback()` result, and a read diagnostic if no override is supplied. Custom coefficients must be finite and positive. New scalar integer encodings use `round_ties_even`, with finite/u32 bounds checks, matching Python `int(round(...))`. `.5` uses source multiplication by 100; daily uses division by its selected coefficient. Date parsing validates Gregorian dates, including leap years; packed dates must fit u16 and packed minutes must be below 1440. Minute keys require seconds zero.

LC1 lookup uses `minline`; LC5 and .5 use `fzline`. EX local file access does not depend on the network EX feature.

## Bounded I/O and append behavior

ReadOptions defaults: 256 MiB positive finite allocation-compatible cap, Omit, IgnoreWithDiagnostic, no cancellation token. Files are opened only by explicit calls. The loader checks file length and then reads bounded chunks with a one-byte overflow probe, so file growth is not silently truncated. Customer cfg and every referenced blk share one aggregate byte budget. Cancellation is checked during loading and at record/line boundaries. No network operation, async executor, background task, or eight-second operation budget is created.

Bar readers parse only complete records in file order. Ignored residual bytes are counted and diagnosed; Reject returns ResourceFormat. Last-key methods align downward to a complete record and return None for missing/directory/short paths, while existing-file metadata/open/read failures propagate. Normal read of a missing file is Io.

AppendOptions defaults: FormatDefault, create_parents=false, no cancellation token. The complete input batch is encoded and bounded before any filesystem mutation, including records later skipped. Alignment is checked even for an empty input batch. Unaligned targets are never truncated or repaired. An opened-file alignment recheck detects pre-open size changes but is not a locking protocol.

Filtering compares each input key only with the original disk last key; the threshold never advances within the input. No sorting or deduplication occurs. Accepted bytes preserve input order. `WriteReport.final_key` is the actual final accepted record, rather than a maximum. Empty or fully skipped writes perform no open/create/flush/sync and report FormatDefault to avoid inventing an achieved sync grade.

The write loop tracks every actual byte. Errors carry `bytes_written`, accepted complete-record count, and skipped count. A fault can leave a complete prefix plus part of a record; cancellation is checked between records and can leave a complete prefix. Neither the API nor its documentation claims rollback, atomic batches, cross-process locking, or concurrent writer safety. Parent directory creation requires the explicit option.

Daily FormatDefault performs flush then sync_all and reports SyncAll only after both succeed. Minute and EX FormatDefault use ordinary File close and report FormatDefault. Explicit Flush and SyncAll perform their named actions. A flush/sync error remains an error with bytes/source, never a success report claiming persistence. File-drop close is ordinary Rust file ownership, not a blanket storage durability guarantee.

## Block, industry, and GBBQ rules

Block parsing retains the 384-byte-header/u16-count/2813-byte-record layout. Declared stock count is retained even above 400; at most 400 seven-byte ASCII-with-replacement codes are read. Name decoding uses GBK replacement and source NUL stripping. The otherwise unknown record u16 is also retained. Category uses source filename priority gn, fg, dy, otherwise industry. A truncated declared record errors with its index; extra data after declared records is diagnosed.

Customer cfg uses complete 120-byte rows: 50 bytes GBK name and 70 bytes GBK leaf, each cut at the first NUL. Empty leaf, missing/non-file blk, and empty name preserve source skips with diagnostics. Nonempty leaf rejects dot, dot-dot, separators, and colon. Resolution follows symlinks, including dangling targets, and verifies containment before deciding whether to skip a missing file. The cfg itself is also contained. UTF-8 replacement blk text follows Python splitlines/whole-line strip and drops exactly one Unicode character for the market prefix; empty/one-character lines are skipped. No unrelated files are extracted or scanned. As with the source, callers must avoid concurrent directory/symlink replacement; no race-free cross-process sandbox claim is made.

Industry parsing uses GBK replacement and the source splitlines/whole-line strip semantics. It requires at least three columns and exactly six Unicode code characters, without digit checking or individual-field trimming. Six-character `abcdef`, unknown industry values, and empty optional SW are kept. SW is column six if present. A Vec stores first insertion order and a temporary HashMap finds later replacements without reordering. Empty bytes/file are empty maps; file and byte paths share one parser.

GBBQ uses the exact copied original key bytes, explicit wrapping u32 additions, descending source subkeys, three independent 8-byte blocks, and the final five clear bytes. Each model keeps category, market, all seven original code bytes through clear_bytes, raw date u32, and four original f32 bit patterns; optional encrypted row bytes follow RawPolicy. Values retain the source category-sensitive field names and acquire no invented universal cash unit. Header shorter than four bytes returns an empty result; insufficient declared rows produce ResourceFormat and the first incomplete record index. Bar-tail options do not weaken declared GBBQ count validation. Strict UTF-8 code errors are explicit ResourceFormat.

## Explicitly authorized codec source-data generation

The controller authorized exactly one source-data generation run to remove WHATWG/Python GBK mapping ambiguity. It used only the installed Python standard library and built-in `gbk` codec, without importing project modules, asserting results, comparing Rust/encoding_rs, invoking a test runner, compiling project code, parsing project ASTs, formatting, checking diffs, or making network calls.

- Generator: `crates/eltdx/src/offline/generate_gbk_pairs.py`.
- Generated source: `crates/eltdx/src/offline/gbk_pairs.rs`.
- Recipe: `python -I -S crates/eltdx/src/offline/generate_gbk_pairs.py`.
- Executable: `C:\Program Files\WindowsApps\OpenAI.CodexPrimaryRuntime.v26-1112-737-0_26.1112.737.0_x64__3k8sg7r9htsxt\dependencies\python\python.exe`.
- Python provenance from the generated file: `3.12.14 (main, Aug 25 2026, 14:01:42) [MSC v.1944 64 bit (AMD64)]`.
- Table enumeration: lead 0x81..0xfe and valid two-byte trail 0x40..0xfe excluding 0x7f; accepted strict `gbk` scalar mappings are stored, undefined mappings are zero sentinels.

The Rust runtime uses this static table, with one-byte error replacement/resume for a rejected lead, ASCII handling, and explicit standalone 0x80 rejection. It has no Python runtime dependency. This generation is source authoring, not validation; the report does not claim that no code of any kind executed. T21 no longer requires encoding_rs itself.

## Authored cases; execution remains deferred

- `offline_bars.rs`: independent four-format bytes, all raw policies, NaN/negative-zero/infinity bits, flags/reserved, original integer slots, HK dual view, all ten coefficient cases and unknown fallback, custom coefficient validation, half-even bounds, Gregorian leap rules, tails versus Reject, empty/nonempty unaligned appends and original byte equality, four last-key entry points, missing/directory/short paths, original-threshold reverse/duplicate/overlap order, actual final record, missing/empty destinations, finite byte caps, pre-cancel, full-batch failure before file creation, explicit parent creation, local code/market paths, and packed year/time bounds.
- `offline_blocks.rs`: source categories, GBK names, declared count above 400, unknown u16 retention, header/record truncation with index, custom cfg/UTF-8 replacement/line behavior/skips/tails, aggregate byte cap, malicious leaf names, pre-cancel, and existing/dangling symlink escape. Windows platforms lacking symlink privileges explicitly skip that platform-specific fixture.
- `offline_industry.rs`: byte/file parity, duplicate replacement and stable order, six-character nondigits, fewer columns and wrong lengths, unknown/empty values, no per-field trimming, GBK Chinese and invalid lead/standalone byte/undefined pair cases, source line separators, missing/empty/cap/cancel.
- `gbbq.rs`: independently specified clear records and an ascending-key/u64-modulo inverse, category/raw-date extremes, float32 special bits, original encrypted/clear bytes, header<4, count zero, incomplete count/index, u32::MAX declared count, invalid UTF-8 code, limits and pre-cancel. The inverse uses original key data but never calls production decode/encode to create expected clear values.
- `offline/bars.rs::authored_writer_cases`: deterministic injected short writes/partial failures, actual 45-byte prefix and one accepted record, cancellation after the first 32-byte record, default daily/minute durability call counts, explicit Flush/SyncAll, flush failure, and sync failure with actual bytes.

Deferred only, after ALL T01–T30 implementation code is complete:

```text
cargo test -p eltdx --features offline --test offline_bars --test offline_blocks --test offline_industry --test gbbq --locked
cargo test -p eltdx --features offline --lib offline::bars::authored_writer_cases --locked
```

No tests, collection, cargo build/check/clippy/doctest, network, project compile/AST, format/diff checks, or equivalent validation was executed. Ordinary source reading, source authoring, the explicitly authorized isolated codec DATA generator, git status, staging exact owned files, and checkpoints were the only work modes used. No compile or runtime success is asserted.

## Files owned by this task

```text
crates/eltdx/src/offline/mod.rs
crates/eltdx/src/offline/options.rs
crates/eltdx/src/offline/models.rs
crates/eltdx/src/offline/alignment.rs
crates/eltdx/src/offline/bars.rs
crates/eltdx/src/offline/paths.rs
crates/eltdx/src/offline/daily.rs
crates/eltdx/src/offline/minute.rs
crates/eltdx/src/offline/ex_daily.rs
crates/eltdx/src/offline/block.rs
crates/eltdx/src/offline/industry.rs
crates/eltdx/src/offline/gbbq.rs
crates/eltdx/src/offline/gbbq_keys.rs
crates/eltdx/src/offline/gbk_pairs.rs
crates/eltdx/src/offline/generate_gbk_pairs.py
crates/eltdx/tests/offline_bars.rs
crates/eltdx/tests/offline_blocks.rs
crates/eltdx/tests/offline_industry.rs
crates/eltdx/tests/gbbq.rs
.superpowers/sdd/development-plan/task-21-report.md
```

## Integration and self-review concerns

Controller registration checkpoint `7c09816` supplies `#[cfg(feature = "offline")] pub mod offline;` and its common encoding_rs dependency. T21 did not edit Cargo.toml, root lib, operation/session/metadata/types, Python files, or controller documents. T10 must provide/reexport Diagnostic and EnrichmentStatus with the frozen constructor/getters; T21 consumes that pending contract. Common encoding_rs remains useful to HQ but is not needed by this static GBK decoder.

T22 should add `offline::financial` registration/reexports and the three reserved functions/models. It can reuse crate-private `ReadResult::new`, `bars::{load,load_with_budget,ByteBudget}`, and the shared local cancelled/malformed/io_error/diagnostic/GBK helpers. No financial placeholder was registered.

T23 should bridge old Python write names to the same append/filter/encoding algorithms and preserve old Python defaults/return shapes/exceptions. Rust-specific byte caps and Gregorian/type validation must not silently become new Python compatibility restrictions; T23 must choose legacy boundary projections explicitly, reusing raw slots and shared local algorithms where applicable. No Python bridge migration is claimed by T21.

Source-only self-review covered original integer/f32 authority under Omit, original-disk threshold versus batch order, empty unaligned input, short/missing last keys, checked packing and wrap operations, post-write cancellation/error byte context, durability success/failure reporting, GBK one-byte resume, stable industry order, and symlink containment. It corrected minute source-volume retention, write error context before opening, and reverse subkey iteration without relying on a stepped inclusive iterator's ExactSize contract. Runtime/platform correctness and crate integration remain unverified by instruction. Filesystem symlink privileges/Windows path behavior, exceptional I/O behavior, and the pending T10 shared-type integration need the deferred validation phase. No broad investigation or unrelated source cleanup was performed.

## Fix round 1: reject empty customer-block leaf names

Controller review at `9734db3` identified that `read_custom` treated an empty `blocknew.cfg` leaf filename as a diagnostic skip, although the T21 brief requires leaf names to be nonempty. The reader now returns `ResourceFormat` with the offending `record_index` for an empty leaf. Missing `.blk` files and empty block names remain separate legacy-compatible skip cases with diagnostics.

Exact changed paths:

- `crates/eltdx/src/offline/block.rs`: reject empty leaf before path resolution or file lookup.
- `crates/eltdx/tests/offline_blocks.rs`: remove the empty-leaf row from the successful skip fixture, keep empty-name/missing-file skips, and add an authored `ResourceFormat` plus row-index assertion for an empty leaf.
- `.superpowers/sdd/development-plan/task-21-report.md`: record this fix round.

Source-only self-review confirmed the empty-leaf branch terminates before constructing a path, while the `!present || name.is_empty()` branch still reports and skips only the missing-file/empty-name cases. No tests, collection, build/check/clippy/doctest, network, compile/AST, formatting, diff checking, or equivalent validation was run for this round.
