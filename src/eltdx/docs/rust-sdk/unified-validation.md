# 全源码门槛与统一验证记录

## 当前结论（2026-11-18）

T01–T30源码实现完成。记录范围内的Rust、Linux、Windows Python 3.10–3.14、压力/性能和stage138本地归档consumer均有通过记录，但属于各自候选SHA与平台的证据，不拼成一个未执行的单候选总矩阵。发布平台隔离、macOS/非Windows产物、授权后的公开registry消费和生产协议样本分属后续外部验收；文档-only变更不触发Rust/Python全矩阵重跑。以下按阶段保留历史原始结果，当前分层结论见[最终验收索引](final-acceptance.md)。

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

## Stage126：完整候选复验与交付工具修正

2026年11月17日，干净候选db82b29f66de15bac3d4ca65ab361b81e3b40e66，证据目录artifacts/unified/stage126-candidate。Linux Rust1.89/full合并SDK、runtime、protocol命令实际exit0，包括30 doctest；Windows相同命令exit101，唯一失败是deadline测试在操作完成后立即把25ms缓存broker快照当成实时租约。单独原用例又通过；源码确认pool_diagnostics只读缓存。测试现以同一socket额外非空请求、单Slot实时pin取得/身份/释放证明租约回收，再确认close、peer EOF、STOPPED及raw/decoded零。首次修正误要求关闭后仍有broker快照，green01实际失败；broker epoch在停止后消失，改为实际None合同后green02五项通过，runtime全目标严格Clippy也exit0。没有为此改变生产调度或增加sleep。

Windows Rust1.89的32显式feature子集、default/full以及最小/full生产依赖闭包全部命令exit0；部分非full组合仍有普通dead_code warning，本记录不称其严格lint通过。独立release wheel构建成功，wheel SHA256为4d48d03814eb8239549f36b729eb0017cc69fd292ee95499ab523ba031f46807，native为73f787e5d505848fe139322f264538e31ee0ba66c3eee3b1d4b0fcfa88cbda4c。任务venv和checkout native均换成该产物，完整Python local3046、loopback276、golden103均exit0；外部隔离venv/no-index/pipcheck/非空HQ/F10/EX/MAC/MAC-EX消费也exit0，隔离结论仍为process-path-only。metadata、MkDocs strict、HTML本地链接检查均exit0。sdist构建与inspect通过，实际从sdist重建wheel仍在session64112执行，观察确认编译器仍活跃，不能重启或填passed。

旧性能基线af471f1adf38757f6aa0188968db8110eb23f6ad已在独立managed worktree C:/Users/ax/.codex/worktrees/rust-sdk-performance-baseline/tdx抓包成功构建release wheel，同Rust1.89/Python/固定PROFILE，旧新lock各自保留，不伪称依赖完全相同。该检出只供基线，不把当前源码退回；后续所有实施命令继续显式使用原rust-sdk-implementation目录。WSL Python3.10缺ensurepip，改在任务cache内用virtualenv构建验证环境，psutil6.1.1/maturin1.15.0已安装；未改系统Python或默认工具链。

registry-observation实际读取三名称均404，保留HTTP响应/hash与UTC；不等于命名权或发布成功。首次crate archives在runtime打包失败，Cargo即使--no-verify仍从注册表解析未发布protocol，workspace patch不足。独立directory-packaging-probe实际使三个归档依次成功，正式工具现改为vendor锁定依赖、staging局部目录源、逐个注册已审查的下层归档及真实checksum，再由外部消费者只patch解包内容。正式脚本修改尚须在保存的干净新候选上完整重跑；不以探针替代消费者行为验收。包内不删版本/path验证，未实际publish/push/tag/merge。

接续：保存本轮测试/工具修正，执行新候选archive consumer、Windows平台/strict docs/外部Rust consumer、Loom/stable及受影响Linux回归；新候选wheel与Python结果按源码影响重建/补齐，再实际运行两平台默认压力、84trial性能及原始日志复算。外部macOS/多Python版本/真实生产协议等缺口按要求继续单列。source-complete不等于整体验收，goal active、无额度限制；所有失败attempt保留。


## Stage127：独立归档与当前 stable 实测

2026年11月17日，干净来源7f1f491f554cfa87433b1a4213ac828760be3fa3。artifacts/unified/stage127-delivery/linux-full-attempt01实际exit0：Linux/Rust1.89/full合并SDK、protocol、runtime与30 doctest均通过。新release wheel也实际构建成功，perf-candidate-build/wheel-build.json记录产物身份；checkout和任务Python测试环境仍使用db82b29的native，不能把旧Python通过投射到此产物。stage126的sdist-rebuild-attempt01现已结束exit0，实际从1437文件、261文档的sdist重建Windows abi3 wheel；此前“仍运行”是当时状态。

