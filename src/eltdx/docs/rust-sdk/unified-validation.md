# 全源码门槛与统一验证记录

## 源码门槛：stage115

日期为2026年11月17日。root在源码检查点 `23bf1c83718e83d20e9c5ba9b919e9dfac3e2db4` 保存后，确认T01–T30约定的实现、兼容绑定、测试/回环/压力/性能工具、消费者、包和文档源码范围均已写齐；普通Git状态显示工作区干净。根据用户“全部写完后统一测试”的既有指令，从本门槛开始允许统一执行验证。此决定只允许开始验证，**不证明任何编译、行为、兼容或发布验收已通过**。

源码交接：`task-30-report.md`第四检查点、[逐服务示例](service-examples.md)、[Serde合同](examples-serde.md)、[最终R01–R12索引](final-acceptance.md)、[压力](t30-evidence-index.md)与[性能](performance-policy.md)。T01–T30原计划的执行验收框仍保持未通过。

统一验证从实际工具链/解释器发现和依赖lock准备开始，随后基础编译、全部离线合同与故障回环、Python兼容、外部consumer/包、feature/MSRV/平台、文档、压力和性能。失败必须保留原始命令、退出码和日志；修正后记录新候选与需要重跑的范围，不降低测试范围或覆盖原失败。耗时进程保存实际session/输出路径，网络中断后先恢复观察原进程。

## 当前结果

此门槛提交时尚未运行任何新测试、构建、依赖解析、网络探测、格式或文档校验。没有通过计数。后续每阶段在此追加真实结果摘要和原始日志位置，完整证据仍以实际输出为准。

生产MAC-EX登录规则、optional MAC setup关联、category22非空capture、未知涨跌停规则、注册表源码身份、专用无编译器镜像及跨平台实际证据继续分别保留具体缺口，不能以本地合成通过替代。目标继续active，无额度上限。本阶段没有授权发布、push、tag或merge。

## stage116：实际工具、lock与首轮编译修正

本机实际为Windows x86_64 MSVC，PowerShell7.6.9，Rust/Cargo1.89.0，另有stable工具链；Python3.12.6。pytest9.0.2、maturin1.15.0、MkDocs1.6.1及psutil6.1.1已存在，WSL列出Ubuntu22.04，但尚未进入Linux执行验证，也未据此声明干净隔离环境。

原始日志保留在 `artifacts/unified/stage116-tooling/`，不提交生成日志。各attempt独立文件，失败未覆盖：

| 实际命令/记录 | 结果与含义 |
| --- | --- |
| `cargo +1.89.0 generate-lockfile` attempt01 | exit0，选出214包，但generator0.8.12及Windows0.100传递依赖要求Rust1.95；不能据exit0称MSRV兼容 |
| `cargo +1.89.0 check -p eltdx-runtime --features loom --locked` attempt01 | exit101，实际复现上述MSRV冲突 |
| 同一lock命令 attempt02，新增resolver fallback | exit0，选出208个符合Rust1.89的包，generator0.8.11；直接精确版本不变 |
| 同一Loom check attempt02 | exit0，仍有警告；仅编译，不是Loom行为通过 |
| `cargo +1.89.0 check -p eltdx --all-targets --no-default-features --features full --locked` attempt01–03 | exit101，保留Hyper客户端API/私有runtime启动及测试类型推断/导入/结果访问的实际错误 |
| 同一full check attempt04 | exit0，仍有警告；不是clippy或全部行为通过 |
| `f10_http::encoded_chunked_body_and_response_header_caps_apply_while_reading` | 移除不可用Hyper方法后实际出现Protocol≠LimitExceeded失败；补有界原始头观察后最终诊断attempt通过1项，26项过滤，旧失败保留 |
| `cargo +1.89.0 test -p eltdx --lib --no-default-features --features full --locked header_budget_tests` | exit0，3通过、230过滤；exact/逐边界拆包、未终结/超一字节、informational与body分界 |

根因与修正：Cargo resolver2默认接受更新的高MSRV传递版本，仓库增加 `.cargo/config.toml` 的 `incompatible-rust-versions="fallback"` 并重建lock，独立staging复制该配置，压力来源清单也包含它。Hyper1.12的 `is_parse_too_large` 受server feature限制，client-only不可用；不引入server功能，改在既有HTTP明文字节观察器用固定状态跟踪头边界、每头byte cap、informational响应和最终body分界，保留typed LimitExceeded及原错误链。Hyper仍负责完整语法与framing校验。

其余编译修正保持合同：并发close测试通过公开begin_connect启动真实runtime并保留pending；cursor和finance测试补明确类型；F10 extras接受slice；NewsChannel补实际导入；pin结果requested元素直接按SecurityId断言。没有删除用例、降低阈值或变更Python公开合同。

