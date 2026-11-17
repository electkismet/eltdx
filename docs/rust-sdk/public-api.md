# Rust SDK 公共 API、类型与会话契约

状态：设计草案，尚未实现。本文约束拟新增的 `crates/eltdx`，代码块用于确定接口方向，不表示仓库已经存在这些类型或方法。落地时应先完成 P0 契约与离线验证，再按域提交实现；不能以本文代码块替代编译结果。

`eltdx` 同时承担公共 facade 和共享业务实现，不另建业务 core crate。它依赖现有 `eltdx-protocol`、`eltdx-runtime`；`eltdx-python` 逐域改为调用 `eltdx`。现有 HQ 调度、连接池、帧边界和退休确认机制继续复用。共享算法通过内部执行策略参数化：新 Rust 公共入口采用整次操作期限与共享尝试预算，Python 兼容入口保留原来的逐请求期限及各层限制；共享实现不等于两种产品必须采用相同的超时语义。

## 1. 现状与必须新增的边界

以下是本次重新核查的源码事实，后文的接口是对这些事实的设计补充。

| 当前事实 | 源码定位 | 对 SDK 的约束 |
| --- | --- | --- |
| `Engine::execute` 已返回 `CommandResponse` | `crates/eltdx-runtime/src/engine.rs:1174` | 不再重写 wire 编解码；但新增上下文解析路径不能直接沿用所有旧解析行为 |
| typed 响应在等待者线程解析 | `engine.rs:735`，尤其 `:758` | 新原始响应入口保留 I/O 与业务解析的分离 |
| `begin_execute` 先启动/取得 runtime，每次再用 `Deadline::after(request_timeout)` 创建请求 deadline | `engine.rs:1137`、`:1694` | 新 Rust 公共入口的总期限必须传入启动、入队和请求流程；Python 兼容入口保留原逐请求计时起点与 startup 范围，不套用整次操作8秒 |
| `Engine`、`PinHandle` 都可 Clone | `engine.rs:392` | SDK 自行定义 owner 与资源句柄，不把底层 Clone 语义直接公开 |
| `EngineInner::Drop` 只尝试发关闭信号，不等待 join | `engine.rs:1848` | Drop 与显式关闭的保证不同 |
| `PinHandle` 没有自动释放；pending pin 的取消可能丢弃已成功的 identity | `engine.rs:844`、`:894`、`:3935` | 正式公开 pin 前必须修复成功交付与取消竞态 |
| 正常底层 close 后可重新启动，失败 lineage 不可 | `engine.rs:1513`、`:1694` | SDK 采用关闭后不可重开的新契约，Python 旧契约通过换代适配 |
| 证券精度为进程级可变缓存，证券表解析会写缓存 | `crates/eltdx-protocol/src/unit.rs:16`、`commands/security.rs:206` | SDK 新路径使用会话 metadata snapshot，不能依赖该全局状态 |
| 多数 parser 在 `include_raw=false` 时仍复制 payload、生成 hex | `commands/klines.rs:235`、`:256`；`commands/trades.rs:339`、`:265` | 新 raw 策略必须下传到 parser；只在 Python 输出时删除字段不算完成 |
| `RuntimeError::InvalidArgument` 含 `python_kind` | `crates/eltdx-runtime/src/error.rs:54` | 公共 SDK 错误必须语言中立，旧字段只留在兼容转换边界 |
| `EngineConfig::new` 接受数字地址；域名需外部解析 | `crates/eltdx-runtime/src/endpoint.rs:17`、`:38` | builder 不偷偷执行 DNS，resolver 要有单独契约 |

## 2. 包、命名空间和阶段

### 2.1 模块可见性

建议目录可使用 `client`、`types`、`services`、`policy`、`cache`、`subscription`，但物理目录不是公共兼容承诺。`policy` 放期限、重试和缺失策略，不放任意业务 helper；`cache` 放有命名空间、版本与失效规则的数据，不持有网络会话并发起隐藏请求。

公共入口固定为少量根级类型和按业务划分的命名空间：

```text
eltdx::{TdxClient, ClientBuilder, ClientConfig, Result, Error, ErrorKind}
eltdx::{OperationOptions, CancellationToken, PageOptions, RawPolicy}
eltdx::{SecurityId, HqMarket, TradingDate, EventTime, Price, Quantity, Amount, Ratio}
eltdx::{BatchOutcome, BatchItem, SnapshotBatch, MissingPolicy}
eltdx::{DownloadResult, DownloadCompletion}
eltdx::{PinnedSession, Subscription, SubscriptionOptions, GapPolicy, PushEvent}
eltdx::types::{...已审核的 DTO、query、SubscriptionOptions、GapPolicy...}
eltdx::services::{...HQ owned service handles、PinnedQuoteService...}
eltdx::advanced::hq::{...精选 HQ 帧/命令/受控会话接口...}
eltdx::f10 / offline / ex / mac     // 对应 feature 开启时存在
```

`SessionOwner`、`OperationContext`、执行策略、底层租约、slot/generation reducer、锁、任务队列与缓存实现为 `pub(crate)` 或更窄。跨 crate 的 `eltdx-python` 不能直接访问它们，须通过第2.4节的绑定桥接面调用。不得使用 `pub use eltdx_runtime::*` 或 `pub use eltdx_protocol::*`。新增根级类型要列入公共 API 审核，不因为内部使用方便而公开。

根级类型是同一 `eltdx::types` 定义的精选重导出，不创建两种同名 DTO。`SubscriptionOptions`、`GapPolicy` 同时能从根和 `types` 使用；活动 `Subscription`/`PinnedSession` 是资源对象，不归入可序列化 DTO。

### 2.2 feature 矩阵

| Feature | 提供的能力 | 依赖规则 |
| --- | --- | --- |
| 默认 `[]` | HQ 7709 业务、其必要的资源/复权/交易日基础 | 不依赖 Python/PyO3，不因构建触发网络 |
| `serde` | 已审核公共 DTO 的序列化/反序列化实现 | 不序列化活动会话、取消 token、锁或 `Instant` |
| `offline` | 通达信本地格式的读取、写入及相关纯数据处理 | 文件写入必须由显式写方法触发 |
| `f10` | 独立 F10 服务及必要 JSON 数据类型 | 内部需要 `serde_json` 不等于用户必须开启 `serde`；`serde` 只控制 SDK DTO derives |
| `ex` | 扩展行情协议、类型和客户端 | 与 HQ 地址/连接状态分开 |
| `mac` | MAC 及 MacEx 业务 | 启用 `ex`，不能遗漏 MacEx 依赖 |
| `full` | `serde + offline + f10 + ex + mac` | 仅为上述五项集合 |

不预设空 `async` feature。真正异步 API 在 P6 完成 completion/cancel 设计后另行确定开关和命名空间。

### 2.3 实施阶段

- P0：冻结默认值、单位、错误、缺失与关闭契约，建立行为基线。
- P1：SDK 骨架、会话所有权、两种执行策略及对应期限/取消上下文、原始响应路径与 metadata 注入。
- P2：HQ 完整业务，包括订阅、资源、复权、calendar；按域把 Python 转到共享实现。
- P3：F10、offline、financial 及依赖 helpers。
- P4：EX、MAC、MacEx 及其余 helpers。
- P5：公共包兼容、打包、依赖与发布。
- P6：可选真正 async，及原生 CLI/服务的独立后续工作。

P1 骨架可构建不等于 P2 业务完成；P2 完成也不能宣称覆盖 F10/EX/MAC/offline 全产品。

### 2.4 Python 绑定桥接面

`eltdx-python` 是独立 crate，不能把 `pub(crate)` 当成可用接口。`eltdx` 提供以下显式、未稳定的绑定专用桥接模块，导出实现跨 crate 调用所需的最小包装类型与方法；其内部创建 `PythonLegacyExecutionProfile` 并调用共享业务算法，不公开内部 owner、context、锁或执行策略枚举：

```rust,ignore
#[doc(hidden)]
pub mod __python_compat;
```

该模块不引入 PyO3，不新增 Cargo feature，不计入面向普通 Rust 用户的稳定 API 承诺；`eltdx-python` 与它按同一发布计划适配。`#[doc(hidden)]` 只隐藏常规文档展示，其他 Rust crate 在技术上仍能调用其公开成员，因此这是 API 兼容边界，不是安全隔离。正常的 `TdxClient`、`ClientBuilder`、`OperationOptions` 没有 legacy 开关，也不能由桥接模块将既有普通 client 静默切换为 legacy 策略。

## 3. 最小公共入口

以下签名是 P0/P1 的拟议 Rust 公共契约，固定采用 `RustPublicExecutionProfile`。query/DTO 的完整字段见 [HQ逐接口规格](hq-api-spec.md) 和 [扩展逐接口规格](extension-api-spec.md)；这些网络入口采用相同的 options、错误与 owner 规则，Python 绑定通过独立桥接入口选择兼容策略。内部跨crate接口与具体转换责任见 [执行与兼容实施规格](execution-and-compatibility.md)。