正式crate-archives-attempt01现已生成、检查三个归档，并基于实际解包归档构建外部最小Rust消费者；整体仍失败于loopback的单连接断言。独立诊断确认两个实际socket：第一页044d走普通池，后两页走endpoint池，054c又复用普通池，不是fixture唤醒连接。Windows/Rust1.89的platform-attempt01中普通测试、minimal/default/full、Serde、五种严格rustdoc、SDK全目标严格Clippy、fmt及最小/full依赖闭包均exit0；整条矩阵最后也失败在外部loopback同一断言。三归档通过不能代表消费通过。

实际当前stable为官方Rust1.100.0（2026-11-12），不是原用户stable1.98。Windows rustup直连attempt01被socket10013拒绝；attempt02经官方manifest/component SHA256验证后安装到任务局部目录并链接current-stable，未改默认工具链。Linux任务局部stable同为1.100。Windows current-stable-platform-attempt02与Linux current-stable-full-attempt01的功能测试均通过；整个流程均因严格Clippy失败而非通过。已知新诊断为固定chunks_exact可用as_chunks、可简化的Option分支，keep-going补充protocol测试中的is_multiple_of；下一步须逐层修正并同时验证MSRV1.89，不能据普通测试通过跳过strict gate。

实际默认原生压力仍0次、性能84trial仍0次。macOS、多Python版本、物理无Python/编译器环境与生产协议证据仍待。所有失败独立保留，不发布、push、tag或merge。


## Stage128：单服务器分页连接复用

基础7f1f491。普通HQ分页在第一页后无条件取得另一endpoint-bound Engine，即使普通目录仅有该唯一endpoint，仍额外占用物理连接。外部消费者单连接断言实际失败；新公开codes回归配置pool_size=1/max_connections=1，pagination-red-attempt01又实际复现第二页LimitExceeded/owner_connections。services::endpoint_engine现在在验证来源属于目录后，仅当目录只有一个endpoint时复用普通Engine；多源目录继续使用原固定来源池。没有提高连接限制或删除原消费者断言，也不改变PythonLegacy运行时路径。

pagination-green-attempt01首次已完成扫描，但新增显式cursor测试误用默认1600大小续读原page_size1游标，实际InvalidArgument；修正测试保持原page_size1后green02为6通过。测试逐请求断言0/1/2扫描和0/1显式续读均为同一物理连接身份。原外部最小消费者consumer-loopback-green-attempt01实际exit0；HQ七target回归hq-regression-attempt01为59通过，SDK/full/all-targets严格Clippy也exit0，均为记录dirty diff的局部证据。归档来源尚未重打包，完整新候选矩阵仍待，不能据局部通过称正式归档流程通过。

接续：先修Rust1.100新增strict lint且保持MSRV1.89，再在干净提交跑完整归档consumer、两平台矩阵/32feature/Loom与受影响Python产物，之后运行默认两平台原生压力和84trial性能。记录的失败/命令/日志/hash位于artifacts/unified/stage128-single-endpoint，统一验收继续，目标active无额度限制。

## Stage129：1.100严格lint与MSRV兼容修正

2026年11月17日。stage129第一次Linux当前stable严格Clippy（attempt01）确定了固定宽度`chunks_exact`、Option末支及其余稳定lint；Rust1.89协议测试98通过。将固定数组读取改`as_chunks`、Option分支用`?`、固定周期用`is_multiple_of`；range-loop改带来源索引的迭代器，枚举手写默认改语言内置derive，测试fixture与重复属性同步清理。try_update虽是1.100新建议，但独立rustc probe证实该API在1.89为unstable，因此保留`fetch_update`语义并只在五个精确调用点注明MSRV限制，未全局屏蔽deprecated。失败attempt01/02/03/04和试验编译错误均保留。

完成这些修改后，Linux当前stable1.100 full/all-target strict Clippy attempt05实际exit0；Windows MSRV1.89 full/all-target strict Clippy attempt03实际exit0。MSRV full合并SDK/runtime/protocol测试在较早stage129 diff attempt01实际exit0（SDK lib232通过/1 ignored，全部常规集成目标、protocol98、runtime130、deadline5、pin1、30 doctest通过）；其中当前diff的严格lint没有改变业务算法。Windows MSRV及1.100整平台矩阵、Linux MSRV/current-stable full已在最终修订diff运行中，结果以其JSON/日志为准；旧stage129 diff不自动代表最新diff。归档attempt01因未提交工作树被脚本正确拒绝，没有生成任何crate archive。

当前strict lint / Rust1.89 full测试通过。正式archive/外部consumer、两种stable 32-feature矩阵、Loom、候选wheel与本机Python全套、无Python环境、两平台原生压力和84trial性能仍未完成。不得把局部严格lint/full test扩大为统一验收。

## Stage130：候选复验结果与最终矩阵

