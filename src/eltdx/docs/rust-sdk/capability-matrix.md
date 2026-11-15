# 纯 Rust SDK 能力迁移矩阵

本文件把当前 Python 产品能力与已有 Rust 实现逐项对应到计划新增的 `crates/eltdx`。它用于范围核对、分域回迁和验收；表中的目标模块与阶段是开发设计，不表示已经实现。当前工作区只有 `eltdx-protocol`、`eltdx-runtime`、`eltdx-python` 三个 crate，尚无面向普通 Rust 用户的 `eltdx` 公共 crate。

表中的模块名为职责映射，稳定 API、公开路径与类型名称以[公共 API](public-api.md)为准；不把内部物理目录作为兼容承诺。公开 HQ 客户端为 `TdxClient`，HQ 市场类型为 `HqMarket`；EX、MAC 的市场/证券标识保留各自协议类型，Python 既有 `Market` 名称在兼容层保留。

核对基线为 `af471f1adf38757f6aa0188968db8110eb23f6ad`；源码链接固定到该提交，文档站点与包内镜像均可访问。现有实现有冲突或缺口时，本文件单独列出，不能把“Python 能返回某种结果”“目录中有一条能力”“Rust 有该命令解析器”当作同一个完成条件。

## 1. 计数口径和范围

| 口径 | 当前数量与证据 | 能说明什么 | 不能说明什么 |
| --- | --- | --- | --- |
| HQ 唯一 wire command code | **26**；[request.rs](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/crates/eltdx-protocol/src/request.rs) 的 `SUPPORTED_COMMAND_CODES: [u16; 26]`，包含 `0x0FFC` | Rust HQ 请求枚举支持的不同命令号 | 不等于分页、聚合、订阅、元数据和业务服务数量 |
| Python 命令 registry | **25**；[registry.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/protocol/commands/registry.py) 的 `COMMANDS` | 按名称分派的传统 HQ 命令；没有注册 `0x0FFC` | 不包含通过其他适配路径访问的资金流，也不包含 HTTP F10、EX、MAC |
| 展示目录的能力条目 | [interface-catalog-data.js](../assets/interface-catalog-data.js) 当前 **124 条**：7709=26、Helper=23、F10=25、EX=12、MAC=24、MAC-EX=14 | 面向用户的分组目录，含组合方法、别名、客户端管理、静态枚举 | 不等于唯一 wire code；一个命令可供多条能力使用，一条能力也可发多个命令 |
| 目录中的 release 元数据 | 同一文件 `release.native_commands=25`，但 7709 分组描述和条目数为26 | 暴露历史计数口径差异 | 不应从该数值推断 Rust 实际缺少第26个命令 |
| 历史资金流 category 22 | [response.rs](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/crates/eltdx-protocol/src/response.rs) 在 `Klines` 分支按 period raw=22 解析 `HistoryFundFlow` | `0x052D` 的一种请求/响应变体 | **不是第27个 wire code**；也不是当前 `MoneyFlowApi.history()` 的公开数据源 |

25 个 registry 命令及额外 wire 命令应全部建立请求字节和解析验收：

| 族 | wire code | registry 名称/其他路径 |
| --- | --- | --- |
| 会话 | `0004`、`000D` | `heartbeat`、`handshake` |
| 公司资料/资源元数据 | `000F`、`0010`、`02C5`、`02CF`、`02D0` | `capital_changes`、`finance_batch`、`block_meta`、`company_info_category`、`company_info_content` |
| 证券目录/限制 | `044D`、`044E`、`0452` | `security_list`、`security_count`、`special_limits` |
| K线/分时 | `051B`、`052B`、`052D`、`0537`、`0FB4`、`0FD1`、`0FEB` | `intraday_aux`、`delisted_bars`、`klines`、`today_intraday`、`historical_intraday`、`sparkline`、`recent_intraday` |
| 行情 | `053E`、`0547`、`054B`、`054C` | `legacy_quotes`、`refresh_stream`、`category_quotes`、`snapshots` |
| 竞价/成交 | `056A`、`0FC5`、`0FC6` | `auction_series`、`today_ticks`、`historical_ticks` |
| 文件 | `06B9` | `file_content` |
| 当日资金流 | `0FFC` | Rust `MoneyFlow` 请求/响应及 Python 专用路径，不在 `COMMANDS` 中 |

这里的数字为十六进制。Rust 测试函数 `exact_twenty_three_command_codes_are_registered` 的旧名称也不是数量依据：其函数体实际断言26。

## 2. 目标边界、阶段和共同验收规则

公共 crate 同时承载业务实现，内部按 `client`、`types`、`hq`、`services`、`metadata`、`policy`、`subscription`、`adjustment`、`calendar`、`offline` 组织；F10、EX、MAC 用独立协议模块。`eltdx-protocol` 继续承载 wire 编解码和基础字段，`eltdx-runtime` 继续承载连接调度与有界资源，`eltdx-python` 逐域调用 `eltdx`，负责 Python 对象及异常映射。

基础 HQ 随公共 crate 提供；可选 feature 为 `serde`、`offline`、`f10`、`ex`、`mac`。`mac` 启用 `ex` 以支持 MAC-EX 路由；`full` 汇集这五项。没有占位 `async` feature；真正异步 API 另在 P6 设计交付。新 Rust 公共 API 的整次操作 deadline/cancellation 与是否公开 async 无关；Python 原有期限和重试语义通过内部兼容执行策略保留。

| 阶段 | 本矩阵含义 |
| --- | --- |
| P0 | 行为基线、语言无关 fixture、冲突裁决和兼容边界 |
| P1 | 公共 crate 骨架、生命周期、整次操作上下文、显式元数据隔离 |
| P2 | HQ 完整业务，包括订阅、资源、复权、交易日；Python 对应域回迁 |
| P3 | F10、offline、financial，以及真正依赖它们的组合 helpers；继续回迁 |
| P4 | EX、MAC、MAC-EX，以及依赖它们的剩余组合；继续回迁 |
| P5 | 独立打包、全功能兼容与发布 |
| P6 | 可选真正异步 API、原生 CLI/服务；不作为 P1–P5 的隐式前提 |

每一表项都继承以下验收条件，表格只列该族特有条件：

