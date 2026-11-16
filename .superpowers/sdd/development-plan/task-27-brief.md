# T27 — EX/MAC/MAC-EX Python 兼容迁移与全覆盖

先读本文。权威要求为 docs/rust-sdk/development-plan.md T27、extension-api-spec.md §§1,3–5,8、execution-and-compatibility.md，以及冻结的能力清单/API 快照和 Python ex/mac 源码。T24–T26 审阅批准后，由 controller 明确 GO。复用 T12/T18/T23 的 generation/native bridge 和同一份 Rust 业务逻辑。

## 完整范围

- 覆盖11个EX、23个MAC、13个MAC-EX业务方法、3个MAC别名，以及全部管理、配置、模块helper、模型、枚举和bitmap presets。实现native binding、Python兼容适配器、差分/契约测试源码与迁移清单。普通native路径不得保留第二份Python业务算法或绕回Python实现以宣称迁移。
- I/O释放GIL；结果转回原有Python DTO。信号取消必须确认资源清理，Python DTO分配失败不能泄漏资源；保留原异常类型、__cause__、自定义transport钩子。自定义对象仅通过明确适配面调用，不能被native路径静默绕过。
- EX/MAC/MAC-EX的owner、端点、认证、探测和缓存独立于HQ。Python close/disconnect/connect/reopen创建新generation；旧句柄失效。保留旧端点优先级、环境/文件错误、15秒超时、切换规则和host属性。不得给Python新增Rust的总8秒、32次extra、65535条输入、2MiB身份副本或分页保护上限。
- 保留旧默认值、参数类型、JSON/raw、模型属性、导入、别名与序列化。FieldBit别名和presets完整保留；bit93/94/136/139按上下文投影旧名称。新增native raw/未知字段不能自动变为Python dataclass新字段。
- 规格冻结的旧parser前缀返回、setup/login宽松行为、无效因子NaN、无事件QFQ、忽略sort、unusual count0、HK 50页返回已有前缀及无TTL宏观缓存，均通过legacy policy保留；严格Rust路径另行执行自己的契约。
- EX单页接口不偷偷收集全部；position/amount双义槽、native单位、GBK、日期/秒和历史逐笔布局与旧行为对应。MAC-EX香港股票走EX、价格除1000、nature/秒/trade_count原含义保留，其他期货仍走MAC。MAC-EX默认7字段与MAC Common不混用。
- 配置写入只发生在显式保存路径；共享owner/关闭原语，不复制清理状态机。仍需保留的自定义适配器逐项记录原因、用例和退出条件，不能丢弃公开入口。
- 核对全部清单与真实导出，采用T01规定的迁移状态词表：已迁移、有意保留适配、实验、不支持。每项绑定真实Rust公共符号、源文件/哈希、fixture和可收集pytest节点；不能用“基本完成”或聚合计数替代逐项映射。没有真实录包的字段不能升级为已验证稳定行为。
- 扩展源码树外installed_wheel_loopback.py，按已迁移域执行EX/MAC/MAC-EX非空公开查询及关闭。附件仅依赖标准库、已安装wheel和fixture；不导入仓库tests/src。T28/T29负责无Rust工具链消费环境。

编写相同语料差分、profile次数/期限、旧异常/raw golden、自定义transport、重连、信号与失败清理的测试源码；不能重生成冻结golden掩盖差异。完整Rust/Python扩展测试和opt-in线上验证均延期，后者须单列来源与日期。

## 工作约束与报告

工作目录C:/Users/ax/.codex/worktrees/rust-sdk-implementation/tdx抓包，分支codex/rust-sdk-implementation。全部T01–T30代码完成前禁止测试、collection、build/check/clippy/doctest、网络验证、编译/AST、格式或diff检查及等价验证。允许读写源码、普通review diff、禁用hooks的精确Git提交。不得派代理、push/tag/publish/merge或编辑controller状态文件；共享Cargo、binding、manifest文件先协调。

报告.superpowers/sdd/development-plan/task-27-report.md：完整方法/导出/native/适配器映射、兼容事实与已写用例、真实清单状态/证据缺口、精确文件与SHA、未执行声明、自审风险。最终只返回简短状态、SHA、未测试声明、风险和报告路径。
