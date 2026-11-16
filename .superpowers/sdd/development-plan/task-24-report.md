# T24 Stage A source checkpoint report

## Stage B source completion — 2026-11-17, base 3bc08b4

**T24 implementation source scope is complete; no execution acceptance has
passed.** This section supersedes older partial/GO-pending statements below.
The first Stage B implementation commit is3bc08b4. The current checkpoint adds
the final implementation seams, source cases, example and domain handoff.

- `profile.rs` provides an immutable per-Session `SetupProfile` negotiation
  interface and the concrete EX profile. Only that EX implementation emits
  the92B setup; T25 must explicitly construct a different MAC/MAC-EX profile.
  Crate-private `session/control/worker/transport` can be reused without
  copying a second socket gate, budget, owner or frame implementation.
  `Work::Encoded` admits an already checked internal profile command through
  the same authorization/worker; no public bare socket API was added.
- Shared `OperationSponsor::remaining_bytes` gates every read before acquiring
  more bytes. The complete header's declared decoded size is charged before
  body acquisition/decompression allocation. Existing frame limits still
  validate the header before body accumulation. Wire and decoded work charge
  the existing parent's byte counter, never a renewed child allowance.
- Source review kept strict command decoding before persistent socket
  publication, checked cancelled publication under the same session lock used
  by caller retirement, and filtered healthy snapshots against candidates and
  subsequent failures. Normal child completion does not consume the parent's
  terminal cleanup budget. Profile/frame errors retain meaningful phases.
- Authored `tests/ex_session.rs` contains15 unexecuted public-client source
  functions: lazy defaults/clone-close/no-reopen; all11 typed methods/nonempty
  values/compressed setup; raw default versus explicit Omit and Unknown versus
  RequestedCount; transport-only failover with old-socket EOF barrier; strict
  decoder Protocol without backup connect; partial-setup cancellation;
  shared-close during partial business body; TcpOnly versus DataReady and
  probe socket cleanup; last-owner Drop; pre-body byte cap; cancellation then
  fresh operation without old-response leakage; concurrent different layouts;
  parent discovery cancellation; two owners' range sequences; shared extra
  attempt limit preventing physical backup connect with prior failure source.
  Race cases use channels/barriers, not sleep-based readiness assertions.
  Existing Stage A `ex_wire` carries literal requests and exhaustive pure frame
  fragmentation/sequence-wrap/schema/time/config cases.
- Added `examples/ex_query.rs`, required only feature ex, using std+eltdx and
  an explicit numeric endpoint; it reads markets/one directory page and closes
  even if the business operation returns an error. Example is not compiled/run.
- Added/mirrored `docs/rust-sdk/ex-implementation-index.md` with complete
  11-method and management/config maps, raw/native/page policy, ownership,
  physical-attempt proof reasoning and T25/T26/T27/T28 handoff. Development
  plan/status and recovery ledger now state23 source-complete/2 partial/5
  unstarted. Every execution acceptance checkbox remains untouched.

Remaining evidence is explicit: production magic/method/sequence capture rules
and a legitimate absent-quote marker are not proven; responses retain an
association-unverified diagnostic and a malformed/short quote is never None.
Dependencies/lock, compiler/type checks, independent feature consumers,
platform close/rename behavior and real-server evidence are still final-phase
work. T25's MAC login/OptionalReply/simple schema breadth, T26 compositions,
T27 Python timing/leniency/futures35-slots-vs34-names, and T28 public
visibility/serde/consumer work are not described as completed T24 behavior.

No tests, collection, build/check/clippy/doctest, dependency resolution,
download/network, compiler/AST/formatter/diff-check or equivalent execution
verification occurred. Only literal source authoring/reading, ordinary diffs,
Git status and hooks-disabled exact-path checkpoints occurred. No agent, push,
tag, publish or merge was used. Goal remains active without a token quota.

## Stage B first checkpoint — 2026-11-17, base c648799

The user's existing all-T01–T30 execution authorization supersedes the old
Stage A controller-GO wording below. T24 remains partial. This checkpoint
contains uncompiled implementation source, not accepted runtime evidence.

