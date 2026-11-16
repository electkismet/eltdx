# Rust SDK 分阶段开发计划

> **For agentic workers:** 执行时使用 `superpowers:subagent-driven-development` 或 `superpowers:executing-plans`，按独立任务实施与审阅；本文件中的复选框记录开发进度，不代表本次文档工作已经实现。

**Goal:** 交付不依赖 Python 的 Rust SDK，并逐域让现有 Python API 复用同一份 Rust 业务逻辑。

**Architecture:** 新增 `crates/eltdx` 作为公开 API 和共享业务实现，复用已有 protocol/runtime；PyO3 只保留兼容与转换。HQ、F10、EX、MAC、离线文件按边界分离；操作上下文和所有权语义贯穿实现。

**Tech Stack:** Rust 2021 / MSRV 1.89、现有 Tokio/bytes/encoding_rs/flate2、PyO3 0.29.2 兼容绑定、Python >=3.10；HQ 统计 ZIP 依赖在 P2 验证锁定，F10 HTTP 依赖在 P3 验证锁定。

**Spec:** [架构设计](architecture.md)、[公共 API](public-api.md)、[能力矩阵](capability-matrix.md)、[验证规范](verification.md)、[内部执行与兼容实现](execution-and-compatibility.md)、[HQ 接口规格](hq-api-spec.md)、[扩展接口规格](extension-api-spec.md)、[逐项兼容案例](compatibility-cases.md)、[决策与默认值](decision-register.md)、[Python 基线](python-baseline.md)。

## 0. 执行规则

- 所有命令、接口和测试示例均是实施计划，除文档中明确列出的检查外，本轮未运行。
- 修改代码前确认实际 HEAD 与基线 `af471f1` 的差异；不要覆盖用户修改。需要分支时使用 `codex/` 前缀。
- 每个任务先补可失败的行为测试，再实现；按用户明确要求，T01–T30全部实现代码完成之前不执行任何测试、构建或网络验证。全部实现完成后统一运行全套测试及构建矩阵；静态审阅不作为通过证据。
- 每个任务形成独立可审阅变更；提交前检查 diff、范围和测试证据。不得自动发布、打 tag、合并或上传包。
- 默认 `features=[]` 仍提供 HQ；`serde/offline/f10/ex/mac/full` 定义按架构文件，`mac` 启用 `ex`；没有预留但不可用的 async feature。
- 不修改已提交黄金输出以配合错误实现；更新必须说明语义变化、独立来源和新旧差异。
- 共享算法迁移后删除重复实现前必须通过 Python 兼容回归；暂时保留的兼容算法必须有清单和退出条件。
- Python 的安装、导入、参数、返回模型、异常、缓存、自定义 transport、关闭后重连、降级与文件行为以原契约验收。已有缺陷的修复独立记录；新 Rust API 的严格语义不能未经决策就改变 Python 默认。
- 新 Rust 采用整次操作的总期限/累计尝试预算；Python 兼容 policy 保持旧逐请求 timeout、原计时起点及各层次数限制，不能把默认 8 秒改成全流程 8 秒。共用业务算法，详见架构 5.4。
- 关闭后的 Rust 会话不重开。Python 旧重开行为通过新建 generation 兼容。
- 任务结束同步文档源与包内镜像，运行文档链接/镜像检查。
- 跨会话恢复点维护于 [IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md)：每阶段、每个进度提交后更新下一步、待办和验证延迟状态。

### 0.1 任务证据格式

每个实现任务的评审记录包含：任务 ID、基线/提交 SHA、涉及需求、运行命令、退出码、用例计数、跳过原因、结果摘要、待解决问题。只写“通过检查”不够；在线验证必须单列，不与离线测试混记。

### 0.2 需求追踪

| ID | 要求 | 首要任务 | 证明方式 |
| --- | --- | --- | --- |
| R01 | 两个产品独立安装与使用 | T04、T28–T29 | 无 Python 的 Rust consumer + 无 Rust 工具链的 wheel consumer |
| R02 | Rust/Python 共用业务 | T12、T18、T23、T27 | 同语料差分 + 旧 facade 回归 |
| R03 | owner/pin/pending 无遗失 | T05–T07 | 确定性竞态、Drop/close 行为 |
| R04 | 操作期限含 startup/队列/子请求 | T06、T08 | 受控时钟/loopback 超时 |
| R05 | metadata 不跨 client 污染 | T09–T10 | 并发多 owner 精度用例 |
| R06 | 顺序、缺失、分页完整性 | T11–T14 | 空页/短页/重复/截断样本 |
| R07 | 单位、日期、复权明确 | T03、T10、T16 | 值与原始位级样本 |
| R08 | 订阅正确持有/释放连接 | T15 | 启停/续订/gap/fanout 测试 |
| R09 | F10/EX/MAC 边界与无损数据 | T19–T20、T24–T26 | 请求黄金、动态列、未知位 |
| R10 | 文件读写/下载安全且有界 | T17、T21–T22 | 截断/ZIP/索引/对齐故障样本 |
| R11 | 兼容行为有明确决策 | T01–T03 | 冲突记录与版本化策略 |
| R12 | 可发布、可复验 | T28–T30 | package/consumer/证据清单 |

## 1. 阶段依赖与可交付物

```text
P0 T01–T03
  -> P1 T04–T10
    -> P2 T11–T18 -> 可用的 HQ Rust SDK
      -> P3 T19–T23 ─┐
      -> P4 T24–T27 ─┴-> P5 T28–T30 -> 全范围候选版本
P6: 独立设计与实施真正 async / 原生 CLI / 服务端
```

P3 与 P4 可由不同人员并行；公共类型、owner 和 operation 的修改必须经过共同评审。T18、T23、T27 是各自阶段的 Python 迁移验收，不能拖到最后才发现兼容问题。

## 2. P0：可复现基线和契约

### T01：修正验证入口，建立源码与能力快照

**文件：** 修改 `scripts/unified_test.py`、`tests/contracts/test_unified_test_plan.py`；新增 `tests/contracts/test_command_registry_consistency.py`、`tests/contracts/manifests/rust_sdk_migration.json`。

**输入：** 当前 26 个 wire code、25 项 Python registry、目录124项、现有测试分组。

**产物：** 不混淆计数的清单、可解析且可收集的离线测试入口、迁移状态逐项记录。

本轮已提供的静态输入是 [Python API 快照](baseline-python-api.json)，生成方法见 [基线说明](python-baseline.md)；它不是下面尚需实施的运行期反射与行为测试。