这些运行发生在b94d762之后的工作树修正中，正式同候选验收须在恢复提交后重跑受影响集合。完整Rust行为、PyO3编译/Python兼容、feature/平台/包/文档/压力性能仍未通过，警告仍待处理。后续继续统一验证，不将本节局部通过扩大成项目完成。

## stage117：首轮完整SDK行为失败与文档修正

候选为 `7ab55ba298c677df8936329999504cf1ac9c955b`。实际命令 `cargo +1.89.0 test -p eltdx --no-default-features --features full --locked --no-fail-fast` 最终exit101；原始日志与JSON在 `artifacts/unified/stage117-correctness/sdk-full-attempt01.*`。失败target为lib、ex_session、extended_boards、extended_catalogs、extended_loopback、f10_http、f10_named_methods、f10_results、helpers、hq_helpers、lifecycle、mac_session、quotes_batch、resources，共14个。其余target有通过，full doctest为30通过，Serde合同为7通过；本节不提供尚未独立汇总的总用例通过率。

lifecycle的 `concurrent_client_clones_close_the_same_owner` 超过60秒仍未结束，精确进程身份、持续时间及停止原因保存在 `lifecycle-timeout-attempt01.json`；仅停止该workspace测试二进制，cargo随后继续并完成其他target。这不是已证实的SDK close死锁：源码明确把spawn写入惰性map，在消费前主线程先进入barrier，因此工作线程尚未创建。后续改为屏障前eager collect，再实际重跑并发关闭。

失败排查已发现Windows非阻塞listener的accepted socket未恢复blocking、取消半帧后对EOF和ConnectionReset的不同表现、错误加法预期、测试持有活动OperationContext调用close等具体原因。目录page count、价格单位、来源diagnostic、半帧failover及pin错误继续查生产实现和wire来源；未经证实不能全部归为fixture或直接降低断言。

文档原始记录在 `artifacts/unified/stage117-docs/`。attempt01实际暴露capability-matrix镜像落后，以及public-api内三个源树JSON链接不在MkDocs目录内。同步镜像并将真实源路径写为代码路径后，attempt02的 `check_release_metadata.py`、`mkdocs build --strict`、`check_pages_links.py site` 均exit0；JSON明确记录基于7ab55ba的dirty文档候选。代码修复尚未因此获得行为或兼容验收。下一步先重跑已定位的失败集合，保留每轮失败与新证据，再回到完整矩阵。

## stage118：fixture修正、局部通过及生产缺陷实证

基础提交 `59888d8`，各attempt记录dirty diff全文/hash、SHA、命令、时间、退出码及原始日志，位于 `artifacts/unified/stage118-repairs/`。F10 accepted sockets显式blocking并保留read/write timeout后，`f10-blocking-attempt01` exit0：f10_http27、f10_named_methods6、f10_results23、helpers11，共67项；HTTP隔离子进程另有1项通过。监听器仍nonblocking保留有界accept。

`lib-lifecycle-attempt01` 中lifecycle3项全部通过，证实惰性spawn屏障修正；lib231通过、1失败、1 ignored，剩余为此前被fixture掩盖的retirement race。EOF fixture只允许read返回0或ConnectionReset/ConnectionAborted代表已断连，数据/timeout/WouldBlock继续失败。目录goods请求遵循min(page_size600,remaining record cap100)，只改4项缓存用例中的错误600预期，普通默认600请求不变。`fixture-portability-attempt01` 中EX15、boards16、catalog32、loopback14全部通过。

报价生成器参数以hundredths命名却先除100，随后strict decoder按decimal2再除100。修正生成器直接编码hundredths，纠正hq_helpers里先前补偿此错误的100000输入为1000（10元）；价格生产投影不变。shortline保留StatsResource的`tdx://zhb.zip`来源URI，修正错误裸路径预期。daily行级diagnostic把日期改为Display，实际日期保留ISO形式。独立Python加法与分类器四桶确认F37正确合计2240001，decision-register追加明确取代此前错误补正的记录。`runtime-diagnostic-attempt01` lib232通过/1 ignored、hq_helpers52通过，整体仍exit101。

尚未修正的生产原因已取得具体证据：

