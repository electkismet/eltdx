# Rust SDK 实施进度

> 本文件是跨会话恢复点。每个阶段和提交后更新状态、当前任务、下一步及未执行验证；重开任务时先读本文件，再读对应实现计划和分域规格。

## 当前状态

2026-11-16 最新恢复点：在27dbe91后新增T18 M88每日股本/M89每日限价源码及未运行用例；当前已有M79–M89/M92–M95共15/19 helper方法首批源码。每日股本分开finance总股/流通与stat自由流通、保留服务端目标日和来源日；双统计主日各覆盖>=95%且同日并限target/previous。限价取最近前序不复权日bar，原序应用同日tag1，未知规则仍UnknownRule且不产涨跌停价，缺参考另标MissingReference。资源/日历/K线/资本内部collector共享外层context，不重设预算。T18仍部分，短线/天梯/板块及完整Python回迁待做。当前18项源码完整、4项部分、8项未开始；没有运行任何测试、构建或执行检查。

当前实际进度：T01–T17及T21实现源码已写完；T18/T22/T24/T25为部分实现（18完整源码/4部分/8未开始）。恢复先读task-18-report第五段和brief及HQ规格11–14节，从M90/M91/M96/M97继续。M79–M89及M92–M95已有首批源码（15/19 HQ helper方法），不重复实现；Python/native完整回迁与consumer仍待做。目标保持active且不设置额度限制；全部执行验证延期。

2026-11-16 第二轮 T12 检查点已提交 `1e25507`：新增 NativeEngine loopback 成功链、协议异常 `__cause__`、KeyboardInterrupt 清理的集成测试源码；`from_sdk` 保持旧 Python native 异常无额外 cause；Rust 与 Python legacy directory 共用分页循环但分开应用严格响应校验，保留 Python 旧行集语义。实现和测试源码均未运行；T12 尚未完成。

2026-11-16 第三轮 T12 兼容审阅：普通、数量正确的首个快照响应保留服务器行序；发生恢复时，按 Python facade 的代码队列重组，仅把明确空单项列为 missing，并忽略恢复响应中未匹配的行。resolver 的结果投影先于 MissingCodesError/skip warning。新增相应合同及 loopback 测试源码；未执行测试、构建或验证。

| 项目 | 值 |
|---|---|
| 实施分支 | `codex/rust-sdk-implementation` |
| 起始基线 | `af471f1adf38757f6aa0188968db8110eb23f6ad` |
| 规格检查点 | `e36c71e` (`docs(rust-sdk): checkpoint implementation specification`) |
| 最新检查点 | T18 M92–M94源码/用例及恢复文档；profile/cache`14f590c`，前序`d0f8c2c`为M79–M85，M95首段e3e768c；全部执行验证延期 |
| 当前阶段 | P2：T01–T17实现源码完成；T18部分，M79–M89/M92–M95首批源码已写。 |
| 已提交进度 | T01–T10 实现与静态审阅完成；T11 检查点 `6f5fadb`、`de9e74f`、`a38dc32`、`ddb5e89`、`73c9365`、`f6ff2b3`、`c0f005d`；T17 `cd6735b`/`bcd6a8f`；T21 `b6e4beb`；T22 `9b6c8fd`/`a64a4a6`；T24 `1220bf2`/`04f613a`；T25 `c87e5b0`/`78bf121`；全部验证延期 |
| 测试约束 | 不运行 pytest 或 collection；不运行 cargo test/build/check/clippy/doctest、网络验证、AST 编译脚本或 `git diff --check` |
| 下一步 | T18：M90/M91/M96/M97，短线/天梯/板块、Python/native逐域回迁及consumer源码；已有实现待最终统一验证 |

## 阶段进度

| 阶段 | 任务 | 实现状态 | 测试状态 |
|---|---|---|---|
| P0 | T01–T03基线、样本、兼容决策 | T01–T03实现与契约源码完成，最终验证延期；涨跌停规则profile因权威证据缺失保持Unknown | 按用户要求暂不运行 |
| P1 | T04–T10 SDK基础、runtime、metadata | T04–T09 静态审阅完成；T10 Stage B 与 fix1 已通过限定静态复审，最终验证延期 | 按用户要求暂不运行 |
| P2 | T11–T18 HQ与Python首批迁移 | T11–T17实现与限定源码审阅完成；T18 M79–M89/M92–M95源码已写，其余helpers/native回迁待做 | 按用户要求暂不运行 |
| P3 | T19–T23 F10、offline、financial | T21 静态审阅批准；T22 Unicode修复 `a64a4a6` 通过限定复审，ZIP/资源集成待做 | 按用户要求暂不运行 |
| P4 | T24–T27 EX、MAC、Python迁移 | T24 Stage A/fix 已批；T25 pure codec/model Stage A `c87e5b0` 已写，完整服务与响应范围待 Stage B | 按用户要求暂不运行 |
| P5 | T28–T30消费者、打包、完整交付文档 | 未开始 | 待所有实现代码完成后统一执行 |