- [ ] 加测试：runner 中每个 `file::test_name` 均能由 pytest collection 解析；先让旧的 `all_22_commands` 节点失败。
- [ ] 修为实际 `test_differential_matrix_covers_all_commands`，保留原测试语义。
- [ ] 清单对每条能力记录 `id/source_files/feature/stage/status/fixtures/compatibility`，wire code 独立字段；money_flow 缺 registry 不是通过改数字掩盖。
- [ ] 校验静态快照的签名、默认表达式、导出、属性及模型字段；补真实 import/继承/decorator/dataclass/enum 反射记录，不能把候选定义计数当稳定接口数。
- [ ] 新增 registry 集合差异断言：显式记录允许的差异及原因，不硬编码“所有集合都相等”。
- [ ] `migrated` 记录的 `sdk_evidence` 必须指向 `crates/eltdx/src/lib.rs::PublicSymbol`，并解析为 crate 根的公开声明或单符号 `pub use` 重导出；不得用私有父模块中的声明、注释或字符串文本充当证据，且 `lib.rs` 必须列在该记录 `source_files`。协议层 wire 支持不能充当 SDK 实现证据。
- [ ] `compatibility` 必须是 pytest 默认文件规则（`test_*.py` 或 `*_test.py`）可收集的精确节点，节点路径匹配模块级 `test_*` 函数或一个顶层 `Test*` 类中的 `test_*` 方法；不接受嵌套测试类。排除定义了 `__init__` 或 `__new__`、设置 `__test__ = False` 的测试类，以及被标记为 fixture 或设置 `__test__ = False` 的函数，不得仅凭 AST 中任意同名节点通过。
- [ ] Python 模块覆盖按来源分别校验：`frozen_static_baseline` 必须映射到冻结快照模块；`migration_manifest` 必须以 `record_id` 绑定具有合法迁移状态的记录且模块路径出现在该记录 `source_files`；`compatibility_contract` 的精确 pytest 节点或测试模块级导入必须显式导入对应 `eltdx` Python 模块，其他测试函数的局部导入不得代替。
- [ ] 运行 `python -m pytest -q tests/contracts/test_unified_test_plan.py tests/contracts/test_command_registry_consistency.py`；输出必须列真实收集数，无网络。

### T02：补充独立样本与差分基线

**文件：** 新增 `tests/fixtures/sdk/` 的 JSON/二进制数据及 `tests/contracts/test_sdk_fixture_inventory.py`；复用 `scripts/fixtures/canonical.py`、`differential.py`；修改现有 fixture 索引时保留来源。

**产物：** `case_id + source_type + source_revision + request + response + expected + evidence_level`；来源类型区分 capture/synthetic/reference。

先把 [COMP 兼容案例](compatibility-cases.md) 中本域案例落到这里的 fixture 目录；每例同时保存新 Rust 与旧/新 Python 的预期，不能只比较两个都调用同一新 Rust 函数的输出。

- [x] 先检查现有 25 normal +12 extra 生成样本与提交样本的映射，列出 money_flow、非空 category22、EX/MAC/F10 的实际缺口；逐样本来源和 SHA-256 见 `tests/fixtures/sdk/inventory.json`。
- [x] 为批量重复/乱序/缺失、短页后还有数据、日期跨日、ETF/债券精度、无效压缩、ZIP/DAT、重复 F10 列和未知 MAC bit 建立具体样本，位于 `tests/fixtures/sdk/cases/`。
- [x] 期望值来源与独立性逐例记录；既有 Python 生成的期望只标为回归基线，不标为独立 oracle。
- [x] category22 没有非空实测响应时标为实验能力，不伪造“已实测”记录，也不阻止稳定 `0x0FFC` 日资金流和 F10 历史资金流路径。
- [ ] 运行 `python -m pytest -q tests/contracts/test_sdk_fixture_inventory.py tests/native/test_differential.py`；原生命令测试需要先按项目既有方式安装当前绑定。

实现与测试源码已完成；上述测试执行按用户要求推迟到 T01–T30 全部实现代码完成后统一进行。当前库存导出脚本只生成来源/哈希清单，不调用协议解析器。

### T03：冻结差异决策、单位和代码规范化

**文件：** 维护本目录 `public-api.md`、`capability-matrix.md`；新增 `tests/fixtures/sdk/policy/{price_limits,normalization,units,dates,legacy_boundaries}.json` 和 `tests/contracts/test_sdk_policy_fixtures.py`。

**产物：** 每项冲突有明确新 Rust 行为、Python 兼容方式、证据和变更条件。

- [x] 涨跌停规则逐一记录主板 ST、注册制 ST、北交所上市阶段、指数、退市整理等案例；当前没有有效日期的权威来源，故不批准任何争议 profile，Rust 返回 `UnknownRule`。
- [x] 有争议且无法证明的场景返回 `Unknown`/明确错误；不默认选一份现有实现当权威。
- [x] 将 today_batch 不支持、裸代码推断遗漏、不同名多 DAT 选择、按位置关联指数、吞错缓存空值分别建立回归样本。
- [x] 为每个已知缺陷注明“新 Rust 可修正”与“Python 仍兼容/另行批准修复”，不能因共享实现就自动改变 Python 行为。
- [x] 明确 Rust `SecurityId` 的严格构造与 Python lenient alias/默认值转换；定义价格/量/金额/比例和时区来源。
- [x] 冻结 Python 每请求 timeout、startup 计时起点、预取/分页/补查的独立尝试限制，以及 `max_pages=None`；新 Rust 总期限另立规则，不能混用预期。
- [x] 按 [决策清单](decision-register.md) 审核默认值和E类待证据项；尚无开始实现的业务域，T03 已列出无证据时的明确限制。
- [x] 审阅所有样本是否能由不看实现的人判断期望；完成后再进入依赖这些规则的任务。最终执行验证按用户要求延期。

## 3. P1：独立 SDK 基础与运行时契约

### T04：新增 crate、稳定类型与纯配置构造

**文件：** 新增 `crates/eltdx/Cargo.toml`、`src/lib.rs`、`client.rs`、`config.rs`、`types/{mod,security,time,numeric,raw,batch}.rs`、`error.rs`；修改根 `Cargo.toml`；新增 `crates/eltdx/tests/config.rs`。

**接口：** `TdxClient::builder() -> ClientBuilder`；`ClientBuilder::build() -> Result<TdxClient>`；公共类型按 API 文档；不立即导出未实现服务。

- [ ] 测试非法负数/零容量、host/port、互相矛盾的 pool 配置；验证失败不创建线程/网络。
- [ ] 测试默认 `build()`、clone、未启动 close 和第二次 close；不得依赖真实主机。
- [ ] 实现验证后私有字段、窄枚举和 `#[non_exhaustive]` 错误种类；不得暴露 `python_kind` 给新 SDK。
- [ ] 定义最低依赖和 feature 表；基础 crate 默认不拉 PyO3 或 HTTP。默认 HQ 所需内部 ZIP 解码在 T17 经验证后加入，不依赖公开 offline feature。
- [ ] 运行 `cargo test -p eltdx --test config --locked`、`cargo tree -p eltdx --edges normal,build --locked`。

最小测试形状（对应类型落地后执行）：

```rust
#[test]
fn build_and_close_without_network() -> eltdx::Result<()> {
    let client = eltdx::TdxClient::builder().build()?;
    let clone = client.clone();
    client.close()?;
    clone.close()?;
    Ok(())
}
```

### T05：修复底层 pin/pending 交接

**文件：** 修改 `crates/eltdx-runtime/src/engine.rs`、`pin.rs`、`supervisor.rs` 及所属 inline tests；新增 `crates/eltdx-runtime/tests/pin_handoff.rs`，必要时提取最小测试夹具。

