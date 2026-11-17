# MAC-EX directory and macro implementation

2026-11-17 stage85 source handoff. Every implementation and fixture below is
uncompiled and unexecuted. This is source coverage, not acceptance evidence.
Stage86 completes T26's source scope and final handoff; every execution
acceptance item remains deferred. T25, T27–T30 and the final unified execution
phase remain outstanding. See [task-wide handoff](t26-implementation-index.md).

## Public entry points and source ownership

`crates/eltdx/src/mac/ex_services.rs` adds the remaining seven named methods:
`goods_count`, `goods`, `instruments`, `macro_indicators`, `macro_quotes`,
`macro_bars`, `quote_list`. MAC now has23/23 named method sources and MAC-EX
has13/13. This count says nothing about successful compilation or production
support. Shared `quotes`/`bars` continue to use the existing MAC command and
decoder; no second transport, socket loop, executor or Python dependency.

The additional `instrument_catalog`, `goods_catalog`, corresponding
`refresh_*` methods and `clear_catalog_cache` expose explicit immutable
snapshots. A refresh currently invalidates both catalog kinds for the owner
before loading the requested kind/market. A failed refresh does not restore
lookup eligibility of the old cache. Existing Arc handles remain readable.

`src/ex/catalog.rs` owns snapshot identity and bounded retention;
`src/mac/catalog.rs` owns the source-pinned composition. They are compiled only
with `mac` (which already implies `ex`). The public checked query layer stays
in `mac/ex_query.rs`. MAC and EX market IDs are never inferred as HQ exchanges.

## Full bounded directory scan

An EX snapshot first reads global count0x23F0, scans0x23F5 from offset0 with
checked returned offsets/counts and complete64B records, then rereads count
on that exact connection generation. Every global row is considered before
claiming sorted market bounds. A matching market may span multiple pages;
all occurrences and duplicates survive in source order. An absent market
gets its true insertion position only if the whole observed directory was
sorted. An unsorted observation returns complete matching rows, a
`DirectoryUnsorted` diagnostic and no sorted range. A premature empty page or
changed final count fails Protocol. The final count is a growth/shrink fence;
equal counts do not establish a server transaction or detect equal-count
in-place mutations. No MVCC or atomic snapshot is asserted.

The first accepted count pins endpoint and socket generation. Subsequent
requests use `PinnedEncoded`, require the already established socket and
allow one physical attempt, without reconnect/failover. An unaccepted first
count can follow normal retry/failover policy. Fallback/name lookup starts
from an already accepted goods source and pins even its first EX count.

MAC goods0x2562 is collected from offset0 until short/empty evidence within
the caller's finite pages/records. Native codes, category and all three
uninterpreted f32 slots retain original bits. Only an initially empty result
activates same-source EX fallback. Derived GoodsItem attaches the complete
original InstrumentInfo, including actual code/name/description/raw64.
Synthetic numeric/default fields are marked by `is_derived()` and
`derived_numeric_code()`, never substituted for the actual identity.
`EmptyGoodsExFallback` identifies this observation explicitly.

`goods_count(None)` is one direct global EX count. `goods_count(Some(market))`
uses the complete bounded goods snapshot. `goods` and `instruments` slice a
complete snapshot with market-relative start/count, continuation and total.
The page's raw row references select its actual returned records while all
observed payloads/frames/source stamps remain available.

## Cache contract

Keys include owner identity, endpoint, connection generation, immutable
profile name, market, catalog kind, raw policy and schema version1. Replaced
source/profile/schema makes old entries ineligible. There is no process
global cache, implicit disk storage or TTL freshness promise. A caller uses
explicit refresh when a live same-source directory may have changed.

Each owner has separate goods and instrument caches. Each kind retains at
most16 entries,65,536 rows and16MiB of accounted rows/strings/provenance.
These are per-kind limits (up to32 retained entries and32MiB across both).
Access updates recency. Oversized valid results are returned without cache
retention. Cache clear advances an epoch so a load started before clear
cannot republish afterward. Versions are monotonic per kind; exhaustion
returns an error instead of wrapping. Arc handles retain their own source
version and survive refresh, clear, source replacement and owner close.

