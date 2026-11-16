# EX 独立 Rust 入口与实现交接

T24 的 EX 客户端、协议、管理、用例和示例源码范围已写完。本页记录实现归属和下一阶段的接入点；编译、测试、依赖解析、独立环境安装与线上验证均未执行，不能据此认定可交付。最终验证继续等待 T01–T30 全部实现源码完成。

## 安装和入口

Rust 应用依赖 `eltdx` 并启用 `ex`，调用 `eltdx::ex::ExClient`。此 feature 激活共享的 Tokio、flate2 和 serde_json，不要求 F10、HTTP、Python 解释器或 PyO3。默认 HQ feature 集合仍为 `[]`。这些依赖图和打包承诺需要在最后阶段实际确认。

`ExClientBuilder::build()` 仅校验内存配置。它不读取环境、用户文件、安装目录或系统代理，不解析主机名，不创建运行时或探测服务器。endpoint 使用数字 socket 地址；有界 DNS 接口没有被隐式添加。需要旧配置优先级时显式调用 `ex::config::load_hosts`；写入只能显式调用 `save_hosts`。

默认总超时 8 秒，raw 为 Omit，wire 尝试最多 2 次，共享额外尝试预算 32，单 operation 的 wire 与解码工作字节合计上限 64 MiB；关闭和清理预算分别为 1 秒。builder 可显式修改。每次公开调用的 `OperationOptions` 在方法入口启动计时，队列、启动、连接、握手、收发、重试和解码均从同一个总期限取剩余时间。显式的每调用 raw=Omit 可以覆盖 builder 的保留配置。

## 源码归属

以下 Rust 路径相对 `crates/eltdx/src/ex/`。

| 功能 | 源码 | 对外结果或边界 |
| --- | --- | --- |
| 配置、惰性 owner、Clone、关闭、公开方法 | `client.rs`、`config.rs` | `ExClientBuilder` / `ExClient` / `ExConfig` |
| 持久连接、完整请求门、socket generation、退役 | `session.rs` | 复用根 `SessionOwner`，无第二份 owner 状态机 |
| 取消、绝对期限、注册 worker、有限清理 | `control.rs`、`worker.rs` | sponsor 不强持有 owner；不能确认退役时保留门票 |
| 完整帧、解压、原始字节、range 序号 | `frame.rs`、`transport.rs` | 16B `<IIIHH>`；单帧编码/解码各不超过 65535 |
| 显式握手 profile | `profile.rs` | 当前 `ExProfile`；MAC/MAC-EX 在 T25 实现独立协商 |
| 探测、排序、候选/健康快照 | `probe.rs`、`hosts.rs` | DataReady 与 TcpOnly 分开，单次探测并发最多 8 |
| 身份、query、本域数值与时间、DTO | `query.rs`、`models.rs`、`time.rs` | 不借用 HQ 市场、价格精度或 A 股手数 |
| 11 个请求和响应 | `commands.rs` | 同一严格 codec，没有第二份 Rust 解析算法 |

## 11 个业务方法

全部网络方法最后一个参数为 `&OperationOptions`。单页返回服务顺序，不隐含拉全、排序或去重。

| 方法 | query | 返回 |
| --- | --- | --- |
| `markets` | 无 | `Vec<MarketInfo>` |
| `instrument_count` | 无 | `u32`，全目录计数 |
| `instruments` | `InstrumentsQuery` | `PageResult<InstrumentInfo>` |
| `quote` | `QuoteQuery` | `Option<Quote>`；目前只有完整报价的 Some 有协议依据 |
| `quote_list` | `QuoteListQuery` | `PageResult<QuoteListRow>` |
| `bars` | `BarsQuery` | `PageResult<Bar>` |
| `bars_range` | `BarsRangeQuery` | `Vec<Bar>` |
| `minutes` | `MinutesQuery` | `Vec<Minute>`，缺源日期时不填系统日期 |
| `history_minutes` | `HistoryMinutesQuery` | `Vec<Minute>` |
| `transactions` | `TransactionsQuery` | `PageResult<Transaction>` |
| `history_transactions` | `HistoryTransactionsQuery` | `PageResult<Transaction>` |

query 构造器验证编码长度、NUL、u16 count、u32 offset、交易 offset 的 i32 上限、Gregorian 日期和区间顺序。range 没有可靠 period 元数据，拒绝 End。普通 bar 的 position-u32 与 amount-f32 为同一槽的不同视图，range 末槽另标结算来源；未知单位、f32 bits、原始 direction 和固定记录内的未知数据保留。

完整短页和合法空页在没有终止标记依据时返回 `PageCompletion::Unknown`。已满足请求条数时返回 RequestedCount，不能据此声称已遍历全量数据。`next_offset` 使用实际 wire 行数，目录优先使用响应 offset；它只给后续查询位置。半条记录、未声明尾部、非法解压不是 Unknown 或 Incomplete，而是 Protocol。

## 生命周期和失败路径