**接口：** 现有公开 typed 方法保持；内部未交付 pin 必须退还；取消确认和正常完成共享唯一终态。

- [x] 用 barrier/channel 固定 pending drop 和“pin 已打开、结果尚未发送”的取消顺序；源码也覆盖失败结果发布前已排队的取消，公开回环以 channel 协调，不使用短 sleep。
- [x] 为“先成功后取消”“先取消后成功”“成功发送失败”“PendingPin 未 wait 就 drop”及失败结果交付竞态编写源码用例。
- [x] 源码断言成功 grant 只有一个所有者，释放后 active leases=0；取消/未交付后归零，后续 pin 可重新取得且重复 release 无效。
- [x] 为 pending request 丢弃添加非阻塞取消登记；源码覆盖取消确认超时触发现有 fail-closed 路径，并避免把成功结果误认为仍在执行。
- [ ] 运行 `cargo test -p eltdx-runtime pin --locked` 和完整 runtime 测试；评审不得破坏原 FIFO/fencing。

### T06：增加原始回复与绝对执行期限

**文件：** 修改 runtime `engine.rs`、`request.rs`、`deadline.rs`；新增/提取 `execution.rs`；新增 `crates/eltdx-runtime/tests/deadline.rs`。

**接口：** runtime `RawReply`、携带绝对 deadline 的提交 options、原始 pending/reply；现有 `execute(CommandRequest)` 继续兼容。

- [ ] 用可控 loopback 验证首次启动、排队、发送、重试共用同一 deadline；deadline 在开始前过期则不建连。
- [ ] 新路径不得提前调用旧全局精度 parser；原始回包包含请求/epoch/slot/generation/host上下文。
- [ ] 检查 raw reply 交付、丢弃和 parse error 后的预算释放，故障不得留存 lease。
- [ ] 旧 typed execute 使用共同原始执行机制后调用旧 parser，避免两套调度器。
- [ ] 运行 `cargo test -p eltdx-runtime --test deadline --locked` 和原有 close/retry/memory 回归。

### T07：SDK owner 与关闭状态机

**文件：** 新增 `crates/eltdx/src/session.rs`、`hq/executor.rs`；修改 `client.rs`；新增 `tests/lifecycle.rs`。

**接口：** 私有 `SessionOwner`；公开 `TdxClient::close(&self) -> Result<()>`，`PinnedSession` 不 Clone；所有子服务共享 owner。

- [ ] 先测 clone A close 后 clone B/子服务拒绝新操作；并发 close 只执行一次实际 shutdown。
- [ ] 测试丢弃 client 但仍持有服务句柄/订阅时 owner 按契约存活；最后 owner drop 触发停止，worker不反向保活。
- [ ] 正常Rust首次 close 创建 owner 级绝对 shutdown deadline；停止 admission、订阅、各专用池仅使用剩余预算，并发 closer 不重置期限。确认退出后才成功；错误时保留诊断和后续清理机会。Python兼容按原域顺序/独立预算触发分域gate，不提前全局封闭；COMP-013的阶段间并发与首错后可用域须先录制旧基线。
- [ ] 测试 close 期间新请求拒绝、进行中请求被唤醒、pin释放无死锁；多个繁忙池和并发 closer 的总耗时不能累加为每池完整 timeout。
- [ ] Python 兼容关闭单独冻结主 transport→资金流→竞价的旧顺序、各阶段期限和异常投影；私有 shutdown policy 复用清理状态机，不能给 Python 强套 Rust 总 1 秒。多池繁忙用例分别验证两种预算规则。
- [ ] `cargo test -p eltdx --test lifecycle --locked`；增加 Windows/Linux 的相同行为测试，勿用线程名计数代替资源观察。

### T08：操作上下文和错误边界

**文件：** 新增 `src/operation.rs`、`policy/retries.rs`；修改 `error.rs`、`hq/executor.rs`；新增 `tests/operation.rs`。

**接口：** `OperationOptions`、`CancellationToken` 公共；`OperationContext` 和执行 policy 私有；正常 Rust 网络方法最后参数 `&OperationOptions`。`RustPublicExecutionProfile` 使用总预算，`PythonLegacyExecutionProfile` 保留旧计时；两者共用业务循环。

- [ ] 在公开方法入口、输入验证之前记录 Instant，deadline 从该入口推导；有界验证、metadata、分页、fallback 共用预算，不重置期限。
- [ ] 测试第二页开始前超时、第二次拆批前取消、wire retry与host failover总次数达到上限、取消不影响其他操作。
- [ ] 错误记录 kind/phase/operation/command/host/attempt/source；不得把所有错误降为字符串。
- [ ] 将 close 与业务 token 分开；已经取消的 token 不能阻止释放。
- [ ] 通过 `#[doc(hidden)] pub mod __python_compat` 暴露窄绑定桥接（跨 crate 不能用 `pub(crate)`），内部创建 legacy policy；无 PyO3 依赖、不新增 feature，普通 Rust API 不提供 legacy 切换，此桥接不承诺 SDK 公开稳定性。
- [ ] 增加双 profile 对照：两页分别低于单请求上限、合计超过总上限时，Rust 超时，旧/新 Python 成功；预取、补查、空值 fallback 次数及无限页选项按各自契约验证。不得给 Python 暗加累计尝试上限。
- [ ] `cargo test -p eltdx --test operation --locked`。

T04–T12 的跨 crate 类型与执行顺序按 [内部实施规格](execution-and-compatibility.md) 实现；`RequestPermit` 将旧启动后计时与新绝对期限交给共同 runtime 执行，业务循环不自行重置 timeout。

### T09：无全局状态的解析上下文

**文件：** 修改 `crates/eltdx-protocol/src/{unit,response}.rs`、相关 `commands`；新增 `context.rs`；新增 `crates/eltdx/src/hq/decode.rs`、`tests/precision_isolation.rs`。

**接口：** protocol 定义 `PricePrecisionSource`/`ParseOptions`/`parse_with_context`；legacy `parse` 暂保留；新 SDK 只用显式路径。

- [ ] 同一证券在两个 resolver 提供不同精度，交错/并发解析仍分别得到正确结果。
- [ ] 新证券列表解析不写全局；旧入口兼容 wrapper 行为有单独测试。
- [ ] 精度依赖整次 operation 捕获的单份 snapshot，所有分页/拆批/fallback 使用同一版本，不在每条请求或记录中重新读可变共享表。
- [ ] `include_raw=false` 不创建永久 record hex；对保留字段进行语义和位级对照。
- [ ] `cargo test -p eltdx-protocol context --locked`、`cargo test -p eltdx --test precision_isolation --locked`。

### T10：元数据加载、单位投影和时钟

**文件：** 新增 `src/metadata/{mod,store,snapshot,precision}.rs`，`src/types`；新增 `tests/metadata.rs`、`tests/units.rs`。

**接口：** 私有 market snapshot store，public typed value 投影及来源信息；同 key single-flight，独立等待者取消。

