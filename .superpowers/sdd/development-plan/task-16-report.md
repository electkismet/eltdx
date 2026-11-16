# T16 implementation recovery report

Status: source implementation and limited self-review complete at6fa4bc7/84061e7. CorporateService M53-M56, pure equity M57, LimitService M58-M59, CalendarService/cache M60-M66 and empty approved-rule policy are authored with test source. No compile/runtime/independent-review claim. All tests/builds/checks/network/AST/format/diff checks remain deferred until ALL T01-T30 implementation code is authored. Historical pending notes below are superseded by the final source section.

Key source choices:

- Original capital declaration is now retained as CapitalChangeBatch.declared_blocks; existing Python block_count remains the parser's normalized actual block count. Rust validates an exact nonempty ordered prefix and each nested event identity, resumes only the unconsumed suffix, and treats empty event blocks as Present. Corporate values preserve original f32 bits/reserved/date_raw; only valid-date tag1 drives factors.
- Factor algorithm preserves stable same-date order, chronological qfq, reverse-date-group hfq, full affine offsets and frozen start/anchor windows. Kline application preserves time/volume/amount/unknown fields and does no price rounding. Source cases and review of composition/deadline enforcement are still pending.
- Snapshot and finance share ordered_tree binary recursion; domain callbacks retain their own structural error/absence rules and the existing authorization/budget executor. Finance uses32 extra recovery attempts per operation, ordered occurrence mapping and typed confirmed absence. Complete31 wire fields decode before sparse projection. Existing finance model getters are unit evidence for only circulating/total shares (10000) and total_assets/net_profit CNY (1000). Every other field remains Unclassified with f32 bits. New source units TenThousandShares/ThousandCny retain original raw source_value.
- Calendar immutable cache lives in SessionOwner, not individual getter clones or a global. It uses bounded16 snapshots, Shanghai-day freshness and publication generations to reject builds invalidated by clear/newer refresh. Full index daily collection shares the outer context, then releases its pin before cache publication. Coverage comes only from observed first/last dates; empty data cannot make is_workday false. Midnight crossing is an explicit MetadataUnavailable. No weekday fallback.
- M60 implementation uses Result<TradingDate>, because the established Clock trait can fail. A total TradingDate return would require panic or fabricated fallback. Update authoritative spec/mirror and decision register before closing T16. Python today behavior is a T18 compatibility concern.
- Special0452 page_size remains a workload hint absent from its14-byte request. Scan uses a fixed pin, actual counts, empty EOF and hard200-page/10000-row bounds. Unknown identity and f32 bits remain. All real policy evaluation returns UnknownRule: no approved dated profile exists. Synthetic private profile tests never enter the production registry.

Resume: finish deterministic adjustment/corporate/finance/calendar/special/policy source cases; inspect typed API/raw ranges/deadline/cancellation/cache concurrency; make necessary source corrections; update spec/progress/mirrors and checkpoint. No subagents/push/tag/publication/merge. Goal active without token budget.

## Final source recovery point — 2026-11-16

The historical pending-case and specification notes above are superseded by
6fa4bc7 and 84061e7. All T16 implementation and deterministic source cases
are authored: adjustment/corporate/calendar/price-limits integration files,
private finance/projection/recovery cases, affine ordering and cancellation,
metadata capture and stale calendar publication boundaries. The M60 fallible
Clock signature correction and decision record are synchronized in the root
and packaged specifications. No tests, builds, collection, network probes or
equivalent execution checks ran. These are source-completion claims only.

Current plan count is 17 source-complete, 4 partial and 9 unstarted tasks.
Resume with task-17-report.md and task-17-brief.md for T17 Stage B, reusing the
existing archive/stats/encoding implementation. Keep the goal active without
a token budget. Preserve the all-T01–T30 validation deferral and the Python
compatibility obligations for T18 and later bridges.