```rust,ignore
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone)]
pub struct TdxClient { /* Arc<SessionOwner> */ }
pub struct ClientBuilder { /* 配置草案；没有网络资源 */ }
pub struct ClientConfig { /* 校验后的不可变配置；字段私有 */ }

impl TdxClient {
    pub fn builder() -> ClientBuilder;
    pub fn config(&self) -> &ClientConfig;
    pub fn connect(&self, options: &OperationOptions) -> Result<()>;
    pub fn close(&self) -> Result<()>;

    pub fn quotes(&self) -> QuoteService;
    pub fn codes(&self) -> CodeService;
    pub fn klines(&self) -> KlineService;
    pub fn minutes(&self) -> MinuteService;
    pub fn trades(&self) -> TradeService;
    pub fn auctions(&self) -> AuctionService;
    pub fn money_flow(&self) -> MoneyFlowService;
    pub fn corporate(&self) -> CorporateService;
    pub fn limits(&self) -> LimitService;
    pub fn calendar(&self) -> CalendarService;
    pub fn resources(&self) -> ResourceService;
    pub fn company_info(&self) -> CompanyInfoService;
    pub fn session(&self) -> SessionService;
    pub fn health(&self) -> HealthService;
    pub fn helpers(&self) -> HelperService;
    pub fn pin(&self, options: &OperationOptions) -> Result<PinnedSession>;
}

impl ClientBuilder {
    pub fn pool_size(self, count: usize) -> Self;
    pub fn request_timeout(self, timeout: Duration) -> Self;
    pub fn close_timeout(self, timeout: Duration) -> Self;
    pub fn cleanup_timeout(self, timeout: Duration) -> Self;
    pub fn heartbeat_interval(self, interval: Option<Duration>) -> Self;
    pub fn endpoints(self, endpoints: Vec<SocketAddr>) -> Self;
    pub fn auction_endpoints(self, endpoints: Vec<SocketAddr>) -> Self;
    pub fn money_flow_endpoints(self, endpoints: Vec<SocketAddr>) -> Self;
    pub fn max_connections(self, maximum: usize) -> Self;
    pub fn wire_attempt_limit(self, attempts: std::num::NonZeroU32) -> Self;
    pub fn extra_attempt_budget(self, attempts: usize) -> Self;
    pub fn build(self) -> Result<TdxClient>;
}

impl ClientConfig {
    pub fn pool_size(&self) -> usize;
    pub fn request_timeout(&self) -> Duration;
    pub fn close_timeout(&self) -> Duration;
    pub fn cleanup_timeout(&self) -> Duration;
    pub fn auction_endpoints(&self) -> Option<&[SocketAddr]>;
    pub fn money_flow_endpoints(&self) -> Option<&[SocketAddr]>;
    pub fn max_connections(&self) -> usize;
    pub fn wire_attempt_limit(&self) -> std::num::NonZeroU32;
    pub fn extra_attempt_budget(&self) -> usize;
}

impl QuoteService {
    pub fn snapshots(
        &self, ids: &[SecurityId], options: &OperationOptions,
    ) -> Result<SnapshotBatch>;
    pub fn snapshots_with_policy(
        &self, ids: &[SecurityId], missing: MissingPolicy,
        options: &OperationOptions,
    ) -> Result<SnapshotBatch>;
    pub fn subscribe(
        &self, ids: &[SecurityId], subscription: &SubscriptionOptions,
        options: &OperationOptions,
    ) -> Result<Subscription>;
}
```

约定正常 Rust 公共网络方法最后一个参数为 `&OperationOptions`，不据此修改旧 Python 方法签名。纯代码规范化、内存 codec、本地 DTO 转换不需要 options；长文件解析/写入是否提供取消由 offline 域另行明确，不能默认为网络 API。

### 3.1 builder 与默认值

`ClientBuilder::build()` 只验证配置并创建惰性 owner。它不得 DNS、探测、TCP、握手、创建后台线程、扫描通达信安装目录或读取用户文件。service getter、getter 后的 Drop、尚未启动 client 的 `close()` 同样不得有这些副作用。

P0 默认配置数值与现有 Python 入口对齐：pool size 1、server count 1、request timeout 8 秒、heartbeat 30 秒、max pending 256、push count 1024、push bytes 64 MiB、connect concurrency 1。但8秒的作用范围不同：正常 Rust 公共入口将它作为整次业务操作的默认总期限；Python 兼容入口保留每个请求独立8秒，不能给分页、精度预取、批量恢复或其他 helper 追加隐含总8秒。第6节分别定义两套策略。

全局 raw/decoded 容量及 worker 数沿用 runtime 的有界自动计算，不在 facade 复制另一套公式。拟议 SDK 的 `close_timeout` 默认 1 秒；现有单 Engine 使用 `CLOSE_TIMEOUT = Duration::from_secs(1)`（`crates/eltdx-runtime/src/engine.rs:46`），但尚不具备多专属池的 owner 级关闭。实现时在共享 close 线性化处创建一次绝对 shutdown deadline，停止订阅和关闭每个池只能使用剩余时间，不能每池重置为1秒。允许修改 `close_timeout` 也必须把期限传到 runtime，不能只修改外层等待。

所有 count、bytes、duration 在 build 阶段做范围和 checked arithmetic 验证。`pool_size(0).build()` 返回 `ErrorKind::InvalidArgument`，不 panic。配置中的 `Duration` 避免 Python 浮点秒进入新公共 API。`ClientConfig` 可提供其余只读 getter，不能通过公开字段绕过校验。

`wire_attempt_limit` 默认2，包含首次发送；`extra_attempt_budget` 默认32，可显式设0禁用额外重发、缺项诊断与fallback。正常必需页面的首次请求不扣额外预算，子请求的每次重发仍单独扣减；具体计数见[决策表](decision-register.md)。没有剩余恢复预算时返回 `LimitExceeded`，不能将未确认项标成缺失。上述 setter 只配置正常 Rust 策略，不修改绑定内部的 Python legacy 计数。

默认候选服务器是打包资源中的数字地址快照。首次 `connect()` 或网络业务方法才惰性启动。设置显式 `endpoints(Vec<SocketAddr>)` 时顺序稳定，不进行额外探测。全量探测、排序与缓存需显式策略；正常 Rust 公共入口计入总操作预算，Python 兼容入口保留旧发现流程的限制与计时边界。

`auction_endpoints` 和 `money_flow_endpoints` 分别覆盖两个专属池的数字地址目录；普通 `endpoints` 只覆盖普通池。三个池即使地址相同也分别持有连接，getter和clone复用各自惰性池。`max_connections` 默认使用checked `4 × pool_size`，跨普通、竞价、资金流与endpoint续读/探测池共享物理连接permit；必须不小于 `pool_size` 且在底层semaphore支持范围内。permit在TCP连接尝试前取得，随stream及握手失败回收；闲置池仍持有的连接计入上限。无剩余permit时返回 `LimitExceeded` 并标注 `resource=owner_connections`，不等待闲置连接自动释放。Python既有Engine构造不启用该新增上限。

当前资金流按操作串行取得域内admission，每批按最多75项组织且逐代码发请求。等待admission受操作取消/总期限控制；备用endpoint池每次探测后确认关闭，再允许下一次复用。串行方式遵守任意正数连接预算；这不承诺75个并发连接。各代码/整批/操作三层探测扣减在实际尝试授权处原子完成。

### 3.2 DNS 边界

正常 Rust 公共入口第一版有硬期限要求的调用使用数字地址或调用方事先解析的地址。支持 hostname 时，通过可注入 resolver/已有地址缓存获取结果；域名与解析地址分别保留用于诊断。

标准同步系统 DNS 通常不能可靠取消。若选择这种 resolver，必须在该配置/API 文档中声明“DNS 阶段不能保证硬 deadline”，记录其阶段耗时，返回前重新检查期限，并停止后续连接。不能把线程外包或丢弃接收者描述成已经停止 DNS。默认路径不偷偷选择这种 resolver。真正有界 resolver 才能声称解析阶段参与可取消的总期限。

## 4. 所有权与关闭

### 4.1 owner 图

```text
TdxClient clone ───────────┐
owned service handle ─────┤
PinnedSession ────────────┼── strong Arc<SessionOwner> ── runtime/controller
Subscription ─────────────┤
活动 API 的有限操作 guard ┘

后台 worker/周期任务 ── Weak<SessionOwner> 或独立 control state
                       不永久反向持有 TdxClient / SessionOwner
```

`TdxClient` Clone 是同一会话的一个 owner，配置、缓存、关闭状态和连接池全部共享。service handle 是 owned 轻量对象，可以离开 `TdxClient` 变量的作用域使用，也算 owner。`PinnedSession`、`Subscription` 持强 owner，因此保留任意 child 会延长会话生命。

后台任务不得因为强引用环而使“最后 owner”永远不出现。订阅 worker 每次短操作可临时升级 Weak，但不得在整个续订循环持有 owner；其临时操作必须有期限。Drop 订阅先发停止信号再释放 owner。已退休/完成的队列项、无人接收的 completion 也必须归还所持资源。

owned service 可以 Clone；`PinnedSession` 和 `Subscription` 第一版不实现 Clone，减少 pin/停止责任歧义。后续若需共享，新增明确的共享类型，不能悄悄改变当前 guard 的关闭含义。

### 4.2 会话状态与线性化

SDK 状态为 `Unstarted → Starting → Open → Closing → Closed`，以及不可再用的 `FailedClosed`。存在 owner 不代表会话已经启动。关闭 gate 与提交 gate 必须是同一个线性化边界。

