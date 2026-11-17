# T01–T30 最终要求与证据索引

本页把开发计划R01–R12关联到实际源码、验证入口和必须保留的运行证据。它是验收导航，不能凭表格或脚本存在判定完成。统一执行已在stage115开放；stage131–134已完成Windows Rust1.89/1.100平台与feature矩阵、Loom、归档consumer、Linux/full与Loom以及两平台默认原生压力聚合；stage135完成Windows/Linux配对性能run及raw复验；stage136完成Windows Python3.10–3.14 wheel、完整回归和独立consumer矩阵。各次候选、平台、命令和具体范围见[统一验证记录](unified-validation.md)。stage135首轮逐项审计的未闭合项由stage136更新；macOS、非Windows wheel、生产/物理隔离及registry证据尚未闭合，整体最终交付仍未验收。未验证、失败、缺失、平台未具备、生产样本未取到必须分别记录。

## 要求逐项追踪

| 要求 | 实现与源码交接 | 验证入口/核心观察 | 达成所需证据 |
| --- | --- | --- | --- |
| R01 两产品独立安装 | `crates/eltdx`；两个 `tests/consumers`；[T28索引](t28-verification-index.md) | Rust-only外部consumer；Windows Python3.10–3.14独立wheel安装、pip check、非空公开API查询及close | 无Python的Rust物理隔离构建；无Rust工具的物理隔离wheel消费；公开registry身份及非Windows产物 |
| R02 共用Rust业务 | HQ/F10/offline/EX/MAC桥接；[HQ](hq-implementation-index.md)、[T23](t23-migration-index.md)、[T27](t27-migration-index.md) | 旧Python样本/黄金输出、新Python native bridge、独立Rust结果；custom hook合同 | 完整能力清单逐项实际node/fixture/差异及结果；两端都调用新算法的比较不能充当独立oracle |
| R03 owner/pin/pending | SDK session/operation，runtime engine/slot；任务T05–T07 | `lifecycle.rs`、`operation.rs`、runtime `pin_handoff.rs`；并发close/Drop与未确认清理 | 每个请求唯一归属和终态；旧generation迟到事件不能串线；超时不提前核销未回收资源 |
| R04 整次期限 | operation sponsor/authorization，metadata与各组合服务 | runtime `deadline.rs`、SDK `operation.rs`、F10/扩展fixture；Python逐请求对照 | startup/queue/metadata/page/retry都消耗同一原始Rust期限；Python原逐页策略不被缩短；清理期限另报 |
| R05 metadata隔离 | SDK metadata/context，显式精度快照与取消赞助 | `metadata.rs`、`precision_isolation.rs`、多owner与等待者取消 | 不跨client/源污染，不因单个waiter取消终止其他赞助者，不在失败时发布部分metadata |
| R06 顺序缺失分页 | codes/quotes/bars/trades/corporate及batch/cursor | `codes.rs`、`quotes_batch.rs`、`klines.rs`、`minutes.rs`、`trades.rs`、`corporate.rs`及逐域Python桥接 | 重复输入occurrence、乱序响应、明确missing、空/短/重复页、截断、上限与next offset；raw来源不被重排伪造 |
| R07 单位日期复权 | numeric/time/provenance、policy、adjustment/calendar | `units.rs`、`adjustment.rs`、`calendar.rs`、`price_limits.rs`、MAC helper/原始bits | 整数精度、f32/f64、负零、源/显示单位、End跨日与源日复权、同日事件顺序；Unknown不猜成权威规则 |
| R08 订阅 | subscription/dispatcher及Python兼容fanout | `subscription.rs`、`legacy_push.rs`、Python orchestration合同和压力subscription | 唯一push消费者、订阅pin持有/释放、续期/重同步、queue字节/帧限额、gap及关闭竞争 |
| R09 F10/EX/MAC边界 | [F10传输](f10-transport-index.md)、[端点](f10-implementation-index.md)、[EX](ex-implementation-index.md)、[MAC](mac-implementation-index.md)、[T26](t26-implementation-index.md) | `f10_http/values/routes/named_methods/route_inventory/results`；`ex_wire/session`、`mac_wire/session/routing`、extended组合 | 非空请求/响应、157入口和具名分支、动态重复列/未知slot/GBK/序号/时间；每个生产profile另有独立证据 |
| R10 文件下载有界 | [offline](offline-migration-index.md)、[financial](financial-implementation-index.md)、HQ/MAC下载 | `offline_bars/offline_blocks/offline_industry/gbbq`、`financial/financial_service/resources/stats/extended_downloads`及故障I/O | 逐字节roundtrip、残尾append不写、原末条阈值、partial write、ZIP目录与解压限额、完整EOF和LimitReached区别 |
| R11 兼容决策 | [Python基线](python-baseline.md)、[56个兼容案例](compatibility-cases.md)、[决策](decision-register.md) | manifest/static/reflection/public/model/错误与逐域合同；Windows Python3.10–3.14全套local/loopback/golden矩阵 | 原签名/默认/别名/异常context/cache/hooks/重开/文件行为；任何有意变化独立记录，不改黄金值遮盖 |
| R12 可交付可复验 | [发布政策](release-policy.md)、[T30压力](t30-evidence-index.md)、[性能](performance-policy.md)、[维护](maintenance.md) | feature/platform/MSRV、doctest/doc、归档consumer、Windows Python3.10–3.14 wheel、原始压力性能复验 | 同一候选的干净来源/lock/hash/平台/命令/计数/失败日志；不把local patch或path消费称为公开registry通过 |

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

