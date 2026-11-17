# Python 兼容与 Rust 新契约：可执行验收规格

状态：**验收设计，尚未执行**。本文件定义56个高价值场景，不是测试实现，也不是通过记录。所列fixture路径均是实施时应创建的目标名称；本文件未生成fixture、测试代码，直接引用主任务提供的[AST快照](baseline-python-api.json)。范围和术语见[能力矩阵](capability-matrix.md)、[公共API](public-api.md)、[开发计划](development-plan.md)。

源码基线为 `af471f1adf38757f6aa0188968db8110eb23f6ad`。旧Python预期来自源码审阅，不等于实测；实现时先用基线环境捕获并固化旧结果，再与新Python比较。协议字节可以是合成golden，但必须标记 `source_type=synthetic`，不能写成生产服务器实测。下文R、O、N分别为正常Rust公共入口、基线旧Python、迁移后新Python；O=N是外部行为要求，允许内部实现不同。

## 1. 统一执行约定

- **执行策略**：R使用 `RustPublicExecutionProfile`；N经 `#[doc(hidden)] pub mod __python_compat` 选择 `PythonLegacyExecutionProfile`。正常Rust API不提供legacy切换。HQ的O/N默认8秒是逐请求期限，保留原startup/预取边界；EX/MAC等原15秒默认按原域保持，显式设置8秒的案例按该参数比较。R默认8秒是整次操作期限。两者共用算法，不能通过复制分页实现达成兼容。
- **时钟**：时间线从调用入口 `t=0` 计时；用可控时钟或按统一比例缩短的本机脚本，不依赖公网睡眠。下文“5秒”必须在证据中同时记录逻辑时间和缩放比例。源端包/请求数以脚本记录为准。
- **请求计数**：除明确列出的setup、metadata、pin外，`calls`只数业务请求；默认元数据已注入、服务已连接、缓存为空。每例必须记录 `protocol, command/entry, identity, offset/count, attempt, host, pin/epoch` 的有序调用表，不能只断言“请求过”。
- **资源断言R0**：场景结束、显式close成功后，新实现的活动请求、pin/lease、专池连接、订阅、排队字节均归零；无新增后台任务、文件句柄或HTTP响应体遗留。坏响应不得使后续独立正常请求死锁。O的旧资源缺陷只记录，不能为了O=N要求新实现恢复泄漏。
- **错误断言E0**：R核对 `ErrorKind` 和适用的 `phase/command/identity/endpoint/attempt/source`；O/N核对具体异常类、既有字段、固定契约消息及 `__cause__`。动态路径/端口/操作ID用类型和关联关系核对，不把随机值写死。没有source的既有异常不强造cause。
- **完整性**：合成输入必须完整列出字节、模型字段或生成参数；不能以待测实现生成expected。所有浮点比较写清原始位比较或数值容差；单精度输入的精确位模式应单独保存。
- **证据状态**：`S`=可合成且已定义判定的规格；`B`=基线结果仍需运行冻结；`D`=新契约/规则尚需P0决定；`L`=真实服务器证据尚缺。每例初始均为未执行。S/B不等于测试通过。
- **快照**：签名、导出、dataclass字段使用主任务生成的AST快照JSON及其实际路径，不在本文件重复生成。执行证据须记录该快照hash、源码SHA、构建SHA、wheel/crate hash、工具链和实际平台。
- **fixture根目录**：以下省略目录的文件名均相对 `tests/fixtures/sdk/compat/`；二进制与同名JSON配对，JSON记录生成参数、独立expected和分支。A/B等代码别名固定为A=`sh600000`、B=`sz000001`；没有业务意义的必填模型字段按AST类型使用0/空串/None，并在fixture完整展开，不能调用待测parser填默认值。

每条案例的结果写入 `evidence/<case-id>.json`，至少含 `case_id/profile/source_type/fixture_sha256/source_sha/build_sha/command/exit_code/assertions/call_trace/resource_before/resource_after/status`。候选尚不存在的Rust接口按公共API最终签名绑定，不以本文概念性调用名当编译承诺。

## 2. 安装、表面API与默认配置

### COMP-001 — 两个产品的独立安装与默认构造

**映射/fixture**：T04、T28、T29；`tests/fixtures/sdk/compat/001_install_defaults.json`；S+B。

**前置/输入**：R在没有Python/pip/maturin的consumer环境；O/N在只有Python且PATH不含rustc/cargo/maturin的环境安装匹配平台wheel，强制 `--only-binary=:all:`。记录版本与产物hash。构造默认客户端、获取service、关闭尚未连接客户端。

**预期**：R构建/运行成功，普通依赖无PyO3；O/N安装及import成功，不现场编译。共同默认数值：pool=1、server_count=1、timeout=8、heartbeat=30、max_pending=256、push_count=1024、push_bytes=64MiB、connect_concurrency=1；作用域差异按COMP-006。R构造不DNS/联网/读用户文件/建线程；O/N按基线初始化副作用比较，不强套R构造规则。

**计数/资源**：业务calls=0；R构造前后线程/连接增量0，close幂等；O/N wheel内native可加载，R0。任何sdist回退均失败。

### COMP-002 — 导出、别名和导入路径不漂移

**映射/fixture**：T01、T03、T12、T30；`002_exports_aliases.json`；B。

**前置/输入**：导入顶层、api、models、helpers、protocol、offline与扩展公开模块；逐项读取根任务AST导出快照。以in-memory/script transport分别调用 `latest_stock_list`、其已公开别名以及 `subscribe_hq` 的同义入口。

**预期**：O/N符号集合、对象用途、别名参数和返回类型与快照一致；不因Rust名称 `HqMarket/TdxClient` 改掉Python `Market/TdxClient`。R只暴露公共API批准的名称，不要求照搬Python别名。未启用f10/ex/mac/offline的R consumer编译访问相应模块必须失败。

**计数/资源**：每对别名使用相同独立脚本，业务调用序列相同；导入不得发业务请求。R0。

### COMP-003 — 签名和关键字默认值按AST全量核对

**映射/fixture**：T01、T03、T18、T23、T27；`003_signature_contract.json`；B。

**前置/输入**：把主任务AST快照中的每个公开函数签名作为数据；分别绑定省略可选参数、显式None、合法位置参数、未知keyword四组调用。重点 `bars.get` 的count800/page_size800/max_pages200、`trades.today` 的count1800、订阅poll0.5/renew7和旧 `close()` 无options。

