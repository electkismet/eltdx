# T20 source implementation report

## First checkpoint — concrete endpoint source foundation

Prior committed checkpoint: `a0893db` (T19 bounded F10 retention and shared
connect probes). Goal remains active with no token quota. All execution
validation is deferred until the complete T01–T30 implementation is authored.

Authored `f10/values.rs`, `news.rs`, and `endpoints/{mod,international,bonds,
sectors,reports,news_queries,valuation,market}.rs`; wired exports and offline
`parse_response`. Macros emit distinct public query structs and named methods.
No generic `call` substitution is counted as an implemented convenience API.

- Explicit A-share, US, HK, third-board, bond, industry and futures identities;
  method-scoped sections, pagination selectors, date/window values. Bond
  default market and six-digit HK wire identity preserve their provenance.
- Initial US17/HK20/SB13, bond31, industry13/futures8 endpoint sources; report
  branches and report-period prerequisites; all keep wire parameter order and
  numeric-versus-string distinctions in source.
- Eight home tabs and 84 news channels, second-table extraction, cache request
  bodies, topic prerequisite, theme/valuation specialized wrappers.
- Market report branch table, index/tracking/board collectors and raw-backed
  typed rows. This is still under source review, not a compiled API claim.

Remaining source work, in priority order:

1. Finish market collector lifetime/metadata/raw bounds; typed projection must
   run before the original context finishes. Board keys need cancellable,
   bounded traversal; prerequisites must include retained raw provenance.
2. Complete limit_up_down_list/limit_board_ladder and typed per-date statistics,
   local-date source, unknown units and invalid-number/date diagnostics.
3. Full scoped fields registry from both Python dictionaries, preserving
   unknown fields and the Entry/section/report-type context.
4. Historical money_flow history/range through F10, requested days, ascending
   date order and start/count slicing; never category22 substitution.
5. Complete per-entry/branch route and result test sources; scrutinize report
   date validation, defaults, closed sections and all 157 public entries.
6. T22/T23 and remaining extension/consumer/package/docs implementation.

Status: 19 tasks source-complete, five partial (T19/T20/T22/T24/T25), six not
started. T19 is still partial because source handoff and dependency/platform
evidence are not complete. No tests, collection, build, check, clippy, doctest,
AST/format/diff checks, dependency resolution/download or network verification
ran. No new agents, push, tag, publication or merge.