1. `mac_session` 半帧EOF返回Protocol，不进入仅对Connection错误的failover；有效完整帧中声明行不足仍应Protocol，两者不能混淆。
2. quotes_batch超时清理返回`Supervisor rejected exact request failure`；新增诊断实测`lifecycle_retirement=false`、`existing_retirement=true`、Supervisor仍Running，即已登记普通取消retirement后worker又报告失败，重复begin_retry被拒。尚需确定性回归及精确wire校验修正。
3. resources lost-pin错误为fallback5字节大于末页请求1字节；计数明确primary2请求、fallback1请求，固定连接确实发生了换源，不是单纯错误断言。尚需禁止pinned请求重连/重试并验证原Python运行时合同。

此检查点保存已验证的部分修正及失败，绝不标记整体验收。下一阶段先写确定性回归、实际确认red，再修生产原因，然后统一重跑完整SDK及下层运行时和Python兼容等矩阵。

## stage119：生产退役、固定来源与半帧断线修复

基础 `74d14b0`，记录位于 `artifacts/unified/stage119-runtime/`。新增运行时确定性用例先实际red：取消已先登记普通Terminal retirement，随后同wire失败再begin_retry返回Internal；修正后green1项，另验证外来generation不能借用旧retirement。原取消原因、精确wire及真实退役确认均保留，不替换为迟到失败原因。首次运行前还修正原有runtime用例的channel错误类型推断和两个Result<Option>误用，原编译失败日志保留。

HQ Rust RequestPermit的pinned提交使用一次物理尝试，沿用原authorization和绝对deadline；普通Rust及PythonLegacy的原重试策略不变。lost-pin公开用例第一次新结果已通过业务断言但fixture主动shutdown后仍写回复，修正fixture显式disconnected状态后attempt02 exit0：primary恰好2请求、fallback0请求，连接丢失不能继续拼文件。扩展transport将未收齐声明帧前的实际EOF归Connection，保留received_bytes及UnexpectedEof source；完整帧的结构/声明行不足仍Protocol。公开MAC partial failover及EX/EX wire/catalog/download五target共107项通过。

合并命令为 `cargo +1.89.0 test -p eltdx -p eltdx-runtime -p eltdx-protocol --no-default-features --features eltdx/full --locked --no-fail-fast`。attempt01暴露两个runtime integration用例的IO错误未转换和SubmissionOptions移动后复用；修正后attempt02完成：SDK所有常规target和30 doctest通过、protocol98通过、runtime lib129与deadline5通过；整条命令仍exit101，唯一失败pin_handoff fixture只accept未回复HQ握手。补完整握手后又实测即时broker缓存断言返回旧0；pool_diagnostics源码明确读取25ms周期缓存。用实时source_identity检查已交付pin取消后仍有效、下一pin排队、原pin关闭失效和下一pin交付/释放替代不可靠的缓存断言，pin-handoff-attempt03 exit0（1项）。这不是放弃独占/释放合同，验证改为运行时实时身份与排队行为。完整合并命令尚需检查点后获得同候选exit0，警告及ignored压力仍待。

WSL实际Ubuntu22.04，任务专用 `/root/.cache/eltdx-sdk-validation/` 下安装Rust/Cargo1.89，install attempt03 exit0，先前Windows到WSL参数转义与CRLF脚本失败保留；没有更改默认用户toolchain或PATH。工具安装不是Linux行为验证。后续继续Windows完整/feature/Loom、Linux真实执行、PyO3/Python、独立消费者/包/文档/压力/性能与生产协议证据，整体目标仍未验收。

## stage120：Windows full合并通过与Python绑定编译修复

干净来源 `1d616698971172621eca9f64bb41adf4ebb1f6c8`，Windows/MSVC Rust1.89的 `cargo +1.89.0 test -p eltdx -p eltdx-runtime -p eltdx-protocol --no-default-features --features eltdx/full --locked --no-fail-fast` 实际exit0。记录 `artifacts/unified/stage120-matrix/windows-full-attempt01.*`：SDK lib232通过/1 ignored，30 doctest、7 serde合同，以及protocol98、runtime129、deadline5、pin_handoff1通过，其他普通integration target同样没有失败。这里不把未独立汇总的全目标用例计数相加；ignored压力没有执行。仍有warning，没有据此声明clippy/fmt或minimal/default/feature矩阵通过。

同一干净来源的Python绑定 `cargo +1.89.0 check -p eltdx-python --locked` attempt01实际exit101，共52个编译错误。根因包括PyO3泛型转换前未确定的数值类型、matches返回Result、Bound/未绑定对象不同签名、list切片和Python比较落到Rust trait、移除的downcast、闭包错误类型推断及跨层错误转换。补明确小整数i32/浮点f64、闭包PyResult、当前PyO3接口；不改变Python小常量的值。attempt02剩3个同类变量推断错误，补width/ratio/value类型后attempt03实际exit0。所有失败、源码diff/hash与新结果都独立保留，未覆盖。当前只是编译通过，实际wheel和兼容行为还待验证。

