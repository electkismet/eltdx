# T27 implementation report
## Stage 12 — all MAC response parsers and private helpers (2026-11-17, stage99)

Base08b9af5. Authored mac_parse.rs with all18 response policies, unusual
text decoding and safe numeric conversion helpers. Existing Python methods
now delegate; the numeric except blocks remain explicit Python object/active
exception adapters. No compile, test, import, collection, dependency, network,
AST, format or diff validation was executed.

Preserve quote20B bitmap/all-bit row width/complete prefixes versus members'
short-value partial row; unknown slots still occupy space, live postprocess/
cast, quote ignore/all-NUL versus members replace/right-NUL decode. Bars keep
clamped counts, date-range skipping, lazy period comparisons including seconds,
raw numeric slots and model-before-datetime capture. Single/multi-day charts
keep modulo24 minute clocks,18B/14B ticks, source tail slots, leading-date/count/
page clamping, short oldest day followed by complete days, zero-count behavior,
live size/format/date/model hooks. Strict Rust validation remains separate.

Transactions remain strict-per-record, auction keeps complete-prefix recovery;
seconds and signed unmatched are preserved. Info retains96/148 offsets and
HHMMSS datetime. Boards retain count//2 and all paired160B columns. Belong JSON
retains9/13 rows, replace decode and truthiness, capital strict GBK/two rows/
repeated live lengths and original net column order. Numeric helpers preserve
cast/float/int capture and swallow only original ValueError/TypeError.

Unusual retains all type/format/overlapping0x14 fields and two DTO passes with
names starting at declared count*32. Server short87B default, unvalidated date/
session text and original optional-param guard remain. Offsets stay big/little
endian, sampling strict per price, goods assigns total before record failure,
file signed flag/ASCII replacement and download slice type are retained.
Shared layout registers response formats and strict transaction/board/goods/
auction row strides. No production login or association evidence inferred.

Added tests/test_native_mac_parse_contract.py with28 public functions,
5 parametrized. Eleven new extension contract files total240 public Python
function sources, all unexecuted. Counts25 source-complete/2 partial(T19/T27)/
3 unstarted(T28-T30), unchanged. Next transport/owner/generation/config/host/
probe/management/models/export/manifest and nonempty installed-wheel consumer;
then T28-T30/T19 source closure followed by unified verification and repairs.
Goal active unlimited; no agents/publication/controller-state edits.

## Stage 11 — all MAC constructors and request builders (2026-11-17, stage98)

Base5f24441. Registered mac_commands.rs and its two native entry functions.
All17 argument-bearing constructors and all18 request builders in the existing
MAC command classes now delegate. mac::layout shares request formats, header
constants, bars flags, JSON tags and server control bytes with strict codecs;
Python retains its public class identities, signatures/defaults and DTOs.

Keep arbitrary Python values and deferred struct/GBK errors, original attribute
assignment/capture order, bars keyword-only times, no strict quote page cap,
None-only quote field default, stocks/list identity, false exclude-list behavior,
chained days validation before assignments, min(count,600) without lower bound,
and goods count upper bound only. Date constructors retain numeric year/month/
day hooks with no new calendar validation. Encoding/int/pack/bytes/build hooks
stay live in source order; members16B request bitmap is separate from20B response.

Members sum keeps the original lazy generator protocol and per-item live int,
including PEP479 RuntimeError/cause. Private Python identity-generator shell is
an object adapter, not a business loop. Quote row unpack errors remain Python
ValueError. JSON8B GBK truncation/head_flag, sampling and filename padding with
two live width reads, file offset/index/size and server bytes.fromhex behavior
remain. Strict input limits/evidence are unchanged.

Added tests/test_native_mac_commands_contract.py with24 public functions,
4 parametrized. Ten new extension contract files total212 public Python function
sources; none collected/compiled/executed. No tests/build/check/network/dependency/
AST/format/diff validation executed. This recovery commit preserves sources only.

Counts25 source-complete/2 partial(T19/T27)/3 unstarted(T28-T30), unchanged.
Next full MAC response parsers/private helpers, then transport/owner/generation/
config/host/probe/export/manifest/installed-wheel consumer. Then T28-T30/T19
source closure and unified verification/repairs with real production login and
optional-boundary evidence. Goal active unlimited, no agents/publication or
controller-state edits.


## Stage 10 — MAC date/price rules and EX/MAC framing (2026-11-17, stage97)