| 动作 | 必须保证 |
| --- | --- |
| 任一 client clone 调用 `close()` | 关闭同一个共享会话及其所有 child 使用的连接；不是仅 Drop 该 clone |
| 两个线程同时 `close()` | 共享同一个 close attempt 和结果；不重复 join、不延长首个关闭期限 |
| close gate 先于新提交 | 新请求返回 `Closed`；不得再启动 runtime |
| 正常完成先于 close gate | 可以交付已提交完成的结果；未完成操作终结为 `Closed` 或其更早确定的错误 |
| `close()` 返回 `Ok(())` | SDK 自有网络任务、renewal worker、pin、socket 与 runtime thread 已停止/释放，join 已确认 |
| 重复 close | 正常已关闭时幂等 `Ok(())`；关闭失败保存失败结果，不能下一次伪装成正常成功 |
| 关闭等待超时/无法确认 | 返回 `CloseTimeout`，状态不可重用；继续 best effort 清理，不声称资源已经全部回收 |
| close 后调用 connect/网络业务 | 返回 `Closed`，不 reopen |

service getter 和配置/诊断 getter 在已关闭会话上仍可调用；取得的服务只能在网络方法上返回 `Closed`。这样 getter 不需要因状态检查而产生竞态或变成 `Result`。

SDK 不提供 `reopen()`。需要重新连接生命周期时创建新的 `TdxClient`，它有新的 session id、generation 和缓存空间。某次普通网络错误可以按既定重试重连 socket，这与已关闭 client 的重新开启是不同操作。

### 4.3 最后 owner Drop

最后 owner Drop 必须同步封闭提交 gate，并向控制路径发停止信号；Drop 不 panic、不阻塞等待 I/O、线程 join 或用户回调。资源由独立清理控制状态收尾，不能要求已销毁 owner 再次升级。句柄丢弃时最好能关闭，但**只有显式 `close()` 的成功结果证明 join 和清理完成**。

不以 `Arc::strong_count()` 的一次观察作为所有权线性化；让 `SessionOwner` 本身的 Drop 负责最后 owner 路径。内部工作持有的有限引用也要纳入泄漏验证，不能靠应用再 Drop 一个 client“唤醒”清理。

### 4.4 Python 旧 close/connect 的换代兼容

现有 Python 正常 close 后 connect 可重开。该行为在迁移期间保留：Python 兼容 adapter 保存配置与当前 SDK 会话，旧 API 再次 connect 时原子创建一个新的 `TdxClient` generation，不能调用已 closed SDK 对象重启。

普通 Python service proxy 可每次从 adapter 取得当前 generation；不得把旧 Rust service handle 偷换其 owner。旧 pin、旧 subscription、旧 pending 仍绑定原 generation，不能在重开后复活或占用新 session。并发 connect/close 的先后通过 adapter 的短状态锁决定，I/O 在锁外完成；成功发布新 generation 前再次检查关闭标记，避免新 client 变成孤儿。

Python 的关闭预算也需兼容：现有 `client.py:285` 依次关闭主 transport、资金流和竞价，各阶段具有原独立等待范围，不能改为新 Rust 公共 close 的总 1 秒。绑定专用入口通过私有 shutdown policy 复用停止/退休/join 状态机，保留旧阶段顺序、预算及异常映射；正常 Rust 的 owner 级单期限规则不变。P0 冻结多池忙碌和中途失败时的可观察行为，P1/P2 用双路径对照验收。

Python原close也没有在入口设置覆盖所有域的总关闭标记。兼容桥不能在ordinary仍关闭时提前拒绝本来可进入money-flow/auction专池的请求，或在ordinary首错后将尚未执行close的域直接报告Closed。采用分域gate和按阶段触发的关闭，资源仍有明确owner；阶段间并发请求、同域重入和close/connect竞态须按COMP-013附加分支先录制旧基线。普通Rust的owner整体关闭规则不受这一兼容例外影响。

## 5. Pin 与 Subscription 契约

### 5.1 PinnedSession

`PinnedSession` 是一个会话内的串行、固定连接 guard，持有不可伪造的 session/generation/租约身份。`close(&mut self) -> Result<()>` 仅释放该 pin，终止其未完成操作并等待对应退休确认，不关闭其他 pin 或 client。重复关闭正常幂等；Drop 发 best effort release。

第一版不把它宣称为交易一致性、服务端快照或断线透明恢复保证。连接断开时返回 `PinLost`/底层连接错误，调用方必须重新 pin。不得把旧 identity 重新绑定到新连接。发送到同一 pin 的业务顺序由其串行化器保证；多步骤复权/资源操作只有明确使用 pin 时才获得该顺序。

pin 专属 service handle 必须借用 guard，不能延长它：例如 `pin.quotes() -> PinnedQuoteService<'_>`。它不返回普通 owned `QuoteService`。执行函数内部可使用有界操作 guard，但不能制造一个逃离 pin 生命周期的别名。

### 5.2 pending pin 成功与取消竞态

当前 `PendingPin::cancel_and_confirm()` 用 `try_recv()` 读到 `Ok(identity)` 后直接返回成功；runtime 交付 pin 时也忽略 send 失败。这两条路径会丢失“谁负责 release”的信息，不能作为新 SDK 的正确基础。

P1 必须采用同一 terminal arbiter 和拥有释放责任的 `PinGrant`。该 grant 从分配开始到交给用户期间，始终由 reservation、completion slot 或 `PinnedSession` 三者之一拥有。它的内部 Drop 负责幂等 release；send 失败必须取回 grant 并释放，不能忽略错误。

| 竞态位置 | 处理规则 |
| --- | --- |
| 取消先于 pin 分配 | 移除等待项，确认无 grant/active lease 后结束 `Cancelled` |
| 分配已发生，用户成功尚未发布，取消或 close 先赢 | 取回 grant，release 并退休；不能只丢 identity |
| 用户成功已经在 terminal arbiter 提交 | 方法返回有效 `PinnedSession`；稍后的 token cancel 不把已交付成功改成错误 |
| receiver 被丢弃或结果从未被取出 | completion 的 owner 释放 grant，不能依赖用户再次 poll |
| 取消确认超时 | 状态标为未确认，并进入受影响会话的失败清理；返回 `CancelUnconfirmed`，不得返回已确认取消 |

内部可以使用 `CancelResolution<T> = Cancelled | Completed(T) | Failed(Error)`，但第一版不必公开 pending API。普通操作的 token cancel 只是请求；调用方法最终返回的 `Result` 表示哪一个终态获胜。成功与取消不能同时交付；失败路径不能留下无人管理的 pin。

### 5.3 Subscription

订阅返回 owned、单消费者的 `Subscription`，持强 owner，并拥有其分组 pin、cursor、renewal 状态。第一版优先提供拉取接口：

```rust,ignore
#[derive(Clone, Debug)]
pub struct SubscriptionOptions { /* 字段私有；独立于 OperationOptions */ }

impl Default for SubscriptionOptions { /* 见下表 */ }
impl SubscriptionOptions {
    pub fn with_queue_limits(self, max_events: usize, max_bytes: usize) -> Self;
    pub fn with_renew_interval(self, interval: Option<Duration>) -> Self;
    pub fn with_gap_policy(self, policy: GapPolicy) -> Self;
}

#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub enum GapPolicy {
    #[default]
    ReportOnly,
    Resynchronize,
    Close,
}

impl Subscription {
    pub fn next_event(&mut self, options: &OperationOptions)
        -> Result<Option<PushEvent>>;
    pub fn close(&mut self) -> Result<()>;
}

#[non_exhaustive]
pub enum PushEvent {
    Initial(SnapshotBatch),
    Update(QuoteUpdate),
    Gap(PushGap),
    Resynchronized(SnapshotBatch),
}
```

| SubscriptionOptions | 拟议默认值/规则 |
| --- | --- |
| per-subscription max events | 1024；必须大于0，包含 Initial/Update 等待队列项 |
| per-subscription max bytes | 64 MiB；必须能放一个受支持最大事件，有独立内存计费；不等于已有 runtime wire 队列预算 |
| renew interval | `Some(Duration::from_secs(7))`，对齐现有 `src/eltdx/api/quotes.py:86`；None 显式禁用，非None必须大于0 |
| gap policy | ReportOnly；保留 gap 事实及失同步标记。Resynchronize 显式开启有限重取快照；Close 报告 gap 后停止订阅 |

构造 options 不做 I/O；`subscribe` 在占用 pin 前校验其值和当前 session 的资源上限。预先比较配置只能拒绝静态不可能满足的组数；动态并发占用要靠有界分组获取及失败回滚，不能声称普通 precheck 是原子资源预留。

gap 控制状态应有独立保留槽/合并计数，不与已经满的 Update 队列争抢最后位置，保证慢消费者仍能获知曾丢数据。单个事件超过 byte cap 要明确终止/报告配置限额，不能无限丢弃再假装正常。ReportOnly 下 gap 后的 Update 是尽力提供、带失同步状态的数据；只有显式重订阅或成功 Resynchronized 后才恢复连续状态。

`next_event` 超时或该次读取取消只结束这次等待，不默认退订；显式 subscription close 或 Drop 才停止整个订阅。`Ok(None)` 表示订阅已正常结束，不同时表示“目前没消息”和“超时”。不在 P1/P2 引入用户回调 worker，避免同步 close 在回调中 join 自己；未来回调适配器须另行定义重入与执行线程。

自身 `Subscription::close()` 后读取返回 `Ok(None)`；父会话显式关闭或失败关闭后，读取返回 `Closed` 或保留的根因错误。父会话 close 不等待某个用户线程再次读取事件才释放 pin。建立订阅使用的 options/token 只控制建立操作，建立成功后整段订阅的寿命由 guard、独立停止状态和会话决定。