## 依赖与冲突预审

| 任务/工作线 | 共用文件或接口 | 预审结论与处理 |
|---|---|---|
| T01 ↔ T02 | 迁移manifest、fixture目录、registry覆盖 | T01先冻结ID/schema及源映射；T02只使用该manifest，不重复定义registry来源。 |
| T04 ↔ T07 ↔ T08 | `TdxClient`、ClientConfig、SessionOwner、Error、OperationOptions | 高耦合；公共类型和生命周期由同一主线先实现，业务域不可自造平行类型。 |
| T05 ↔ T06 | runtime engine/request/deadline、pending/pin交接 | 高耦合；pin终态/所有权优先，后新增deadline raw-reply路径；共享代码不得复制Engine执行循环。 |
| T09 ↔ T10 | protocol parser上下文、metadata snapshot、数值投影 | protocol层只定义语言中立解析trait/value；SDK metadata owner与缓存保持在eltdx。 |
| T11 ↔ T18 | batch/query/service、Python HQ facade | T11先定批量和缺失契约；T18按域替换facade，不提前删除未迁移实现。 |
| T12 ↔ T18 ↔ T23 ↔ T27 | native service bridge、Python公开入口 | 每阶段只迁移有Rust域实现及兼容范围的入口；保留既有外观，custom transport单独适配。 |
| T13 ↔ T14 ↔ T15 | 时间、owner、metadata、quote pin与push | 共享类型先冻结；订阅独立dispatcher唯一消费push，不能与旧poll路径双读。 |
| T16 ↔ T18 | corporate/limits/adjustment/calendar/helpers | 未取得涨跌停权威依据的路径返回Unknown；不从旧冲突实现猜定。 |
| T17 ↔ T22 ↔ T23 | HQ resources、offline DAT/ZIP、财报下载 | T17先定完整下载契约和zip实现；T22复用同一decoder；Python直接DAT旧上限保持。 |
| T19 ↔ T20 | F10 HTTP、JSON、query/response types | T19冻结传输层与byte cap；T20只实现入口/解析，不再引第二HTTP栈。 |
| T24 ↔ T26 ↔ T27 | EX session/commands/catalog/Python bridge | T24先提供有界profile；目录/组合服务等session稳定后实现。 |
| T25 ↔ T26 ↔ T27 | MAC bitmap/routing/services/Python bridge | 位图与profile为唯一解析来源；MAC-EX复用MAC模型并独立登录路由。 |
| T28 ↔ T29 ↔ T30 | features/package/consumer/evidence/docs | T28确定consumer矩阵，T29定package metadata，T30汇总实际证据；用户要求的测试延迟覆盖整个实施期。 |

**实施裁定：** 用户要求在全部实现代码完成前不运行测试；测试代码仍按计划逐任务写好，延迟到末尾统一运行。现有规格中的P0样例和默认值为新API设计输入，不表示已有生产协议已验证。联网事实不确定的域实现显式Unknown/Unsupported，不以猜测解锁。用户未授权发布、推tag或合并。

2026-11-16 恢复记录：目标模式保持 active，没有 token 额度限制。T07 已提交 `26488cf`（owner 生命周期）、`abb4285`（操作取消、pin owner 和真实 runtime 并发关闭源码）、`ca372d2`（取消 Condvar 丢失唤醒修复）。限定复审确认操作唤醒与真实 runtime 用例已处理，仍需解决 pin 无界关闭等待与 Drop 提前解除 owner 注册；第二轮修复只沿用现有 owner 列表和 runtime pending close，不增加并行资源状态机。测试/构建/检查均未执行。

第二轮 pin 修复已提交 `0656e63`：新增有界 runtime close 等待，SDK helper 同时遵守固定本地 cap 和 owner 当前期限；owner 进入关闭时唤醒等待者，Drop 不再注销，显式释放成功才注销，保留 runtime source 与结构化上下文。限定复审尚未返回，不能声称 T07 完成。T08/T09 实施者目前仅做源码阅读；T08 尝试授权会放在 worker 真正连接/发送前，T09 解析上下文会贯穿全部嵌套报价/分钟分支，并保存除法前整数。全部实现完成前仍不运行验证。