**预期**：O/N参数接受/拒绝、默认值和返回类型一致；未知keyword为原 `TypeError`。R typed query/options按独立契约编译，不要求Python动态签名。N不能新增必填 `OperationOptions`，不能把 `max_pages=None` 换成固定大数。

**计数/资源**：绑定失败calls=0；合法单页调用按各默认count核对一次业务请求，纯构造/订阅句柄创建不得被误计成activate。分页、组合和订阅执行各用对应案例的完整调用表，不能统一要求一次请求。R0。

### COMP-004 — 模型字段、便捷属性与JSON投影

**映射/fixture**：T02、T12、T18、T28；`004_models_serialization.json`；S+B。

**前置/输入**：按AST字段快照构造QuoteLevel、TradeTick、TradePage、TradeBatch、StockProfile及日期/bytes/tuple样本；TradeTick设price=2.5、price_milli=2500、volume=4、status_raw=5、time_minutes=905、time_label='15:05'、event_kind='trade'；序列化含 `date(2026,11,16)` 与bytes `00ff` 的对象。

**预期**：O/N字段顺序、slots/frozen行为、默认None/raw一致；trade_amount_yuan=1000、is_after_hours_fixed_price=true、is_actual_trade=true；日期为`2026-11-16`，bytes为`00ff`，tuple转list，TradeBatch经to_page物化。R DTO保留同语义，serde按其独立格式核对，不能要求Python任意set排序。

**计数/资源**：calls=0；物化结果与批视图一致，不借助网络；R0。

### COMP-005 — 参数错误不发请求

**映射/fixture**：T03、T04、T11、T13；`005_invalid_inputs.json`；S+B。

**前置/输入**：分别传 `pool_size=0`、不合法端口、bars count=801、delisted count=true、日期`20260230`；每个错误单独构造执行，禁用真实网络。

**预期**：R为 `InvalidArgument` 或构造期类型拒绝，无panic；O/N逐入口原异常类及消息保持。bars单页count801为`ValueError('page size must be between 1 and 800')`；delisted bool count触发`ValueError`，不能按整数1接受。日期/端口/构造参数的Python类与消息由基线逐项冻结，不统一改成一种异常。

**计数/资源**：calls=0；R构造失败线程=0；O/N不留下半初始化连接，R0。

## 3. 执行策略、生命周期与批量能力

### COMP-006 — 两页各5秒与整次8秒

**映射/fixture**：T06、T08、T13、T18；`006_two_pages_deadline.json`；S+B。

**前置/时间线**：已连接/精度已知；bars all_pages、page_size=2、max_pages=10。offset0在t=5返回2条，offset2在该请求后5秒返回1条，offset3立即返回空。默认timeout=8。

**预期**：R在t=8终止 `Timeout`，phase=response/aggregate须与实际等待阶段一致，不返回成功收集结果；O/N在t≈10成功返回3条。O/N没有新的高阶总期限；R不启动offset3。

**计数/资源**：R调用offset[0,2]，O/N调用[0,2,3]；R取消第二请求并确认终态，延迟回包不污染下次调用；R0。

### COMP-007 — startup不借迁移改计时起点

**映射/fixture**：T06、T08；`007_startup_boundary.json`；S+B。

**前置/时间线**：注入数值地址，首次runtime startup耗时6秒，业务响应耗时3秒，startup自身上限内；不开DNS线程。一次quotes请求，timeout=8。

**预期**：R从公共入口起算，t=8为`Timeout`且不再交付成功；O/N按旧 `reserve_submission`→创建request deadline顺序，约t=9成功。另分支startup耗时超过原startup限额，O/N仍抛原超时，不无限等待。

**计数/资源**：主分支业务calls=1；trace记录startup完成与deadline建立顺序；失败时所有半建worker/连接清理，R0。

### COMP-008 — 元数据预取与主请求独立期限

**映射/fixture**：T08–T10、T12；`008_metadata_then_quote.json`；S+B。

**前置/时间线**：证券精度缓存冷；需要价格精度的refresh请求。目录 `044D` t=5返回已完整市场并随即空页，主 `0547` 再耗5秒。已连接、不需额外setup。

**预期**：R整体t=8超时；O/N目录和主请求各在8秒内，约t=10成功，价格按该目录精度。N不新增第二轮预取；缓存热的同请求直接发0547。

**计数/资源**：冷分支顺序044D数据页、044D空页、0547；热分支仅0547；R失败后不得把未完成目录标为完整，R0。

### COMP-009 — 恰好32次额外恢复上限

**映射/fixture**：T02、T08、T11；`009_recovery_32.json`；S+B。

**前置/输入**：按有序64个代码请求snapshot；代码为`sh600000`至`sh600063`，脚本对每个非空子集返回可恢复的`empty snapshot response`，每次立即响应。expected由独立深度优先二分遍历生成，保存在fixture中。R显式给足总时间，操作额外尝试预算32，每个wire自身最多2次且本例无传输重发。

**预期**：O/N初始+32次额外=33次后，在第34次发送前抛 `ProtocolError`，消息含 `recovery request budget exhausted (32)`；不能错误地把初始计入32。R在局部/操作额外上限到达时均为结构化 `LimitExceeded`，记录recovery=32、wire_attempts=33和待请求子集；畸形记录错误仍为Protocol，不能成功返回空。

**计数/资源**：精确核对33个有序子集及无第34请求；各响应释放；R0。

### COMP-010 — 新Rust共享额外尝试不截断旧Python分层重试

**映射/fixture**：T08、T11、T14；`010_shared_attempts.json`；S+B。

**前置/时间线**：二证券快照初始请求遇一次可重试连接故障，重发后返回可恢复缺项；两个单证券恢复均成功。R配置operation额外尝试预算2、每wire最多2次（含首次）、总时间充分；O/N保持原runtime单请求重试与32恢复限制。

**预期**：R初始wire1失败，wire2重发消耗额外预算1；wire3恢复A再消耗1，发送B前返回LimitExceeded，累计wire_attempts=3、extra_attempts=2；O/N允许第4次wire恢复B，按输入返回A/B。不能把R累计预算放入N兼容入口。

**计数/资源**：R=3、O/N=4，顺序为AB、AB、A、B的相应前缀；故障连接替换有epoch记录；R0。

### COMP-011 — max_pages和None语义

**映射/fixture**：T08、T13、T14；`011_page_limits.json`；S+B。

