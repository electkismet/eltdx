# T01–T30 最终要求与证据索引

本页把开发计划R01–R12关联到实际源码、验证入口和必须保留的运行证据。它是验收导航，不能凭表格或脚本存在判定完成。统一执行已在stage115开放，Windows Rust1.89/full合并集合已于stage120通过，Linux/full与Windows32feature检查已于stage121通过；各次候选、平台、命令和具体范围见[统一验证记录](unified-validation.md)。整体最终候选和完整交付矩阵仍未验收。未验证、失败、缺失、平台未具备、生产样本未取到必须分别记录。

## 要求逐项追踪

| 要求 | 实现与源码交接 | 验证入口/核心观察 | 达成所需证据 |
| --- | --- | --- | --- |
| R01 两产品独立安装 | `crates/eltdx`；两个 `tests/consumers`；[T28索引](t28-verification-index.md) | Rust-only外部consumer；Python-only镜像wheel消费；生产normal/build tree | 无Python的Rust构建及真实非空查询；无Rust工具的实际wheel安装/查询/close；各自真实依赖、路径及hash |
| R02 共用Rust业务 | HQ/F10/offline/EX/MAC桥接；[HQ](hq-implementation-index.md)、[T23](t23-migration-index.md)、[T27](t27-migration-index.md) | 旧Python样本/黄金输出、新Python native bridge、独立Rust结果；custom hook合同 | 完整能力清单逐项实际node/fixture/差异及结果；两端都调用新算法的比较不能充当独立oracle |
| R03 owner/pin/pending | SDK session/operation，runtime engine/slot；任务T05–T07 | `lifecycle.rs`、`operation.rs`、runtime `pin_handoff.rs`；并发close/Drop与未确认清理 | 每个请求唯一归属和终态；旧generation迟到事件不能串线；超时不提前核销未回收资源 |
| R04 整次期限 | operation sponsor/authorization，metadata与各组合服务 | runtime `deadline.rs`、SDK `operation.rs`、F10/扩展fixture；Python逐请求对照 | startup/queue/metadata/page/retry都消耗同一原始Rust期限；Python原逐页策略不被缩短；清理期限另报 |
| R05 metadata隔离 | SDK metadata/context，显式精度快照与取消赞助 | `metadata.rs`、`precision_isolation.rs`、多owner与等待者取消 | 不跨client/源污染，不因单个waiter取消终止其他赞助者，不在失败时发布部分metadata |
| R06 顺序缺失分页 | codes/quotes/bars/trades/corporate及batch/cursor | `codes.rs`、`quotes_batch.rs`、`klines.rs`、`minutes.rs`、`trades.rs`、`corporate.rs`及逐域Python桥接 | 重复输入occurrence、乱序响应、明确missing、空/短/重复页、截断、上限与next offset；raw来源不被重排伪造 |
| R07 单位日期复权 | numeric/time/provenance、policy、adjustment/calendar | `units.rs`、`adjustment.rs`、`calendar.rs`、`price_limits.rs`、MAC helper/原始bits | 整数精度、f32/f64、负零、源/显示单位、End跨日与源日复权、同日事件顺序；Unknown不猜成权威规则 |
| R08 订阅 | subscription/dispatcher及Python兼容fanout | `subscription.rs`、`legacy_push.rs`、Python orchestration合同和压力subscription | 唯一push消费者、订阅pin持有/释放、续期/重同步、queue字节/帧限额、gap及关闭竞争 |
| R09 F10/EX/MAC边界 | [F10传输](f10-transport-index.md)、[端点](f10-implementation-index.md)、[EX](ex-implementation-index.md)、[MAC](mac-implementation-index.md)、[T26](t26-implementation-index.md) | `f10_http/values/routes/named_methods/route_inventory/results`；`ex_wire/session`、`mac_wire/session/routing`、extended组合 | 非空请求/响应、157入口和具名分支、动态重复列/未知slot/GBK/序号/时间；每个生产profile另有独立证据 |
| R10 文件下载有界 | [offline](offline-migration-index.md)、[financial](financial-implementation-index.md)、HQ/MAC下载 | `offline_bars/offline_blocks/offline_industry/gbbq`、`financial/financial_service/resources/stats/extended_downloads`及故障I/O | 逐字节roundtrip、残尾append不写、原末条阈值、partial write、ZIP目录与解压限额、完整EOF和LimitReached区别 |
| R11 兼容决策 | [Python基线](python-baseline.md)、[56个兼容案例](compatibility-cases.md)、[决策](decision-register.md) | manifest/static/reflection/public/model/错误与逐域合同 | 原签名/默认/别名/异常context/cache/hooks/重开/文件行为；任何有意变化独立记录，不改黄金值遮盖 |
| R12 可交付可复验 | [发布政策](release-policy.md)、[T30压力](t30-evidence-index.md)、[性能](performance-policy.md)、[维护](maintenance.md) | feature/platform/MSRV、doctest/doc、归档consumer、wheel/sdist、原始压力性能复验 | 同一候选的干净来源/lock/hash/平台/命令/计数/失败日志；不把local patch或path消费称为公开registry通过 |

