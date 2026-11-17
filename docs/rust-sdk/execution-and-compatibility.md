# 执行内核与 Python 兼容层实施规格

> 状态：拟议设计；限定 T04–T12、T15、T18 的跨模块接口。本文补全 [架构](architecture.md) 和 [公共 API](public-api.md) 的内部边界，不表示这些 Rust 类型已经实现。外部逐业务方法见 [HQ 规格](hq-api-spec.md) 和 [扩展规格](extension-api-spec.md)。

## 1. 必须同时成立的三件事

1. Rust 应用直接调用 `eltdx`，依赖闭包无 Python/PyO3。
2. Python wheel 仍提供原 Python 签名、对象、异常、默认值和行为，无须用户安装 Rust 工具链。
3. 正常业务算法只有一份：分页、分组、关联、复权、字段投影等由 Rust 业务模块实现。旧 Python 宽松输入和历史策略通过适配/执行策略表达。

策略差异不只包含超时。代码规范化、批量缺失处理、最大页数、可选补充源降级、返回 raw、关闭顺序及错误投影都要逐项确定。不能把一个 `legacy: bool` 分散到各模块；也不能为每个旧行为建立可自由组合的公共配置系统。

## 2. 依赖与可见性

```text
Python facade -> PyO3 binding -> eltdx::__python_compat
                                    |
Rust public API --------------------+-> services/domain
                                          |
                              executor + execution policy
                                |                    |
                          HQ runtime           F10/EX/MAC driver
                                |
                        context-aware protocol
```

| 边界 | 允许类型 | 不得越界的内容 |
| --- | --- | --- |
| Python → binding | 已有 Python 参数、对象、异常 | Rust 内部锁、slot、owner 引用 |
| binding → `__python_compat` | Rust 字符串/数值/枚举、兼容输入/输出 DTO | `PyAny`、`PyErr`、GIL token、Python runtime |
| service → executor | 已验证请求、预算许可、元数据快照 | Python class 名、tuple stride |
| executor → runtime | wire request、期限、raw 策略 | `StockProfile` 等组合业务对象 |
| runtime → protocol | 编码/原始回复、连接上下文 | 缓存刷新和业务重试决策 |

`__python_compat` 必须 `pub` 才能被另一个 crate 调用，并用 `#[doc(hidden)]` 隐藏常规文档。它是随绑定协同升级的窄入口，不是访问控制措施。正常 Rust facade 永远创建 `RustPublicExecutionProfile`；不能把一个已创建的普通 client 转成 legacy client。

## 3. 执行策略：明确分歧，不复制循环

### 3.1 策略表

| 行为 | Rust 公共策略 | Python 兼容策略 |
| --- | --- | --- |
| HQ `timeout=8s` | 一次公开业务调用的总期限 | 原 HQ wire 请求各自 8s，保留原计时起点；EX/MAC等原默认15s的域仍为15s |
| startup | 计入总期限 | 按旧 runtime 真实边界；不新占原请求 8s |
| metadata 预取 | 同操作总预算，完成后冻结快照 | 原预取/主请求各自预算，原代码推断规则需兼容 |
| wire retry | 每请求有限尝试，另受操作累计上限 | 保留旧 runtime 各请求最大尝试 |
| 批量补查 | 共享累计预算及域上限 | 保留旧恢复计数、顺序、协议错误分类 |
| 分页 | 必须有有限总预算；明确 `PageOptions` 上限 | 原 `max_pages=None` 仍不新增调用方页数上限 |
| 可选补充失败 | 声明过的来源允许降级；总期限/取消终止 | 保留原降级条件，含原本允许的请求级超时 |
| close | 同 owner 单个绝对 shutdown deadline | 保留原各阶段关闭顺序、期限、异常语义 |
| Python 返回形状 | 不适用 | tuple/dataclass/dict、别名、顺序、raw 的旧投影 |

