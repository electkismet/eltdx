# Rust SDK 架构设计

> 状态：拟议架构，未实现；基线 `af471f1`，2026-11-16。入口见 [开发指南](index.md)。

## 1. 目标、约束与非目标

### 1.1 必须达到的目标

- Rust 应用使用正常 Cargo library dependency 即可完成数据查询；构建、运行和部署不依赖 Python。
- Python 保留现有公开入口、模型、异常映射和默认行为；迁移后调用共享 Rust 业务实现。
- 修复规则、分页、重试等问题时，主体修复只有一份；语言适配只承担输入兼容与结果转换。
- 复用已有 7709 协议和调度器，对其增加必要能力，避免重新实现连接池。
- 每项公共能力有清晰的数据单位、缺失语义、时间语义、资源所有权和验收样本。
- 支持 Windows、Linux、macOS；初始 MSRV 为 Rust 1.89，与现有 workspace 一致。

### 1.2 不作为本次必要交付

P6 的 async、Rust CLI、HTTP/MCP 服务端是独立后续交付。P5 不要求删除所有 Python 文件，也不要求引入插件框架、通用任务 DAG、数据库、Arrow/Polars 或交易下单能力。Python 输出格式和服务框架不进入基础 SDK。

### 1.3 “纯 Rust”的确切含义

SDK 是 Rust 原生库，整个依赖闭包没有 Python 解释器、PyO3、maturin 或运行 Python 的桥接。系统网络、TLS、操作系统 API 等正常平台依赖允许存在；不承诺 `no_std`，也不以是否完全没有系统链接库来定义本项目目标。

交付为两个独立安装产品：Rust 使用者只通过 Cargo 依赖 Rust 库；Python 使用者只安装 Python 包。匹配平台的 Python wheel 内自带编译好的 Rust 扩展，不要求用户再装 Rust 库或编译器；从源码构建 Python 包才需要相应工具链。开发仓库共享源码不意味着用户安装时互相依赖。

Python 用户无需因这次迁移修改安装命令、导入、调用参数或结果处理。现有默认值、返回模型、异常类型与因果链、自定义 transport、缓存、关闭后重连、可选数据源降级以及文件读写语义均属于兼容契约。发现既有缺陷时，另列修复与版本决策；不能把迁移直接当成改变 Python 行为的授权。

## 2. 现状与关键事实

| 现状 | 设计含义 |
| --- | --- |
| `eltdx-protocol` 已实现 26 个 wire command 的编码/解析 | 保留协议资产；25 项 Python registry 与 124 项文档能力不是同一计数 |
| `Engine::execute` 返回 `Result<CommandResponse, RuntimeError>` | 已有 typed Rust 调用，不需要要求用户手动解析二进制 |
| `PendingExecution::wait_timeout` 在 caller 中调用 `CommandResponse::parse` | 可保持解析在 I/O 调度器之外，并为显式解析上下文增加新路径 |
| `unit.rs` 全局精度缓存影响解析，证券列表解析会写缓存 | 新 SDK 必须绕开该隐式状态，而非仅在 Client 外包缓存 |
| 当前 public engine 是同步 API，内部 Tokio | 先同步；async 不得直接包装阻塞调用并声称可取消 |
| Engine/Pin/Pending Drop 不提供完整业务资源清理 | 需要明确 owner、pin 交接与终止确认 |
| F10、EX、MAC 和多数业务组合在 Python | 全功能 SDK 仍需分阶段迁移这些实现 |
| 发布验证围绕 wheel/sdist | 增加独立 Cargo 消费、crate 打包和 feature 矩阵 |

可核对位置：`crates/eltdx-runtime/src/engine.rs` 的 `execute`、`PendingExecution`、`PendingPin`、`EngineInner::drop`；`crates/eltdx-protocol/src/unit.rs` 的 `SECURITY_DECIMALS`；`src/eltdx/api`、`helpers`、`ex`、`mac`、`f10`、`offline`。

## 3. 方案比较与选择