2026年11月17日，检查点提交 `2f3526169e846a07943b2f5e0084312f24aa94cb`。Windows x64/MSVC Rust 1.100.0 platform attempt01 保存于 `artifacts/unified/stage130-clean-candidate/windows-current-stable-platform-attempt01/`，最后仅在fmt失败。attempt03复用专用Rustup toolchain但联网consumer锁生成超时；attempt04启用Cargo离线，完整矩阵31个Cargo门禁全部exit0，包括protocol/runtime、minimal/default/full SDK、Serde、strict rustdoc、strict Clippy、fmt、依赖闭包及所有consumer模式。当前candidate即attempt04成功。另用系统Rust 1.98.0 执行platform attempt02也通过31个门禁，记录在 `windows-stable-platform-attempt02/`，作为额外兼容性数据，不替代官方1.100和MSRV1.89矩阵。

同候选的Rust-only PATH隔离消费者 attempt01 在 `cargo generate-lockfile` 请求crates.io `config.json` 时连续超时，未进入consumer构建；原始日志和失败状态保留在 `rust-only-path-isolated-attempt01/`。离线attempt02现已exit0：最小及full依赖树均无Python/PyO3/maturin；外部Rust consumer的lazy、loopback、offline、financial、F10、EX、MAC、MAC-EX模式，以及offline/F10/EX/MAC禁用模块的负向检查和启用检查均通过。该检查仅证明PATH不暴露Python可执行文件，不能替代物理隔离的无Python容器。stage130文档metadata/MkDocs/site links attempt01在旧检查点通过；加入本次日志后将重跑正式文档门禁。

Windows MSRV1.89 platform attempt01执行于 `2f35261`，协议/runtime、minimal/default/full、Serde、五组strict rustdoc、SDK全目标strict Clippy及所有运行测试/doctest均通过；最后 `fmt --check` 因Rustfmt1.89与1.100对 `eltdx::__python_compat` 和 `eltdx::RawPolicy` 的排序相反而失败。现在测试改为crate-qualified路径，Rustfmt1.89和1.100独立的全仓fmt命令均exit0；完整MSRV平台矩阵待新提交后复跑。旧失败记录保留在 `windows-msrv-platform-attempt01/`。

## Stage131：Rust 1.89 / 1.100 最终平台矩阵

提交 `50e97c2469319e78a3b1309b84e67871f2c61d79` 的Windows x64/MSVC最终平台矩阵均通过，完整报告及逐命令日志分别位于 `artifacts/unified/stage131-clean-candidate/windows-msrv-platform-attempt01/` 与 `artifacts/unified/stage131-clean-candidate/windows-current-stable-platform-attempt01/`。Rust 1.89 使用主机已安装工具链；Rust 1.100 使用任务局部 `current-stable` toolchain。两个矩阵都在Cargo offline模式下执行，各有31条门禁命令、全部exit0：协议/runtime测试、minimal/default/full SDK行为与all-targets、Serde合同、五组严格rustdoc/doctest、SDK full all-targets strict Clippy、fmt、最小/full normal+build依赖闭包，以及lazy/loopback/offline/financial/F10/EX/MAC/MAC-EX外部consumer。两个结果的 `source_sha` 相同且工作区均为clean。

这些平台矩阵中的candidate-only ignored压力用例尚未执行。较早提交 `2f35261` 的Rust 1.100 attempt04、Rust 1.98 attempt02与Rust-only PATH consumer attempt02仍作为补充证据保留；新候选仍需运行两版32 feature子集、Loom、正式crate archives/解包consumer、文档站点检查、最终wheel与完整Python回归、Linux行为、两平台默认原生压力及84-trial性能记录。物理无Python镜像、macOS、多Python版本和生产协议抓包仍需要外部环境/证据。整体验收仍active。

## Stage132：Feature、Loom 与实际归档消费

Windows feature matrices 在 `af0e9cf5adcabed49928f6d1523325eb9539364f` 上以离线模式分别使用Rust1.89及任务局部Rust1.100执行，各37条cargo命令全部exit0。矩阵包含五个feature的32种显式子集、default/full、最小/full normal+build依赖树及对Python/PyO3/maturin的排除断言。非full feature组合可能显示dead-code warning；这里通过的是编译检查，不把它描述成各组合的strict lint。报告见 `artifacts/unified/stage131-clean-candidate/windows-msrv-features-attempt01/matrix.json` 和 `windows-current-stable-features-attempt01/matrix.json`。

同一候选的Windows Loom运行 `cargo +1.89.0 test -p eltdx-runtime --features loom --locked` 与任务局部 `cargo +current-stable ...` 均exit0；每个工具链都实际运行132个runtime单测，其中两项Loom并发模型成功，另有deadline5和pin handoff1成功。命令日志及SHA见 `artifacts/unified/stage132-checks/loom-results.json`。这不是socket或跨平台压力证据。

