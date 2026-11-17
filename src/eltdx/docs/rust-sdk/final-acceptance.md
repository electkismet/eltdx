# T01–T30 最终要求与证据索引

本页把开发计划R01–R12关联到实现、验证入口和交付证据。R01–R12是完整交付范围的追溯索引，不是代码完成度或单一验收百分比；“尚未满足全部发布条件”不表示实现源码或本地核心验证仍未完成。各阶段、候选SHA、命令与原始结果见[统一验证记录](unified-validation.md)。

## 当前状态

| 范围 | 状态 | 结论 |
| --- | --- | --- |
| T01–T30实现 | 完成 | 计划内Rust SDK、Python兼容桥接、独立Rust入口及文档/示例源码已写齐。Stage145–148依据生产样本修正Rust MAC-EX、标准EX商品总数的31字节响应解析。 |
| 本地核心验证 | 已通过所记录范围 | Rust平台/feature/Loom/Linux、压力与性能、Python 3.10–3.14矩阵，以及当前三crate归档和解包consumer均有通过记录。证据按各自候选SHA和平台解释，不合并成一个未经执行的“全矩阵单候选”。 |
| 发布平台与隔离 | 已有 Linux x64、ARM 模拟和 Windows 证据；macOS、原生 ARM 待跑 | Stage140补齐两种工具链隔离；Stage142/143从sdist重建x64和arm64 manylinux2014 wheel。Stage148的Windows wheel在干净候选上通过Python 3.10/3.14独立消费。 |
| 注册表发布与消费 | 未发布 | Stage148在干净候选实际查询三个名称均返回404；本地归档通过，先前的protocol Cargo发布dry-run也通过。缺少crates.io发布凭据；发布后仍需用不带patch的独立项目验证。 |
| 生产协议验收 | 部分公开主站流程已验证 | 标准EX主站完成连接、查询52个市场与111218个商品、关闭，与Python实测一致。两台MAC-EX主站以即时完整登录样本完成独立Rust查询，三个普通MAC主站通过默认三步握手及业务查询。仍待可跨时段复用的MAC-EX登录规则、可选setup和category22非空证据。已核对五份带证券与日期的公告，尚缺交易所当日价格边界。 |

因此，当前可报告为“实现完成；记录范围内的本地核心验证通过；发布/生产验收未完成”。后两项依赖外部平台、注册表权限或生产数据，不用来回滚前两项状态，也不合并成一个完成比例。

## 要求逐项追踪

| 要求 | 实现与源码交接 | 验证入口/核心观察 | 达成所需证据 |
| --- | --- | --- | --- |
| R01 两产品独立安装 | `crates/eltdx`；两个 `tests/consumers`；[T28索引](t28-verification-index.md) | 最新候选三crate归档和11种解包Rust consumer模式；Windows Python3.10–3.14独立wheel安装、pip check、非空公开API查询及close | 无Python的Rust物理隔离构建；无Rust工具的物理隔离wheel消费；公开registry身份及非Windows产物 |
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
| R12 可交付可复验 | [发布政策](release-policy.md)、[T30压力](t30-evidence-index.md)、[性能](performance-policy.md)、[维护](maintenance.md) | feature/platform/MSRV、doctest/doc、最新候选crate归档consumer、Windows Python3.10–3.14 wheel、原始压力性能复验 | 同一候选的干净来源/lock/hash/平台/命令/计数/失败日志；不把local patch或path消费称为公开registry通过 |

表中的测试名是 `crates/eltdx/tests` 中的target，runtime文件在 `crates/eltdx-runtime/tests`；Python合同与集成目录分别是 `tests/contracts`、`tests/integration`，扩展/F10迁移合同还在其分域测试目录中，精确路径与nodes以分域索引和manifest为准。目录名、方法名和函数数量只是定位信息，执行后须核对实际收集集合和行为断言覆盖。

逐服务普通使用示例与非空回环精确函数见[示例对应表](service-examples.md)。E06 的格式和checked类型策略见[Serde合同](examples-serde.md)，实际运行target为 `serde_contract`：finite/nonfinite、负零、bytes/缺失/空、日期时区/Unknown和源单位；三个trait缺失compile-fail由serde doctest执行。`serde` 单独生产依赖和F10独立文档也要验证，不以full成功推定它们成功。

## 完整发布审计流程

以下流程只在准备新的完整发布候选时适用。文档-only提交不要求重跑Rust/Python全矩阵；仅当运行时代码或其合同发生变化时，按影响范围重跑相应验证。既有报告继续绑定各自source SHA。

1. **全源码门槛**：逐任务确认实现、绑定、测试源码、示例、打包与文档都已写齐；记录当前仍缺的执行证据，不以先跑测试探路替代源码完成。
2. **工具与lock**：门槛之后才发现实际工具链/解释器、解析新依赖和更新Cargo.lock。验证声明的pin、MSRV、features及license；修改形成新源码检查点，lock与候选来源一致后再进行locked验证。
3. **基础编译和离线正确性**：先明确三个Rust库和Python绑定的编译边界，随后完整Rust/协议/runtime、Python合同与黄金差分及回环故障集合。新代码的失败必须修复，不能减范围或删除期望换绿。
4. **交付矩阵**：MSRV1.89/current stable、Linux/Windows/macOS、32feature子集/default/full、负面API/feature、外部Rust消费者、Python3.10–3.14及wheel/sdist分开执行。
5. **文档**：默认/full rustdoc警告视为错误、doctest、MkDocs strict、生成HTML本地链接/fragment和完整包内文档镜像；文档命令不联网抓取外部链接代替本地校验。
6. **压力和性能**：基础正确性之后再运行两套独立手动负载，保留每次失败，使用同候选/lock/fixture/脚本和固定事前政策。raw重算通过后才接受汇总。
7. **显式外部证据**：真实服务器、生产登录/setup规则、注册表身份、专用平台镜像和在线兼容单列。记录未运行项与线上失败；实际发布在独立的授权阶段按[发布政策](release-policy.md)执行。
8. **逐项完成审计**：为R01–R12及原计划全部复选项提供证据定位、SHA、命令、平台、actual/expected、退出码、日志hash和结论。源码改动会使受影响旧证据失效，须标注重跑范围；只靠绿色总状态不能完成审计。

