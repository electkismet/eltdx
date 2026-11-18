# T19 F10 传输源码与验证交接

本页记录 T19 的传输分工和实际验收范围。[Stage181](unified-validation.md#stage181-f10-dependency-decision) 核对锁定依赖，并将 27 条回环用例逐个对到冻结候选在 Windows、Linux、macOS 的实际通过日志。具名 157 入口与历史资金流见 [T20 端点索引](f10-implementation-index.md)，Python 复用与对象适配见 [T23 迁移](t23-migration-index.md)。

## 实现分工和不变量

| 源码（`crates/eltdx/src/`） | 责任与关键边界 |
| --- | --- |
| `f10/config.rs`、`client.rs` | checked URL/header/byte/timeout配置；build/getter无DNS、根证书或环境代理读取；独立或HQ共用owner；clone共享不可逆close |
| `f10/request.rs`、`json.rs` | Entry分路、compact UTF8 Params；整个文档共用node/depth/byte上限；ordered keys/重复键/数字lexeme保留；BOM只在输入边界处理 |
| `f10/response.rs` | HTTP结果与业务码分离；ordered ResultSets/cells/重复列/未知顶层metadata；raw与表JSON按策略保留，可取消复制 |
| `f10/worker.rs` | 每owner最多4个registered工作线程/runtime；队列在原deadline内；显式connect额外共用1个探测门；未确认清理保留两个lease至真实退出 |
| `f10/control.rs` | 原OperationSponsor、stop、同一绝对deadline；异步wait持续检查；单地址connect有限份额以留地址族fallback时间 |
| `f10/resolver.rs` | Hickory异步UDP/TCP DNS；数字IP不查DNS；本地系统配置或显式nameserver；IPv4优先/IPv6可回退；不创建脱离owner的getaddrinfo线程 |
| `f10/http.rs` | 物理连接沿用RequestPermit/AttemptAuthorization；Host/SNI/roots/CONNECT；无默认环境proxy/cookie/cache；POST不自动应用重试；redirect仅显式同源307/308且不降级TLS |
| `f10/io.rs` | Hyper前的有界明文观察及请求header顺序适配；读取中wire/encoded/decoded限额；gzip拼接/footer、zlib StreamEnd/checksum/trailing拒绝；unsupported/多重coding显式报错 |
| `operation.rs`、`session.rs` | 同一高层期限、累计bytes/attempt预算、owner admission/task登记；取消根因与cleanup失败分别保存，成功页不启动父操作terminal cleanup期限 |

worker先销毁Tokio runtime，再发布done，实际线程结束后才join并释放slot。调用者中途退出时StopOnDrop发信号，owner登记继续持有责任；CancelUnconfirmed不等于线程已退出。已完成但未确认的任务在下一次准入前于gate锁外reap，避免占满并发槽后永远等待。

HTTP连接driver拥有socket，所有结果路径都drop sender并abort+await driver。解码或状态验证失败不会将同一socket交给下一次调用。解压writer在增长前检查最大decoded和父累计byte预算；encoded/wire保留副本也先计账。RetainWire记录收到的HTTP明文，TLS密文不在该合同内；Omit不保留完整表JSON的冗余副本，不代表解析无分配。

## 已执行的公开接口回环用例

下列用例来自 `crates/eltdx/tests/f10_http.rs`。测试使用本机 TCP/UDP、合成证书和独立字面量响应，覆盖本地传输行为。生产 F10 样本另见统一验证记录 Stage163。

| 边界 | 主要用例 |
| --- | --- |
| 正常UTF8/数值/多表/raw与来源 | `public_http_call_preserves_unicode_numbers_tables_and_actual_raw` |
| 三路独立URL与业务失败 | `public_route_overrides_are_independent_and_business_failure_is_normal_response` |
| 非对象/坏UTF8/JSON/状态/截断 | `public_http_rejects_bad_json_nonobject_status_and_truncated_bodies` |
| 分片header/chunk/BOM/trailer | `partial_headers_chunked_bodies_bom_and_trailers_are_supported` |
| encoded/decoded/header限额与父预算 | `compressed_payload_retention_differs_from_decoded_json_and_enforces_expansion_cap`、`encoded_chunked_body_and_response_header_caps_apply_while_reading`、`operation_bytes_include_retained_copies_before_the_response_is_accepted` |
| 多请求父期限/来源/raw/bytes | `report_prerequisite_retains_both_raw_sources_and_shares_streamed_byte_limit` |
| 取消/期限/关闭释放 | `cancellation_timeout_and_close_during_response_confirm_connection_release`、`close_during_dns_releases_the_owner_and_rejects_future_calls` |
| Host/SNI/根证书/IPv4失败后IPv6 | `https_uses_custom_roots_sni_and_ipv4_failure_then_ipv6_fallback` |
| CONNECT与不受信证书/hostname | `explicit_connect_proxy_keeps_tls_hostname_and_rejects_untrusted_certificate`、`invalid_certificate_hostname_is_rejected_even_when_root_is_trusted` |
| DNS/TLS/代理等待中取消 | `dns_cancellation_retires_pending_resolver_without_connecting`、`cancelled_tls_and_connect_proxy_waits_confirm_socket_release` |
| 环境代理隔离 | `environment_proxy_variables_do_not_affect_the_explicit_none_policy` |
| 惰性/idempotent connect与waiter取消 | `successful_connect_is_idempotent_and_closed_owner_rejects_before_network`、`simultaneous_connect_calls_share_one_transport_probe`、`connect_waiter_cancellation_keeps_the_first_probe_running` |
| Header顺序/重复名与4-worker准入 | `request_header_order_keeps_separated_duplicate_names`、`queue_admission_honors_cancellation_before_a_fifth_worker_starts` |
| 压缩/短header/坏chunk | `truncated_compression_invalid_chunks_and_short_headers_are_protocol_errors`、`zlib_requires_checksum_end_and_rejects_trailing_bytes`、`gzip_requires_footer_and_preserves_concatenated_member_semantics` |
| 明确拒绝不支持coding | `ambiguous_or_unsupported_content_codings_are_rejected` |
| POST redirect与拒绝边界 | `opted_in_same_origin_redirect_preserves_post_body_and_request_headers`、`redirect_policy_rejects_cross_origin_method_rewrite_and_limit_exhaustion` |

值层及request/response预算的纯源码用例还在 `f10_values.rs` 与各模块内部；T20具名方法、路由inventory和typed结果不能被这一张HTTP表替代。独立Rust消费者、已安装Python wheel消费者和HTTP owner压力是额外的全链路证据。

## 依赖、系统配置和生产证据

精确版本、feature、归档许可证、声明的 MSRV 和实测工具链见[依赖决策](dependency-decisions.md)。Stage179 在 Windows/Rust 1.89 仅启用 `f10` 执行 HTTP 目标；Stage166、Stage167 的 macOS 和 Linux 结果来自 `full`。三平台分别检查默认生产依赖、F10 的 normal/build 及含 dev 的闭包。`rcgen` 仅在 dev 闭包中，默认 HQ 不含 HTTP/TLS/DNS 栈。

系统 DNS 配置和 native roots 加载仍是已登记 worker 内的本地同步 OS 调用；调用超出清理期限时会报告未确认清理，并保留 worker 责任。回环用例实测了显式 DNS、自定义证书、代理、SNI、证书拒绝和回环 IPv6 回退。Stage163 的 Windows 默认 `http://static.tdx.com.cn:7615/TQLEX` 查询走系统 DNS 并得到非空结果。该查询不执行 TLS；三平台实际系统证书库导入，以及 Linux/macOS 的系统 DNS 配置行为，仍需独立运行证据。回环 IPv6 也不证明生产 IPv6 路由可用。

变更传输或依赖后，按影响范围复验：

```sh
cargo +1.89.0 test -p eltdx --no-default-features --features f10 --test f10_http --locked
cargo +1.89.0 test -p eltdx --no-default-features --features f10 --test f10_values --test f10_routes --test f10_named_methods --test f10_route_inventory --test f10_results --test f10_shared_templates --locked
```

第二条命令涉及 T19 值层和 T20 端点；各目标的既有证据以开发计划和统一验证记录中绑定的实际命令为准。
