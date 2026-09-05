# 变更记录

## 工作区变更（未发布）

暂无。

## v3.1.3 - 2026-09-05

- `client.bars.get()` 支持传入代码列表，按连接池并发返回以规范化完整代码为键的结果字典；`batch_size` 可限制并发数。
- 新增 `client.f10.limit_board_ladder()` 和 `eltdx_limit_board_ladder`，通过 `CWServ.cfg_fx_lbtt` 查询指定日期 / 日期范围的逐股连板明细，并可选返回市场概况。
- 补充连板天梯 F10 模型、字段映射、接口目录和包内文档镜像。

## v3.1.2 - 2026-09-05

- 集合竞价当日和历史查询统一使用 35 台专用主站；普通行情、K 线和成交接口继续使用默认 43 台主站。
- 创建标准 `TdxClient` 时并发测速普通 43 台和专用 35 台候选主站，合并重复地址后分别缓存排序结果；首次业务请求不再重复测速。
- 测速只使用临时 TCP 探测连接；普通池、集合竞价池和资金流向池按首次使用建立业务连接并完成握手，后续请求复用连接。
- 补充 `realtime_rank()` 全部服务端排序字段、编号和排序方向说明。

## 修复内容

- 避免集合竞价历史请求因落到普通行情主站而返回空数据。
- MCP 客户端注册表适配构造阶段测速，避免构造客户端时持有注册表锁阻塞其他客户端。
- 集合竞价和资金流向专用池复用同一份 35 台主站排名，关闭客户端时统一释放已创建的连接池。

## v3.1.1 - 2026-09-02

- 资金流向 `0x0ffc` 使用独立主站池，避免与普通行情主站列表混用。
- 协议回归改用仓库内当前 7709 golden fixture，不再下载或对比旧版本 wheel。
- `client.bars.get()` 默认按代码自动识别股票或指数，修复指数 K 线布局解析并补充指数上涨 / 下跌家数。

## v3.1.0 - 2026-09-01

- 新增 `client.money_flow.daily(code)`，接入 7709 `0x0ffc` 日资金流向服务。
- `client.money_flow.daily()` 支持传入代码列表，并按连接池并发返回 `MoneyFlowBatch`。
- 返回主力净额、主买净额及两种口径各自的超大单、大单、中单、小单净额。
- MCP 新增 `eltdx_money_flow`，参数和批量规则与 Python API 保持一致。
- 新增资金流向独立详情页、接口目录条目、命令映射、字段说明和真实返回样本。

## v3.0.9 - 2026-08-30

- 成交明细、历史成交明细和 09:25 正式撮合入口现在支持传入代码列表。
- 列表查询返回以规范化完整代码为键的结果字典；底层仍按单股票请求，并按连接池 Slot 并发调度，超出并发数的代码自动排队。
- 同步 API 参考、方法手册、接口目录、包内文档镜像和批量回归测试。

## v3.0.8 - 2026-08-28