Basee9fbac1. Authored extension_rules.rs/extension_frame.rs; MAC bar-datetime
and GBK helpers, all3 price-rule functions, MAC request/header/body framing,
EX header/body/encode helpers and common limited-zlib policy now native-backed.
Seconds projection and limit-price factor/multiply primitives are shared with
strict MAC clock and approved Rust price arithmetic. The strict approved
profile registry remains empty; Python compatibility rules do not create
approved dated exchange evidence.

Keep MAC seconds/minute/second, daily lazy clock, original date errors/live
datetime hooks and arbitrary numeric components. GBK ignore/all-NUL removal
and custom decode/replace objects remain. Price policy retains upper-name
before index, live market/index/window/CHINEXT/round hooks, code prefixes,
registration-board priority beforeST, positive chained listed-days comparison,
source epsilon and Python round behavior, and original nonfinite policy.

Frames retain10/16-byte layouts, live struct/constants/two len calls, original
unknown magic acceptance and positionalMAC versus keywordEX DTO construction.
EX header length guard remains outside try and model construction outside
struct-error wrapping. Body checks preserve raw identity, header/property/len
rereads and original message/format/cause/context. Explicit Python except and
custom decompress call shells remain object adapters. Limited zlib calls the
standard/custom Python decompressor adapter from native policy, with original
max argument, tail/unused/eof short circuit, no flush/new cap, original errors
and returned object identity; independent Rust still uses its native codec.
No assertion here establishes response association or production login.

Added tests/test_native_extension_rules_frame_contract.py with22 public
functions(6 parametrized;7 decorators) and2 Rust scalar source functions.
Nine new extension contract files total188 public Python functions. All remain
uncompiled/uncollected/unexecuted. No tests/build/check/network/dependencies/
AST/format/diff validation executed. Sources only.

Counts25 complete source/2 partial(T19/T27)/3 unstarted(T28-T30), unchanged.
Next full MAC command constructors/builders/response parsers/helpers, then
transport/owner/generation/config/host/probe/export/manifest/consumer migration.
Then T28-T30/T19 source closure, unified verification/repairs and real production
login/optional-boundary evidence. Goal active unlimited, no agents/publication/
controller-state edits. Recovery commit preserves source without hooks.


## Stage 9 — all EX response parsers and shared slot schemas (2026-11-17, stage96)

Base3f73981. Authored extension_parse.rs and registered three native entry
functions. All11 EX response methods now delegate, including today's/history
minute/transaction private record helpers and both public-callable quote-list
helpers. Shared ex::layout now owns quote/list field names/slots/types and
record formats; strict quote and list projections use the same field schemas.
Python keeps existing DTO/OrderedDict/struct/GBK/custom helper object adapters.

Preserve every legacy short-header result, directory complete-prefix handling,
zero-market-and-category padding, right-only versus both-side NUL trimming,
GBK replace versus single-quote UTF-8 replace and original raw slice/type.
Count keeps live guarded unpack and int. Today minute/transaction use self
helpers; history captures the live today class helper. Keep no clock/calendar
validation, signed interest, raw integer price, seconds>59 coerce0 and nature.
Ordinary bars call live datetime before numeric completeness, retain position
integer plus its same-slot float amount, and ignore the last float. Range
requires all32 bytes before overridden date/time hooks and keeps settlement
as amount. Quote takes only150 source bytes, all31 named numeric fields,
unknown slots and nonfinite values stay unchanged.

Quote-list keeps per-record category/hook lookup, 300-byte stride but140-byte
numeric sufficiency, both-side code trim, original OrderedDict order and
custom position in-place addition. The frozen futures source format returns35
values for34 assignment targets: retain ValueError on ordinary full records;
custom unpack that returns34 still projects original names. Independent Rust
keeps native strict decoding/opaque slot bytes. This is an explicit existing
Python defect, not repaired or hidden. Custom range/count/struct/model errors,
constructor-before-raw/call-before-argument captures and markets <=/None raw
semantics have source fixtures.

Added tests/test_native_extension_parse_contract.py with26 public functions
(5 parametrized), plus1 Rust schema function. Eight newly authored extension
contract files total166 Python public functions. None collected/compiled/run.
No tests/build/check/network/dependency/AST/format/diff validation executed.

