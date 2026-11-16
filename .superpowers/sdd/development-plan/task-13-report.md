# T13 implementation report — HQ K-lines, delisted bars and minutes

## Scope

T13 adds the Rust HQ service surface for M25–M40 and M98–M99 while leaving the
existing Python API and compatibility policy unchanged. `TdxClient` exposes
owned lazy `klines()` and `minutes()` services. Query and DTO types live under
`eltdx::types`; service implementations live under `eltdx::services`.

## Implementation

- `services/klines.rs` implements ordinary and delisted page/collect methods,
  all four many variants, checked wire offsets/counts, source-pinned page work,
  immutable metadata capture, cursor verification, and aggregate metadata.
- Ordinary collection advances by actual returned rows, continues after short
  non-empty pages, stops only on an empty page, reverses page order while
  preserving in-page order, and keeps an empty legacy raw projection.
  Delisted collection performs stable date ordering, preserves equal-date source
  order, and keeps the first page for the compatibility raw projection. Page
  payload/range provenance remains available through `ResultMeta` when raw is
  retained. Aggregate row references follow the delivered row order while
  payloads, wire frames and source stamps keep response order, including the
  terminal empty page. Repeated-page evidence is retained privately even in
  Omit mode so omitted diagnostic buffers cannot cause false repetition.
- `types/klines.rs` defines validated period, adjustment, kind, bar-time and
  page queries. Periods 8/9/13 remain parameterized; week/month/quarter/year
  remain distinct; category 22 is rejected as non-OHLC. End labels shift only
  known minute periods with Gregorian rollover; `source_time()` preserves the
  original timestamp for the different Python HQ same-day projection. Public
  start setters/getters use u16; internal offset arithmetic is checked u32.
- `services/minutes.rs` implements today/history/recent and their many forms,
  auxiliary buy/sell and volume-comparison series, and sparkline. Current date
  resolution happens once per operation; explicit dates do not add a current-date
  clock read (metadata freshness can still consult the injected clock).
  The 0537, 0FB4 and 0FEB layouts keep their own date, average and reference
  fields. Minute prices use preserved protocol integers plus metadata scale;
  auxiliary float values retain wire `f32` bits and unknown units. Sparkline
  identity, selector echo and bounded response length are validated.
- `MarketDateChoice` is shared by `MinuteQuery` and future HQ date queries.
  Client/module exports were extended without changing the Python bridge or
  adding a PyO3 dependency.
- OHLC captures already published owner-local catalogs without fetching them;
  continuation binds to that frozen metadata version and the once-resolved
  market date. Tokens are verified before acquiring a collection pin. Metadata
  publication, changed date, owner, identity or page size invalidate a token.
- Serde for auxiliary f32 fields reuses the existing finite-value serializer.
  KlineSeries does not claim Serialize for the opaque DataPage/cursor aggregate;
  the shared pagination serialization schema remains part of the later package
  and feature work rather than serializing internal cursor identities.

## Source cases authored

`tests/klines.rs` covers COMP-030 short pages (`offsets 0,2,4`), page-order
reversal, cursor continuation, stable delisted sorting with duplicate dates,
parameterized/calendar periods, index breadth fields, end-time leap-day
rollover, response-count protocol rejection, max-page limit rejection,
repeated-page rejection, wire-offset overflow, expired operation deadlines,
cancellation on a later page, many/cursor rejection, foreign owner/security,
changed period/page size/date/metadata, explicit start clearing continuation,
legitimate repeated times in distinct pages, duplicate batch occurrences,
invalid/truncated dates and records, unexpected trailing bytes, query candidate
validation, raw row ordering, original timestamp retention and empty batches.

`tests/minutes.rs` covers explicit/current date clock policy, duplicate batch
occurrence order, all three minute wire families, average/reference semantics,
auxiliary `f32` bit retention, sparkline identity/selector/window validation,
raw retention, valid empty historical series, empty batches, outgoing date-year
bounds, truncated records and unexpected trailing bytes. All payloads authored
here are synthetic independent small cases, not real-server captures.

## Self-review and compatibility hooks

The implementation was reviewed against `hq-api-spec.md` §6, the protocol
command parsers, Python `bars.py`, `delisted_bars.py`, `minutes.py` and
`_bar_time.py`, COMP-030/032/056, and the existing operation, metadata, cursor
and raw helpers. Ordinary-versus-delisted legacy raw projection remains
intentionally different for T18. Continuation tokens keep owner, endpoint,
page-size, resolved date, query fingerprint and metadata-version checks.

This is a source implementation checkpoint and self-review, not an independent
review or compilation/runtime approval. Live source-loss/epoch behavior and the
full feature matrix remain for the unified final validation phase. T18 still
owns the Python binding migration; its legacy request timing, aggregation and
same-day end-label projection must consume these hooks without applying Rust
public caps to existing Python calls.

No tests, builds, collection, clippy, doctests, formatting, AST/compile
checks, network probes, or equivalent verification were run. Runtime and
compile evidence remains deferred to the final T01–T30 validation phase.

## Files

- `crates/eltdx/src/services/{klines,minutes}.rs`
- `crates/eltdx/src/types/{klines,minutes}.rs`
- `crates/eltdx/src/types/time.rs`
- `crates/eltdx/src/operation.rs` (once-per-operation market date)
- `crates/eltdx/src/types/provenance.rs` (raw row permutation)
- `crates/eltdx/src/types/numeric.rs` (reuse existing finite-f32 serializer)
- `crates/eltdx/src/{client.rs,services/mod.rs,types/mod.rs}`
- `crates/eltdx/tests/{klines,minutes}.rs`