- Added `ex/client.rs`: lazy independent `ExClientBuilder`/`ExClient`, immutable
  configuration, shared irreversible close, connect/ensure/disconnect, all11
  typed business methods, diagnostics/host snapshots, explicit probe/discover/
  refresh/ping/best-host entry points. No implicit file/environment read,
  resolver, scan, probe, Python interpreter or HQ connection at build.
- Added `ex/session.rs`, `control.rs`, `worker.rs`, `transport.rs`: reuse the
  existing SessionOwner and OperationContext/AttemptAuthorization, with one
  owner-local full request/reply gate and registered owner-free jobs. No
  synthetic HQ CommandRequest or separate operation budget is introduced.
  Workers perform bounded async connect and exact remaining-frame reads, then
  convert a successfully completed socket to nonblocking std I/O before their
  runtime is dropped. Strict command decode precedes publishing that socket.
  A failed attempt owns and destroys the old socket before another reserve/
  start, protocol/decode failures never host-hunt, range sequences are held
  through the complete response or socket destruction. Already pending idle
  unsolicited bytes retire the connection as Protocol, with association
  capture rules still explicitly unverified rather than guessed.
- Added `ex/probe.rs`: one registered runtime owns a JoinSet bounded to1..8
  probe sockets. All children inherit the original sponsor/attempt accounting,
  and a child timeout is min(parent deadline, explicit per-host probe timeout).
  DataReady performs full setup+markets+strict decode. TcpOnly results carry
  their own mode and cannot populate data health or data ranking. Parent
  cancellation/deadline/limit errors terminate the whole discovery.
- EX is registered under its existing optional feature, activating the shared
  optional Tokio/flate2/serde_json dependencies without F10/HTTP/Python. No
  dependency resolution, download or Cargo.lock rewrite was attempted.
- Added `PageCompletion::Unknown`, source-count offset advancement, true socket
  generation provenance and association-unverified diagnostics. Single-page
  short/empty responses do not assert EOF. Raw defaults distinguish an omitted
  per-call raw option from an explicit Omit through a narrow OperationOptions
  presence bit; existing default/legacy behavior remains as authored.

Pending before T24 source completion: author actual public-client lifecycle/
loopback source cases, review new code against specification and Stage A
seams, finalize management/config/method handoff and document any remaining
capture limits. T25 must extend the explicit profile seam for MAC/MAC-EX
handshakes; it must not invoke the EX setup on MAC-EX. T26 owns collections and
catalog composition. T27 owns the Python EX legacy adapter. None is claimed
complete by this checkpoint. Global counts stay22 complete-source/3 partial/5
unstarted. No tests, collection, build/check/clippy/doctest, dependency/network,
compiler/AST/formatter/diff-check, agents, pushes or publication occurred.

Status: **partial T24; Stage A authored, Stage B pending explicit controller GO.** This is not a completed T24, public-client implementation, successful build, or verified network implementation.

Review base: controller-authorized `b6e4beb`. The shared worktree advanced while other authorized tasks wrote their own files; this task did not reset the branch or include their edits. Stage A code commit: `1220bf22da0171a1eff097a6e29b58a6e78e2a3f` (`feat(ex): author Stage A pure wire codecs and checked models`). Hooks were disabled for the exact-path commit.

## Scope and files

Only new owned code paths were authored:

- `crates/eltdx/src/ex/mod.rs`: pure module organization and EX error context helpers; no owner/session facade.
- `crates/eltdx/src/ex/query.rs`: checked market/code/identity/period/category and command-specific query values.
- `crates/eltdx/src/ex/models.rs`: immutable raw/native DTOs, shared ResultMeta alias, raw projection, page result integration seam.
- `crates/eltdx/src/ex/time.rs`: Gregorian/packed-minute/daily decoding, dated rollover, synthetic close provenance, TimeOfDay.
- `crates/eltdx/src/ex/frame.rs`: fixed setup, 16-byte header, bounded exact zlib decode, incremental one-frame accumulator, association inputs, pure session sequence lease, explicit EX/MAC-EX profile choice.
- `crates/eltdx/src/ex/commands.rs`: all eleven request encoders and response decoders.
- `crates/eltdx/src/ex/hosts.rs`: numeric candidates, strict host parsing, probe/discovery values, immutable snapshots, pure ranking.
- `crates/eltdx/src/ex/config.rs`: explicit config sources/load/save and immutable EX configuration.
- `crates/eltdx/tests/ex_wire.rs`: unexecuted source cases.

