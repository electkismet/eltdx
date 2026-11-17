# Rust SDK 维护、恢复与候选验收手册

本手册与[详细计划](development-plan.md)、[最终验收索引](final-acceptance.md)共同约束后续工作。源码阶段先全部写齐再统一验证；进入执行阶段后用实际失败修复实现。不得因网络波动、上下文压缩或单次会话结束遗失未完成项，也不得把恢复提交当作测试通过。

## 恢复入口与可靠检查点

恢复时先读 `IMPLEMENTATION_STATUS.md` 的**最上方最新**记录、对应 `.superpowers/sdd/development-plan/task-XX-report.md` 的**最后**检查点，以及真实Git status/log。旧进度段是历史，不因为其语句仍写“下一步T20”就重新执行已完成任务。工作区真实文件和提交是权威，聊天摘要只作导航。

每个有意义阶段保存：基础SHA、变更任务/文件、完成的源码范围、未执行验证、具体剩余工作/缺口、依赖或协议风险、恢复所需入口、下一步。用户工作与本轮改动分开，精确stage所需文件；不做全目录reset/clean，不删未识别文件。原工作分支使用 `codex/`；已有隔离worktree不重复创建或迁移。

提交只作为可恢复源码快照，消息不能声称未执行的测试通过。源码阶段禁止测试/构建/check/collection/AST/格式/依赖解析/网络探测及等价验证；保存提交时禁用可能隐式触发这些操作的hooks。文档源在 `docs/`，包内镜像在 `src/eltdx/docs/`，同阶段同步。正式执行文档镜像校验仍需在全源码门槛后。

逐服务与Serde示例的权威源码为`docs/rust-sdk/examples-{hq,f10,offline,ex,mac,serde}.md`；同时逐字复制到`crates/eltdx/src/guide/{hq,f10,offline,ex,mac,serde}.md`供rustdoc的include_str使用，并同步通常的包内docs mirror。既有metadata检查器只验证六对字节一致，不替代真实doctest、非空回环或归档编译；对应关系维护在`service-examples.md`。单独serde的运行合同与Serde/F10各自文档门禁须保留，不能仅依赖full feature。

## 网络波动与进程恢复

开启耗时命令时保存其确切命令、开始时间、输出目录和实际session/process/job句柄；输出写文件，避免上下文只留下截断文本。观察超时不等于进程退出，重连后先查询同一真实句柄/进程，不凭一份lock或running JSON就重启。确认终态或句柄不存在后，才按任务语义决定继续、修复或重试。

依赖下载或外部查询失败保留原始stdout/stderr、退出码、时间和具体URL/服务；不能把缓存中有同名目录当作已成功解析，也不能无说明换版本。重试新建目录或追加独立attempt记录，不覆盖旧失败。观察器/fixture崩溃与SDK错误分别记因，实际业务和资源日志保持关联。

不得将自动续跑改为定时器、偷偷发布或上传证据。持续目标保持原T01–T30范围；普通状态查询不重定义目标。若确实被外部条件阻塞，记录同一阻塞的证据和可继续的独立工作，按目标控制规则处理，不因困难或剩余工作多自行暂停。

## 唯一业务实现与兼容边界

新业务逻辑进入SDK域模块，共用protocol/runtime，不在Python和Rust分别写第二份算法。共享类型、owner、metadata、operation、结果来源和缓存键变动先检查所有调用域。PyO3与Python facade保持对象、异常和生命周期适配；保留项和退出条件按HQ/T23/T27索引维护。

新Rust严格语义与PythonLegacy必须明确选择，不能在共用函数里无条件加新上限/预取/排序/去重。先增加能说明行为的fixture和合同源码，再改实现；已冻结黄金输出不能由新实现自己重新生成后冒充独立预期。日期/单位/涨跌停/login/setup关联没有依据时保留Unknown/Unsupported或要求显式profile，不从“测试能过”推导生产协议。

owner、pin、worker资源必须有单一登记责任；取消/close未确认时保留责任及必要租约，不能提前释放并让新请求重用旧连接。完成标志、线程退出、join确认和预算释放是不同事实。任何共享状态修复须覆盖相邻取消/关闭/迟到事件/新generation路径。

## 全源码完成后的统一执行

先记录门槛，然后准备真实工具链和依赖lock；补lock形成候选后执行locked命令。最早的编译失败不能靠删除feature、跳过目标或缩小API来消除。源代码修正后更新相关恢复记录和候选SHA；与旧SHA绑定的验收不得移花接木。

| 阶段 | 入口 | 需要保存的事实 |
| --- | --- | --- |
| Rust基础/矩阵 | `rust_sdk_matrix.ps1`及`rust-sdk.yml` | default/minimal/full/32子集、三平台、MSRV/stable、doctest/rustdoc/clippy/fmt和外部consumer |
| Python兼容 | 原unified九轮及`run_python_test_group.py` | 真实collection、逐域合同/黄金/回环/适配器、Python版本与native身份 |
| 独立环境 | `rust_sdk_no_python.sh`、`consume_python_wheel.py` | 物理镜像或进程PATH隔离强度、实际安装/构建、非空查询/close、normal/build tree |
| 包内容 | `rust_sdk_packages.py`、原wheel/sdist脚本 | 规范化依赖、归档逐文件/许可证、真实解包consumer、本地patch与registry区别 |
| 文档 | MkDocs strict、`check_pages_links.py`、`check_release_metadata.py` | 警告、HTML本地链接/fragment、根/包内镜像；不得以浏览首页代替全站 |
| 压力 | `rust_sdk_stress.py run/verify/aggregate` | Linux+Windows同SHA默认负载、真实budget/OS/身份、原始日志重算与固定阈值 |
| 性能 | `rust_sdk_performance.py build-wheel/run/verify` | 同构建profile的旧/新wheel、Rustworker、84个默认trial、raw样本/流量/资源、分位数波动 |
| 外部事实 | 显式在线/registry/profile取证 | 具体服务器、市场/时间、样本来源、命令/响应身份；失败/未运行独立保留 |

文档已有严格构建和HTML链接复验器，不再增加仅检查文件存在便声称覆盖通过的总验收器。R01–R12索引是审计导航，最终仍逐项检查实际行为和原计划所有验收项。

## 失败、阈值与发布交接

每个失败保留原始日志，不先修改测试期望。先定位不变量、最小真实触发和受影响调用路径，再做最小修正；重跑受影响集合及必要完整矩阵。测试代码自身错误也须有来源解释，不能因为通过更容易就降低负载、删edge case或放宽预算。

压力阈值、Windows循环减量和性能比较范围在[压力索引](t30-evidence-index.md)、[性能政策](performance-policy.md)事前固定。RSS采样峰值不等于精确分配峰值，TIME_WAIT不等于活连接泄漏，严格Rust与legacy Python耗时不能相减伪造“转换成本”。任何政策调整保留前次失败、依据和覆盖变化。

候选交接包包含真实source/lock/产物hash、feature与版本矩阵、平台/兼容/资源/性能结果、已知缺口、重跑命令和license。修改任何共享实现后核对需失效的全部证据，不只重跑最快的一条。发布/tag/push/merge需要独立授权；本目标准备代码和可审阅证据，不自动执行发行。最终完成声明必须经逐项审计，不能由“没有看到错误”或工具JSON里的status字段替代。