| 方案 | 收益 | 代价与限制 | 决定 |
| --- | --- | --- | --- |
| 仅重导出 protocol/runtime | 改动少，立即暴露底层能力 | Rust 用户自行实现分页、精度、订阅和组合业务，无法满足目标 | 仅作为高级入口的一部分 |
| Rust 与 Python 各保留完整业务实现 | 初期互不影响 | 两套规则、错误修复和兼容测试长期分叉 | 不采用 |
| 共享 Rust 业务层 + 两种语言 API | 单一业务事实来源，Rust 可独立部署 | 初期需解决类型、生命周期及兼容边界 | 采用 |

共享不等于同签名或同一套默认策略。Rust 使用类型化参数、所有权、`Result`；Python 继续接受兼容字符串/关键字参数，输出 dataclass 和原异常。分页、合并和计算共用算法；超时、重试及历史兼容行为通过明确策略传入，并由两种入口分别验证。

## 4. crate 与模块边界

```text
Rust application ───────────────> eltdx
Python application -> eltdx-python ─┘
                                    |
                          shared services / domain
                             |              |
                  eltdx-runtime       optional drivers
                             |        F10 / EX / MAC / files
                    eltdx-protocol
```

### 4.1 第一阶段 crate

| crate | 职责 | 禁止承担的职责 |
| --- | --- | --- |
| `eltdx-protocol` | 字节编码、帧、解析、命令原始模型、解析上下文接口 | 联网、Python 对象、业务缓存调度 |
| `eltdx-runtime` | 7709 会话、队列、socket、deadline、取消、pin、push 原始帧 | 复权、证券筛选、F10 HTTP、用户模型 |
| `eltdx`（新增） | 稳定 API、领域类型、元数据、业务编排、可选扩展 | PyO3、Python 反向调用、FastAPI/MCP |
| `eltdx-python` | Python 参数/异常/DTO 兼容，绑定调用共享服务 | 新增独立分页、计算或容错算法 |

初期不新增 `eltdx-core`/`eltdx-sdk`/每协议一个 crate。可选模块仍可单独测试。只有独立版本周期、明显依赖隔离或外部复用需求经验证后，才提案拆 crate。

### 4.2 `crates/eltdx` 目标目录

```text
src/
  lib.rs                    # 精选稳定导出
  client.rs                 # ClientBuilder、TdxClient
  error.rs                  # Error/ErrorKind 与来源链
  operation.rs              # options/token；私有 OperationContext
  session.rs                # 私有 owner，关闭状态和资源图
  config.rs                 # 已验证的配置、默认值
  types/
    mod.rs security.rs time.rs numeric.rs raw.rs batch.rs
  metadata/
    mod.rs store.rs snapshot.rs precision.rs
  policy/
    mod.rs retries.rs limits.rs compatibility.rs
  hq/
    mod.rs executor.rs hosts.rs decode.rs
  services/
    mod.rs session.rs codes.rs quotes.rs klines.rs trades.rs
    minutes.rs auctions.rs corporate.rs limits.rs resources.rs
    company_info.rs money_flow.rs health.rs
  subscription/
    mod.rs dispatcher.rs cursor.rs lease.rs
  adjustment/
    mod.rs factors.rs apply.rs
  calendar/
    mod.rs service.rs
  helpers/
    mod.rs market.rs boards.rs shortline.rs profile.rs
  stats.rs                  # 默认 HQ 统计资源及有界 ZIP 解码
  f10/                      # feature=f10
    mod.rs client.rs http.rs request.rs response.rs fields.rs news.rs
  ex/                       # feature=ex
    mod.rs session.rs frame.rs commands.rs models.rs hosts.rs
  mac/                      # feature=mac；包含 MAC-EX 路由
    mod.rs session.rs bitmap.rs commands.rs models.rs routing.rs
  offline/                  # feature=offline
    mod.rs paths.rs alignment.rs bars.rs block.rs industry.rs financial.rs gbbq.rs
  financial.rs              # feature=offline，组合HQ资源下载与DAT解析
```

目录表达职责，不要求第一天创建空文件。文件随所属任务落地；增长过大的命令文件按协议命令族拆分，禁止再产生数千行的单个业务 facade。

### 4.3 feature 契约