Counts25 complete source/2 partial(T19/T27)/3 unstarted(T28-T30), unchanged.
Next MAC commands/parsers, frame/codec/price/date helpers, then complete
transport/owner/generation/config/host/probe/export/manifest/consumer migration.
Then T28-T30/T19 source closure and unified verification/repairs/real production
login and optional-boundary evidence. Goal active unlimited; no agents,
publication or controller-state edits. Recovery commit is source preservation.


## Stage 8 — EX wire requests and binary/date adapters (2026-11-17, stage95)

Basec48a967. Resumed with two unfinished source files: EX layout registry and
strict command references. Registered public ex::layout and authored
extension_commands.rs, sharing all11 literal headers with strict request
codecs. The11 EX command builders, their9 argument-bearing constructors,
range sequence/date/time helpers and MAC-EX login builder/parser now delegate.
This is source migration, not compilation or protocol acceptance.

Preserve code/category/page validation order, original ValueError constructor
and f-string capture, arbitrary Python values and UTF-8 encoding/struct's
9-byte truncation and original struct.error. Keep assignment order and object
identity, live bytes/bytearray/struct hooks and pack-before-attribute capture.
Range state stays class-level with original dynamic type/maximum/minimum,
1..255 wrap and overridden _next_seqid. Login retains module constants, two
live len calls and len(body)>=2 comparison object; the heuristic remains
legacy-only and is not production LoginReplyRules evidence.

Authored extension_binary.rs. Both EX/MAC guards, preparation and slices now
use native helpers; original struct buffer call and except/raise shell remain
explicit Python adapters so sys.exc_info, __context__, original __cause__,
exception matching/constructor/format hooks remain observable. Guard is outside
the try, captured before calcsize, with original two live lengths and max
capture; bytes is captured before end arithmetic and intrinsic slicing.
EX datetime's three functions now delegate, sharing YYYYMMDD, packed-date and
clock projection primitives with strict Rust while retaining arbitrary ints,
negative/custom categories, shift-vs-floor arithmetic, no Gregorian/clock
validation, live helper return identity, unpack arity and same pos+4 behavior.

Added tests/test_native_extension_commands_contract.py with22 public function
sources(7 parametrized); the header table has10 direct command rows plus the
separate range fixture. Covers constructors/wire hooks/sequence/state/login,
all binary guards/cause/context/ordering, date/clock/custom arithmetic and
arity/identity. Added2 Rust scalar fixture functions. Seven new extension
contract files now total140 Python public function sources. All sources remain
uncompiled/uncollected/unexecuted; no test/build/check/network/AST/format/diff
validation or dependency operation was performed. Source reading corrected a
new temporary loader reference to the established _native_abi.load_native.

Counts25 complete source/2 partial(T19/T27)/3 unstarted(T28-T30), unchanged.
Next full EX response parsers and MAC command/parser/codec/price helpers, then
transport/owner/generation/config/host/probe/export/manifest/installed-wheel
consumer. T27 is not complete. Then T28-T30/T19 source closure, followed by
unified verification/repairs and actual production login/optional-frame evidence.
Active unlimited goal; no agents, publication or controller-state edits.
Recovery commit preserves source only and disables commit hooks.

## Stage 7 — shared facade bar-time and YYYYMMDD helpers (2026-11-17, stage94)

Stage6 committedcbb78cd. Authored native bar_time.rs; all8 _bar_time.py
business functions and MacClient module _ymd_to_date now delegate. Public
tables/constants/typing remain Python object adapters. Strict EX/MAC date
decoding and legacy arbitrary-number date projection share ymd_parts_with,
preserving strict validation and Python floor/mod/constructor behavior.

Keep start identity/short circuit, validator-before-duration check for end,
the original final rich-comparison object, live validation tables/error
constructors and repr ordering. EX8 means3m, MAC8 means5m*times, HQ8 means
parameter; repeated live int conversion, parameter laziness and min1 remain.
Clock-field and wrapped-datetime alignment use modulo24 on the same date;
datetime alignment creates one real timedelta before iteration and can
cross dates. DTO/field/getattr/replace/timedelta hooks stay live at their
old capture points, retained object fields/seconds/microseconds remain, and
calendar exceptions are not translated. Original assertion is honored
when Python optimization is disabled, including a custom _needs_align.

Added tests/test_native_bar_time_contract.py with15 public source functions
(two parametrized) plus one shared Rust source function. Including the five
extension contract files, six newly authored files total118 Python public
functions. None compiled/collected/executed. Cases cover date wrap vs
rollover, no-op identity, rich comparison, live constants/representations,
domain8 distinctions/int side effects, lookup order, empty-input timedelta,
per-row capture/hooks, calendar errors and arbitrary-size/custom dates.

