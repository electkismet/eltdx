# T27：EX、MAC、MAC-EX 迁移与兼容边界

本索引记录已写入的源码及保留的 Python 适配边界。T27 的实现、用例、消费夹具和逐项映射源码已写齐；所有编译、测试收集、运行、安装和网络验证均等待 T01–T30 全部实现源码完成。以下映射和用例是待执行证据，不是验收结果。

## 两个独立产品入口

Rust 调用 `eltdx` crate 的 `ex`、`mac` 功能和各自客户端，不调用 Python。Python 继续使用原 `eltdx.ex.ExTdxClient`、`eltdx.ex.MacExClient`、`eltdx.mac.MacClient`；发行 wheel 内的绑定调用 Rust 策略及共用算法。普通 wheel 用户无需自行安装 Rust；这一点仍须 T28/T29 的干净环境消费验证。

Python 对象约定包括可替换的模块函数、类、socket、锁、executor、解析器和模型构造器。绑定接受这些对象并保留原读取与调用顺序。严格 Rust 入口采用类型化参数、总期限、限额、关联规则和独立 owner；不能将其新增拒绝规则直接引入旧 Python 入口。

## 实现归属

下列 Rust 路径相对 `crates/eltdx/src/`，绑定路径相对 `crates/eltdx-python/src/`。

| 范围 | Rust 与共用实现 | Python 绑定与入口 |
| --- | --- | --- |
| EX 请求、响应、目录、行情、K 线、分时、逐笔 | `ex/commands.rs`、`ex/layout.rs`、`ex/legacy.rs`、`ex/client.rs` | `extension_commands.rs`、`extension_parse.rs`、`extension_query.rs`，`src/eltdx/ex/commands/`、`client.py` |
| MAC 请求与全部响应、字段、特殊值与日期 | `mac/commands.rs`、`mac/bitmap.rs`、`mac/layout.rs`、`mac/routing.rs` | `mac_commands.rs`、`mac_parse.rs`、`extension_bitmap.rs`、`extension_rules.rs` |
| MAC 分页、复权、板块聚合排行、下载 | `mac/client.rs`、`mac/services.rs`、`mac/python_legacy.rs`、共用下载游标 | `extension_client.rs`、`extension_boards.rs`、`extension_adjust.rs`，`src/eltdx/mac/client.py` |
| MAC-EX 目录、商品回退、宏观、港股逐笔 | `mac/client.rs`、`mac/routing.rs`、目录与缓存实现 | `extension_catalog.rs`、`extension_query.rs`，`src/eltdx/ex/mac_client.py`、`_hk_transaction.py` |
| EX/MAC 帧与有界解压策略 | `ex/frame.rs`、`mac/commands.rs` 与对应 wire 格式 | `extension_frame.rs`，Python 显式异常与自定义解压适配 |
| 主机来源、显式保存、候选排序、探活 | `ex/config.rs`、`ex/legacy.rs` | `extension_hosts.rs`、`extension_probe.rs`，原文件上下文、线程池与 socket 适配 |
| 连接、重试、关闭、代次、结果提交 | `session.rs`、`__python_compat/extension.rs`，各协议客户端 owner | `extension_management.rs`、`extension_transport.rs`、`extension_lifecycle.rs` |

各分段的确切文件和公开调用记录见 `.superpowers/sdd/development-plan/task-27-report.md`。本页不提升能力矩阵的已验证状态。

## 模型、枚举与字段对象库存

