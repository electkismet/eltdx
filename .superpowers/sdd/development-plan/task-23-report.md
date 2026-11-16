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
## Eleventh checkpoint — mapped selectors and US page branches

Base `437dfed` (stage58). Extended the shared wire arguments with mapped
selectors, numeric constants and named inputs. The mapped US/HK finance
queries and US capital/shareholders/market page queries now consume the same
request definitions from their strict Rust APIs and Python public methods.
This adds six named methods: total98/157 have shared request composition
source (95 fixed/mapped templates plus3 conditional page templates).

The three mapped selectors retain the live module dictionary and original
module-level tuple override. _require_section is captured before tuple(table)
as Python originally evaluated it; the mapping is looked up again after
validation and params lookup, before code conversion. Replacement tables and
arbitrary mapped objects survive unchanged. Strict Rust uses the closed map
values and its original validation policy.

US page branches keep original Python comparisons and lookup order. Capital
001 ignores page and sends numeric1; shareholders006 ignores both inputs and
sends numeric1/10; market003 ignores both and sends numeric0/0. Other accepted
branches preserve the caller's objects without numeric coercion. The original
section validation occurs before branch equality and params lookup. An equality
exception is propagated unchanged. Rust continues checked u32 inputs.

Authored5 Python contract functions (including6 page and3 mapping cases) and
one Rust function; prior source-only tests now cover the expanded argument enum.
No validation executed: no tests/collection/build/check/dependencies/network,
AST/format/diff-whitespace checks. Plain source reads/diff and hook-disabled
Git checkpoint only. Counts21/4/5, active unlimited goal; T23 remains partial.

Next: remaining59 F10 named method compositions (including dynamic industry/
futures, report tables/prerequisites, news/cache/valuation and typed result
collectors), F10 fields/models and cross-source helpers; migration manifest
accounting; all remaining T24–T30 sources before final unified validation.
## Twelfth checkpoint — named inputs and industry/futures selections

Base `789ffd1`. Added19 shared request compositions (117/157 total):
hk_pepb_band, hk_valuation, hk_analyst_forecast, allotment_details,
finance_report, finance_diagnosis, stock_score, governance, hy_stage_rise,
hy_capital_stats, qh_basis_spread, qh_warehouse_receipt, hy_summary,
hy_members_rank, qh_position_report, qh_variety_info, qh_index_info,
qh_index_valuation and qh_weights.

SimpleArgument now includes date inputs and deferred section comparisons.
Named-input definitions and sector_templates.rs supply one request layout to
both checked SDK queries and Python adapters. Dynamic string concatenation
stays a Python operator operation for legacy custom objects. Industry/futures
symbols and unconstrained arguments retain object identity; checked Rust
query setters keep the original validation and normalized date policy.

Sector adapters preserve dictionary lookup (including KeyError/unhashable
TypeError) for summary, variety and positions; industry rank preserves ordered
if-comparisons and exact custom ValueError. Position handling retains index
truthiness first, section dictionary selection, BOTH date conversions, then
its separate tuple-membership/section comparisons, params lookup and request.
The weights date is converted before branching even when omitted from Params.
Dynamic index Entry concatenation happens after params lookup and before
argument conversion. HK analyst defaults retain date.today, lazy start/end
truthiness, previous-year replace and the original leap-day ValueError.

Added13 Python contract functions including13 named-input cases,14 sector
selection cases and10 position cases; added2 Rust source functions. These are
unexecuted source assertions, not acceptance evidence. No tests/collection/
build/check/dependency/network/AST/format/diff-whitespace verification ran.
No manifest status promotion. Counts21/4/5 remain; active goal has no quota.

Next40 named methods: report-table/prerequisite/market, news/cache/valuation,
result collectors and aliases; then fields/models/cross-source helpers and
manifest evidence. Complete all remaining T24–T30 implementation sources
before entering the unified execution gate. Stage60 source checkpoint only.
## Thirteenth checkpoint — news inputs and cache payloads

Base `f8fd116`. Eleven additional methods share Rust composition source:
topic_compare, company_news, northbound_holding, detail, market_news,
channel_news, cache_list, announcements, news, roadshows, news_channels.
The named-entry count is128/157. The cache layout/key slots and alias kinds
are defined once in request_shapes.rs; numeric/text news slots extend the
same ordered template used by checked Rust query types.

Legacy news resolves self.call before argument conversions and constructs
an explicit Params body. The live str callable is captured before int(tab)
or int(channel), and raw keyword/detail_type/section/filter objects remain
intact. Northbound converts page twice, in source order. Finance review
converts only code and page. Cache keeps real set membership, iterable
unpacking, late self.call lookup, live DEFAULT_QSID and sorted/channel map.
Literal str.format supplies f-string special-method lookup and rejects
non-string __format__ results without unsafe FFI or module format hooks.