同一个 owner 只有一个完整请求/响应门，所有克隆排队共享。门覆盖握手、完整帧、严格业务解码和 socket 发布；排队者保留各自原始总期限。每次物理尝试只从既有 `AttemptAuthorization` reserve/start 一次，冷连接的 setup 和该请求属于同次尝试。退役旧 socket 后才能授权下一次尝试。只有 Connection 可按配置重试，Protocol、严格解码错误、取消、期限和限额错误终止。

EX setup 为固定 92B，完整验证对应 header、声明 body 和解压结果后才 Ready。接收始终只读取当前帧剩余字节，字节上限在读取前检查，声明解码字节在 body 获取和解压分配前记入同一操作预算。range 的 1..255 序号按 owner 分配，完整回复或确认 socket 退役前不释放租约。

worker 使用当前线程 Tokio runtime 完成有界异步 I/O；成功 socket 转为非阻塞 std socket 后保存在 session，下一次 worker 再登记到其 reactor。worker 不强持有 `SessionOwner`。正常完成确认线程退出并释放登记；取消无法在清理期限内确认时返回 CancelUnconfirmed，原始原因和清理失败分别保留，门票继续由已登记任务持有。最后一个 owner 的 Drop 只通知清理；显式 close 等待确认，关闭共享且不可 reopen。

## 显式服务器管理

`connect/ensure_connected` 在已有本地 Ready 连接时幂等；`disconnect/close` 共享不可逆关闭。`host/hosts/healthy_hosts/unhealthy_hosts/diagnostics` 读取本地快照，读取不会联网。

`probe_endpoints` 对显式 endpoint 执行探测；`discover/ping_all/refresh_hosts` 对当前候选探测，`refresh_endpoints` 在完整探测后原子更新候选。结果保留输入顺序；DataReady 的成功结果用于稳定延迟排序。失败和未探测主机仍保留候选次序。TcpOnly 只表示 TCP 连接成功，不进入数据健康或数据排序。探测默认每主机 5 秒、最多 8 个并发 socket；每个期限取父期限和局部期限较早者。

`from_best_host(builder, discovery, options)` 的探测与随后连接使用同一个 operation。它要求 DataReady；无一完成 setup+markets 时返回 DataNotReady。主操作的取消、期限和限额失败不会降级成某个失败主机条目。每次探测结束或早退，全部子任务和 socket 都在登记 runtime 退出前清理。

`config()` 保留初始不可变配置；刷新后的候选属于 `hosts()/diagnostics()`，不能把历史候选常量或旧探测结果说成实时健康。

## 尚缺的外部证据和后续任务

当前仓库缺少可证明生产响应 magic/method/sequence 接受规则的抓包。实现保留 Association 的可校验能力，普通响应明确记录 association-unverified 诊断；不从请求序号或本地 encoder 常量臆造响应规则。纯 codec 的人工响应测试不能冒充生产抓包。待最终允许网络执行后采集证据，再加入命令/profile 支持的规则。完整报价之外的确认缺失标记也没有依据，当前不得把坏帧或短 payload 返回 None。

- T25 通过 crate 内的 `SetupProfile` 实现 MAC 三阶段 OptionalReply 和 MAC-EX 登录；`Session::with_profile` 为每个独立 owner 固定 profile。复用现有登记 worker、完整请求门、授权、字节预算和帧 I/O。MAC-EX 必须提供自己的协商，不能发 EX92B setup。`Work::Encoded` 仅是受 owner/operation 控制的已校验内部命令入口，不是公开裸 socket。
- T26 继续目录缓存和完整分页/扩展组合，不能把 Unknown 认作 EOF。缓存必须使用真实 profile/endpoint/socket generation，不复用临时探测连接身份。
- T27 为 Python EX/MAC 实现旧策略适配。Python 的独立请求超时、宽松 prefix、原异常和 futures 35 槽/34 名称决策仍按该任务处理；本次没有改动 Python EX API。
- T28–T30 完成公共依赖、示例、独立消费者、可见性/serde、平台和发行证据。统一验证之前不升级 manifest 的验收状态。

## 已编写的验证源码

`tests/ex_wire.rs` 保留独立请求黄金、所有 split 位置、严格长度/解压、模型、时间和配置用例。新增 `tests/ex_session.rs` 的 15 个用例函数通过实际 ExClient 设计了：11 方法/非空响应、完整压缩 setup、克隆/关闭/不可重开、显式 Omit 覆盖、Unknown/RequestedCount、旧 socket 退役后换源、解码错误不连备用、分片握手取消、分片业务关闭、DataReady/TcpOnly、最后 owner Drop、body 前字节 cap、取消后新连接不受旧回复污染、并发不同响应布局、父探测取消、独立 range 序号和共享额外尝试上限。

这些函数和 `examples/ex_query.rs` **均未编译或执行**。例子仅依赖 std 和 eltdx，要求显式数字 endpoint，并展示读取市场、单页目录及确认关闭。最终阶段必须运行真实命令、独立环境和平台检查，届时才可给出通过或失败结论。