This report is the only additional task-owned documentation path. No Cargo manifest/lock, root lib, shared types, owner, operation, runtime, metadata, Python, controller progress, or status file was edited by this task. No session.rs or ex_session.rs was added because that would imply the Stage B transport surface.

## Authority read

Read task-24-brief first, `docs/rust-sdk/development-plan.md` T24, `extension-api-spec.md` sections 1 and 3, public owner/error contracts, existing operation admission and shared metadata/time/raw types. Read Python EX client, frame, hosts, datetime, enums, models, binary helpers, transport/sync, all eleven command classes, and `_bar_time.py`. Referenced existing protocol frame zlib checks as implementation evidence. No network evidence was acquired.

The local skills instruction explicitly says dispatched subagents ignore using-superpowers. The task-specific no-execution and no-subagents instructions were followed. No subordinate agent was spawned.

## Eleven-method source map

The following are **codec functions**, not implemented ExClient network methods. Each uses immutable checked query inputs and outputs `Decoded<T>` for owner-supplied provenance.

| Promised method | Query/default | Request / response source implementation | Response layout |
| --- | --- | --- | --- |
| markets | none | markets_request / decode_markets, 0x23f4 | u16 count + 64-byte rows; category+market zero padding skipped; wire count retained |
| instrument_count | none | instrument_count_request / decode_instrument_count, 0x23f0 | u32 at byte 19; exactly 23 bytes |
| instruments | start required, count 100 | instruments_request / decode_instruments, 0x23f5 | returned u32 offset+u16 count; 64-byte rows |
| quote | id | quote_request / decode_quote, 0x23fa | 10-byte UTF-8 identity+4 unknown+136 known; exactly 150 bytes |
| quote_list | market, category HK/Futures, start 0, count 80 | quote_list_request / decode_quote_list, 0x2400 | u16 count+300-byte slots; category controls independent field schema, not request bytes |
| bars | id, period, start 0, count 700, Start | bars_request / decode_bars, 0x23ff | 18 bytes+u16 count+32-byte rows |
| bars_range | id, ascending start/end dates, Start only | bars_range_request / decode_bars_range, 0x240d | per-session 1..255 leased sequence; 12 bytes+u16 count+32-byte packed-minute rows |
| minutes | id | minutes_request / decode_minutes, 0x240b | 10-byte identity+u16 count+18-byte rows |
| history_minutes | id, date | history_minutes_request / decode_history_minutes, 0x240c | date precedes id in request; 18 bytes+u16 count+18-byte rows |
| transactions | id, start 0, count 1800 | transactions_request / decode_transactions, 0x23fc | 14 bytes+u16 count+16-byte rows |
| history_transactions | id, date, start 0, count 1800 | history_transactions_request / decode_history_transactions, 0x2406 | date precedes id; response same 16-byte header and record schema |

All count setters reject zero and values above 65535 before encoding. Quote-list start is checked u16; transaction starts are checked against i32::MAX. Code is nonempty UTF-8, maximum nine **encoded bytes**, with NUL rejected; no `9s` truncation. Date constructors validate Gregorian values and the four-digit YYYYMMDD range. Known periods 0..11 are explicit; other u16 codes require Wire. Unknown periods have no derived End duration and decoding records a diagnostic for source-compatible date interpretation.

## Management/config map and remaining implementation