An authoring replacement initially crossed the market_news/channel_news
method boundaries into theme_market; source comparison detected this and
restored theme_market before this checkpoint. The retained client diff
contains only the eleven named methods above.

Eleven Python contract functions and one Rust function were authored;
none executed. No compile/build/check/collection/dependency/network/AST/
format/diff-whitespace checks, no agents or publication. Counts21/4/5,
active unlimited goal. Remaining29 named methods plus F10 fields/models,
cross-source helpers and manifest; then T24–T30 and unified validation.
## Fourteenth checkpoint — eager report tables and prerequisites

Base `c75fb89`. Added shared report_templates.rs for shareholders,
capital_structure, business_report, industry_analysis, trading_report,
fund_report and market_report. Both strict query composition and the Python
bridge consume the same ordered entries/arguments/default slots. Migrated
these seven public methods plus business_composition and shareholder_change_plans:
137/157 named methods now have shared composition source,20 remain.

The Python bridge constructs every original table row before selecting it.
Repeated arg/date truth tests remain separate, including unused branches;
page and size conversion precedes date defaulting and selector checks.
Market aliases/codeless membership remain live. Capital detail stays outside
the eager table and validates section before date truthiness, then captures
params before calling the live _date_text. Fund report captures the live
_latest_report_period before self.params and rereads the live period map;
legacy empty/invalid prerequisite values are not subjected to strict Rust
metadata/date validation. Selected entry/params unpack before params lookup,
then star arguments expand afterwards. Public alias/business method overrides
and original objects are retained.

Eleven new Python contract functions (ten literal route cases in the first)
and one Rust nondefault wire function authored; none executed. Source review
compared retained old report tables to new shared definitions; no build,
check, test, collection, dependency/network, AST/format/diff-whitespace runs.
No manifest promotion.21 source-complete/4partial/5unstarted, active unlimited.
Next20 named F10 methods/collectors, fields/models/cross-source helpers and
manifest, then allT24–T30 sources before unified verification.
## Fifteenth checkpoint — object requests, digest and topic composition

Base `a3bee51` (stage62). Added shared object field definitions for theme,
board quotes, valuation ranges/history/series, window ids and stage aliases.
Rust checked queries and Python bridges consume the same layouts; strict
Rust extra restrictions remain distinct from Python dict.update overwrite.
Python retains two req_id conversions for themes, live split_code versus
_code6 behavior, explicit raw Page fields, percentile repeated conversions,
conditional default values, f-string formatting and object/list payloads.
Aliases call live theme_market/valuation with original objects and integer
request ids. Digest uses the shared eager report table including unused
page_size conversion; topic_compare_first retains first matching row value,
None failure, zero acceptance and live topic_ids/topic_compare/_first_value.

Ten Python contract functions (with six valuation branches and four stage
aliases) and one Rust object-family function authored; none executed. Static
source review repaired reserved pytest parameter naming before checkpoint.
No build/test/check/collection/dependency/network/AST/format/diff-whitespace
verification, agents or publication.

Inventory correction: prior totals counted only business named methods and
omitted the already migrated call/params foundation. A source-only pass over
all157 F10 public methods now finds151 with native entry source. Remaining6:
limit_up_down_list, limit_board_ladder, index_members, index_tracking_products,
board_members, ipo_calendar. This does not imply F10 models/fields/helpers
are migrated or any code accepted. Task counts remain21/4/5, active unlimited.
Next the six model/collector methods, then fields/models/helpers/manifest and
T24–T30; all execution stays deferred.
## Sixteenth checkpoint — collection loops and all157 entry bridges

Base `f3da958`. Added f10/collection.rs common pagination driver, typed row
field map and ladder parameter layout. Checked Rust market collectors now
use the common loop while retaining hard limits, total/record validation,
BusinessError and missing-progress errors. Their public row getters consume
the same field mapping as the Python row constructors.

Python index_members/index_tracking_products/board_members/ipo_calendar
now run through native code while retaining live market_report overrides,
row constructors, converters, dict/tuple/len/max/min/int hooks and raw object
values. Historical total lookup, soft max_pages (including zero still making
one request), nonzero business-code handling and per-page timing remain.
Board dedup is still by Python record_key, keeping original hash exceptions
and first-seen order. IPO retains its code-less named call.