第二轮复审已返回：以上两个原问题均已处理，但 pin 超时后 Engine 退出会使下一次 pin receipt 不可创建，导致清理状态无法确认。第三轮通过精确 Engine 归属与已确认的 Engine 退出结果核销其 pin，继续返回原失败；资源已回收的事实与错误根因分开记录。未增加第二套 pin 状态机，全部验证仍延期。

第三轮提交 `98e8558` 已获限定复审批准：Engine 确认退出后可以核销其保留 pin，跨 Engine 的同编号 reservation 不会互相删除，原失败原因继续向外返回。T07 实现与静态审阅完成。用户再次要求继续目标模式，当前目标保持 active 且没有额度限制；T08/T09 已获 GO，在分开的文件范围内实施。T10 brief 已保存，T11 及后续代码尚未实施。不把静态审阅当作编译或运行成功证据。

2026-11-16 恢复记录：目标保持 active，`remainingTokens=null`。最新提交 `a64a4a6`。T10 fix round1 `151489f` 已通过限定静态复审，四项原始 Important 均已修复；controller 检查确认 `execute_raw_inner` 在 raw 请求提交前以正常 owner 注册启动 cleanup controller。T11 是下一任务。T22 Unicode 日期数字修复已通过限定复审，但 ZIP/archive 与服务注册未完成。T25 pure codec/model Stage A 已提交 `c87e5b0`、报告 `78bf121`；未注册，部分响应 schema、owner/session/transport 与 T26/T27 组合仍待实现。无测试、构建、check、网络、编译/AST、格式化或 diff 检查执行。

## 恢复规则

最新恢复点（2026-11-16）：goal模式active，`remainingTokens=null`。T17源码检查点88b9036，恢复先读task-17-report最新章节及task-18-brief，进入T18。T13 source_time及raw/日期/cursor约束继续有效；T18必须保留普通/退市raw、HQ同日回绕、native今日逐笔列拒绝和Python原计时/次数，并继续使用T15受控raw reader维持旧推送投影。财务/复权/weekday/特殊限价及资源下载的旧Python投影仍由T18实现，不能直接套新Rust默认或新增来源保护限额。ZIP依赖尚未解析/锁定/验证。没有运行测试、collection、构建、检查、格式化、diff检查或网络验证。

T21 于 `9734db3` 完成35个非财报本地 API、模型/格式算法、GBK 静态查表数据及四组测试源码；所有验证延期。仅运行了经授权的一次 Python 3.12.14 标准库 GBK 映射数据生成，脚本、版本和来源已在实现报告记录。任务审阅包以 `230e566` 为基线，仅含 T21 自有文件，scoped source review 进行中。T10 的 `Diagnostic`/`EnrichmentStatus` 共享实现仍待完成；这不是 T21 已验证集成的证据。

T21 第一轮静态审阅指出 `read_custom` 将空 leaf 当作可跳过数据，违背 brief 的路径名边界；另一个 Minor 是用例同样期望跳过。修复限定为拒绝空 leaf 并保留空 block 名称/missing `.blk` 的独立兼容跳过。原实施者正在从 `9734db3` 进行 fix round 1；之后需对修复 diff 做 scoped re-review。没有运行测试或验证。

1. 以当前实施分支最新提交为准；不切回或直接提交 `main`。
2. 先读本文件、`development-plan.md` 对应任务、相关API规格及 `git status`；保留未提交的用户工作。
3. 只完成计划中有源文件、兼容边界和验收条目的任务；不声称未编译代码可用。
4. 实现可按阶段提交。提交消息用 `feat(rust-sdk): ...`、`fix(runtime): ...` 或 `docs(rust-sdk): ...`，提交后记录短SHA和变动任务。
5. T01–T30所有实现、绑定、打包及示例代码完成后，更新状态为“待统一测试”；再一次性运行计划要求的全套验证，修复后重跑受影响集合，并记录命令、计数、平台与结果。
6. 不实际发布crate、wheel、tag或合并分支；发布仍需独立授权。

## 提交记录