| feature | 内容 | 依赖/组合 |
| --- | --- | --- |
| 默认 `[]` | HQ 客户端、基础领域类型、HQ 业务及其 helpers | 含统计资源所需的内部有界 ZIP 解码；不含 HTTP、EX/MAC、本地文件公开 API、公共 serde derives |
| `serde` | 稳定领域 DTO 的序列化 | 不序列化 live client、pin、token 或内部状态机 |
| `offline` | 本地文件读写、行业配置、财报 DAT/ZIP、专业财报组合服务 | 复用内部 ZIP 解码，显式限制功能与压缩算法 |
| `f10` | F10 HTTP、动态表、新闻和相关 helpers | 内部需要 `serde_json`，不强制打开公共 `serde` feature |
| `ex` | EX 7727 | 独立会话配置与握手 |
| `mac` | MAC 7709、MAC-EX 与路由 | 显式启用 `ex`，复用香港逐笔等路径 |
| `full` | 上述五项全部启用 | 不自动启用未来 async/CLI/server |

扩展专属 API 用 `cfg(feature)` 显式出现；不能在默认构建中留下必然返回“未编译”的假方法。组合 helper 按最小依赖启用；矩阵给出每项需求。Python bindings 启用其现有产品功能所需的完整 feature 集合。

### 4.4 依赖选型边界

基础层沿用已有 bytes/encoding_rs/flate2/Tokio 等。默认 HQ 的 `read_stats` 和短线 helpers 已需解码 ZIP，因此 P2 实现统计资源前就要选定并锁定内部 ZIP 库；`offline` 开关控制公开本地文件能力，不能解释成关闭一切 ZIP 依赖。ZIP 候选须通过重复条目、加密、大小限额及 zip bomb 语料，限定所需压缩算法，避免自制不完整解码器。

F10 在 P3 先评估原生 blocking HTTP client 的 TLS、代理、CONNECT、SNI、IPv4 优先及 IPv6 回退，满足 [验证规范](verification.md) 后固定依赖；不得为 IPv4 优先修改全局 DNS。优先考察 `reqwest::blocking` 配 rustls，但 1.89/MSRV 与传输行为验证通过前不写死未经验证的版本。

两个阶段的依赖决策分别记录候选比较、锁定版本、feature、许可证与编译矩阵。P3 财报复用 P2 已验证的 ZIP 依赖，并补财报专属语料；不把选库留到消费该依赖的功能完成之后。

## 5. 控制流与执行上下文

### 5.1 一次 HQ 业务操作

本节主流程描述新的 Rust 公共入口；Python 旧入口的计时与重试兼容策略见 5.4。

```text
public typed input
  -> 在方法入口记录 Instant，验证输入并建立 OperationContext（deadline/token/budget）
  -> 获取同一操作所需的 MetadataSnapshot
  -> executor 启动/选择专用会话并提交 wire request
  -> runtime 返回 RawReply（帧 + request/session context）
  -> protocol 使用 ParseOptions 解码
  -> domain 投影、单位转换、分页/批量合并
  -> owned result；调用方持有的结果不占 runtime 队列额度
```

deadline 从入口 Instant 推导，有界输入验证也计入耗时。同一操作中的元数据请求、分页、fallback、拆批共享上下文和一次捕获的元数据快照，不新建完整期限。推送持续订阅是长生命周期资源，每次启动、续订、取消为独立有限操作；队列、续订及 gap 配置由 `SubscriptionOptions` 表达，与启动操作的 `OperationOptions` 分开。

### 5.2 对 protocol/runtime 的必要增量

新增原始执行入口和绝对 deadline/options 入口；保留当前 typed 入口以兼容直接依赖低层 crate 的调用方。`RawReply` 的字段只包含 owned 请求上下文、帧与会话身份，不返回 socket 或调度器锁。

protocol 增加 `ParseOptions`/`PricePrecisionSource` 及 `parse_with_context`。接口由 protocol 定义，`eltdx` 实现，所以没有反向依赖。新路径无全局状态读写；旧 `parse` 可暂留兼容 wrapper。证券列表解码在新路径仅产生记录，由业务层决定何时原子提交元数据。

不要先调用旧 typed parser，再对错误精度的 float 做二次乘除来冒充上下文隔离；那样仍存在竞态、舍入及不可逆信息损失。

`RawReply` 交给业务层前继续受 runtime raw/decoded 额度约束；交付之后数据所有权和内存责任明确转移。公开高级 raw 入口应明确大小限制，不能用于绕过帧/解压上限。

### 5.3 期限、取消与重试

以下统一期限和累计预算是 `RustPublicExecutionProfile` 的约束，不直接改变现有 Python 调用的计时范围。