A cache hit still charges its returned rows and retained memory to the new
operation. A new scan charges all consumed global records; a hit does not
pretend to reread that global input. Row/string/range allocations and retained
metadata copies are charged before the newly authored result projection;
some shared storage is conservatively charged again. Raw=false is not a
zero-copy claim. Incoming frame and declared decode bytes retain the shared
transport budget gates.

## Macro and source-order consumers

Market38 selectors preserve case. Full codes require an exact code match;
bare selectors match an exact underscore suffix or actual display name.
Duplicate occurrences of the same full code collapse only for ambiguity
decisions; different matching codes cause InvalidArgument with candidates.
Missing selectors cause MissingData. Native goods carries codes without a
display name; unresolved bare names may use the same-source EX directory.
Full/suffix matches do not require that extra scan. Both observations remain
in enriched provenance, without copying the whole catalog into another cache.

`macro_quotes` batches at80, retains requested duplicate occurrences and
joins replies by actual market/code identity, correcting response reordering.
A missing resolved quote is MissingData; it never borrows a positional row.
All batches pin the catalog source. `macro_bars` resolves before installing
the checked request identity, then uses the existing normal/QFQ collector
with that same exact source binding. An intervening connection replacement
fails rather than using the wrong directory. Directory provenance is kept;
primary raw ranges describe only returned quotes/bars.

`quote_list` selects directory occurrences in source order and requests at
most `min(count,80)`. Its result includes the full known directory total and
the actual continuation. Count>80 adds `SourceOrder80`; completion is
MoreAvailable when the complete directory has more rows, EndOfData when
exhausted. Unsupported sort pairs are rejected during checked construction.

Every catalog/macro composition freezes the common operation's records,
bytes, deadline and total logical-request cap before its first child. The
opt-in request cap covers Page, Metadata, Fallback and injected XDXR source
requests; physical retries keep the existing attempt/extra authorization.
It remains active when resolved macro bars enters the shared QFQ service.
Ordinary HQ/EX/Python legacy contexts do not opt into this new cap.

## Authored source evidence and remaining work

`tests/extended_catalogs.rs` has32 authored public functions covering full
sorted/absent/unsorted directories, native/fallback goods and exact raw rows,
direct/market counts, changed/malformed/empty directories, total request and
record gates, fallback extra budget, cache reuse/refresh/clear/in-flight
invalidation/raw policy, macro duplicates/ambiguity/case/name/missing quote,
80-row source-order truncation, monthly macro bars, preaccept failover,
pinned-source loss, cancel/close half-frame and local entry/byte guards.
Stage86 additionally covers source replacement with retained old handles,
independent owners at one endpoint,80+1 macro occurrences and raw joins,
exact display-name ambiguity, empty/beyond-end pages without quote sends,
and source-bound macro bars loss. Some functions contain several cases.
Six private cache functions cover
eviction/identity/epochs/retained handles/overflow; two OperationContext
functions cover opt-in total-request admission. All are unexecuted.

The remaining lifecycle, source-replacement, multi-batch sources and bounded
review are now recorded in the stage86 task-wide handoff. T25 still needs its remaining
simple-method/schema source coverage. T27 must preserve Python's separate
legacy cache/input/order/timeout/partial-result policies. T28–T30 still own
consumer, platform, packaging and final documentation sources.

Production MAC optional-boundary/login captures remain unverified. Synthetic
login rules are fixture inputs only. MAC-EX has no production default login
rules and fails Protocol/Handshake without explicit evidence; this is an
open final evidence-phase requirement, not a completed production feature.
No tests, builds, checks, dependency resolution/download, network probes,
compiler/AST/format/diff checks ran for this source checkpoint.
