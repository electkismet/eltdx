# T16 — 股本、复权、交易日和涨跌停策略

Read first. Authority: `docs/rust-sdk/development-plan.md` T16; `hq-api-spec.md` §§3–4,8–9,12; `public-api.md` §§6–9; T03 policy fixtures, `decision-register.md` E01 and related COMP cases. Read reviewed T08–T13 execution/numeric/batch/calendar dependencies. Controller gives GO.

## Exact scope

New `src/services/{corporate,limits}.rs`, `adjustment/{mod,factors,apply}.rs`, `calendar/{mod,service}.rs`, `policy/limits.rs`; add types/query modules and client getters. New `tests/{adjustment,calendar,price_limits}.rs` and corporate/finance source cases as needed. Implement HQ M53–M66 including pure public `eltdx::equity::build_adjustment_factors` M57; reuse one existing batch/metadata/numeric/cursor set, not alternative DTOs.

- Capital strict-prefix partial blocks continue remaining codes; zero/nonprefix/foreign block is Protocol, never positional misassociation.
- Adjustment scale+offset, same-day event order, anchor/window, qfq/hfq direction, no extra rounding.
- Calendar from specified benchmark source with coverage; lost bound client never silently weekday fallback.
- Limits use T03-approved effective-date rules only; no evidence means UnknownRule; keep distinct old Python compatibility projections for T18.
- Deferred `cargo test -p eltdx --test adjustment --test calendar --test price_limits --locked`.

## Complete requirements

CapitalQuery default75, wire1..200. All chunks share one operation, checked occurrence bounds, empty ordinary batch=>empty/no I/O. Each partial response must be exact strict prefix of remaining ordered IDs; a returned2 consumes exactly2, preserves declared blocks/raw page provenance and resumes suffix. No guessing absent securities from zero events; legal empty block is Present. Finance batch/recovery uses T11 shared algorithm and the same32-extra/operation accounting, preserves duplicate occurrences and typed confirmed missing. FinanceQuery missing=Error. FinanceField is full HQ enumerated fixed set and finance_fields preserves requested field order/duplicates, empty selection still identity/date. Decode full fixed wire fields even if projection sparse. Original units verified individually: circulation/total source万股×10000 shares, assets/net-profit specified千元×1000 yuan; unproven fields Unclassified with f32 bits, never infer every financial quantity by naming. Strict nonfinite required values fail with source.

CapitalEvent preserves date_raw, category/tag, c1..c4 and original bits/reserved, invalid source date as Option with raw where protocol allows, not guessed. Only valid-date tag1 changes adjustment; source order for same day stable. `m=(10+c3+c4)/10`, `o=(c1-c4*c2)/10`; reject nonfinite/nonpositive multiplier. qfq chronological updates scale/=m, offset=(offset-o)/m; hfq reverse-date traversal updates scale*=m, offset=m*offset+o. Within equal dates preserve original source order (do not reverse equal-date events). qfq factor considers events>=factor_date and<=anchor if supplied; hfq events<=factor_date; start filters events as frozen old algorithm. Anchor<start InvalidArgument. Emit date ascending, no two-decimal rounding; checked nonfinite/overflow failure. Test independent affine expected values, not comparing function with itself. Adjustment apply to kline fixed directions must preserve provenance/time/units and not rewrite unknowns.

Special limit0452 has no client page-size field; PageOptions default800 only workload hint, max200pages/10000 actual rows. Cursor offset/start_index checked u16, actual count progression, empty EOF, cap before proof LimitExceeded. Preserve unknown market/raw code/price f32 bits as optional identity; no fabricated SecurityId. Not nationwide approved policy evidence.

CalendarQuery default sh000001, no-adjustment index daily pages{800,200,160000}; source and cache owner-scoped, refresh fully builds immutable sorted unique date snapshot then atomic publish. Snapshot tracks benchmark/coverage/completeness/source metadata; empty cannot prove every date closed. Query outside coverage MetadataUnavailable with bounds, not false; next/previous within snapshot returns None at edge, no predictions. range swaps reversed endpoints then returns inclusive date order honoring descending. Clear cache is local, no I/O. today uses T10 injectable Shanghai clock and is not assertion market open. No automatic weekday fallback for network calendar. Python unbound weekday utility/weak-client ReferenceError remain compatibility adapter concerns, not new Rust fallback.

LimitRule/LimitStatus follow HQ §12. T03 has NO dated authoritative profile approved: all disputed real exchange cases remain UnknownRule with reason, reference evidence and date; cannot select daily/shortline/MAC old formula as universal rule. You may implement versioned rule application engine and controlled synthetic approved test profile without claiming its artificial fixtures real trading rules. Keep legacy daily10%/shortline5%/MAC distinct mapping hooks. Policy dependencies/unknown data must remain diagnostic, not f64 zero or unbounded fallback.

All public network entries capture Instant before validation; shared metadata snapshot and operation budget through child services (no calling public methods to reset budget). Retain original fields/ranges, no global decimal state, use source-only tests covering exact f32 bits, large values, unknown units, prefix errors, cancelled next chunk, metadata owner isolation, calendar expiry/midnight/coverage, same-date affine events and all T03 limit conflict cases.

## Sources and workflow

Read Python `api/{corporate,limits}.py`, `equity.py`, `workday.py`, relevant helper rule paths/models; protocol `commands/{corporate,limits}.rs`. Preserve frozen goldens and list unit evidence in report. Expected old Python prefix exceptions/field aliases/raw remain for T18 bridge; expose narrow shared compatibility projections, no PyO3 in SDK.

Workspace `C:/Users/ax/.codex/worktrees/rust-sdk-implementation/tdx抓包`, branch `codex/rust-sdk-implementation`. NO tests/collection/build/check/clippy/doctest/network/compile/AST/format/diff checks/equivalents until ALL T01–T30 implementation code complete. Source reads/writes, ordinary review diff generation, git checkpoint commands allowed. No subagents/push/tag/publish/merge or controller ledger/status edits; coordinate shared files and commit exact scope with hooks disabled.

Report `.superpowers/sdd/development-plan/task-16-report.md`: method/type coverage, numeric evidence and policy unknowns, cache/source semantics, exact files/commits, authored cases, no-execution, T18 compatibility hooks, self-review concerns. Return compact status/commits/no-tests/concerns/path. No unverified success claims.
