# Offline shared implementation index

This is a source implementation map for T23. Every referenced test remains
unexecuted until the full T01–T30 source gate. It does not promote a capability
to accepted/migrated status or prove that the extension compiles.

## Public export map

The public Python package retains its functions, signatures, dataclasses and
return containers. Rust callers use `eltdx::offline` with the `offline` feature;
they do not load Python. The Python distribution supplies its native extension
through the existing wheel installation path.

| Python exports | Shared Rust source | Binding / retained adapter |
| --- | --- | --- |
| `detect_tdx_home`, `resolve_vipdoc`, `market_to_exchange` | `offline/paths.rs` | `offline_paths.rs`: live Python Path/environment/platform hooks and arbitrary equality |
| `find_daily_bar_file`, `find_lc1_bar_file`, `find_lc5_bar_file`, `find_5min_bar_file` | `offline/paths.rs` | `offline_paths.rs`: original folder/name construction and Path instances |
| `detect_security_type`, `security_coefficients` | `offline/daily.rs` | `offline_paths.rs`: live detector and coefficient dict |
| `read_daily_bar`, `read_min_bar`, `read_5min_bar`, `read_ex_daily_bar` | `offline/bars.rs` and respective typed readers | `offline_bars.rs`: standard bytes; custom buffer/Struct/layout branch retained in Python |
| `encode_daily_bar`, `encode_min_bar`, `encode_5min_bar`, `encode_ex_daily_bar` | `offline/bars.rs::python_legacy::encode_fields` | `offline_bars.rs`: Python scalar arithmetic, round/int, live Struct pack |
| `append_daily_bar`, `write_daily_bar`, `append_min_bar`, `write_min_bar`, `append_5min_bar`, `write_5min_bar`, `append_ex_daily_bar`, `write_ex_daily_bar` | `offline/bars.rs::python_legacy::select_newer` and typed append | `offline_write.rs`: Python context managers and writes; fixed original-last threshold; original encoding/open order |
| `get_last_bar_date`, `get_last_min_bar_datetime`, `get_last_5min_bar_datetime`, `get_last_ex_bar_date` | shared record/key layouts and alignment | `offline_write.rs`: tail positioning, original Struct and date/time callbacks |
| `ensure_aligned_for_append` | `offline/alignment.rs` | `offline_write.rs`: Python numeric modulo, exact error text |
| `read_industry_cfg`, `parse_tdxhy_cfg` | `offline/industry.rs` | `offline_paths.rs`: GBK codec and live file/parser callbacks |
| `read_block` | `offline/block.rs` DAT layout/category | `offline_blocks.rs`: exact bytes/default constants; custom buffer/layout/builtins branch retained |
| `read_customer_block` | `offline/block.rs` config layout/safe leaf/code lines | `offline_blocks.rs`: original Path checks, live safe-name/model hooks and Python codecs; custom text object iteration preserved |
| `read_gbbq` | `offline/gbbq.rs::python_legacy::decrypt_block` | `offline_gbbq.rs`: original sequential Struct reads, key/mask callbacks, strict Python UTF-8 and model construction |
| `parse_financial_file_list`, `parse_financial_dat`, `read_history_financial` | `offline/financial.rs` | `offline_financial.rs`: Python scalar/date/limit/model hooks and original zipfile strategy |
| `DailyBar`, `MinBar`, `ExDailyBar` | `offline/bars.rs` date/key helpers and Rust typed models | Python dataclasses stay frozen/slots with original defaults and raw compare/repr behavior; only business properties cross the native boundary |
| `BlockInfo`, `CustomerBlock`, `FinancialFileInfo`, `FinancialRecord`, `GbbqRecord` | corresponding Rust offline models | Python passive dataclasses remain the public objects |

## Policy differences that must survive

- Rust validates its value types and local byte budgets. Python keeps invalid
  raw dates/time values, arbitrary numeric objects, original direct-file read
  behavior and previously ignored partial tails.
- Python GBBQ walks records sequentially. A truncated word raises the original
  Struct error at its read point; an invalid code raises `UnicodeDecodeError`.
  Rust prevalidates the declared record area and maps invalid data to SDK errors.
- Python block DAT preserves the declared stock count even above400, consumes at
  most400 slots, trims NUL at both ends and ignores undeclared trailing bytes.
  Runtime category/model hooks remain callable. Rust retains reserved bits and
  trailing-byte diagnostics.
- Python custom blocks skip empty leaf names. Rust rejects them. Both apply the
  same lexical safe-leaf rule, while each retains its filesystem resolver and
  error model. Python resolves the file and root before file/name skip checks;
  missing files and empty names do not bypass containment checks.
- Custom block lines remove exactly one Unicode character after source trim.
  Duplicate rows/codes and source ordering survive. No numeric market-prefix
  validation or deduplication is introduced.
- Python appends keep the original-last threshold throughout the input batch,
  including duplicate/newer out-of-order rows. Daily writes flush+fsync;
  minute/EX close normally. No implicit parent directory creation is added.
- Python financial ZIP uses the first distinct DAT and its exact ZipInfo;
  strict Rust archive-wide checks do not run on the Python compatibility path.

## Explicit remaining adapters and exit conditions

The custom byte-buffer/Struct/layout branches in bar/block readers remain
intentional object adapters. They are not evidence of complete business-loop
migration for those overridden objects. Remove a branch only after a native
callback adapter preserves its read/evaluation/exception order and the matching
custom-object differential cases pass in the final validation phase.

Python owns local file context managers and OS durability hooks. These are the
extension surface for existing Path/Struct/codec/custom parser callers and are
not to be replaced with strict SDK filesystem behavior. Passive Python models
are likewise intentional public DTOs, not a requirement to replace constructors
with Rust object classes.

## Authored validation sources

- `tests/test_native_offline_contract.py`: native compatibility cases for paths,
  industry, bars, append, model properties, GBBQ and blocks. Includes an
  independently written GBBQ inverse cipher, malformed codes, truncation,
  custom hooks and path containment ordering.
- `tests/test_offline.py`: existing public offline regression suite, unchanged.
- `crates/eltdx/tests/offline_bars.rs`, `offline_industry.rs`, `offline_blocks.rs`,
  `gbbq.rs`, `financial.rs`: typed Rust behavior and shared policy/callback cases.

Full F10 migration, cross-source helpers and final capability manifest evidence
remain T23 work. Compile/install/consumer/platform verification remains deferred.
