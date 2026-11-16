# 实施决策、默认值与冻结检查表

> 本文收束 [公共契约](public-api.md)、[内部执行规格](execution-and-compatibility.md)、[HQ 规格](hq-api-spec.md)、[扩展规格](extension-api-spec.md) 的实施选择。`D` 表示本提案已明确的选择，`E` 表示必须取得外部或运行证据才能解除的限制；它们不是实现完成记录。

## 1. 决策状态的含义

| 状态 | 可以做什么 | 不能据此声称什么 |
| --- | --- | --- |
| 设计已确定 | 按本文件写接口、行为测试和实现 | 不代表已编译或已验证 |
| 兼容基线已静态提取 | 比较已有参数/字段/导出 | 不代表旧行为已运行录制 |
| 证据待取得 | 可以实现有限、明确失败的路径及取证工具 | 不把未知字段、市场规则或平台支持猜成已支持 |
| 验收已通过 | 必须附命令、结果、提交与样本来源 | 不能用文档复选框或自测名称代替证据 |

开发者发现源文件与规格冲突时，先标明具体冲突，不修改黄金样本来迎合实现。本文的“已确定”是这份提案的选择；未来可变更，但需同步受影响接口、案例和发布说明。

## 2. 总体与兼容决策

| ID | 决策 | 适用与验证 |
| --- | --- | --- |
| D01 | 新增一个 `eltdx` crate，同时提供公开 API 与共享业务实现；复用 protocol/runtime | T04；生产依赖闭包不包含 PyO3 |
| D02 | 第一版只公开同步 API | 不发布空 async feature；P6另做awaitable completion |
| D03 | 默认 feature 空集；HQ及其必要内部ZIP/资源解析可用 | 公开offline/F10/EX/MAC分别feature，mac带ex |
| D04 | Rust和Python独立安装，二者不调用对方入口 | 双向干净consumer，Python只装wheel |
| D05 | Python所有既有公开外观和默认行为保持 | 签名快照+三方行为对照；缺陷修复单独决策 |
| D06 | 正常Rust总预算；Python legacy原请求预算与各层限制 | 两页各5s、总8s的结果必须不同且可解释 |
| D07 | 普通Rust无legacy开关；绑定经doc-hidden pub桥接 | 实际跨crate编译，无PyO3泄漏 |
| D08 | build纯验证；clone共享owner；Rust关闭后不重开 | 无网络/线程build测试与并发close测试 |
| D09 | Python重开创建新generation；旧pin/订阅仍失效 | Python旧服务代理取得新generation，旧资源不复活 |
| D10 | 默认精度以owner隔离，整个operation固定snapshot | 禁止新路径读写全局精度表 |
| D11 | 原始整数/f32及单位来源保留，未知值不猜 | 禁止所有数值先转f64再冒充无损 |
| D12 | strict batch默认；显式partial只表达确认缺失 | 损坏帧/连接/取消不能伪装missing |
| D13 | 可选补充来源逐项声明，Rust状态可诊断 | Python原降级形状保留，不新增必填字段 |
| D14 | 有界下载显式Complete/LimitReached；严格下载单列 | 精确cap不是EOF；Python截取bytes不变 |
| D15 | 本地write既有追加语义保持 | 不批内排序/去重；空输入也先拒绝残尾 |
| D16 | 新Rust默认不隐式写用户目录 | Python既有缓存路径与副作用按兼容层维持 |
| D17 | 扩展域单页无法证明 EOF 时返回 `PageCompletion::Unknown` | 完整解析成功与已遍历全部分开；短页/空页不擅自推断终止 |

## 3. 新 Rust 默认预算

此表为拟议首版 Rust 默认值，不覆盖 Python 旧默认值。Python 原值来自静态快照与行为案例；禁止仅因为数字相同就认为计时或计数语义相同。