本节较早阶段记录保留为历史；stage131–136的更新结果优先，完整范围见统一验证记录。性能在Windows/Linux已各84 trial并通过复验；macOS、非Windows wheel、物理隔离环境及生产协议证据仍需补齐，不把本机合成测量扩大到缺失环境。

## Stage135 逐项审计结论

以下结论按已保存的候选、平台和证据范围审计，不把脚本存在、源码完整或不同候选的结果合并成全通过。`部分`表示实现源码已完成且有相应本地证据，但仍有明确的环境、生产样本或兼容矩阵缺口。

| 要求 | 当前结论 | 已有证据 | 未闭合项 |
| --- | --- | --- | --- |
| R01 两产品独立安装 | 部分 | Windows wheel独立安装回归；Rust归档解包后的外部consumer执行11种模式 | 无Python/无编译器物理隔离镜像；公开注册表安装与源码身份 |
| R02 共用Rust业务 | 部分 | Python native bridge、独立Rust入口、Python local/loopback/golden回归及Linux Rust完整集合 | category22非空生产样本；各生产profile独立对照和完整能力逐项差分 |
| R03 owner/pin/pending | 部分 | Rust并发关闭/取消集合、Windows/Linux Loom及默认原生压力资源归零 | 未确认生产清理边界；macOS运行证据 |
| R04 整次期限 | 部分 | operation deadline、超时/取消边界与跨平台压力合同已有运行覆盖 | 声明平台/解释器矩阵仍不齐；生产endpoint与外部时延证据 |
| R05 metadata隔离 | 部分 | metadata owner隔离、等待者取消、精度快照相关Rust与Loom集合 | macOS和其余受支持环境的独立运行记录 |
| R06 顺序缺失分页 | 部分 | HQ/F10/financial分页与独立consumer回环；单endpoint单连接回归通过 | 全部声明平台/Python版本的逐域矩阵；真实服务分页形态 |
| R07 单位日期复权 | 部分 | 数值、时间、来源精度、调整与未知值由Rust用例及Python兼容回归覆盖 | 权威涨跌停规则仍未知；category22与生产复权样本缺失 |
| R08 订阅 | 部分 | Windows/Linux原生压力覆盖队列、actor、取消、关闭和资源回收 | 仅有合成负载；无生产长时订阅和macOS记录 |
| R09 F10/EX/MAC边界 | 部分 | F10具名入口与EX/MAC wire组合有本地fixture、Python回归及独立consumer证据 | MAC-EX生产LoginReplyRules和optional setup关联无抓包证明；category22缺样本 |
| R10 文件下载有界 | 部分 | offline/financial ZIP、DAT、K线和边界处理有Rust/Python本地验证及归档consumer | 无生产下载源的截断/限额行为证据；其他平台矩阵 |
| R11 兼容决策 | 部分 | Python 3.12 local 3046、loopback 276、golden 103通过；基线与差异规则有文档 | Python 3.10–3.14全矩阵未完成；生产hook/profile仍需对应样本 |
| R12 可发布可复验 | 部分 | Windows/Linux候选、lock/wheel、压力与性能raw均可复验，stage135性能raw复验通过 | macOS、公开registry发布后消费、真实隔离镜像；生产协议证据；未执行发布 |

stage135审计快照之后，stage136补齐了Windows Python3.10–3.14全部兼容回归和独立wheel消费。R11现在有该平台与版本范围的实际证据；R01、R12也增加了相同wheel交付证据。其余条目仍按具体平台、物理隔离和生产证据条件审视，没有任何一项因此升级为全环境、全生产条件通过。

## Stage136：Windows Python 版本矩阵

候选 `e2b8819c4729de28f0472ecbe743b0c2e100872f` 使用Windows x64/MSVC构建的`eltdx-3.3.5-cp310-abi3-win_amd64.whl`。Python3.10.22、3.11.18、3.12.6、3.13.18、3.14.10分别通过local 3048、loopback 276和golden 103项，合计17135 passed、0 failed、0 errors、0 skipped。每个版本另从临时环境安装同一wheel，pip check及独立非空公开API consumer均exit0；记录了wheel/native身份并成功close。

wheel SHA256为`82b0bf70a847ea7775079cb4d958130cfca850e9ca515ebf1e38b686de65fd3e`，native SHA256为`465e28d1dc950a97ce36bb3ee82766c6aebe52448d3486fad5203b67c030de66`，Cargo.lock SHA256为`214eecd8f24fd6c3a2bafc519087e608e0cb2df4fa9c409db33bfba19c475143`。逐版本解释器、各JUnit SHA、installed-wheel报告与consumer证据摘要见`artifacts/unified/stage136-python-matrix/results.json`。consumer为`process-path-only`工具链隔离，不能代表物理隔离镜像；这些是Windows合成loopback和兼容证据，不提供macOS、生产协议、公开registry或非Windows wheel证明。stage136的跨Python修正只调整了测试/快照比较假设，未改变运行时代码。文档门禁与最新恢复点结果见统一验证记录stage136章节。

这些不是授权缩减任务范围的理由。源码范围须全部实现，执行失败须继续处理；任何确实需要独立外部输入的项保留具体缺口和后续行动，不能借文档标记自动通过。