**前置/输入**：bars每页返回1条、第四请求才空。分支A max_pages=2；分支B max_pages=None且每页即时返回；R用允许至少4页的显式PageOptions比较B。

**预期**：A O/N在第二非空页后抛原`RuntimeError`及bars max_pages消息，不发送第三页；R对应collect为`LimitExceeded`。B O/N四次完成3条，不以新默认上限替代None；R满足显式限制后同数据。

**计数/资源**：A calls=2，B calls=4；无多余探测页，聚合失败不伪报complete；R0。

### COMP-012 — Python多池关闭顺序与独立预算

**映射/fixture**：T07、T08、T18；`012_close_three_pools.json`；S+B。

**前置/时间线**：普通池、money-flow dedicated、auction dedicated各已启动，preferred未建立；每池从收到close到确认需0.6秒。观察close事件，不发业务请求。

**预期**：O/N按 `ordinary -> money_flow -> auction` 顺序，单池原1秒预算，约1.8秒成功；N经兼容桥不套owner总1秒。R显式owner close总1秒，允许并行关闭后约0.6秒成功；若顺序关闭导致到期仍有未确认池，必须`CloseTimeout`，保留closing诊断并可后续等清理，不能把预算累加为1.8秒后成功。额外分支让至少一池到t=1.2才确认，R必须在总1秒到期失败。

**计数/资源**：每池close一次；O/N成功后全零。R返回错误不宣称全零，继续受控完成后归零；无无主资源，R0最终成立。

### COMP-013 — 旧Python关闭首错短路

**映射/fixture**：T07、T18；`013_close_first_error.json`；S+B。

**前置/输入**：三个pool已启动，普通pool.close抛 `TransportCloseTimeoutError('ordinary timeout')`；用脚本记录后两个pool.close是否调用。并发分支A用barrier让ordinary close阻塞，在其返回前分别发money_flow和auction专池业务；分支B在ordinary首错返回后分别发这两域业务。专池脚本健康且各返回一条有效模型，不注入关闭失败。

**预期**：O/N首次close原类/消息向外传播，money-flow/auction close调用为0；不因Rust更强owner清理而让用户看到不同首错。R owner关闭可继续清理所有池并返回首因+cleanup状态，不能伪报成功。N内部安全清理允许进行，但不得伪装为旧外部close调用序列或吞掉首错，更不能提前封闭尚未轮到的域。并发A/B先运行O冻结专池成功/错误、返回模型和请求序列，N必须保持；若O允许专池业务成功，N不能因全局early close返回Closed。资源加强仅限不可再用的失败域。

**计数/资源**：兼容外部close序列仅ordinary。A/B各独立重置后记录两次专池业务的实际calls/顺序及结果，以O证据作N的判定；不得把并发顺序写死成调度巧合。测试finally显式清理两池；新实现不得遗失owner，错误后诊断可见，最终R0。

### COMP-014 — reopen换代与旧child

**映射/fixture**：T07、T12、T18；`014_reopen_generation.json`；S+B。

**前置/输入**：连接A、持有pin/subscription、close成功、再次connect，执行一条quote；再用旧pin/subscription请求。固定两代不同epoch。

**预期**：R同一closed client不重开，`Closed`；新建client后正常。O/N公开close→connect仍可用，N创建新generation。旧pin失效，不能路由进B；旧订阅已终止。O具体旧child异常类/消息运行冻结，N保持，不能只按新Rust `Closed`统一映射。

**计数/资源**：B quote一次；旧child业务calls=0；A lease=0且不会复活，最终R0。

### COMP-015 — owner生命周期与pending交接竞态

**映射/fixture**：T05、T07、T15；`015_owner_handoff.json`；S+B。

**前置/时间线**：R保留owned service后drop client，service执行1次；随后drop service。另一分支barrier卡在pin grant已分配/接收者尚未取得处，先丢弃接收者再放行。

**预期**：R service保活owner直至最后拥有者释放；丢失交付的grant必释放一次。O弱引用API对象不因迁移被要求等同R owned handle；O/N保留基线弱引用/报错表面。N的内部grant竞态修复允许加强资源回收。

**计数/资源**：成功service calls=1，竞态分支业务calls=0；grant分配1、release1、active leases=0；不可双release，最终R0。

### COMP-016 — today bulk的新Rust成功与旧原生错误

**映射/fixture**：T03、T14、T18；`016_today_bulk_native.json`；S+B。

**前置/输入**：today ticks合成2行，调用today_batch和all_today_batch；Python分别使用真实SocketTransport与PooledSocketTransport。metadata热、setup完成。

**预期**：R直接typed列批量API发送0FC5并返回2行，列访问与物化一致。O/N上述两种transport在发送前抛 `ValueError('trade batch only supports historical ticks')`；all_today_batch同样第一步失败。只有另行批准的缺陷修复才能改变N默认。

**计数/资源**：R单页calls=1；O/N业务calls=0；错误不创建额外专池。R0。

### COMP-017 — 自定义transport的today bulk成功不被统一禁止

**映射/fixture**：T12、T14、T18；`017_today_bulk_custom.json`；S+B。

**前置/输入**：适配器A有execute_trade_batch并返回TradeBatch；B仅原execute并返回TradePage；C bulk返回字符串。均调用today_batch('sh600000')，合法页面2行。

**预期**：O/N A、B均成功2行；C原`TypeError('execute_trade_batch() must return TradeBatch')`。R脚本executor成功结果同语义，不需要接受任意Python对象。N不得为保持COMP-016而禁止所有today调用。

**计数/资源**：每分支恰1次0FC5；A不调用fallback execute，B不要求新增方法；R0。

## 4. 资源、组合服务、顺序与缓存

### COMP-018 — max_bytes=0

**映射/fixture**：T17、T18；`018_download_zero.json`；S+B。

**前置/输入**：空闲已构造客户端，download_file('x', max_bytes=0)，服务脚本若被读取立即报测试失败。

**预期**：R空data、`LimitReached`、next_offset=0，不建连接；O/N返回b''，按原实现可能先进入pin上下文但不发06B9。N不能为了R零副作用要求改变旧外部pin观察；严格Rustdownload_complete返回`LimitExceeded`。

**计数/资源**：业务calls=0；记录O pin是否获取，N须同；退出后pin归零，R0。

### COMP-019 — 文件恰好等于字节上限

**映射/fixture**：T17、T18；`019_download_exact_cap.json`；S+B。

