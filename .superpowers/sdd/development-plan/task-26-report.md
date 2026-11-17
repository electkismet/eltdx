# T26 implementation report

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