- [ ] 测试市场完整分页后原子发布；中途失败不标记已完成，不清除旧好版本。
- [ ] 测试刷新期间已开始请求仍用旧snapshot；两个owner互不污染。
- [ ] 参数化股/手、万元/元、百分比/比值、f32位模式、债券精度、跨日时间；未知单位不猜。
- [ ] 注入时钟验证上海日期边界与 metadata 过期，不改系统时钟。
- [ ] `cargo test -p eltdx --test metadata --test units --locked`。

## 4. P2：HQ 完整业务及第一批 Python 迁移

本阶段的完整签名、query默认值、结果字段/单位、EOF及case映射以 [HQ逐接口规格](hq-api-spec.md) 为约束；[决策清单](decision-register.md) 固定拟议新Rust的扫描/批量/尝试上限。新类型进入实现时先补可编译定义，再让后续任务使用。

### T11：代码表、批量快照与缺失结果

**文件：** 新增 `src/services/{codes,quotes}.rs`、`types/batch.rs`；新增 `tests/{codes,quotes_batch}.rs`。

**接口：** `quotes().snapshots(&[SecurityId], &OperationOptions) -> Result<SnapshotBatch>`；按输入索引保留重复；缺失策略见 public API。

- [ ] 测试一次全返回、部分缺失、返回乱序、重复输入、未知响应代码、重复冲突响应。
- [ ] 按 T02 用例实现二分补查，最多32次额外补查并服从操作总预算；具体兼容阈值见矩阵。
- [ ] strict 在确认缺失时返回带原因的结构化错误；allow_partial 用槽位保留缺失原因。两种策略的损坏数据、取消和主请求连接失败都返回错误，不伪装为 missing。
- [ ] 证券全表遇短页继续、空页停止、重复页/无进展报错；不先依赖 count 才能翻页。
- [ ] `cargo test -p eltdx --test codes --test quotes_batch --locked`。

### T12：第一条跨语言业务链

**文件：** 修改 `crates/eltdx-python/Cargo.toml`、绑定 `lib/request/response/transport`，新增 `service.rs`；修改 `src/eltdx/api/quotes.py`、`codes.py`、`client.py`；新增 `tests/contracts/test_sdk_quote_bridge.py`。

**接口：** native service 经绑定兼容桥接调用共享 Rust，输出原 tuple/dataclass；Python 特有别名、异常映射与逐请求计时保留。

**实现状态（2026-11-16）：** quote/code bridge 源码和兼容边界审阅完成；测试源码已补齐，所有测试、构建和验证按用户要求延至 T01–T30 实现代码完成后统一执行。普通快照响应保留服务器顺序；恢复结果按 Python facade 的代码队列重组。

- [ ] 用同一 fake/loopback 数据比较旧 Python facade、Rust service 和新 Python facade，字段/顺序/异常/原值全部相符。
- [ ] GIL 释放、KeyboardInterrupt、`__cause__`、自定义 Transport仍被验证；回调不得在持GIL的阻塞I/O中运行。
- [ ] 将旧算法从 native 常规路径移除，兼容 custom transport 路径显式标注和覆盖。
- [ ] 运行 `python -m pytest -q tests/test_client.py tests/test_helpers.py tests/contracts/test_sdk_quote_bridge.py tests/contracts/test_native_errors_contract.py`。
- [ ] 运行 SDK quote测试，证明 Python绑定变化不改变Rust consumer API。

### T13：K线、退市K线与分钟线分页

**文件：** 新增 `src/services/{klines,minutes}.rs`，完善 `types/time.rs`；新增 `tests/{klines,minutes}.rs`。

**接口：** 稳定 query/options + page/complete结果；raw多页按页映射；周期与跨日按协议保留。

- [x] 已编写短页/空页/重复页、max_pages、无进展、start偏移及总deadline的测试源码；执行延期。
- [x] 已编写周/月/季/年、category8、HQ/EX/MAC时间差异HQ部分的测试源码；结束标签真实跨日并保存source_time。
- [x] 退市结果raw保留首页，普通聚合legacy raw清空；Rust逐页payload/wire保留响应顺序，逐条range对应输出顺序。
- [x] 无效日期、畸形计数、尾部数据按照协议parser契约处理；已编写边界测试源码，执行延期。
- [ ] `cargo test -p eltdx --test klines --test minutes --locked`。

T13实现源码与自审已写完，尚未编译或运行；待T01–T30全部实现代码完成后统一验证。M25–M40及M98–M99已接入公开Rust服务，Python迁移仍由T18完成。

### T14：逐笔、竞价、开盘撮合与金额流

**文件：** 新增 `src/services/{trades,auctions,money_flow}.rs`；新增 `tests/{trades,auctions,money_flow}.rs`。

**接口：** typed记录和批量列视图分别明确；开盘撮合按服务器时间/记录性质判定，日内资金流专用策略。

- [x] 已写逐笔全量逆转页序而保留页内序、混合记录/盘后成交源码及合成用例；执行延期。
- [x] 已写Rust today/history列批量及纯转换；Python native today_batch既有拒绝保持，三方兼容验证由T18/统一测试完成。
- [x] 已写竞价原序列/秒/f32原位保留及可追踪开盘撮合，不添加固定时间窗过滤；执行延期。
- [x] 已写代码3/整批min(n,8)×3≤24/操作额外预算的原子授权、独立空值策略及合成用例；执行延期。
- [x] 当前未暴露历史区间stub，P3 F10 T20实现后启用；category22仍为实验路径。
- [ ] `cargo test -p eltdx --test trades --test auctions --test money_flow --locked`。

T14全部13方法实现及自审源码已写完：M41–M52/M100。专属惰性池、owner物理连接上限、备用探测关闭和错误链一并实现；没有运行编译或测试。测试源码均为合成输入，完整0FFC真实抓包仍缺失。Python迁移及legacy计时/次数/raw投影由T18继续，不能以此任务状态声称两种语言已验证兼容。下一任务T15。

### T15：订阅、单消费者分发和 gap

**文件：** 新增 `src/subscription/{mod,dispatcher,cursor,lease}.rs`；修改 `services/quotes.rs`；新增 `tests/subscription.rs`。

**接口：** `subscribe(&ids, &SubscriptionOptions, &OperationOptions)`；`Subscription` 唯一所有者、typed `PushEvent`；`SubscriptionOptions` 设置长期队列容量、续订间隔和 gap 策略，建立订阅的总预算单独传入。

- [x] 已写metadata预取、静态容量预检、100代码分组/总期限获取及全部回滚源码；执行延期。
- [x] 已写第二组启动失败、动态占用及失败续订回收用例源码；整轮续订成功后才发布，执行延期。
- [x] 已写唯一dispatcher精确源路由、独立订阅与Python独占raw reader；保持原overflow计数/异常投影，执行延期。
- [x] 已写按接收顺序保留跨日游标回退和独立gap控制状态，以及相关源码用例；执行延期。
- [x] 已写close/Drop/父close/无owner环、慢消费者、溢出、源丢失及同源重同步成功/失败用例源码；执行延期。
- [ ] `cargo test -p eltdx --test subscription --locked`；真实1000证券在线用例单列opt-in，见验证文档。

T15实现及限定源码自审检查点为`e35b5f2`，M24此前检查点为`e995825`。这里只关闭源码实施，不表示编译或运行通过；没有执行测试、构建或等价检查。下一任务T16。

