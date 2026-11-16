# HQ 源码交付索引

2026-11-17：这是 T18 的源码范围索引，不是编译、差分、回环或安装验收报告。
所有命令执行仍等 T01–T30 实现完成。`hq-api-spec.md` 定义 M01–M100；
下表将每个编号落到当前实现、Python 桥和已编写用例。编号范围不代表同等工作量。

路径以仓库根为基准。Rust 实现根为 `crates/eltdx/src/`；桥根为
`crates/eltdx-python/src/`；所有公开服务经 `eltdx` 根的 client/service 导出。

| 规格编号 | Rust 实现 | Python 路径和桥 | 未执行的主要用例文件 |
| --- | --- | --- | --- |
| M01–M17 | `services/codes.rs`, `services/code_selection.rs` | `api/codes.py`, `service.rs`, `transport.rs` | `crates/eltdx/tests/codes.rs`, `test_sdk_code_bridge.py`, `test_native_code_bridge.py` |
| M18–M22 | `services/quotes.rs`, `services/recovery.rs` | `api/quotes.py`, `service.rs`, `transport.rs` | `quotes_batch.rs`, `test_sdk_quote_bridge.py`, `test_sdk_quote_domain_bridge.py`, `test_native_quote_domain_bridge.py` |
| M23–M24 | `subscription/`, `services/quotes.rs`, `session.rs` | `api/subscription.py`, `subscription.rs`, controlled raw reader in `transport.rs` | `subscription.rs`, `legacy_push.rs`, `test_sdk_subscription_orchestration.py` |
| M25–M30, M98–M99 | `services/klines.rs`, `services/bar_collection.rs` | `api/bars.py`, `api/delisted_bars.py`, `service.rs`, `batch.rs` | `klines.rs`, `test_sdk_bar_bridge.py`, `test_sdk_batch_bridge.py` |
| M31–M40 | `services/minutes.rs` | `api/minutes.py`, `service.rs`, `batch.rs` | `minutes.rs`, `test_sdk_minute_bridge.py`, `test_native_minute_bridge.py` |
| M41–M50 | `services/trades.rs` | `api/trades.py`, `service.rs`, `batch.rs` | `trades.rs`, Python trade bridge/integration sources |
| M51 | `services/auctions.rs` | `api/auctions.py`, `service.rs` | `auctions.rs`, Python auction bridge/integration sources |
| M52, M100 | `services/money_flow.rs`, `services/money_flow_probe.rs` | `api/money_flow.py`, `service.rs`, `batch.rs` | `money_flow.rs`, Python money-flow bridge/integration sources |
| M53–M56 | `services/corporate.rs`, `services/corporate_legacy.rs` | `api/corporate.py`, `corporate.rs`, `service.rs`, `batch.rs` | `corporate.rs`, Python corporate bridge/integration sources |
| M57 | `adjustment/` | Python equity facade, `corporate.rs` | `adjustment.rs`, Python equity bridge sources |
| M58–M59 | `services/limits.rs` | Python limits facade, `limits.rs`, `service.rs` | `price_limits.rs`, Python limits bridge/integration sources |
| M60–M66 | `calendar/` | Python workday facade, `calendar.rs` | `calendar.rs`, Python calendar bridge sources |
| M67–M70 | `services/resources.rs`, `services/resource_legacy.rs` | `api/resources.py`, `resource.rs`, `service.rs` | `resources.rs`, `test_sdk_resource_bridge.py`, `test_native_resource_bridge.py` |
| M71 | `stats.rs`, `archive.rs`, `services/stats_projection.rs` | Python stats facade, `stats.rs` | `stats.rs`, Python stats bridge sources |
| M72–M73 | `services/company_info.rs` | `api/company_info.py`, `service.rs` | `company_info.rs`, Python session/company bridge sources |
| M74–M78 | `services/session.rs`, `services/health.rs` | session/health facades, `service.rs`; local ping remains transport-only | `hq_loopback.rs`, Python session/health bridge sources |
| M79 | `helpers/service.rs`, shared depth predicate | `helpers/core.py`, `helpers.rs` | `hq_helpers.rs`, `test_sdk_helpers_bridge.py` |
| M80–M84 | `helpers/service.rs`, migrated code/minute services | thin original helper delegates | `codes.rs`, `minutes.rs`, `test_sdk_helper_forwarding.py` |
| M85 | `helpers/service.rs`, shared rank page traversal | `helpers/core.py`, `helpers.rs` | `hq_helpers.rs`, `test_sdk_helpers_bridge.py` |
| M86–M87 | `helpers/profile.rs`, `helpers/cache.rs` | `helpers/core.py`, `helper_profile.rs` | `hq_helpers.rs`, Python profile bridge sources |
| M88–M89 | `helpers/daily.rs`, `helpers/stats_alignment.rs`, `helpers/calculation.rs` | `helpers/core.py`, `helper_daily.rs` | `hq_helpers.rs`, `support/daily_helpers_cases.rs`, Python daily bridge sources |
| M90–M91 | `helpers/shortline.rs`, `helpers/calculation.rs` | `helpers/shortline.py`, `helper_shortline.rs`, `helper_indicator.rs`, `helper_limits.rs` | `hq_helpers.rs`, `support/shortline_cases.rs`, Python shortline bridge sources |
| M92 | `helpers/auction.rs` | `helpers/market.py`, `helper_market.rs` | `hq_helpers.rs`, `test_sdk_market_composition_bridge.py` |
| M93–M95 | `helpers/market_service.rs`, `helpers/market.rs` | `helpers/market.py`, `helper_market.rs`, `helpers.rs` | `hq_helpers.rs`, `test_sdk_market_composition_bridge.py`, `test_sdk_helpers_bridge.py` |
| M96–M97 | `helpers/boards.rs`, `helpers/board_decode.rs` | `helpers/boards.py`, `helper_boards.rs` | `hq_helpers.rs`, `support/board_cases.rs`, Python board bridge sources |