正常 Rust 公共入口初次 subscribe 的元数据读取、分组 pin 和初始 refresh 共用传入 OperationOptions 的一次操作预算。成功后只保存校验后的 SubscriptionOptions，不保存用于建立的取消 token/绝对 deadline。订阅建立后的每次 renewal 使用有限的独立 operation context（默认时限来自 ClientConfig），继承 session close 状态和订阅停止信号，不能继承一个早已耗尽的建立 deadline。renewal 超时按订阅错误/重同步策略处理，不无限续期。Python 订阅桥接保留原建立、续订和等待的各自期限语义，不因共用算法而新增整次建立8秒上限。

队列越界必须发出带丢失数量/范围的 `Gap`，不能悄悄继续声称连续。自动恢复策略要先报告 gap，再按预算重新获取快照、更新 cursor，并报告 `Resynchronized`；策略未开启或失败时保持明确的失同步/错误状态。原始 push 的 slot/generation 必须与订阅上下文核对，过期数据不更新新 cursor。

同一 runtime 当前只有一个消费式 push 队列。SDK 必须有单一 demultiplexer，再分发到订阅的有界队列；多个 Subscription 不能直接竞争调用底层 `poll_push()`。`advanced` 原始订阅也要经过受控分发或独占模式，不能偷走普通订阅帧。

## 6. 操作期限、取消与重试预算

本节的 `RustPublicExecutionProfile` 与 `PythonLegacyExecutionProfile` 是内部执行策略名称，不是公共配置类型。共享分页、请求拆分、元数据准备与恢复算法由策略决定如何创建请求期限、扣减尝试数及执行 fallback；不能为复用新 Rust facade 而改变旧 Python 语义，也不能复制两套业务算法后分别修补。

| 维度 | RustPublicExecutionProfile：正常 Rust 公共入口 | PythonLegacyExecutionProfile：绑定桥接入口 |
| --- | --- | --- |
| 默认8秒 | 一次公共业务操作的总期限 | 每个旧请求独立使用 request timeout，无隐含操作总8秒 |
| 计时起点与 startup | 方法入口记录起点，包含校验、metadata、runtime startup、admission和后续业务 | 保留旧 `begin_execute` 在 `reserve_submission` 后创建 `Deadline::after` 的边界；不将原计时前的 runtime startup 突然计入请求8秒。connect/pin 等另按各自旧入口冻结 |
| 精度预取与分页 | 全部子请求共用总 deadline | `ApiBase._execute_priced` 的精度预取及后续请求、bars 各页分别取得原请求期限 |
| 额外尝试 | 每请求上限加操作级共享 retry/fallback 预算 | 保留 runtime 每请求策略、批量恢复等各层原计数及作用域，不新增共享 attempt cap |
| 页数与终止 | 按正常 Rust query/PageOptions 的期限、页数/记录数上限 | 保留旧 page 限制及 `max_pages=None`；None 不被换成新的有限页数或隐含总期限 |
| fallback | 遵守新 Rust 总期限与共享预算，同时满足业务条件 | 保留旧触发条件、次数与错误捕获范围；不能因为新的总期限或共享 cap 而提前跳过/终止 |

例如两页各耗时5秒、合计10秒且每次请求都正常成功时，默认 Rust 公共操作应在总8秒处超时，Python legacy 分页仍可成功。选择 legacy 策略不关闭每请求超时、旧恢复上限、会话关闭、协议校验或已定义的取消机制；它只保留已存在的作用范围。

### 6.1 公共 options 与内部 context

```rust,ignore
#[derive(Clone, Default)]
pub struct OperationOptions { /* 字段私有 */ }

impl OperationOptions {
    pub fn with_timeout(self, timeout: Duration) -> Self;
    pub fn with_deadline(self, at: Instant) -> Self;
    pub fn with_cancellation(self, token: CancellationToken) -> Self;
    pub fn with_raw(self, raw: RawPolicy) -> Self;
}

#[derive(Clone, Default)]
pub struct CancellationToken { /* 仅取消状态，不持 SessionOwner */ }
impl CancellationToken {
    pub fn cancel(&self);
    pub fn is_cancelled(&self) -> bool;
}
```

以下 `OperationOptions` 规则专用于正常 Rust 公共入口：timeout、absolute deadline 同时提供时取更早值；都未提供时，以 ClientConfig request timeout 为整个公开操作期限。`with_timeout` 保存 Duration，在方法入口计算 deadline，不在 builder 时启动计时。token 可被多个独立操作共享；其取消不等同于 client close，且不能复位。Python legacy 桥接不把旧 timeout 参数填入这一整次操作选项。

正常 Rust 公共调用遇到已取消 token、已过期 deadline 或该次操作零 timeout，在任何 lazy startup/DNS/提交前返回对应 `Cancelled`/`Timeout`。这与 ClientBuilder 的默认 request timeout 必须大于零不同。deadline 加法溢出返回 InvalidArgument，不饱和成“永不超时”。

内部 `OperationContext` 至少包含 operation id、session id、执行策略、取消状态、root cause、metadata snapshot、raw policy 和累计工作量，并按策略持有期限/尝试计数：Rust public 有总绝对 deadline 和共享 retry/fallback 预算，子请求持受限视图，不各自重置；Python legacy 没有凭空生成的总 deadline，逐请求在原边界创建 deadline，保留各层计数。调用者传入 options 后不可再修改正在运行的那次操作。

正常 Rust 公共操作期限从方法入口开始，包含参数相关的有界校验、metadata 获取、可取消解析、启动、admission、排队、connect/handshake、send/response、协议解析、页合并、复权与最终交付检查。无网络的 `build()` 不是一次业务操作。每次可能阻塞前和每个有界 CPU 批次之间检查适用于该策略的 deadline/cancel；Python legacy 检查已存在的请求期限与取消条件，不检查一个新造的整次业务总8秒。

### 6.2 必需的 runtime 扩展

新增原始响应执行路径（内部名称可为 `begin_execute_raw_with_options`）支持由执行策略提供期限与尝试规则。Rust public 路径传入入口已确定的绝对期限和每请求尝试上限，贯穿 `reserve_submission → ensure_runtime → admission → send/response`；pin/connect 同样接收剩余期限。Python legacy 路径在原 `reserve_submission` 之后为本次请求创建 deadline，并沿用旧请求尝试规则。两者共用后续调度、I/O、解析与退休逻辑，不允许在适配层把旧请求期限提前启动，或仅在 Rust facade 套 `wait_timeout` 后离开。

原始 reply 至少包含原请求、`ResponseFrame`、endpoint 和 session/epoch/generation 诊断信息。业务 parser 不在该路径调用，除连接建立必需的握手/heartbeat 控制解析。SDK 再用 context-aware parser 解码。这不是删除现有 typed 入口；旧 `Engine::execute` 继续作为兼容 wrapper，保持其旧行为到迁移结束。

同时操作等待同一 runtime startup 时，某个调用者超时只移除自己的 waiter，不能因为它退出就关闭仍被其他操作需要的共享启动。只有 session close、启动本身 fatal 或无人需要时的明确清理策略能终结共享 startup。

### 6.3 正常 Rust 公共入口的三种预算

| 预算 | 计数与边界 |
| --- | --- |
| 总时间 | 每页、每个批次、metadata/fallback 都用同一个绝对 deadline；内层可取更短 slice，不能延长 |
| 正常工作量 | query/page 的 max pages、max records、拆分批次数，以及有界 metadata 工作；每个正常请求各计一次 |
| 额外尝试 | endpoint failover、重发、单代码诊断和 fallback 都要消耗操作级共享额外预算，同时服从每请求最大尝试数 |

正常 Rust 公共入口P0建议每请求最大尝试数为2；另设有限操作级额外尝试预算，避免“每页重试2次再每代码fallback2次”形成乘法放大。正常的下一页不等同于 retry，但受页/记录预算限制。高阶服务在发起拆分前就计算上限，诊断缺失的请求也计预算。Python legacy 则保留原每请求及各层限制，例如 `src/eltdx/api/_batch.py` 的 `_RECOVERY_REQUESTS = 32` 在原作用域内计数，不能被新增共享 cap 提前截断，也不能把旧 `max_pages=None` 映射为 Rust 默认上限。

现有 `CommandRequest::retry_safe()` 无条件 true，Rust public 新路径必须按命令与阶段分类。读请求也不能让两个并发层同时重发；由所选策略授权、runtime 执行一次尝试推进。Python legacy 保持已有重试及批量格式恢复条件，其修订需独立兼容决策。业务缺失、格式损坏与传输失败区分处理，不能把任意 parse error 都改成另一来源“兜底成功”。第9.4节明确允许降级的可选补充源也要保留失败诊断，不能改写为该来源成功。

### 6.4 超时、取消和清理

正常 Rust 公共操作的业务期限控制结果产生，不等于可以不退休已发送请求。操作级超时/取消获胜后停止新子请求，提交 cancel，等待有界 cleanup confirmation。cleanup 使用单独短上限，不重新获得业务预算，也不允许返回业务成功。返回耗时可能达到业务 timeout 加清理上限；必须在 API 文档中明确，不能承诺严格在 timeout 毫秒内返回且已完成 I/O 清理。Python legacy 的单次请求超时仍先退休该请求，再由原 fallback/异常传播规则决定后续业务，不能把它自动提升为一个原本不存在的操作级总超时；已有显式取消和会话 close 仍终止其相应作用域。