| 配置 | 新 Rust 默认/上限 | 校验与含义 |
| --- | --- | --- |
| 普通请求 timeout | 8s | 从公开方法入口计整次操作；配置必须正数 |
| 单次 `OperationOptions` timeout | 未指定，继承配置 | 显式0允许立即超时；与absolute deadline取更早 |
| close timeout | 1s | owner总期限；各池只用剩余量，重复close不延长 |
| operation cleanup timeout | 1s | 一次操作取消/超时后的清理总预算；与已开始close的剩余期限取小，Python原确认范围不变 |
| HQ每逻辑请求尝试数 | 2，含首次 | 不等于额外重试2次；F10默认单次HTTP应用尝试，无自动POST重试 |
| 操作额外尝试预算 | 32 | 首次必要页不算额外尝试；wire重发、缺项诊断及fallback消耗 |
| 资金流空值探测 | 每证券最多3；整批`min(n,8)*3`，最多24 | 同时消耗32的额外预算，哪项先耗尽先停 |
| 公开批量输入数 | 最多65535个 occurrence | 重复输入计数；内部按各命令wire上限拆分 |
| 规范化身份逻辑缓冲 | 2 MiB | HQ每项1字节market+6字节ASCII；其他域按编码长度累计，checked加法；不是总堆内存上限 |
| 普通池 | 1个连接/1个服务器，连接并发1 | 扩大须验证host/全局连接上限 |
| HQ owner总连接 | checked `4 × pool_size`；builder可配置 `max_connections` | 至少pool_size，跨普通/竞价/资金流/endpoint池共享物理permit；容量满返回LimitExceeded，不覆盖Python既有Engine限制 |
| 竞价/资金流地址 | 分别使用独立打包数字地址目录，可分别覆盖 | 普通endpoints不覆盖专属池；同地址仍不同连接，getter/clone复用同域池 |
| 资金流调度 | 域内操作串行，单wire一个代码，每工作块最多75项 | admission计入总期限；每次备用探测关闭并确认后才复用，不建立75个隐含连接 |
| heartbeat | 30s | 禁用通过显式None，不能用0模糊表达 |
| pending请求 | 256 | 配置为0返回InvalidArgument |
| 订阅队列 | 1024个事件、64MiB | 每订阅预算；与runtime原始帧预算分开 |
| 订阅renew | 7s | 显式None禁用；建订阅token不长期保存 |
| gap策略 | ReportOnly | 独立gap控制槽；不默默标为连续 |
| raw | Omit | 不保留逐条hex；不承诺零临时分配 |
| 缺失策略 | Error | AllowPartial才返回ConfirmedMissing槽位 |

普通工作量、wire尝试和额外尝试是不同计数。例如3个正常页面各首次成功：wire次数3、额外尝试0；其中第2页重发一次：wire次数4、额外尝试1。发生一次缺项补查本身也计额外尝试；该补查再重发一次则再加1。测试必须逐步断言这些数值，避免重复扣减或漏扣。

共享额度的预留、开始与归还规则见[执行规格3.4节](execution-and-compatibility.md)。正常Rust恢复预算耗尽为 `LimitExceeded`，损坏协议仍为 `Protocol`，单请求达到尝试上限仍保留最后实际网络错误。Python旧恢复耗尽的 `ProtocolError` 不随之变化。

### 3.1 HQ 扫描默认值

| 领域 | page size | max pages | max records | 终止条件 |
| --- | --- | --- | --- | --- |
| 证券目录 | 1600 | 200 | 320000 | 合法空页；短页仍继续 |
| K线完整收集 | 800 | 200 | 160000 | 合法空页；倒转页序，不倒转页内 |
| 成交完整收集 | 1800 | 100 | 180000 | 合法空页；保留混合记录与源顺序 |
| 分类排行收集 | 80 | 200 | 16000 | 按该命令短页规则，不套成交终止规则 |
| 特殊限价扫描 | 按wire字段允许值与query指定 | 200 | 10000 | 依据该命令响应计数/EOF规则 |

此表的页数/条数是安全限制，不保证服务器拥有或返回这么多数据。触限且尚不能证明目标完成，严格collect返回LimitExceeded；单页API保留合法cursor。用户可显式设置其他正且有限的上限，仍受总期限和整数边界约束。Python `None` 页上限经legacy策略处理，不强套此表。

### 3.2 wire与资源限制

| 范围 | 数值/规则 | 来源与策略 |
| --- | --- | --- |
| HQ代码页 | 最大1600 | 现有protocol常量；不能窄化截断 |
| HQ K线页 | 最大800 | `MAX_KLINE_PAGE_SIZE` |
| HQ成交页 | 最大1800 | `MAX_TRADE_PAGE_SIZE` |
| HQ股本批次 | 默认75，单命令最大200 | 按已有常量；批量服务自动切分 |
| HQ文件块 | 默认30000，最大60000 bytes | 用户总max_bytes单列，offset checked |
| stats压缩体 | 32MiB | 上限加一探测；大于上限拒绝 |
| stats条目 | 最多128，单条32MiB，累计64MiB展开 | 分配/展开前与过程中执行 |
| 财报ZIP | 64MiB、64条目 | 专业财报与stats不能共用错误上限 |
| 财报DAT | 单DAT256MiB；每行最多4096float；总4Mi float | report_size须4倍数，索引范围检查 |
| 新Rust财报ZIP累计展开 | 256MiB，覆盖所有成员 | 单一内部ZIP解码器对所有成员做流式完整性检查时的CPU/输出总预算；这是新Rust保护，不施加到Python旧选中DAT路径 |
| 新Rust本地读取 | 默认256MiB | ReadOptions可显式设置正有限值；不向Python旧直接DAT读取新增该上限 |
| 新Rust扩展下载 | 默认64MiB；chunk30000 bytes | DownloadOptions显式配置；HQ单块仍不超过60000，未证实EOF不交付完整结果 |
| F10响应 | 新Rust暂定16MiB解码后上限，可显式配置 | 设计新增限制，Python旧默认不受影响；上线前E03验证真实样本是否适合 |