**前置/输入**：文件b'ABCDEFGH'，chunk_size=4，max_bytes=8；脚本offset0/4各返回4字节。

**预期**：R data相同、`LimitReached`、next_offset=8，不能凭真实文件恰好8字节宣称Complete；严格download_complete为`LimitExceeded`。O/N返回8字节，无新增限额异常。

**计数/资源**：06B9请求(offset,size)=[(0,4),(4,4)]，没有offset8探测；全程同pin/host，R0。

### COMP-020 — 文件超过上限时最后请求缩短

**映射/fixture**：T17、T18；`020_download_over_cap.json`；S+B。

**前置/输入**：文件b'ABCDEFGHIJ'，chunk_size=4，max_bytes=6。

**预期**：R b'ABCDEF'、LimitReached、next_offset=6；O/N完全同bytes。分支服务谎报chunk_len=3但content两字节时，R `Protocol`，O/N原`ProtocolError`，不可把坏chunk当EOF。

**计数/资源**：正常[(0,4),(4,2)]；坏块后不再发请求，pin必释放，R0。

### COMP-021 — 无上限精确chunk边界与来源锁定

**映射/fixture**：T17；`021_download_eof_pin.json`；S+B。

**前置/输入**：文件8字节、chunk4，无截取上限或R显式足够大上限12；offset8返回空。分支offset4返回来源host B，首块host A。

**预期**：正常R Complete/next=None、O/N8字节；注入host改变但仍回包的分支R为Protocol并指出来源不匹配，真实pin断开则为PinLost，不能混为成功EOF。O/N host改变为`TransportError`且消息包含server changed，不拼接两台文件。

**计数/资源**：正常offset[0,4,8]；坏来源offset[0,4]后停止；只获取1个pin，不逐块释放再拿；R0。

### COMP-022 — stats上限加一与解压前拒绝

**映射/fixture**：T17、T18；`022_stats_max_plus_one.json`；S+B。

**前置/输入**：生产上限M=32MiB；在脚本中生成M+1字节压缩体而不全部驻留，chunk=30000；断言下载上限=M+1。另有小型正常ZIP含两个唯一cfg；再有同名cfg重复、加密、129entries分支。

**预期**：大体R `ResourceFormat/LimitExceeded`按定版kind，O/N `ResourceFormatError`，ZIP解压调用数0；正常两cfg解析；重复/加密/过量entries拒绝，不选首个悄悄通过。缩小测试常量只能用于单测，生产常量另断言。

**计数/资源**：大体数据量精确M+1、请求数ceil((M+1)/30000)，最后块请求余数；不继续EOF探测。每分支R0。

### COMP-023 — full_quotes补充超时可降级，总期限不可吞

**映射/fixture**：T08、T18；`023_quote_enrichment_timeout.json`；S+B。

**前置/时间线**：snapshot1秒成功，bid/ask各1档；refresh故障分支A在2秒内返回连接错误或较短子期限超时；分支B一直无响应。R总8秒；O/N单请求8秒。

**预期**：A R返回snapshot+补充Failed/原因，O/N原snapshot，无新增字段；B R在总t=8抛Timeout不返回成功，O/N在refresh自身8秒超时后按旧catch返回snapshot（约t=9）。若refresh只有一侧5档仍不覆盖；两侧≥5才覆盖并采用refresh opening amount。

**计数/资源**：每分支054C→0547各1次，无重试（测试禁用wire retry）；失败不缓存伪五档；R0。

### COMP-024 — shortline时间与统计日期门槛

**映射/fixture**：T18；`024_shortline_readiness.json`；S+B。

**前置/输入**：握手日期2026-11-16、时刻09:24:59；预载交易日[11-13,11-16]。请求100只代码，stats双表各100行。分支09:25:00、两表主日期均20261116且各95行、另5行20261113；94/100、两表主日期不同、两表主日期均20261113再三分支。所有价格/财务/历史bar补充源用完整固定模型注入，单独记录补充调用。

**预期**：09:24:59 O/N `ShortlineIndicatorsNotReadyError`，R DataNotReady；95%边界可用。94%或两表主日期不同时O/N为`ResourceFormatError`；两表一致但日期既非目标日又非前交易日才为`TdxStatsDateError`。R相应映射由错误表固定。前交易日stat2按既有列移位，不能把昨天字段当今日。服务器时钟优先于本机。

**计数/资源**：已预载交易日的未ready分支握手后不发后续行情；坏日期/覆盖率分支只读一次cfg，不调用报价/财务补充。可用分支cfg一次，后续snapshot80→refresh80→snapshot20→refresh20；其余补充源各按独立脚本核对，每指标来源/None与基线一致，R0。

### COMP-025 — profile财务失败缓存的兼容与新状态

**映射/fixture**：T10、T18；`025_profile_finance_cache.json`；S+B。

**前置/输入**：两证券profile、quote和code目录成功；首次finance超时，第二次可成功。连续两次相同有序代码调用，再以逆序代码调用，最后clear_cache后同序调用。

**预期**：O/N首次财务字段None，第二次命中既有空finance缓存仍None；逆序是新key，clear后重新请求。R可区分Failed/空/过期并按新缓存策略重试，不能把失败永久标完整；R不同结果需携带补充状态。现有profile全部HQ，f10/offline关闭也可用。

**计数/资源**：O/N finance次数同序2次=1、逆序+1、clear后+1，总3；quote每次独立；R按明示策略且有trace，R0。

### COMP-026 — clear_cache不偷改旧日历缓存

**映射/fixture**：T10、T16、T18；`026_cache_calendar_scope.json`；S+B。

**前置/输入**：先加载benchmark日历[11-13,11-16]；脚本随后变为加11-17；调用client.clear_cache再问11-17，最后显式client.workdays.clear/refresh。

**预期**：O/N client.clear_cache不清旧workdays，第二问仍false；显式workdays清理后true。R须用覆盖至11-17的旧calendar快照验证同一缓存差异：清calendar后true，不清时false；选项必须标明作用域。若R旧快照覆盖只至11-16，则11-17应MetadataUnavailable，不能误把超覆盖日期断言为休市。清precision不触碰其他owner。

**计数/资源**：O/N benchmark完整加载两轮而不是三轮；每轮至空页。缓存已加载时is_workday不联网，clear后首次查询可触发加载；R0。

### COMP-027 — market_stat缺中间源的旧错位

**映射/fixture**：T03、T18；`027_market_stat_missing_middle.json`；S+B。