- `client.helpers.daily_price_limits()` 现在必须传入 `trade_date`，按指定 T 日计算涨跌停价。
- 参考价改为取 T 日之前最近一根不复权日线；停牌导致没有 T-1 日线时，继续使用最近实际成交日线，找不到则明确返回 `missing_pre_close`，不再使用实时快照兜底。
- 应用 T 日 `0x000f` 权息事件，并输出 `pre_close_trade_date` 和 `pre_close_source`。
- 更新 2026 年 7 月 6 日起主板 ST/*ST 涨跌幅为 10% 的规则。
- MCP 新增 `eltdx_daily_price_limits`，`trade_date` 为必填参数；同步 MCP 文档和工具契约。

## v3.0.7 - 2026-08-28

- 修复 Python 3.10 下 `/methods` 枚举 `WorkdayService` slots 描述符时抛出 `TypeError` 的问题。
- 同步 README 版本化接口横幅和赞助图位置测试，恢复 Python 3.10-3.14 主分支 CI。

## v3.0.6 - 2026-08-28

- 新增可选的 FastAPI 跨语言网关 `eltdx-http`，通过 HTTP JSON 和 WebSocket RPC 调用公开 API。
- WebSocket 支持桥接原生 `0x0547` 行情增量订阅；普通 RPC 仍然是一问一答，推送频率由 7709 主站决定。
- 默认 `pip install eltdx` 不增加 FastAPI/Uvicorn 依赖，也不会启动额外服务；网关依赖通过 `eltdx[http]` 单独安装。
- 新增网关文档、订阅测试、发行包 smoke 检查，并同步包内文档镜像。

## v3.0.5 - 2026-08-27

- `client.corporate.capital_changes()` 和 `adjustment_factors()` 支持代码列表，默认每批 75 只，可通过 `batch_size=1..200` 调整；主站短响应会自动补拉，批次按连接池并发执行。
- `capital_changes()` 覆盖标签 `1..15` 的广义权息、股本变迁、增发、回购、缩股和权证资料；新增 `CapitalChangeBatch` 和 `AdjustmentFactorBatch`。
- 新增 `client.corporate.adjustment_factors()`，仅使用 `0x000f` 标签 `1` 在本地计算事件级前、后复权仿射系数。
- MCP 新增 `eltdx_capital_changes` 和 `eltdx_adjustment_factors`，与 Python API 的批量参数保持同步；MCP 工具总数为 20 个。
- 修正配股字段映射、标签解析、TCP 拆包和复权事件复合逻辑，并补充多市场和批量回归测试。

## v3.0.4 - 2026-08-22

- 修复 `client.trades.all_today()` 和 `client.trades.all_history()` 的分页展开顺序：主站从 `start=0` 返回最新页时，最终结果现在按时间正序返回。
- 保留服务端原始 `absolute_index`，分页合并不会重新编号，便于定位原始成交记录。
- 增加分页顺序回归测试，覆盖当日和历史成交的多页合并边界。
- 修复 v2.0.5 基线导出器遇到 v3 新增模块、方法、数据类或异常时中断的问题；缺失项现在会显式记录并继续生成对照夹具。
- 同步成交明细方法手册、接口总览和包内文档镜像。

## v3.0.3 - 2026-08-20

- 补充最新 A 股股票、ST 和停牌列表封装。
- 新增每日股本、每日涨跌停价、实时榜单、连板天梯和题材强度排行。
- 新增买卖力道、成交对比的命名入口。
- 短线指标补齐开盘价、昨收、开盘量比、普通流通股本、封单等字段；开盘量比按最近 5 个完整交易日计算。
- 更新 README、接口目录、字段手册和 Helpers 文档，并同步 wheel 内置文档。

## v3.0.2 - 2026-08-17

- 成交明细新增 `actual_trades`，在保留原始 `ticks` 的同时排除非成交的 `status=8` 集合竞价快照；09:25、15:00 和普通盘中成交继续作为真实成交保留。
- 新增 `is_actual_trade`、`is_after_hours_fixed_price` 和 `after_hours_trades`，明确识别 15:05-15:30、`status=5` 的盘后固定价格真实成交。
- 成交明细不再把 `status=8` 的原始数量字段解释为竞价匹配量；完整秒级竞价过程、虚拟匹配量和未匹配量统一以 `client.auctions.series()` 为准。
- 修复显式 `connect()` 握手期间并发 `close()` 时仍切换备用服务器的问题，关闭中断现在会立即停止连接流程，避免无谓的 `CloseTimeout`。
- 21 个 7709 原生接口和 16 个 Helpers 页面新增默认折叠、可复制的真实 JSON 返回样本；F10 原生 Entry 页面不新增样本。

## v3.0.1 - 2026-08-17

- `client.auctions.series(code, date=None)` 现在通过 `0x056a` 同时支持当日和历史集合竞价过程快照，并按主站响应顺序保留全部时间点。
- 删除 `client.trades.auction_today()` 和 `auction_history()`；成交明细仍保留原始混合记录及其分类属性，正式 09:25 撮合入口保持不变。
- `client.helpers.auction_data()` 的历史竞价过程改由 `0x056a` 获取；`0x0fc6` 只负责历史 09:25 正式撮合和前收盘参考价。

## v3.0.0 - 2026-08-16

- Native Engine 改为单 Supervisor、自适应 Tokio runtime；worker 默认按 Slot 数和系统逻辑处理器自动计算并允许手动覆盖，单 worker 使用 current-thread 快速路径，两个以上 worker 使用 multi-thread。
- 默认使用测速排名最快2台7709服务器、每台4条连接；新增 canary 握手、受控分批建连、每服务器连接上限和排名后备服务器补位。
- 新增 Engine 全局 raw/decoded 内存预算和峰值 diagnostics；默认 push 字节上限提高到64 MiB，并增加160 Slot压力与资源归零合同。
- 保持 `TdxClient`、模块化业务 API、公开 dataclass、异常、Helpers、F10、MCP 和 CLI，完整 7709 构包、解析和网络运行核心迁入 Rust。
- 新增无 Tokio/Python 依赖的协议 crate、Supervisor/Slot runtime crate 和私有 PyO3 ABI3 扩展 crate。
- 21 个命令全部使用强类型 Rust request/response；公开 `eltdx.protocol` 继续存在，但只委托无状态 native 入口。
- `SocketTransport` 与 `PooledSocketTransport` 统一使用每 Engine 一个 Rust runtime，并实现有界 FIFO admission、absolute deadline、一次安全重试、pin-local FIFO、低优先级心跳和有界 push。
- 保留 diagnostics 和 `ActorSnapshot` 公开字段名；`actor_alive` 现在映射 Rust Slot task，不再存在 Python Actor 运行核心。
- 增加显式 Python/native ABI 校验，移除纯 Python 7709 fallback；安装版本来自 Cargo/distribution metadata。
- 构建产物改为五个 `cp310-abi3` wheel 和一个 sdist，支持 CPython 3.10-3.14 的首发平台矩阵。
- 新增版本绑定的十轮统一测试、黄金 fixture、故障注入、压力、性能、真实主站和安装证据资产。
- 正式版在五个平台完成 wheel 安装验证后，以同一批文件发布到 PyPI 和 GitHub Release。

## v2.0.5 - 2026-08-14

- `client.helpers.factors(code, anchor_date=...)` 支持把指定日期或此前最近交易日的前复权因子归一为 `1`；不传锚点时保持原有结果。
- `FactorResponse` 新增 `anchor_date`，明确返回因子采用的前复权基准日期；后复权因子不受锚点影响。
- `client.helpers.local_adjusted_kline()` 同步支持前复权锚点，并在返回的 `KlineSeries` 中记录锚点日期。
- 本地复权 K 线明确只支持日 K，避免周线、月线按日因子精确日期匹配时静默产生不正确结果；其他周期继续使用 `client.bars` 服务端复权。
- 本地日 K 复权复用同一次不复权日 K 查询，不再重复请求完整日 K 历史。
- 补充锚点计算、返回字段、示例和周期限制文档。

## v2.0.4 - 2026-08-14

- `client.helpers.auction_data()` 现在按是否传入日期使用明确的数据源：当日使用 `0x056a + 0x0fc5 + 0x054c`，历史只使用 `0x0fc6` 自动分页。
- `AuctionData` 新增 `auction_records`，用于返回历史成交明细中的 `status=8` 集合竞价记录；`series` 继续用于当日 `0x056a` 秒级竞价过程。
- 历史查询从同一份 `0x0fc6` 数据中取得竞价记录、09:25 正式撮合和昨收价格基数，不再使用当前行情补历史数据。
- 开盘价、开盘成交量和开盘成交额只取自 09:25 正式撮合；`0x054c` 当日快照只用于补昨收和计算开盘涨幅。
- 补充聚合接口的参数、返回字段、返回示例和底层数据来源说明。

## v2.0.3 - 2026-08-14

- 修复 `v2.0.2` 版本测试仍断言 `2.0.1`、wheel 冒烟脚本仍断言 17 个 MCP 工具导致 CI 失败的问题。
- `client.helpers.auction_data(include_snapshot=False)` 不再在当前交易日额外请求 09:25 正式撮合。
- `TradePage.has_more` 与空页终止的分页规则保持一致，短页不再被误判为已经结束。
- 更新 README 接口目录横幅和当前版本导航，并修正 Helpers 目录说明数量。

## v2.0.2 - 2026-08-14

- 新增 `client.trades.auction_today()` / `auction_history()`，分别查询当日和历史成交明细中的集合竞价记录。
- 新增 `client.trades.opening_match_today()` / `opening_match_history()`，分别查询当日和历史 09:25 正式撮合。
- 删除原先合并查询的 `client.trades.auction_snapshots()` 和 `client.helpers.auction_0925()` 入口；`client.auctions.series()` 继续用于 `0x056a` 集合竞价过程快照。
- 为拆分后的接口补充独立文档和示例，并整理服务器文件相关文档。

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