1. 新 Rust 公共入口采用内部 `RustPublicExecutionProfile`：一次用户操作共享同一个绝对 deadline、cancel token、重试预算。元数据预取、分页、缺项恢复、主机探测、F10 补充、缓存刷新不重新获得完整 timeout；取消后不继续派生请求。旧 Python 入口采用内部 `PythonLegacyExecutionProfile`，保留每次请求默认8秒、原startup计时位置以及分页/恢复/切换各层独立限制，不强加新的整次8秒期限或共享尝试上限。两者共用业务算法，差异由执行策略表达，不复制分页/重试实现。长期订阅按生命周期管理，各profile明确activate/renew/poll的原有或新操作范围。
2. 请求元数据采用显式 `MetadataContext`/快照，以客户端或用户显式共享的 store 为边界；写入目录、预取和解码使用同一上下文。新 SDK 不依赖进程全局精度表。
3. 单页 raw 与合并结果来源要明确。聚合没有单一原始报文时，不用第一帧冒充完整 raw；需要逐页证据时提供页级来源。未知字段/原始整数/原始字节在对应模型中保留。
4. 用 Rust 类型表达请求、结果、缺项、部分完成、取消、服务端业务状态和错误上下文。Python 的可变参数、dict/dataclass、warnings、异常链由绑定层投影；不把 `_native_models` 的私有 tuple stride 设为 Rust 公共格式。
5. 必须有纯 Rust 的录制响应/脚本化服务测试，不依赖 Python 才能完成该能力验收；同一 fixture 在 Python 兼容层核对用户可观察行为。
6. 表中“现有 Rust 有解析”只算 wire 基础。只有业务编排、类型、操作预算、资源清理与 Python 回迁均有证据，才把该行标成完成。
7. Rust 与 Python 是两个独立安装产品：Rust 用户只通过 Cargo 依赖库，不安装 Python 包；受支持平台的 Python 用户只通过 pip 安装自带 native 扩展的 wheel，不需要 Rust 编译器、Cargo 或另外安装 Rust SDK。源码安装/开发绑定另行说明工具链要求；P5用无 Rust 工具链的干净 Python 环境安装实际 wheel，并验证 import、查询与 close。
8. Python 回迁必须保持公开 import/别名、签名和默认参数、模型字段与顺序、raw、异常类型及 `__cause__`、close/connect 换代、自定义 transport、已有可选数据补充失败时的降级和文件行为。新 Rust 严格类型或错误策略经适配投影到这些契约；行为修订须单独列出旧新结果、证据和发布决定，不能由迁移隐式引入。
9. 主请求失败与可选数据补充失败分别建模。`full_quotes` 的深度补充、profile/shortline 等已有降级允许返回主结果，同时在 Rust 结果中保留缺失来源和原因；Python 保持旧返回形状及已存在的单请求超时降级条件。`RustPublicExecutionProfile` 的取消或总deadline耗尽终止整次操作，不被best-effort分支吞掉；`PythonLegacyExecutionProfile` 保留原请求异常/信号处理与降级边界，不因引入原先不存在的高阶总期限而提前失败。

### 2.1 两种执行策略的验收边界

这两个profile是共享业务实现的内部策略，不是让Python用户切换新旧算法的公共开关。原生Rust用户取得新公共契约，旧Python入口取得兼容契约；仅映射异常类不足以证明兼容。

| 维度 | `RustPublicExecutionProfile` | `PythonLegacyExecutionProfile` |
| --- | --- | --- |
| 默认期限 | 从公共方法入口起算整次操作8秒，所有子请求使用剩余时间 | 每次native请求按原配置取得8秒；高阶分页/组合调用没有新增8秒总上限 |
| startup与预取 | lazy startup、admission、metadata和主请求计入同一总期限 | 保留当前初始化、DNS、startup及预取的计时边界；例如Engine在`reserve_submission`之后才创建该次请求deadline，预取和后续主请求分别执行。不能把原先在计时外的启动自动计入每次请求 |
| 次数上限 | wire retry、缺项恢复、failover受整次共享尝试预算约束，也遵守业务局部上限 | 保留各层已有计数：缺项恢复最多32次额外请求、资金流每代码/整批探测上限、各分页max_pages和runtime自己的重试限制；不新增跨层共享attempt上限 |
| 测试证据 | 两页各耗时小于8秒而累计超过8秒时，按总deadline终止；取消后不派生请求 | 同一脚本按各页请求期限完成；超过单页期限仍产生原异常或原有降级。另测startup计时边界、多次恢复及各层上限，检查实际请求顺序/次数 |

期限基线来源为 [ApiBase](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/base.py)、[SocketTransport](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/transport/socket.py)、[Python native桥接](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/crates/eltdx-python/src/transport.rs)和[Engine::begin_execute](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/crates/eltdx-runtime/src/engine.rs)。下表提到整次操作预算时均指Rust公共profile；Python兼容入口继承本节边界。

## 3. 客户端、元数据和传输基础

