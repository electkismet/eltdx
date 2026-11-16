# T28 — 独立消费者、feature与平台CI源码

先读本文。权威要求为docs/rust-sdk/development-plan.md T28、verification.md §§3,7–10的完整模板、public-api.md feature/所有权/可见性和实际API。相关业务实现完成后controller GO。本任务编写CI、consumer、示例及验证脚本；T30代码完成前不得执行。

## 交付范围

- SDK examples、运行时复制到workspace外的独立Rust consumer fixture、Python wheel consumer附件、独立Rust CI jobs、平台矩阵和验证命令。保留旧Python兼容job；不能仅修改job名称或用含PyO3的--workspace构建证明独立性。
- Rust consumer只直接依赖eltdx，不显式补protocol/runtime/Python依赖。实际调用build、服务getter、close/重复close，以及至少一个非空HQ公开回环查询，核对请求、字段、单位和清理。扩展/F10 consumer按feature调用相应API。运行不安装Python/maturin、不访问外网；独立lock/target在末尾验证阶段生成，实际依赖图确认没有PyO3/Python discovery。
- 编写真实compile-fail/feature禁用/NonClone/private OperationContext用例，后续由编译器验证，不能用字符串搜索代替。default=[]、no-default、serde/offline/f10/ex/mac单feature、32个子集、代表组合和full按验证规范覆盖；mac蕴含ex，full精确等于五项合集。关闭feature后相关方法不可见；默认HQ的ZIP/stats/helpers仍可用。
- MSRV1.89/current stable；Windows/Linux/macOS的规定离线集合。Rust job显式选择三个Rust库，接入protocol/runtime/SDK测试、fmt、clippy、doctest、RUSTDOCFLAGS=-Dwarnings。目标名称必须真实存在，文档命令同步。
- 示例使用真实公共API、feature标签、错误处理、单位/时间/raw语义和显式close；不能全部ignore。展示Python与Rust各自安装和使用，无需同时安装两个产品。
- Python wheel构建job与消费job完全分离。消费环境只有受支持Python/pip/系统运行库，没有rustc/cargo/maturin；仅pip install --only-binary=:all: --no-index --find-links <wheelhouse>安装候选版本。缺少匹配wheel必须失败，不能回退sdist或安装Rust。
- wheel检查脚本/fixture复制到源码树外，python -I，不通过PYTHONPATH/src/tests导入工作区模块；核对实际import位于site-packages。使用公开客户端进行非空回环、close/重复close/兼容重开，确认连接释放。工具缺席在真实运行环境安装前后检查，不能只建venv隐藏PATH。分别记录wheel文件名/tag/hash、Python/OS/ABI、导入路径、请求/结果、连接/线程和日志。
- 每个平台如实记录运行证据；Linux不能代替Windows。CI输出命令、完整source SHA、lock/语料/工具链/features、日志与退出状态。未运行/跳过不能记passed；线上/压力单独opt-in，离线CI不隐含访问行情服务器。

## 工作约束与报告

工作目录C:/Users/ax/.codex/worktrees/rust-sdk-implementation/tdx抓包，分支codex/rust-sdk-implementation。T01–T30全部代码完成前禁止测试、collection、build/check/clippy/doctest、网络验证、编译/AST、格式/diff检查及等价验证，包括执行这些CI/consumer/generator脚本。允许源码/配置/文档编辑、普通review diff和精确Git恢复提交，禁用hooks。不得派代理、push/tag/publish/merge或编辑controller状态。

报告.superpowers/sdd/development-plan/task-28-report.md：job/矩阵/consumer/脚本映射、独立性如何落到实际环境、已写源码用例/文件/SHA、明确未运行的平台证据、T29/T30交接与自审风险。最终简短返回状态/SHA/未测试/风险/报告路径。