F10 16MiB 是本提案给新 Rust 的保护默认值，不是服务器协议上限，也不是现有 Python 限额。HTTP压缩数据在解压后仍受该限制；压缩输入及header还应由所选库设置有限上限。真实合法响应超过此值时，先记录样本，再调整默认或由用户显式配置，不能无声截取JSON。

## 4. 内部缺口与需证据事项

| ID | 问题及当前可执行决定 | 取得什么证据后可以解除限制 | 任务 |
| --- | --- | --- | --- |
| E01 | 涨跌停的3套规则冲突；不能选任意一套称权威。新Rust无已审核证券/日期规则时返回未知 | 有效日期、证券类别、ST/上市阶段和交易所原规则的来源；边界样本 | T03、T16 |
| E02 | category22有parser但缺非空实测，不作为稳定历史资金流入口 | 非空capture、请求参数、字段单位/日期与独立核验 | T02；稳定历史仍走F10 |
| E03 | F10 HTTP库尚未验证 | MSRV1.89、TLS/SNI/代理/IPv4/IPv6/取消/上限矩阵实际结果及lock | T19 |
| E04 | ZIP库尚未验证 | stats与财报独立语料、CRC/重名/加密/超量/截断的实际结果 | T17先完成，T22扩展 |
| E05 | crate名称、版本、注册表发布链未确认 | 实际注册表查验、manifest版本关系及独立consumer | T29 |
| E06 | 新DTO/serde格式未编译验证 | Rust类型定义、字段含义、unknown/nonfinite/bytes编码的roundtrip和编译结果 | 各域及T28 |
| E07 | 跨语言时间/取消兼容未录制 | 本地clock/server三方对照；旧/新Python相同，Rust按新策略 | T02、T08、T12 |
| E08 | EX/MAC服务器响应关联规则部分只解析未校验 | 对应profile录包与迟到/错seq/合法push样本 | T24–T26 |
| E09 | 旧Python动态导出、继承及decorator外观未运行提取 | 真实import/inspect/dataclass/enum快照 | T01–T03 |
| E10 | wheel/platform/MSRV支持仅为目标 | 对应平台干净consumer实际安装、查询、close与产物hash | T28–T30 |

E01不阻断证券目录、原行情等无此规则依赖的域。E02不阻断F10历史资金流。选库/注册表缺口不以问用户偏好解决，应由实施任务取得事实并记录；若外部证据不足，则保持明确限制，不编造通过记录。

## 5. 接口冻结的最小交付包

每个域开始实现前，应能在一次审阅中找到以下七项：

1. Rust签名、query字段、类型/单位/默认值和feature；不含未定义的占位返回类型。
2. Python原签名/别名/模型字段的基线记录，以及旧外观向Rust的投影。
3. 请求顺序、分页停止、重复/缺失、raw来源、缓存与文件副作用。
4. 超时、取消、重试和close在两种profile下的区别。
5. 至少一个非空正常输入输出和一个失败/边界案例；均有fixture ID、预期及出处。
6. 依赖前置任务、需要修改的源码/测试路径和本域可独立运行的命令。
7. 仍需外部证据的项及阻断范围；明确不把其算入稳定支持。

T01–T03优先冻结契约并录制基线，随后实施各域。不得以文件长度、接口数量或删除Python行数作为完成标准。

## 6. 本轮文档补充的验证边界

T16实施补正（2026-11-16）：M60 `CalendarService::today` 返回 `Result<TradingDate>`。T10已实现的Clock::unix_seconds本来就返回Result，系统时钟也可能超出受支持日期范围；返回裸TradingDate无法完整传播错误，且不得panic或编造日期。此处在未发布的新Rust入口修正签名，Python旧today由T18按原兼容边界映射。新增失败时钟源码用例，执行延期；不是已验证兼容证据。

可以验证：静态快照重新生成一致、所有源码链接对应固定提交、Markdown链接与表格/围栏、MkDocs严格构建、根文档/包内镜像一致、已有文档测试。

不能据此声称：拟议Rust API编译成功、所有业务与Python一致、线上行情正确、性能提高、wheel支持某个平台或已发布。上述结论必须等待实现后的独立验收。
