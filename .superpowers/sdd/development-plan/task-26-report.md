# T26 implementation report

## Stage 2 — full board compositions (2026-11-17, stage83)

Base95721fc. Added mac/boards.rs and tests/extended_boards.rs; updated
mac/{mod,client,query,responses,collect}.rs and the mirrored domain docs.
MAC22/23 methods now have implementation sources; download_file remains.
T26 stays partial, with23 source-complete/3 partial/4 unstarted overall.
No source, fixture, example or existing implementation has been compiled,
collected, tested, formatted or runtime-verified. No dependency/network work.
No subagents, push/tag/publish/merge. Goal stays active without a token quota.

Public MacClient::board_summary, board_ranking and board_change_ranking each
admit one existing parent operation. Their pages freeze total page and input
record limits in addition to the existing byte ceiling. Directory EOF pages,
every member page and every daily-history page use the same sponsor. Changed
ordinary list/boards page authorization from Required to Page so these child
pages actually participate in that shared total; neither purpose consumes an
extra for an ordinary first attempt. Per-board member_pages80/2000/160000 and
history_pages700/200/140000 remain additional smaller protections. All required
failures, cancellation/close/deadline and full caps propagate, never become a
successful empty rank. Global page gate rejects before starting another send.

BoardSummary collects Target::All, rather than the ordinary members query's
100000 target. It aggregates checked u64 volume, source-f32 widened f64 money,
four money metrics, up/down counts and all member occurrences. Source unit
and evidence descriptors are preserved, unknown units stay unknown. No field
is created in an individual Quote when missing. Numeric subtotal plus per-bit
missing_member_count and MissingAggregateField make incomplete observation
explicit. Nonfinite inputs/results survive and get NonfiniteAggregateField.
Missing/nonfinite metric keys sort behind known keys in either direction.
All member fields/unknown legal slots remain; additional fields get occurrence-
qualified NamedNativeField/NativeValue views, source bits/bytes intact.

BoardSummary/BoardRank/BoardChangeRank/NamedNativeField fields are private with
read-only getters. Rank.source includes the entire original board/leader pair.
Changed BoardRankingQuery top_n to Option<NonZeroU32>, defaultSome50, checked
with_top_n(Option<u32>), Noneall. This intentionally resolves the original API
table contradicting the already specified Noneall rule. Zero present fails.
Money/volume/net/net3/net5 request every candidate summary before top-N. The
change-only route can first stable-sort/truncate candidates using source price
and preclose, then load only selected summaries. Zero/nonfinite preclose gives
None plus InvalidBoardReference, even if that candidate is later truncated.
No round is applied to native Rust return percentages. Existing shared
helpers::calculation::percentage_change_with supplies the arithmetic sequence.

Daily history uses the exact existing bar request/decode/projection seam,
collect::bars_page, factored out of the prior collector without another wire
layout/decoder. It requests min(history page size,days+10), advances actual
wire occurrences, prepends intact pages and stably orders by source date.
It continues towards older pages until target-date eligible N+1 distinct dates
exist, or command-specific EOF. Full cap while a window is still needed is
LimitExceeded. Confirmed insufficient histories are skipped with a diagnostic,
never0%. All duplicate observations and raw rows survive; last stable daily
observation supplies an endpoint and DuplicateBoardHistoryDate explains that
duplicates do not create missing trading intervals. Nonpositive/nonfinite
window endpoints are skipped with InvalidBoardHistoryPrice. start_date/end_date
and original close_start/close_end WireFloat bits are exposed. Even an empty
rank publishes aggregate diagnostics.latest_meta with all missing evidence.

One fallible stable index-merge sort checks parent boundaries during merge,
copy and permutation, retaining equal-key input order. Temporary indices,
native result slots, extra fields and distinct-date keys charge the same parent
bytes before accepted retention. In-place occurrence permutation avoids a
second full vector of heavy summaries/parsed bars. Aggregates retain directory
and all member/history payloads/wires/stamps in physical observation order.
Primary rank raw row references select returned directory board occurrences,
in returned rank order; member summaries retain their own member raw maps.
Returned rank rows share a single aggregate meta. Original source/leader data
and per-member/per-history raw data are not replaced by invented wire fields.