**前置/输入**：legacy请求[880005,880001,880006]，只返回第一和第三。第一last/open/low/high=10/20/3/40；第三last/open=7/2。

**预期**：O/N按原位置返回up100/down200/neutral30/total400、market_cap=7e10、limit_up/down=0（保留已知错位行为，直至批准修复）。R按身份关联，880001缺失必须MissingData或结构化缺失，不用880006当市值。禁止把O错值当真实市场事实。

**计数/资源**：053E一次、输入顺序固定；原helper不补查；R补查仅按明确策略，fixture记录差异；R0。

### COMP-028 — 重复输入、缺项与skip warning

**映射/fixture**：T11、T12；`028_snapshot_duplicates_missing.json`；S+B。

**前置/输入**：输入[A,B,A]；脚本顺序固定为[A,B,A]→可恢复count mismatch、[A]→A#0、[B,A]→可恢复count mismatch、[B]→空、[A]→A#2。两条A用独特last值10/12标明occurrence。

**预期**：R默认MissingData标B及input_index1；AllowPartial保留3槽，A/Bmissing/A。R若内部合并重复查询，必须明示同一回复复制到两处的来源关系；若不合并则保留各occurrence的来源值。O/N raise为MissingCodesError，missing_codes=(B,)；skip警告恰1个MissingCodesWarning，输出价格[10,12]对应[A,A]原顺序，不能去重为单A。

**计数/资源**：O/N共5次业务请求，初始1+恢复4；raise/skip分别重置脚本重跑，精确核对上述深度优先序列。R固定所选内部合并策略后记录对应请求序列，不能以允许合并为由省略输入槽位断言。无坏数据冒充missing；R0。

### COMP-029 — 坏单例不是缺失证券

**映射/fixture**：T11、T12；`029_corrupt_single.json`；S+B。

**前置/输入**：单A响应价格记录marker损坏，产生`ProtocolError('snapshot record marker not found: ...')`；on_missing=skip。

**预期**：O/N仍抛ProtocolError，不发MissingCodesWarning，不返回[]；R Protocol且identity=A、command054C/source保留。另分支真正空单例允许COMP-028的missing路径。

**计数/资源**：坏单例calls=1，不无限二分或重试；坏frame释放后下次正常请求成功，R0。

### COMP-030 — K线短页不是终点，聚合raw有来源

**映射/fixture**：T13、T18；`030_bars_short_pages.json`；S+B。

**前置/输入**：page_size=3；offset0返回时间[09:40,09:45]，offset2返回[09:30,09:35]，offset4空；页raw分别P0/P1/P2。

**预期**：R完整items=[09:30,09:35,09:40,09:45]并保留页来源；O/N同items但聚合raw_payload=b''，只逆转页序不逆页内。单页调用仍保留该页原raw（include_raw=true）。

**计数/资源**：offset[0,2,4]，不能短页提前结束；请求count始终3，R0。

### COMP-031 — 同分钟逐笔原序与事件种类

**映射/fixture**：T14、T18；`031_trades_same_minute.json`；S+B。

**前置/输入**：新页两条09:31价格10/11、原index0/1；旧页依次09:24 auction_snapshot、09:25 opening_match；最后空。另有独立15:05 status_raw5记录。

**预期**：R/O/N只逆页序，结果时分为[09:24,09:25,09:31,09:31]，09:31价格仍10/11；Python index/absolute_index保留原页值，Rust页来源记录对应原页。auction_snapshot不计actual_trades，opening_match计入；15:05 status_raw5为after_hours_fixed_price。聚合raw按语言契约，批量select([1,1,0])保持重复顺序且子集raw清空。

**计数/资源**：两数据页+一空页；没有按时间合并去重；R0。

### COMP-032 — 退市聚合的排序和首raw兼容

**映射/fixture**：T03、T13、T18；`032_delisted_raw_sort.json`；S+B。

**前置/输入**：T代码分页P0日期[11-16,11-13]、P1[11-12]、P2空，include_raw=true，P0原bytes独特。

**预期**：O/N按time排序[11-12,11-13,11-16]且raw仍P0，不能套普通bars清raw策略。R用显式页来源，不把P0伪称完整aggregate raw；迁移不删Python首raw。

**计数/资源**：offset[0,2,3]；每页count按原推进；R0。

### COMP-033 — 订阅先metadata后两pin及失败回滚

**映射/fixture**：T05、T15；`033_subscribe_two_groups.json`；S+B。

**前置/输入**：101唯一代码、pool_size=2、冷metadata；第二组activate注入ProtocolError。另分支pool_size=1。

**预期**：R/O/N metadata必须先于两个pin；组大小100/1，各绑定原连接。第二组失败释放全部已获pin。pool1在占有部分资源之前拒绝；O/N原ValueError含`needs at least 2 pool slots`，R InvalidArgument/PoolBusy按定版契约。

**计数/资源**：正常失败分支activate0547两次、pin2/release2；pool1 pin0、activate0；无metadata自锁，R0。

### COMP-034 — 续订cursor回退与push gap

**映射/fixture**：T15；`034_subscription_cursor_gap.json`；S+B。

**前置/时间线**：单A订阅，收到cursor150000后再收到93000，逻辑7秒触发renew；队列容量2却注入3帧；再close并唤醒poll。

**预期**：O/N下一renew带93000，不能取max；原PushOverflowError/on_error路径按基线保持。R emit明确gap及游标93000，next_event单次超时不退订；close后正常终止。Python回调仍在原调用线程执行，不强加新callback worker。

**计数/资源**：首次activate1、renew1；gap只报告一次丢失事件而非每次poll永久报错；close后pin0，R0。

### COMP-035 — 两owner精度隔离

**映射/fixture**：T09、T10；`035_precision_isolation.json`；S+B。

**前置/输入**：A/B owner对同一基金代码注入decimals3/2；相同raw价格12345并发解析100次；中途A clear/refresh，B不动。

**预期**：R每次A=12.345、B=123.45，不读写legacy全局表；N也隔离并保持各自输入精度。O只记录进程全局行为，不要求新实现复制污染，这是允许加强的隔离修复；baseline不能作为数值oracle。未知代码fallback按显式policy。

**计数/资源**：已注入metadata时业务每owner100次，无额外目录请求；循环结束context没有互改，R0。

### COMP-036 — 一次Rust操作冻结快照，日历用真实日期

**映射/fixture**：T09、T10、T16；`036_snapshot_calendar.json`；S+B。

