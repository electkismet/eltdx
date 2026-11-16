# T09 — explicit protocol parse context

Status: implementation and test sources authored; execution deliberately deferred.

## Authority and scope

Read `task-09-brief.md`, `docs/rust-sdk/development-plan.md` T09,
`docs/rust-sdk/public-api.md` §§7–8.3 and
`docs/rust-sdk/execution-and-compatibility.md` §4, plus current protocol/SDK/runtime
source. Source writes began only after the controller's explicit GO following the
T07 gate. No subagents were spawned. Controller ledgers/status were not edited.

The implementation preserves the existing `CommandResponse` variants and old
parser signatures. It adds a context path through the same parser bodies rather
than copying field-decoding algorithms or temporarily overriding a global table.

## Implemented contracts

- Protocol owns `PricePrecisionSource: Send + Sync`, `ParseOptions<'a>`,
  `ParseRawPolicy::{Omit, RetainPayload}`, `RecordRange` and `RecordEncoding`.
- Aggregate signature is exactly
  `CommandResponse::parse_with_context(CommandRequest, bytes::Bytes, &ParseOptions<'_>) -> Result<Self, ProtocolError>`.
- Every precision/raw-bearing command has a `*_payload_with_context` entry
  that calls the common body also used by its legacy wrapper. All quote helpers,
  the refresh XOR view and today/history/recent minute helpers receive that
  context. The explicit path queries only its injected source.
- Security-list registration is confined to the legacy branch. Old wrappers
  preserve their existing numeric projections, payload contents and eager hex.
  Legacy decimal lookup and its divisor/provenance are captured together.
- A missing injected decimal returns `InvalidData` with context
  `"price precision"`; an unrepresentable u16 divisor returns the same context.
  No implicit code-prefix/default fallback is added to the explicit path. A
  captured source may implement its own audited fallback; the resolver's decimal
  alone does not assert a verified precision provenance.
- Explicit Omit returns empty diagnostic root payloads and skips record hex
  encoding/allocation before it happens. Explicit RetainPayload shares the
  supplied Bytes for page payloads and also suppresses eager row hex. Legacy
  Arc<str> trade hex shape uses one shared empty sentinel per explicit page,
  with no record-sized hex allocation or encoding. Semantic native/unknown
  fields are retained separately; Omit copies only those required fields so a
  small field slice does not keep a full diagnostic frame alive.
- Records carry page-relative ranges. Refresh ranges use `RefreshXor93` and
  on-demand conversion applies XOR only to the requested range. The complete
  XOR-decoded buffer is temporary on the explicit path; its legacy diagnostic
  field is empty. Plain range bytes share the retained payload. Minute-aux
  ranges are on the series, preserving the enum variant shapes used by the
  Python adapter. File content and sparkline values also have ranges.
- Exact quote numerators, denominator, applied divisor and resolved decimal
  survive integer floor projection in `QuotePriceRaw`. Category bid/ask deltas
  are also preserved. Minute records preserve existing exact price/average
  integers plus applied divisor/decimal. Bars now preserve packed time bytes
  and the formerly discarded delisted suffix. Historical fund flow preserves
  its eight custom-float u32 words and unknown prefix; money flow preserves its
  unknown fixed header. Existing scalar/native f32 byte fields remain present.
- Trade milli-yuan conversion remains independent of quote decimal metadata;
  each trade additionally exposes its audited command-specific denominator.
- SDK `hq::decode::decode_reply(RawReply, RawPolicy, &dyn PricePrecisionSource)`
  returns `DecodedReply` with request, parsed response, source identity/host,
  one shared payload and optional original `ResponseFrame` for RetainWire.
  Omit retains neither diagnostic payload nor frame. RetainWire maps to protocol
  RetainPayload while framing stays in the SDK adapter. `record_bytes` and
  `record_hex` use the record ranges on demand.
- The SDK manifest adds only `bytes.workspace = true`; no lockfile was changed
  or regenerated. No Python output fields were added.

## Owned files