资源回收、不得遗失 pin grant、帧大小校验和引用生命周期不是可兼容关闭的选项。旧代码的 bug 若影响用户观察结果，应单独登记，不因修资源清理而夹带修改业务输出。

### 3.2 内部接口形状

以下是内部拟议接口，采用枚举与普通函数优先，不引入通用任务引擎。具体字段名可在 T08 内调整，跨任务语义必须保持。

```rust,ignore
pub(crate) enum ExecutionProfile {
    RustPublic(RustPublicLimits),
    PythonLegacy(PythonLegacyLimits),
}

pub(crate) struct OperationContext {
    identity: OperationIdentity,
    profile: ExecutionProfile,
    cancellation: OperationCancellation,
    metadata: Option<Arc<MetadataSnapshot>>,
    raw: ParseRawPolicy,
    work: WorkCounters,
}

pub(crate) struct RequestPermit {
    deadline_policy: RequestDeadlinePolicy,
    max_wire_attempts: NonZeroU32,
    identity: RequestIdentity,
}

pub(crate) enum RequestDeadlinePolicy {
    Absolute(Instant),
    LegacyAfterStartup(Duration),
}

impl OperationContext {
    fn begin_child(&mut self, purpose: ChildPurpose)
        -> Result<RequestPermit>;
    fn check_boundary(&self, phase: OperationPhase) -> Result<()>;
    fn install_metadata(&mut self, snapshot: Arc<MetadataSnapshot>)
        -> Result<()>;
}
```

类型含义与约束：

- `RustPublicLimits`：方法入口 `Instant`、最终绝对期限、允许的总额外尝试、业务页数/记录数/字节上限。数值均来自已验证配置或 query。
- `PythonLegacyLimits`：原 per-request duration 和域计数限制；不伪造 `Instant::now() + 8s` 总期限。支持旧无页数上限用显式 `Option<NonZeroUsize>`，不能用 `usize::MAX` 再乘 duration。
- `OperationIdentity/RequestIdentity`：操作 ID、子请求序号、协议族；不含用户可构造 slot/generation。
- `ChildPurpose`：至少区分 `Metadata`、`Page`、`MissingRecovery`、`Fallback`、`Renewal`；已有域继续保留独立的合法恢复计数，不能一概视为普通页。
- `OperationCancellation`：关联用户 token、相应关闭域和内部停止信号；普通Rust关联owner整体关闭，Python兼容关联旧域边界，不能提前取消尚未进入close阶段的域。是否有用户 token 由入口决定。Python KeyboardInterrupt 由 binding 协调，不用普通字符串模拟。
- `WorkCounters`：用于预算授权与诊断的正常页数、wire 次数、恢复次数、收到字节数；计数加法必须 checked。
- `ParseRawPolicy`：protocol 自有枚举；由外层 raw 策略转换，不能反向依赖 SDK。
- `OperationPhase`：沿用公开错误的阶段分类，不能为每个函数名增加一个新阶段。

一次 service 算法只调用 `begin_child/check_boundary`，不直接算 timeout 或重试次数；profile 决定该 child 的授权。禁止业务模块到处匹配 `ExecutionProfile`，确有域兼容分歧的地方使用该域的已审核策略值，例如 `MissingRecoveryPolicy` 或 `LegacyRawProjection`。

### 3.3 runtime 新旧入口如何共存

T06 新增的绝对期限入口在 `ensure_runtime` 前检查期限；旧 typed `execute` 保留原启动后创建请求期限的语义。两者复用共同的启动、排队、发送、退休实现，不维持两套调度器。

`LegacyAfterStartup` 只允许来自绑定桥接/旧兼容入口，正常 Rust service 不能生成它。进入旧计时边界后立即转换为真正 `Absolute(Instant)`；后续排队/收发/重试不得再次重置该请求期限。这样保留“每个旧请求重新计时”，不会错误地变成“同一请求每次重试重新计时”。

T06 的行为测试必须分别验证：启动耗时不侵占旧请求期限、侵占新公开操作期限；相同已创建请求的重试不扩大期限；两个旧子请求仍拥有各自合法期限。