**前置/时间线**：R两页raw同值，第一页后后台把精度2刷新成3；此次操作捕获旧snapshot。日历独立分支benchmark仅[2026-11-13,11-16]，问11-14及范围11-16→11-13。

**预期**：R两页均按2解码，下次按3；O/N按各自已冻结兼容metadata预取顺序验收，不能新增IO。日历R/O/N绑定来源时11-14=false，逆向输入range仍返回升序[13,16]，descending时反转；已失效client不能静默weekday fallback。

**计数/资源**：R两页请求+结束页，刷新不插入本操作；日历加载至空页一次，后问命中cache；R0。

## 5. 离线格式、财报和计算规则

### COMP-037 — 残尾可读，但空append也必须拒绝

**映射/fixture**：T21；`037_offline_partial_tail.bin`及`.json`；S+B。

**前置/输入**：day文件1条完整32字节+`aabbcc`；分别read、append([])、append([较新日期])；对lc1/lc5/.5/ex-day参数化同形残尾。

**预期**：R/O/N read返回1条；append均拒绝，O/N ResourceFormatError，R ResourceFormat含record_size/actual_length；原35字节逐位不变。不得空输入提前return0，也不得自动截断修复。

**计数/资源**：网络0；写入调用0、fsync0；文件可再次打开删除，无句柄遗留，R0。

### COMP-038 — write实际上追加且批内不排序去重

**映射/fixture**：T03、T21；`038_append_order.bin`及`.json`；S+B。

**前置/输入**：磁盘末日期20261110；输入[12(price12),11(price11),12(price13),10(price10)]，调用既有write_daily_bar，分钟/EX用等价timestamp参数化。

**预期**：O/N实际新增3条，顺序[12@12,11@11,12@13]，旧10跳过；不内部sort/dedupe，不replace旧文件。R legacy-compatible append同结果；另有显式排序/replace策略才允许不同，不能成为默认。

**计数/资源**：append一次、文件增96字节，旧prefix完全不变；缺文件/空文件同输入写4条原顺序；R0。

### COMP-039 — daily与minute/EX耐久性差异

**映射/fixture**：T21；`039_writer_durability.json`；S+B。

**前置/输入**：写1条合法新记录，文件层记录open/write/flush/fsync/close事件；平台fsync替身只记录不改变业务结果。

**预期**：O/N daily显式flush、fsync各1且顺序write→flush→fsync→close；minute/EX没有显式os.fsync。R按显式durability策略选择，不宣称普通close等于fsync。无新行返回0时不得凭空新增fsync。

**计数/资源**：各writer一次append；全部关闭句柄。异常fsync分支O/N原OSError传播，不能返回写入成功；R Io/source保留。

### COMP-040 — 原始float位、分钟格式与EX amount双视图

**映射/fixture**：T10、T21；`040_offline_units.bin`及`.json`；S+B。

**前置/输入**：EX amount槽0x4996B43C；OHLC保存指定f32位；.5价格整数1234，lc1价f32=12.34；分钟日期/时分给2026-11-16 09:31。未知day文件名使用默认系数。

**预期**：R/O/N EX amount整数1234613308，hk_stock_amount=1234567.5，encode原amount4bytes不变；.5为12.34、lc1按f32容差；unknown类型返回UNKNOWN且系数(0.01,0.01)。路径lc1为minline、lc5/.5为fzline。

**计数/资源**：网络0；往返必须对原始位/字节而非仅格式化小数；R0。

### COMP-041 — 行业配置宽松边界包含六字符非数字

**映射/fixture**：T21；`041_industry_gbk.bin`及`.json`；S+B。

**前置/输入**：GBK文本行`0|000001|A|||S1`、`0|000001|B|||S2`、`0|12345|X`、`bad`、`0|abcdef|Y|||Z`，另加行业字段非法GBK字节。

**预期**：O/N 000001→(B,S2)，abcdef→(Y,Z)，长度5和少列跳过；非法GBK替换字符保留，不整文件失败。R兼容parser同规则，严格校验若另设须显式。这里“代码合法”原判断仅len==6，不是isdigit。

**计数/资源**：bytes入口0IO、file入口一次read；空输入{}，缺文件FileNotFoundError/R Io；R0。

### COMP-042 — 多不同名DAT与同名重复分开

**映射/fixture**：T03、T22；`042_dat_selection.zip`及`.json`；S+B。

**前置/输入**：ZIP依次A.dat内容b'A'、B.dat内容b'B'；另分支两个A.dat；只测试选取阶段，不把单字节当合法财报。

**预期**：O/N不同名取第一A；同名重复ResourceFormatError。R无selector的严格API对多不同名返回ResourceFormat/ambiguity，显式A selector可取A；不得把O首选行为写成“唯一DAT验证通过”。

**计数/资源**：O/N不同名或R显式selector只解压选中条目1次；R无selector歧义、所有同名重复错误均解压0；所有ZipFile关闭，R0。

### COMP-043 — DAT结构和资源上限

**映射/fixture**：T22；`043_financial_bounds.bin`及`.json`；S+B。

**前置/输入**：合成header：report_size=3非法；第二分支合法size8、两个有效index指向相同offset；第三分支index最后不足完整记录；第四分支指向文件外。

**预期**：O/N非4倍数及重复有效offset拒绝ResourceFormatError；截断index按基线break保留已解析项，越界data按基线skip。字段数4096/4097分别通过/拒绝；累计浮点用128字段×32768行=4Mi通过、增加1条完整记录后拒绝。ZIP路径额外覆盖DAT256MiB、ZIP64MiB、entries64的exact/+1边界。旧direct.dat读取及parse_financial_dat没有DAT总字节上限，O/N保留该差异；R严格入口统一有界。R strict/compat的结构容错分别明示，不混用。

**计数/资源**：不得按伪count分配无界数组；重复offset第二行不复制数据；网络0，R0。实测有效财报样本仍L。

### COMP-044 — 主板ST冲突按入口兼容

**映射/fixture**：T03、T16、T18；`044_price_main_st.json`；S+B+D。

**前置/输入**：sh600000、name='ST样本'、pre_close=10、非IPO；对daily_price_limits、shortline内部ratio、MAC compute_price_limits分别调用。

**预期**：O/N daily ratio10、上下11/9；shortline ratio5；MAC上下10.5/9.5。R只有经P0批准且带有效日期的rule profile才能给权威值，证据未批准则Unknown；不得从三实现任择一覆盖Python全部入口。

