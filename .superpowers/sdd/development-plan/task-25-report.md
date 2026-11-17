# T25 Stage A report

Latest status: Stage B simple-response groundwork is now authored; read the
Stage B checkpoint at the end of this report. T25 remains partial. Historical
Stage A statements about missing simple codecs do not describe the latest
tree; owners, profiles, checked queries and public registration remain pending.

Status: isolated source checkpoint authored; not a public MAC feature and not a claim that T25 is complete. Authority read: `.superpowers/sdd/development-plan/task-25-stage-a-brief.md`, the complete `.superpowers/sdd/development-plan/task-25-brief.md`, extension API spec §§1, 4 and 5, development plan T25, and original Python MAC bitmap/enums/commands/models/codec plus MAC-EX command/model/login/client/HK routing sources. T24 Stage A baseline `04f613a` was treated as fixed. No shared or Python file was edited.

Implementation and authored source tests are commit `c87e5b0` (`feat(mac): add stage A bitmap and wire codecs`); this report is committed separately so the implementation checkpoint stays identifiable.

## Contract map

MAC uses the source `<BIBHH>` 10-byte request envelope: head flag, zero customize, version 1, and equal compressed/uncompressed inner lengths; the inner payload begins with a little-endian command u16. The pure response parser reads the `<IIIHH>` 16-byte header and checks the received compressed-body size against `zip_size`. Compression, stream reads, association, setup sequencing, profile readiness, cancellation, serialization and retirement are outside this checkpoint.

The native identity types distinguish `MacMarketId(u16)` from `MacExMarketId(u8)`. `MacCode` is strict GBK, nonempty, NUL-free, and limited to the 22-byte MAC field; the 0x1218 request additionally checks its narrower source 8-byte code field. No HQ market inference is performed. MAC-EX login payload 0x2454 is represented as its source 80-byte literal and first-byte packet rewrite only changes `0x1c` to `0x01`; the response parser does not claim login success.

0x122B response rows use 20 bitmap bytes, u32 total, u16 count, and fixed 68-byte identity prefix followed by one four-byte slot for every set bit in 0..=150. Unknown legal bits retain raw slot bytes and advance following offsets. Bits 151..=159 are retained in the observed reserved bitmap and create no slots. `FieldSelection` deduplicates to a bitset and emits ascending ids. 0x122C has a 16-byte request bitmap plus four control bytes; selected ids 128..=150 are rejected. Named field ids and wire types mirror the Python enum; ambiguous 93/94/136/139 names are selected by instrument/board context. AH_CODE keeps its raw u32 slot and derives text only using the response market argument. MAC-EX defaults to exactly ids `[0,1,2,3,4,5,7]`, separate from MAC Common.

The implemented request builders cover 0x122B/0x122C/0x122A/0x122D/0x122E/0x122F/0x123E/0x1231/0x1218/0x123D/0x1237/0x120F/0x124A/0x254D/0x2562/0x1215/0x1217 and MAC-EX 0x2454. The 0x122E request includes all four flags and the source 36-byte `<II7f>` records are preserved with f32 bits and period-13 seconds. Response payload parsing currently covers quote rows, bars, transactions, response frame header, and the mixed-endian 0x124A result. HK transaction price is stored as raw u32 plus divisor 1000; time seconds, volume, `nature`, and absent trade-count evidence remain explicit.

## Source ownership and deferrals

The 23 MAC methods are: `quotes`/`quote_list` (request/query and quote bitmap schema), `bars`, `tick_chart`, `tick_charts`, `chart_sampling`, `transactions`, `symbol_info`, `boards`, `board_members`, `belonging_boards`, `board_summary`, `board_ranking`, `board_change_ranking`, `capital_flow`, `auction`, `unusual`, `server_info`, `kline_offsets`, `file_meta`, `download_chunk`, `download_file`, and `goods`. Builders exist for the wire commands listed above; return-model parsing not named in the prior paragraph and owner/page behavior remain deferred. The aliases `get_stock_minute_time`, `get_stock_minute_time_hist`, and `get_stock_transaction` remain aliases to tick chart, multi-day chart, and transactions; they are not a second wire layout.

