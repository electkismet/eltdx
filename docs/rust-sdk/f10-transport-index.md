# T19 F10 传输源码与验证交接

本页完成T19的源码库存与交接，不代表依赖验证、编译或运行验收通过。已按源码检查HTTP边界、worker所有权、原操作授权、DNS/TLS/代理、限额、压缩和错误清理；全部执行仍待T01–T30源码门槛。具名157入口与历史资金流属于[T20端点索引](f10-implementation-index.md)，Python复用与对象适配属于[T23迁移](t23-migration-index.md)。

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

## 已写公共fixture矩阵

下列均为 `crates/eltdx/tests/f10_http.rs` 的真实函数名，尚未收集或执行。fixture使用本机TCP/UDP、显式合成证书和独立字面量响应，不能当作生产F10数据样本。

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

## 依赖与平台验收仍待执行

精确pin/features、已有缓存metadata与尚未知的API/MSRV/license记在[依赖决策](dependency-decisions.md)。实际lock须在全源码门槛后解析，并检查完整normal/build/dev闭包与许可证；缓存里同名版本或本页表格不是可下载、可编译或合规证明。若候选版本不可用或API不符，修正源码声明及文档，再生成新的候选和证据。

系统DNS配置与native roots加载是registered worker内的本地同步OS调用。其无法及时返回时只能报未确认清理并保留责任，不允许声称能抢占任意OS代码。Windows/macOS/Linux系统trust、代理、DNS配置/IPv6可用性都需实际平台用例；某个平台无IPv6应明确记录环境缺口，不能把未触发fallback当作通过。

全源码完成后的执行模板：

```sh
cargo +1.89.0 test -p eltdx --no-default-features --features f10 --test f10_http --locked
cargo +1.89.0 test -p eltdx --no-default-features --features f10 --test f10_values --test f10_routes --test f10_named_methods --test f10_route_inventory --test f10_results --test f10_shared_templates --locked
```

最小feature还需证明不引入HTTP依赖；full/交叉feature/MSRV与current stable重复构建；真实F10服务可用性和生产响应另列显式在线证据。以上命令目前均未执行。