### T16：股本、复权、交易日和涨跌停策略

**文件：** 新增 `src/services/{corporate,limits}.rs`、`adjustment/{mod,factors,apply}.rs`、`calendar/{mod,service}.rs`、`policy/limits.rs`；新增 `tests/{adjustment,calendar,price_limits}.rs`。

- [x] 已写strict prefix续取、原声明/事件位保留、财务31字段/稀疏投影和共享32次恢复源码及用例；执行延期。
- [x] 已写scale+offset、同日原序、anchor/窗口/qfq/hfq及K线单位/来源保持源码与独立仿射预期；执行延期。
- [x] 已写指定指数完整日线、owner不可变缓存、覆盖区间、午夜/清除/失败发布和父close源码与用例；无weekday回退，执行延期。
- [x] 已写0452实际条数分页与空EOF、未知identity/f32位保留、有效日期profile应用及UnknownRule默认；六个T03冲突case源码保持未知，旧Python兼容映射由T18继续。
- [ ] `cargo test -p eltdx --test adjustment --test calendar --test price_limits --locked`。

T16源码检查点`6fa4bc7`/`84061e7`，另有`tests/corporate.rs`与私有用例源码。M60返回Result传播Clock错误的规格补正见decision-register；没有运行任何编译或测试。后续T17源码已在`88b9036`接续完成。

### T17：服务器文件、统计文件和公司资料

**文件：** 新增 `src/services/{resources,company_info,session,health}.rs`、`src/stats.rs`；新增 `tests/{resources,company_info,stats}.rs`。

- [ ] 实现统计资源前完成内部 ZIP 库选型：锁定 MSRV、feature、许可证；用重复条目、加密、压缩/展开大小限额、截断及 zip bomb 语料验收。默认 HQ 构建即支持该解码；决策记入实施时新建的 `docs/rust-sdk/dependency-decisions.md`。
- [x] 同一文件全程pin；offset/长度/host及物理连接身份校验源码与边界用例已写，整块结尾在预算允许时空块探测；执行延期。
- [x] 有界下载返回 `DownloadResult { data, completion, next_offset }`；到 max_bytes 且 EOF 未证实时为 `LimitReached`，只有 EOF 证据才为 `Complete`。严格方法、分配/来源限额、pin清理源码已写；执行延期。
- [x] 零/精确/超限、短/空块、offset及lost-pin不换源用例源码已写；stats上限加一且解压前拒绝。Python旧截取/逐请求/None行为由T18适配；执行延期。
- [x] 统计文件解析保留源身份、日期、计数字符串与单位，95%/主日期规则留给T18；默认HQ注册唯一ZIP及GBK源码。
- [x] 公司GBK字节范围/有损诊断、会话控制字段、ping/probe及公开客户端用例源码已写，执行延期。
- [ ] `cargo test -p eltdx --test resources --test company_info --test stats --locked`。
- [ ] 新增 `crates/eltdx/tests/hq_loopback.rs`，经公开Client/service完成至少一次非空查询、一次分页和一次故障关闭；运行 `cargo test -p eltdx --test hq_loopback --locked`，不能只测试内部Engine。

T17源码检查点`88b9036`；上述执行命令与ZIP的实际MSRV/许可证/lock/独立语料验收继续延期。已新增dependency-decisions.md记录选型的暂定状态，不能将Cargo声明当作已解析或已验证。下一任务T18。

### T18：P2 Python 回迁及可用版本验收

2026-11-17第二十四段：full_quotes/资金流helper已提交`2c6d9f4`。realtime_rank共用count/offset分页循环，Python保持短页EOF、超目标整页、每页名称补充、连续rank和custom count/数值hook；strict继续空页EOF与原限额。新增5合同/2Rust函数源码和排行source-only manifest，均未执行。其余helper/批量/订阅及consumer仍待完成，18/4/8不变。

2026-11-17第二十三段：special limits已提交`9d75306`；full_quotes的80分批与五档深度资格共用、Python原对象/回调合并及M95逐笔资金流档位累计已接入Rust。Python保留重复depth最后项、四字段替换、Exception降级与BaseException传播、自定义对象/原算术及逐请求时序。新增10合同函数源码和full_quotes source-only manifest，均未运行；其余helper/批量/订阅与consumer仍待做，T18部分、18/4/8不变。

源码首段`e3e768c`已写M95纯classify_fund_flow；第二段`d0f8c2c`增加M79–M85首批HelperService；第三段`14f590c`已写M86 profile/M87共享owner有界缓存/clear。第四段`ab5f8bf`增加M92 auction_data/M93 market_stat/M94 today_fund_flow。第五段`4823fb8`增加M88/M89及共享双stat对齐/缓存/parent collector。第六段`f6a99a8`增加M90短线/M91天梯及未执行用例，覆盖服务端就绪/源日期/单位/可选补扫/UnknownRule天梯诊断。第七段增加M96/M97板块、显式来源/固定HQ下载、内部文本/DBF解析、owner有界缓存、raw/display/excluded及8组公开用例源码。完整Python/native回迁、真实manifest迁移证据及consumer/example仍待完成，T18仍部分；已有M79–M97共19/19方法首批源码不等于完整T18或验证通过。全部执行验证延期，详情task-18-report。

**文件：** 新增 `src/helpers/{mod,market,boards,shortline,profile}.rs` 和 `tests/hq_helpers.rs`；对上述服务及 HQ helpers 逐一修改 `src/eltdx/api`、`helpers`、`equity.py`、`workday.py`、`stats.py` 和 native 桥接；新增域级差分测试与 `crates/eltdx/examples/hq_query.rs`。

2026-11-17补充源码检查点：板块阶段已提交`1bb3c1a`；hq_query例子及`tests/consumers/rust-sdk/`独立manifest/std-only假服务器/非空分页、重复输入、乱序行情、raw/source和关闭consumer源码已写。均未编译运行，workspace外复制及依赖图/feature/MSRV/package实际验收仍延期；T18的完整Python/native和installed-wheel consumer尚待完成。

2026-11-17第九段：例子/consumer已提交`39c8e3e`。普通/退市K线已新增共享bar_collection分页/排序、LegacyOperation.bars及native服务入口，Python常规标量路径接入；不加整段pin/目录预取/总页数上限，保留短页继续、普通反转页/清raw、退市稳定排序/首raw、原max_pages异常及对齐时机。合同/本机差分用例源码和manifest source-only证据已写，custom transport/自定义标量对象暂留adapter并记录退出条件。没有编译/collection/执行，不能把本次源码记录当作已验证迁移；剩余全部HQ域仍继续推进。

2026-11-17第十段：K线阶段已提交`b5ed0b5`；today/history/recent/aux/sparkline五类分钟线Python/native接入源码及10个合同、7个本机差分/集成函数源码已写。共用分钟请求/解码边界，保留原scalar/负volume、raw记录、resolver与原输入市场推断、每请求默认日期和批量workers；同时补上一段K线raw hex恢复。五项manifest仅增加source-only证据，不升迁移状态；仍未编译/collection/执行。继续其他HQ native域、全量manifest及installed-wheel consumer，T18仍部分。