MAC-EX's 13 entries are `macro_indicators`, `macro_quotes`, `macro_bars`, `goods_count`, `goods`, `instruments`, `quotes`, `quote_list`, `bars`, `tick_chart`, `chart_sampling`, `transactions`, and `all_transactions`. Stage A shares MAC value/command definitions and contains only pure route facts: HK stock markets 27/31/48/49/71/98 choose the EX transaction route; markets 23..=26 and other values choose MAC. No EX model, EX command, session, frame transport, or login-success policy is copied into this directory.

T26 owns board aggregation and ranking compositions, complete download composition, directory fallback/binary search, macro-name resolution/cache, sorting, adjustment, and all-pages HK transaction collection. T27 owns Python bridge compatibility, including legacy independent request timeouts/retry behavior, old leniency and legacy full-list cap behavior. T25 Stage B still must implement/coordinate the broader simple response schemas and public methods without false `Unsupported` results.

## Exact Stage B seams and evidence gaps

Stage A files here are deliberately not registered by `crates/eltdx/src/lib.rs`; `Cargo.toml` features are also untouched. Stage B registration/owner work must update those root surfaces and add MAC owner/profile/session modules, configuration, endpoints and methods. The MAC owner must consume these pure codecs via the shared operation/runtime gates, use an explicit MAC profile, execute bounded three-step setup with `OptionalReply`, and publish only after validated setup. The MAC-EX owner must create a new MAC-EX profile connection, send 0x2454 before Ready, reject invalid/negative login at `Handshake` with Connection/Protocol classification, and use EX wire machinery only through the approved EX integration seam. First-byte MAC-to-EX rewrite must leave all subsequent packet bytes untouched. No test establishes a late optional setup response boundary in this source-only checkpoint.

Evidence gaps remain for MAC setup optional-response boundaries and the login handshake success/negative payload; actual frame sequence/association and decompression policy; source-confirmed market-specific value units; the hash algorithm; 0x1218 JSON payload details; full response parser contracts for tick charts, symbol info, board and resource methods; and network behavior. The new response codec set is therefore preparatory rather than complete coverage of all simple method results. Do not infer A-share units or claim login from Python's leniency. The MAC-EX `all_transactions` accepted market set and the 50-page/90,000-row operation cap belong to Stage B/T26 coordination, not the pure route helper.

The files staged for the next integration are `crates/eltdx/src/mac/{mod.rs,bitmap.rs,commands.rs,models.rs,routing.rs}`, `crates/eltdx/tests/mac_wire.rs`, and this report. `mod.rs` remains internal and unreferenced from the crate root until the controller grants Stage B GO. T24's `ex/` tree is a dependency, not an additional copy site. T26 additions belong in the separately owned MAC services and EX catalog paths; T27 changes belong in the Python bridge paths under `src/eltdx/mac` and `src/eltdx/ex`.

## Authored source cases

`crates/eltdx/tests/mac_wire.rs` authors literal cases for (1) legal unknown bits between known slots and reserved bits 151/159, (2) deduplicated selections and 0x122C rejection of bit 128 while allowing 127, (3) exact request envelope bytes, 16-byte header parsing and truncated/half bar response rejection, (4) bit-identical seconds-period bar time and negative-zero float bits, (5) exact seven-field MAC-EX defaults, HK stock versus HK futures route, raw HK price/nature and missing trade count, (6) checked MAC market/code boundaries, and (7) signed i32 raw slot interpretation. These tests are source only.

## Self-review and execution status

The source keeps identities, bitmap selection and pure payload codecs separate from session/transport code, preserves unknown legal response slots and full bitmap bytes, records raw bar and scalar bits, and checks declared quote/bar/transaction cardinality exactly. Context aliases are chosen during projection without changing their bit. The principal incompleteness is breadth: several simple command responses and typed endpoint schemas remain for Stage B; the report intentionally does not describe those as covered. There is no MAC client, public feature registration, profile, handshake lifecycle, host selection, budget/cancellation, pagination, retry/failover, T26 composite support, or T27 compatibility implementation here.