正式 `rust_sdk_packages.py archives` 在干净候选 `af0e9cf5adcabed49928f6d1523325eb9539364f` / Rust1.89上exit0：协议、runtime和SDK三个实际 `.crate` 均已生成并检查，基于归档逐文件checksum登记staging directory source，再仅从实际解包内容构建外部消费者。24条工具命令全部通过，其中11次consumer运行覆盖最小、loopback、default、full及F10/offline/financial/EX/MAC/MAC-EX；报告为 `artifacts/unified/stage132-checks/windows-archives-attempt01/packages.json`。这是本地归档和patch模拟消费，不是发布、注册表解析、无patch消费者或注册表身份验证。

`artifacts/unified/stage132-checks/docs-attempt02/results.json` 绑定于 `031e5aedb636c88ce8e9da9747790a3fc2af1cfd`，`check_release_metadata.py`、`python -m mkdocs build --strict` 和 `check_pages_links.py site` 均exit0，日志SHA256随报告保存。attempt01没有有效汇总，因为PowerShell包装器未预建静默成功命令的空日志文件；它不作为通过或失败的文档证据。最终wheel/Python、Linux行为、两平台压力、84-trial性能和外部环境缺口仍未完成。

## Stage133：最终 Python wheel 与兼容回归

干净提交 `ef6b243bfc67a9f0d4dde8538967b911a42218ed` 在Windows x64/MSVC、Rust1.89、Python3.12.6下执行 `maturin build --release --locked`，离线构建exit0，生成 `eltdx-3.3.5-cp310-abi3-win_amd64.whl`。wheel SHA256为 `e4b47923031756f514e34171d55aefc1bcbea08c93db17348a1eb4c55adb36fa`；封装及checkout测试路径的 `_native.pyd` SHA256均为 `465e28d1dc950a97ce36bb3ee82766c6aebe52448d3486fad5203b67c030de66`。之前被忽略的checkout native已在构建前备份到 `artifacts/unified/stage133-final-python/prior-checkout-native.pyd`，测试后续都使用候选wheel同字节native。

`round4-local-attempt01` 有3045通过/1失败，唯一失败为全局系统安装ELTDX metadata 3.3.2而源码候选3.3.5；失败原始日志保留。task venv从候选wheel本地安装3.3.5、保留host pytest后，完整round4-local attempt02为3046通过/0失败。相同候选与task venv上的round5-loopback为276通过，round3-golden为103通过；各组日志、计数、source SHA、安装version及native hash记录在 `artifacts/unified/stage133-final-python/{round4-local-attempt01,round4-local-attempt02,round5-loopback-attempt01,round3-golden-attempt01}/result.json`。

独立checkout外wheel消费位于 `wheel-consumer-attempt01/installed-wheel.json`：只用本地 `--only-binary` wheel安装，`pip check`、public API consumer及wheel/installed native字节hash比对全通过。外部临时venv经PATH隔离后找不到rustc/cargo/maturin，证据明确标为 `process-path-only`，不代表物理无工具链镜像。最终wheel、local/loopback/golden和独立消费的整体候选门禁通过。Linux行为、Windows/Linux默认原生压力、84-trial性能仍待；macOS、多Python版本、生产协议抓包与物理无Python镜像仍是外部环境证据缺口。

## Stage134：Linux 最终行为与两平台默认原生压力

2026年11月18日，干净候选 `588d5e439bffe5a61e8ff21c2d7e7f1e658686d4`。Linux Ubuntu22.04/Rust1.89在任务专用工具链及离线registry cache中运行full合并命令（SDK、runtime、protocol；`--no-default-features --features eltdx/full --locked --offline --no-fail-fast`），attempt02实际exit0，65个target汇总1111项通过，含SDK 30 doctest；明确ignored压力由独立负载执行，不记作普通测试通过。Linux Loom命令 `cargo +1.89.0 test -p eltdx-runtime --features loom --locked --offline` 实际exit0：132单测（含2模型）、deadline5、pin_handoff1。完整日志、逐target计数、lock/source/log hash见 `artifacts/unified/stage134-linux-final/results.json`。

原Windows worktree的Git指针不能由Linux原样解析，先用Git archive导出干净候选到Linux本地文件系统，Windows/WSL核对源档SHA256同为 `1dc997c2f2fe498b02c0122ae9a8f54b6db2ed87bcae79e0595205be594684f2`。full attempt01在启动时找不到cargo而exit127，没有进入编译；attempt02使用显式CARGO_HOME/RUSTUP_HOME/RUSTC/RUSTDOC后完成，原失败未覆盖。压力需要Git commit/tree/blob证据，另建 `/root/.cache/eltdx-sdk-validation/stage134-linux-checkout` 的共享clone，HEAD与候选一致且clean；没有把解包源档冒充Git checkout。

同一候选在Windows x64/MSVC及Linux WSL2分别执行 `rust_sdk_stress.py run --implementation-complete`，均使用Rust1.89/full/release/default counts，十个case全部exit0，runner原始事件与资源日志复算均为passed，随后各自独立 `verify` 实际exit0。Linux close为10000次、Windows为2000次，按事前平台政策执行；unique125000、inflight2000、subscription/limits/retry/financial/f10各1000、mac_cache500、budgets32000的终态数不减少。每个负载有独立预热，预热不计业务终态；物理请求、业务identity、取消确认、关闭、raw/decoded预算、线程/socket/FD或handles与RSS阈值均从原始记录重算。