| 能力/当前来源 | 已有 Rust 与缺口 | 目标归属、阶段 | 保留行为和具体验收 |
| --- | --- | --- | --- |
| `TdxClient` 构造、`from_hosts`、`in_memory`、connect/close、context manager；[client.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/client.py) 53、174、279行 | `Engine`/`EngineConfig` 已存在；公共配置、服务装配、专池和缓存仍在 Python | `client::{TdxClient,ClientBuilder}`、`services`；P1→P2 | 构造不偷偷发业务请求；显式 connect 与惰性首次请求行为有文档；close/Drop 回收主池、竞价池和资金流池。部分初始化失败回收已建连接；重复关闭、并发关闭、旧 epoch 对象均可测 |
| 三套市场连接：普通HQ、懒建竞价专池、懒建资金流专池；[client.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/client.py) 226–257、293行，[auctions.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/auctions.py)、[money_flow.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/money_flow.py) | Rust Engine 可复用；专用路由策略没有公共服务层 | `client`、`services::routing`、`policy`；P2 | 专池独立；Rust公共profile共用整次预算，Python兼容profile保持原专池请求期限及各层限制；并发首次使用只建一次；切换失败不污染普通HQ；最后一个所有者释放后全部 worker/连接归零 |
| host字符串、IPv6、默认7709、DNS、配置和排名；[hosts.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/hosts.py)、[_paths.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/_paths.py) | Rust `endpoint`/`supervisor` 有连接及切换；Python 包资源、路径、排名持久化仍需迁移 | `client::hosts`、`policy::hosts`；P1→P2 | 保留 host 去重与顺序、IPv6括号语义、端口验证；探测默认关闭，不能因 SDK 构造自动联网或写排名。排名保留暂时不可达主机；显式刷新才按策略更新；用户路径优先级与包内默认地址可测试 |
| pool layout、CPU/亲和感知、全局/slot内存预算；[transport/_config.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/transport/_config.py) | Rust 已有 Engine配置、memory budget、connection limiter；Python重复校验需整合 | `client::config` 解析为 `EngineConfig`；P1 | 不一致的 pool_size/server_count/connections_per_server 明确报错；每host上限有效；预算上下限、worker数和连接分布与诊断一致。P0记录现有自动值，不复制两套逐渐漂移的计算 |
| `Transport`、`InMemoryTransport` 自定义适配；[base.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/transport/base.py)、[memory.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/transport/memory.py) | Rust低层请求已有类型；缺少注入业务级测试传输的公共边界 | `hq::transport`/内部 transport trait，测试用脚本传输；P1 | Rust默认走Engine；Python仅实现旧 execute 的自定义transport仍可经适配使用。测试传输必须表达命令、返回/错误、延迟和取消，不强制所有实现伪装为 socket |
| `SocketTransport`、`PooledSocketTransport`、pin；[socket.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/transport/socket.py)、[pool.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/transport/pool.py) | Engine已有 admission、slot、pin、deadline、push、重连、close rollback | `client` handle/`hq` session；P1→P2 | 保留pin连接亲和、旧epoch失效、FIFO/容量、取消终结一次、失败回收；长下载和订阅不能在持有所有slot后请求元数据，防止自锁 |
| Actor/Pool/Transport diagnostics；[actor.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/transport/actor.py)、[pool.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/transport/pool.py) | Rust有diagnostics与snapshot；Python enum/dataclass投影 | `types::diagnostics`、绑定层映射；P1 | 持续有界诊断读取不占业务命令队列；计数表达一致；关闭后的slot/内存/活动lease状态有测试；Rust不暴露Python Actor历史私有实现 |
| 精度预取、证券分类、市场归一化；[client.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/client.py) 362–413、[api/base.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/base.py) 51、[protocol/unit.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/protocol/unit.py) 82 | [unit.rs](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/crates/eltdx-protocol/src/unit.rs) 有进程全局 `OnceLock<RwLock<HashMap<...>>>`；解析会读该表 | `metadata` + `types::SecurityId` + 上下文解析路径；P1→P2 | 新请求显式携带元数据快照；证券目录预取先于需要精度的解码。两个client对同一代码注册不同精度、并发解析/clear/重连时互不影响；混合市场与裸基金/北交代码覆盖，未知时采用明确 fallback/error 策略 |
| `clear_cache` 与服务缓存；[client.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/client.py) 354、[helpers/core.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/helpers/core.py) 289 | 缓存主要在Python；clear当前不清WorkdayService | `metadata`、`services::cache`、`calendar`；P1→P3 | 定义缓存作用域、来源、日期、刷新和失败是否可缓存。新SDK显式全清/分域清理；Python旧clear语义保留或经P0宣布调整。失败空finance缓存不得被误当永久完整结果 |
| ABI/异常/对象边界；[_native_abi.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/_native_abi.py)、[_native_models.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/_native_models.py)、[transport/native.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/transport/native.py)、[exceptions.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/exceptions.py) | `eltdx-python` 已有PyO3请求/DTO/异常映射 | `eltdx::Error` + `eltdx-python`；P1→P5 | 新SDK依赖树无PyO3；错误保留类型、命令、host、阶段、重试/缺项上下文。Python ABI fail-fast、既有异常类和真实cause链继续单独验收 |
| 公共frame/command facade；[protocol/frame.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/protocol/frame.py)、[commands/codec.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/protocol/commands/codec.py)、[commands/registry.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/protocol/commands/registry.py)、[constants.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/protocol/constants.py) | frame与命令编解码已委托Rust；Python仍提供公开值对象、名称和常量 | `eltdx-protocol`复用 + Python兼容facade；P1→P5 | RequestFrame/ResponseFrame的control、msg_id、reserved、raw与负载界限保持；25个名称和26个wire code按第1节分别核对。Rust公开强类型命令，不把动态dict作为唯一入口 |
| 文本codec和限量解压；[protocol/codec.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/protocol/codec.py)、[decompress.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/protocol/decompress.py) | UTF-8文本兼容函数仍在Python；HQ Rust frame已有解压校验 | `eltdx-protocol`/Python兼容工具；P1→P5 | 文本UTF-8错误语义与旧导出保留；HQ单帧解压上限65535、流终止、声明长度、截断和尾随数据都核验。不能因名称都是codec而把文本函数误当二进制协议入口 |

旧 `eltdx-protocol` 公开解析函数和注册函数已被使用时，不在P1直接删除：新增显式上下文解析路径供SDK使用；旧签名可暂时通过标记为legacy的上下文适配维持兼容，并在文档中说明其全局行为和废弃路线。验收必须证明新SDK路径不会读写旧全局注册表，不能只把全局表隐藏到另一个模块。

## 4. HQ API 逐族矩阵