No tests, collection, build/check, clippy, doctest, dependency resolution, download/network, compiler/AST/format/diff check, or equivalent validation was run. All authored tests remain unexecuted until every T01–T30 implementation is complete, as required by the task boundary.

## Stage B checkpoint 1 — simple response schemas (2026-11-17, stage76)

Base `d8b0e48`. Re-read the complete original MAC commands/models/codec/transport
and MAC-EX login/command/client routing sources, plus extension spec §§4–5.
Authored `mac/responses.rs` and `mac/decode.rs` without copying the EX network
loop or changing Python. This is a source checkpoint, not T25 completion.

The simple codecs now cover single-day and five-day charts, symbol snapshots,
board/leader records, sampling, goods, auction, unusual, server session,
file metadata/download chunks, and both distinct 0x1218 JSON shapes. Existing
Stage A quote/bar/transaction/kline-offset codecs remain the only definitions
of their layouts. Every count-based fixed layout checks its aggregate length
before allocating rows; truncated records fail Protocol rather than appearing
as empty successful pages. Unknown snapshot fields, symbol/server/file tails,
unusual raw records, source identity bytes, native float bits and unresolved
units remain observable. New response DTO fields have private construction and
getters. No prices/volumes are scaled from their field names.

The shared chart tail is **120 bytes**, derived field by field from source
`<44sBHf5x2I5ffIf12s2fI>`, not 128. Single-day ticks are 18 bytes; multi-day
ticks are 14 bytes. Multi-day dates/count/page_size and exact tick payload are
checked; a short first day is followed by complete days and then empty days.
Bad date slots, zero active page size, totals exceeding day capacity and partial
tails fail Protocol. Single-day valid zero-tick charts still require their
header and snapshot tail. Symbol HHMMSS is distinct from transaction seconds;
both zero date/time represent absent symbol event time.

Server-info is parsed from source field spans: 22-byte prefix, today at22,
timestamp at26, two16-byte window groups at30/46, flag at62, previous day at63,
timestamp at67. The complete core is71 bytes; optional u32s occupy71/75 and
must individually be complete. Bytes after79 are retained. This strict model
does not copy Python's historical87-byte minimum-to-empty projection; T27 must
preserve that projection separately. Invalid observed server dates keep raw
and source text with parsed=None; the future DataReady path must require a
valid today and attach diagnostics. They are not silently calendar dates.

0x1218 ignores exactly27 prefix bytes before strict GBK/JSON decoding. Belonging
boards validate9/13-element source rows; capital-flow preserves separate today
and five-day tables, absent/null/present/derived states and source JSON scalars.
Missing amounts are None, not fabricated0; numeric invalid/nonfinite fields
fail Protocol. Main/small net is derived only from two present operands. Mid/
large five-day net uses source indices4/3; no date is invented. Valid empty JSON
arrays retain the source no-object semantics. These strict results must not
replace Python's lenient defaults or exception/conversion order in T27.

`RemoteFileName` now independently validates nonempty/NUL-free strict GBK<=70B.
The previous helper incorrectly constructed `MacCode` first, limiting names to
22B. Resource encoders use the new name check; chunk data validates its complete
8-byte prefix and never exceeds requested bytes. Hash bytes/text are retained
without asserting a digest algorithm. MacCode owns the GBK Cow before moving
its source text. Stage A crate-local imports/FieldContext visibility were
corrected, and source integration cases are explicitly gated by feature mac.

Authored14 unexecuted `tests/mac_responses.rs` functions for filename boundaries,
nonempty/empty/partial charts, short-first-day partition, invalid date/count/
page size, snapshot source offsets/seconds/negative-zero/NaN bits, board leader
identity and pairing, goods placeholder bits, signed auction quantity,
unknown unusual kind/name suffix, observed bad server date/optional parameter,
sampling variable length, resource prefix/hash/bounds, GBK JSON prefix and
capital-flow presence/nonfinite/error behavior. Existing7 mac_wire functions
remain source only. The implementation index records all36 public entries
as pending owner/query integration rather than false completed methods.