| Python 原路径 | 保留对象 | 兼容承诺 |
| --- | --- | --- |
| `ex/models.py` | `ExMarketInfo`、`ExInstrumentInfo`、`ExInstrumentQuote`、`ExInstrumentBar`、`ExMinuteBar`、`ExTransactionRecord` | 原 frozen/slots dataclass、位置和关键字参数、类型身份；`_raw=b""` 不参加 repr 和 equality |
| `mac/models.py` | `MacQuoteField`、`MacSymbolInfo`、`MacBar`、`MacTick`、`MacTickChart`、`MacMultiTickDay`、`MacMultiTickChart`、`MacTransaction`、`BoardInfo`、`AuctionItem`、`UnusualItem`、`CapitalFlowData`、`BelongBoardInfo`、`ServerSession`、`KlineOffsetInfo`、`GoodsItem`、`FileMeta` | 原 17 个 frozen/slots dataclass；嵌套 dict/list 保持可变，各实例工厂独立，原 datetime/date/time 和默认值保留 |
| `mac/adjust.py` | `XdxrEvent` | 原 dataclass；category 默认 1，四个因子默认 0.0；计算方法已迁入绑定 |
| `ex/frame.py`、`mac/codec.py` | `FrameHeader`、`MacFrameHeader` | 五字段及原模块 pickle 路径保留；解析和帧策略已迁入绑定 |
| `ex/transport/sync.py` | `ExHostProbe` | 原 host/ok/latency_ms/error 及默认值；探活编排已迁入绑定 |
| `ex/enums.py`、`mac/enums.py` | 两个 `ExMarket` 类；`Period`、`Adjust`、`BoardType`、`ExBoardType`、`Category`、`SortType`、`SortOrder`、`FilterType` | 两个市场枚举数值对应但类身份保持独立；现有整型运算、成员与 pickle 路径保留 |
| `mac/bitmap.py` | `FieldBit`、`PresetField`、`FieldSelection` | FieldBit 枚举构造保留；field_name、组合与反向组合、迭代/长度/真假/包含/repr 策略在 Rust 绑定；`__or__`/`__ror__` 保留动态方法转发 |
| `ex/mac_models.py`、`ex/mac_commands.py` | 原模型/命令再导出 | 不创建替代类；MAC-EX 默认字段严格为 0、1、2、3、4、5、7；不同于 COMMON |
| `ex/__init__.py`、`mac/__init__.py` | 原包级公开路径 | `MacExClient` 保持惰性导出；其余属性及未知属性错误保留 |

FieldBit 的 93、94、136、139 保留个股 canonical 名 `BID2_VOLUME`、`ASK2_VOLUME`、`BID5_VOLUME`、`ASK5_VOLUME`；板块别名指向同一 Python 成员。Rust 解码保留 wire slot 与上下文含义。Python 字段选择保持首次出现顺序；Rust 位图扫描按 wire 位序，二者通过同一位插入/扫描原语而保留各自公开约定。

被动 dataclass/Enum 声明仅负责 Python 对象表示，不另存一套业务实现。禁止用 PyO3 类替代这些对象来制造类型、pickle、dataclasses、默认可变容器或反射兼容问题。Rust 自有模型从 wire 解码后通过其公开 Rust 类型返回，无需构造 Python 对象。

## 保留适配的原因及退出条件

| 保留边界 | 原因 | 可移除的前提 |
| --- | --- | --- |
| Python socket、socket_factory、显式锁及 executor | 原注入与替换接口，std socket 的阻塞调用释放 GIL；异常和上下文退出顺序可观察 | 经明确兼容设计保留全部注入与关闭约定，并有完成后的差分和并发证据；不能仅因严格 Rust 传输可用就删除 |
| Python `try/except/finally/with` | 活动异常、cause/context、清理范围、锁与 executor 退出语义 | 新桥接能保留异常身份、活动上下文和重入行为，且相应执行用例通过 |
| 动态模型、Struct、日期、数值、迭代与 codec hook | 旧接口允许原对象、任意精度整数、非有限数、自定义回调和惰性求值 | 所有被支持替换点已有等价公开适配并通过回归；严格 Rust 的限制不能代替旧行为 |
| 纯 dataclass、Enum、再导出与惰性导入 | 原类型身份、反射、pickle 和模块路径属于 Python 产品接口 | 通常长期保留；改变它们应作为单独兼容性变更，不能作为本次迁移收尾手段 |
| `FieldSelection.__or__` 与 `__ror__` | 原方法动态调用 `self.__add__`/`self.__radd__`，允许实例/类替换 | 只有动态调用链不变时才可搬走；底层实际组合策略已经在绑定中 |

共用 owner 只管理 admission、连接代次和结果提交；它不接管任意 Python 自定义 socket 的物理清理。严格 Rust 的 worker 与取消确认承诺不能直接宣称适用于 Python 自定义回调。真实登录响应规则与可选帧关联证据尚未取得，合成短 ACK 和旧 Python 宽松登录不构成生产证据。