| Contract | Stage A | Stage B |
| --- | --- | --- |
| builder/build/config | ExConfig contains default 8s, raw Omit, candidates, auto_failover, frame limits, checked setters; construction is pure | ExClientBuilder and ExClient ownership, immutable config getter, lazy build |
| connect/ensure_connected | fixed EX setup+pure response accumulator available | admission, connect, full setup response validation before Ready, idempotence |
| close/disconnect | no replacement lifecycle state machine created | confirmed close, cancellation, cloned owned services, no reopen |
| diagnostics/host/hosts/healthy/unhealthy | immutable DTOs and candidate ranking | owner snapshots; never present TCP success as DataReady |
| from_best_host/discover | DiscoveryOptions max_workers<=8, probe default5s/DataReady | bounded concurrent probing+setup+markets sharing caller operation |
| ping_all/probe_endpoints | ProbeOptions, HostProbe types | actual networking and limits under owner/operation |
| refresh_hosts/refresh_endpoints | configuration/ranking helpers | owner-scoped candidate/exclusion refresh and cancellation |
| config::server_file_path | explicit data directory joined with ex_servers.json | caller selects explicit directory; no hidden platform scan |
| config::load_hosts | explicit > explicit environment snapshot/process opt-in > explicitly provided file > defaults | builder must never call automatically |
| config::save_hosts | explicit file mutation only, unique sibling create_new+sync_all+rename, cleanup owned temporary | integration only; no automatic save |

Configured empty/invalid host sources fail instead of falling back; an absent env or nonexistent optional file permits the next source. Config endpoints are numeric standard socket addresses (no DNS); a bounded resolver is a separate future seam if hostname support is required. Legacy file uses `hosts` bare addresses, schema_version=1; save rejects non-7727 addresses rather than lose port information. JSON source parsing is explicit. IPv6 is bracketed in saved bare-host strings. Snapshot names `PROBE_VERIFIED_EX_HOSTS` and defaults are historical candidates, not current health evidence.

## Frame, profile and association evidence

- Exactly 92 setup bytes copied from Python `_EX_SETUP_PKG`; explicit WireProfile::Ex chooses setup, WireProfile::MacEx does not. Nothing infers the profile from port.
- Response header is exactly `<IIIHH>` (magic/sequence/method/u16 encoded/u16 decoded), and no parser reuses HQ header layout.
- Length limits checked before body acquisition/allocation; local positive limits <=65535 may be smaller. Decompression allocates declared output+one detection byte, max65536. Equal lengths are the source-defined uncompressed case. Compressed case requires StreamEnd, all input consumed, exact declared output. Invalid zlib, trailing garbage, truncation and length bombs fail.
- FrameAccumulator accepts only the current header/body and reports input bytes consumed; a coalesced next frame remains with its caller. It becomes unusable after push failure. Finish on an incomplete frame fails. This is pure source reasoning, not a socket test.
- `ENCODED_MAGIC=7654321` is Python encode_header evidence only. Python production receive does not validate header association. Production magic/method/sequence rules and exceptions still require captures. Association accepts explicit expected fields and reports whether all were supplied; Stage B must derive them from evidence and issue unknown-rule diagnostics. Synthetic tests exercise mismatches but are **not production capture proof**.
- RangeSequence is per-instance, atomic and excludes a second in-flight range lease, wraps255->1. The lease must be held until complete response or socket retirement. It does not serialize other command types or own retries/budgets; Stage B must keep one full request/reply gate for every command.

## Native/raw/time preservation

WireFloat stores exact u32 bits including NaN, infinities and negative zero; f32/f64 are views. No global /100, /1000 or lot conversion. Native prices/counts/quantity/amount slots retain Unknown unit/evidence until a market-specific source proves more.

Instrument and market rows retain every one of64 bytes under Omit. Quote-list rows retain all300 bytes, including10-byte identity,140 named/unnamed payload bytes, and150-byte unused tail. Futures/HK schemas remain distinct. Five-level quote preserves all unknown slots within its complete150-byte layout even under Omit; undocumented trailing bytes are rejected after round1 review. Required scalars survive Omit; optional full row bytes for bars/minutes/transactions are Bytes slices under retention. project_raw reuses shared Bytes and row offsets in the existing RawData, validates ranges, and accepts caller remaining byte allowance without creating another budget or metadata type.

Ordinary bar position u32 and amount-f32 are two views of the same bits; final slot remains uninterpreted. Range final slot is settlement with explicit RangeSettlement source; its position is not promoted into a fictitious amount. Packed date/year/minutes follow source; daily15:00 carries SyntheticClose provenance. End changes only known intraday durations, including actual date rollover, while preserving source_time. Range End is rejected. Minute/history-minute timestamps intentionally have no guessed date. Transaction second is direction%10000 and nature direction/10000; illegal seconds fail Protocol, keeping Python's legacy coercion-to-zero separate for T27.