拟议 `cleanup_timeout` 默认1秒，配置必须大于0，计整个operation清理而非每个请求单独1秒；已在执行owner close时再取shutdown剩余量的较小值，不能绕过close总期限。无已提交资源可立即返回，不强制等待满1秒。确认失败返回 `CancelUnconfirmed` 并保留原Timeout/Cancelled根因及受控清理责任；Python桥接继续原有取消确认范围，不强套此新增默认值。

若退休确认失败，错误保留最初 timeout/cancel 和 cleanup 失败信息，并封闭受影响的会话路径，不能将连接立即复用。control channel 不受普通 ingress 满额阻塞。取消不能回滚已经发送的 wire 请求，确认的含义是本地已终结/退休且不会再把迟到响应交给后续请求。

CPU parser 的单条解码以及操作系统调用不是可抢占的。用最大帧大小、单次处理块大小和循环检查限制延迟，分页总上限服从各自执行策略，不能借此为 Python `max_pages=None` 添加新上限；不能宣传所有计算或同步 DNS 可被精确硬中断。

## 7. Metadata 与 context-aware parser

### 7.1 protocol 接口归属

`PricePrecisionSource` 必须定义在 `eltdx-protocol`，protocol 不能依赖 `eltdx`。拟议最小接口：

```rust,ignore
pub trait PricePrecisionSource: Send + Sync {
    fn security_decimal(&self, code: &NormalizedCode) -> Option<u8>;
}

pub struct ParseOptions<'a> {
    pub price_precision: &'a dyn PricePrecisionSource,
    pub raw: ParseRawPolicy,
}

impl CommandResponse {
    pub fn parse_with_context(
        request: CommandRequest,
        payload: bytes::Bytes,
        options: &ParseOptions<'_>,
    ) -> Result<Self, ProtocolError>;
}
```

这里的 `NormalizedCode`、`ParseRawPolicy` 是 protocol 自有类型，不能引用 SDK 的 SecurityId/RawPolicy。SDK adapter 负责转换。使用 `Bytes` 所有权使 parser 在需要保留 raw 时可共享/切片；旧 `parse(request, &[u8])` 保留兼容签名。

新 `parse_with_context` 及其所有递归命令 parser 必须只从注入的 precision source 读取精度。证券列表 parser 返回证券元数据，不写全局 cache。旧 `parse` 可以暂时使用 legacy global-cache wrapper，包括旧表解析注册行为，但新 SDK 不经过该 wrapper。不能只修改 aggregate dispatcher，却漏掉 quotes/minutes 中直接调用旧 `price_divisor()` 的分支。

### 7.2 SDK metadata 生命周期

`SessionOwner` 保存版本化 metadata cache，key 至少包括协议 family、来源/目录命名空间和完整 instrument identity。多个 TdxClient 会话默认隔离；同一会话的 clones 与 services 共享。主动共享 metadata 只能通过显式只读快照/来源注入，不重新引入进程全局可变表。

正常 Rust 公共业务先在同一 OperationContext 内准备必需 metadata，再冻结一次只读 snapshot；该操作所有 chunk/page/fallback 用相同版本。并发刷新生成新版本并原子替换，不能使一个 batch 前后半段按不同小数位解析。snapshot 的查询不做 I/O，也不持锁调用 resolver/transport。Python legacy 同样使用注入的只读精度数据，但保留 `ApiBase` 原精度预取的触发条件、请求顺序和独立请求期限，不能为了共用 snapshot 准备流程而占用后续主请求的8秒或新增共享尝试限制。

结果元数据记录 precision 来源：证券表、经过核实的 wire 规则、明确的代码前缀 fallback、用户 override。前缀匹配不等于上市状态/证券类型的事实。元数据缺失时按已审核的 fallback 规则或明确 `MetadataUnavailable` 结束，不能随意除以100。缓存失败可有限合并请求，但取消一个 waiter 不取消其他会话/操作的工作。

逐笔可转债与报价的精度合同不同，不能把 quote decimal=4 套用到逐笔 milli-yuan；每种命令保留自己的转换规则。需要“修正证券表精度”的高阶逻辑也属于共享业务层，不能继续只放 Python。

## 8. 类型、单位、日期和 raw 策略

### 8.1 identity 与日期

`HqMarket::{Shenzhen, Shanghai, Beijing}` 只表示 HQ 市场；`SecurityId` 的字段私有，支持 `FromStr`、`Display`、`market()`、`number()` 与显式 market 构造。六位代码规范化沿用已验证的推断规则，显式前缀优先；规范化不执行证券存在性检查。EX/MAC 保留各自的市场/商品 identity，不能强制塞进六位 `SecurityId`。

新 Rust 的构造只接收字符串代码或显式 `HqMarket + 六位 ASCII 数字`；不做整数、空值、未知市场或任意对象到证券代码的隐式转换。裸代码只在已有无歧义规则覆盖时推断市场；显式 `sh`/`sz`/`bj` 前缀优先。扩展协议的 market id、香港股票和期货代码使用各自类型。Python facade 继续执行当前 `normalize_code`/`normalize_market` 与 host 输入投影，包括 `normalize_code` 的字符串化行为、大小写/外围空白、`0`/`1`/`2`、`sza`/`sha`/`bse`、中文市场别名，以及 `normalize_host` 对裸域名和 IPv6 补默认 7709 端口；这些宽松行为不泄漏到 Rust 类型构造。Rust 显式 endpoint 使用带端口的 `SocketAddr`。

精度预取的旧实现不是 `normalize_code` 的等价映射：例如裸 `510300` 和 `430047` 可分别规范化为沪市、北交所身份，但 `_ensure_decimals` 不识别 5/4 开头，回退为同时请求沪深北三份目录。Python 兼容路径保留旧请求范围；Rust 先得到严格 `SecurityId`，再只按该身份与 owner namespace 查找元数据。新路径的解析不得读写 `eltdx-protocol` 的 legacy 全局精度表。

`TradingDate` 使用校验后的年/月/日，公开构造不可产生无效日期。`EventTime` 明确区分：仅交易日期、交易所本地日期时间（已知 Shanghai +08:00）、来源未明确时区的 local 日期时间、仅日内时间。来源未明确不得默认成 UTC；仅有 time+用户 date 的值也不能伪装成服务端完整时间。

`TradingDate` 只验证 Gregorian 日期，不验证交易所是否开市；开市状态来自显式 `TradingCalendar` 数据。Python 未绑定 client 的 workday helper 保留 weekday fallback。Python `yyyymmdd(None)` 继续使用 `date.today()`；Rust 的默认“当前日”必须来自注入时钟或 server session，不能暗中读取主机本地日期。HQ 时间保留已知 +08:00；MAC naive datetime 保留为 source-local/时区未知。Rust 派生 bar end 使用真实日期进位并同时保留源时刻，不能把次日 00:03 回绕成同日。

高阶默认“今天/最近交易日”通过会话的时钟与 calendar 策略决定，测试可以注入，不能在每个 parser 自行调用系统当前日期。时间排序必须保留秒；MAC 秒级时间是当天秒数，不能压到分钟。同一日期重复 bar 的合并规则由业务契约定义，不能因为时间精度损失自动去重。

### 8.2 数值

`Price` 不是把所有 f32/f64 强制转定点整数的别名。其私有表示可保留 scaled integer、wire f32 或 f64 的值与精度来源，公开提供 `as_f64()`、精度/来源查询以及显式带 rounding 的转换。没有可靠尺度时，返回“不能无损转为指定单位”，不能自行四舍五入后声称精确。

若某条现有 typed parser 只输出经过取整/缩放后的 f64，新的上下文解析路径必须补齐建立 Price 所需的原始数值/尺度信息，或明确该字段只能提供派生精度；不能先丢精度，再把 f64 包成 Price 就宣称无损。

`Quantity`、`Amount`、`Ratio` 显式携带单位/尺度：股、手、张、元、百分数与比例等不得仅靠字段名约定。未知单位用明确 Unknown/raw typed 值，不填猜测单位。F10/financial 原生 f32、扩展行情价格可保留其浮点性质。wire bits 在 raw 模式可用，不应由于每个值都附一份大 raw buffer 而显著放大默认 DTO。

单位在字段与协议维度冻结，不存在“行情数值统一乘/除一个常数”的转换：

| 来源字段 | 规范化单位 | 规则与兼容边界 |
| --- | --- | --- |
| HQ 逐笔证券价格 | CNY；原值为 milli-yuan 的字段保留整数及 scale | 125528 → 125.528；逐笔成交量原始手数按字段语义转换为股时乘100。Python `TradeTick.volume` 及 `trade_amount_yuan = price * volume * 100` 保留。 |
| HQ A股K线成交量 | wire value 为股，`volume_lots = shares / 100` | 不能套用逐笔的手数输入，也不能把原始压缩字段误标为股数。 |
| HQ 已验证可转债 K线 | wire value 为手，保持手数 | 已覆盖沪市110/113/118、深市123/127/128；不得再除以100。其他债券不能仅凭相似前缀推断。 |
| EX 港股逐笔 | wire price 为 milli-HKD，公开价格为 HKD | 除数1000仅用于该来源字段；不套用 HQ tick 或普通 EX 原生整数价格。 |
| 金额、比例、市场统计 | 逐字段保留已证实的 CNY、native、fraction 或 percentage-points | 例如市场统计指数880005计数缩放、880001市值×1e10只作用于对应来源字段；没有字段证据时单位为 Unknown。 |

每个转换结果保留 wire 原值及 scale provenance。协议字段单位没有证据时，Rust 返回 Unknown 单位而不是用名称猜测；Python 的既有数值和属性投影继续按旧路径提供。独立示例与预期在仓库 `tests/fixtures/sdk/policy/units.json`，政策说明见[决策清单](decision-register.md)。