Counts remain25 complete source/2 partial(T19/T27)/3 unstarted(T28-T30).
Next full EX/MAC command/parser/helpers and frame/transport/owner/generation/
config migration, export/manifest/adapter evidence and installed-wheel
extension consumer. Then T28-T30 and T19 source closure before unified
execution/repairs/production protocol evidence. Unlimited active goal;
no verification commands, agents, publication or controller edits.

## Stage 6 — direct entries, quote composition and HK routing (2026-11-17, stage93)

Previous turn was concrete progress:93be13d/1af9450 changed authoritative
implementation and recovery files. Base for this stage1af9450. Authored
extension_query.rs with explicit MAC/EX/MAC-EX command dispatch, native
MAC-EX quote-list composition/transaction routing, and all six synchronous
HK helpers. The11 EX business methods,14 remaining simple MAC entries,
five MAC-EX direct entries/info and quote/transaction compositions now use
native entry logic. Existing public signatures, aliases and DTO/raw objects
remain. This is not full command/parser/transport migration acceptance.

Preserved capture of execute and command before conversion/constructor
side effects; MAC chart live date hooks and unusual truthiness-or600;
MAC quotes still pass None to the original command, while MAC-EX selects
its own7 fields before execute lookup. EX remains single-page/range;
bar alignment follows fetching and range validation precedes I/O. MAC-EX
bars keep keyword request layout. Quote-list uses min(count,80), extracts
all labels before its second filter pass, keeps duplicates and deliberately
ignores the old reserved sort parameters.

HK stock membership stays the live Python set/hook, with only6 original
markets; common/futures stay MAC. Date/command/time/model hooks remain live.
Strict TransactionPrice, routing::hk_price_from_wire and legacy mapping now
share hk_price_with while retaining source u32/divisor, second precision,
nature/volume and absent trade_count0. Python HK paging keeps actual wire
offset/overdelivery/duplicates, per-page live cap/len, no32-request limit,
date conversion even for count<=0, lazy live mapper and StopIteration cause.
The all endpoint uses original live range(50) and returns its accumulated
prefix at that bound without strict LimitExceeded. Transport errors retain
identity. A tiny identity generator shell preserves the original generator
object protocol without retaining a Python business loop.

Added tests/test_native_extension_query_contract.py with22 public function
sources(three parametrized), including23 direct-entry table cases and all
HK stock/futures decisions; added1 Rust HK arithmetic function source.
Five new extension contract files total103 Python public function sources,
all uncollected/uncompiled/unrun. Removed a leftover Python loop line found
by ordinary source reading before checkpoint. This is not syntax/compiler
or runtime verification; those remain prohibited until the full source gate.

Counts25 source-complete/2 partial(T19/T27)/3 unstarted(T28-T30). Next
shared _bar_time/date helpers, full command/parser migration, transports/
owner/generation/config, full exports/manifest/adapter evidence and installed-
wheel extension consumer. Then T28-T30/T19 source closure and all unified
verification/repairs/production evidence. Unlimited active goal; no tests,
collection/build/check/dependencies/network/compiler/AST/format/diff checks,
agents, publication or controller edits. Checkpoint preserves source only.

## Stage 5 — MAC-EX catalog fallback and macro cache (2026-11-17, stage92)

Stage4 committed93be13d. Authored extension_catalog.rs with10 native
entries: _to_goods_items, _first_code, goods_count, goods_list,
_market_bounds, instruments, _macro_code, macro_indicators, macro_quotes
and macro_kline. Existing Python signatures/defaults and names now forward
to native business orchestration. Strict macro resolution and legacy exact
string resolution share macro_suffix; custom string subclasses retain
their live partition method. Missing separator vs empty suffix remains
distinct in the primitive and caller-selected in each policy.

Retain page-relative enumerate0, name=code-or-name, one Python float slot
or empty list on TypeError/ValueError, Python nonfinite floats, original
category/model hooks and no invented raw fields. _first_code strips the
label first and otherwise int-converts every numeric part before checking
the first value. Goods uses fixed600 requests, final prefix slicing and
fallback even for nonpositive count; no strict catalog snapshot cap/TTL/
whole-operation timeout was introduced. Count checks live command.total
and otherwise subtracts old EX binary bounds. Bounds cache retains original
tuple identity and old zero-total uncached behavior. Instruments keeps
market-relative start, global directory positions and original DTOs/raw.