中断后的自定义 socket 关闭失败会保留在连接的私有清理队列；该引用独立于已经退役的代次，临时 MAC 建连失败和后来替换连接也不会丢掉它。显式 `close()` 先遵循当前连接原关闭规则，再重试这些资源；失败保留以供再次重试，同一资源不重复清理，清理回调重入时跳过在途记录。此队列不启动后台任务，不把关闭失败改称成功。若自定义连接拒绝私有状态赋值，原信号异常带保留资源及状态错误供调用者处理。

## 待执行对象兼容证据

`tests/test_native_extension_objects_contract.py` 覆盖原字段运算、反向运算、不支持操作的 NotImplemented、live isinstance/constructor、动态转发、容器回调、repr 求值、四个别名、预设顺序、23 个业务 dataclass、帧/探活对象、默认容器、pickle、市场枚举身份与包级导出。`crates/eltdx/tests/mac_routing.rs` 从独立字面量核对全部字段、别名、预设、市场路由和请求标志。

其余已写测试源码覆盖请求/响应、分页、目录、板块、复权、日期、帧、主机、探活、传输、管理与生命周期。完整清单已记录实际 Rust 符号、来源 hash、fixture 和待收集节点，最终统一执行后才记录通过、失败和真实平台结果。

## 独立 wheel 消费者源码

`tests/consumers/python-wheel/consumer.py` 仅导入 stdlib 和已安装公开 eltdx 包。扩展部分使用三个独立本地 TCP 连接和字面量请求预期：EX 的 92 字节 setup、非空市场/目录/报价与 raw；MAC 三次 setup、未知字段槽跨越/AH-code 及服务器日期/时段；MAC-EX 登录及默认七字段非空报价。所有响应分片发送，各连接必须显式 close、再次 close 并观测对端 EOF。合成登录仅验证 Python 原行为，不解决严格 Rust 生产默认登录规则缺口。实际隔离安装和运行待 T28/T29 统一验证。

## 全部目录记录的逐项源码映射

`tests/contracts/manifests/extension_migration_sources.tsv` 是可逐行审阅的映射源：EX 的 11 个业务入口加连接、MAC 的 23 个业务入口加连接、MAC-EX 的 13 个业务入口加连接，共 50 行。每行绑定实际 Rust 文件/类型/方法、Python 入口文件、native 绑定文件、Python 合同节点和 Rust 行为节点。三个 MAC 别名保留原公开转发路径，其实际业务入口与同名目标共用绑定；管理、配置、对象和生命周期按上表及 shared inventory 关联，不伪装成新增 wire 命令。

`rust_sdk_migration.json` 中这 50 项的 `authored_evidence` 保存完整公开路径、直接来源文件 SHA256、用例/fixture 来源和保留适配的原因及退出条件。`extension_source_inventory.shared_source_sha256` 另外固定共用 owner、布局、位图、路由、分页/复权/下载、模型/枚举/再导出和独立 wheel consumer。哈希记录的是该源码检查点的文件字节；后续修改须追加新检查点，不能将旧 hash 宣称为新候选版本证据。

逐项源码归类为：业务编排已迁入 Rust native 入口；Python 对象、注入、异常活动作用域和同步上下文按本文有意保留适配；category22 等实验能力维持原矩阵标签；生产登录规则及可选 setup 帧证据继续列为未验证缺口。为避免将源码当成验收，原权威目录的 `planned` / `unimplemented` 状态、26 wire / 25 registry / 124 capabilities / 37 fixtures 计数和冻结 source_revision 均不因本段写作提升。新字段统一为 `source_only_unverified`，最终运行后才能按门禁更新验收状态。

下一阶段 T28 补独立 Rust EX/MAC/MAC-EX 消费者及 feature/MSRV/平台矩阵；T29 补包版本和内容验证；T30 补压力、性能、维护和最终覆盖材料。T19 的依赖故障验证继续在全源码完成后执行。当前不能据此承诺任何平台编译成功、已安装兼容或生产服务器登录成功。