Next: checked query/enums/page DTOs and explicit conversions, standard-MAC
OptionalReply boundary and MAC-EX login verifier/profile, then independent
owners using T24 Session/worker/transport and full public simple methods. Keep
T26 directory/aggregate/download/adjustment services and T27 compatibility
separate. Production optional-setup and login-acceptance captures are still
missing: do not invent acknowledgment fields or call a quiet timeout a proven
boundary. T25 module remains unregistered until its actual owner exists.

No test/collection/build/check/clippy/doctest/dependency/network/AST/compiler/
format/diff-check or equivalent execution occurred. Only source reads/edits,
ordinary Git diff/status and a hooks-disabled checkpoint are allowed. Counts
remain23 source complete/2 partial/5 unstarted; active goal has no quota.

## Stage B checkpoint 2 — checked queries and pagination (2026-11-17, stage77)

Base `80de370`. Preserved and completed the uncommitted enums/query/model edits
from the previous context. `mac/enums.rs` contains the complete source closed
BoardType/ExBoardType/Category/SortType/SortOrder/FilterType registries, checked
wire conversion, OR-based FilterMask and source-preserving BoardSymbol. Board
conversion uses checked u32 arithmetic and the six specified source examples;
prefixes remain case-sensitive. `QuoteListOrder::SourceOrder` is explicit.

`mac/query.rs` now owns all23 standard-MAC checked queries, including composite
query configuration for T26. Counts are NonZeroU32 total targets rather than a
u16 wire page; page count/record policies remain separate. Narrow GBK8 JSON
request subfields,20B/16B bitmap constraints, u16 unusual offsets,70B file names,
positive chunk indices,30000B chunks and u32 source spans validate before I/O.
Shared DownloadOptions accepts max_bytes=0 for the future no-I/O path. Added
MacPeriod checked known registry, known-semantics guard and exact duration:
MAC Mins8 uses5min*times, Seconds13 uses times seconds; calendar periods have
no fixed duration. This does not alter EX period8 or Python seconds alignment.

`mac/ex_query.rs` supplies all query-bearing MAC-EX entries. IDs are canonical
ExInstrumentId; explicit bidirectional TryFrom rejects non-GBK EX codes,
u16-to-u8 market overflow and MAC text exceeding the EX9-byte UTF-8 field.
MAC-EX quote defaults remain exactly0/1/2/3/4/5/7; SourceOrder count100 exposes
an actual goods request<=80 and rejects unsupported sorting at construction.
Shared standard-MAC bar/tick/sampling queries carry the one encoding policy;
macro bars default Monthly. HK transaction pages are1800, others1000; HK offsets
also fit the source signed i32 EX field. all_transactions accepts only the six
HK stock markets and at most50pages/90000rows, with caller smaller limits.
Directory queries default600/2000/1200000 with explicit smaller-policy support.
Macro codes trim whitespace only, retain case and do not claim source existence
before T26 directory resolution; macro quote occurrences retain duplicates.

`mac/pagination.rs` implements pure checked request/accept progression. Requests
use the minimum of remaining target, page size, wire maximum and remaining row
allowance. Only complete decoded wire rows advance offsets, not postprocessed
row counts. A second pending request, oversized response or overflow fails.
The source-confirmed empty/short end policy is distinct from empty-only policy;
ordinary EX single-page semantics are not changed. Full safety caps fail
LimitExceeded rather than claiming EOF; hitting a finite requested count is
RequestedCount. PageRows appends lists or prepends whole bar pages without
reversing rows within a page or removing duplicate occurrences. PageResult and
PageCompletion reuse the existing EX shared schema rather than copying it.