Linux本地原始证据保留于 `/root/.cache/eltdx-sdk-validation/stage134-linux-stress-attempt01`，副本归档为 `artifacts/unified/stage134-linux-final/linux-stress-attempt01.tar.gz`（SHA256 `7b7aa9c1270df191530eb27a83f6d88229126cece663e9b58aca4b0303e00b16`），归档包含原始日志、资源、事件及二进制hash，排除target目录，实际二进制保留在原Linux目录的target中。Windows原始证据为 `artifacts/unified/stage134-windows-stress-attempt01`；两边副本位于 `stage134-stress-inputs`，聚合器核对相同候选tree/源码blob/lock/fixture/script并再读两边全部原始数据，最终结论以 `stage134-linux-final/native-stress-aggregate.json` 与退出码为准。

跨平台aggregate实际exit0，报告status=passed、两平台各10场景。Linux全部case最高RSS 16695296B、线程46、FD20、冷却RSS增量8060928B；Windows对应19496960B、线程50、handles169、冷却RSS增量2314240B，均满足事前政策。两边budgets峰值raw220B、decoded12582912B、queue28、live_actors/configured_workers4；结束后预算和活动actor/queue归零。这里是采样和合成负载证据，不推定生产服务器长期资源行为。

候选588d5e4上的metadata、MkDocs strict和站点本地链接命令也均exit0，记录在stage134 results.json。性能对照见stage135；该文档三门禁先于stage135文档修改，stage135提交前按新文档重跑。Windows两版Rust平台/feature、归档consumer及最终Python回归仍以stage131–133各自候选为准；其后到588d5e4仅文档/恢复记录变更，没有扩大旧结果的源码范围。macOS、多Python版本、生产协议、物理隔离环境及发布后无patch注册表消费继续单列；目标active，无额度限制。

## Stage135：Windows/Linux 配对性能测量

候选 `a3cc4c2317f4714b0e80de29d8f899aef664a59b`，冻结基线 `af471f1adf38757f6aa0188968db8110eb23f6ad`。两者各自由 `maturin build --release --locked` 构建；Windows/MSVC使用Rust1.89/Python3.12.6，Linux Ubuntu22.04使用Rust1.89/Python3.10.12。候选与基线保留各自Cargo.lock hash，不宣称依赖完全相同。每平台固定profile opt-level3/debug0/LTO=false/codegen-units16/incremental=false/panic=unwind，500 samples、50 warmup、7个独立trial/曲线，交错顺序，共84次run；外部网络状态unrun。

Windows run和raw verify都exit0：`artifacts/unified/stage135-performance/windows-measurement/`，candidate wheel SHA256 `a34e90464391c556648fa076ec4e640885be2c2a1e52ce2432d6b8d1c72f2afd`，native `5d997d44a83869b994bcf7555defd7cc5d8d84535a3d706682c437a80f4d7640`；baseline wheel SHA256 `81caf775f94aebb212fbb5397bc88b836ad797382e3a0995e9f290a1b441aca0`。84/84 run无错误，外部测量未运行。

Linux baseline/candidate build、run、raw verify均exit0，Linux结果压缩归档解包后的独立verify也exit0。完整压缩raw证据 `artifacts/unified/stage135-performance/linux-measurement-attempt01.tar.gz` SHA256 `639ebb1898ffb3135673d0e522c4680e0c8cc3621f7233e8d20d8fa0f75cce0e`；candidate wheel SHA256 `8f8940e711527073431a050d303f43c3bc8540796e8c91ec61f53ff4d8c09955`，native `a0af568a32a3fcf24c58e433d61afa97337b56b572dfff65e35cd458ff0a4f57`；baseline wheel SHA256 `47c3da197167894f82c3f0b6c252357dec7c16490b22e1621662f0d7ad3dbff6`。摘要、各项锁及log hash在 `artifacts/unified/stage135-performance/results.json`。

candidate/baseline p50比率（>1表示候选慢）为：Windows parse1.0043、projection1.0049、loopback2.3671、close1.0570；Linux parse0.9628、projection0.9752、loopback1.8178、close1.0368。loopback测的是本机合成服务器，候选额外绝对p50约0.17–0.21ms；它不代表公网请求或真实服务器吞吐。Rust core parse严格策略p50另行记录，不与legacy Python策略作减法。结果按测量政策完整报告，不设必须提速门槛，也不以比值替代API兼容、生命周期或产品验收。

失败准备attempt均保留：Linux安装venv首轮代理超时、绕过代理后因旧apt索引404、更新索引后安装成功；baseline构建attempt01使用了不存在的staging路径，未开始编译，attempt02成功。Windows当前候选wheel build通过。Stage135仍不补齐macOS、其他Python版本、物理无编译器镜像、注册表发布后无patch消费、生产MAC握手/category22/涨跌停权威规则等外部证据。