Macro full codes bypass lookup; instance cache retains no TTL, source order
and duplicate-key last object, and returns the original first items object.
Bare suffix requires one match and preserves original ValueError/ProtocolError
messages and sorted ambiguity candidates. Quote resolution keeps input
occurrences/order, live hooks and fields, without an added batch limit.
Macro kline captures its original goods_kline method before code resolution.

Added tests/test_native_extension_catalog_contract.py with20 public test
function sources(one parametrized), plus one shared Rust suffix function.
Four new extension contract files contain81 Python public function sources;
none collected/compiled/executed. Source cases cover fallback shape/error/
nonfinite/model hooks, full numeric conversion, fixed paging/overdelivery/
count0 fallback, captured execute/errors, bounds cache/zero total, relative
instrument offsets/DTO raw, macro cache identity/duplicates/no TTL/isolation,
full/suffix/custom-partition/missing-separator decisions,90 quote occurrences
and captured macro kline callbacks. PyO3/runtime compatibility awaits final gate.

T27 still partial: remaining EX/MAC/MAC-EX direct commands, quote composition,
HK/common transactions and legacy50-page prefix, all commands/parsers,
transport/owner/generation/config, full manifest/export/adapter evidence and
installed-wheel extension consumer. Counts25 complete source/2 partial/3
unstarted. Continue T27, T28-T30 and T19 source closure before all unified
verification/fixes. Unlimited active goal; no execution checks, agents,
publication or controller edits. Checkpoint is source preservation only.

## Stage 4 — MAC board summary and both rankings (2026-11-17, stage91)

Stage3 committed997ae40. Authored extension_boards.rs and four native
entries, replacing Python get_board_summary, get_board_ranking, _board_row
and get_board_change_ranking business bodies with thin delegates. Strict
board aggregation and legacy summary now share BOARD_MONEY_AGGREGATES;
both ranking projections reuse the existing shared percentage arithmetic.
Pure Rust remains independent of Python and preserves its stricter missing,
finite, interval, budget and source-evidence policies.

Legacy summary retains the original member object, dictionary key order,
four money passes followed by volume/up/down passes, captured outer float/
int/sum, live per-element conversions and original lazy generators. The
small Python generator and lambda shells contain only iteration/object
access/call forwarding; the predicates, aggregation, selection and sorting
decisions are native. Body StopIteration becomes RuntimeError with cause.
Changes retain duplicate-code last-value semantics, zero/NaN references,
stable sorting, original custom hooks and unrestricted Python slice end.
Change ranking truncates before summaries; amount/volume/net ranking
aggregates every candidate before truncation.

History retains days validation before I/O, empty-board early return before
target conversion, days+10 requests, TransportError-only skipping, stable
datetime sorting, last eligible occurrence including duplicate source dates,
short available intervals, zero-start skip, round2 and late top_n slicing.
No strict complete-distinct-day or finite-price policy leaks into Python.

Added tests/test_native_extension_boards_contract.py with15 public test
function sources(one parametrized), and one shared registry Rust function.
Three new extension contract files now contain61 Python public test function
sources. All uncollected/uncompiled/unrun. Authored cases cover lazy/live
seven-pass evaluation, captured conversions, generator causes, output and
member identity, duplicates, pre/post aggregation, stable ties, None/negative/
custom slice, validation order, dynamic sort lambdas, history errors/short
windows/duplicate dates and custom enumerate indices. Runtime ABI, PyO3
signatures and actual wheel behavior still await the final gate.

Counts remain25 source-complete/2 partial(T19/T27)/3 unstarted(T28-T30).
Next EX/MAC-EX business/catalog/macro/HK, commands/parser, transport/owner/
generation/config, full manifest/export/adapter evidence and installed-wheel
consumer. Then T28-T30/T19 source closure and unified verification/fixes.
No verification execution, agents, publication or controller edits. Goal
active without quota; recovery commit is source preservation, not acceptance.

## Stage 3 — MAC paging, QFQ and download orchestration (2026-11-17, stage90)