表中的测试名是 `crates/eltdx/tests` 中的target，runtime文件在 `crates/eltdx-runtime/tests`；Python合同与集成目录分别是 `tests/contracts`、`tests/integration`，扩展/F10迁移合同还在其分域测试目录中，精确路径与nodes以分域索引和manifest为准。目录名、方法名和函数数量只是定位信息，执行后须核对实际收集集合和行为断言覆盖。

逐服务普通使用示例与非空回环精确函数见[示例对应表](service-examples.md)。E06 的格式和checked类型策略见[Serde合同](examples-serde.md)，实际运行target为 `serde_contract`：finite/nonfinite、负零、bytes/缺失/空、日期时区/Unknown和源单位；三个trait缺失compile-fail由serde doctest执行。`serde` 单独生产依赖和F10独立文档也要验证，不以full成功推定它们成功。

## 执行顺序与候选冻结

1. **全源码门槛**：逐任务确认实现、绑定、测试源码、示例、打包与文档都已写齐；记录当前仍缺的执行证据，不以先跑测试探路替代源码完成。
2. **工具与lock**：门槛之后才发现实际工具链/解释器、解析新依赖和更新Cargo.lock。验证声明的pin、MSRV、features及license；修改形成新源码检查点，lock与候选来源一致后再进行locked验证。
3. **基础编译和离线正确性**：先明确三个Rust库和Python绑定的编译边界，随后完整Rust/协议/runtime、Python合同与黄金差分及回环故障集合。新代码的失败必须修复，不能减范围或删除期望换绿。
4. **交付矩阵**：MSRV1.89/current stable、Linux/Windows/macOS、32feature子集/default/full、负面API/feature、外部Rust消费者、Python3.10–3.14及wheel/sdist分开执行。
5. **文档**：默认/full rustdoc警告视为错误、doctest、MkDocs strict、生成HTML本地链接/fragment和完整包内文档镜像；文档命令不联网抓取外部链接代替本地校验。
6. **压力和性能**：基础正确性之后再运行两套独立手动负载，保留每次失败，使用同候选/lock/fixture/脚本和固定事前政策。raw重算通过后才接受汇总。
7. **显式外部证据**：真实服务器、生产登录/setup规则、注册表身份、专用平台镜像和在线兼容单列。未运行则保留未运行，线上失败不伪装成离线失败；不得上传包、tag、push或合并。
8. **逐项完成审计**：为R01–R12及原计划全部复选项提供证据定位、SHA、命令、平台、actual/expected、退出码、日志hash和结论。源码改动会使受影响旧证据失效，须标注重跑范围；只靠绿色总状态不能完成审计。

`scripts/verification/rust_sdk_matrix.ps1`、`rust_sdk_no_python.sh`、`rust_sdk_packages.py`、`consume_python_wheel.py`、`rust_sdk_stress.py`、`rust_sdk_performance.py` 是不同边界的执行入口。旧 `scripts/unified_test.py` 保留Python产品九轮门禁，不能把旧Python压力或21命令标题当作新Rust全域证据。

## 每条运行记录的最小内容

记录 requirement/task IDs、完整40位source SHA、dirty状态与tree/关键blob、Cargo.lock和语料/工具脚本hash、工具链/解释器版本、features/profile、OS/架构/CPU及必要镜像digest、完整命令/工作目录/环境差异、起止UTC和耗时、真实退出码/超时、实际收集/运行/跳过计数、原始stdout/stderr与SHA256。行为报告还须保留实际请求、业务identity、来源及断言，不仅填写JSON字段格式。

每次新运行用新目录。failed/timeout/缺依赖的日志不可覆写为passed；外部平台报告必须同候选，不能拿旧SHA结果补当前空缺。需要重新生成黄金值、改固定阈值或减少工作量时保留旧失败和依据，先审查合同再执行。

## 当前明确缺口

- stage122完整Python local3046、loopback276及golden103通过；stage123干净92f033b的Windows/Linux full、Loom及独立wheel公开消费通过。后续平台矩阵暴露的超时/取消清理竞态已于stage124确定性red/green修复，修复后的Windows/full/all-targets命令exit0。stage125三个纯Rust库的full/all-targets严格Clippy exit0，Python源码合同603通过，但native仍是旧候选；须对新候选重建wheel并重跑受影响Rust/Python、平台/feature/Loom与完整消费者/包/文档/压力性能。32feature和其他旧候选通过不能补新候选空缺，source-complete不等于整体验收。
- MAC-EX生产LoginReplyRules默认与optional MAC setup的响应identity/order/fence仍无独立证明。合成成功/拒绝字节只允许在显式合成profile使用。
- category22没有独立非空生产样本，未知涨跌停规则仍返回Unknown；不影响已独立实现的HQ0x0FFC与F10历史资金流归属。
- 注册表名称/同版本源码身份、全部新依赖lock/API/MSRV/license、跨平台镜像与实际wheel支持仍需真实执行。
- Windows/macOS hosted的process-path-only隔离不能证明整台机器没有编译器；更强环境要求须提供专用镜像证据。

这些不是授权缩减任务范围的理由。源码范围须全部实现，执行失败须继续处理；任何确实需要独立外部输入的项保留具体缺口和后续行动，不能借文档标记自动通过。
