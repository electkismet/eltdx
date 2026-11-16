# F10 implementation and recovery index

This is a source map for T19/T20, not a passing test report. All test source
listed here is deliberately unexecuted until the complete T01–T30 source gate.
The normative method signatures, defaults and literal wire shapes remain in
`extension-api-spec.md` §2.2–2.5. Source completion does not establish build,
dependency, platform, server availability or Python compatibility acceptance.

## Public surface and source ownership

Every method below belongs to `eltdx::f10::F10Client`. Unless explicitly noted,
the query type is the method name in PascalCase plus `Query`, with private
fields, constructor/getters and checked builders. Network methods additionally
take `&OperationOptions`. All ordinary endpoints return `Result<Response>`.
The whole module requires the optional `f10` feature and has no Python runtime
dependency; default HQ-only features do not gain an HTTP dependency.

| Family | Methods | Source |
| --- | --- | --- |
| Generic requests | `call`, `params` | `f10/client.rs`, `f10/request.rs` |
| Generic/A-share single requests | `stock_info`, `business_periods`, `topic_ids`, `company_profile`, `shareholders`, `shareholder_change_plans`, `capital_structure`, `business_report`, `industry_analysis`, `trading_report`, `fund_report` | `f10/endpoints/reports.rs` |
| Dependent company report | `business_composition` | `f10/endpoints/reports.rs`; one shared context for period lookup and data |
| Limit-board detail and alias | `limit_up_down_list`, `limit_board_ladder` | `f10/endpoints/ladder.rs`; return `LimitBoardLadder` |
| US | `mg_name`, `mg_tips`, `mg_profile`, `mg_finance`, `mg_finance_indicator`, `mg_business`, `mg_industry`, `mg_industry_compare`, `mg_dividend`, `mg_capital`, `mg_shareholders`, `mg_directors`, `mg_market`, `mg_announcements`, `mg_rating`, `mg_topic`, `mg_deep_analysis` | `f10/endpoints/international.rs` |
| Hong Kong | `hk_name`, `hk_tips`, `hk_events`, `hk_profile`, `hk_related`, `hk_executives`, `hk_capital`, `hk_shareholders`, `hk_finance`, `hk_finance_indicator`, `hk_business`, `hk_industry_compare`, `hk_market`, `hk_capital_flow`, `hk_news`, `hk_topic`, `hk_pepb_band`, `hk_valuation`, `hk_price_chart`, `hk_analyst_forecast` | `f10/endpoints/international.rs` |
| Third board | `sb_name`, `sb_profile`, `sb_tips`, `sb_events`, `sb_capital`, `sb_shareholders`, `sb_compliance`, `sb_dividend`, `sb_finance`, `sb_business`, `sb_major_events`, `sb_news`, `sb_industry_compare` | `f10/endpoints/international.rs` |
| Convertible bonds | `bond_profile`, `bond_finance`, `bond_rating`, `bond_conversion`, `bond_rate`, `bond_announcements` | `f10/endpoints/bonds.rs` |
| Government bonds | `gz_profile`, `gz_interest`, `gz_market_data`, `gz_rating`, `gz_finance` | `f10/endpoints/bonds.rs` |
| Corporate bonds | `qyz_profile`, `qyz_interest`, `qyz_guarantee`, `qyz_rating`, `qyz_rate`, `qyz_review` | `f10/endpoints/bonds.rs` |
| Repos | `gzhg_info`, `gzhg_quotes` | `f10/endpoints/bonds.rs` |
| ABS | `zczq_profile`, `zczq_finance`, `zczq_rating` | `f10/endpoints/bonds.rs` |
| Exchangeable bonds | `kjhz_profile`, `kjhz_finance`, `kjhz_interest`, `kjhz_guarantee`, `kjhz_rating`, `kjhz_exchange`, `kjhz_rate` | `f10/endpoints/bonds.rs` |
| Shared bond pages | `bond_terms`, `bond_bulletin` | `f10/endpoints/bonds.rs` |
| Industry | `hy_summary`, `hy_stage_rise`, `hy_profile`, `hy_commentary`, `hy_news`, `hy_members_rank`, `hy_capital_stats`, `hy_fund_holdings`, `hy_capital_flow`, `hy_analyst_index`, `hy_major_holders`, `hy_rating_summary`, `hy_index` | `f10/endpoints/sectors.rs` |
| Futures/index futures | `qh_position_report`, `qh_basis_spread`, `qh_warehouse_receipt`, `qh_inventory`, `qh_variety_info`, `qh_index_info`, `qh_index_valuation`, `qh_weights` | `f10/endpoints/sectors.rs` |
| Market dynamic page | `market_report` | `f10/endpoints/market.rs` |
| Typed market collection | `index_members` → `IndexMemberTable`; `index_tracking_products` → `IndexTrackingTable`; `board_members` → `BoardMemberTable` | `f10/endpoints/market.rs` |
| IPO | `ipo_calendar` → `IpoCalendar`; no query argument | `f10/endpoints/market.rs` |
| Company finance/governance | `dividend_financing`, `allotment_dates`, `allotment_details`, `finance_report`, `finance_diagnosis`, `stock_score`, `profit_forecast`, `ranking_detail`, `governance`, `hot_topics`, `stock_finance_weights` | `f10/endpoints/reports.rs` |
| Topic/news | `topic_compare`, `topic_compare_first`, `company_news`, `digest`, `northbound_holding`, `detail`, `cache_list`, `announcements`, `news`, `roadshows`, `market_news`, `channel_news` | `f10/endpoints/news_queries.rs` |
| Local news registry | `news_channels` → `Vec<NewsChannelInfo>`; no query/options and no network work | `f10/news.rs`, `f10/endpoints/news_queries.rs` |
| Market themes | `theme_market`, `stock_stage_gain`, `stock_main_funds`, `stock_industry_rank`, `stock_industry_pct_rank`, `stock_board_member_quotes` | `f10/endpoints/valuation.rs` |
| Valuation | `valuation`, `stock_valuation_percentile`, `stock_pepb_percentile`, `stock_history_trend` | `f10/endpoints/valuation.rs` |