PageResult uses the shared ExtensionMeta alias and an internal checked construction seam. No codec chooses EndOfData or MoreAvailable from a short/empty row count. Decoded exposes actual wire_row_count and returned instrument offset for the owner's eventual page semantics, and filtered padding does not change consumed wire count. **The four-state page contract lacks an explicit Unknown/SinglePage completion value; controller must resolve representation of a successful short ordinary EX page without invented EOF or invented more-data evidence in Stage B.** Incomplete requires diagnostics and cannot silently become ordinary partial success.

## Source cases authored; none executed

`tests/ex_wire.rs` includes independent literal request goldens for all11 commands/defaults and nondefault boundary payloads; setup golden; every split position of header/body and two coalesced frames; strict association mismatches; invalid/truncated/trailing zlib/declared-length/cap cases; per-session range wrap and in-flight isolation; all command declared-row truncation and valid zero-count forms; quote unknown absence boundary; GBK+UTF8 replacement diagnostics; 64/300-byte retention and all futures/HK named slot offsets/native types; five levels; NaN/negative-zero; ordinary dual slot versus range settlement; daily/packed/Gregorian/bad minute/bad second/datetime rollover; raw sharing and caller allowance; host default/options/preference/invalid JSON/empty sources/explicit file roundtrip and overwrite source cases. hosts.rs also has an unexecuted pure ranking case.

These are authored Rust source, not collected/compiled tests or actual fixtures captured from servers. No claim is made that tests pass. Real transport fragmented response, stale response after failure, transport-only failover, close/cancel during handshake/request, owner clone and no-reopen cases belong to Stage B/ex_session and were not authored against a nonexistent transport.

## Self-review concerns and exact Stage B seams

1. Add feature-gated root registration for ex and optional workspace-pinned **flate2 1.1.2** and **serde_json 1.0.145** direct dependencies (ex feature). Existing bytes and encoding_rs are already direct dependencies. Cargo/root files intentionally untouched. No compilation performed.
2. Current RequestPermit::into_submission accepts HQ CommandRequest. Introduce/consume the coordinated T19 domain-neutral request descriptor or extraction hook, carrying EX command/profile/idempotence metadata, preserving shared AttemptAuthorization begin/start/refund exactly once per physical attempt. Do not synthesize HQ requests. OperationContext must carry one remaining deadline through connect/setup/send/receive/decode and owner cleanup. This task did not create another authorization/accounting design.
3. Stage B owns independent lazy ExClient/Builder/service handles and SessionOwner-compatible lifecycle, full request/reply gate, socket retirement before retry, transport-only failover, endpoint exclusions, bounded discovery and cancellation/join. Parser errors must not trigger host hunting.
4. Frame accumulator methods need to be driven with exact remaining-byte reads. Only mark Ready after setup finish validates the complete response. Hold SequenceLease across full range reply or confirmed retirement. Attach source/endpoint/session ID and association-unverified diagnostics through Decoded::with_meta.
5. Public page completion evidence remains unresolved as above. Quote has no captured absence marker; decode_quote returns Some for a complete quote and Protocol for all currently unproved short forms. Add None only for an explicit independently supported absence response.
6. Python futures tuple assignment has34 names for a35-slot140-byte format. Source-known fields are placed at sequential named offsets; unnamed slot34 and other unknown slots survive in slot_raw. A captured futures response and T27 compatibility decision are needed; do not claim old Python parser successful behavior for the inconsistent assignment.
7. Config save uses Rust sync_all/rename; cross-platform replacement/failure behavior and file ACL equivalence have not been exercised. The source-only roundtrip case is intentionally unexecuted. Numeric-host-only Rust config and explicit data-directory API are intentional conservative boundaries, with hostname resolver behavior remaining an integration decision.
8. Advanced codecs are pure; owner-controlled advanced exposure and visibility audit remain Stage B/T28. Root module is not registered, so these source paths currently do not expose a runnable public client.

## Handoffs

