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