复权价是计算结果，须带调整模式、anchor、因子来源/版本和 rounding 信息；不能改写原始价并丢失来源。对 Python 的兼容转换保持历史 float/整数表现，在新 Rust 类型之外完成。

业务数值的 NaN/无穷与 wire 保留位分开：是否允许某个字段非有限值由该字段契约决定；不能统一 `as i64` 或替成零。`serde` 输出必须对非有限值、整数尺度和未知值有明确 schema，不能默认交给 JSON 把它们变成 null。`Error`、原始位数据与带 provenance 的价格表示不要求用同一个 JSON 形状。

### 8.3 RawPolicy

```rust,ignore
#[derive(Clone, Copy, Default)]
pub enum RawPolicy {
    #[default]
    Omit,
    RetainPayload,
    RetainWire,
}
```

| 策略 | 响应交付后的持有 | 解析中的允许成本 |
| --- | --- | --- |
| Omit | DTO 不持整份 payload/wire，也不生成逐行 hex | I/O/解压/校验必需的临时 frame、字段读取与小型未知字段仍存在 |
| RetainPayload | response 级 `Bytes` 保存解压 payload，记录只存 offset/length | 尽量共享 buffer，hex 按需计算 |
| RetainWire | 同时保存匹配的 wire frame 与 payload/压缩说明 | 明确两份表示可能共存的成本；禁止在每行重复复制 |

RawPolicy 控制诊断原文保留，不控制业务必需信息。request context、数量/单位、未知 MAC 字段、响应来源与缺失原因仍属于结果。`Omit` 不能解释为零临时分配、零 raw frame 或进程内存硬封顶；用户长期持有的 DTO/Bytes 不再属于 runtime 的排队预算。

新模式需要 parser 本身避免 `record_hex` 的预先计算和原文复制。若迁移过渡仍使用旧 parser，发布说明应诚实标注“仅省去交付后的保留，尚未省解析分配”，不能提前兑现优化承诺。

### 8.4 MAC 未知字段

MAC 自定义字段有位图布局，不宜用 `HashMap<String, f64>` 作为完整公开模型。已识别字段用 `MacFieldId` 与匹配的 `MacFieldValue`，未知 bit 保留 `Unknown { bit, bytes: [u8; 4] }`，按 wire 顺序保存；不要为未知字段发明业务名称或猜测 f32/整数。

响应行宽按全部置位的合法字段位计算，未知合法 bit 仍占槽位；0x122B 当前合法范围为0–150，151–159保留位不占槽位（`src/eltdx/mac/bitmap.py:495`、`:511`）。不同命令位图长度和控制尾不同，必须各自核验；例如现有 MAC 0x122B 读取20字节位图但屏蔽保留位，而0x122C后4字节是控制区，不能一概当成160位字段。此处另见 `src/eltdx/mac/commands.py:102`、`:141`。

未知原始4字节是 forward-compatible 语义数据，即使 RawPolicy::Omit 也保留。已识别但未证实单位的字段标记单位未知。截断行不能作为完整结果静默返回；按该域明确 partial/framing 契约报告。

Python 兼容 adapter 可以继续把已知字段投影为原来的字典形状；Rust 内部保留未知字段不要求擅自在旧 Python 返回字典中添加新键，也不能影响已知字段的 wire offset。

### 8.5 P0 policy 冻结与兼容边界

#### 涨跌停

现有 Python 的 `daily_price_limits`、shortline 与 MAC 各有独立规则，不存在可直接提升为唯一规则源的实现。Rust 统一使用带 `id`、有效期、证券类别、交易所范围和证据 provenance 的规则 profile。没有已审核 profile 时返回 `LimitStatus::UnknownRule` 和 `LimitRule::Unknown`，上下限留空；不能用 `NoPriceLimit` 代替“规则未知”。缺少前收盘价是 `MissingReference`，与规则未知、已确认不设限分开。

| 案例 | Python 兼容投影 | 新 Rust 默认 |
| --- | --- | --- |
| 主板 ST | daily helper 10%；shortline 5%；MAC 5% | `UnknownRule` |
| 创业/科创 ST | daily helper 20%；shortline ST 优先而为5%；MAC 20% | `UnknownRule` |
| 未分配的 `30x` 代码段 | daily/shortline 的 `30` 前缀逻辑为20%；MAC 只承认300/301/302而退回10% | `UnknownRule` |
| 北交所 IPO 第2交易日 | daily helper 缺省按上市前5日逻辑；shortline按30%；MAC首日窗口后按30% | `UnknownRule`，等待有日期的上市阶段规则 |
| 指数/板块 identity | daily/shortline 可能按10%；MAC可按指数分类为不适用 | 只有分类及规则都有证据后才可 `NoPriceLimit`，否则 `UnknownRule` |
| 退市整理期 ST | 三个 Python 投影受各自 ST 判断影响；没有独立退市规则 profile | `UnknownRule` |

旧 Python 的名称检查、IPO窗口、价格舍入方式在迁移期分别保留：daily/shortline 使用两位 `round(value + 1e-9)`，MAC 使用 `round(value + 1e-5)`。共享 Rust 算法不覆盖旧投影；已批准的 Rust profile 必须同时给出最小价格单位和明示舍入模式，不能从任一 Python 函数复制一个默认值。六个样本、各入口旧值和证据缺口记录在仓库 `tests/fixtures/sdk/policy/price_limits.json`，政策说明见[决策清单](decision-register.md)。取得有效日期的交易所/监管规则和边界样本后，才可按版本批准 profile 并改变 Rust 结果。

#### 已知行为差异

| 场景 | Rust 业务契约 | Python 兼容契约 | 样本 |
| --- | --- | --- | --- |
| today bulk | Rust trade service 支持 today batch | native Socket/Pool 保留 `ValueError("trade batch only supports historical ticks")`；既有 custom transport 成功路径不收窄 | COMP-016/017，`legacy_boundaries.json` |
| 裸代码与精度预取 | 一个严格 `SecurityId` 驱动单市场 metadata lookup | `normalize_code` 与 `_ensure_decimals` 的不一致及全市场 fallback 保留 | `normalization.json` |
| 多个不同名 DAT | 未给 selector 是 ambiguity；显式 selector 才选取 | 继续按ZIP顺序选择第一个不同名 DAT；同名重复仍拒绝 | COMP-042，`042_financial_zip_dat.json` |
| 缺中间市场统计指数 | 按证券身份关联；缺 `sh880001` 时不把 `sh880006` 投影成市值 | 保留旧位置投影，不能把错值称为真实统计 | COMP-027，`legacy_boundaries.json` |
| 财务补充失败 | cache 区分成功空、失败和过期，失败不是完整的空缓存 | 保留吞掉单页异常并可能缓存空 tuple 的当前结果 | COMP-025，`legacy_boundaries.json` |
| 页数与期限 | Rust complete collect 使用有限页数和整次 operation deadline | Python `max_pages=None` 继续到协议终止页；timeout、startup、prefetch、恢复请求按既有分层预算 | COMP-006–011，`legacy_boundaries.json` |

以上是迁移隔离契约，不代表 Python 缺陷已修复。要调整 Python 表现需另列版本化变更并更新签名/行为基线。契约样本在仓库 `tests/fixtures/sdk/policy/legacy_boundaries.json`，政策说明见[决策清单](decision-register.md)。

## 9. 批量结果、缺失和分页

### 9.1 批量

```rust,ignore
pub type SnapshotBatch = BatchOutcome<Snapshot>;

pub struct BatchOutcome<T> {
    // 私有：items、来源与诊断统计；通过 getter 读取
}
pub struct BatchItem<T> {
    // 私有：input_index、SecurityId、状态
}
pub enum BatchItemState<T> {
    Present(T),
    ConfirmedMissing(MissingReason),
}
#[derive(Clone, Copy, Default)]
pub enum MissingPolicy { #[default] Error, AllowPartial }
```

结果按输入顺序对应，每个重复输入保留 occurrence/input_index。内部可以合并重复 wire 请求，不能悄悄删掉用户输入项。默认 `MissingPolicy::Error` 对已确认缺失返回 `ErrorKind::MissingData`，附确切 identity；不交付一个看似完整的缩短数组。`AllowPartial` 才返回带 `ConfirmedMissing` 的结果。

“服务端未返回”不是“证券确认不存在”。需要进行有预算的单项核验/已知协议处理后才标记缺失。低层请求与主数据链的损坏、外来代码、重复响应、解析失败、timeout、取消和重试耗尽仍返回 Err，不能当缺失项丢掉。某些主数据块成功后发生致命错误，第一版不返回普通成功；若以后需 checkpoint API，另设显式 typed partial-error 类型。该约束不取消第9.3节的有界下载结果，也不把第9.4节已定义的可选补充源失败升级为主数据失败。

空输入不建立连接，返回空且完整的 BatchOutcome。正常 Rust 公共入口的批量字节/条目限额在提交前检查，拆分后总预算仍属于原操作，价格解析使用同一 metadata snapshot。Python legacy 的拆分和恢复使用第6节保留的原请求期限与计数作用域。

新 Rust 默认每批最多65535个 occurrence、规范化身份逻辑缓冲最多2 MiB。HQ每项按1字节market加6字节ASCII代码累计，重复项也计入；其他协议按其规范化身份实际编码长度累计，均使用checked加法。这是SDK输入工作量限制，不是 `size_of`、总堆内存或调用者原始切片大小的声明；wire帧、输出、缓存另有独立预算。超限在复制及提交前返回 `LimitExceeded`。Python原入口不因该新增限制改变行为。

