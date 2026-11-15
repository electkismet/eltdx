# Rust SDK 实施进度

> 本文件是跨会话恢复点。每个阶段和提交后更新状态、当前任务、下一步及未执行验证；重开任务时先读本文件，再读对应实现计划和分域规格。

## 当前状态

| 项目 | 值 |
|---|---|
| 实施分支 | `codex/rust-sdk-implementation` |
| 起始基线 | `af471f1adf38757f6aa0188968db8110eb23f6ad` |
| 规格检查点 | `e36c71e` (`docs(rust-sdk): checkpoint implementation specification`) |
| 当前阶段 | P0 / T01，修复第二轮复审发现并补齐迁移/反射覆盖契约 |
| 已提交进度 | T01 首轮实现已提交：`b6c2ea6`、`5a096aa`、`c0e2c1c`；第二轮复审尚有 Important 契约缺口，未进入 T02 |
| 测试约束 | 用户要求 T01–T30 实现完成前不运行任何测试；不运行pytest、cargo test、build/check、clippy、doctest或网络验证 |
| 下一步 | 完成反射字段对照、SDK迁移状态不变量及新增 Python 模块覆盖规则，做限定范围静态复审，通过后开始 T02 |

## 阶段进度

| 阶段 | 任务 | 实现状态 | 测试状态 |
|---|---|---|---|
| P0 | T01–T03基线、样本、兼容决策 | T01 修复中；T02–T03 未开始 | 按用户要求暂不运行 |
| P1 | T04–T10 SDK基础、runtime、metadata | 未开始 | 按用户要求暂不运行 |
| P2 | T11–T18 HQ与Python首批迁移 | 未开始 | 按用户要求暂不运行 |
| P3 | T19–T23 F10、offline、financial | 未开始 | 按用户要求暂不运行 |
| P4 | T24–T27 EX、MAC、Python迁移 | 未开始 | 按用户要求暂不运行 |
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

## 恢复规则

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
| 待提交 | T01 修复 round 2 进度 | 处理反射字段比较、迁移状态不变量和新增 Python 模块覆盖 |

## 阻塞与风险

- 涨跌停权威规则、EX/MAC部分协议字段和真实服务器样本仍按设计文档中的证据缺口处理；未知值不能伪造成正确结果。
- 计划的 Python 业务迁移必须保留当前公开API并按域切换。尚未有native adapter入口前，不先移除Python实现。
- 全流程测试延期意味着各提交在最终测试前均未声明可编译或可运行；代码静态编辑不能作为通过证据。