**计数/资源**：纯规则calls=0，service分支注入既有日历/finance后不新增额外源；记录profile ID与规则来源，R0。

### COMP-045 — 创业板ST、未分配30段及北交IPO窗口

**映射/fixture**：T03、T16；`045_price_segments_ipo.json`；S+B+D。

**前置/输入**：pre_close10；a=sz300001名ST、b=sz303001普通、c=bj920001上市第2个交易日。

**预期**：O/N a：core20%、shortline5%、MAC20%；b：core/shortline20%、MAC默认10%；c：core仍前5日无幅度，MAC第2日常规30%。R各情况须依据已批准日期profile或Unknown；未有法规fixture前不能标D已解除。

**计数/资源**：纯规则0网络；上市日由注入交易日序列算，不按自然日；R0。

### COMP-046 — 复权为仿射而非只乘系数

**映射/fixture**：T16；`046_adjustment_affine.json`；S+B。

**前置/输入**：同日category1事件E1(c1=2,c2=3,c3=1,c4=2)，E2(c1=1,c2=0,c3=0,c4=0)；输入顺序E1,E2，均有效日期；另分支c3=-10,c4=0。

**预期**：m1=1.3、o1=-0.4，m2=1、o2=0.1；qfq scale=1/1.3，offset=0.4/1.3-0.1；hfq同日原序scale1.3、offset=-0.3。O/N与R按明确公式容差1e-12；m<=0 O/N ValueError、R InvalidArgument。anchor/start筛选不改同日顺序。

**计数/资源**：纯计算0网络；原始事件字段不改写，R0。

## 6. F10与扩展协议

### COMP-047 — 动态表、重复列和多结果集

**映射/fixture**：T19、T20、T23；`047_f10_dynamic.json`；S+B。

**前置/输入**：JSON含ErrorCode0，表0 ColName=['x','x','']、Content=[[1,2,3,4],7]；表1 ResultSetKey='detail'、Content=[[{'k':true},9007199254740993]]，无列名。

**预期**：O/N表0首行dict={x:1,x__2:2,col_2:3,col_3:4}，row_cells名称x/x/col_2/col_3及index0..3；标量7为单cell；表1两cell，整数不变成f64舍入。R保留表序、重复列与原值，Python投影规则同旧；response.rows只指首表，不能替代detail读取。

**计数/资源**：HTTP1；响应body关闭，原raw保留；R0。

### COMP-048 — HTTP失败、业务非零和坏JSON分别处理

**映射/fixture**：T19、T20；`048_f10_errors.json`；S+B。

**前置/输入**：独立分支HTTP503、HTTP200+ErrorCode17+空ResultSets、200+`[]`、200+非法UTF8/JSON、200+ResultSets='bad'。

**预期**：O/N 503为TransportError且cause为HTTPError、消息含503/entry；ErrorCode17正常返回F10Response(ok=false,error_code=17)，不自动抛；non-object/坏JSON/坏表结构为ProtocolError，解码cause仅在旧逻辑存在时保留。R保留业务状态与HTTP错误的区别，不能用Ok=true掩盖17，也不能在N统一raise。

**计数/资源**：每分支HTTP1、自动重试0；失败body关闭；R0。

### COMP-049 — F10路由和Params类型/顺序

**映射/fixture**：T19、T20；`049_f10_routes_params.json`；S+B。

**前置/输入**：自定义本地三个base URL捕获请求；entry依次`CWServ.tdxf10_gg_comreq`、`CWServ.cfg_fx_lbtt`、`CWServ.skef10_hk_cwzb`、`CWServ.qhf9_quote`。各独立调用params=['001','00700','',0]；另独立body={'标签':'中文'}；再单独同时传body和params。

**预期**：普通→base、cfg→ladder_base、hk/qhf9→mg_hk_base；query Entry精确编码；POST compact UTF8 JSON，字符串'001'不转1、末项0仍number。body+params O/N ValueError且零请求；R typed输入禁止歧义或InvalidArgument。不能全路由到base。

**计数/资源**：四路由+中文body共合法5 HTTP，非法0；中文body走普通entry/base；不全局修改DNS/代理配置，R0。

### COMP-050 — F10历史资金流的请求与日期方向

**映射/fixture**：T14、T20；`050_flow_history_f10.json`；S+B+L。

**前置/输入**：history('sh600000',days=5,start=1,count=1)，F10服务升序dates[11-12,11-13,11-16]，每行有未知N099；另以乱序dates[11-16,11-12,11-13]确认本地仅反转、没有sort；days=7非法分支。

**预期**：entry CWServ.tdxf10_gg_sj、Params=['zjlx_sj','600000','5','600000']；先reverse为16/13/12再slice得13，未知N099保留。乱序分支反转为13/12/16再取index1，结果必须12；旧实现不先按日期sort，N保持。R按明确反转契约取同结果。O/N/R不发052D category22或0FFC冒充历史；days7为`ValueError('days must be one of: 3, 5, 10, 20, 60')`，calls=0。真实空/多区间响应证据仍L。

**计数/资源**：每个合法分支HTTP1，HQ业务0；R总预算/O/N独立请求策略保持，R0。

### COMP-051 — EX完整头、半包与有界解压

**映射/fixture**：T24、T27；`051_ex_frames.bin`及`.json`；S+B+L。

**前置/输入**：frame层正常样本为16B `<IIIHH>`头magic7654321、seq7、method0x23FC、zipsize=4、unzipsize=4，body=`01 02 03 04`；socket分片[5,4,7]头再分body，该样本只断言frame不冒充合法逐笔payload。坏分支直接向frame decoder交付少1byte的body、zlib尾截断、头声明unzipsize65535但实际展开65536。transport分支发送完整头后仅交付3byte body即EOF；业务整链另用合法逐笔响应。

**预期**：正常R/O/N拼完整头后解析，返回body四字节，不能把首次recv不足16当完整；直接frame错误O/N为ProtocolError，R为Protocol，明确长度/解压来源。transport读body遇EOF时O/N为`TransportError('连接被服务器关闭')`，R为Connection且保留EOF原因，不能混为decoder错误。setup沿基线92字节，不能拿HQ握手替代；关联只使用已验证字段规则，不臆造所有回包method相等。

**计数/资源**：frame直接解码0网络；transport整链setup1、业务1，并关闭自动重试以固定EOF结果。坏连接不复用错位数据，R0。真实不同host关联字段覆盖仍L。