### 3.4 额外尝试如何跨线程准确计数

`begin_child` 只授权一个逻辑子请求，不能因为它携带 `max_wire_attempts=2` 就让 runtime 自行绕过共享预算重发。T06/T08 需增加一个语言中立的尝试授权边界：每个 `RequestPermit` 持有私有的共享预算引用，runtime 在每次开始连接/发送尝试前申请一次不可 Clone 的许可。Python legacy 使用其原请求/域计数实现同一内部接口，不读取新 Rust 的32次计数器。

| 尝试用途 | 正常 Rust 额外预算 | 必须同时满足 |
| --- | --- | --- |
| 首次必需主请求、正常下一页、必需metadata子请求 | 不扣额外次数 | operation期限、页/记录/字节额度、owner可接纳 |
| 同一逻辑请求重发或故障后换endpoint | 每次扣1 | 本请求尝试次数仍低于上限，命令/失败阶段允许重试 |
| 缺项二分/单项诊断、空值探测、fallback的首次请求 | 每次扣1 | 对应域的恢复/探测限制和operation剩余额度 |
| 上一行的请求再次重发 | 每次再扣1 | 不能因最初已扣一次就给重发免额 |
| 已拒绝或未真正开始的尝试 | 不记已开始次数 | 归还未使用预留；不得退回已开始但失败的尝试 |

预算实现区分 `reserved` 与 `started`，在同一小型临界区执行“核对终态、核对额度、预留”，并以唯一许可完成一次“开始”或“归还”。开始提交点必须在相同终态/admission仲裁gate再次检查相应owner/域、取消和deadline，再把预留原子转为started；不能只在预留时检查，随后越过close/cancel继续启动。检查失败则归还未用额度。一次attempt覆盖该次连接和发送整体，不对connect与send重复计数；中途失败后的重新连接/重发才是另一次attempt。

诊断分别记录逻辑请求、已开始尝试与额外尝试；禁止service和runtime对同一发送重复扣账。锁不跨连接、收发、parser、Python callback或等待；CPU工作和排队仍按总期限停止。许可在开始前被取消时，Drop仅归还尚未开始的预留，不能把已发送失败当成没有发生。

即使初版同步串行，测试也必须覆盖两个并发子请求争抢最后一个额外额度，以及关闭/取消与许可开始竞争。只有一个许可可获准；不得读一次剩余计数后分别发出两次请求。局部恢复额度和全局额外额度必须一并成功预留，局部失败不能泄漏全局额度。

新 Rust 在未获额外额度或到达页/记录上限时返回 `LimitExceeded`，context包含限制名、配置值、已开始数量和最后一次可恢复错误。若额度仍充足但单请求达到自身尝试上限，保留最后一次 `Connection`/`Timeout` 等实际错误和attempt信息；畸形帧按 `Protocol`，不能改成缺失或额度耗尽。F10默认只有一次HTTP应用尝试、不隐式重试，是扩展域的明确收紧，不能从HQ的默认2次推导为自动重发POST。

`extra_attempt_budget(0)` 的意义是正常必需请求仍可执行，但恢复/重发/fallback不能开始。若单次可恢复失败随后需要恢复，它返回含原失败原因的 `LimitExceeded`；合法空结果则直接成功，不为了耗尽预算而额外确认。Python旧恢复预算的错误类、扣减时间及顺序按COMP-009/010保持。

## 4. 元数据准备与不可变快照

1. 入口先验证证券标识、请求大小和参数互斥条件。
2. 根据协议命令确定精度需求；无需价格精度的请求不强制加载整个市场。
3. 需要预取时，通过同一操作 policy 申请 metadata 子请求；不同等待者共享下载任务但保留自己的停止权。
4. 完整验证候选元数据，再一次性发布新版本；失败不覆盖旧好版本。
5. 该业务调用第一次解码前安装一份不可变快照，其后全部 chunk/page/fallback 使用相同版本。
6. 结果保存来源版本/必要精度来源；不把完整元数据表复制进每条行情记录。