Stage2 committed48b7f07. Added extension_client.rs and four native
functions: mac_page_query (five explicit legacy paging modes),
mac_local_recompute, mac_stock_kline, mac_download_file. Python
get_stock_quotes_list, _fetch_kline_pages, _local_recompute_qfq,
get_stock_kline, get_transactions, get_board_list, get_board_members and
download_file are thin native delegates. Existing method signatures,
aliases/defaults and original model objects stay at the Python surface.

New mac/python_legacy.rs has no Python dependency. Strict Pagination and
legacy paging share remaining_page_with (subtract, live maximum, minimum);
strict QFQ service and legacy stock_kline share repair_needed_with lazy
predicates. Strict budgets/validated counts/full-operation pinning remain
strict policies; legacy retains original per-wire timeout/execute hooks and
unbounded old paging count. No strict8s/32extra/page/record/byte cap was added.

Python quote/member pages append server order and return overdelivered/
duplicate occurrences; bar pages use original whole-batch addition at the
front, preserving custom __add__. Members convert board once even count0.
Quote default fields are constructed once but category int conversion
occurs per page. Live min/page constants and three distinct len evaluations
remain. Transactions and board lists always issue their first request, even
for nonpositive count, preserve that first result identity, and keep the
old behavior of continuing after a short first page. Execute is captured
before each command constructor, preserving a replacing custom hook.

Kline calls the seven positional page arguments, then optional QFQ invalid
check/None refetch/provider repair; None repair retains original server
series. Alignment/period validation occurs after fetching. Local legacy
repair accepts a valid unadjusted series even when every selected event
was skipped. Download keeps optional metadata for filesize<=0, literal
30000B requests,1-based index, partial bytearray on short/empty, declared
size stopping without clipping an overdelivered chunk, and original errors.
These are business algorithms in Rust; commands/DTOs/transport overrides
are explicit Python object adapters pending their own migration stages.

Added tests/test_native_extension_client_contract.py with16 public source
functions (one parametrized), plus two shared-policy Rust functions. Cases
exercise remainder/order/identity/defaults/category, live max/len,35 pages,
overdelivery/duplicates, board conversion count0, whole-page prepend/custom
add, first short/nonpositive initial requests, first-object extension,
date/TransportError identity, full QFQ sequence/seven args, deferred period
error, all-skipped event acceptance, partial download/index/no clipping and
captured execute. Together with helper contracts there are46 Python test
functions authored in the two new files. None collected/compiled/executed.

T27 remains partial: MAC summary/ranking/history; EX/MAC-EX business,
catalog/cache/HK; transports/owners/generations/config; commands/parsers;
all actual exports/manifest/adapter rationale and installed-wheel consumer.
Source-read fixes are not execution evidence. Generic/PyO3/source alias and
runtime ABI risks await final gate. Counts25 complete source/2 partial
(T19/T27)/3 unstarted(T28-T30). Goal active unlimited; no tests/build/check/
collection/dependencies/network/compiler/AST/format/diff checks, agents,
push/tag/publication/merge or controller edits. Continue all source scope,
then unified verification and all necessary repairs.

## Stage 2 — bitmap building and local adjustment (2026-11-17, stage89)

Stage1 committed ebcb278. Counts remain25 source-complete/2 partial/3
unstarted. Added native build_bitmap/build_exclude_flags/_post_ah_code and
all three adjustment helpers. Shared write_bit is used by strict bitmap
encoding and Python's ordinary integer builder. Custom widths/bits/numeric
methods preserve the Python path without a new cap; live normalizer and
bytearray/exception constructor timing remain visible. Strict and legacy
A/H code projections share the response-market width decision.

Strict and Python adjustment use the same denominator/numerator operation
order, OHLC mutation traversal and invalid-price short-circuit primitive.
Strict finite-input/output validation stays strict; Python retains original
NaN policy, stable datetime/event sorting, original date/bisect/compute/
math/range/enumerate/model hooks, per-column attribute collection order,
custom inplace numeric methods, shallow-copy no-event path and newly built
DTOs for selected but all-skipped events. Python sorting key functions are
explicit tiny object-access adapters, retaining lambda identity/type for
custom sorted hooks rather than routing business back into Python.

Added14 public Python contract functions (now30, two parametrized) and one
private Rust source function; expanded the shared bit writer fixture. Cases
cover normal/debug/high-bit/error layouts, dynamic32B/high200 bit, exact
double .value access, custom shift/inplace-or and captured bytearray,
exclude object/size, A/H membership/str, NaN/numeric-factor order, no-event
shallow identity/empty early return, same-date sequential adjusted closes,
all-skipped DTO rebuilding, live reverse range/inplace OHLC/model and eager
OHLC tuple before finite short-circuit. Also fixed source-read-observed
captured int before custom board slicing, and live FieldBit reread after
registry.get; both have authored contract cases.

