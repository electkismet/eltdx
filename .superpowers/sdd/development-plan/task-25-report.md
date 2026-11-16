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