`install_metadata` 在已开始解析后不可替换快照，应返回内部状态错误；refresh 创建新版本供后续业务使用。新 Rust 默认 owner 隔离。Python 的代码推断、预取覆盖及 fallback 必须通过基线样本；某些旧全局污染若被确定为缺陷，可以独立修复，但不能宣称固定快照天然与任意旧全局副作用等价。

## 5. binding 的转换责任

### 5.1 输入

1. Python facade 继续执行原本可观察的轻量验证，例如类型错误的先后顺序、位置/关键字参数规则。
2. PyO3 提取已确定的 Rust 兼容输入，不在 GIL 下执行阻塞网络。
3. 桥接选择 legacy policy 及域兼容参数，调用同一 service 算法。
4. 对自定义 Python transport，由明确的 executor adapter 进入同一算法；native 正常路径不反向调用 Python。

没有可靠证据时，不改变“先验证哪一个参数”的异常优先级。旧入口接受字符串、整数枚举或序列的宽松规则不应直接套严格 `SecurityId::from_str`；兼容输入先做旧规则归一化，再产生业务内部身份。

### 5.2 输出

| 类型 | Rust 内部结果 | Python 兼容投影 |
| --- | --- | --- |
| 单值/单证券 | typed value/记录 | 原对象类型和 `None` 条件 |
| 批量 | 输入索引、状态、来源 | 原 list/tuple/dict 及去重/顺序规则 |
| 行情/文件 raw | `Bytes`、页范围、未知字段 | 原 `raw_payload`/`record_hex` 是否保留的规则 |
| 订阅 | 单消费者事件流 | 原 poll/callback 适配，不把新 enum 直接泄露 |
| 有界下载 | `DownloadResult` | 原截取 `bytes`；单块读仍 `FileContentChunk` |
| 复权/单位 | 有来源的领域数值 | 原 float/int/日期；原舍入规则 |
| 错误 | `ErrorKind + context + source` | 原异常类、属性、`__cause__`、warning |

转换发生一次。不能在 Python 再实现排序/分页补查/业务公式；若旧模型需要派生属性，保留纯对象层属性或调用已共享的纯计算，不增加第二份规则。

## 6. 自定义 transport 与 GIL

自定义 transport 是 Python 已有扩展点。迁移时按能力分为 native executor 和 callback executor，但不新增 Python 用户必须实现的方法。

- 输入仍为旧 `execute` 名称和参数格式；返回可为旧协议模型/兼容对象。T12 先验证最小自定义实现，不要求继承完整 native transport。
- callback 在调用 Python 的当前受控线程获得 GIL；进入同步 Rust 等待网络之前释放 GIL。不能由持有 runtime 状态锁的 I/O worker 回调 Python。
- 支持回调的 executor 独立标记可取消性和返回类型；Python callback 运行不可硬中断时保留原能力范围，不能声称 Rust token 可抢占任意 Python 代码。
- callback 重新进入同一 client 时不得死锁；如果某条路径必须禁止重入，先核对旧行为并作为显式变更处理，不能用新增 panic 兜底。
- 异常保存为桥接错误直到返回 Python；Rust 公共 `Error` 不持有 `PyErr`。跨边界的因果链由 binding 层保存/恢复。

若在 T12 尚无法用同一编排无损承接所有自定义对象，允许暂留该特定兼容路径，但在迁移清单记录入口、原因、差分测试及退出条件；不能把 native 常规路径也回退 Python。

## 7. shutdown 与返回结果的交接

正常 Rust close 和 Python legacy close 共用关闭状态及资源执行器，不共用错误的计时假设。