| Python业务入口与来源 | 已有Rust基础 | 目标模块/阶段 | 行为与验收场景 |
| --- | --- | --- | --- |
| `SessionApi.ping/handshake/heartbeat`、`api.health.ping`；[session.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/session.py)、[health.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/health.py) | session wire解析、Engine握手和低优先级heartbeat | `hq::session`、`client`；P2 | 区分本地可用性ping与真正网络握手；服务端日期/时间及raw完整，握手失败不发布半成品连接；压力解除后heartbeat恢复。health无client的in-memory行为留在Python兼容层 |
| `CodeApi.count/list/all`、stock/a_share/ETF/index筛选、跨市场目录、latest_stock_list/latest_st/latest_suspended及别名；[codes.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/codes.py) | `044D/044E` 和证券分类类型已在protocol | `hq::securities`、`metadata`、`services::universe`；P2 | all直到空页，按实际返回长度推进；0条limit与page_size边界区分；跨市场顺序明确；目录写当前元数据store；ST前缀与停牌bit `0x20`验证。停牌每80证券取legacy行情，缺项不编造正常状态 |
| `QuoteApi.get_snapshots`；[quotes.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/quotes.py) 14、[_batch.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/_batch.py) | `054C`解析已在Rust；缺项恢复仍在Python | `hq::quotes`、`services::batch`；P2 | 保持请求顺序和重复代码；初始请求后最多32次恢复；仅规定的协议错误允许二分，transport错误透传，坏单例不能当missing。全缺、部分缺、重复项、预算耗尽和raise/skip行为均有fixture |
| `legacy/get_depth/list_by_category/refresh`；[quotes.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/quotes.py) | `053E/0547/054B`已在Rust | `hq::quotes`；P2 | legacy上限、refresh最多100项、sort/category原始值与别名；元数据预取先于价格解析；保留零值/缺值差异、深度层数、状态位和未知字段 |
| `poll_push/drain_pushes/subscribe/subscribe_hq`；[quotes.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/quotes.py)、[subscription.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/subscription.py) | runtime已有有界push队列和gap；订阅分组/续订/分发仍在Python | `subscription`、`services::push_router`；P2 | 每100代码占一原连接pin，先元数据后占pin；默认7秒续订；最新收到的时刻作为cursor，不能擅自取max阻止跨会话回退。多订阅由共享router分发，不竞争消费丢彼此数据；部分激活失败释放全部已持pin，关闭唤醒poll；gap可观察，callback错误不会泄漏lease |
| `BarApi.get` 单/多证券、分页、all_pages、数量、adjust、bar_time；[bars.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/bars.py) | `052D`类型/解析已在Rust；多证券和聚合仍在Python | `hq::bars`、`services::pagination`、`adjustment`；P2 | start=0是较新页，短页不是终点；空页终结、max_pages有错误；倒转页序不倒转页内顺序；合并raw清空或给页级raw。多代码归一化/去重后按输入顺序返回；周期和时间对齐见第7节 |
| `DelistedBarApi.get`；[delisted_bars.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/delisted_bars.py) | `052B`解析已在Rust | `hq::bars::delisted`；P2 | `T`代码、日期/索引、请求参数及返回排序有golden；当前聚合排序并保留第一raw与其他all接口不同，必须P0裁决，不能无意复制成共同聚合策略 |
| `MinuteApi.today/history/recent/aux/buy_sell_strength/volume_comparison/sparkline`；[minutes.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/minutes.py) | `0537/0FB4/0FEB/051B/0FD1`解析已在Rust | `hq::minutes`、`services::batch`；P2 | today/history/recent多代码归一化去重、空输入拒绝、并发有界、顺序可预测；aux种类0/11字段不同；minute totals/raw/date与未知字段保持；价格显式元数据解码 |
| `TradeApi.today/history/all_today/all_history` 及多证券；[trades.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/trades.py) | `0FC5/0FC6`解析、event kind和raw存在 | `hq::trades`、`services::pagination/batch`；P2 | 空页终止而非短页；倒转页不改同分钟源顺序、source index/absolute index；保留原max_pages；Rust公共profile另受整次deadline约束，Python兼容profile仍按各页请求计时；以真实已成交/竞价快照/09:25正式撮合/盘后固定价格分类，保持原状态 |
| `history_batch/today_batch/all_history_batch/all_today_batch`、`TradeBatch`列访问/选择/物化；[trades.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/trades.py) 123、158、[trade_batch.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/models/trade_batch.py) | 历史bulk PyO3通路存在；native adapter当前拒绝today bulk | `hq::trades::batch`，Rust行/列视图；P2，P0先定缺陷 | Rust不复制19字段Python tuple为稳定协议；列选择不需逐条建Python对象，负索引/重复索引由Python适配处理；子集raw清空。新Rust直接API用真实编解码/传输路径证明today bulk可用，不能只通过宽松mock；旧Python真实Socket/Pool路径在另行批准修复前继续抛原ValueError及原消息。自定义transport已有的成功路径（execute_trade_batch或原execute回落）保持可用，并分别验收 |
| `opening_match_today/opening_match_history`；[trades.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/trades.py) 268 | tick语义解析已存在；查找编排仍在Python | `hq::trades::opening_match`；P2 | 查找正式09:25撮合，不用竞价过程快照替代；找不到返回可区分的None，页上限/网络失败是错误；历史和当日各有录包 |
| `AuctionApi.series`；[auctions.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/auctions.py) | `056A`解析已在Rust | `hq::auction`、专池routing；P2 | 当前/指定日期、价格数量及估算金额单位；懒建专池与close；专池超时与普通池独立；Rust公共profile服从整次operation预算，Python保留原单请求期限 |
| `CorporateApi.capital_changes/adjustment_factors/finance_batch`；[corporate.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/corporate.py) | `000F/0010`解析已在Rust；恢复、投影、复权系数在Python | `hq::corporate`、`adjustment`、`services::batch`；P2 | 股本多代码服务端可能只返回请求前缀，继续请求未返回部分；finance保留重复请求及on_missing；fields投影属于边界而非新wire；股本×10000，资产/利润×1000；同日多事件顺序和scale+offset验收 |
| `CompanyInfoApi.category/content`；[company_info.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/company_info.py) | `02CF/02D0`解析已在Rust | `hq::company_info`；P2 | 目录filename/start/length和GBK正文保存；按code+market请求，偏移长度字节语义一致。这是HQ公司文本能力，不依赖HTTP `f10` feature |
| `LimitApi.special/scan_special`；[limits.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/limits.py) | `0452`解析已在Rust | `hq::limits`；P2 | 服务端特殊限制表与本地涨跌停规则分开；扫描按实际条数推进、空页停止，max_rows界限与最后页可能越界现状在P0固定；未知证券/限制标志不丢 |
| `ResourceApi.read/block_meta/download_file/read_stats`；[resources.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/resources.py) | `06B9/02C5`解析已在Rust；整文件下载与ZIP解析在Python | `hq::resources`、`services::download/stats`；P2 | 整次下载pin同一连接；chunk_len=实际长度且不超请求，host变化报错；无截取上限时精确chunk边界需额外空读。Rust下载结果显式区分“已观察到EOF而完整”与“达到字节上限、完整性未知”，仅达到cap不能判定完整；严格调用方可按该状态拒绝截取。Python `download_file(max_bytes=...)`继续返回原截取bytes，不因新Rust状态改为报错；read_stats仍用上限+1识别超大档案。逐块、cap为0/精确cap/超cap、取消及异常后回收有测试 |
| `MoneyFlowApi.daily` 单/多证券；[money_flow.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/money_flow.py) 145、316 | `0FFC`解析已有；空返回切换和偏好host在Python | `hq::money_flow`、`policy::empty_failover`；P2 | 每代码最多3次候选探测，总预算 `min(N,8)*3`，轮转起点；首个有数据host后续复用、没用的probe池关闭。全空返回空；不能把某只无数据推广成host全局失效。transport失败与空业务结果分别测试 |
| `MoneyFlowApi.history`；[money_flow.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/money_flow.py) 278 | public路径不使用Rust category22；当前走F10 | `services::money_flow_history`、`f10`；P3 | entry=`CWServ.tdxf10_gg_sj`，Params为`zjlx_sj,裸代码,str(days),裸代码`；days限3/5/10/20/60；将服务端日期升序记录反转为最新在前，再start/count切片；未知N00x保留。禁用f10时明确能力不可用，不回落category22假装同一数据 |
| `ApiBase`、`_batch`、`api.__init__` | Python负责弱引用回调、命令名分派和missing编排 | 内部 `services` + Python兼容facade；P1→P2 | 不给Rust业务公开Python弱引用工厂；内部服务持有可回收共享句柄；兼容层不再复制分页/重试。全部回迁后旧API文件仍可作为import入口存在 |

主要既有测试来源：[test_client.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/tests/test_client.py)、[test_helpers.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/tests/test_helpers.py)、[test_protocol_unit.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/tests/test_protocol_unit.py)、[test_trade_batch.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/tests/test_trade_batch.py)、[test_company_info.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/tests/test_company_info.py)、[test_delisted_bars.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/tests/test_delisted_bars.py)、[test_money_flow_failover.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/tests/test_money_flow_failover.py)、[test_fund_flow_history.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/tests/test_fund_flow_history.py)、[test_subscription.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/tests/test_subscription.py)。这些测试是回归输入，不替代上述新增纯Rust场景。

## 5. Helpers 必须按真实依赖拆分

