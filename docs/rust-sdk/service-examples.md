# 公开服务示例与非空回环对应表

本页登记逐服务普通调用示例与真实回环源码的对应关系，当前均未编译、收集或执行。表格存在只证明源码交接，不能作为通过证据。后续还须核对实际 doctest 收集集合、用例执行、非空结果、错误和资源断言。

## 示例如何进入实际编译

权威文本为本目录的 `examples-hq.md`、`examples-f10.md`、`examples-offline.md`、`examples-ex.md`、`examples-mac.md` 与 `examples-serde.md`。它们逐字复制到 `crates/eltdx/src/guide/{hq,f10,offline,ex,mac,serde}.md`，由 crate 的 `include_str!` 文档属性收录；可选域仅在相应 feature 下收录。`src/**` 已纳入 SDK 归档，因此离开仓库打包解包后仍能编译这些示例。

现有 `check_release_metadata.py` 增加六对文档的字节一致性检查，另继续检查完整 Python 包内 docs mirror。它只检查同步，不宣称示例覆盖或可编译。实际编译由 `rust_sdk_matrix.ps1` 的最小/default/serde/f10/full 严格 doctest 和 rustdoc 负责，并由后续归档消费门禁确认包装后的路径。所有命令尚未运行。

使用说明：[HQ](examples-hq.md)、[F10](examples-f10.md)、[财报/本地](examples-offline.md)、[EX](examples-ex.md)、[MAC/MAC-EX](examples-mac.md)。函数借用调用者配置的 client 和 options，不取得擅自关闭共用 owner 的权利；顶层调用者须在业务成功或失败后 close，并保留二次清理失败。网络示例为 `no_run`，不是 `ignore`。

[Serde 合同](examples-serde.md)独立说明有限/非有限值、bytes、日期时区、Unknown 和 checked 类型的反序列化边界。`serde_contract.rs` 有七组实际运行合同源码，rustdoc 另有普通泛型 serializer 和三个 compile-fail 合同；它们也必须在 `serde` 单独 feature 下验证，不能只由 full 构建掩盖生产依赖错误。

## 18 个公开 service

测试路径以下均相对 `crates/eltdx/tests/`；右栏为源码中的真实函数名。某一正常用例不替代分域索引中完整的边界、失败、取消、单位和来源合同。

