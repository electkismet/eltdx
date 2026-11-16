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

## Seventh checkpoint — GBBQ, block formats and model properties

Base `dcf2f3f`. The sole GBBQ descending cipher now accepts a scalar/word adapter;
strict Rust uses native u32 and Python uses live Struct/key/mask arithmetic.
Python records remain sequential, so truncated input raises the original
Struct error before later records and invalid code retains UnicodeDecodeError.
GBBQ retains original model lookup and original7-byte code trimming semantics.

Block DAT layout/count/code slots, filename category, custom config layout,
safe-leaf rules and Unicode code-line traversal are shared. Python preserves
GBK/ASCII replacement, first-NUL config names, both-end DAT NUL stripping,
declared count above400, order/duplicates, exact Chinese errors and original
Path resolve/containment ordering. Empty custom leaves still skip on Python;
strict Rust still rejects. Custom buffer/layout/global-builtin branches remain
explicit adapters, not native-migration acceptance evidence. Custom read_text
objects retain their repeated strip calls; normal strings share the Rust loop.

Daily/EX date arithmetic and minute model keys now use shared ordered helpers.
Python dataclasses remain frozen/slots with original constructors/defaults/raw
comparison; arbitrary numeric/field objects are not narrowed. Rust MinuteKey
adds a matching field tuple view. No public Python model is replaced.

Sources added offline_gbbq.rs/offline_blocks.rs and offline-migration-index.md;
updated shared offline sources, binding registration, Python model/GBBQ/block
facades. Authored16 Python test functions (including parameterized cases) and
three Rust source cases, all unexecuted. The inverse-cipher fixture is separate
from the production decryptor. Source review only; no compilation or runtime
acceptance claim, no manifest status promotion.

Next: all157 F10 compatibility routes/fields/models and cross-source helpers,
remaining T23 evidence and adapter accounting, thenT24–T30. Counts remain21
source-complete/4partial/5unstarted; goal active without cap. No test, collection,
build, check, dependency/network, format/AST validation, agents or publication.

## Eighth checkpoint — F10 legacy response table foundation

Base `eb9487a`. Enabled the existing f10 feature for the Python binding without
dependency resolution. Added shared result-table/cell traversals, column name
selection and duplicate collision projection. Both strict Rust Response and
the Python binding use these algorithms. Strict allocation pre-reservation,
boundary checks and original typed validation remain in place.

Python parse_tqlex_response/_parse_result_set/_columns/_row_values/_column_name/
_row_dict now call f10_response.rs, preserving live parser/model/cell hooks,
Mapping/Sequence acceptance, falsey legacy empty containers, original objects,
first-table defaults, arbitrary-size ErrorCode conversion and exact errors.
Duplicate names preserve individual cells; projected suffix collisions overwrite
the old value without moving its first key position. Cell name descriptors are
read at each original expression. The HTTP context managers and per-wire timing
are still the original Python adapter; no native HTTP replacement yet.

Authored10 Python compatibility functions and one Rust shared callback case,
all unexecuted. Named endpoint parameter normalization/dispatch, fields,
remaining F10 model business properties and cross-source helpers are still
pending. Do not count this foundation as157-method migration or promote the
manifest. Review custom global enumerate/tuple replacement behavior when
finishing adapter accounting; current foundation covers actual parser/model
extension hooks, not every monkeypatched Python builtin.

Counts21/4/5; unlimited active goal. No tests, collection, builds, checks,
dependency/network execution, format/AST validation, agents or publication.

## Ninth checkpoint — F10 request preparation and identity adapters

Base `c3fbe18`. Shared lazy route selection is used by Rust F10Config and
Python request preparation; both prioritize ladder, then international, then
futures prefixes. Python retains live prefix/constants/config/Request/JSON/
urlencode hooks, original user-agent/header merge, UTF-8 compact body, custom
call/_post dispatch and unchanged HTTP context managers. Request construction
still precedes the HTTP exception block; wire timeout/HTTP/URL exception cause
mapping and per-request timing remain Python's original behavior.

Native identity adapters keep the actual Python str/Unicode operations,
BRK.B→B legacy behavior, HK arbitrary digit lengths/zfill, case-sensitive bond
number prefixes versus lowercase bond-market detection and live split_code.
Only the symbol-allowance loop is shared with strict Rust; strict Rust identity
types remain separately validated. Native section selection retains sorted
error choices, table mapping hooks and the original exception type.

Authored eight Python functions (several parameterized over identity/errors/
routes) and one Rust lazy routing case; all unexecuted. This finishes request
foundation only: named157 methods still need their actual shared route/argument
composition and source-specific differences. Fields/model properties, helpers,
manifest accounting and T24–T30 remain. Counts21/4/5, unlimited active goal;
no tests/build/check/dependency/network/format/AST execution, agents/publication.

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

## Tenth checkpoint — 92 fixed named endpoint templates

Base `c9d76ae`. Replaced fixed literal request assembly for92 named methods
with one SimpleEndpoint/ordered SimpleArgument definition per Rust query.
The Rust query's to_call and the Python f10_simple_endpoint dispatcher both
consume those definitions. No Python dependency is introduced into the SDK;
Python continues exposing its original method signatures and docstrings.

Scope: international fixed methods (41), all31 bond methods, ten fixed A-share
report methods, and ten industry/futures methods. Exact names are the entries
in crates/eltdx/src/f10/endpoints/simple_registry.rs. Complex/dynamic Entry,
indicator mapping, paginated, prerequisite and model/field compositions remain
outside this checkpoint and are NOT counted as migrated here.

The binding validates only methods that originally called _require_section,
using the same closed choice order and a real tuple literal equivalent. It
captures self.params before converting any argument, then traverses shared
slots in order. Live _code6/_us_code/_hk_code/_bond_code/_bond_sc overrides,
unchanged custom input objects, open section identity, arbitrary return objects
and original exceptions are retained. Industry/futures symbols remain raw
Python objects. Bond number and market conversion are distinct calls on the
original input; the query's checked Rust identity policy remains independent.

Authored8 additional Python contract functions (including20 representative
wire cases and2 raw-symbol cases) and3 Rust shared-template functions. Existing
independent route inventory remains the fixed-wire oracle. No tests, build,
collection, dependency/network execution, formatter, AST or diff check ran.
Source reads and ordinary Git diff only. No manifest promotion or acceptance
claim. Counts still21 source-complete/4partial/5unstarted; active unlimited goal.

Resume next: complex F10 named request policies and live selector dictionaries,
all remaining named methods/models/fields, cross-source helpers and migration
evidence; then T24–T30 before the unified verification gate. Checkpoint58 is
complete as a source checkpoint, not complete T23.