- 以单调时钟 `Instant` 测量期限；业务日期使用独立时钟抽象。
- deadline 从公共方法开始，包括 lazy startup、admission、metadata 和所有后续请求。
- runtime 必须接受剩余/绝对期限，不能在首次启动后才重新计时。
- 默认重试仍有上限；wire retry 和业务 fallback 共享累计预算，避免乘法重试。
- 只有明确安全的读取请求可自动重试；未来含副作用的命令默认禁止。
- 全空数据不是通用连接错误；仅资金流等有证据的专属策略可做有限空结果切换。
- 普通请求取消走独立控制路径；确认取消失败时关闭受影响会话并给出可判断的错误，不能无声继续占用槽位。
- 显式 close 不受用户已取消的业务 token 阻止，采用独立 shutdown timeout。

标准库 DNS 对自定义 hostname 无可靠强制取消。默认服务器资源使用数值地址；显式自定义 DNS 可预解析/注入 resolver。若使用系统 resolver，必须在配置与诊断中标明它可能超出软期限；不声称所有网络前置动作具备严格硬超时，也不允许为每次超时无限生成解析线程。

### 5.4 Python 超时与重试的兼容执行策略

旧 Python 的 `timeout=8` 是每次底层请求的期限，不是高阶操作的总期限：`client.py:58` 设置默认值；`transport/socket.py:210` 每次发起 native execute；runtime `engine.rs:1137` 在启动后为该请求创建 deadline。`api/bars.py:111` 的每页、`api/base.py:51` 的预取与主请求、`api/_batch.py:18` 的缺项恢复各自调用 execute。直接套入高阶总 8 秒会使原本成功的 Python 查询失败，不能这样迁移。

共享业务算法接受私有执行策略：

| 策略 | 计时与尝试预算 | 入口 |
| --- | --- | --- |
| `RustPublicExecutionProfile` | 入口到结果的单一总期限；预取、分页、fallback 共用累计预算 | 正常 Rust client/service API |
| `PythonLegacyExecutionProfile` | 保留原逐请求期限及原计时起点、启动/预取范围；保持各层原有重试、恢复和页数限制，不隐含新增总 8 秒或共享尝试上限 | 绑定专用兼容调用 |

`max_pages=None` 等原允许选项也要保留，不能通过换算成一个很大的总 timeout 假装等价。两种策略共用循环、解析、合并与资源所有权，policy 负责申请子请求预算和判定何时停止；不要复制两套分页算法。兼容策略不恢复资源泄漏，资源交接修复仍共用。

`eltdx-python` 是独立 crate，不能访问业务 crate 的 `pub(crate)`。因此通过 `#[doc(hidden)] pub mod __python_compat` 提供窄的绑定专用入口，由其内部选择兼容 policy。此模块不依赖 PyO3、不新增 Cargo feature，不进入普通 SDK 稳定 API 承诺；Rust 可见性不是安全边界，不能称它物理上不可被外部调用。常规 `TdxClient` 不提供切到 legacy 的开关。

验收要包括两页各自小于 8 秒、合计超过 8 秒的对照：新 Rust 总期限超时，旧/新 Python 均按原行为成功；另测预取、恢复次数、`max_pages=None` 和补充源失败。具体测试用可控时钟或缩短比例的本地回环完成，不靠公网时延。Python 若要未来引入总期限，应作为独立、显式的新选项与发布变更。

## 6. 会话与资源所有权

`TdxClient`、owned service handle、进行中的同步调用、`PinnedSession`、`Subscription` 持有同一 `Arc<SessionOwner>`。后台 worker 只持有停止信号、completion 和 runtime 数据，不强引用 `SessionOwner`，避免自保活。

```text
Client clones / Service handles / Pin / Subscription / active operation
                            |
                       SessionOwner
                   /        |         \
             HQ engine  auction pool  money-flow pool
                            |
                 无反向 SessionOwner 强引用
```