| Commit | 任务 | 内容 |
|---|---|---|
| `e36c71e` | 规格检查点 | 已审阅的架构、接口、计划与兼容规格，以及初始进度文件 |
| `b6c2ea6` | T01 首轮实现 | 修正验证入口并新增迁移清单与基线契约 |
| `5a096aa` | T01 修复 round 1 | 补齐命令注册、源码哈希、pytest 节点与目录映射证据 |
| `c0e2c1c` | T01 修复 round 1 | 区分协议支持与 SDK 迁移状态并增加运行期反射快照 |
| `02de834` | T01 修复 round 2 | 加入反射细节、状态词表与 Python 模块覆盖清单；复审仍有迁移证据和覆盖扩展问题 |
| `edacc482` | T01 修复 round 3 | 加入独立 SDK 证据、可扩展覆盖来源及路径归一化；等待限定复审 |
| `8cc1a0b` | T01 修复 round 4 | 第三轮未通过；增加 crate 公共符号、精确 pytest 节点、三类模块覆盖来源绑定及正反契约源码；所有验证仍按要求禁止 |
| `0178eae` | T01 进度 round 4 | 记录第四轮已提交、复审等待状态和验证禁令 |
| `2eb0618` | T01 修复 round 5 | 收紧 crate 根公开证据、默认 pytest 文件/节点规则、目标测试导入范围及迁移状态关联；未运行任何验证 |
| `6f7cd82` | T01 修复 round 5 | 排除嵌套测试类、构造器赋值、fixture 与禁用节点；补充对应反例契约源码；未运行任何验证 |
| `7026d5d` | T01 进度 round 5 | 记录三项复审发现的静态闭合判断、验证延期及进入 T02 的恢复点 |
| `ea4721c` | T02 样本与证据库存 | 逐例记录 37 个既有 7709 样本来源/哈希、7 个补充案例、期望值证据等级和真实抓包缺口；最终验证延期 |
| `8d11baa` | T03 策略契约 | 冻结涨跌停冲突、身份/精度预取、字段级单位和日期；添加跨域兼容边界样本与契约测试源码；未运行测试、构建、check或diff whitespace检查 |
| `1777e4f` | T04 crate foundation | 新建纯Rust `eltdx` facade、配置/错误/基础类型与惰性 builder；只添加测试源码 |
| `4cb8511` | T04 review fix 1 | 移除受校验类型的直接反序列化、增加时间/价格 getter 与 loopback side-effect 源测试 |
| `eb0935f` | T04 review checkpoint | 记录裸 `127` 歧义裁定、复审 finding 与 no-startup 证据裁定 |
| `4b6a524` | T04 review fix 2 | 修正大整数价格舍入经 `f64` 精度丢失，添加大值/边界/极端 scale 源测试 |
| `c1b8c34` | T04 completion checkpoint | 记录两轮 scoped review 关闭与待最终复审的 `i64::MIN` 覆盖观察 |
| `4bf47d7` | T06 runtime raw/deadline | 增加 RawReply、共同 typed/raw 执行路径、绝对期限传递及 pin/connect deadline；首轮 scoped review 的共享 connect waiter 问题由 `561dcf5` 修复并通过限定复审 |
| `561dcf5` | T06 fix 1 | connect waiter 仅结束自己的等待；共享 attempt 保持运行并在终态后清理；限定复审通过；未运行验证 |
| `d4e3d37` | T06/T07 recovery checkpoint | T06完成，记录T07绝对deadline扩展与cleanup预算裁定 |
| `26488cf` | T07 owner lifecycle | 共享 owner、绝对关闭期限及资源清理责任 |
| `abb4285` / `ca372d2` | T07 fix round 1 | 操作取消/唤醒、pin owner、真实 runtime 并发关闭源码及 Condvar 同步 |
| `0656e63` | T07 fix round 2 | 有界 pin close、Drop 保留注册、原 runtime source 链 |
| `98e8558` | T07 fix round 3 | Engine 确认退出后的 pin 核销及跨 Engine reservation 身份隔离；限定复审批准 |
| `6f5fadb` | T11 checkpoint | QuoteService/CodeService 初始实现与进度恢复记录 |
| `de9e74f` | T11 checkpoint | 恢复路径、服务实现与严格身份处理 |
| `a38dc32` | T11 checkpoint | Python legacy metadata 预取顺序与初始快照请求兼容 |
| `ddb5e89` | T11 tests/checkpoint | 第二 chunk 取消、恢复超时、category 非有限必需值源码用例；同步状态文档 |
| `73c9365` | T11 fix | category continuation 无法表示下一个 offset 时返回 `RequestedWindow` 并省略 cursor |
| `f6ff2b3` | T11 tests | public CodeService 保留未知证券分类为 `Unknown` |
| `c0f005d` | T11 tests | 主快照连接失败仍返回 connection error，不降级成 missing |
| `684a157` | T12 compatibility fix | 保留 Python 快照服务器顺序与恢复语义；统一验证延期 |
| `338d99b` | T13 source implementation | K线/退市K线/分钟18方法，query/DTO、日期/精度/续读/raw映射和合成测试源码；尚未执行验证 |
| `992ba98` | T14 trades | 逐笔10方法、行/列/开盘撮合与源追踪；合成测试源码，执行延期 |
| `ce46437` | T14 completion source | 竞价/日资金流3方法、专属池、owner总连接限制、原子探测预算及合成测试源码；执行延期 |
| `9bbafde` | T15 partial foundation | options/events/gap/cursor/typed queue、runtime独占push consumer及合成测试源码；活动订阅/dispatcher/owner/renewal集成未完成 |