## Stage135 文档审计与门禁

已在[最终验收索引](final-acceptance.md)为R01–R12逐项记录局部证据和未闭合条件，全部保持“部分”，不将本地fixture、不同候选或缺失的生产/平台证据升级为全通过。stage135文档更新后，`check_release_metadata.py`、`mkdocs build --strict`、`check_pages_links.py site`及`git diff --check`最终结果和日志SHA256保存在`artifacts/unified/stage135-docs-attempt04/results.json`。attempt02曾因T30索引的中文fragment与MkDocs生成id不符而失败；修正为`stage135windowslinux`后通过，失败日志保留在attempt02。

文档检查点`8e4a7ca1196bc239095dc72903ad81cf919d0e76`提交后再次执行上述四项命令，均exit0，见`artifacts/unified/stage135-docs-attempt05/results.json`。随后对照Linux原始构建报告更正本页误抄为Windows值的Linux native哈希；原始性能报告和性能数值未变。更正后的最终文档门禁记录为`artifacts/unified/stage135-docs-attempt06/results.json`，以其中实际退出码和文件hash为准。

## Stage136：Windows Python 3.10–3.14

候选提交`e2b8819c4729de28f0472ecbe743b0c2e100872f`只调整跨解释器测试假设，没有改运行时代码。由该候选构建的Windows x64/MSVC abi3 wheel SHA256为`82b0bf70a847ea7775079cb4d958130cfca850e9ca515ebf1e38b686de65fd3e`，native `_native.pyd` SHA256为`465e28d1dc950a97ce36bb3ee82766c6aebe52448d3486fad5203b67c030de66`，Cargo.lock SHA256为`214eecd8f24fd6c3a2bafc519087e608e0cb2df4fa9c409db33bfba19c475143`。Python3.10.22、3.11.18、3.12.6、3.13.18和3.14.10均安装了同一wheel，`pip check`均exit0，实际native hash均相同。

五个解释器各自执行`round4-local`、`round5-loopback`、`round3-golden`，分别为3048、276、103项通过，合计17135 passed、0 failed、0 errors、0 skipped。每个解释器的临时独立wheel consumer完成本地`--only-binary=:all:`安装、`pip check`、公开API非空TCP回环查询及关闭，三个命令均exit0。consumer工具链隔离记录为`process-path-only`，不等同物理无Rust/cargo/maturin镜像。

Python版本差异修正在测试层：AST快照不再依赖`ast.unparse`的括号格式，Enum反射忽略CPython内部MRO/签名细节；长度提示与datetime错误断言改为结构化或对照当前解释器行为。3.14首轮失败日志保留，修复后的完整五版本复验通过。逐版本JUnit hash、安装/native身份、consumer命令和报告SHA汇总于`artifacts/unified/stage136-python-matrix/results.json`（SHA256 `d0ea046b18f962ee719647c626186aa4bdc0a2b855162c1729bdca0476e846b2`）；各原始日志与JUnit在同目录的`python-3.10/`至`python-3.14/`。

该矩阵补齐了Windows上的声明Python版本范围，不补齐macOS/非Windows wheel、物理隔离环境、公开registry无patch消费、生产MAC/setup/category22样本或权威涨跌停规则。统一验收仍active；这些外部证据缺口维持未闭合。

stage136文档 attempt01 针对本节与验收索引的更新执行了`check_release_metadata.py`、`mkdocs build --strict`、`check_pages_links.py site`及dirty-tree `git diff --check`，四项均exit0，逐命令SHA和日志见`artifacts/unified/stage136-docs-attempt01/results.json`（文件SHA256 `1d8c0013c9efa9af1e78e87983c5c9dc45c31466067b12392b5b2f5a5ecd8d81`）。恢复提交`fa9355e`之后再次执行相同四项门禁，均exit0且Git工作树clean；逐命令日志及hash见`artifacts/unified/stage136-docs-attempt03/results.json`（文件SHA256 `33ceffc0a69374ca7b74c76a74c0d3d74966de5e7ca2249ad885111621b0e41a`）。attempt01保留dirty-tree证据，attempt03绑定已提交的文档树，不相互覆盖。

## Stage137–138：Registry观察与当前归档

提交`aa4255a2e9baeb547ae70f30a67b671bef5cef99`上的只读crates.io观察于`2026-11-17T17:55:41Z`完成：`eltdx-protocol@3.3.5`、`eltdx-runtime@3.3.5`和`eltdx@0.1.0`均HTTP404。原始响应、Rust1.89版本、候选lock SHA和退出码见`artifacts/unified/stage137-registry-observation-attempt01/packages.json`（SHA256 `4cb9969e56817b5149ffeb9f5e1dbf9229d6dc9f068c0e2c4fa073692c360f91`）。这只表示所记录时点查无该名称，不证明名称可发布或账号拥有权限；没有执行发布dry-run或上传。