[helpers/core.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/helpers/core.py) 同时包含HQ聚合、F10聚合和纯计算，不能整体搬迁后统一标为P2。所有别名均为同一业务能力的Python兼容入口，不重复统计交付数量。

| 方法/模型族 | 当前数据路径及关键行为 | 目标、阶段和依赖 | 具体验收 |
| --- | --- | --- | --- |
| `full_quotes` | 每80代码取snapshot，再best-effort合并refresh的五档；仅当买卖各≥5档时合并，opening amount采用refresh | `services::quotes`；P2，依赖HQ snapshot/refresh | 1档不会覆盖成伪五档；单证券/多证券、无refresh、部分refresh、重复输入、非标准transport返回有基线。新SDK结果给出深度来源/降级状态 |
| `latest_stock_list/latest_st/latest_suspended` | 调用CodeApi，helper停牌输出SecurityCode行而CodeApi输出代码 | `services::universe`；P2 | 保留两种返回形状的Python投影；Rust用统一实体和明确查询结果，不靠同名方法隐式变化 |
| `buy_sell_strength/volume_comparison` | 归一化代码后调用aux0/11 | `hq::minutes`快捷方法；P2 | 与底层同fixture，别名不增加额外请求 |
| `realtime_rank/stock_realtime_rank` | `054B`分页+代码表补name；rank从1递增；当前短页即停止 | `services::ranking`；P2 | sort/category/count/ascending、稳定排序、缺name及跨页rank；短页停止规则单独保持/裁决，不直接套ticks的空页策略 |
| `daily_share_capital/daily_shares` | handshake/交易日 + 两份stats日期验证 + finance；free_float万股×10000；记录share_source与finance日期 | `services::share_capital`；P2 | 总股本/流通/自由流通来源可追溯；缺部分源保持None；目标/前交易日资源及95%覆盖验证 |
| `daily_price_limits/stock_daily_price_limits` | 指定真实交易日，未复权前收盘+当日股本事件调整+IPO日期/上市天数+本地价格规则 | `services::price_limits`、`policy::price_limits`、`calendar`、`adjustment`；P2，先P0冲突裁决 | ST/创业/科创/北交/新股/无前收盘、同日多事件和价格舍入；返回no_price_limit与missing_pre_close区别；不能只证明计算得出一个数字 |
| `shortline_indicators` | [shortline.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/helpers/shortline.py)：目录、stats、行情、市场日期；21个指标与涨停/开板/封单/连板解释 | `services::shortline`、`policy`；P2 | 09:25前未ready；握手服务时间为准；两表主导日期一致且各≥95%覆盖，只接受目标/上一交易日；上一日资源列移位；缺证券字段为None而非0 |
| `limit_ladder/stock_limit_ladder` | shortline筛sealed，可选touched；按层数、封单额、代码排序后截断 | `services::limit_ladder`；P2 | 相同层数/金额稳定顺序、包含touched、count非法、缺封单/层数；阶段不能早于shortline |
| `market_stat` | [market.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/helpers/market.py)：legacy伪指数880005/880001/880006；计数×10，市值×1e10 | `services::market_stat`；P2 | 数量、停牌残差不小于0、金额单位；当前按返回位置关联存在缺项错位风险，P0确定后新SDK按证券身份验证来源 |
| `today_fund_flow`、`classify_fund_flow` | all_today成交；跳过竞价快照和中性方向，按金额4万/20万/100万分桶，包含实际正式撮合；金额price×volume×100 | `services::fund_flow`纯聚合；P2 | 恰等阈值、买卖中性、竞价过程、09:25撮合、盘后固定价分类；与`0FFC daily`是不同来源和含义，不互换 |
| `stock_profile_table`、StockProfile | full_quotes + 可选security/finance；按输入代码构造行；缺字段None | `services::profiles`；P2 | 可选依赖关闭不发请求；局部finance失败可见；市值/收益率等零值与缺值区别；避免复用永久缓存的失败空结果 |
| `auction_data`、AuctionData | 当前/历史竞价过程 + 正式撮合 + 行情前收；历史从历史成交price_base取前收，可显式传入前收 | `services::auction`；P2 | 各include flag正确减少请求；历史不得用当前quote补前收；前收为0/缺失、无撮合、仅过程数据有fixture |
| `board_quotes/board_member_quotes` | [boards.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/helpers/boards.py)：从HQ下载infoharbor/tdxhy/base等资源、定义文件、目录和snapshot组合；按服务日期缓存 | `services::boards`；P2，使用内部必要资源解析；公开offline接口P3 | 覆盖概念/风格/地域/880行业/申万分级；原始成员、当前有效成员、排除原因三份保留；不能把board_members分红表混用。新交易日目录刷新、文件失败/过期缓存来源显式；DBF长度边界验证 |
| `index_members/index_etfs/ipo_calendar` | F10查询、kind+code缓存；index_etfs返回跟踪产品，不全是ETF；成分权重不存在不能编造 | `services::indices`、`f10`；P3 | 证券清单与产品类型、申购/上市日期和单位一致；refresh使缓存更新；禁用f10时能力明确不可用 |
| `board_members`、BoardMemberTable | F10分红记录自动分页与记录级去重；同股票可多条，`total`是记录数 | `services::board_distributions`、`f10`；P3 | 去重键含代码/方案/日期/报告期；同日跨页不稳定排序不会漏项；`.stocks/full_codes`只是投影视图 |
| `stock_topics`、StockTopic(s) | F10 topic_ids+hot_topics按id/名称合并，保持发现顺序、source集合及两份raw | `services::topics`、`f10`；P3 | 缺id仅名称、两个源互补/冲突、未知类别及raw；不能按代码表误判为行情板块 |
| `topic_stocks`、TopicStockTable | 先按seed解析id/name，再调topic_compare；section/sort透传 | `services::topics`、`f10`；P3 | 指定id无需多余查找，指定name失败明确；无id/name默认首题材的旧行为保留在compat；明确结果顺序 |
| `theme_strength_rank/stock_theme_strength_rank` | 先全量limit_ladder，再逐股F10题材，按涨停数/最高板/连板数/封单额排序后topN | `services::theme_strength`；P3，依赖P2 shortline和P3 topics | 不提前截断梯队；题材源失败当前被吞掉，Rust部分结果需携带缺失证券/原因；同指标排序和leader选择可复验 |
| `clear_cache`、内部security/finance/index/stats/board caches | security按market；finance按有序代码tuple；F10按kind/code；stats按path和有效交易日；board含磁盘日期 | `services::cache`；P1→P3 | 并发首次加载、刷新冲突、失败cache、跨日期、显式共享/隔离均有测试；cache命中不改变所选profile的期限/取消规则，不为Python引入新的总期限 |

当前这些Helper不要求MAC；P4的“余下组合”指MAC/MAC-EX客户端自身的板块排名、全量逐笔、宏观目录与跨协议补偿等组合，不应为排期整齐把上述F10能力提前标到P2。