Rust 用例文件位于 `crates/eltdx/tests/`；`test_sdk_*` 位于
`tests/contracts/`，`test_native_*` 位于 `tests/integration/`。精确函数节点与
源路径以 `tests/contracts/manifests/rust_sdk_migration.json` 的
`authored_evidence` 为准，均标 `source_only_unverified`。M93–M95 等没有独立
catalog 行的能力放在 supplemental evidence，不能增加原权威清单的 124 项计数。

## 消费方式

`crates/eltdx/examples/hq_query.rs` 为普通 Rust 例子；
`tests/consumers/rust-sdk/` 使用独立 Cargo workspace、公开 SDK 和 std 本机服务器；
`tests/consumers/python-wheel/consumer.py` 使用标准库与安装后的 Python 公开 API。
两个 consumer 都已写源码，尚未复制到外部、构建、安装或运行。T28/T29 仍须完成
实际的无 Python Rust 环境和无 Rust 工具链 wheel 环境、特性、MSRV、平台和包内容验收。

## 保留的语言适配与后续范围

Python 自定义 transport/方法覆盖、任意对象、属性访问次数、codec、异常活动上下文、
executor 生命周期和回调继续经过兼容 adapter。常规请求使用共享 Rust 域边界；
部分对象遍历在 PyO3 桥内保留 Python 运算和调用语义，并非全部复用类型化 SDK 投影。
Rust 严格有限总预算、校验和已知单位，不改变 Python 逐请求期限、旧宽松输入及价格规则。
T15 的唯一 push reader 继续负责推送，没有额外 dispatcher。

`helper-theme-strength-rank`、`helper-stock-topics`、`helper-topic-stocks` 虽在旧
manifest 标 P2，实际依赖 F10，归 T23，与 index/IPO/financial 等组合一起实现。
不得将这三项的原 P2 标签解释为 T18 已完成。F10 历史资金流归 T20/T23；EX/MAC
服务归 T24–T27。T18 的源范围闭合不代表后续任务或最终兼容验收已完成。