The source inventory totals 157 original named methods. Generic/A-share16,
international50, bonds31, industry/futures21, and market/composite/news/valuation39
refer to the original specification groupings, rather than rows in this table.
`CompanyProfileQuery` etc. follow the query naming rule; `CallQuery` and
`ParamsQuery` are the explicit generic exceptions. Getter/builder/close APIs
are additional lifecycle/configuration APIs, not included in the 157.

Historical money flow is an additive service beyond that list:
`F10Client::money_flow_history() -> MoneyFlowHistoryService`, then
`history/range(&MoneyFlowHistoryQuery, &OperationOptions) -> FundFlowRangeSeries`.
See `services/money_flow_history.rs` and extension §2.5 for the exact one-request,
reverse-then-slice contract and separation from HQ daily flow.

## Evidence layers to run at the final gate

| Source file | What it can establish when eventually executed |
| --- | --- |
| `tests/f10_values.rs` | Independent owner/config/value boundaries and zero-network construction |
| `tests/f10_http.rs` | HTTP/DNS/TLS/CONNECT, IPv4/IPv6, cancellation/close/deadline, compression, limits, redirects and ordered headers |
| `tests/f10_route_inventory.rs` | Independent literal default requests, closed selector branches, open selector pass-through, 84 channels, eight tabs, mixed scalar types, aliases and nondefault arguments |
| `tests/f10_routes.rs` | Checked identities, dates, page limits, prerequisite-needed query states and history window request shape |
| `tests/f10_named_methods.rs` | Actual public calls for 148 single-request methods, including call/params; query forwarding, configured HTTP route, response structure, nonzero business status, raw bytes and rejection after close |
| `tests/f10_results.rs` | Remaining two dependent methods, two ladder methods and four typed market methods; raw/unknown/multiple tables, typed fields, dates, units and pagination behavior. The local `news_channels` registry is covered separately by `f10_route_inventory.rs`. |
| Private module source cases | Duplicate column collisions, JSON bounds, field scope/inheritance and internal cancellation points |

The public-method integration suite intentionally uses each query's `to_call`
as the expected request, so it proves forwarding and actual transport use only.
It is paired with the separate hand-authored literal request suite; it must
never be described as an independent oracle for the request values themselves.
Every case contains synthetic local responses, not independently captured
production data or proof that a remote server still supports the page.

## Result and collection contracts

- Raw cells retain position, duplicates, unknown names, nested JSON and exact
  number lexemes. Python suffix projection is separate and opt-in.
- Normal business errors remain inspectable responses. Typed market collection
  stops on the first such page with `BusinessError` and preserves its rows.
  Required prerequisite failures return `MetadataUnavailable` before a second
  request. Cancellation, deadline, transport and protocol errors remain errors.
- Market collections retain `page_responses`; their first-table rows move into
  the aggregate. IPO/history/ladder retain their source responses similarly.
  Additional table content and top-level metadata survive even with raw omitted.
- Only explicit `zs` cells supply strict Rust page totals. Unrelated numbers do
  not establish EOF. Board keys include code, plan, all three source dates and
  report period; same-code annual/interim dividends are separate records.
- Page/record/byte limits, no-progress detection and source errors do not prove
  full collection. Explicit one-page requests are distinguished from EOF.
- Field evidence separates repository mapping, inferred semantics and Unknown.
  Numeric/date conversion failures are diagnosed; raw values are preserved.
  No global N001 meaning or blanket yuan/share/percent interpretation exists.
- One public operation owns the context across all prerequisite requests,
  pages, decoded bytes, projected rows and retained source metadata.

## Mandatory T23 handoff

No Python source is modified by T20. The following differences must survive
in the Python compatibility adapter rather than adopting strict Rust defaults:

- `BRK.B` is historically normalized to `B`; HK accepts old six-digit input.
- Python uses per-request timing and its original loop/clamp/stop rules; Rust
  operation totals and new default resource caps are not retrofitted.
- `_result_total` scans old additional-table values permissively; Python board
  no-progress/max-page results return their existing tuple/list shape.
- `topic_compare_first` uses Python's first-row `_first_value` behavior; Rust
  skips unusable IDs. Missing fund report periods follow the old Python path.
- Python HK analyst forecast fills missing bounds from local today, using its
  runtime date behavior; strict Rust optional date inputs remain explicit empty
  wire values. Preserve the old leap-day error and default-date behavior.
- Runtime Python field dictionary dispatch, dynamic bank T088 notes, duplicate
  suffix/collision semantics, custom HTTP overrides and `payload.update(extra)`
  retain their original behavior.
- History weak factories/executor scope and reserialized `raw_payload` remain
  separate from new Rust actual HTTP raw bytes and strict history days/count.

## Remaining work outside this source gate

T19 still needs real dependency resolution, API/MSRV/license/feature-tree and
platform verification, deferred by the user. T22 financial archives/downloads,
T23 Python and cross-source migration, T24–T27 extension services/bridges and
T28–T30 packaging/consumers/final verification are separate required tasks.
Do not infer completion of those tasks from this F10 source index.