2026-11-17第十一段：分钟线已提交`ac99b99`；session握手/心跳、公司目录/内容四入口Python/native源码及7个合同、4个本机差分集成函数源码已写。共用Rust域请求/解码，保留原raw/session快照/日期、显式市场、旧GBK及字节范围规则；本地ping保持不加载native的transport adapter。四项manifest仅增source-only证据，全部验证延期；T18仍部分，继续目录count/list/筛选统计和其余HQ域。

2026-11-17第十二段：session/company已提交`2cbcd53`。目录count/list与股票/A股/ETF/指数/ST的native纯筛选/统计源码已写，strict/legacy共用Rust边界与选择器；保留原行引用、市场顺序、Unicode ST、limit=0、旧日期/精度registry副作用及自定义属性/异常/求和时序。新增11个合同、7个本机差分集成函数及1个Rust用例源码；两项manifest仅增source-only证据，全部未执行。T18仍部分，继续其他quotes/latest_suspended、其余HQ域、manifest总核对与installed-wheel consumer。

2026-11-17第十三段：目录/筛选已提交`6fd1e05`。legacy/category/refresh行情、CodeApi.latest_suspended及标准subscription group的pin刷新native源码已写，共用Rust请求边界/停牌位规则，保留原resolver/registry/raw/负值/重复及无resolver不预取、先预取后pin顺序。新增9个合同/9个本机差分集成函数源码，三项manifest仅增source-only证据；全部未执行。T18仍部分，继续trades/auctions/dailyflow等其余HQ域、helper与剩余订阅编排、manifest/installed-wheel consumer。

2026-11-17第十四段：行情已提交`d3c1215`。today/history逐笔单页、全量/历史bulk、正式开盘撮合共用Rust分页/请求边界及Python/native接入源码已写。保留原逐请求期限/无整段pin、无metadata预取、重复/超count页、页内次序、首base及非finite/raw/日期、空页优先和原max_pages/offset错误。bulk保留分块及usize总量，不构造Python逐笔对象；today bulk仍拒绝，自定义参数/日期/分页覆盖继续逐页adapter。新增14合同/12本机集成/3Rust函数源码及四项source-only manifest，全部未执行；T18仍部分，继续auctions/dailyflow及剩余HQ/helper/订阅迁移、manifest和installed-wheel consumer。

2026-11-17第十五段：逐笔阶段已提交`6544772`。竞价共用Rust请求/响应边界及Python/native源码已写，保留专用pool的原延迟创建/弱回调/主机配置/关闭重建、execute扩展及legacy原time/count/raw/错误语义，不增加目录预取。新增5合同/4本机集成函数源码及1项source-only manifest，全部未执行；T18仍部分，继续dailyflow及其余HQ/native/helper/订阅/manifest/consumer。

2026-11-17第十六段：竞价已提交`fa7e07d`。daily money flow请求/备用源循环共用Rust源码和Python native接入已写，保留原循环预算3/24、skip扣额、cursor、selected/新源错误区别、factory/adopt/close、raw/unknown/multiple/nonfinite/date及自定义回调；不预取目录。新增12合同/6本机集成/3Rust函数及1项source-only manifest，均未执行。批量调度及F10 history仍是后续scope；T18仍部分，继续corporate/equity/calendar/resources/stats/health及其余helper/订阅/manifest/consumer。

2026-11-17第十七段：daily flow已提交`6b4fc0b`。capital/finance共用Rust请求边界和资本prefix cursor/财务二分树已写；Python保留每请求预算、32次恢复、原始响应序/raw、重复项恢复排序/缺失warning、旧字段别名/生成器/属性读取和custom execute。新增15合同/7本机集成/3Rust函数源码及两项source-only manifest，全未执行。外层资本chunk调度暂保留adapter；下一步equity/calendar/resources/stats/health和剩余T18 scope，任务计数18/4/8不变。

2026-11-17第十八段：corporate已提交`851774a`。equity前后复权共用Rust affine算法，保留稳定日期排序、同日原始顺序、offset及Python非finite/非正乘数错误；custom记录/标量/函数覆盖继续兼容adapter。新增9合同/3Rust函数源码及1项source-only manifest，均未执行。下一步calendar/resources/stats/health和剩余T18；总计18源码完整/4部分/8未开始。

2026-11-17第十九段：equity已提交`fc3088e`。calendar有序日期/范围/邻居/包含查询和Python weekday近似已接入共用Rust源码，保留基准加载/缓存/弱引用失效/边界异常及custom对象；新增9合同/2Rust函数源码，全未执行。日历按模块证据记录，不新增124 capability项。下一步resources/stats/health和剩余T18，总数18/4/8不变。

2026-11-17第二十段：calendar已提交`b7ae0d1`。资源单页/metadata及标准内置连接的下载共用Rust请求和分页cursor，保留原pin生命周期、cap0、EOF/cap截断、逐请求预算/raw/换源错误；custom入口保留adapter。新增8合同/2本机集成/1Rust函数及2项source-only manifest，全未执行；继续stats/health及剩余T18，总数18/4/8不变。

2026-11-17第二十一段：resources已提交`4b0d38b`。stats成员的GBK/文本/数值/行解码共用Rust实现，Python ZIP codec/异常cause、原空表/重复错误顺序和custom parser继续adapter。新增8合同/2Rust函数源码及1项source-only manifest，全未执行；继续special limits/health和剩余T18，总数18/4/8不变。

2026-11-17第二十二段：stats已提交`c2ef123`。special limits共用请求/offset扫描和native接入源码已写，保留raw/nonfinite/未知identity/超目标最终页和逐请求行为；6合同/4本机集成/2Rust函数源码未执行。health沿用已迁移heartbeat及zero-native-load本地ping adapter，并补用例。继续全部helpers/订阅/批量、manifest和installed-wheel consumer；18/4/8不变。

- [ ] 在默认 HQ feature 下实现矩阵中的 full_quotes、每日股本/涨跌停价、排行、board_quotes、limit_ladder、market_stat、shortline 和完整 stock_profile_table。现有 profile 仅依赖 HQ 的 full_quotes、证券目录和 finance_batch，不虚构 F10 依赖。
- [ ] 按代码关联指数、市场统计及各批量来源；短线验证服务器日期、09:25、双文件主日期、>=95% 覆盖和前交易日对齐门槛，不能按返回位置猜对象。
- [ ] 主数据链失败返回错误；full_quotes 刷新等已定义的可选补充源失败保留主数据并附诊断，新 Rust 取消/总 deadline 则终止操作。Python 逐请求超时、原可选请求超时降级条件与结果模型按旧样本验收。
- [ ] 运行 `cargo test -p eltdx --no-default-features --test hq_helpers --locked`，证明默认 HQ helpers 不暗含 f10/offline；另测可选功能关闭下的公开 API 编译边界。
- [ ] 按域替换算法，不一次删除所有Python实现；更新迁移清单的fixture和实际状态。
- [ ] 保留close/reconnect generation、custom transport、缓存清理、默认参数与异常因果链。
- [ ] 执行相关Python业务测试及native/contract/loopback套件；不得自动包含未声明在线测试。
- [ ] 构建无Python外部consumer运行本地loopback请求、分页、close；记录实际结果。
- [ ] 发布说明只承诺HQ范围；F10/EX/MAC/离线尚未完成时清楚列出。

