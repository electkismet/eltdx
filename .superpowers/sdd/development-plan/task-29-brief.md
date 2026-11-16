# T29 — 包版本、发布内容与可复验消费脚本

先读本文。权威要求为docs/rust-sdk/development-plan.md T29、verification.md §§9–10.1、decision-register E05/E10、现有Cargo/pyproject/wheel CI。controller GO后编写版本/打包代码；实际registry查询、依赖解析、package/consumer执行在全部代码完成后进行。不得publish/tag/merge/push。

## 交付范围

- 三个Rust库的Cargo元信息、内部path+version与绑定依赖、包Readme/license/include/exclude、consumer配置、发行检查/证据脚本、wheel构建和消费矩阵、版本文档。SDK选独立首个0.x候选；protocol/runtime3.x、Python包3.x关系明确。实际查询前crate名称可用性必须标待确认。
- topology为protocol→runtime→SDK；所有生产内部依赖含path+version，不残留path-only。绑定兼容关系明确，SDK生产闭包无PyO3。每个包包含完整src/license/readme，无绝对路径或未打包兄弟目录依赖。源码归档/wheel绑定完整候选SHA、lock、feature、工具链和hash。
- 内容验证/解包consumer只引用实际解包文件。下层版本尚未发布时使用本地registry或仅对解包包施加显式patch；以后读取真实consumer lock证明patch被选中，不冒充公共registry成功。下层真实版本可解析后才执行正式publish dry-run。最终registry consumer移除全部path/patch/replace，使用SDK自己的0.x版本，不能误用workspace3.x。
- wheel构建可使用Rust/maturin；T28安装/运行job真实没有这些工具。候选wheel文件名、平台、ABI、hash、source/包版本关系被记录；无对应wheel失败，不回退源码。普通Python用户无需编译工具，源码构建要求单独列明。没有实跑的平台/MSRV不宣称支持通过。
- 编写注册表名称/版本只读查询脚本，末尾执行后保存日期、URL、响应。若名称冲突，以明确包名/lib名或registry决定解决，并保护use eltdx契约；不能编造名称空闲。lock只在全部代码完成后解析，当前如实标待解析。不得把cargo metadata、resolver或脚本执行冒充源码阅读。
- 编写必要的包内容、来源约束、证据解析源码用例，确保consumer不静默回退repo/git路径。后续执行package/list/publish--dry-run/registry lookup/解包consumer/wheel消费；缺工具/平台标未验证，不能填假成功。

## 工作约束与报告

工作目录C:/Users/ax/.codex/worktrees/rust-sdk-implementation/tdx抓包，分支codex/rust-sdk-implementation。全部T01–T30代码完成前禁止测试、collection、build/check/clippy/doctest、网络、编译/AST、格式/diff检查或等价验证。允许编辑源码、普通review diff、精确Git提交，禁用hooks。不得派代理、push/tag/publish/merge或编辑controller状态；协调Cargo/CI共享文件。

报告.superpowers/sdd/development-plan/task-29-report.md：精确版本/依赖/包/wheel矩阵和暂定状态、脚本与文件/SHA、最终验证命令和证据缺口、未执行与自审风险。最终简短返回状态/SHA/未测试/风险/报告路径。