`run_legacy` 原取消分支把外层mpsc RecvError当作SDK错误，且内层CancelUnconfirmed被忽略。现在分别保留：确认取消/已返回正常结果不增加KeyboardInterrupt cause；SDK CancelUnconfirmed附原SDK错误，通道断开/锁失效附RuntimeError并报告无法确认。真实中断与资源回收仍须由安装候选的Python回环用例验证，不由check推定。

Linux实际首轮 `linux-full-attempt01` exit101，crates.io config.json连接连续超时，没有得到编译或测试结果。attempt02在同一WSL Ubuntu22.04/Rust1.89独立target中，用rsync复制Windows现有registry缓存到任务专用CARGO_HOME，然后 `--locked --offline` 执行相同full合并集合；本检查点尚在运行，未填passed。无系统全局代理、工具链或默认PATH更改。继续实际wheel/完整Python库存和矩阵/消费者/包/压力性能，不减少验收范围。


## Stage121：Python实测与Linux/full、32feature检查

2026年11月17日stage121统一Python与跨平台验收：源码083383113bf297511ed870c508d17f9b4ecfcc4b。Linux Rust1.89/full合并SDK/runtime/protocol在补齐现有crate缓存后locked/offline实际exit0；Windows32feature子集检查exit0。release abi3 wheel构建exit0，wheel SHA256 f5aa24115cc98846eb15b4446741d62425dfccb47dafb9fe771bf07a38e08581。隔离venv安装/pipcheck成功，但公开corporate查询因legacy unpriced admission拒绝而失败，不能算consumer通过。补齐61个未分组文件后122个Python测试文件均归属明确，inventory3通过；golden103通过；完整loopback235通过/41失败，完整local2971通过/17失败。local首轮坏fake目录永不返回空导致无界分页，精确进程记录后停止，修复fake再完整运行；保留失败证据。目录默认1600、普通/退市bar fixture周期与019001旧native解析差异更正后四文件113通过。生产缺口为legacy capital/finance/limits admission与snapshot旧parser、bulk datetime DTO比较和动态transport override；另有测试静态假设/patch作用域/模块导入及全局旧metadata差异待修。原始记录artifacts/unified/stage121-python，各次失败未覆写。30/30仍仅源码范围，整体未验收；目标active无限额，继续修复兼容再跑统一矩阵。


## Stage122：旧profile解析与扩展点修复

2026年11月17日stage122兼容修复：基础a512f4a。legacy corporate/finance/limits增加仅Python profile的unpriced准入与旧parser；strict Rust分支不放宽。快照错误原先经RuntimeError转换后失去ProtocolError来源，修复为保留原始typed source并维持旧分类/context，使旧分裂恢复与单条缺失重新生效。ApiBase每次调用核对内置transport原execute身份，类/实例构造前后替换和恢复都受支持，显式native custom adapter仍保留。确定性red后相关208通过；更新native的四集成文件79通过。批量逐笔原测试混比custom datetime blocks与冻结native lazy tuple blocks，现使用原native per-page bulk路径作参考并追加两者全部公开列相等，不改DTO/dataclass或提前物化。修正pytest模块self函数可收集、ST名称normal=N、MAC hook作用域、offline同名function/module导入及三新增ABI tag/共享buffer静态假设。64模块AST增量逐项审计，旧冻结baseline未改：91新增import绑定、38 sdk方法、2私有DTO helper、1私有__all__及三个可选native_services构造参数，反例防旧契约漂移；13项合同通过。候选wheel attempt02成功并核对native hash41d4ef1f977c4f5ec1dfe0e54314bfbe7870adb3c2ca9d410e7f8b192404e054；任务venv安装3.3.5成功，解决系统旧metadata3.3.2，不改生产版本逻辑。实际新完整Python集合/干净wheel消费、受影响Rust重跑及剩余交付矩阵仍待。证据artifacts/unified/stage122-compat，失败轮次保留。goal active无限额。


stage122完整集合复验补充：任务专用venv装入本轮3.3.5 wheel后，完整round4-local实际3046通过、round5-loopback276通过、round3-golden103通过，均exit0；fixture/静态修复没有改旧golden、模型或版本算法。新AST增量清单当时为untracked，runner diff未含该新文件，故单独保存其SHA256至stage122-compat/ast-additions-identity.json并随本检查点提交。受影响pure Rust/新干净wheel消费及其余矩阵仍待，不据此整体验收。


## Stage123 candidate evidence and cancellation race