## 6. 公共模型和序列化覆盖

Rust已有wire结构不等于已经具备Python全部便捷属性。迁移时检查**字段、原始值、单位、派生属性、缺值和顺序**；Python dataclass的slots/frozen/构造签名留在绑定层。

| 当前模型文件与完整类型族 | Rust目标与阶段 | 必须保留/验证 |
| --- | --- | --- |
| [enums.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/models/enums.py)、[security.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/models/security.py)：Python `Market`、`SecurityCode` | `types::{HqMarket,SecurityId,SecurityMetadata}`；P1→P2 | HQ 0/1/2与EX market id、MAC及F10 setcode不共用封闭枚举；Python保留既有Market导出和取值；full_code、decimal、category/board/reason及未知值保留 |
| [session.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/models/session.py)：`HandshakeInfo`、`HeartbeatAck`；[response.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/models/response.py)：`Response` | `types::session`；P2；Response兼容壳可留Python | 服务器双日期、时间、名字/raw；in-memory原样payload仅作兼容测试，不把任意Python对象带入Rust公共类型 |
| [quote.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/models/quote.py)：`QuoteLevel`、`QuoteSnapshot`、`LegacyQuote`、`CategoryQuoteRecord/Page`、`QuoteRefreshRecord/Page` | `types::quotes`；P2 | 深度数量、金额、change/change_pct、封单、交易状态raw，sum_buy/sell及0分母规则；snapshot仅顶层深度，与refresh五档不混淆 |
| [kline.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/models/kline.py)：`KlineBar/Series`；[minute.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/models/minute.py)：`MinutePoint/Series`、`MinuteAuxPoint/Series`、`SparklineSeries` | `types::bars/minutes`；P2 | 原始OHLC、周期raw/parameter、adjust/kind、date/time、volume/amount、成交量累加、unknown/raw；同名Bar跨HQ/EX/MAC字段含义不强行统一 |
| [trade.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/models/trade.py)：`TradeTick/Page`；[trade_batch.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/models/trade_batch.py)：`TradeBatch` | `types::trades`、内部bulk表示；P2 | index/absolute index、None日期与真实时间、side/status/event_kind、auction matched/unmatched、盘后、金额和过滤视图；bulk/raw/选择与物化语义见HQ矩阵 |
| [auction.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/models/auction.py)：`AuctionPoint/Series` | `types::auction`；P2 | 过程与正式撮合类型区分、estimated金额属性、full_code/count |
| [corporate.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/models/corporate.py)：`CapitalChangeRecord/Block/Batch`、`XdxrRecord`、`EquityRecord/Response`、`FactorRecord/Response`、`AdjustmentFactor/Response/Batch`、`FinanceRecord/Batch`、`CompanyInfoCategory`、`BlockMeta` | `types::corporate/resources`、`adjustment`；P2 | 原始category和c1–c4解释、events顺序、items/records投影、scale与offset、历史别名；财务单位不能从字段名猜测；目录字节范围及MD5 |
| [limit.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/models/limit.py)：`SpecialLimitRecord/Page` | `types::limits`；P2 | 服务端原始限价及换算后价、full_code/count，不并入本地规则结果而丢来源 |
| [money_flow.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/models/money_flow.py)：`MoneyFlowDaily/Block/Batch`、`FundFlowHistoryRecord/Series`、`FundFlowRangeSeries` | `types::money_flow`；P2 daily/legacy category22，P3 F10 range | 当前daily、遗留category22、F10区间三种来源可区分；净额/比例聚合、不认识N00x、原始字段及日期方向 |
| [resource.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/models/resource.py)：`FileContentChunk`、`TdxStatRow`、`TdxStat2Row`、`TdxStatsResource` | `types::resources`、`services::stats`；P2 | chunk is_last只是单请求判断；两表原始列、按(market,code)查找、日期计数/覆盖度、空字符串与非有限数None |
| [helpers/core.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/helpers/core.py)：`StockProfile/StockProfileTable`、`StockTopic/StockTopics`、`TopicStock/TopicStockTable`、`DailyShareCapital/DailyShareCapitalTable`、`DailyPriceLimit/DailyPriceLimitTable`、`RealtimeRankRow/RealtimeRankTable`、`LimitLadderTable`、`ThemeStrengthRow/ThemeStrengthTable`、`AuctionData` | `types::services`；随P2/P3对应服务 | count、来源、目标日/参考日、缺字段、排行、raw；F10组合模型不能因为wire没有对应结构而漏掉 |
| [helpers/boards.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/helpers/boards.py)：`BoardQuoteRow/BoardQuoteTable`、`BoardMemberQuoteTable`；[shortline.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/helpers/shortline.py)：`ShortlineIndicator/ShortlineIndicatorTable`；[market.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/helpers/market.py)：`MarketStat`、`FundFlow` | `types::services`；P2 | 成员排除信息、资源准备日、每指标来源/None、资金流阈值与合计 |
| [offline/models.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/offline/models.py)、[offline/gbbq.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/offline/gbbq.py)：`DailyBar`、`MinBar`、`ExDailyBar`、`BlockInfo`、`CustomerBlock`、`FinancialFileInfo/Record`、`GbbqRecord` | `offline::types`；P3 | file raw、保留字段、文件格式专用单位；离线ExDailyBar仅需offline feature，不要求联网ex feature |
| [serialization.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/serialization.py)：`to_jsonable/to_json` | 可选 `serde` 的Rust映射 + Python兼容导出；P1→P5 | Python日期ISO、bytes hex、tuple/set→list、dict key字符串化、TradeBatch显式物化后导出；Rust serde另有明确格式版本，不承诺Python任意set次序。JSON不承载Rust所有请求/结果的唯一类型 |

## 7. 纯计算、交易日、统计资源

| 当前来源 | 目标/阶段 | 验收要点 |
| --- | --- | --- |
| [equity.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/equity.py) `build_adjustment_factor_response` | `adjustment`；P2，可在无网络时计算 | 只取category1及有效日期；`m=(10+c3+c4)/10`、`o=(c1-c4*c2)/10`，qfq/hfq不同方向的仿射组合；`m<=0`错误；anchor/start过滤、同日源顺序、空事件恒等系数。不能退化成只有乘数的复权因子 |
| [_bar_time.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/_bar_time.py) 所有对齐函数/映射 | `types::time`/内部 `policy::bar_time`；HQ P2、EX/MAC P4 | start不偏移；day以上和seconds不按分钟偏移；HQ/EX同日时分回绕，MAC真实duration可跨日；period8为HQ参数分钟/EX3分钟/MAC5×times。边界23:xx、上午结束、下午开始和秒值分别测试 |
| [workday.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/workday.py) 全部方法 | `calendar::{TradingCalendar,CalendarProvider}`；P2 | 绑定client默认以`sh000001`全量日线的真实日期集合为准；未绑定才weekday fallback；normalize/text/same_day、闭区间range/反向、include_self、范围外None。客户端已失效不能静默退回weekday；refresh/clear有显式作用域 |
| [stats.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/stats.py) `parse_tdx_stats_archive`及行解析 | 内部 `services::stats`，HQ资源能力不因公开offline feature关闭而消失；P2 | ZIP压缩体≤32MiB、每entry≤32MiB、累计≤64MiB、entry≤128；唯一目标`tdxstat.cfg`/`tdxstat2.cfg`、拒绝加密；重复证券key拒绝；非有限数→None；目标日期/覆盖率由调用服务校验 |
| [protocol/unit.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/protocol/unit.py)、Rust [unit.rs](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/crates/eltdx-protocol/src/unit.rs) | `types`、`metadata`和protocol上下文解码；P1→P2 | 代码/日期/category/sort/period归一化；价格精度来源；tick手→股×100，可转债tick按milli，K线手数不能错误÷100；原始字节不受单位转换影响 |

