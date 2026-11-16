# T23 F10/offline/financial compatibility implementation

## First checkpoint — shared financial list row parser

Previous source checkpoints: T20 `1722815`; T22 `6885e00` and completion
`df29d6b`. Read both implementation indexes and the task brief before recovery.

The native binding now enables the existing SDK `offline` feature. Added a
shared borrowed row iterator to offline/financial.rs, used by both the strict
list parser and Python legacy candidate extraction. New PyO3 financial_file_list
projects rows with the actual Python int callable supplied by the module.
This preserves arbitrary precision, negative sizes, runtime Unicode decimal
digits and the original rule of skipping only ValueError. Other errors escape
unchanged. Standard exact bytes use this entry; custom bytes/decode objects
retain the Python adapter. Empty input still returns before loading native.

Files: crates/eltdx/src/offline/financial.rs;
crates/eltdx-python/Cargo.toml and src/{lib,offline_financial}.rs;
src/eltdx/offline/history_financial.py;
tests/test_native_financial_contract.py (four authored source cases).

Cases cover Unicode/sign/arbitrary-size conversion, invalid integer skipping,
replacement/whitespace, custom decode, overridden int and exception behavior.
None has been collected, compiled or executed. No capability manifest status
is promoted based only on this source work.

Next: DAT shares the existing parser algorithm while preserving Python raw
date objects and exact old error messages; ZIP first-member/entry policy and
all remaining offline codecs, F10 methods, cross-source helpers and manifest
evidence remain required. Full T23 completion is not claimed. Counts21 source
complete/4 partial/5 unstarted; active unlimited goal, no execution checks,
agents or publication.

## Second checkpoint — shared DAT traversal and Python streaming projection

Base: `74ccae3`. Added `python_legacy::DatCallbacks`/`visit_dat` as a
doc-hidden object-language policy seam over the existing DAT core. Strict Rust
and legacy streaming use the same header/index traversal, accepted-offset set
and float decoder. Fields are decoded in bounded chunks; each Python row is
projected immediately, avoiding a second complete Rust table and input copy.

The Python exact-bytes route borrows immutable bytes. Header callback applies
the original `report_date == 0` comparison, preserving arbitrary raw objects,
negative/huge values and comparison exceptions. Dynamic module field/total
limits use Python rich comparisons at the original logical points. Error
formatting uses the live ResourceFormatError constructor and original Chinese
texts; captured Python exceptions are returned unchanged. Total limits are
checked before allocating each row, duplicate accepted offsets before bounds,
unique overlapping offsets remain allowed, incomplete/out-of-range records
retain legacy skipping. On 32-bit targets an offset sum outside address space
is skipped in legacy mode, matching Python's unbounded sum and length check.

Explicit custom bytes/buffer objects or replaced struct module/calcsize/unpack
use the retained original Python adapter. Its exit condition is a replacement
generic buffer/codec callback adapter validated with the full compatibility
suite; do not delete it before that evidence exists. The old body is not used
for ordinary exact-bytes parsing. ZIP selection and file I/O are still pending.

Changed production sources: crates/eltdx/src/offline/financial.rs;
crates/eltdx-python/src/{offline_financial,lib}.rs;
src/eltdx/offline/history_financial.py. Authored nine Python contract functions
(one parameterized) for raw dates, evaluation order, short headers, exact
errors, duplicates/invalid offsets, overlap/float values, struct/buffer hooks,
dynamic comparison exception identity and incomplete tables. Added three
Rust visitor functions for shared bits/order, early dynamic limit rejection
and independently selected legacy field/overlap policy.

Commands were source reads, ordinary Git status/diff, file edits and disabled-
hook checkpointing only. No tests, collection, build/check, install, dependency
resolution, network, AST, formatting or diff-whitespace checks ran. No manifest
status is promoted. Source completion remains21/partial4/unstarted5.

Resume: Python ZIP first-DAT/duplicate/selected-only policy; remaining offline
functions/models and all financial compositions; all157 F10 methods and cross-
source helpers; actual manifest evidence; T24–T30 before any execution gates.

## Third checkpoint — shared ZIP selection and financial compositions

Base `690a825`. Added a generic ordered archive-inventory selection traversal
used by strict Rust financial ZIP and the Python adapter. Strict Rust continues
whole-archive preflight/decode and requires a unique or explicitly named DAT.
Python preserves suffix-only first distinct DAT selection, Python duplicate
name count/hash/sort, selected-only encryption/declared/actual size checks and
reads the exact validated ZipInfo. Inventory iteration is interleaved with
filename access, so a custom iterator is not eagerly consumed before errors.
The original zipfile codec/context manager remains an explicit Python I/O
adapter; no new strict all-member, directory, ZIP64 or aggregate rule leaks in.

Financial list/download adapters now execute resource method lookup and
record/model loops in the native binding. Native post-download cap/date parsing
and local dispatcher use live module attributes. Python keeps only ZIP open/
close and BadZipFile exception-chain handling plus the lazy calc constructor.
All four network entrypoints preserve their documented callable adapters;
there is no extra listing request or total operation deadline for Python.
Downloaded cap-limited bytes still reach zipfile and raise its original error.
Local DAT filenames do not supply dates. Date matching uses live Python regex,
Path and int, including invalid first eight-digit dates and Unicode digits.
Model constructors are fetched at the original loop point, before fields
conversion. Exception formatting calls Python format rather than str for
custom object compatibility. Generic iterable unpacking consumes at most the
one extra element needed to detect an overlong row.