`scripts/verification/rust_sdk_matrix.ps1`、`rust_sdk_no_python.sh`、`rust_sdk_packages.py`、`consume_python_wheel.py`、`rust_sdk_stress.py`、`rust_sdk_performance.py` 是不同边界的执行入口。旧 `scripts/unified_test.py` 保留Python产品九轮门禁，不能把旧Python压力或21命令标题当作新Rust全域证据。

## 每条运行记录的最小内容

记录 requirement/task IDs、完整40位source SHA、dirty状态与tree/关键blob、Cargo.lock和语料/工具脚本hash、工具链/解释器版本、features/profile、OS/架构/CPU及必要镜像digest、完整命令/工作目录/环境差异、起止UTC和耗时、真实退出码/超时、实际收集/运行/跳过计数、原始stdout/stderr与SHA256。行为报告还须保留实际请求、业务identity、来源及断言，不仅填写JSON字段格式。

每次新运行用新目录。failed/timeout/缺依赖的日志不可覆写为passed；外部平台报告必须同候选，不能拿旧SHA结果补当前空缺。需要重新生成黄金值、改固定阈值或减少工作量时保留旧失败和依据，先审查合同再执行。

## 后续工作

- 发布平台：对程序候选 `f2703c1` 或其后仅更新文档的提交运行macOS平台和归档工作流。获得公开分支推送授权后，推送候选分支触发macOS Rust任务，再对同一SHA手动运行 `Publish`，验证macOS Intel/ARM及Linux原生arm64 wheel和sdist。手动运行不上传PyPI，也不创建GitHub Release；留存 run ID、SHA、产物散列和安装结果。x64隔离消费和ARM模拟重建见[统一验证记录](unified-validation.md)。
- 生产协议：取得跨时段的MAC-EX登录成功样本及独立失败样本，确认时钟字段与响应身份后再确定可长期使用的规则。另补可选setup的身份/顺序/fence、category22非空路径和交易所当日涨跌停价；未验证含义继续保持Unknown。
- 注册表：在本机配置获授权的crates.io发布凭据后，核对包名、版本和账号权限，按依赖拓扑发布；随后用不含patch的独立consumer验证实际registry身份和消费结果。发布前重新查询，404仅代表该时间点的查询结果。

R01–R12首轮逐项审计是stage135时点的历史快照，后续更新以本节状态和[统一验证记录](unified-validation.md)为准，不再复制一份逐行“部分”表。已有Windows Python矩阵、Windows/Linux压力与性能、以及stage138归档结果分别保留在对应阶段记录中。

## Stage136：Windows Python 版本矩阵

候选 `e2b8819c4729de28f0472ecbe743b0c2e100872f` 使用Windows x64/MSVC构建的`eltdx-3.3.5-cp310-abi3-win_amd64.whl`。Python3.10.22、3.11.18、3.12.6、3.13.18、3.14.10分别通过local 3048、loopback 276和golden 103项，合计17135 passed、0 failed、0 errors、0 skipped。每个版本另从临时环境安装同一wheel，pip check及独立非空公开API consumer均exit0；记录了wheel/native身份并成功close。

wheel SHA256为`82b0bf70a847ea7775079cb4d958130cfca850e9ca515ebf1e38b686de65fd3e`，native SHA256为`465e28d1dc950a97ce36bb3ee82766c6aebe52448d3486fad5203b67c030de66`，Cargo.lock SHA256为`214eecd8f24fd6c3a2bafc519087e608e0cb2df4fa9c409db33bfba19c475143`。逐版本解释器、各JUnit SHA、installed-wheel报告与consumer证据摘要见`artifacts/unified/stage136-python-matrix/results.json`。consumer为`process-path-only`工具链隔离，不能代表物理隔离镜像；这些是Windows合成loopback和兼容证据，不提供macOS、生产协议、公开registry或非Windows wheel证明。stage136的跨Python修正只调整了测试/快照比较假设，未改变运行时代码。恢复提交`fa9355e`上的metadata、MkDocs strict、站点链接和diff门禁均exit0；最新门禁与后续恢复记录见统一验证记录stage136章节。

该矩阵覆盖Windows声明的Python版本范围，不覆盖macOS/非Windows wheel、物理隔离、公开registry发布后消费或生产协议证据。它与这些外部验收项分层记录，不互相替代。

## Stage137–139：注册表观察、归档与文档口径

stage137的crates.io HTTP 404是记录时点的只读结果；stage138通过的是本地归档和解包consumer模拟。stage138报告SHA256为`1519ace848c8809e5a8ad8c337c9a043955f9c26a444c2683471a47f38e1530f`。stage139 attempt02绑定干净提交`a000eaabc7b8e479b9dd288d8a12a548dd090acc`，metadata/mirror、MkDocs strict、站点链接与diff-check均通过；attempt03的四项dirty-tree结果完整保留且均exit0。验收口径更新后的attempt04–06也均通过四项门禁；attempt06结果JSON SHA256为`424cfed41146d834ffb6f6f0425be4de09baa29b32cb6685bcc5b4cee51fdff8`。所有旧attempt原样保留。