## 8. Offline 与专业财报完整边界

`offline` feature 覆盖**读取、编码、增量写入和本地路径发现**，不是“下载数据”的别名。它可在不建网络client时使用。`financial` 是使用HQ计算服务器下载与offline财报解析的组合服务，归属 `offline` 可选域，复用基础HQ；不应依赖HTTP `f10` feature。

| 当前模块/函数范围 | 目标模块、阶段 | 必须保留/裁决的行为与验收 |
| --- | --- | --- |
| [paths.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/offline/paths.py)：detect_tdx_home、resolve_vipdoc、market_to_exchange；`__init__.py`重导出 | `offline::paths`；P3 | 显式vipdoc优先，TDX_HOME后平台候选；缺目录明确错误；当前market只支持SZ/SH，不能宣称自动支持北交。目录定位与文件读取分开测试，feature关闭不访问本机目录 |
| [daily_bar.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/offline/daily_bar.py)：read/encode/find、detect_security_type、security_coefficients | `offline::bars::daily`；P3 | 32字节`<IIIIIfII>`、各证券类型价格/量系数、amount f32、adjust_flag/raw；读只取完整记录并忽略尾片；未知类型返回UNKNOWN但系数当前回落(0.01,0.01)，不能文档误称未知一律拒绝 |
| [min_bar.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/offline/min_bar.py)：read/encode lc1/lc5与.5，三个find函数 | `offline::bars::minute`；P3 | .lc1/.lc5为f32 OHLC，.5为整数价格÷100；2字节日期压缩/午夜分钟；lc1路径`minline`，lc5/.5路径`fzline`；raw/reserved及尾片忽略；往返用f32精度和整数范围验收 |
| [ex_daily_bar.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/offline/ex_daily_bar.py)：read/encode | `offline::bars::extended`；P3 | 32字节`<IffffIIf>`；同一4字节amount同时保留u32位模式和HK f32视图；encode使用原amount位模式；不能转换后丢位；末尾片段忽略 |
| [write_daily.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/offline/write_daily.py)、[write_min_bar.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/offline/write_min_bar.py)、[write_ex_daily.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/offline/write_ex_daily.py) 的append/write/get_last | `offline::writer`；P3 | 现有write实为追加，不覆写；仅过滤大于文件末尾时间的输入，**不对同一输入batch排序/内部去重**。同日/旧数据跳过；缺文件/空文件可写；返回实际条数；daily会flush+fsync，minute/ex当前不保证同样耐久级别，P0明确新writer选项 |
| [alignment.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/offline/alignment.py)：ensure_aligned_for_append | `offline::writer`；P3 | 读端可忽略残尾，写端在筛选新行**之前**拒绝未对齐；即使空输入也不能掩盖坏文件；错误不改原文件。自动截断不是既有功能；人工/显式修复后才追加，不能在迁移时偷偷修文件 |
| [block.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/offline/block.py)：read_block/read_customer_block | `offline::blocks`；P3 | 普通dat 384头+count+2813固定记录，缺完整记录报错，stock_count最多读400但保留声明count；name GBK；客户cfg120字节+blk，去首位市场码的旧投影；路径仅叶名、拒绝分隔符/盘符/..、解析后仍在目录内；坏配置与缺文件分别测试 |
| [industry.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/offline/industry.py)：parse_tdxhy_cfg/read_industry_cfg | `offline::industry`；P3 | GBK替换解码，按`\|`取code/TDX行业/申万行业，少列/非6位code跳过，同码后值覆盖；未知行业保留字符串；新强校验只能作为另一路径 |
| [gbbq.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/offline/gbbq.py)：read_gbbq及静态key | `offline::corporate`；P3 | count+每29字节记录、3轮8字节块解码+5字节尾、u32 wrapping、原category和4个浮点解释；与HQ事件映射显式。缺头当前空、记录截断当前底层struct错误；Rust转结构化格式错误，不越界/恐慌；golden测试完整key与多记录 |
| [history_financial.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/offline/history_financial.py)：文件列表、parse_financial_dat、read_dat_from_zip、read_history_financial | `offline::financial`；P3 | zip≤64MiB、≤64entry、dat≤256MiB；report_size为4倍数、每行≤4096 floats、累计≤4Mi floats、重复有效offset拒绝；零/短头/截断索引/越界offset的既有空或skip规则有golden；报告日期override与header分别测，原float及未知列保留 |
| 同上ZIP选择 | `offline::financial`；P0裁决→P3 | 注释称“唯一.dat”，但实现只拒绝同名重复，再取第一个不同名.dat。必须决定新SDK拒绝多个dat或显式选择，Python旧行为保留/发布调整，不能直接宣称现状是严格唯一 |
| [financial.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/financial.py)：CALC_HOSTS、open_calc_client、list_archive_files/download_archive/download_records | `services::financial` + `offline::financial`；P3 | 计算服务器与普通HQ路由分开；`tdxfin/gpcw.txt`列表、64MiB下载上限在读取中生效、ZIP限制、文件名报告期和None/空数据；列表MD5/size当前只是元信息未校验，新增完整性验证需明确策略/测试；无偷偷联网的构造 |

验收复用 [test_offline.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/tests/test_offline.py) 和 [test_financial.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/tests/test_financial.py) 的合成二进制及边界场景；再补 Rust `Read/Write/Seek`/文件错误、取消、整数越界、完整格式往返。PyO3/全局证券精度不得成为离线解析的依赖。

## 9. F10、EX、MAC交界和产品入口

扩展协议的详细公共API另见对应设计；本表确保业务矩阵没有漏掉跨域依赖。