2026年11月17日stage123干净候选验收：来源92f033b77adb1d041fc819fbeacc3af60d06c9d2，dirty=false。Windows及WSL Ubuntu22.04/Rust1.89的full合并SDK/runtime/protocol命令均exit0；Windows Loom也exit0（runtime131含两模型、deadline5、pin1）。release wheel重新构建成功，独立venv/no-index/pipcheck与非空HQ/F10/EX/MAC/MAC-EX公开消费exit0；隔离仍仅process-path-only，不是物理无编译器镜像。Windows平台矩阵protocol/runtime、minimal、default完成exit0，但full/all-targets在operation::tests::runtime_two_page_comparison_rust_times_out_both_python_profiles_keep_request_windows偶发失败：业务Timeout之后close得到started cancellation was stale。需确定性回归并修生产退役仲裁，不能把先前full通过视为消除竞态。独立严格Clippy失败于protocol四项warning，fmt --check也exit1；保留原始日志。证据artifacts/unified/stage123-candidate，各attempt不覆盖。忽略压力尚未运行，feature/stable/外部Rust包/平台/压力性能与真实服务器证据仍待。目标active无额度限制，无代理或发布。


## Stage124 late cancellation arbitration

2026年11月17日stage124退役仲裁修复：基础b4ffd32。确定性late cancellation回归在生产修复前实际失败started cancellation was stale（red-attempt02；attempt01因遗漏完整模块名而0项，不能算red）。Supervisor只允许精确wire转terminal意图；Runtime取消加入已有清理，先发生的失败终态保留原原因，待retry/endpoint清理则转取消并禁止新attempt，确认仍等待真正ReconnectAck，owner lifecycle清理保留原归属。新用例覆盖普通/pinned、terminal/retry/endpoint六种顺序及外来generation拒绝，green通过。带最终测试增量的Windows Rust1.89合并all-targets实际exit0：SDK lib232/1 ignored、protocol98、runtime130、deadline5、pin1及全部普通integration/examples成功；不包含doctest/ignored压力。证据artifacts/unified/stage124-retirement。Clippy/fmt、Linux/绑定/Python重新构建、完整交付及压力性能仍待。目标active无限额。

## Stage125：严格Clippy与恢复检查点

2026年11月17日，基础为取消竞态修复0b4bd48，Rust1.89的全工作区格式化独立保存于c62716e428be4190cfee3181a3ea52d3db26fe20。后续语义lint修正没有混入该格式化提交。运行证据位于 `artifacts/unified/stage125-lints/`，每轮原始失败、dirty diff/hash、命令和退出码保留。

`all-targets-attempt11` 的完整命令为 `cargo +1.89.0 clippy -p eltdx -p eltdx-runtime -p eltdx-protocol --all-targets --locked --no-default-features --features eltdx/full -- -D warnings`，实际exit0，来源c62716e加记录的dirty diff，SHA256为516bb299f2a73c9d27a4229d4bcad0314e51adc4fd83cf4715e53b73275ce773。前轮attempt10仅剩测试helper类型复杂度，使用PinReservationRuntime别名后通过。`--fix`成功不算严格lint成功；本结论仅引用未使用fix的attempt11。`format-apply-attempt02` 与独立 `format-check-attempt01` 均实际exit0；后者完整执行 `cargo +1.89.0 fmt --all -- --check`。

修正边界：移除未使用的私有包装/字段/事件分支，实际unstarted取消仍由UnstartedRejected负责；仅单元测试使用的helper限定cfg(test)，集成支持文件的dead_code例外仅限测试共享fixture。测试unwrap/expect许可限定测试目标。生产动态不变量改为typed Internal，固定常量/内建表与高参数数/大枚举仅在具体声明处注明例外；没有全局生产warning豁免。RawResultSender私有send不再返回无人消费的大型SendError payload，而返回送达bool，终态发布顺序不变；对应测试验证真实接收者和终态。私有metadata sponsor helper消耗Self，保持原授权/期限路径；订阅cursor保留先完整校验再修改。

`python-contract-source-attempt01` 实际603 passed、exit0，使用任务venv内先前候选native。它证明本轮Python源码合同通过，不能替代本轮Rust修改后重新构建的wheel/绑定行为验收。此前stage122的local3046/loopback276/golden103及stage123的Windows/Linux/full、Loom和独立wheel消费者仅证明各自记录候选；stage124修复后Windows/all-targets通过也不能推定本次lint修正的运行行为。下一阶段保存新候选后重跑完整受影响Rust正确性、doctest、feature/Loom/Linux，重建wheel并回归Python，再继续外部Rust消费者、包/文档、压力和性能。外部平台/真实协议缺口保持明确，整体目标active且无额度限制。