extended_boards.rs has16 authored public functions: all81 members/native units/
u64 sums/unknown bytes; full member cap; third candidate wins amounttop2 and
stable ties/raw selection; volume and all three net keys; zero reference None
and ordering both directions; change-only early selection; shared page cap;
required malformed member propagation; older target/full intervals/missing
history; full history cap; zero endpoint with empty-result evidence; query
defaults/None/overflow/child policy checks; missing fields/source absence;
nonfinite money/raw NaN bits and both directions; duplicate daily occurrence
retention and continued older fetch; cancellation/owner-close in member header
half-frame. Fixtures use independent literal command offsets/bit values and
std socket/channel barriers, not the SDK encoder as their expected oracle.
No function has been compiled, collected or run.

T27 must keep old Python two-decimal round, zero-reference-as0 ordering,
missing-field defaults, required historic TransportError skip, and the old
max(0,end-days) short window even if fewer than N intervals are available.
Do not apply Rust's distinct-date/full-window/shared-cap policy to that legacy
surface. No Python implementation was modified here. Continue same-source
download, full MAC-EX catalog/cache/macro and remaining7 public methods, T25
remaining source coverage/real handshake evidence, then fullT27-T30 before any
unified verification. Production login/optional-boundary gap remains explicit;
synthetic public fixtures still are not production protocol evidence.

## Stage 1 — parent-scoped MAC QFQ repair (2026-11-17, stage82)

Base0ec8907. T26 is partial, not complete. T19 and T25 are also partial;
23 tasks have source-complete implementations,3 are partial,4 have not started.
No implementation has been compiled, tested, collected or runtime-verified.
The whole T01-T30 objective is active without a token quota. Continue the full
scope; this source checkpoint is not a smaller replacement completion goal.

### Sources and public integration

Added crates/eltdx/src/mac/services.rs. Both public MacClient::bars and
MacExClient::bars call this same composition with their existing common query
and owner. Configs/builders expose with_xdxr_provider/without_xdxr_provider;
configuration remains lazy. Added reexports XdxrProvider, XdxrBatch,
XdxrContext and XdxrSourceRequest. No Python source was changed in this stage.
The original Python client/adjustment source was read to preserve the order of
refetch/provider/error propagation and to record exact T27 legacy differences.

Healthy QFQ, None/Hfq, and empty initial rows return their original collection
without refetch or provider invocation. Nonempty invalid server QFQ first
refetches None bars with identical identity/period/times/start/count/pages and
bar-time alignment, under the original operation. collect::bars_with_purpose
and MacOwner::exchange[_leased]_with_purpose pass the original authorization
to the existing worker/transport. Only the first refetch page is Fallback;
normal next pages are Page and do not consume another extra. Each actual retry
still consumes an extra through the existing ChildAuthorization. The anomalous
QFQ is remembered as the cause of an exhausted fallback allowance.

Explicit provider receives exact MAC identity and owner-free parent control.
XdxrBatch contains events plus optional actual provider ResultMeta; the default
stamp only labels injected events. Callback errors propagate. No automatic HQ
client, guessed exchange/cache or hidden source lookup was added. External
providers must use source_request/start_attempt for actual connect/send work
and record_bytes for streamed input. This reuses OperationSponsor's same
reservation gate, parent deadline, extra allowance and wire-attempt bound;
metadata_authorization delegates to the same factored source gate. In-memory
providers consume no synthetic physical attempt.

The callback runs in ex::worker::run with the same owner registry and serial
slot. It must poll check_boundary and bound its I/O by remaining(). A callback
that cooperates can confirm cancellation/close/timeout. An uncooperative
blocking callback cannot be force-killed: CancelUnconfirmed retains the worker
slot until actual retirement. It may not synchronously reenter the same MAC
owner. No second socket loop, executor, task registry or operation was added.
Provider event Vec capacity and retained meta heap are charged against the
same parent bytes, not only event length. Provider raw ranges are validated,
Omit drops provider raw, RetainPayload drops provider frames, RetainWire keeps
allowed evidence. The budget constrains SDK/provider accepted retention;
arbitrary external code remains responsible for honoring the context contract.