Authored10 unexecuted `tests/mac_queries.rs` functions and6 private pagination
functions covering lossless conversion/encoding rejection, checked symbols,
OR duplicates/registry values,100000-row targets, bitmap/code boundaries,
period duration/seconds, dates/downloads, MAC-EX seven defaults/SourceOrder,
HK routes/signed offsets/full caps and macro defaults. Existing path-based
MAC source tests now import canonical EX/types/PageOptions/DownloadOptions so
the growing unregistered module has its actual dependencies at source level.

No test/collection/build/check/clippy/doctest/dependency/network/AST/compiler/
format/diff-check or equivalent execution occurred. Counts remain23 complete
source/2 partial/5 unstarted. MAC client/profile/root registration are still
missing; T25 is partial. Next: owner/profile negotiation with explicit setup
boundary and login verification evidence, shared T24 worker integration,
simple public methods/meta/raw/management; then T26–T30 and final validation.

## Stage B checkpoint 3 — explicit handshake evidence (2026-11-17, stage78)

Query/pagination checkpoint committed `538b01d`; no reconstruction is needed.
Authored pure `mac/handshake.rs`, registered only with the existing unregistered
MAC groundwork. It does not yet perform negotiation or create any client.

Source inspection reconfirmed the three literal standard-MAC setup packets and
that Python swallows setup socket/transport failures; login0x2454 only checks
len(body)>=2 and its caller ignores that bool. There are no independent
production response association or positive/negative login payloads in the
current evidence inventory. No new network probe was run to fill that gap.

ResponseIdentity supplies exact magic/sequence/method expectations as explicit
caller assertions. OptionalSetupBoundary requires disjoint optional identities
and a mandatory fence identity, an evidence label and an ordered-response
contract supplied by the caller. Optional steps form a deferred suffix: a
Required reply may not consume a pending optional frame. Its bounded tracker
consumes each optional reply at most once, accepts absence only when the exact
fence arrives, and makes unknown/duplicate/post-fence use terminal. A future
profile must still strictly decode the fence business response and validate
today before publishing a socket. The pure tracker is not proof that actual
production servers obey the supplied ordering assertion.

MacSetupPolicy's conservative source-only default requires all3 complete
replies. Silent timeout is a handshake failure, not OptionalReply evidence.
OptionalWithFence supports an evidence-backed profile rather than guessing
response associations. This default may reject real hosts that omit a reply;
production optional-boundary fixtures remain an acceptance gap and must be
resolved in the final allowed evidence phase. Do not claim full host support.

LoginReplyRules holds bounded, disjoint exact complete positive/negative
payloads and an evidence label, with a redacted Debug implementation. Unknown
payloads, including arbitrary>=2B payloads, cannot pass require_accepted;
missing rules also fail Protocol/Handshake. Authored fixtures use explicitly
synthetic positive/negative strings; they establish no production ACK/status
bytes. This is a preparation seam, not a final usable default login policy:
the final evidence pass must supply a production classifier before accepting
the public default MAC-EX path. Do not falsely mark T25 complete or advertise
authentication based on Python leniency. T27 keeps the old Python behavior.

Authored5 private source functions for optional absence/exact fence,
unknown/duplicate/post-terminal responses, overlapping/interleaved rules,
positive/negative/unknown login decisions and invalid/absent evidence. No test
or other execution verification occurred. Current source totals remain23/2/5;
goal active, no token cap, no agents/publication. Next implementation remains
owner/config/profile transport reuse, then all simple APIs/meta/raw/probes,
T26/T27 and T28–T30; response evidence is explicitly unresolved rather than
an excuse to skip independent remaining source work.

## Stage B checkpoint 4 — independent owners and first public methods (2026-11-17, stage79)

Base2a11033; retained pending config/profile/session and shared EX probe edits
across context compression, then connected them to public clients. Do not
reimplement these owners on recovery. This is source progress, not acceptance.

MacConfig/MacExConfig reuse checked ExConfig limits but keep independent numeric
candidate lists,8s default,rawOmit and immutable setup/login inputs. Pure builders
validate endpoint/positive monotonic timeout configuration without I/O or
environment/disk resolution. Each build creates an independent SessionOwner
and Session; clones retain the same irreversible close state. Both clients
offer config/host(s)/diagnostics/healthy/unhealthy/close/disconnect/connect/
ensure_connected/probe_endpoints/discover/ping_all/refresh_hosts/
refresh_endpoints/from_best_host. Discovery and selected connect share one
context and physical attempt/stream-byte sponsor; no renewed per-page clock.

