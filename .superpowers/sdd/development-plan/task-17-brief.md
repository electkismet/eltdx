# T17 — 服务器文件、统计文件和公司资料

Read first. Authority `docs/rust-sdk/development-plan.md` T17; `hq-api-spec.md` §§3–4,10,14; `public-api.md` §9.3; `decision-register.md` §3/E04; COMP-018–022 and HQ F23–F27. Read reviewed operation/pin/projection APIs. Start only controller GO.

## Exact scope

New SDK `src/services/{resources,company_info,session,health}.rs`, `src/stats.rs`, internal reusable bounded ZIP module, DTO/query modules and lazy client getters; new `tests/{resources,company_info,stats,hq_loopback}.rs`. Implement HQ M67–M78 fully. Default empty-feature HQ supports ZIP/stats; no dependency on public offline/f10. Decision/source evidence in new `docs/rust-sdk/dependency-decisions.md`, mirror when source complete.

- Choose one internal ZIP dependency/decoder, record pinned version/features/MSRV/license and limits; author duplicate/encrypted/compressed/uncompressed/truncated/bomb/CRC source corpus. Verification is deferred by explicit user ruling, so dependency choice is provisional, never label tested/locked success.
- Multi-block file remains same pin/host; checked offset/length and empty EOF read after exact block boundary when cap allows.
- DownloadResult distinguishes Complete versus LimitReached, strict complete refuses uncertain completeness; byte/frame limits before reads/allocations; failures release pin.
- Cover zero/exact/over cap, short/empty blocks, offset/host change. Python download still truncated bytes, stats max+1 probe rejects before inflate.
- Stats preserves source/date;95%/same-main-date belongs T18 helper, not raw stats parser.
- Company section byte offsets/GBK separately covered; HQ company info !=HTTP F10.
- Public-client nonempty query/pagination/fault close source in hq_loopback, not internal Engine-only test.
- Deferred `cargo test -p eltdx --test resources --test company_info --test stats --test hq_loopback --locked`.

## Exact boundaries

DownloadResult public fields data:Bytes, completion:DownloadCompletion, next_offset:Option<u64>; Complete only EOF/valid short evidence. read size0..60000, offset+size checked u32;0 returns empty/LimitReached/Some(start), no I/O. download_file max_bytes explicit0..u32::MAX, chunks30000 ordinary API; last request shrinks to remaining cap. At max exactly, do not perform forbidden extra read; result LimitReached, next offset. strict download_complete cap without EOF=>LimitExceeded including cap0. Enough cap permits final empty block and Complete. Same pin source cannot switch endpoint and concatenate; unexpected source changes Protocol, actual lost pin PinLost. Chunk declared_len must match content and <=requested, checked next offset. Reserve bounded chunks/output before growing; no disguised unbounded Vec or allocation before byte validation. Legacy optional max_bytes=None/chunk_size and cap0 pin observation maintained via shared internal policy for T18.

Server file path trim+ASCII nonempty/noNUL/<=300bytes, slash normalization; not a local path write permission. BlockMeta filename nonemptyASCII/noNUL/<=40bytes, returned size/md5+source, no hash-verification claim. CompanySectionQuery filename trim+GBK encodable1..80bytes/noNUL, length>0, checked u32 offset+length. Catalog fields name64/filename80 bytes; offset byte-based. Content tracks lossy GBK flag/diagnostic rather than hiding truncation; legacy ignore/drop behavior projected by bridge.80-byte40x公 valid,82-byte invalid before I/O.

Stats compressed32MiB; download M+1 and reject oversized before ZIP decode, entries<=128, each expanded<=32MiB,total<=64MiB. Validate central/local sizes and enforce actual streaming inflate caps, not trust claimed length only. Reject encrypted and duplicate required names, truncated/badCRC/badheaders; required unique tdxstat.cfg+tdxstat2.cfg and usable rows, duplicate identity per table rejected. No extraction to filesystem. ZIP helper must support T22's distinct64MiB/64entries/DAT256MiB policy without copying archive implementation; choose narrow internal limits object, no public unlimited bypass. Stats table parser source-based GBK| columns mirrors old nullable/finite behavior, prices/volumes units explicit; unknown unsupported market/code retains diagnosis rather than forged identity. Parser need not require both main dates align; T18 does.

Session/health local ping returns pong/remote_verified=false and no implicit connection (also diagnostic after close); handshake and heartbeat use owner operation, preserve control flags even Omit. probe explicitly performs heartbeat, reports source endpoint and elapsed; not whole-pool health. All result metadata preserve actual request/host/epoch/generation/date and raw page ranges; same shared deadline through download→decode CPU boundaries, cancel stops and cleans up.

## Workflow/report

Controller may authorize Stage A before shared T10/T11 service integration is ready. Stage A owns only a new internal bounded archive module (and its private submodules if needed), `src/stats.rs` and `tests/stats.rs`; it may author ZIP/STATS decode functions and source cases. Do not modify shared root/Cargo/types/owner/operation/services/Python or dependency-decisions document yet. Record the exact proposed dependency version/features and required registration in the task report for controller integration. This partial checkpoint must expose internal pure/boundary-aware APIs suitable for later download composition, retain the source cap and cancellation checks, and must not introduce stub public resource methods or claim T17 complete. Stage B has separate GO for resources/company/session/health integration and public DTO/getter wiring. No dependency resolution, package download, build or test commands.

Read Python `api/{resources,company_info,session,health}.py`, `stats.py`, models, protocol resource/session/corporate parsers. Source cases include exact cap6 vs known file6 still LimitReached; cap7 allows empty probe; malformed declared length; source changes; fake oversized input with zero inflate calls; required members/duplicate keys; GBK byte offsets; public TdxClient getter nonempty loopback/paging/badframe then successful close. Existing goldens remain unchanged.

Workspace `C:/Users/ax/.codex/worktrees/rust-sdk-implementation/tdx抓包`, branch `codex/rust-sdk-implementation`. No tests/collection/build/check/clippy/doctest/network/compile/AST/format/diff checks/equivalent verification until ALL T01–T30 implementation code complete. Source read/write and ordinary review diff generation/git checkpoint allowed. Do not run dependency resolution/metadata checks now. No subagents/push/tag/publish/merge/controller status edits; coordinate Cargo/shared-files, commit exact scope with hooks disabled.

Report `.superpowers/sdd/development-plan/task-17-report.md`: method coverage, shared decoder/dependency rationale+unverified status, source limits and cleanup proof reasoning, exact files/commits, authored source cases, T18/T22 handoff, no-execution, self-review concerns. Return compact status/SHAs/no-tests/concerns/path.