| Service / feature | 示例的实际调用 | 非空回环源码与关键观察 |
| --- | --- | --- |
| CodeService / 默认 | HQ `directory` → `codes().list` | `codes.rs::short_short_empty_scan_does_not_query_count_and_retains_page_origins`：两个非空目录、短页继续和 EOF、逐页来源；`hq_loopback.rs::public_nonempty_directory_paging_then_fault_can_close_cleanly`：公开 list/分页及故障关闭 |
| QuoteService / 默认 | HQ `snapshot` → `quotes().snapshots` | `quotes_batch.rs::complete_reordered_duplicate_occurrences_preserve_distinct_values_with_zero_extra_budget`：非空乱序、重复 occurrence 与不同价格；missing及恢复在同target另验 |
| PinnedQuoteService / 默认 | HQ `refresh` → `pin.quotes().refresh` | `subscription.rs::pinned_nonempty_refresh_preserves_identity_cursor_raw_and_releases_its_connection`：真实目录预取、非空刷新、身份/cursor/raw、pin及owner关闭；新增源码未运行 |
| KlineService / 默认 | HQ `bars` → `klines().page` | `klines.rs::collect_continues_after_short_pages_and_reverses_only_page_order`：真实 page/collect路径、四条非空bar及源序；其它query/日期边界另验 |
| MinuteService / 默认 | HQ `minutes` → `minutes().history` | `minutes.rs::minute_wire_families_keep_their_time_and_average_semantics`：历史/当日/近期非空分时及时间/均价差异 |
| TradeService / 默认 | HQ `trades` → `trades().page` | `trades.rs::collect_keeps_page_local_order_absolute_indices_late_trades_and_raw_ranges`：非空分页/位置/收盘后逐笔/raw；`today_and_history_columns_reconstruct_every_row_field_without_a_second_request`：两类逐笔/列路径 |
| AuctionService / 默认 | HQ `auction` → `auctions().series` | `auctions.rs::auctions_preserve_source_sequence_seconds_bits_raw_fields_and_date_dispatch`：非空竞价点、秒/位/日期分路 |
| MoneyFlowService / 默认 | HQ `money_flow` → `money_flow().daily` | `money_flow.rs::daily_preserves_raw_business_fields_exact_float_bits_ratios_and_unknown_amount_unit`：HQ日资金流非空字段/位/单位；非F10历史 |
| ResourceService / 默认 | HQ `resource` → `resources().read` | `resources.rs::exact_cap_is_uncertain_but_one_more_byte_allows_the_empty_eof_probe`：非空资源数据和达到上限/真实EOF区别 |
| CompanyInfoService / 默认 | HQ `company` → `company_info().category` | `company_info.rs::catalog_uses_fixed_gbk_byte_fields_and_content_preserves_loss_diagnostics`：非空目录/正文与GBK损失诊断 |
| SessionService / 默认 | HQ `session` → `session().handshake` | `hq_loopback.rs::nonzero_controls_and_source_dates_survive_the_public_session_projection`：非零控制字段、源日期和heartbeat内容 |
| HealthService / 默认 | HQ `health` → `health().probe` | `hq_loopback.rs::session_and_explicit_probe_preserve_control_semantics_under_omit`：实际heartbeat/端点/来源和共用物理连接；本地ping不算探活 |
| CorporateService / 默认 | HQ `finance` → `corporate().finance` | `corporate.rs::finance_preserves_duplicate_occurrences_original_bits_and_field_order`：非空财务、重复输入/原始位/列序；资本和复权另验 |
| LimitService / 默认 | HQ `limits` → `limits().special` | `price_limits.rs::special_scan_uses_actual_counts_and_empty_eof_without_encoding_a_page_size`：非空特殊限价、未知身份、原始位与EOF；不证明权威通用规则 |
| CalendarService / 默认 | HQ `calendar` → `calendar().refresh` | `calendar.rs::sorted_unique_calendar_has_explicit_coverage_inclusive_ranges_and_bounded_neighbors`：非空源日历、覆盖和边界邻居 |
| HelperService / 默认 | HQ `full_quote` → `helpers().full_quotes` | `hq_helpers.rs::full_quotes_batches_eighty_and_joins_reordered_depth_by_identity`：非空snapshot+depth、80分批和身份join；F10 helper另见T23索引 |
| MoneyFlowHistoryService / f10 | F10 `history` → `money_flow_history().history` | `f10_results.rs::money_flow_history_comp050_reverses_then_slices_without_sorting`：三条HTTP源行、反转再截取、未知字段与原payload |
| FinancialService / offline | 财报 `download` → `financial().download_records_detailed` | `financial_service.rs::records_compose_list_hq_archive_zip_and_dat_with_source_provenance`：非空清单/下载/ZIP/DAT、原期限及来源 |

## 独立客户端和本地读取补充

| 入口 / feature | 普通示例 | 真实行为源码 |
| --- | --- | --- |
| F10Client / f10 | F10 `company_name` 与 `raw_call` | `f10_named_methods.rs::every_international_named_method_executes_its_query_and_retains_response`含mg_name；`generic_call_and_params_execute_their_distinct_body_shapes`及`f10_http.rs::public_http_call_preserves_unicode_numbers_tables_and_actual_raw`覆盖底层非空HTTP/多表/raw |
| ExClient / ex | EX `directory` | `extended_loopback.rs::actual_ex_client_uses_its_92_byte_setup_nonempty_directory_raw_and_shared_close`：实际EX握手/非空目录/关闭；市场及其余入口见EX索引 |
| MacClient / mac | MAC `quotes` | `mac_session.rs::required_setup_nonempty_quotes_bits_meta_and_close_use_the_public_entry`：三步setup、非空字段位图报价、未知slot和关闭 |
| MacExClient / mac | MAC-EX `quotes` | `mac_session.rs::mac_ex_uses_login_then_head_rewrite_without_the_ex_setup_packet`：显式合成登录、改头、非空报价；合成profile不充当生产登录证明 |
| offline 财报读取 | 本地 `read_report` | `financial.rs::local_zip_uses_header_date_explicit_override_and_cancellation`及`zip_selects_exact_checked_entry_and_retains_both_raw_sources`：真实本机文件/ZIP/DAT与来源；不访问真实用户目录 |

上述源码在统一执行时若编译失败、fixture前提无效、未产生非空结果或清理未确认，必须修正并重跑；不能凭行名和调用文本保留通过结论。最终仍按 R01–R12 和完整计划逐项验收，不能用这张最小示例表取代全部方法与分支的合同。