## 5. P3：F10、离线与跨来源组合

### T19：HTTP 依赖验证与 F10 基础传输

**文件：** 新增 `src/f10/{mod,http,request,response}.rs`、测试 `tests/f10_http.rs`；修改crate feature/dependency；测试依赖评估记录放 `docs/rust-sdk/dependency-decisions.md`（实施时创建）。

- [ ] 使用本地HTTP/HTTPS/代理fixture验证Host/SNI/证书、CONNECT、环境代理、IPv4优先及IPv6回退。
- [ ] 超时和响应byte cap在读取过程中执行；无GIL/Python/DNS全局monkeypatch。
- [ ] 记录候选 HTTP 依赖的 MSRV、feature 树与许可证；固定通过者版本。复查 T17 的 ZIP 决策供 P3 财报复用，不重新引入另一套解码器。
- [ ] F10编码compact UTF-8 JSON，解码UTF-8-sig，服务器业务失败与HTTP失败分开。
- [ ] `cargo test -p eltdx --features f10 --test f10_http --locked`。

### T20：F10 动态结果、端点与历史资金流

**文件：** 新增 `src/f10/{client,fields,news}.rs`，完善response，新增 `tests/{f10_results,f10_routes}.rs`；启用money_flow历史区间。

- [ ] 多表、重复/缺失列、标量行、嵌套JSON保留ordered cells和raw。
- [ ] static/hot/page2/qhf9入口表、参数顺序/类型/默认值与原方法逐项对齐。
- [ ] 报告期→报表等依赖调用共享总deadline；资讯读取实际第二结果集。
- [ ] 历史资金流指定days、反转日期升序并按start/count切片；不得改走category22。
- [ ] 为约定宽松输入保留Python映射；严格Rust输入另测，不顺便修正BRK.B等旧行为。
- [ ] `cargo test -p eltdx --features f10 --test f10_results --test f10_routes --locked`。

### T21：本地行情、股本与行业配置读写

**文件：** 新增 `src/offline/{mod,paths,alignment,bars,block,industry,gbbq}.rs`，测试 `tests/{offline_bars,offline_blocks,offline_industry,gbbq}.rs`。

- [ ] day/lc1/lc5/.5/ex-day逐字节往返，HK amount同槽u32/f32含义不丢失。
- [ ] 读取残尾保留完整记录；append残尾拒绝且文件内容完全不变；显式replace才允许替换。
- [ ] Python `write_*` 保持追加含义：只与调用前磁盘末条时间比较，输入批内不排序/去重；空输入也先拒绝残尾。测试逆序输入、重复时间、重叠区间和原文件字节不变。
- [ ] daily 原有 fsync 与 minute/EX 的耐久性差异分别覆盖；Rust 显式 durability 策略与 Python 默认映射固定，不能笼统声称所有写入都已持久化。
- [ ] 日期编码、股/手系数、债券系数、ties-to-even分别参数化。
- [ ] 路径发现不写磁盘；板块配置拒绝目录逃逸；GBBQ运算采用wrapping u32。
- [ ] 行业 `parse_tdxhy_cfg/read_industry_cfg` 覆盖 GBK 替换解码、少列/非6字符代码跳过、6字符非数字仍保留、同码后值覆盖、未知行业保留及空文件；字节与文件入口一致。
- [ ] `cargo test -p eltdx --features offline --test offline_bars --test offline_blocks --test offline_industry --test gbbq --locked`。

### T22：专业财报、ZIP 与 DAT

**文件：** 新增 `src/offline/financial.rs`、`src/financial.rs`；新增 `tests/financial.rs`。

- [ ] ZIP条目/压缩体/展开体限额在分配前生效，拒绝加密/同名重复；多不同名DAT按T03选择规则。
- [ ] DAT report_size必须为4倍数；字段/总float限额、索引越界/重复/重叠、空和短文件分别验证。
- [ ] 专业财报下载走HQ资源接口而非HTTP F10，max_bytes下传到下载源。
- [ ] hash字段只保留时不宣称已验完整性；如新增hash验证，规定算法和失配错误。
- [ ] `cargo test -p eltdx --features offline --test financial --locked`。

### T23：F10/文件组合 helpers 与 Python 回迁

**文件：** 在 T18 已有 helpers 中加入 F10/文件相关部分；逐项修改 Python 对应 helper/F10/offline/financial 桥接；新增 `tests/helpers.rs`。

- [ ] 完成 index_members/index_etfs/ipo_calendar、board_members、stock_topics/topic_stocks、theme_strength_rank 及财报等实际依赖 F10/文件的能力；完整 stock_profile_table 与其他 HQ-only helper 保持 T18 的默认 feature 可用性。
- [ ] 每个跨来源 helper 固定必需/可选来源及降级条件；新 Rust 的主源失败/取消/总 deadline 终止，Python 按 legacy policy 保留原逐请求计时与降级，不能强套总期限。
- [ ] 组合helper按实际feature启用，缺feature在编译时没有该方法，不在运行时跨Python补齐。
- [ ] 运行 `cargo test -p eltdx --features f10,offline --test helpers --locked` 及Python F10/offline/financial/helper相关套件；同步迁移清单。

## 6. P4：扩展市场

P3/P4 的 F10、EX、MAC、MAC-EX、offline 和财报接口由 [扩展逐接口规格](extension-api-spec.md) 规定；功能映射到T19–T27，不能用通用动态接口替代所有承诺的便捷方法而宣称完整覆盖。

### T24：EX profile、帧与命令族

**文件：** 新增 `src/ex/{mod,session,frame,commands,models,hosts}.rs`，命令增长时拆 `commands/`；新增 `tests/{ex_wire,ex_session}.rs`。

- [ ] 精确验证92字节setup、完整响应读取、有界解压、序号规则和GBK/UTF-8差异。
- [ ] 输入host优先级、非法配置报错、协议级探测和受限transport错误切换逐项对齐。
- [ ] 同一连接串行请求，response关联校验只采用有录包证据的规则。
- [ ] K线position/amount不同解释、原始record与动态quote缺口明确处理。
- [ ] `cargo test -p eltdx --features ex --test ex_wire --test ex_session --locked`。

### T25：MAC、MAC-EX 与未知字段

**文件：** 新增 `src/mac/{mod,session,bitmap,commands,models,routing}.rs`；新增 `tests/{mac_wire,mac_routing}.rs`。

- [ ] 三步MAC握手及无响应步骤、MAC-EX登录、head flag按profile区别。
- [ ] 0x122B/0x122C不同bitmap布局；未知合法位消费4B并保留，reserved151..159不消费。
- [ ] bit别名按上下文投影，原始bit id保留；MAC K线秒级时间不降到分钟。
- [ ] 香港股票逐笔EX路径、其他市场MAC路径、价格除1000和性质转换有独立样本。
- [ ] `cargo test -p eltdx --features mac --test mac_wire --test mac_routing --locked`。

### T26：扩展组合服务与目录缓存