No tests/collection/build/check/dependencies/network/compiler/AST/format/
diff checks ran. Source-only compatibility remains unverified; PyO3/generic
signatures and actual wheel import must be checked only after all source
scope is complete. Full client/transport/config/generation/command/parser/
catalog/manifest/consumer scope of T27 remains. Pure DTO declarations and
enum registries stay as original public object adapters, with business
methods still to be individually mapped. Goal active unlimited; no agents
or publication. Continue complete T27 and T28-T30, T19 final source handoff,
then unified execution/fixes and production protocol evidence.

## Stage 1 — board/selection/bitmap compatibility (2026-11-17, stage88)

Base5593729. T27 is started, not source-complete. Totals are25
source-complete/2 partial(T19/T27)/3 unstarted(T28-T30). Goal active with
no quota. Full implementation must continue before any execution check.

The binding SDK dependency now enables mac (implies ex). A new
extension_bitmap.rs registers seven functions and a private iterator.
Python convert_board_code, FieldSelection.__init__, normalize_fields,
_field_bits, count_set_bits, iter_set_bits and get_active_fields delegate
their business flow into native Rust. Python public DTO/enum/preset objects
remain the existing objects. Strict Rust BoardSymbol and Python conversion
share board_code_plan; strict decode_bitmap and Python ordinary bit
iteration share next_set_bit. Strict input/size/overflow limits do not leak
into Python's arbitrary integer and dynamic FIELD_BIT_LIMIT policies.

Preserved source decisions: strip then US/HK then len6 and88/399/899/000;
actual slice(2,None,None), live int/len and left/right arithmetic order;
FieldSelection live set/tuple/isinstance, ordered dedup/original references;
normalize identity and real tuple star expansion/length hints without adding
a module tuple lookup; mask before live int.from_bytes; iteration defers
conversion until first next, resolves live registry/fmt between yields,
counts unknown legal slots, ignores masked reserved bits, and retires after
errors. Standard exact nonnegative integers use the shared Rust byte scan;
custom/negative _field_bits results preserve lowbit Python numeric methods,
including __ixor__, without an invented finite iteration cap. Outer Python
iter_set_bits remains a generator using yield from.

Authored tests/test_native_extension_contract.py with16 public test
functions (first parametrized): Unicode/signed/huge board integers, strip
slice and short-circuit order, live len/arithmetic/error identity, selection
ordering/aliases/live set/tuple, normalize identity/star/iteration errors,
unknown/reserved/arbitrary bitmap width, deferred evaluation/live registry,
dynamic limit/mask conversion order, custom inplace xor, negative lazy
results, generator exception closure and live count/active helpers. Added
two public Rust mac_routing functions for shared scanner and plan source
contracts. None have been collected, compiled or executed.

Not migrated yet: build_bitmap/exclude_flags; adjustment/helper/model
business; all EX/MAC/MAC-EX ordinary business orchestration, management,
configuration, connection generation, raw/exception/custom transport
integration; full manifest and installed-wheel consumers. Existing protocol
bridge alone is not sufficient proof of those migrations. Continue the full
task-27-brief.md scope; do not replace it with this helper substage.

Source-reviewed risks: PyO3 method/lifetime signatures remain uncompiled;
Python star expansion edge errors must be assessed at final execution;
production MAC optional-frame boundaries/MAC-EX login proof still absent.
No production ACK was invented. Runtime/native ABI issues remain deferred.

Files: crates/eltdx-python/{Cargo.toml,src/lib.rs,src/extension_bitmap.rs},
crates/eltdx/src/mac/{bitmap,enums}.rs, src/eltdx/mac/{bitmap,client}.py,
tests/test_native_extension_contract.py, crates/eltdx/tests/mac_routing.rs,
canonical progress/status/plan and their package mirrors. Recovery commit
contains exact file changes; its SHA is recorded by the following stage.

No tests/build/check/collection/clippy/doctests/dependency resolution or
download/network/compiler/AST/format/diff checks. Ordinary source reads,
edits and Git review only; checkpoint disables hooks to preserve progress.
No agents, controller edits, push, tags, publication or merge.