## 阻塞与风险

- 涨跌停权威规则、EX/MAC部分协议字段和真实服务器样本仍按设计文档中的证据缺口处理；未知值不能伪造成正确结果。
- 计划的 Python 业务迁移必须保留当前公开API并按域切换。尚未有native adapter入口前，不先移除Python实现。
- 全流程测试延期意味着各提交在最终测试前均未声明可编译或可运行；代码静态编辑不能作为通过证据。
- 第三轮复审未通过：迁移证据未解析到 crates/eltdx 公共实现，兼容证据未证明可由 pytest 精确收集，模块覆盖来源未建立逐类关联。第四轮只处理这些 T01 契约缺口和状态文档；没有运行任何测试/collection、Rust 验证、网络验证、AST 编译脚本或 diff whitespace 检查。
- 第四轮限定复审未通过（3项 Important）：Rust 符号搜索未排除私有父模块/注释字符串且解析路径未约束在 crate 源目录；pytest 文件规则漏 `.py` 后缀且漏 `__new__`；Python 导入遍历整文件，模块 coverage 未要求有效迁移状态。第5轮针对这些缺口补充静态契约；当前没有 crates/eltdx 实体或 migrated 记录，修复只定义未来证据格式，不提升任何迁移状态。
- 第五轮静态复核：三项发现已在契约实现中覆盖，T01 实现状态关闭；全部测试和构建仍待 T01–T30 实现代码完成后统一执行。没有 Rust SDK crate 或 migrated 记录，尚不宣称 SDK 能力已经实现。
- T02 库存确认既有 7709 样本中 36 个来自合成输入，`delisted_bars/normal` 引用了仓库中缺失的 UI 样例记录；37 个 `expected.json` 均为 Python 回归基线，不是独立 oracle。新增 7 个案例均为合成输入；完整 `0x0FFC` 抓包、非空 category22 和 EX/MAC/F10 实测响应仍是明确缺口。
- T03 未取得有日期的权威涨跌停规则来源，因此六个 Rust 涨跌停案例均保持 `UnknownRule`，而 Python 的 daily/shortline/MAC 三套投影分别保留。T03 policy fixture 是代码审阅/兼容及算术契约，不是监管规则、线上抓包或独立业务事实的证明；全部契约测试源码待 T01–T30 实现完成后统一执行。
- T04 两轮静态 scoped review 已关闭；`Price` 的 `i64::MIN` 缩放边界没有单独 authored case，最终全分支复审需再次检查。serde schema E06 仍未解决，身份/日期/时间仅保留 Serialize，未暴露不安全 Deserialize。

2026-11-16 M23恢复点：已写TdxClient::pin、借用PinnedQuoteService::refresh、同pin冷元数据加载、空wire轮询和失败激活失效处理；depth/refresh共用投影，元数据分页共用scanner。新增pool1冷元数据/空轮询/超100无额外命令/已关闭pin的合成用例源码，均未执行。T15仍部分完成，接续M24 dispatcher/lease/owner/renewal/resync/真实内存计费/Python受控fanout。任务完成计数仍为15完整源码、5部分、10未开始；没有测试或构建成功证据。

2026-11-16 第四段恢复提交：ab5f8bf（26文件），包含M92–M94及共享上下文/源日期/单位/EOF源码用例。前段profile/cache为14f590c。仍为18项源码完整、4部分、8未开始；goal active，无额度限制，尚无任何执行验证。继续M88–M91/M96–M97及完整Python/native回迁；shortline须额外满足server_datetime/09:25/双stat日期门槛，不能把auction日期fallback当ready证明。