Pure local math is the stage81 util implementation. It gains source_indices
and a crate-private shared-events accessor. Source-index mapping preserves
stable order and duplicate occurrence identity when selecting raw references.
Invalid events keep reasons, finite nonpositive outcomes remain inspectable.
No provider, empty provider/None series, or no applicable factor falls back to
the original QFQ with AdjustmentFallbackUnavailable. Still-bad computation
falls back with AdjustmentStillInvalid. Strict mode returns MetadataUnavailable
or Protocol respectively with adjustment context, and still records final
metadata. Required refetch/connection/decode errors, provider failures,
cancellation and timeout are propagated, never downgraded.

A successful repair requires at least one actually applied finite event and
valid effective OHLC. This intentionally does not silently call an unchanged
raw series successful QFQ when all events were skipped. T27 must preserve the
old Python nonempty-but-all-skipped event behavior separately: its existing
algorithm may accept the None series when its prices happen to be valid.

### Raw and provenance

ResultMeta gains private MAC-only with_raw_row_selection and with_raw_policy
helpers. Original QFQ, unadjusted refetch and provider payloads/frames/sources
stay in chronological source order. Primary row references select only the
returned original or locally sorted refetch rows. Every returned row shares
one aggregate meta; per-row record_raw points to its actual wire record and
original source f32 bits remain untouched. Detailed adjustment events share
one Arc. diagnostics.latest_meta keeps the final result, including strict
fallback failure. Normal no-repair bars retain their prior page-level meta.

Manual reading exposed a real source-level mismatch in the preexisting EX
raw projection: decoder pairs are (start,end), while shared RawData triples
are (payload,offset,length), as HQ already uses. Corrected only the shared
projection in ex/models.rs to end-start. Updated authored ex_wire/mac_session
and new extended_services expectations; decoder pair APIs remain start/end.
Added the direct offset+length payload slice assertion to the existing EX
projection fixture source. No test was executed to discover or verify this.

### Authored fixtures, all unexecuted

Added tests/support/extension_peer.rs, a std-only literal setup/request/frame
fixture shared by the two new suites. Synthetic login text is explicitly not
production authentication/association evidence.

extended_services.rs has13 authored public functions: healthy/non-QFQ no
fallback; successful f64 repair/raw/provider provenance; absent provider;
strict unavailable/still-invalid; empty/all-skipped events; invalid finite
repair evidence; provider Io/Cancelled/Timeout; empty/malformed raw refetch;
zero extra before send with original cause; multiple fallback pages using one
extra and accurate raw row mapping; provider reserved-memory cap; explicit
external-source shared extra gate versus in-memory events; provider raw policy
and malformed range rejection. Some functions contain multiple cases.

extended_loopback.rs has7 authored public functions: cancelled half-frame
unadjusted refetch; cooperative provider cancellation; owner close during
provider; uncooperative cleanup/gate retention and subsequent true retirement;
exact parent deadline without renewal; actual MacExClient login/rewrite/repair;
raw refetch failover with repeated setup and both physical sources retained.
Channel gates establish observation points; no timing sleep is used as proof
of cancellation or source ordering. No function was compiled/collected/run.

### Remaining full T26 and later scope

Implement board_summary/all-members, full-candidate money/volume ranking,
N-day histories; pinned same-source complete/truncated downloads; MAC-EX
goods/EX count/global directory fallback, bounded market-range validation,
owner/source/profile/generation cache snapshots and refresh, unique macro
resolution and all remaining public methods. Broaden extended_loopback to
actual ExClient and remaining directory/download/ranking lifecycles; these7
functions do not complete that whole suite requirement. Finish remaining T25
simple-method/schema coverage; production MAC-EX login and optional-boundary
acceptance evidence remains a final allowed evidence-phase gap, not fake ACKs.
Then implement full T27 legacy Python migration, T28-T30 consumer/CI/package/
docs sources before any unified verification.

No tests/build/check/clippy/doctest/collection/dependency resolve/download,
network probes, compiler/AST/format/diff checks or equivalents ran. Source
reads/edits and ordinary Git review/status only. Hooks-disabled checkpoint
commits are recovery snapshots, not a claim of passing verification. No
subagents, push, tag, publication or merge.