**文件：** 新增 `src/mac/services.rs`、`src/ex/catalog.rs`；新增 `tests/extended_services.rs`。

- [ ] goods list空值fallback、按市场区间定位、宏观目录唯一匹配和刷新语义。
- [ ] 板块金额/净流入排名先聚合再topN；N日排行受总预算约束。
- [ ] MAC复权异常检测→原始K线→有效除权事件修复流程保留来源，不算transport重试。
- [ ] 未实现排序参数不能假称支持；返回功能状态或在稳定API中拒绝。
- [ ] `cargo test -p eltdx --features mac --test extended_services --locked`。
- [ ] 新增 `crates/eltdx/tests/extended_loopback.rs`，通过公开EX/MAC客户端覆盖各profile握手、半包、切换和释放；运行 `cargo test -p eltdx --features ex,mac --test extended_loopback --locked`。

### T27：扩展 Python 迁移与全能力核对

**文件：** 修改 `src/eltdx/ex`、`mac`、native桥接及迁移清单；新增相应差分测试。

- [ ] EX/MAC/MAC-EX每个公开方法对照能力矩阵，并保留模型字段、raw和异常。
- [ ] 独立客户端的host配置与关闭语义保留；不强行塞进HQ客户端池。
- [ ] 运行Rust full suite与Python扩展相关测试；线上验证单列来源和运行日期。
- [ ] 清单中每项为已迁移/有意保留适配/实验/不支持之一，没有模糊“基本完成”。

## 7. P5：独立消费、发布和文档

### T28：消费者与 feature/平台矩阵

**文件：** 新增 `crates/eltdx/examples/`、CI Rust job、独立consumer fixture；`verification.md`包含具体执行模板。

- [ ] 无Python环境先编译，再运行build/close及loopback真实请求；不是只导入crate。
- [ ] default、no-default-features、单feature、代表组合、full均构建；mac蕴含ex。
- [ ] MSRV1.89与current stable，Windows/Linux/macOS运行有限离线用例。
- [ ] cargo tree确认无PyO3/Python discovery；新例子通过doctest或编译检查。
- [ ] 对称验证普通 Python 安装：在未提供 rustc/cargo/maturin 的干净环境，仅用 `pip install --only-binary=:all: --no-index --find-links <wheelhouse> eltdx` 安装已构建 wheel，并执行公开 API 非空 loopback 查询及 close；没有匹配 wheel 必须失败，不允许回退 sdist。
- [ ] 运行完整协议/运行时/SDK测试、fmt、clippy；在线和压力单列，失败不可改阈值后不说明。

### T29：package 与版本矩阵

**文件：** 修改三个Rust库Cargo元信息/内部path+version、bindings依赖；新增发行检查脚本及package consumer配置。

- [ ] 决定新SDK首个0.x版本及可用crate名称，实际查询注册表后记录，不提前断言名称可用。
- [ ] protocol/runtime原3.x和SDK0.x依赖范围写明；Python包版本与绑定兼容矩阵明确。
- [ ] wheel 消费测试与 wheel 构建 job 分离，记录 wheel 文件名/平台/ABI/hash；安装及运行 job 均无 Rust 工具链。源码构建另列所需工具，不混作普通 Python 安装要求。
- [ ] 逐crate列包内容和license/readme；path-only依赖不得残留。
- [ ] 已发布下层可执行完整package/publish dry-run；未发布下层用本地registry或明确patch的内容验证，不能当作公开registry成功证据。
- [ ] 不实际publish/tag；产物hash、source SHA、工具链和features固定后才进入发布审批流程。

### T30：最终覆盖、性能和维护文档

**文件：** Rust rustdoc、示例、Python迁移说明、能力索引、发布说明、压力/性能证据索引。

- [ ] 核对R01–R12和能力矩阵逐项证据；实验category22等明确排除稳定承诺。
- [ ] 压力证据写入实际参数/计数/峰值/系统/SHA/日志hash，读原始日志复验；禁止只验证字段格式。
- [ ] 新增 `crates/eltdx/tests/stress.rs` 的显式ignored压力用例及证据导出，按验证规范执行；与默认快速离线CI分开。
- [ ] 性能对比固定相同工作量和样本，区别Rust核心、Python转换、外部网络耗时。
- [ ] 所有示例标记feature需求、错误处理、显式close、单位/时间语义；不把raw=false宣传为零拷贝。
- [ ] 同步docs mirror并检查链接、严格构建；保留失败和未验证项。

## 8. P6：独立后续项目

### A. 真正异步 API

前置：P5同步契约稳定、P1取消与交接用例通过。先写独立设计，规定Future拥有权、drop取消、awaitable completion、用户Tokio环境与后台runtime关系、阻塞DNS限制。不得直接在async fn中调用blocking execute，也不得把spawn_blocking取消当作底层请求取消。

验收必须包括：Future poll不阻塞executor，drop后pin/request回收，close与await竞态，取消确认失败路径，多个runtime/嵌套调用，以及与同步API共享同一业务实现。通过后才增加async feature。

### B. Rust CLI / HTTP / MCP

这些是共享SDK的消费者。各自确定输出兼容和服务生命周期，不进入基础crate依赖。Rust CLI若只是用户需要一个可执行入口可先做HQ查询/文件读取；HTTP/MCP继续复用现有Python适配器也不影响“纯Rust SDK”目标达成。

## 9. 并行实施与冲突控制

| 工作线 | 可以独立推进 | 必须等待 |
| --- | --- | --- |
| runtime | T05–T06测试和底层修复 | T03期限/关闭契约 |
| SDK基础 | T04/T07–T08 | T05–T06接口稳定后联调 |
| metadata/parser | T09–T10 | T03精度/单位契约 |
| HQ业务 | T11–T17各服务域 | T04–T10共用类型与上下文 |
| F10/离线 | T19–T23 | 公共错误/类型及资源下载 |
| EX/MAC | T24–T26 | 公共错误/单位/owner；MAC-EX等待EX |
| Python桥接 | 按完成域推进T12/T18/T23/T27 | 对应Rust服务通过差分 |
| 发布 | 文档和consumer夹具可提前写 | 最终执行等待稳定候选 |

修改公共类型、feature表、OperationContext、SessionOwner必须先向其他工作线通知；不让多个任务同时编辑同一未稳定的接口。边界新增需求先回到契约，不能把局部临时修补扩散为新默认行为。

## 10. 阶段完成标准

P1结束：无网络build、可控close、pin/pending交接、总deadline和metadata隔离都有行为证据。P2结束：矩阵中全部 HQ 域及仅依赖 HQ 的 helpers 由 Rust 独立运行，对应 Python 回迁及兼容测试完成；缺失项要显式缩小版本范围，不能只凭 quotes 通路宣布全 HQ 完成。P3/P4结束：矩阵对应域无遗漏，未知/实验项有明确标记。P5结束：两个方向的独立安装消费者、包、feature/平台矩阵、文档与产物来源可以复验。

任一阶段不得用“代码量搬完”“公开方法存在”“测试名包含功能”作为完成依据。相应验收失败则保留阶段未完成状态，修复或缩小明确声明的交付范围。
