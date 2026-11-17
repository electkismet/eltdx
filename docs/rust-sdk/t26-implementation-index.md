# T26 implementation and compatibility handoff

Stage86,2026-11-17, source base27adc95. T26's implementation and fixture sources
are complete. No test, collection, build, check, dependency, network, compiler,
AST, format or diff-check verification has run. Runtime, production protocol
and packaging acceptance remain outstanding until allT01-T30 sources exist.

## Composition map

| Public behavior | Single shared implementation | Authored source evidence |
| --- | --- | --- |
| MAC and MAC-EX bars with QFQ recovery | mac/services.rs; existing collect::bars, mac/util and injected XdxrProvider | extended_services.rs:13 public functions; shared provider lifecycle in extended_loopback.rs |
| Board all-member summary | mac/boards.rs; quote/member collector and named native fields | extended_boards.rs:16 public functions across summaries/rank/history |
| Full-candidate amount/volume/net ranking and stable ties | mac/boards.rs; same summary and fallible stable ordering | third-candidate winner, both directions, missing/nonfinite fields and primary raw selection |
| N-day board history | mac/boards.rs; shared collect::bars_page and percentage arithmetic | older target pages, distinct dates/duplicate observations, required failures and full caps |
| Same-source file download and strict complete file | mac/download.rs; shared resource_legacy::DownloadCursor | extended_downloads.rs:17 public functions, absolute EOF/resume/caps/cancel/close/failover |
| Global count, native goods and EX directory fallback | mac/catalog.rs, shared MAC/EX commands and decoders | extended_catalogs.rs:32 public functions across directory/cache/macro |
| Immutable catalogs and explicit refresh/clear | ex/catalog.rs cache; mac/catalog.rs source composition | private cache6 functions, clear epoch, independent owners, replacement source and retained Arc |
| Unique market38 macro selection, quotes and bars | mac/ex_services.rs; shared quotes and QFQ services | exact code/suffix/name, ambiguity/missing, duplicate occurrence identity,80+1 batches/raw |
| Source-order MAC-EX quote_list | mac/ex_services.rs; complete goods snapshot then min(count,80) | actual continuation/total, unsupported sort rejection, empty/beyond-end no quote request |
| Actual EX/MAC/MAC-EX lifecycle integration | existing shared owner/session/worker/frame machinery | extended_loopback.rs:14 public functions, independent92B EX setup, partial frames, generation, queue timeout and lease release |

These counts are authored function counts, not executed or passing cases. Some
functions contain several scenarios. Existing ex_session/mac_session/mac_wire
sources cover the prerequisite commands and owner mechanics; T26 does not add
a second transport, cache namespace shared across owners or Python dependency.

## Operation, source and result rules

Every Rust composition reuses the original operation's absolute deadline,
cancellation, physical authorization and extra allowance. Bounded page/record/
byte protections are additional limits, not pagination completion evidence.
Catalog/macro operations opt into total logical-request admission, including
metadata, fallback and provider requests. Allocation/retained provenance is
charged before the corresponding new retained result where specified.

Goods fallback, accepted directory continuations, macro consumers and accepted
file transfers bind endpoint plus socket generation. Unaccepted initial work
may retry/fail over; accepted source loss is an error. Failed required reads,
cancel, close and deadline cannot become a successful empty or partial result.
Uncooperative provider cleanup retains the registered worker's slot until
actual retirement and reports CancelUnconfirmed.

QFQ recovery retains original server bars, None refetch, provider events,
applied/skipped factors and diagnostics. Without usable factors it returns
original QFQ with an explicit diagnostic, or an error under strict policy.
Full-board sums retain all occurrences and missing/nonfinite evidence. Rust
N-day ranks require a complete distinct-date window. Files need actual short/
empty EOF and consistent absolute size; metadata size alone is a hint. A full
cap returns LimitReached and accurate next_offset; strict full-file requires
start0. Raw primary row mappings select only returned rows while auxiliary
payloads, frames and individual physical sources remain in observation order.

See [catalog handoff](catalog-implementation-index.md) for scan/count fences,
unsorted market handling, cache keys/bounds/epochs, refresh scope and no server
atomicity guarantee. See [MAC handoff](mac-implementation-index.md) for simple
schemas, native units/unknown fields, profiles and unresolved login evidence.

## T27 legacy Python policy

The Python entry points must call the shared business sources through explicit
legacy adapters. Their public behavior remains the existing baseline:

- Per-wire timeout and original attempt/page policies; no new Rust total8s,
  shared request ceiling or automatic strict complete-collection cap.
- QFQ's old nonempty-but-all-skipped events may accept a valid None series;
  preserve provider/refetch order and original exception boundaries.
- Ranking rounds to2 decimals, zero reference yields0 and missing fields use
  old defaults. Historical TransportError handling and shorter windows remain.
- File download returns partial bytearray, obtains metadata for size<=0 and
  stops at the old declared-size/short/empty rule, without Rust EOF strictness.
- Macro full codes bypass lookup; preserve old case/input/errors/cache, no new
  TTL, exact request order, EX market lookup and ignored quote-list sort args.
- EX fallback `_to_goods_items` enumerates each returned Python page from0,
  uses info.code or info.name, applies runtime Python float and keeps its
  variable-length numeric list, including parseable nonfinite values. Rust's
  complete-snapshot index, finite f32 placeholder and fixed array are different
  new-API policies and must not leak into that projection.
- Preserve dataclasses, raw dictionaries, installed extension exception/cause,
  custom transport/model hooks, host configuration, reconnect-after-close and
  independent owner/cache domains.

## Remaining acceptance and work

T25 still needs the remaining simple-method/schema source coverage and its
handoff. T27 is the full extension Python migration; T28-T30 add independent
consumers, feature/platform/package/CI sources, coverage and maintenance docs.
Only after all those sources are complete may unified verification begin.

MAC defaults currently require all three setup replies. Optional trailing
replies need production association/order/fence evidence. MAC-EX has no proven
production LoginReplyRules default and fails Protocol/Handshake without explicit
rules. Synthetic positive/negative replies establish fixture behavior only.
These evidence gaps stay open for the final permitted collection phase. They
cannot be resolved by invented ACKs or by relabeling source coverage as usable
production support. No push, publication, tags or merge occurred.