MacProfile sends the three original literal setup packets and reads complete
replies via the existing EX read_frame/write functions. Optional suffix replies
are deferred to the exact caller-asserted fence tracker; server_info fence must
strictly decode and have valid today. No silent timeout proves absence.
MacExProfile requires configured LoginReplyRules, sends the source80B login
body and requires an exact positive reply. It never sends EX92B setup. Every
replacement socket carries this same immutable per-owner profile. None of this
resolves the still-missing production login/optional ordering evidence: default
MAC may reject optional servers, and default MAC-EX cannot authenticate yet.
Final allowed evidence work must resolve that before accepting T25.

MacOwner reuses the existing EX registered worker/session lease/frame decoder,
domain-neutral authorization and cancellation/cleanup. Encoded MAC-EX changes
only a0x1c first byte to1; ordinary EX Work::Request is unchanged. Decoding,
over-response rejection,raw projection and source stamp happen before socket
publication. exchange_leased permits later fixed-source downloads without a
second session or a new operation. The existing EX probe scheduler now takes
an immutable DataProbe for both protocols;8-worker bounded runtime and terminal
parent limits/cancellation are unchanged. TCP-only is still distinct. MAC
readiness requires valid server today; MAC-EX requires positive configured login
then exactly one market38 goods record. This proves only that sample command;
do not claim every market or production classifier has been validated.

MAC first13 wired methods: quotes,tick_chart,tick_charts,chart_sampling,
symbol_info,belonging_boards,capital_flow,auction,server_info,kline_offsets,
file_meta,download_chunk,goods. MAC-EX shares quotes/tick_chart/chart_sampling
with canonical checked EX-to-MAC identity conversion. Quote validates response
identity while retaining code bytes/name replacement indication, ordered bits,
legal unknown fields,reserved bitmap and per-response ResultMeta. Goods returns
PageResult with actual count/next offset, no invented global market total or
single-page EOF. KlineOffsetInfo keeps independent mixed-endian counters.
Server bad dates retain observations plus diagnostics. Other simple methods'
last-response meta is observable through diagnostics.latest_meta.

Registered mac module in lib.rs behind existing mac=[ex]. Replaced the three
path-included MAC source suites with imports from actual public eltdx::mac;
private owner/profile modules are no longer cloned into integration crates.
Authored5 public mac_session source functions: lazy independent build/shared
close; required three-step setup/nonempty known+unknown quote/raw provenance;
optional missing reply plus exact ordered fence and aligned next response;
MAC-EX80B login/head rewrite/seven-field request; negative login with no business
request. All replies are explicitly synthetic and establish no production ACK.

No test/collection/build/check/clippy/doctest/dependency download/network probe/
AST/compiler/format/diff-check execution occurred. Ordinary reads and Git diffs
only. Counts remain23 complete-source/2 partial/5 unstarted, goal active with
no token cap and no agents/publication. Next T25 source: public Bar/Transaction
and exact seconds, paginated quote_list/board_members/bars/transactions/boards/
unusual with chronological metadata/raw mapping, HK EX commands and lifecycle/
cancellation/half-frame/failover source coverage; then T26–T30. Production
handshake acceptance remains explicit final-phase work, not fake success.

## Stage B checkpoint 5 — rich records and public page collectors (2026-11-17, stage80)

Basec66fe5c. Preserved all pending market_data/collect/metadata/second-arithmetic
edits across context compression, manually read their dependencies and wired
them into the public owners. No source was compiled, collected or executed.