### COMP-052 — MAC未知合法bit占槽，保留bit不占

**映射/fixture**：T25、T27；`052_mac_bitmap_unknown.bin`及`.json`；S+B+L。

**前置/输入**：20字节bitmap固定置bit47（基线未定义，<151）、bit72（BID2_PRICE）、reserved152；bitmap hex=`0000000000800000000100000000000000000001`。record data依升序为`DE AD BE EF`和12.5f32小端`00 00 48 41`。分支0x122C单独请求bit128。

**预期**：R BID2_PRICE=12.5且未知bit47原4bytes保留；O/N对应已知bid2_price=12.5，既有dict不擅自增加未知键。reserved152不增加record宽度。0x122B使用20B bitmap；0x122C高于127位必须原显式拒绝，不能与exclude_flags重叠。

**计数/资源**：codec测试0网络；每record消费8数据字节而非12/4；截断为错误不越界，R0。真实未知bit非空捕获仍L。

### COMP-053 — MAC-EX香港逐笔路由与单位

**映射/fixture**：T24–T27；`053_hk_transaction_routing.json`；S+B+L。

**前置/输入**：market31/code00700今日逐笔EX记录price123450、volume8、nature1、09:31:02；历史指定2026-11-13。再market23、24、25、26参数化。

**预期**：31走EX今日23FC/历史2406，映射price123.45、vol8、bs_flag1、trade_count0、秒保留。衍生品23–26不走HK股票EX分支，维持MAC路径；R不因统一市场enum误路由。O/N结果类型及所有字段不变。

**计数/资源**：每参数只选一协议driver，不双发；EX/MAC独立setup和close，R0。真实品种volume口径覆盖仍L。

### COMP-054 — MAC-EX目录fallback与宏观歧义

**映射/fixture**：T26、T27；`054_extended_catalog_fallback.json`；S+B。

**前置/输入**：MAC goods_list空，EX分市场目录提供目标一条；宏观检索分支同名两条不同market/code，再refresh时只剩唯一条。排名分支提供3板块净流入[10,30,20]请求top1。

**预期**：O/N保留空目录触发EX定位、歧义处理与refresh原异常/结果（执行基线冻结）；R歧义必须InvalidArgument/MetadataUnavailable，不任择一条；ranking必须先全量聚合后取30，不取源端第一页首10。未实现sort参数的R拒绝，N保留原行为直至单独批准。

**计数/资源**：trace列出每市场范围和fallback顺序；失败空cache不得让refresh无效；R0。

## 7. 错误链、单位与证据缺口

### COMP-055 — 原异常链与取消清理失败

**映射/fixture**：T05、T08、T12、T18；`055_error_causes_cancel.json`；S+B。

**前置/时间线**：脚本连接断开/响应超时/队列满三个独立分支；第四分支Python KeyboardInterrupt同时native cancel确认失败；R等价token cancel确认失败。错误context带固定command054C/identityA/endpoint。

**预期**：O/N分别为既有ConnectionClosedError或TransportError、ResponseTimeoutError、PoolBusyError，保持class层级和cause。KeyboardInterrupt仍为外层，cleanup错误挂cause不替换成普通Timeout；R CancelUnconfirmed保留原请求和cleanup原因。正常取消确认只交付一个终态。

**计数/资源**：失败后不得继续recovery；取消控制1次、确认结果1次；资源未确认时诊断不可宣称0，受控清理后R0。N信号轮询不跨阻塞等待持GIL。

### COMP-056 — 时间对齐和量单位不可一刀切

**映射/fixture**：T10、T13、T14、T24、T25；`056_time_units.json`；S+B。

**前置/输入**：bar start=2026-11-16 23:58、period5分钟、bar_time=end；HQ/EX按同日时分回绕，MAC按真实duration；tick price10、volume2；category8 HQ参数7/EX固定3/MAC times2。

**预期**：O/N的HQ/EX结束00:03仍11-16，MAC00:03为11-17；category8分别偏7/3/10分钟。R保留原始source_date=11-16、source_time=23:58及协议周期；派生真实end_time应为11-17 00:03，并保留与旧投影的区别，不能丢掉跨日信息后把wrap当真实日期。tick交易额=2000元（手×100），不将K线所有量也再统一乘/除100；原始price/volume/raw字节保持可追溯。

**计数/资源**：纯时间/单位变换0网络；非分钟周期和start模式不偏移；R0。

## 8. 需要补齐的证据与实施入口

1. **基线运行证据**：本文件所有B项都需要实际执行旧Python并存expected；AST快照覆盖全表面，56案例覆盖重要行为，二者不能互相替代。尤其旧child异常、MAC宏观歧义，不在运行前臆定具体旧值；已写定的源码异常类/message也仍须运行核对。
2. **合成golden**：COMP-006–011使用可控时间/请求树；COMP-022/043使用可生成的大小边界；COMP-030/031/040/046/047/051/052需独立构造字节和数学expected。所有生成器与fixturehash进入证据，不能用新实现回写expected。
3. **真实服务器缺口**：F10日期方向/不同空表、EX响应关联、MAC未知合法位、HK品种单位、财报样本、订阅跨日与gap需追加有来源日期/host/profile的录包；网络验证另设opt-in，不阻塞默认离线规格实施，也不能用一次线上成功代替边界测试。
4. **P0决策缺口**：price规则profile的有效日期和来源、严格财报容错范围仍需定版；新Rust恢复预算耗尽已固定为LimitExceeded。O/N既有行为不因这些新决定自动改变。D未解除的案例不得标为稳定能力已验收。
5. **运行入口**：每个case应同时能由Rust纯测试和Python兼容测试按ID选择，脚本打印实际case数、参数、source SHA、fixture hash及profile；不能只用文件存在或测试名字包含关键字证明覆盖。目标测试组为 `sdk_compat_rust`、`sdk_compat_python_baseline`、`sdk_compat_python_migrated`，名称为计划标签，不表示现有命令已经可运行。

关键源码依据：[Python客户端](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/client.py)、[runtime期限与关闭](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/crates/eltdx-runtime/src/engine.rs)、[批量恢复](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/_batch.py)、[业务helpers](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/helpers/core.py)、[资源下载](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/resources.py)、[行业解析](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/offline/industry.py)、[F10解析](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py)、[MAC位图](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/mac/bitmap.py)、[HK逐笔路由](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/ex/_hk_transaction.py)。
