# T30 — 完整交付文档、压力源码与统一验证交接

先读本文。权威要求为docs/rust-sdk/development-plan.md T30、R01–R12/capability matrix、verification.md全文（尤其§§11–12）、全部任务报告/manifest和实际API。T01–T29实现完成后controller GO。先完成全部实现、测试/压力/证据脚本与文档源码；由controller宣布进入统一验证阶段，实施者不能因自己的文件已写完就先运行测试。

## 交付范围

- Rustdoc/示例、Python迁移说明、能力索引、发布说明、维护和发行手册、crates/eltdx/tests/stress.rs显式ignored用例、独立Rust压力/测量/证据导出与聚合/复验工具、证据索引及全部docs mirror。不得写占位性能或伪造passed记录。发现承诺范围缺实现时与controller补全，不能只把标签改为完成。
- R01–R12和每项能力/迁移记录关联实际方法、源码、fixture、测试、consumer、证据路径。category22实验、UnknownRule、未验证profile/录包明确不属于已确认稳定能力。Python/Rust各自安装、不需同时安装；Rust无Python依赖，wheel消费无Rust工具。说明保留的兼容差异/扩展钩子、同步API/pull订阅；真正async/CLI/HTTP/MCP属后续范围。
- 示例使用正确feature/公共API、单位、日期、raw、错误、来源和关闭。不能宣称RawOmit零拷贝、未知单位已核实或保留hash已验完整性。E06 serde策略说明finite/nonfinite/bytes/date/unknown，checked类型不能经无校验Deserialize绕过不变量。真实联网例子no_run供稍后编译，公开回环承担实际行为；不能全部ignore。
- ignored压力用例至少100000请求，核对唯一完成/取消、最多一个终态、generation隔离；有界队列、慢服务器、满推送队列、暂停消费、坏帧、断线/重试；owner/pin/subscription/cache/F10池创建关闭循环；分页/下载/ZIP限额和取消释放。Windows/Linux分别采样真实次数、并发、seed、payload、配置、耗时、各阶段预算峰值、线程/socket/句柄/RSS，默认快测与压力分开。OS数据真实读取，不能填固定值或仅看最终RSS。大生成产物不进入Git。
- 性能比较固定相同语料、工作量、并发、假服务器、release/工具链及执行profile，记录基线/候选SHA、预热、波动。Rust核心、Python转换、网络分开，测吞吐、p50/p95/p99、CPU/峰值内存/取消关闭延迟。不能把旧Python压力日志改名为Rust证据；真实服务器采样opt-in且单列来源/日期。
- 导出完整SHA+dirty、lock/语料/runner哈希、工具链/features、平台CPU、命令、实际计数、退出码、起止UTC、日志hash。聚合器读取原始日志重算次数/资源峰值/错误/退出状态，拒绝不同SHA、缺平台/日志、failed/unrun伪装passed；只验schema不够。阈值或Windows循环缩减写明依据与覆盖代价，不能无说明放宽以求通过。
- 文档“拟议”状态按实际代码更新；统一验证前只能写源码已实现/未验证。mirror同步到包内现有docs目录，不删除无关用户文件。链接/严格文档构建脚本现在编写、之后执行；保留失败/未运行记录，不执行发布。

## 工作约束与报告

工作目录C:/Users/ax/.codex/worktrees/rust-sdk-implementation/tdx抓包，分支codex/rust-sdk-implementation。controller确认全部T01–T30实现代码完成前，禁止测试、collection、build/check/clippy/doctest、网络、编译/AST、格式/diff检查及等价验证。仅读写源码/文档、普通review diff和精确Git恢复提交，禁用hooks。不得派代理、push/tag/publish/merge或编辑controller账本/状态；共享广泛文件先协调。

报告.superpowers/sdd/development-plan/task-30-report.md：完整需求/证据映射、压力/benchmark/导出源码及未运行事实、文档/mirror文件与SHA、任何剩余实现缺口（有缺口就不是实现完成）、统一验证交接命令、自审风险。最终简短返回状态/SHA/未执行/风险/报告路径；整个目标必须在统一验证之后才能标complete。