Public MAC now has19/23 specified method sources: the previous13 plus
quote_list,board_members,bars,transactions,boards,unusual. Remaining four are
T26 compositions: board_summary,board_ranking,board_change_ranking,download_file
(and its strict complete convenience). MAC-EX now has6/13 method sources:
quotes,tick_chart,chart_sampling,bars,transactions,all_transactions. Its other
seven require directory/cache/macro services in T26. Method source existence
does not complete T25 or establish public usability. Auxiliary fields/util/
advanced surfaces and broader coverage also remain.

market_data.rs supplies private-field Bar/Transaction and typed time/source/
count/nature provenance. Bar retains all7 f32 bits, exact seconds from midnight,
raw date/time, and only known intraday durations shift End. Calendar periods
use local midnight; explicitly unknown periods retain unknown EventTime and
diagnostics instead of guessing calendar or rejecting their raw dates. Strict
36B bars always provide float_shares: Some(wire0) is distinct from absence in
another schema. Optional raw records are Bytes slices of the same body, never
per-record raw copies. EventTime::add_seconds shares checked dated arithmetic;
add_minutes retains its prior validation/error boundary. QFQ repair/provider/
strict_adjustment behavior is still T26, not yet implemented by these routes.

collect.rs reuses checked Pagination and existing services::aggregate_meta.
Each reply is fully decoded and over-response-checked inside the worker before
publication. Progress uses actual wire occurrences, never deduplicated keys.
List/member requests use80 maximum, bars700, MAC trades1000 and HK trades1800;
the last request is the remaining target or smaller explicit safety allowance.
Quote rows append, whole bar pages prepend without reversing rows inside a
page. Raw row references alone receive the same permutation; retained bodies,
frames and SourceStamps stay chronological. Each page's records share one
Arc<ResultMeta>, preventing quadratic raw-range cloning at large row counts.
Changing reported totals yields None with diagnostic; no single total is
asserted. Aggregate metadata is retained in diagnostics.latest_meta.

quote_list uses Board context only for the closed10000..10010 categories;
board_members always uses Instrument context, preserving the bit93/94/136/139
aliases and actual bit IDs. boards uses EmptyOnly because original source
permits another request after its first short page; paired_count is not a
global count, and total_raw remains available only in the pure BoardPage.
unusual is explicitly one bounded response, matching the source's lack of any
pagination loop. It returns actual next offset; RequestedCount only if target
is fulfilled, otherwise Unknown even on short/empty, never fabricated EOF.

MAC-EX HK stocks27/31/48/49/71/98 send unchanged ordinary EX23FC/2406 requests
on the same MAC-EX owner/profile. Their data uses the existing strict EX decoder
and a diagnostic/range-preserving Decoded::try_map seam; no parallel EX socket
or second decode. Raw u32 price/divisor1000 with documented HKD, count0 with
NotProvided, nature, original direction and open-interest change survive.
Other markets, including23..26 futures/options, use MAC122F. all_transactions
supports only the checked HK markets and Target::All; a full50-page/90000-row
cap, or a smaller explicit cap, fails LimitExceeded without terminal evidence.

Added14 public fixture source functions (19 total in mac_session.rs):80+20 and
duplicate list rows/shared metadata; category/constituent aliases and changing
total; bar page/raw permutation and seconds/end rollover; calendar/unknown
period; MAC trades and provided count; HK today/history commands/native price/
absent count; futures route and full all cap; unusual Unknown; malformed clock
and pre-cancel; boards short-followup/leader; cancelled partial frame/fresh
setup; close during partial frame; transport failover/repeated setup; complete
over-response/no reusable socket. All synthetic responses remain independent
of production login/association evidence. No test was collected or run.

Ordinary source reading found PageOptions is Clone but not Copy, and corrected
four draft dereferences to clones; the explicit unknown period fixture now
uses the existing Wire variant. No compiler/AST/format/diff check was used.
T25 production login/optional-boundary evidence remains unresolved and default
MAC-EX fails explicitly without rules. Continue remaining T25 helpers/coverage,
T26 compositions and T27–T30 source before the unified verification phase.
Counts23 complete source/2 partial/5 unstarted, active unlimited goal; no
tests/build/check/collection/dependency/network/equivalent verification,
subagents/push/tag/publish/merge.