同一源码树的`rust_sdk_packages.py archives` Windows/Rust1.89 attempt成功：24条命令exit0，生成并检查`eltdx-protocol 3.3.5`、`eltdx-runtime 3.3.5`、`eltdx 0.1.0`三个crate归档；外部Rust consumer从实际解包内容运行11种lazy/loopback/default/full/F10/offline/financial/EX/MAC/MAC-EX模式，均通过。归档SHA256依次为`f12daac55b851830782f8bfed834775ca9acadb37898156f0dc5c633a0150910`、`3d012ac9698de03dc49409f20cfa992fb2b056a834230e80eb0c4354cf6b0c37`、`568ec8eb422337fcd123db3d4cbae50672c1f72456f9173e37722cecb3312cbe`；报告为`artifacts/unified/stage138-archives-attempt01/packages.json`（SHA256 `1519ace848c8809e5a8ad8c337c9a043955f9c26a444c2683471a47f38e1530f`）。这是directory-source与unpacked-patch本地模拟消费，不是registry消费。

发布后无patch注册表消费仍不可执行：当前下层候选名称未出现在registry，实际发布也不在本次授权范围。stage138不提供macOS、物理隔离或生产协议证据；统一验收继续active。

stage139 attempt02在干净候选`a000eaabc7b8e479b9dd288d8a12a548dd090acc`上通过metadata/mirror、MkDocs strict、站点链接和diff-check，报告SHA256 `42732c380e73d5dabbca70d13bc2bdeff7710da63e560e773a97703b82a1374a`。用户中断附近的attempt03结果文件完整，dirty候选四项exit code均为0；保留其记录，不覆盖。验收口径更新后的attempt04–06最终文档候选均在dirty状态下四项exit code为0，attempt06结果JSON SHA256为`424cfed41146d834ffb6f6f0425be4de09baa29b32cb6685bcc5b4cee51fdff8`。随后会对本恢复记录的最终版本执行门禁并提交文档。

## Stage140：当前候选打包与独立 Linux 消费

本阶段候选是干净提交 `16d367e3896247bcd15a89870cf22f482414cfab`，tree `607693b60070f3d3490ee61a809895afe470272a`。它与stage136候选只差文档及恢复记录。Windows锁文件散列沿用stage136；用于Linux的 `git archive` 导出 SHA256 `d1a16c51f674848e85ed73336efa58bd8719cd53428a5ce16cc8c8ba14e2bf39`，导出锁文件的LF字节散列 `589f36a2855fe6f455031205214d7bf274d8b119e443f4190bce5d07bf2b1fb4`。所有下列目录位于 `artifacts/unified/`。

| 项目 | 实际结果与证据 |
| --- | --- |
| Registry 观察 | `stage140-registry-observation-attempt01/packages.json`，SHA256 `5e3283e1c22965b225cc43bbc4a36b96542ad6c849599fcf948717a2e3953722`。UTC 2026-11-17 18:56:33–34，三个crate名称均404，无实际发布。 |
| Windows 归档 | `stage140-archives-attempt03/packages.json`，SHA256 `a5e82fa292f7c95207d742b6ffe706bff420320c72c5666706aae21eb9bb84b5`。24条命令exit 0，3个归档hash均与stage138一致，11种解包消费通过，仍使用局部目录源及patch。attempt01、02被暂停中断，保留原始日志且无完成报告。 |
| 首包发布 dry-run | `stage140-protocol-dry-run-attempt02/result.json`，SHA256 `83ef9df1882e9b04cc70a427b2d976ed5f799def5895eacd19bfb87497358b81`。`cargo +1.89.0 publish -p eltdx-protocol --dry-run --locked` exit 0，标准归档编译通过且Cargo明确跳过上传。attempt01指定offline而exit 101。 |
| Linux x64 wheel | `stage140-linux-wheel-attempt02/result.json`，SHA256 `fc474a5910dc43575bed10add767d12dda97e0dcff1aa1be46ccf1a3f969c2dc`。WSL2 Ubuntu 22.04，Rust1.89/maturin1.15、CPython3.10构建abi3 wheel，exit 0，wheel SHA256 `6303163f21ff7143013ba27a6d1ad44418e8dede07c8f787c5dae237fd8f195e`。attempt01在PowerShell传递shell脚本时失败，未进入构建。 |
| 无 Rust 的 Python 消费 | `stage140-isolated-wheel-attempt02/installed-wheel.json`，SHA256 `2372c2eb531dcbb716f1f2bbf91fba443d01ab1c851ac9725509d93b4cb0b97f`。完整镜像中没有rustc/cargo/maturin及C编译器；CPython 3.12.16二进制离线安装、pip check和回环非空查询/关闭三项exit 0。镜像摘要、原始日志及过程记录见同目录。attempt01的内部消费成功，但外层包装器把末尾CR传给shell而返回2；修正后新attempt的外层exit 0。 |