### 9.2 分页

`PageOptions` 私有字段提供校验 builder，包括 page size、max pages、max records。每个方法根据 wire 边界校验，不能因为公共字段是 usize 就默默截成 u16。公开查询使用 `KlineQuery`、`TradeQuery` 等，不给 OHLC query 接受 `flow` 后返回另一种模型；历史资金流走明确的服务方法。

```rust,ignore
pub struct PageOptions { /* 三个私有 usize 字段 */ }
impl PageOptions {
    pub fn new(page_size: usize, max_pages: usize, max_records: usize)
        -> Result<Self>;
    pub fn with_page_size(self, value: usize) -> Result<Self>;
    pub fn with_max_pages(self, value: usize) -> Result<Self>;
    pub fn with_max_records(self, value: usize) -> Result<Self>;
    pub fn page_size(&self) -> usize;
    pub fn max_pages(&self) -> usize;
    pub fn max_records(&self) -> usize;
}
```

三个值均须大于0，显式值必须有限且算术可表示，不以最大整数表示“无限”。`PageOptions` 不实现全域 `Default`，各域query提供自己的默认值，见[默认值表](decision-register.md)及分域规格。setter只检查通用范围，网络入口再检查该命令的page size、offset等wire限制；不能因为options通用合法就截断参数。`max_records` 可以小于一页；聚合时在追加前核对剩余额度，不能先超量扩容再截取为完整结果。

返回 `DataPage<T>` 至少携带 items、来源、已读范围、`next_cursor: Option<PageCursor>` 与结束依据。cursor 包含版本/query fingerprint，禁止跨 query、代码、日期或来源误用；不把内部 slot id 作为公共 cursor。`collect` 方法返回 `Collected<T>` 或分域规定的具名聚合类型（例如 `KlineSeries`、`TradePage`、`TradeColumns`），均明确完整性、页数和来源；以[HQ逐方法签名](hq-api-spec.md)为准。

`CodeQuery`、`CodeScanQuery`、`CategoryQuery`、`KlineQuery`、`DelistedQuery`、`TradeQuery`、`SpecialLimitQuery` 增加 `pub fn with_cursor(self, cursor: &PageCursor) -> Result<Self>`，复制不透明token到私有continuation，使用token中的下一offset。对应 `with_start/with_start_index` 会显式清除continuation；改变其他影响语义的字段不静默清除token，而在提交前发现fingerprint冲突并返回 `InvalidArgument`。token不可手造，不承诺稳定序列化或跨版本持久化。

fingerprint绑定协议/命令、实际证券或市场、已解析交易日期、period/adjust/kind/bar_time和返回形状等语义字段；`Current`绑定原请求解析出的日期，跨日失效。raw策略、`OperationOptions`及本次max_pages/max_records不参与语义fingerprint；不同page size能否使用由该命令offset语义决定，第一版不允许改变page size。token还绑定owner generation、endpoint、metadata版本和必要累加基准；失配在发请求前返回 `InvalidArgument`。服务器原来源暂不可连接为 `Connection`；本次取得pin后丢失才是 `PinLost`，不能自动换源继续。

每次续读创建新的operation预算；token不持活跃pin、不阻止close，也不保证两次调用间服务端数据快照相同。`*_many` 拒绝带continuation的query，包括只有一个输入的情况；使用对应单证券接口。带cursor的collect只收集续读起点以后的范围，不补回已经交付的页。非空满页且未证明结束时为 `Unknown + Some(cursor)`；合法空页或已证明EOF为None。`RequestedWindow + None` 可以表示调用方目标窗口已完成，但不能解释成全数据集EOF。续读样本覆盖跨证券、跨日、跨owner、metadata刷新、page size改变、旧endpoint不可用与offset溢出。

承诺收集完整结果的正常 Rust 公共 `collect` 方法达到 max pages/max records 而尚不能证明结束，返回 `LimitExceeded`，不能说成功拿全量。单页读取通过 cursor/结束依据明确后续状态；资源的有界字节读取则使用第9.3节的 `DownloadResult`。调用者要的是“最近N条”时，应在 query 中显式指定目标N，满足目标是合法结束，区别于防失控上限。不能单凭一页少于 page size 就推断所有协议都结束；按命令记录其 EOF/短页/固定总量证据。Python legacy 保留原 `max_pages` 的值、None语义与对应异常，不通过这些 Rust 公共上限添加新限制。

自动分页中的重复页检测、历史逐笔 absolute index、跨页累加基准、排序/去重、交易日分段和缺失核验由共享服务实现。跨页去重以已定义 identity 为准，不能按价格相同或分钟相同删除合法记录。空页、无进展、cursor overflow 要终止并保留原因。

### 9.3 有界资源读取与完整下载

资源服务区分“在上限内读取字节”和“已证明读取至 EOF”。建议沿用 `read`、`download_file` 名称，并新增显式严格方法 `download_complete`；它们的最后一个参数仍为 `&OperationOptions`。字节上限与 offset 使用经校验的参数，不能窄化后截断到 wire 字段。

```rust,ignore
pub struct DownloadResult {
    pub data: bytes::Bytes,
    pub completion: DownloadCompletion,
    pub next_offset: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadCompletion {
    Complete,
    LimitReached,
}

impl ResourceService {
    pub fn read(
        &self, path: &str, offset: u64, size: usize,
        options: &OperationOptions,
    ) -> Result<DownloadResult>;
    pub fn download_file(
        &self, path: &str, max_bytes: usize,
        options: &OperationOptions,
    ) -> Result<DownloadResult>;
    pub fn download_complete(
        &self, path: &str, max_bytes: usize,
        options: &OperationOptions,
    ) -> Result<bytes::Bytes>;
}
```

`read` 限定单次读取范围；`download_file` 从 offset 0 开始，在整个下载上限内分块读取。`Complete` 表示已经观察到该命令可证明 EOF 的证据，`next_offset` 为 None；从非零 offset 开始的 `read` 只证明该位置之后已读至 EOF，不声称拥有整个文件。`LimitReached` 表示请求字节范围或下载上限已用完且尚未证实 EOF，`next_offset` 为 checked `起始 offset + data.len()`。它不宣称文件必然还有内容。调用方后续读取还需校验来源/文件身份；仅凭 offset 不能保证跨会话续传一致性。

刚好收到 `size` 或 `max_bytes` 字节不是 EOF 证据，即使文件真实长度恰好等于这个值，也返回 `LimitReached`，除非另有经过验证的结束证据。命令认可的空响应/短响应可作为 EOF 依据，但不能把任意异常短响应、截断帧或长度不匹配解释成完整结束。上限为0时不启动连接，返回空 data、`LimitReached` 和 `Some(0)`；`read` 则保留传入 offset。配置和 offset 仍须先校验。

正常 Rust 公共 `download_complete` 只在证实 EOF 时交付 bytes；达到上限却未证实 EOF 时返回 `LimitExceeded`。若实现提供额外 EOF 探测，该探测的字节/请求上限必须显式计入预算并沿用同一个总 deadline，不能借探测无限下载。其低层传输、解析、来源改变、取消或总期限错误始终返回 Err，不转换为 `LimitReached`。整次分块下载固定同一 pin/来源，并验证 chunk_len 等于实际内容长度且不超过请求长度；pin 丢失或服务器切换使操作失败。

`read_stats` 需要完整且受限的档案：可读取 `档案上限 + 1` 字节，在 checked 加法和独立探测预算内区分超限；大于档案上限时在解压前拒绝。探测结果仍为 `LimitReached` 且无法证实完整性时也要拒绝，不能将截取 ZIP 交给 parser 当作完整档案。压缩后、解压后、条目数与单条目大小分别设限。

Python adapter 保留既有 `ResourceApi.read` 的 `FileContentChunk` 形状，以及 `download_file(max_bytes=...)` 返回截取 bytes 的行为；Rust `LimitReached` 不使旧 Python 方法新增限额异常或改返回类型。每个旧分块请求保留独立期限，不追加整份下载总8秒。`read_stats` 继续按旧上限加一探测与既有异常契约处理。新严格 Rust 方法的语义不能借回迁改变 Python 默认行为。

### 9.4 可选数据补充的显式降级

组合 helper 必须逐项声明必需主数据与可选 enrichment 来源。低层执行方法及主数据链继续严格传播错误；主数据完整成功后，已定义为可选的深度、profile、shortline 等补充失败，可以按该 helper 的策略返回主结果，并在 Rust 结果中提供每个补充来源的 `status` 与结构化 `diagnostics`。状态至少区分成功、未请求、无可用数据和失败，诊断保留来源、阶段及错误原因，不能用零值/空对象伪造成功，也不能把“补充失败”标为“主证券确认缺失”。

例如 `full_quotes` 的快照已成功、五档刷新失败时可保留快照并报告深度补充失败；是否允许部分证券补充、何时采用刷新值，按该方法已经冻结的规则执行。该降级是公开结果契约的一部分，不是所有方法通用的吞错开关。正常 Rust 公共入口的可选请求与重试仍使用整个操作的共享预算；单个补充源在较短子期限内失败可以记录为降级，调用方取消或整个操作 deadline 到期则终止整次操作并返回相应错误，不能落入 best-effort 分支继续请求或交付成功。

