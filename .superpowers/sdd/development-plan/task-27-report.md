# T27 implementation report

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
