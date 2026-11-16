# T11 — 代码表、批量快照与缺失结果

Read this first: these are your requirements. Authority: `docs/rust-sdk/development-plan.md` T11, `hq-api-spec.md` §§1,3–5, `public-api.md` §9, `execution-and-compatibility.md` §§3–6, and `decision-register.md` §3. Read those sections and current T08–T10 interfaces before editing. The controller supplies GO only after dependencies are ready.

## Exact plan task

**Files:** new SDK `src/services/{mod,codes,quotes}.rs`, extend existing `types/batch.rs`, necessary domain DTO/query modules and client service getters; new `tests/{codes,quotes_batch}.rs`.

**Interface:** `quotes().snapshots(&[SecurityId], &OperationOptions) -> Result<SnapshotBatch>`; preserve each input occurrence/index. Missing behavior is shared, not Python-specific.

- Author source cases for complete/partial/reordered responses, repeated inputs, external response identities and conflicting duplicate records.
- Bounded binary recovery follows T02 fixtures, at most 32 additional recovery requests and the stricter operation-wide budget.
- Strict confirmed absence produces structured `MissingData` with identity and reason. AllowPartial retains an occurrence slot with a reason. Corrupt data, cancellation and connection failure always remain errors.
- Security full scans continue on short pages, stop on empty page, reject repeated/no-progress pages, and do not require a count call before paging.
- Final deferred commands: `cargo test -p eltdx --test codes --test quotes_batch --locked`.

## Complete domain scope and interfaces

Implement every `CodeService` method M01–M17 from HQ §5, including ordered all-market aggregation, filters/counts, ST aliases' underlying Rust operations and suspension via the actual 0x20 status bit. Implement QuoteService M18–M22 (snapshots/default and explicit missing policy, legacy, category, temporary-pinned depth). Permanent pinned refresh and subscriptions are T15; do not expose stub subscription APIs. Queries and DTOs use HQ §§3–5 exact field/getter/builder contract. `Security`, `Snapshot`, `LegacyQuote`, `CategoryQuote`, `QuoteLevel`, `QuoteUpdates` and their query types are under `eltdx::types`; owned Clone services are under `eltdx::services`. Client getters are lazy and share owner. Replace the T04 generic placeholder SnapshotBatch alias with the documented concrete `BatchOutcome<Snapshot>` without introducing a second batch type.

All public network entry methods capture Instant before validation and use T08 begin_context_at. Validate total input before copying: 65535 occurrences, checked 7-byte normalized HQ identity logical accounting against 2097152; bounded reserve for indices/results. Empty ids returns empty without metadata/connection work. Preserve duplicates and input_index; do not replace one occurrence's legitimate distinct result with another unless wire deduplication is explicit and provenance remains clear. COMP-028 expects A/B/A inputs to preserve independently returned A prices 10 and 12. Unexpected IDs or conflicting excess records are Protocol, not missing.

The first required request/page is normal ChildPurpose; recovery children are MissingRecovery, atomically charged by T08 both to local recovery and shared extra budget when attempts actually start. At most 32 extra diagnostics per recovery invocation; runtime retry/failover also consumes shared budget. Do not write another retry loop or reset deadlines per chunk. A single missing/empty response can confirm only the requested singleton after approved diagnostic behavior. Generic parse errors do not permit binary recovery: use typed ProtocolError context/reason or narrowly audited existing categories from `api/_batch.py`, preserving source chain. Full malformed scalar/truncated/compression failures stay Protocol. Author cases that distinguish marker/count structural recovery from unrecoverable corrupted fields.

Code pages: defaults CodeQuery start=0, limit=1600; CodeScanQuery PageOptions={1600,200,320000}, category=None. Wire max1600, explicit single-page limit zero follows the documented valid empty-window semantics; scan page size positive. Source order Shanghai→Shenzhen→Beijing, filter only after complete source order. Advance by actual received rows using checked u32 offset; empty page proves EOF, short page does not. Duplicate page/no progress/duplicate conflicting identity is Protocol. Reaching a cap without EOF is LimitExceeded, never a complete Collected. No eager count. Implement opaque cursor/version/query/owner/endpoint/metadata matching as public §9.2 requires; no fake EOF or caller-constructible offsets masquerading as cursor. General reusable pagination helpers belong here for T13/T14 to consume.

Use immutable T10 metadata snapshot throughout the operation, T09 decode adapter only, no global decimals. Security list context decoder must not mutate legacy process state; publish directory metadata only via owner store after full candidate validation. Quote scalar projections use preserved raw integers/divisors/float bits and source units; no reconstructed raw integers from rounded f64. Missing precision is MetadataUnavailable. Quote volume is documented hands; unknown fields/units remain unknown. Retain payload/wire at page level via Bytes/ranges, not per-row copies, default Omit avoids permanent record hex allocation.

Keep compatibility projection possible for T12. The one shared retrieval/recovery/pagination algorithm must expose a narrow internal legacy path using T08 Python profile and old protocol DTO/raw projection, not apply ordinary Rust's new batch caps to Python. Python snapshot recovery accepts old duplicate occurrence ordering, warning class/count and source error categories. Retain exact original raw scalars for its existing 27-stride tuple ABI; don't force binding to reparse payload or reverse normalized float projections. Do not add PyO3 to SDK. Native bridge belongs T12; do not edit Python facade in T11.

## Source anchors and useful source cases

- `src/eltdx/api/{codes,quotes,_batch,base}.py`, `src/eltdx/models/{security,quote}.py`, `src/eltdx/client.py`.
- Protocol `commands/{security,quotes}.rs`, `request.rs`, `response.rs`, current SDK metadata/operation/executor modules.
- `tests/fixtures/sdk/cases/`, policy normalization/units; COMP-008–010, COMP-028/029 and HQ domain F cases.
- Cases include cancelled second chunk; total operation timeout on second recovery; unknown/duplicate IDs; zero budget forbids recovery but allows initial success; short/short/empty directory; repeat page; cap before EOF; two owners disagree on precision; empty batch leaves no new resources; retained page/range origin; nonfinite required value rejection; real public client loopback where practical (source only).

## Workflow and report

Workspace `C:/Users/ax/.codex/worktrees/rust-sdk-implementation/tdx抓包`, branch `codex/rust-sdk-implementation`. NO tests, collection, builds, check, clippy, doctest, network probes, compile/AST/format/diff checks or equivalent verification until all T01–T30 code is implemented. Author test source; do not execute. Plain source reading, ordinary review diff generation, git status/log/add/commit allowed. No subagents, push/tag/publish/merge. No controller ledger/status edits. Commit exact owned files with hooks disabled (`git -c core.hooksPath=NUL commit`). Coordinate shared session/operation/metadata edits with controller.

Write full report to `.superpowers/sdd/development-plan/task-11-report.md`: requirements implemented, exact files and commits, authored source cases, no-execution statement, self-review, limitations and interfaces handed to T12/T13. Return compact status/SHAs/no-tests/concerns/report path. No compilation or runtime success claims.