- `build`：纯配置验证；`connect` 或首次查询才启动。service getter 不联网。
- `close`：关闭同一 owner 下所有专属 HQ 池、订阅与任务；首次 close 建立一个 owner 级绝对 shutdown deadline，各池仅获得剩余时间，并发 closer 不重新计时。成功时确认线程/连接退出；拟议默认总期限为 1 秒，不是每池各 1 秒。
- 关闭后的 Rust 会话拒绝新任务；不自动重新打开。用户可重新 build 一个客户端。
- `Drop`：最后 owner 尽力发停止，不长时间等待，不 panic；确定性回收使用显式 close。
- `PinnedSession` 不 Clone，归属单会话/epoch；Drop 发出释放，显式释放等待确认。
- `Subscription` 不 Clone；Drop 发出停止及 pin 释放，显式 close 等待整个订阅结束。
- 完成与取消同时发生时只能有一个终态。pin 若未交付必须回收，不能因 receiver 丢弃而遗失。
- 关闭失败保留可诊断的 closing/failed 状态及资源身份，允许后续等待清理；不得清空所有引用后报告成功。

Python 旧 `connect -> close -> connect` 契约由适配层创建新 generation 的 SDK owner；旧 pin/订阅仍属于旧 generation，必须失效。不要让 Rust SDK reopen 来迁就 Python facade，也不要破坏已有 Python 重开行为。

上述单一 1 秒 shutdown deadline 是新 Rust 的关闭契约。旧 Python `client.py:285` 依次关闭主 transport、资金流及竞价，各底层 close 原有独立等待范围；兼容桥接须保留原关闭顺序、每阶段预算与异常映射，不能直接转调 Rust 总 1 秒 close 导致更早失败。共用关闭状态机和资源清理代码，由私有 shutdown policy 决定期限分配；在 P0 用繁忙多池场景固定基线。

Python关闭入口没有先封闭全部域，兼容桥采用分域提交gate，不能提前阻止尚未进入close阶段的专池请求。ordinary首错后未执行后续显式close的语义同样保留；安全清理不得暗中改变后续域可调用的既有行为。阶段间并发、同域重开和close/connect竞争必须另用旧版运行基线确认，不能将新Rust整体owner关闭语义套在Python上。

完整状态转换与方法约定见 [公共 API](public-api.md)。

## 7. 元数据、缓存和配置

### 7.1 元数据作用域

默认每个 owner 拥有独立 metadata store。同 owner 的 services/pools 使用同一证券身份和快照；不同 owner 不共享可变精度。EX 市场、HQ 市场、F10 setcode 各自建模，不能拿同一个 `u8` 当成统一市场。

一次完整市场加载先构造候选快照，收到终止空页并通过验证后原子发布，失败不发布“完整”标记。并发相同加载合并为一次；等待者保留各自取消/期限，单个等待者取消不取消其他等待者。operation 捕获不可变快照，刷新不改变已开始请求的解析结果。

快照版本、来源、获取时间可诊断。默认 fresh 数据优先；fallback 精度必须标注来源，不把猜测包装成已验证 metadata。已知可从协议确认的特殊精度由命令规则决定，不能盲目统一所有价格通道。

### 7.2 缓存分类

| 缓存 | key 至少包含 | 生命周期与失效 |
| --- | --- | --- |
| HQ 精度/证券表 | owner 数据源域、市场、版本 | 显式刷新原子换代；失败不覆盖好值 |
| 财务批次结果 | 有序代码序列、选项、数据日期 | 不能排序后丢失调用方顺序 |
| F10 helper | entry、完整 params、scope、日期 | 默认不擅自新增持久 HTTP 缓存 |
| 统计文件 | 路径、服务器交易日、来源 | 日期改变失效；错误不能当有效空表缓存 |
| 板块文件 | 来源、文件元数据、交易日 | 原子落盘；旧Python路径兼容策略另记 |
| 交易日 | 日历来源、标的、覆盖区间、版本 | 支持独立失效；统一 clear_cache 要明确是否含日历 |
| EX/MAC 目录 | 协议族、服务器/市场 | 显式刷新；不可永久隐藏失败结果 |

Rust 默认不隐式写用户磁盘。持久缓存需配置目录和策略，使用临时文件+原子替换；不覆盖未知用户文件。Python adapter 保留旧路径优先级和默认缓存行为。

## 8. 数据模型与协议边界

### 8.1 稳定 DTO 与 raw

SDK 不直接全量重导出现有 wire structs。领域 DTO 对外有稳定语义，内部 wire 模型可保留原字段；`advanced` 仅精选且标明兼容级别。未知字段保存在带来源的 raw 或扩展字段集合，不反向污染已有字段定义。