- `crates/eltdx-protocol/src/context.rs`
- `crates/eltdx-protocol/src/context_tests.rs`
- `crates/eltdx-protocol/src/{lib,response,unit}.rs`
- `crates/eltdx-protocol/src/commands/{auctions,corporate,delisted_bars,history_fund_flow,klines,limits,minutes,money_flow,quotes,resources,security,session,trades}.rs`
- `crates/eltdx/Cargo.toml` (one authorized dependency line)
- `crates/eltdx/src/hq/{decode,mod}.rs`
- `crates/eltdx/tests/precision_isolation.rs`
- This report.

No edits were made to SDK root lib/session/operation/executor or runtime files;
those were owned by T08/controller and were excluded from T09 commits.

## Test sources authored, not executed

Protocol context source cases cover all four quote branches; exact pre-division
price data; XOR record ranges; missing and unrepresentable precision; read-only
context security lists versus legacy registration; all three priced minute
branches; Omit/Retain sharing; on-demand hex matching legacy hex; native f32
NaN payload/signed-zero bits; scalar/unknown retention in corporate, finance,
limits, ordinary/delisted bars, auction, current/historical funds, native minute
aux/sparkline, handshake, file content and current/historical ticks; and range
overflow/unavailable payload errors. Native-bit assertions compare integer bits,
not NaN float equality. Payloads are labeled synthetic, not server captures.

SDK integration source cases interleave two independent immutable precision
sources for the same security and decode pages concurrently with a barrier.
Context security-list decode is interleaved and checked against the prior legacy
global value. Adapter unit source exercises Omit/RetainPayload/RetainWire ownership,
shared pointers, preserved request/session identity and heartbeat unknown bytes.

## No-execution evidence

No tests, test collection, builds, cargo check, clippy, doctests, compilers,
network probes, AST/compile scripts, format checks, diff checks or equivalent
verification were run. Work used source reads/searches, source-only edits,
`git status`, exact-file `git add`/`git commit --only` and disabled hooks.
No push, tag, publish or merge was performed. There is no compilation, runtime,
test-passing or formatting claim in this report.

Recovery checkpoint: `ae11c2a` (explicit contexts, raw ranges, adapter and initial
source cases). Subsequent task completion commit contains the additional source
cases/self-review adjustments and this report; its exact SHA is in the handoff.

## Self-review and handoffs

- Final source-only review covered the context/range helpers, raw-retention
  branches, explicit trade hex sentinel, precision divisor boundary and authored
  synthetic cases. Reference-byte operations were made explicit and a redundant
  legacy-quote divisor cast was removed. This review did not execute code.
- T10 must pass the single immutable snapshot captured by OperationContext to
  `decode_reply`; this adapter intentionally does not install metadata, publish
  a store, read a clock or coordinate page/chunk/fallback execution. The trait's
  immutable-operation contract is documented, but implementers must honor it.
- T10 may map protocol `InvalidData { context: "price precision", .. }` to SDK
  `MetadataUnavailable`. Missing metadata is never replaced by the legacy table.
  The existing u16 divisor representation supports decimals through 6 and
  explicitly rejects larger scales; audited wire metadata normally supplies
  smaller scales. Legacy saturation remains legacy-only.
- For new Price projection, use exact quote numerator divided by
  `wire_denominator * divisor`. Minute price uses existing `price_raw` with
  denominator `100 * price_divisor`; average uses `avg_raw` with denominator
  `10_000 * price_divisor`. Preserve the raw numerator before legacy floor.
  Bar milli integers and trade denominators retain their own command contracts.
- Vector response variants have no root payload field. The accepted SDK adapter
  owns the retained payload once for them; protocol-only consumers that want
  diagnostics must keep their input Bytes alongside the returned ranges.
- T11 owns context-specific snapshot identity/reordering and input occurrence
  semantics. T09 preserves the current request-order splitting and existing
  foreign-singleton classification. The shared snapshot-record parser is the
  extension point; select slices/order in its caller without duplicating field
  decoding. No T11 policy work was folded into this task.
- New protocol model fields are additive for native callers. Python's existing
  explicit field projections were inspected and are unchanged, including its
  exhaustive minute-aux variant pattern.
- Source inspection cannot establish compilation or runtime correctness. All
  authored cases and appropriate workspace checks remain for the controller's
  unified validation after all T01–T30 implementation code is complete.