隔离使用官方 `linux/amd64` OCI 镜像的展开文件系统。WSL2的独立mount/PID/IPC/UTS/network命名空间不挂载宿主源码或工具，验证时网络只启用loopback。Python镜像 `python:3.12-slim-bookworm` 的 manifest digest 是 `sha256:0f086103b026048cc50cb5da25ec435c023c5241a83fc44daea593377abc61c4`，rootfs tar SHA256 `b16b488c85b743bef8594b9811f1ab5ada8735b65bde702255ceac215b07d7c4`。镜像导出及检查散列见 `stage140-oci-inputs-attempt01`；这证明工具链隔离，不推断物理硬件或macOS产物。

无Python的Rust隔离结果见 `stage140-isolated-rust-attempt01/`。镜像 `rust:1.89.0-slim-bookworm` 的 manifest digest 是 `sha256:294917190b5a3fed18d3303213943f40ec9b644b3cd6e5cdc8cd029334a770b3`，rootfs tar SHA256 `32627f624fc8c7dfd65c14c0141e90b0a5ac4e14fac39e4819071297beef1f07`。实际运行12次独立消费者场景，验证offline/F10/EX/MAC关闭时分别产生预期的E0432、启用时编译通过；最小和full生产依赖树均无Python绑定。镜像内验收脚本写出的 `exit-code.txt` 为0；其 `consumer.log` SHA256 `9ecdb9cfa0bbaec9c944c5b072840dbb52b3ad534d819a36c26b1f19c7917fb3` 与镜像内散列一致。外层WSL包装器在复制完整证据后因末尾CR将 `exit 0` 解析失败而返回2，保留此状态，不能把外层run标记为exit 0。

macOS runner入口已在本地准备：向公开仓库的 `codex/rust-sdk-implementation` 分支推送含工作流变更的候选时，仅在macOS执行Rust 1.89/stable平台矩阵与归档消费；不触发PyPI/tag发布。工作流用actionlint 1.7.12检查，见 `stage140-actionlint-attempt01/result.json`。候选尚未推送，没有macOS运行结果。后续可对同一远端候选使用已有 `Publish` 手动模式，只构建和验证wheel/sdist，不上传到PyPI。缺少crates.io发布凭据及新生产证据；MAC-EX登录/setup、非空category22及涨跌停规则的生产结论仍未验证。

交易所规则文本的第一轮取证成功下载了上交所通知、修订说明和深交所通知，北交所主域名返回403；原文及状态散列见 `stage140-exchange-rules-attempt01/results.json`。第二轮从北交所公告域名下载当前规则，详见 `stage140-exchange-rules-attempt02/results.json`；同目录的上交所规则正文附件SHA256为 `fc922c433438b2636cb631eab25cca405209712acbb6aaded768c45456ff8888`。条款与适用限制见[决策记录](decision-register.md)。规则文本未提供历史逐证券类别、风险状态与上市日；尚不足以启用生产profile。

## Stage141：公开 registry 无 patch 消费入口

干净候选 `2540b02f5a9c52bff6c28cc37fbf67b6473c5fd6` 增加只读 `registry-consumer`。它要求下载的三个 `.crate` 与公开 checksum 一致，核对候选的源码字节与许可证，再在工作区外用全新 Cargo home 生成 lock、核对三包版本/来源/校验值并运行既有11种消费者场景。产品源码与工作区 Cargo.lock 未改。下列证据在 `artifacts/unified/`：

| 项目 | 结果 |
| --- | --- |
| 消费验证合同 | `stage141-red-attempt01/pytest.log` 保留新增入口及锁检查尚未实现时的6项失败。`stage141-green-attempt02/pytest.log` 记录7通过，SHA256 `64a614102fc81276bdcacd0d424fdb5b9fddfc2e71ce890608b5a25126e4aaf1`；模拟已发布的成功场景验证命令、来源和证据流程。 |
| 真实 registry | `stage141-registry-consumer-attempt01/packages.json`，SHA256 `9de1c1827c632a603bc9a81e1e4b387756450ddeed999ed3f272f3a410bf14f5`。2026-11-17 20:31:16–18 UTC，三个名称均404；命令exit 1，未进入Cargo消费。无发布凭据且未上传。 |
| Windows 归档回归 | `stage141-archives-attempt01/packages.json`，SHA256 `d689be960ad89757c924677789224ea3d45a5d1b15e8c87c9e0558163ccb3516`。24条命令exit 0，三归档hash与stage140一致，仍是本地目录源及解包patch消费。 |
| 文档 | `stage141-docs-attempt01/results.json`，SHA256 `472851f5993cfda947e7af5fb7956feea1c93032505f7dc7d1102b8a909faeff`。严格构建与站点链接通过；metadata/mirror、Ruff和diff检查亦通过。 |

接续：macOS候选未推送，须获得公开推送授权或接入现成macOS runner；三个 crate 发布后按[发布政策](release-policy.md)再次执行 `registry-consumer`；生产证据缺口与stage140相同。