`RawPolicy::Omit` 表示返回结果不保留 raw/record hex，不代表解码无需接收缓冲。`RetainPayload` 保留共享payload字节，`RetainWire` 另保留原始帧；调试文本按需生成，禁止每条记录无条件提前 hex 编码。多页 raw 返回分页映射而非伪造一段单页 payload；Python adapter 根据旧契约清空或保留。

### 8.2 金额、价格、量与日期

不把所有浮点行情强行变成一个定点类型。原始整数通道保留比例和原值，f32/f64 通道保留其来源类型，业务计算显式选择转换与舍入。SDK 值类型命名表示价格、金额、股数/手数/份数和比率，防止跨单位相加。

交易日与时间点分开。HQ 上海本地时间、MAC 秒级时间、EX 日期编码、F10 日期字符串按原协议解析；缺失时区保留未知，禁止默认为 UTC。分钟结束时间跨日规则按协议区别实现。

财务及文件写入的舍入显式匹配既有 Python ties-to-even 行为；不能直接使用行为不同的 Rust `.round()`。

### 8.3 部分结果

批量查询结果按输入索引关联；重复代码有独立槽位。已核验缺失、元数据不可用和协议/连接失败分开。默认 `MissingPolicy::Error` 对缺失返回结构化错误；显式 `AllowPartial` 才以 `BatchOutcome` 保留 `ConfirmedMissing` 槽位。低层请求及主数据链的协议损坏、连接失败使操作返回错误，不能用空对象填充坏记录，也不能把失败项伪装成已核验缺失。

组合 helper 的可选补充链单独定义：例如快照已成功而五档刷新失败，可返回主数据并附补充来源的状态和诊断；不能给缺失的补充字段伪造零值。`full_quotes`、profile、shortline 等逐项列明哪些来源为必需、哪些允许降级。新 Rust 入口的取消或整个操作 deadline 到期仍使整次操作终止；只允许在操作仍有效时对已定义的补充源失败降级。Python 适配继续保持原返回模型、逐请求计时和原有降级条件，包括可选请求原本允许吞并的请求级超时；新增诊断通过不破坏原模型的诊断通道提供。

分页暴露边界、是否完整和停止原因。触及 max_pages/max_records/byte cap 后不能把截断数据标为 complete；需要继续位置或显式错误。

### 8.4 下载完整性

有界下载返回 `DownloadResult { data, completion, next_offset }`，`completion` 区分已证实 EOF 的 `Complete` 与达到用户上限、尚未证实 EOF 的 `LimitReached`。恰好收到上限字节不是 EOF 证据。严格 `download_complete` 在上限内无法证实完整性时返回 `LimitExceeded`；底层传输/解析失败始终返回错误，不转成普通限额停止。

Python `download_file(max_bytes=...)` 仍按旧规则返回截取后的 `bytes`，不因 Rust 新类型改变签名或改抛限额异常。`read_stats` 使用上限加一的有界探测识别超大档案，并在解压前拒绝；两条路径都要验证单文件 pin、续传 offset 和限额前的分配行为。

## 9. 协议族与业务组合

### 9.1 HQ 池

ordinary、auction、money-flow 的专用连接所有权继续分开，即使主机地址相同。共享 owner 管理总连接限额；单池配置和失效互不误用。不能为了简化把业务空结果策略放进通用 runtime。

### 9.2 EX/MAC

EX 固定 setup、MAC 三步握手、MAC-EX 登录及 head flag 区分为协议 profile。先独立实现串行会话驱动，不把不同握手硬套进 HQ Supervisor。帧读取/解压等公共实现只有在黄金字节与故障测试证明等价后再提取复用。

MAC bitmap 必须按有效位消费字段槽：未知合法位仍消耗四字节，保留位不消耗。保留未知槽原字节。香港逐笔通过 EX 路由的规则留在 MAC-EX service，不放到通用市场枚举或帧 decoder。

### 9.3 F10

独立 HTTP client 拥有连接管理。静态、hot、page2 的入口路由显式记录。响应保存 server status、多个有序表、重复列名与 raw；“HTTP 成功”和“业务成功”分开。