| 阶段 | 正常 Rust | Python legacy |
| --- | --- | --- |
| 封闭 admission | 共享 owner 线性化，一次 | 按原ordinary/money-flow/auction关闭阶段触发域gate，不在入口提前封闭所有域 |
| 停止订阅 | 使用 owner 剩余 shutdown 预算 | 按旧域顺序与可观察等待语义 |
| 关闭 HQ/专用池 | 各池获得同一总期限剩余量 | 按旧主 transport、资金流、竞价等顺序保留阶段预算 |
| 返回成功 | 所有自有资源已确认结束 | 保留原结果/异常映射，同时不遗失资源所有权 |
| 超时/中途失败 | 保留失败根因与清理状态 | 保留原异常先后，后续 best-effort 清理不覆盖首因 |
| 再次 connect | 返回 Closed，需新 client | 桥接创建新 generation；旧子对象不复活 |

普通Rust整体关闭、或Python相应域开始关闭时，已在终态仲裁中成功的业务结果可交付；否则按该域规定返回 Closed/原失败。Python尚未开始关闭的域不能仅因另一个域正在关闭就拒绝或取消；ordinary首错也不能直接把后续域标为Closed。对同域关闭中的重入、ordinary已关而专池仍开等并发细节，先录制COMP-013基线再冻结桥接，不能凭“更一致”扩大关闭范围。

不可在 native 返回成功后、Python DTO 构造失败时丢失 pin/subscription：桥接资源必须先处于一个 Rust RAII 容器中，直到 Python holder 成功接管后再提交所有权。best-effort清理只处理已终止或确认失败路径，不得以清理名义主动关闭原语义下仍可使用的其他域。

## 8. 分任务的接口冻结与测试要求

| 任务 | 输出接口/决定 | 先写的失败案例 | 通过标准 |
| --- | --- | --- | --- |
| T04 | client/config/value 类型 | build 创建线程、无效配置泄漏 | 无副作用、结构化错误 |
| T05 | owned PinGrant + 终态仲裁 | 成功发送后 receiver drop | 每个 grant 恰好一个释放责任 |
| T06 | Absolute/legacy 请求期限桥接 | 慢 startup + 两种调用 | 时间窗与实际旧边界相符 |
| T07 | shutdown policy + generation | 多池慢关闭、并发 close | 正常 Rust 总期限不相加；Python 原预算保持 |
| T08 | OperationContext/profile 许可 | 两页 5s、总限 8s | Rust超时、两代Python均成功 |
| T09–T10 | 快照准备/安装/解析 | 两owner冲突精度、半批刷新 | 整个新操作单版本、兼容规则有证据 |
| T11–T12 | quotes/codes 和 Python桥 | 缺项、别名、自定义对象/GIL | 新Rust严格、旧Python外观保持 |
| T15 | subscription owner/交接 | 创建成功但Python对象构造失败 | pin完整回收，无悬挂worker |
| T18/T23/T27 | 各域兼容输入/输出表 | 原异常顺序、raw、超时、降级 | 对应case全部通过才切换默认路径 |

最终必须创建一个真正独立的 Rust consumer 编译 `eltdx`，再在另一个任务构建 binding，证明桥接可见性不会迫使普通 Rust 依赖 PyO3。只看 `cargo metadata --no-deps` 不足以证明生产依赖闭包。

## 9. 不允许通过临时补丁绕过的检查

- 不能给每页重新创建普通 Rust `OperationOptions::with_timeout(8s)`，这破坏总期限。
- 不能把 Python 旧 timeout 直接放入新总期限，也不能把所有旧限制统一成共享 attempts。
- 不能给每个 raw record 复制完整 payload 以简化生命周期。
- 不能从错误的浮点结果反推原始整数或通过补乘除冒充精度隔离。
- 不能通过增加 `unsafe Send/Sync` 或持锁回调解决跨线程编译问题。
- 不能因一段 Python adapter 暂留而把全部业务重新委托 Python，或发布要求 Rust 用户启动 Python 服务的 SDK。

这些检查需要行为证据。源字符串包含 `deadline`、`Arc` 或 `drop` 都不能证明契约成立。