Python adapter 保持既有 DTO 字段、返回形状、异常和原有降级条件；Rust 新增状态通过不改变旧 DTO 的诊断通道提供。它使用 `PythonLegacyExecutionProfile`，保留可选源各次请求的超时/fallback条件，不受新的操作总8秒或共享 attempt cap 影响；已有显式取消仍按原契约处理。哪些源允许降级由P0基线逐项确认，迁移不能擅自把旧 strict 方法变成 best effort，也不能把旧可选补充改成强制成功。

## 10. 语言中立错误与 Python 映射

公共 `Error` 是一个结构化、实现 `std::error::Error` 的非穷尽错误对象。提供 `kind()`、`message()`、`context()`、`source()`；kind/phase/code 是稳定判定入口，message 供人阅读，不鼓励业务解析字符串。

`ErrorKind` 使用 `#[non_exhaustive]` 的无字段 variants，详细字段在 ErrorContext 中，方便 consumer 使用 `matches!(error.kind(), ErrorKind::InvalidArgument)`。建议初始集合：

```text
InvalidArgument / Unsupported / Closed / ForkedProcess
Timeout / Cancelled / CancelUnconfirmed / CloseTimeout
PoolBusy / Connection / PinLost / Protocol
MissingData / MetadataUnavailable / LimitExceeded / PushGap
ResourceFormat / DataNotReady / Io / Internal
```

context 包含 operation id、session id、协议 family、command、阶段、endpoint、请求 identity、页/尝试计数、缺失项、可选 cleanup 状态；不得塞 Python 类名。`TimeoutPhase` 至少保留 startup、admission、queue、resolve、connect、handshake、send、response、decode、aggregate、retry、cancel-confirm、close 区分。

`InvalidArgument` 进一步有语言中立的原因（格式、范围、溢出、组合冲突）；Rust 类型层已排除的 Python TypeError 由 PyO3 参数提取阶段处理。`Unsupported` 区分 feature 未启用、协议能力不支持、未迁移能力。feature 不启用时 Rust 模块可不存在，动态边界则返回稳定能力错误。

Python adapter 的目标映射：

| SDK 错误 | 现有 Python 外观 |
| --- | --- |
| InvalidArgument/范围或格式 | ValueError；数值转换溢出按兼容规范为 OverflowError；类型不匹配由绑定层 TypeError |
| Protocol | ProtocolError |
| MissingData | MissingCodesError，保留 missing_codes；AllowPartial 可按旧入口产生 MissingCodesWarning |
| LimitExceeded | 新Rust按结构化限制原因返回；Python桥接保留对应域原异常（例如恢复预算的ProtocolError、分页上限的RuntimeError），不新增统一Python限额异常 |
| Closed、连接关闭、PinLost | ConnectionClosedError；底层其他连接错误保持 TransportError |
| Timeout | ResponseTimeoutError，保留 phase/context |
| PoolBusy | PoolBusyError |
| PushGap | PushOverflowError 或订阅结构化 gap，根据原方法契约 |
| CloseTimeout | TransportCloseTimeoutError |
| Unsupported | UnsupportedCommandError 或该扩展既有异常 |
| ResourceFormat | ResourceFormatError；日期不匹配细分为 TdxStatsDateError |
| DataNotReady | 对相应短线指标入口映射 ShortlineIndicatorsNotReadyError |
| Cancelled | Python 显式取消机制所定义异常；SIGINT 保留 KeyboardInterrupt，不包装成普通网络错误 |
| CancelUnconfirmed / Internal / 未分类 Io | 保留 source 和清理状态的 TransportError 或该域既有 Io 错误 |

映射表要由 P0 Python 基线逐项确认，不能在绑定层吞掉任意 Exception 后猜类名。旧 NativeError 三元组可以暂时兼容，生成它的最后一层才允许加入旧 `python_kind`。Rust 公共 Error 不依赖 Python、不持 PyErr，也不需要 GIL。

## 11. advanced 与同步/异步分界

### 11.1 精选 advanced

`advanced::hq` 第一版仅公开经过审核的 `RequestFrame`、`ResponseFrame`、`FrameDecoder`、`CommandRequest` 和一个受 `SessionOwner` 控制的 `HqSession`。纯 codec 可以不联网单独使用；它们有各自明确的兼容说明。

`HqSession` 由 client 的受控 getter 创建，持同一 owner，执行仍接受 OperationOptions，遵守共享 close、deadline、容量和 identity 栅栏。不能公开裸 Engine 引用、可变 decoder/socket、未经校验的 slot id 或直接注入 scheduler event。低层执行结果可以是 RawReply，业务解析需显式调用有 metadata context 的入口。

`FrameDecoder` 是 SDK 选择的公开包装名；当前 protocol 中对应实现名为 `ResponseFrameDecoder`，不能把草案名称误当成现有源码符号。公开包装必须固定缓冲上限、分片/损坏恢复与错误契约，新增 raw 执行能力也不提供任意 socket 写入绕过通道。

高级用户若直接依赖独立 `eltdx-runtime` crate，其 API 稳定范围由该 crate 另行说明；它不因 SDK 的存在自动获得 SDK 契约。

### 11.2 P1–P5 同步保证

公开 client/service/pin 方法可能阻塞当前线程，不应直接在异步 executor worker 中长时间运行。用户可自行使用 `spawn_blocking`；该桥接并不改变底层取消语义，也不能证明 Drop future 后 wire 已取消。

TdxClient 与 owned service 目标为 Send + Sync；方法内部不在状态锁下等待网络。PinnedSession 的串行请求与 Subscription 的单消费者可移动性需通过编译断言确认，不能靠文档自行声称任意并发调用安全。

### 11.3 P6 真正 async 的进入条件

进入 async 前必须具备可 await 的 completion/waker、单终态仲裁、Future Drop 的取消/资源移交、后台清理保障以及同一 session 的 sync/async close 协议。异步 Future 不能持有状态锁跨 await，也不能把整个 Engine::execute 放进 `async fn`。

Future Drop 无法 await 退休确认：应发取消并把 owned grant/cleanup责任转移给独立控制路径；显式 async cancel/close 才能返回确认。与 sync 相同，若成功先完成，资源结果必须由 completion 或 guard 管理，不能因未来无人 poll 而泄漏。普通 sync 和 async facade 应调用同一业务状态机/OperationContext，而不是复制分页与重试算法。

## 12. 实现完成前的验收门槛

本文未执行测试。后续实现至少用离线测试证明以下契约，再讨论发布：

1. 外部 consumer 能 `build → config → get/drop services → close → close`，没有 DNS、网络或后台线程；非法配置返回 ErrorKind 而非 panic。
2. 最后 client 丢弃但 service/pin/subscription 仍在时会话按约定活着；显式任一 clone close 后所有 child 拒绝新请求；worker 无永久 owner 环。
3. pin 的分配/发送成功/接收丢弃/取消/close 全排列均只有一个释放责任，无 orphan pin，成功结果也不丢失。
4. 正常 Rust 公共入口的首次 startup、排队、metadata、分页、fallback 共用同一个 deadline/预算；关闭控制在满额队列下仍能推进；取消确认失败不复用旧连接。
5. 两个不同 metadata namespace 的 client 并发解析同代码不串精度；一个 batch 中刷新不会产生两套尺度；新路径不触碰 legacy global decimal cache。
6. RawPolicy::Omit 不交付/保留原文与逐行 hex；Retain 模式共享 payload 且范围正确；未知 MAC 槽位与字段顺序在所有策略中保留。
7. 默认 strict batch 缺失与协议损坏分开；重复输入、空输入、页无进展、上限触发和主数据部分成功后的 fatal error 均符合返回契约；可选 enrichment 失败保留显式状态/诊断，正常 Rust 公共入口的总期限与取消仍终止整个操作，Python legacy 保留原期限/fallback作用域。
8. 资源覆盖空文件、零上限、精确上限、超过上限、精确 chunk 边界、短响应和异常；仅有 EOF 证据才标 Complete，严格下载在无法证实时返回 LimitExceeded，read_stats 的上限加一探测在解压前拒绝超限。
9. Python close/connect 换代保持旧行为，旧 pin/subscription 永久失效；错误和 public models 逐域兼容，原 max_bytes 截取与可选补充降级不因新 Rust 类型改变。
10. 同一 fake transport/受控时钟下双向验证执行策略：两页各5秒、总耗时10秒时 Rust 默认总8秒超时而 Python legacy 成功；单个 Python 请求超过8秒仍触发旧请求超时。分别验证精度预取与主请求各自小于8秒但合计超过8秒、超过Rust共享cap仍在旧恢复次数内、原 `_RECOVERY_REQUESTS=32` 用尽及 `max_pages=None` 的终止条件；Rust与Python都按各自契约完成或失败，不以统一预期掩盖差异。另用受控startup延迟验证Rust入口计时与Python原 `reserve_submission` 后计时边界。
11. 从独立 `eltdx-python` crate 编译调用 `#[doc(hidden)] pub mod __python_compat` 的最小桥接面；桥接内部选legacy策略，普通Rust API无legacy开关，内部context/owner仍不导出。默认、逐项 feature、mac→ex、full、serde 组合可从独立consumer编译，依赖图无Python/PyO3；真实async未实现时不导出虚假能力。

所有 online 行情、延迟和订阅覆盖验证另列显式环境条件。空响应/合成 fixture 可以证明边界行为，不能证明服务端完整业务语义；尤其历史资金流 category22 的完整记录目前缺实际样本，见 `crates/eltdx-protocol/src/commands/history_fund_flow.rs:78`。