F10 不继承 HQ 自动重试；在没有接口幂等性和服务行为证据前不启用自动重试。端点包装器和中文标签映射可共享静态表，但依赖查询（报告期→报表、主题ID→主题成员）仍是业务操作。

### 9.4 离线文件与财报

离线 parser 与联网分开测试；读操作可按格式允许完整记录加残尾，追加写必须先拒绝不对齐文件，即使本次输入为空也先检查文件。Python `write_*` 的既有含义是追加：只与磁盘原末条比较，不对输入批内排序或去重；daily 的 fsync 与 minute/EX 的现有差异需保留并测试。Rust 的 append/replace/durability 选项显式建模，不隐式修复或截断用户文件。

行业配置保留 GBK 替换解码、少列或代码长度不为6的行跳过、同证券后值覆盖及未知行业值；旧解析器没有数字校验，6字符的 `abcdef` 也保留，不得把它暗改为跳过或整文件报错。ZIP/DAT 在分配/解压前做条目数、压缩/解压大小、重复索引和总解析量限制。

多个不同名 DAT 的旧行为需要基线记录；新严格 API 无显式选择器时返回歧义错误，Python 兼容 wrapper 保持有证据的旧选择行为。

## 10. Python 迁移方法

每个业务域采用同一个闭环：

1. 冻结该域 Python 请求、结果、异常、顺序、raw、空数据与默认值样本。
2. 实现 Rust service，并与固定样本和必要的旧 Python 行为差分。
3. PyO3 增加 typed service 调用，转换到已有 tuple/dataclass。
4. Python facade 经绑定专用桥接转调，选择 `PythonLegacyExecutionProfile`；保留自定义 `Transport` 的已有扩展契约，不能强制所有替身变 native。
5. 删除该域重复算法前，运行新旧两个入口与现有 Python 套件；保留转换和兼容分支。

自定义 Python transport 若仍需参与共享业务编排，用独立 executor 适配接口；不要在 I/O worker 中持 GIL 反调 Python，不允许跨同步等待持 GIL。原生常规路径始终不调用 Python。特殊注入能力若无法等价实现，作为兼容分支有明确覆盖，不悄悄移除。

不保留永久“可随机选新旧业务实现”的全局开关。迁移期按域切换，release 说明记录域覆盖；回退通过版本控制/包版本完成。

## 11. 可观测性与性能

诊断包含 operation id、协议族、command、host、epoch/generation、尝试数、排队/连接/收发/解析耗时、队列/内存占用和 push gap 计数。默认不记录完整财报、F10响应、请求身份信息或无限原始 payload。

性能基线分为协议解析、纯Rust服务、Python适配三层；相同语料、工作量和配置比较。不得用移除验证、少取数据或不同机器的耗时声称提速。目标首先是语义一致、资源有界和独立使用，性能门槛在采样前固定。

结果离开 runtime 后不再受其驻留预算约束，因此“runtime 有界”不等于整个进程内存硬上限。批量/分页提供结果大小限制，应用仍负责释放其保留的数据。

## 12. 发布与设计治理

先完成纯 Rust `cargo package` 和外部 consumer；原有 Python wheel/sdist 继续独立验证。还需在没有 rustc/cargo/maturin 的干净 Python 环境中用 `--only-binary=:all:` 安装已构建 wheel，执行非空 loopback 查询及 close，证明普通 Python 用户不需要构建工具链。内部 path 依赖补版本，发布顺序为 protocol→runtime→eltdx；bindings 不进入 Rust 用户依赖链。

SDK 初始以 `0.x` 发布并注明稳定范围；不能把现有项目 3.3.5 等同于新 API 已成熟。底层 crate 和 Python 版本可保持既有节奏；版本矩阵明确绑定范围，Python PEP440 与 Cargo SemVer 预发布标签分别映射。最终版本号在发布候选任务中实际确认名称可用性后固定，文档不虚构 crates.io 占用状态。

架构变更必须附带：影响范围、兼容判断、迁移方式、验证用例和需要更新的本文档集合。只涉及内部文件拆分且不改变契约的改动无需重做整份架构；公共类型、生命周期、重试、精度或默认行为变更必须重新审查。

P0–P5 的任务依赖及每项验收见 [详细开发计划](development-plan.md)，对所有“已通过”的要求见 [验证与发布规范](verification.md)。