Sources: crates/eltdx/src/offline/financial.rs; crates/eltdx-python/src/
offline_financial.rs and lib.rs; src/eltdx/financial.py and
src/eltdx/offline/history_financial.py. Authored 20 further Python contract
functions (including parameterized ZIP guards) and one Rust selection traversal
function; no collection/compile/run has happened. Existing financial source
tests remain required for final regression. No manifest migration promotion.

Next: remaining offline codecs, append/durability/path/model functions and
full F10/native compatibility; cross-source helpers and capability evidence.
Counts remain21 source complete/4 partial/5 unstarted. Goal active without a
token cap. No execution verification, agents, push, tags or publication.

## Fourth checkpoint — offline paths, classification and industry parser

Base `f2aabd0`. Shared first-directory selection and market mapping now serve
the Rust filesystem API and Python object adapter. Explicit paths, environment
priority, platform candidate order, lazy early return and Python equality/
formatting are retained; no Python market value is narrowed to u8. Path
composition keeps the original resolve-before-market order and joins through
Python operators, including custom code formatting and permissive old values.
The `.lc1`/`.lc5`/`.5` folder differences remain explicit.

Daily security prefix classification now shares the original Rust table with
the Python Path/lower adapter. Live Python detector/coefficient mapping hooks
remain callable. Industry GBK decoding remains a Python codec adapter (custom
decode hooks survive), followed by the shared Rust splitline/whole-line trim,
six-Unicode-character code and field parser. Python dict insertion/replacement
keeps first key order and last duplicate value. Shared traversal checks the
boundary even on malformed lines; no direct-file read cap is added to Python.
Industry local open/UnicodeDecodeError wrapping remains Python I/O glue.

New binding source `crates/eltdx-python/src/offline_paths.rs`; modified shared
offline paths/daily/industry, lib registration, Python paths/daily/minute/
industry facades. New `tests/test_native_offline_contract.py` contains12 source
functions with parameterized prefix/layout cases; two Rust source functions
cover shared selection/classification and industry traversal/error behavior.
None collected, built or run. Remaining read/encode/append/alignment/GBBQ/block
and model paths remain pending; F10/helpers/manifest andT24–T30 are unchanged.
Counts21/4/5, goal active without token cap; no execution validation or agents.

## Fifth checkpoint — shared local bar decoding and encoding

Base `0ff04c4`. Added one raw32-byte record layout decoder and field-order
encoder used by strict Rust daily/minute/EX codecs and Python adapters. Rust
date/price validation remains after raw decode; Python retains permissive raw
date/time arithmetic, full source bytes, float32 views and ignored partial tails.
EX integer amount and its float view still share the same original bits.

New binding offline_bars.rs projects existing Python models with live coefficient,
date/time decoder, model and float hooks. Python still performs its original
Path/read_bytes and empty/short-return sequence. Explicit custom Struct/buffer
objects use the retained old decoder until generic adapter parity is verified.
No Python input byte cap is introduced. All four public encoders now use shared
field order and Python numeric/Struct conversion, keeping ties-to-even, division
by zero, arbitrary numeric hooks and struct.error rather than strict SDK errors.
The Python Struct object remains an intentional low-level codec adapter, not a
second business loop; common field/record logic is in Rust.

Changed shared offline bars/daily/minute/ex_daily, native registration plus
offline_bars/financial unpack helper, and Python daily/minute/EX facades. Twelve
additional Python source functions cover raw invalid dates/time, short files,
bit views, custom decoder/model/coefficient/float hooks, encoder rounding and
codec error behavior. One Rust function covers raw record/encoder ordering and
date decomposition. All unexecuted. Append, alignment, model properties and
GBBQ/block remain pending, followed by F10/helpers/manifest and T24–T30.
Counts21/4/5; active unlimited goal; no tests/build/check/install/network/agents.

## Sixth checkpoint — append selection, alignment and last-record adapters

Base `b28222c`. Shared constant-original-threshold filtering now serves strict
Rust append and Python row selection; input order and in-batch duplicates are
preserved. Python alignment uses original arbitrary-number comparisons/modulo
and exact error text. Shared boundary exposes the original modulo operation;
strict Rust keeps its validated NonZeroUsize and structured error projection.

New offline_write binding handles alignment, selection, encode batch, last
record positioning and decoding. Python retains file context managers, direct
write/flush/fsync calls and their custom hooks. Daily/EX encode before opening;
minute opens before encoding. No parent creation, extra sync or revalidation is
introduced in legacy mode. Last-record lookup aligns tails but keeps invalid
date/time values; custom Struct/decoder hooks remain live. Existing original
Path conversion still precedes record-size lookup during append preparation.

Nine further Python source functions cover original-last order/duplicates,
alignment-before-empty filtering, custom encoder/fsync and dynamic lookup,
last raw date/time, exact errors, open-before-encode differences and fully
filtered no-write. One Rust selection case authored. All unexecuted.
Remaining T23 work: offline models, GBBQ/block, full F10/native compatibility,
cross-source helpers and manifest evidence; then T24–T30 before validation.
Counts21/4/5, unlimited active goal, no tests/build/check/network or agents.