Both ladder entries now call native assembly, preserving normalized-date
hook, body identity, live entry, detail then optional summary requests before
row parsing, original model/raw and aliases. A native iterator preserves
lazy row parsing if tuple is overridden, including StopIteration conversion.
All157 public F10 methods have native entry source. This is not T23 complete:
remaining scalar/date/row helpers, model algorithms, fields, cross-source
helpers and manifest accounting still require implementation.

Nine Python contract functions and one Rust pagination-driver function
were authored and remain unexecuted. Source reads/ordinary diffs only;
no tests/collection/build/check/dependency/network/AST/format/diff-whitespace
verification or agents/publication.21/4/5 task counts, active unlimited goal.
Next F10 field scope/duplicate/label logic, model business, native scalar/row
helpers and cross-source helper composition; then allT24–T30 sources.
## Seventeenth checkpoint — field dictionaries and news helpers

Base45bc64a. Added f10_fields.rs and f10_news.rs native adapters. Both Python
field modules now use the SDK ordered projection traversal; exact/raw lookup,
unknown fields, labels and notes retain order. Existing mutable dictionaries
stay Python data. Mainland only falls back on None; international falls back
on falsey labels but does not use wildcard for an exact empty dictionary.
Scope retains live Mapping/Sequence/str/len/index hooks, negative custom indices,
metadata truthiness, isolated surrogates, Unicode suffix digit/int behavior and
two int calls. Literal str.format retains type-level __format__ semantics.

News name lookup retains two int calls and eager default conversion even for a
known selector; registry.get is captured before arguments. News extraction
shares second-result index and ordered traversal; copies each row with the live
dict callable, catches only AttributeError while getting result_sets, and keeps
row value identity. Python public types/signatures remain unchanged.

Fourteen Python contract functions and one Rust projection function authored,
all unexecuted. Only ordinary source review/diffs and hooks-disabled checkpoint
allowed. No manifest status promoted. Still21 source complete/4 partial/5
unstarted. Next models, scalar/date/row, cross-source helper services and full
T23 manifest evidence; thenT24–T30 and unified validation after all sources.

## Eighteenth checkpoint — scalar, dates and ladder row helpers

Base05479e1. New f10_values.rs backs all remaining client scalar/date/row
helpers, first field selection, latest report period and legacy total scan.
Shared first_match explicitly distinguishes matching None from no match;
strict report prerequisite also uses it. Shared date-pair/default resolution,
ladder row field map and short-circuit positive board selection are used by
strict and legacy adapters. Python keeps str-int versus direct-int, arbitrary
precision, nonfinite values and old conversion-error boundaries; total still
scans all values and treats bool as int. Date parsing uses live date/datetime
hooks and keeps invalid-calendar ValueError cause. Model constructor, dict,
converters and raw value identity remain live. Default clock returns before
comparison, and row arguments preserve their old preparation/evaluation order.

Eight Python contract functions and one Rust function authored, not executed.
Stage65 authored-function count correction:13 Python functions, not14 (one
has two parameter cases). No execution checks/collection/build/dependency/
network/format/AST, agents or publication.21/4/5 unchanged; next model business
loops, cross-source helper services and manifest beforeT24–T30.

## Nineteenth checkpoint — F10 model business views

Base3fdebe4. f10_models.rs now provides existing Python model identity setup,
market inference, code/ETF convenience views, ordered unique board codes,
record-key, IPO raise amount, ladder status, counts, rates and overview.
Original Python dataclasses/slots/frozen construction and simple field aliases
remain DTO adapters. ModelValues/OverviewTopics retain lazy tuple overrides,
including generator StopIteration conversion. Live normalizer/market registry/
getters/sorted/dict/tuple/format hooks and source object identity are retained.
Python ladder statistics still merge ranges and allow promotion ratios>1;
strict Rust keeps per-day evidence and omits contradictory rates. Both use
shared classification/promotion traversals and common prefix/identity/topic
field constants. Board records retain all six identity fields.

Eight Python contract functions and one Rust source function authored; none
compiled/collected/run. No build/check/network/dependency/AST/format/diff-check,
agents or publication. T23 remains open for cross-source helper services,
native migration and manifest/consumer evidence; thenT24–T30 source work.
Counts21/4/5 and active unlimited goal remain.

## Twentieth checkpoint — shared owner and F10 collection helpers

Base c3f683b. TdxClient now accepts an optional F10 builder and exposes a lazy
F10 domain sharing the HQ SessionOwner. Closing either domain closes the same
owner; standalone F10Client builders remain independent. The common owner's
close/cleanup configuration governs composed clients. No builder or accessor
starts a network worker. HelperService now offers index_members, index_etfs,
board_members and ipo_calendar through in-context F10 collection seams, with
one admission/deadline/work budget and no HQ fallback. Query bounds, typed
rows, response metadata and business error completion remain unchanged.