- T25: reuse EX Frame/FrameAccumulator/limits with explicit WireProfile::MacEx; perform MAC-EX login/auth in its own profile; no EX setup for MAC-EX. Share NativeUnit/WireFloat only where ownership and API decision support it, preserve independent market/identity semantics. MAC period8 differs from EX period8. No duplicate EX owner or budget.
- T26: wire counts/ranges and returned offsets are available; apply explicit PageOptions and shared deadline for aggregates; do not treat ordinary short pages as EOF or sort/deduplicate them. Do not apply HQ units to EX. Confirm catalog and range metadata before changing semantics.
- T27: separate legacy policy adapter needed for prefix parsers, absent short-quote behavior, invalid-second coercion, UTF8 request/GBK response specifics, historical15s timing, and legacy env/host defaults. No Python edits made.

## No-execution statement

No tests, test collection, cargo build/check/clippy, doctest, network probe, compilation, AST parsing, formatter, diff-check or equivalent validation ran. Only source reading/writing, ordinary source review, Git status/rev-parse, normal diff/stat viewing, exact staging and hooks-disabled commits occurred. Python was used only for literal source text edits, not importing/parsing/executing project code. This remains an uncompiled, unvalidated source checkpoint until the all-T01–T30 gate opens.


## Stage A fix round1 — scoped response to persisted review

Review base for this round: `83540ec` (the Stage A review head). Review read: `.superpowers/sdd/development-plan/task-24-stage-a-review.md`. Controller authorized only this fix round and adjacent two Minor items; Stage B remains pending. This section supersedes the original permissive command-trailer behavior.

- **Important complete-layout correction:** `counted()` now requires payload length to equal header length + declared row count × fixed row width, with checked arithmetic and existing wire ceiling. It rejects both partial suffixes and complete undeclared rows before producing records. This covers markets, instruments, both quote-list schemas, bars, range bars, minutes/history-minutes and transactions/history-transactions. The single-count response must be exactly23 bytes and the single-quote response exactly150 bytes. `finish()` only wraps an already layout-validated value; it no longer downgrades an unknown suffix to a diagnostic. Errors retain `expected_bytes` and `actual_bytes`. Zero-count payloads still succeed when their header is complete and no undeclared bytes follow.
- **Evidence/decision:** extension-api-spec §1.1 requires partial records to fail Protocol; §3.2 and Python command parsers provide complete header/row widths. Re-reading the single-count and single-quote Python sources supplies no independently documented trailer rule. Python accepting/ignoring extra bytes is not evidence those bytes form a legitimate Rust response trailer. There are therefore no accepted command trailers in this checkpoint. Any future exception requires a command-specific independently supported layout and source cases. Required unknown bytes *inside*64-byte records/300-byte slots/150-byte quotes remain preserved. Frame-level byte boundaries and subsequent frames are untouched.
- **Minor clock context:** malformed minute/second errors retain decimal `raw_minutes` and `raw_seconds`; transaction parse adds original `direction_raw` and record `offset`. ErrorKind remains Protocol and no invalid-second coercion is introduced.
- **Minor model getters:** InstrumentInfo now forwards readonly `market()` and `code()` to its preserved response identity, alongside `id()`.
- **Focused source cases:** each counted command/schema has zero- and one-record valid controls, every suffix length1 through its record width rejected, plus complete undeclared rows rejected. Count/quote suffix cases cover their scalar/single-record layouts. Additional cases assert minute/second/direction/offset context and identity getter equivalence. Previously authored full-layout, raw-tail-inside-record and zero-count cases remain the source baseline.
- **Scoped self-review:** reviewed only this round's ordinary source diff, its helper call sites, new source cases and report changes against the reviewed checkpoint. No untouched Stage A surface was presented as newly re-reviewed. The original review's Stage B concerns remain unresolved and unchanged. Controller/reviewer should re-review the fix diff against83540ec; no subagent was spawned.
- **No execution:** no tests, collection, build, check, clippy, doctest, dependency resolution, network, compiler, AST, formatter or diff-check ran. Literal source edits, source reads, ordinary Git diff, and hooks-disabled exact-path checkpoint only. Cases remain uncompiled and unexecuted.