| 来源/能力 | 当前Rust状态 | 目标与阶段 | 边界验收 |
| --- | --- | --- | --- |
| [f10/client.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py)、[_http.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/_http.py)、[models.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/models.py)、[fields.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/fields.py)、[fields_mg_hk.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/fields_mg_hk.py)、[news.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/news.py)、`f10.__init__` | 无Rust F10 HTTP业务 | `f10`、`services`；P3 | 通用entry/body/params入口、不同hot/page2/qhf9路由、多结果集、重复/缺失列名、有序raw值、业务ErrorCode；代理/TLS/DNS行为；字段标签按entry/params scope。F10资金流、指数/IPO/分红/题材/helpers真实依赖本域 |
| [ex/](https://github.com/electkismet/eltdx/tree/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/ex) 客户端/host/frame/commands/models/transport | 当前Rust HQ engine不能直接替代EX的握手和响应关联 | `ex`；P4 | 7727固定setup、完整16字节头和有界解压、不同market id、GBK、请求序号、单连接串行、host探测/切换；行情/K线/范围K线/分时/当前历史成交/目录覆盖 |
| [mac/](https://github.com/electkismet/eltdx/tree/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/mac) 客户端/codec/bitmap/commands/transport/adjust/price_rules | 无Rust MAC业务 | `mac`、共享policy/adjustment；P4 | MAC三setup、bitmap未知合法位消耗槽、0x122B/0x122C差异、秒级时间、服务器异常复权回退、本地规则；文件下载、板块/资金流/排名、完整目录组合 |
| [ex/mac_client.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/ex/mac_client.py)、mac_commands/mac_models、_hk_transaction | 无Rust MAC-EX编排 | `mac::extended`，`mac`依赖`ex`；P4 | MAC-EX独立登录；HK股票交易走EX而其他走MAC，衍生品排除；价格/成交单位；goods_list回落EX定位、宏观目录唯一匹配、全量逐笔、排名先全量后截取；缓存及无效排序参数见P0缺口登记 |
| [cli.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/cli.py)、[http_server.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/http_server.py)、[mcp.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/mcp.py)、[__main__.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/__main__.py)、smoke/f10_smoke | 当前Python产品入口组合既有能力 | P2–P5验证其SDK回迁后的行为；Rust原生CLI/服务另P6 | 保留现有输出字段/错误/退出码、MCP资源/工具及HTTP参数到SDK调用的映射。入口能力不重复算wire code；不因公共RustSDK项目顺便重写所有服务 |
| [__init__.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/__init__.py)、helpers/models/protocol/api各`__init__`、[_command.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/_command.py) | Python import/exports及本地请求入口 | Python兼容门面；逐域回迁，P5全量核对 | 旧import/别名/签名可继续存在；body在Rust完成后的Python代码仅转换/适配，不保留第二份分页和业务规则 |

## 10. P0必须登记并裁决的冲突/缺口

以下不是要求立即改变用户行为，而是防止迁移文档把互相矛盾的实现同时称为“规范”。每项要留下旧行为fixture、新SDK约定和Python兼容决定。

| 编号 | 证据 | 裁决/验收要求 |
| --- | --- | --- |
| C01 涨跌停规则三套不同 | `helpers/core.py:1289` 主板ST仍10%、任何30前缀20%、新股统一5日；`helpers/shortline.py:714` ST先判5%、N/C无幅度；`mac/price_rules.py`及`tests/test_price_rules.py` 主板ST5%、创业/科创ST20%、北交上市首日 | 新SDK共用可追溯规则policy，先定证券/日期规则；旧Python各入口若继续不同必须明确compat投影，不让名称掩盖差异 |
| C02 today bulk只有上层入口 | `api/trades.py:158`支持today_batch，`transport/socket.py`、`pool.py`和`_native_models.py`现仅接收historical bulk；`test_trade_batch.py`的真实adapter拒绝today | P2分别验收：新Rust直接API的today bulk成功；旧Python真实Socket/Pool仍抛原ValueError，不因迁移自动修复；自定义transport原有成功路径保持。修复Python旧缺陷须另行批准并记录版本 |
| C03 元数据市场推断不完整/全局污染 | `client.py:362`缺裸5/4及部分沪债；`protocol/unit.py:82`规则更广；Rust `unit.rs:18`全局store | 所有预取调用统一SecurityId推断；新上下文隔离；两个client冲突精度并发fixture |
| C04 聚合raw和排序不一致 | bars/trades合并清raw、翻页保留页内序；delisted排序且保留第一raw | 标准明确“页raw/聚合来源”；Python兼容性单独记录；完整历史的raw不能伪称单帧 |
| C05 失败缓存和clear范围 | helper finance吞错误后缓存可能空；client.clear不clear workday | 部分结果带完整性；新SDK缓存状态区分成功空/失败/过期；clear策略显式 |
| C06 市场统计按位置取源 | `helpers/market.py:68`按quotes[0..2]取不同伪指数 | 缺某个证券时不能错位解读；新SDK按身份关联并定义缺项策略，Python基线另验 |
| C07 离线注释与行为 | financial ZIP“唯一dat”实际first；unknown证券系数fallback；write函数实为append | 严格新接口与兼容接口分清；测试多不同名dat、未知代码、同batch重复/逆序数据 |
| C08 Rust总预算与Python既有分层预算 | Python每次native请求独立计时；startup先于该次deadline，缺项恢复、资金流空切换、分页和runtime重试各有原限制 | P1实现内部双profile，共用算法；Rust公共profile贯彻总deadline/attempt预算，Python兼容profile保留原每请求8秒、startup/预取边界和分层次数上限。以两页累计超过8秒、单页超时、startup和恢复计数脚本差分验收，不能仅靠异常映射宣称兼容 |
| C09 目录/契约与实际测试不完全一致 | 26wire/25registry/124目录；unified_test旧`all_22_commands` nodeid；dataclass清单无消费者 | 本矩阵以源码和行为测试为准；P0更新运行入口及能力清单验证，P5不能凭静态字符串检查声称全部完成 |

## 11. 分域完成记录模板

开发时为上表每一族补充以下记录；不以删除Python代码行数作为完成标准。

| 字段 | 必填内容 |
| --- | --- |
| 能力标识 | 本矩阵对应行、公开Rust路径、Python兼容入口 |
| 基线证据 | 原Python源/现有Rust源/录包fixture/冲突编号 |
| Rust实现 | 所属模块、features、真实依赖阶段 |
| 操作语义 | 所选execution profile、deadline/startup计时边界、cancel传播、各层/共享重试预算、分页停止/顺序、raw来源；分别记录Rust新契约与Python兼容结果 |
| 元数据与状态 | 显式context、cache作用域、刷新/失败状态、专池与pin回收 |
| 验收 | 纯Rust测试、Python兼容测试、消费者示例；必须有失败/取消/边界场景 |
| 发布状态 | 未开始/已实现未验收/已验收未回迁/已回迁/已独立发布验证 |

P5最终范围核对必须覆盖全部 `api`、`helpers`、`models`、传输适配、`equity`、`workday`、`stats`、`offline`、`financial` 以及可选扩展协议；不允许只完成26个wire请求就称“Python全部能力已迁移”。