Python's four public collection helpers and private cache dispatcher now use
native adapters. Existing _index_cached override/getter lookup order, normalized
cache keys with original source argument, static IPO key, None-as-miss, refresh
failure preservation and object identity remain intact. Native builtin lookup
checks module globals then __builtins__; ordinary str no longer requires a
module-global binding. Six Python contract functions (including parameterized
cases) and four Rust public helper functions authored; none executed.

T23 still needs stock_topics/topic_stocks/theme_strength_rank, their private
projection/resolution functions, full migration/consumer evidence, then all
T24–T30 source work. Counts remain21 source complete/4 partial/5 unstarted.
Goal active with no token budget. No tests/collection/build/check/dependency/
network/AST/format/diff-check, agents, push, tags or publication.

## Twenty-first checkpoint — stock topics and topic constituents

Base52355bb. Added feature-gated StockTopicsQuery/TopicStocksQuery and owned
StockTopic/StockTopics/TopicStock/TopicStockTable types, exported at the SDK
root. The two-source topic composition shares one F10 operation, byte cap and
input-record budget; both sources are required. First discovery order is
retained, later rows overwrite per-source raw and hot fields even with null.
Business response codes remain available through responses()/ok(); required
source transport/decode/owner errors never become successful empty data.
Raw dates/category values borrow the retained original row rather than cloning
whole arbitrary JSON trees. TopicCompare explicit-id queries use one source;
named/explicit-first queries resolve id before name and preserve physical
source responses, metadata and original comparison-row order. Resolution of
business-error prerequisites fails with MissingData rather than selecting an
unproven topic. Invalid identities remain raw and add diagnostics. All new
methods exist only with the f10 feature; HQ-only helpers remain unchanged.

Python stock_topics/topic_stocks, resolver and topic/scalar/row projections
now delegate to native. The SDK and binding share source traversal, ordered
field definitions and selector priority. Python retains eager materialization
of both sources, live normalizer/converter/model/helper overrides, first
present None, source union/raw copies, arbitrary precision/nonfinite scalar
values, original f-string object semantics and standard frozen dataclasses.
Lazy native iterators retain tuple override consumption and convert model
StopIteration to RuntimeError with cause. Converter lookup occurs before the
live _pick callback, matching Python expression evaluation order. Ten Python
and five Rust public source-test functions authored; none run or collected.

T23 still needs theme_strength_rank/native aggregation, full manifest and
installed-consumer evidence. T24–T30 remain afterwards.21 complete-source/
4 partial/5 unstarted, goalactive without token budget. All build/test/check/
dependency/network/AST/format/diff-check execution remains deferred.

## Twenty-second checkpoint — complete-ladder theme strength composition

Basefa891d0. Added feature-gated ThemeStrengthQuery/Row/Table/TopicSource and
HelperService::theme_strength_rank. Factored a shortline ladder seam so the
parent context owns admission, total deadline, work counters and metadata for
HQ plus per-security F10 topics. A supplied ladder's count is discarded;
all topic observations precede stable descending count/maximum-level/streak/
seal ordering and final top-N. Finite nonnegative confirmed-CNY seal values
and bounded groups/input records are explicit strict Rust requirements.
Optional source outages remain visible per security through source results,
errors, missing_ids and metadata diagnostics. A business error retains its
response; a transport/decode failure retains the root error, with no invented
partial source result. Limit exhaustion, cancellation, closure, internal
errors and expired parent context terminate the composition.

Python theme_strength_rank now delegates native with a shared ordered update
skeleton and rank-field definition. Existing stock_theme_strength_rank remains
a live forwarding alias. Python preserves full ladder input, optional fetch
Exception scope (iteration errors and BaseException still propagate), duplicate
topic observations, original numeric/nonfinite arithmetic, first maximum
leader, six-decimal output rounding, count validation after source work,
dynamic hooks and lazy tuple/model construction including StopIteration cause.

Eight Python contract functions, four private Rust functions and two public
Rust functions authored, none compiled/collected/run. Source review corrected
the private optional-error case: it now cancels a live token, drops the active
context and only then closes the client, avoiding same-thread close waiting
for its own admission. Shared close and zero-deadline entry are covered by
separate authored public cases. This is source review, not execution evidence.

All T23 cross-source entry implementation sources now exist; T23 remains open
for the full migration manifest, independent F10/offline/financial/helper
consumer source coverage and documentation handoff. Counts remain21 source
complete/4 partial/5 unstarted; no execution validation until allT01–T30 source
work completes. Goal active without token budget; no agents or publication.
