# 变更记录

## Unreleased

## v2.0.2 - 2026-08-14

- 拆分成交明细、成交明细竞价记录和 09:25 正式撮合文档，每个公开入口对应独立页面并补充可复制示例。
- 拆分服务器文件分块读取、完整下载和统计文件解析文档，明确三者的调用边界和返回模型。
- 更新 README、方法索引、接口目录、命令对照和 GitHub Pages 映射，移除旧的合并页面引用。

## v2.0.1 - 2026-08-14

- `0x0fc5` 当日成交明细和 `0x0fc6` 历史成交明细现在区分普通成交、`status=8` 集合竞价快照和 09:25 正式撮合，并提供对应筛选属性。
- 09:25 正式撮合拆分为 `client.trades.opening_match_today()` 和 `opening_match_history()`，分别固定使用 `0x0fc5` 与 `0x0fc6`；集合竞价记录拆分为 `auction_today()` 与 `auction_history()`。
- 当日分时、当日成交明细和当日集合竞价明细继续使用 `today()` / `all_today()` / `series()` 公开入口；文档明确“当日”指主站当前保存的交易日。
- 代码表、K 线和成交明细按真实服务端单页上限校验，并按实际返回条数推进分页，直到空页确认结束，避免服务端短页造成漏数。

## v2.0.0 - 2026-08-13

- 移除 `TdxClient` 上全部旧版扁平 `get_*` 兼容入口，统一使用 `codes`、`quotes`、`bars`、`minutes`、`trades`、`auctions`、`corporate`、`resources`、`f10` 和 `helpers` 模块。
- 保留全部 7709 原生协议接口，包括 `0x053e` 的 `client.quotes.legacy()` 和 `0x06b9` 的 `client.resources.read()`。
- 将五档补齐、09:25 快照、除权除息、股本、换手率和本地复权等组合能力归入 `client.helpers`。
- 新增成交明细自动分页和全市场证券分类的模块化入口。
- 更新 MCP、smoke、文档和示例，使仓库内部统一使用 2.0 API。

## v1.3.1 - 2026-08-13

- 修复六位北交所 `92xxxx` 代码被通用上海 `9xxxxx` 规则误判为沪市的问题；上海 B 股 `900xxx` 仍保持沪市识别。
- MCP 自定义行情主站现在会在客户端注册前校验 `host:port`，拒绝缺少端口或端口越界的配置。
- MCP 客户端构造失败时会回滚 pending key 并唤醒同配置等待者，避免后续请求永久等待。

## v1.3.0 - 2026-08-04

- 补全短线指标 21 个字段的中文名称、业务含义、单位和来源/计算口径，并明确自由流通 `Z` 口径、百分比/倍数读法及封单字段的适用条件。
- MCP 服务迁移到 MCP Python SDK 2，修复新版 SDK 安装后无法导入旧 `FastMCP` 的问题。
- MCP 新增五档盘口、分时、成交明细、当日集合竞价、竞价汇总、短线指标、财务报表和公司资讯工具，并发布 8 个内置文档资源。
- MCP 服务现在复用有界的 `TdxClient` 实例，在服务退出时统一关闭；工具参数增加批量数、分页数和超时边界。
- CI 显式安装 MCP optional dependency，并通过 SDK 2 客户端验证工具发现、调用、资源读取和客户端生命周期。
- `0x0547` 增量刷新单次最多接受 100 个代码，避免主站静默截断第 101 个及之后的结果；实时完整五档统一通过 `0x0547` 首次刷新和推送获取。
- MCP 行情客户端按 `host + timeout` 独立初始化，每组使用 4 个连接槽位并支持空闲 LRU 淘汰；同服务器多线程调用不会直接共用同一个 socket，F10 不占用行情客户端槽位，服务退出会等待在途行情调用完成。

## v1.2.0 - 2026-07-19

- 新增 `client.helpers.shortline_indicators()` 和兼容入口 `client.helpers.get_shortline_indicators()`，返回固定 21 个统计资源及短线计算字段。
- 新增 `ShortlineIndicatorTable`、`ShortlineIndicator`、`ShortlineIndicatorsNotReadyError` 和 `TdxStatsDateError`。
- 目标交易日来自 TDX 握手，上一交易日来自上证指数实际日 K；`tdxstat.cfg` 与 `tdxstat2.cfg` 必须日期一致且主导日期覆盖率均不低于 95%。
- 统计资源仅接受目标交易日或上一实际交易日；过期、CFG 日期冲突、低覆盖率及交易日 09:25 前未就绪等情况明确失败，不跨日猜算。
- 已验证统计资源在客户端内存缓存；`refresh_stats=True` 可强制刷新，`client.clear_cache()` 同步清理统计缓存，不新增数据库或磁盘缓存。
- README、Pages 接口目录、API 参考、方法手册、字段手册和短线指标专页同步补充调用及字段口径。

## v1.1.0 - 2026-07-19

- 7709 transport 改为每连接槽位一个单线程非阻塞 `ConnectionActor`。
- 请求使用全池 FIFO admission、exact-once lease 和真正独占的 `pin()` proxy。
- `timeout` 现在覆盖数字 IP/已缓存 endpoint 的排队、连接、握手、发送、响应和一次 retry。
- push queue 改为有界 buffer；溢出会丢弃最旧帧并通过 `PushOverflowError` 明确报告 gap。
- 新增 `max_pending_requests`、`push_queue_size`、`push_queue_bytes`，以及 `PoolBusyError`、`PushOverflowError`、`TransportCloseTimeoutError`。
- `TdxClient`、`TdxClient.from_hosts()`、`PooledSocketTransport` 和 `eltdx-smoke` 的 `pool_size` 默认值统一为 `1`；该参数现在必须是正整数，非法值直接抛出 `ValueError`，不再静默截断或改写。
- 自定义 hostname 的首次 DNS 仍使用标准库阻塞解析，但在 Actor 外执行，不占 slot；该解析无法提供严格取消保证。
- Actor fatal 或 close deadline 到期现在 fail-closed，不会悄悄创建替代线程。
