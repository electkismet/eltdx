# 纯 Rust SDK 验证与发布门禁

本文规定拟议 `crates/eltdx` 的实现验收方法，配合 [架构设计](architecture.md)、[开发计划](development-plan.md)、[公开 API](public-api.md) 和 [能力矩阵](capability-matrix.md) 使用。验证对象包括复用的 `eltdx-protocol`、`eltdx-runtime`、新增业务 crate，以及调用该业务 crate 的 Python 绑定。

**状态说明：本文是开发设计，不是通过报告。** 本轮只读源码、测试、工作流并编写文档，没有执行本文命令，也没有取得新 SDK 的编译、联网、回放或压力测试结果。本文出现的 `crates/eltdx`、其集成测试、consumer 工程和 Rust 压力工具都是待实现交付物；命令用于实现后的验收，不表示这些目标现已存在。当前可以确认的是仓库中已有相应源文件、测试定义和 CI 配置，不能据此宣称当前提交已通过 CI。

## 1. 证据等级与完成条件

2026-11-17 T28 源码交接：新增独立 Rust EX/MAC/MAC-EX 消费者、32 个 feature 子集与三平台/MSRV 脚本、Rust-only 容器入口，以及构建 job 外的 installed-wheel consumer 和 native 字节核对。逐项入口和隔离强度见 [T28 源码索引](t28-verification-index.md)。这些文件已经写入，全部尚未运行；本文早期“待实现”描述不构成新的执行证据。

每项能力应分别登记以下证据，不用一个“支持”标记合并：

| 等级 | 要求 | 不能替代的证据 |
| --- | --- | --- |
| 已设计 | API、协议路由、单位、错误及资源限制有书面约定 | 没有可执行实现 |
| 已实现 | Rust 业务实现和公开入口存在 | 不能仅靠 Python 方法仍可调用 |
| 已编译 | 最小、默认、目标 feature 与 MSRV 下可编译 | 没有行为正确性结论 |
| 已离线验证 | 黄金编码、解码、业务投影及错误用例通过 | 没有 socket 生命周期结论 |
| 已回环验证 | 真正 Rust 客户端连接本机假服务器，覆盖正常与故障 | 没有真实服务器兼容性结论 |
| 已外部消费验证 | workspace 外的 consumer 使用 path、打包目录和最终注册表包通过 | workspace 内测试不能代替 |
| 已采样兼容验证 | 指定日期、服务器配置、市场和命令的授权采样通过 | 不能推导其他服务器或所有市场都支持 |
| 已发布 | 发布包通过上述适用门禁，并记录版本、包校验值与提交 | 本地 wheel 成功不是 Rust 包发布成功 |

每次验收记录完整 Git SHA、工作区是否有修改、Rust 版本、OS/架构、feature 集、Cargo.lock 校验值、命令、退出码、日志路径及日志摘要。失败、超时、未运行和环境不适用必须分开记录。对既有 Python 行为有意调整时，能力矩阵应链接变更决定及对应新旧测试，不能通过改 expected 文件悄悄接受差异。

2026-11-17实施状态补充：`crates/eltdx/examples/hq_query.rs`和`tests/consumers/rust-sdk/`已经有首批源码。后者使用独立Cargo workspace，仅直接依赖eltdx，以std实现本机HQ假服务器，覆盖非空目录分页、重复输入/反序行情、raw/source与关闭；README列出最终统一验收命令及workspace外复制要求。**尚未解析依赖、编译或运行，也未证明无Python环境或打包后的独立消费成功。** 其他章节的拟议consumer模板/平台矩阵仍是待执行要求，不能将这些源码当作通过报告。

沿用开发计划的 P0–P6 阶段标识，为每一阶段登记增量证据；本文件不重新定义阶段范围。基础 Rust 独立消费、相应业务语义和适用故障用例全部具备后，才把能力从“已实现”提升为“已验证”。

2026-11-17补充：`tests/consumers/python-wheel/consumer.py` 已编写安装后独立使用
源码，标准库本机服务器覆盖非空目录/终页、重复快照/价格、股本分批/raw 和连接关闭。
该文件要求导入路径不在源码目录，支持检查 PATH 无 Rust 构建工具，并在真实成功后
才生成版本/平台/native SHA256 证据。尚未执行安装、启动服务器或 consumer。
最终 T28/T29 仍需使用独立干净环境证明无源码 fallback 和无编译工具；路径检查
本身不是安装成功的证据。HQ M01–M100 源路径见 [源码索引](hq-implementation-index.md)。

## 2. 当前可复用的测试资产及局限

| 已有资产 | 从源码确认的用途 | 新 SDK 仍需补齐 |
| --- | --- | --- |
| `tests/fixtures/7709/` 与 `tests/native/test_differential.py` | 测试声明覆盖当前25个注册命令；保存请求、响应、上下文、类型化 expected；包括部分错误、空页和原始精度用例 | Rust 原生 runner；非空业务样本；EX/MAC/F10 语料；独立于 Python 的消费测试 |
| `scripts/ci/run_wire_golden.py` | 调用 Python fixture检查、差分和契约测试 | 文件中历史描述仍称21命令；以实际 registry/fixture 集合为准，不用标题当覆盖证明 |
| `eltdx-protocol` 单测、属性测试；`eltdx-runtime` 单测、`loom` | 协议编解码、状态机、不变量及部分并发模型 | 新业务编排、可选模块、公开句柄、跨 feature 行为及真实 socket 路径 |
| `tests/integration/test_loopback_faults.py`、`test_loopback_concurrency.py` | Python 调用 native Engine 的握手、并发启动和回滚场景 | 不经过 PyO3 的 Rust socket 回环测试；EX/MAC 各自握手配置 |
| `tests/stress/` 与 `build_stress_evidence.py` | 有10万请求、反复关闭、线程/RSS上限场景及按提交聚合 Linux/Windows 记录的机制 | Rust SDK独立运行版本；业务分页、句柄及推送生命周期；具体提交的新证据 |
| `test_ex*.py`、`test_mac.py`、`test_macro_indicators.py` | 请求字节、字段位图、价格日期、路由、分页、切换、宏观解析等断言 | 不能把 Python mock 成功视作 Rust实现或真实服务器正确 |
| `test_f10*.py`、`test_news.py` | HTTP行为、ResultSets、字段标签、大量入口参数及边界 | wrapper参数测试不证明响应模式、真实网关可用性或 Rust HTTP实现 |
| `test_financial.py` | ZIP前置限额、重复DAT、财报字段与日期 | 完整损坏语料、大型真实样本、Rust资源下载的全过程限额 |
| `test_cli.py`、`test_mcp.py`、`test_http_server.py` | 参数分发、序列化、服务registry、WebSocket与可选适配器 | 这些适配器不应成为Rust库默认依赖；mock生命周期不代替真实清理 |
| `test_pages.py` | 目录、签名、字段文档和Python示例语法一致性 | 源码字符串匹配/签名绑定不能代替Rust doctest、行为测试和consumer |

现有部分 expected 会调用同一个 native解码器生成。Python新绑定若仅包装新Rust实现，再比较“新绑定输出”和“同一Rust函数输出”，只能发现包装/序列化错误，不能证明协议正确。应同时保留迁移前已冻结的expected、人工核对的字节与独立录包证据。更新语料需要审查来源和语义变化。

### 2.1 两种内部执行策略的兼容证据

共享Rust业务算法不要求两种语言具有相同的期限和计数语义。实现应使用两个内部策略：`RustPublicExecutionProfile`面向正常Rust公开API，使用有限的整次operation预算；`PythonLegacyExecutionProfile`面向兼容绑定，保留旧Python逐请求期限及各业务路径的计数规则。后者不是正常Rust API的legacy开关，也不新增Cargo feature。绑定经`#[doc(hidden)]`的`__python_compat`桥调用同一业务实现；此桥不属于稳定公共API，不依赖PyO3，PyO3仍只存在于绑定crate。

| 行为 | `RustPublicExecutionProfile` | `PythonLegacyExecutionProfile` |
| --- | --- | --- |
| 期限起点 | 公开操作开始，包含lazy startup、排队、预取、子请求和重试 | 保留旧路径真实起点和覆盖范围；不把旧路径外的startup/预取强行并入新8秒总窗 |
| 分页wire请求 | 共用剩余operation期限 | 每个wire请求按旧配置获得独立期限，例如每次新8秒 |
| retry/page/missing恢复 | 按Rust操作预算和对应计数约束 | 分别保留旧retry、分页及missing补查计数，不合并成一个shared attempts池 |
| `max_pages=None` | 正常公开API仍需要有限operation预算与明确分页安全策略 | 保留旧Python无调用方页数上限含义及原EOF/终止条件，不偷偷变成Rust默认页限 |
| optional enrichment | 按Rust剩余总预算执行；总期限到则终止整次操作 | 保留旧补充源期限、失败降级条件及主结果投影，不能仅因Rust总窗耗尽改成Timeout |

将策略及计数也记入`tests/fixtures/sdk/policy/`，同一预设服务器时间线分别跑迁移前Python、新Python绑定和Rust公开API。最低补充用例：

- 两页响应各用小于8秒的时间完成，而总耗时大于8秒；Rust显式设置8秒operation预算，预期`Timeout`；旧Python及配置同样逐请求8秒的新Python预期成功，记录两次wire期限均重新起算。
- startup、证券精度/目录等预取后再请求主数据；逐项冻结旧计时起点，验证新Python不会因共享Rustoperation窗而缩短旧请求预算，Rust则验证总期限包含这些步骤。
- 批量缺失二分补查、网络重试和分页穿插：记录请求顺序和每类实际计数，新Python应匹配旧Python，而非消耗同一新增全局attempts池。
- `max_pages=None`跨过Rust公开默认安全页数后才返回EOF：旧/新Python输出一致，Rust按其公开限制返回相应终态；测试服务器仍设置外部总时限，避免测试自身无限运行。
- 主数据与可选补充各自及时成功但累计超过Rust总窗，以及补充失败触发旧降级：Rust和两代Python分别符合各自策略，不以统一超时或统一降级作为通过条件。

优先使用可注入单调时钟推进确定性时间；另保留少量真实回环时间测试，设置宽容差并完整记录实际期限。8秒只是此兼容用例的明确配置，不表示所有协议默认都是8秒。新Python禁止以新增Rust总预算静默改变旧行为；若将来要改变Python契约，应单独版本化和迁移，不能包含在本次实现复用中。

## 3. feature、MSRV 与依赖闭包

### 3.1 拟定配置

`eltdx` 的 `default = []`。HQ核心始终可用；它不是另一个名为 `core-HQ` 的可选 feature。最小配置表示未开启扩展功能，不表示 `no_std`、无异步运行时内部依赖或零第三方依赖。当前 `eltdx-runtime` 使用 Tokio 是实现事实；本期不承诺公开 async API。

| 选择 | 必须可用 | 不应由此额外开启的公共能力 |
| --- | --- | --- |
| 不写 features，或 `--no-default-features` | HQ同步客户端、基础模型、错误、配置、取消/关闭、统计资源及HQ helpers所需的内部有界ZIP解析 | EX、MAC、F10、公开离线文件API、模型serde派生 |
| `serde` | SDK结果模型的序列化/反序列化，按公共API约定执行 | HTTP、Python绑定 |
| `offline` | 本地二进制/文本文件解析及相关类型 | F10、EX、MAC |
| `f10` | F10 HTTP/TQLEX、动态表和相应业务入口 | EX、MAC、公共模型serde派生 |
| `ex` | EX客户端、线协议、市场和结果类型 | MAC与F10 |
| `mac` | MAC及MAC-EX，显式依赖 `ex` | F10 |
| `full` | `serde, offline, f10, ex, mac` 五项 | Python绑定、MCP、Web服务框架、公开async API |

F10内部使用 `serde_json` 是正常功能依赖，不要求开启名为 `serde` 的公共 feature；后者只控制SDK结果类型的序列化能力。`mac -> ex` 用于实现 MAC-EX，减少难以维护的 feature 交集。这一依赖应在文档和依赖图中可见。将来公开async API时，再增加真实实现、依赖和对照测试，不先发布空壳 `async` feature。

默认HQ的`read_stats`及相关helpers已经需要有界ZIP解析，因此最小配置不承诺排除ZIP依赖。ZIP库选型、压缩算法与feature限制在P2/T17消费统计资源前完成；P3/T19再完成HTTP库选型与连接行为验证，P3财报复用已验证ZIP能力并补DAT等专属测试。两项依赖决策分别记录MSRV、许可证、feature树和故障样本，不一并推迟至P3。

在所有以上选择中，生产 `normal/build` 依赖图不得包含 `eltdx-python`、`pyo3`、`pyo3-ffi`、`pyo3-build-config`、`maturin`。测试fixture生成工具可以独立用Python，但不能被Rust `build.rs`、运行时或者文档示例暗中调用。MCP/HTTP服务适配器另外打包，不要求最小消费者安装Python或启动子进程。

安装与开发门禁分开：Rust用户通过Cargo依赖已发布的`eltdx`即可，不需要Python、pip或maturin；Python用户安装受支持平台的预编译wheel只需`pip install eltdx`，不要求自行安装Rust工具链。Python源码构建、缺少对应wheel或开发绑定时才需要Rust与maturin，这个例外应在安装说明中单列，不能写成所有用户的前提。

### 3.2 编译矩阵

当前 workspace 的声明和 `rust-toolchain.toml` 均为 Rust 1.89/1.89.0。本方案以 **MSRV 1.89.0** 为门禁；只有明确变更版本政策后才能调整。新的依赖升级必须重新跑MSRV，不能仅依据本机stable可编译。

| 组合 | Rust 1.89.0 | 最新stable | 平台 |
| --- | --- | --- | --- |
| 最小、默认、`full` | check/test/doc/consumer | check/test/doc/consumer | Linux、Windows；macOS至少check/consumer |
| 五个单独feature，含 `mac` 自动带 `ex` | check/test | check/test | Linux；Windows覆盖offline和HTTP/二进制回环 |
| 五feature的32个显式子集 | check所有targets | check所有targets | Linux独立矩阵；失败不得被full掩盖 |
| `loom` | 独立runtime job | 独立runtime job | Linux即可，覆盖适用状态机；不等于socket测试 |
| `serde`模型文档/往返、`f10`文档 | test/doc | test/doc | Linux；完整功能再做Windows行为校验 |
| 打包后consumer与注册表consumer | check/run本地smoke | check/run本地smoke | Linux、Windows；发布前至少一次macOS |

`mac`隐式开启`ex`会让部分子集闭包相同；仍执行32个显式选择，确保manifest别名和调用者写法稳定。另测 `full` 别名。`cargo --all-features`只能增加覆盖，不能代替最小/默认/子集检查。

以下是**实现后建议执行、尚未执行**的基础命令，在仓库根运行。显式包列表避免 `--workspace`把PyO3 crate拉入独立SDK门禁：

```sh
cargo +1.89.0 check -p eltdx-protocol -p eltdx-runtime --all-targets --locked
cargo +1.89.0 test -p eltdx-protocol -p eltdx-runtime --locked
cargo +1.89.0 test -p eltdx-runtime --features loom --locked

cargo +1.89.0 check -p eltdx --all-targets --no-default-features --locked
cargo +1.89.0 test -p eltdx --no-default-features --locked
cargo +1.89.0 test -p eltdx --locked
cargo +1.89.0 test -p eltdx --no-default-features --features full --locked
cargo +1.89.0 clippy -p eltdx --all-targets --no-default-features --features full --locked -- -D warnings
cargo +1.89.0 fmt --all --check
```

预期：适用测试均运行并通过、没有因缺Python跳过核心验收；最小/默认功能相同。`fmt --all`读取workspace源码但不编译Python crate。再以stable执行同样适用检查，不能用stable结果覆盖MSRV失败。

32个组合的Linux Bash示例；此处循环只执行Cargo检查，不运行公网采样：

```bash
set -euo pipefail
names=(serde offline f10 ex mac)
for mask in $(seq 0 31); do
  selected=()
  for bit in 0 1 2 3 4; do
    if (( (mask >> bit) & 1 )); then selected+=("${names[$bit]}"); fi
  done
  args=(--no-default-features)
  if ((${#selected[@]})); then
    joined=$(IFS=,; printf '%s' "${selected[*]}")
    args+=(--features "$joined")
  fi
  cargo +1.89.0 check -p eltdx --all-targets --locked "${args[@]}"
done
```

依赖检查应先确认Cargo成功，再检查输出；不要让管道返回值吞掉Cargo错误：

```bash
set -euo pipefail
cargo +1.89.0 tree -p eltdx --no-default-features --edges normal,build --locked > minimal-dependencies.txt
cargo +1.89.0 tree -p eltdx --no-default-features --features full --edges normal,build --locked > full-dependencies.txt
if rg '(^|[[:space:]─├└│])(eltdx-python|pyo3|pyo3-ffi|pyo3-build-config|maturin)([[:space:]]|$)' minimal-dependencies.txt full-dependencies.txt; then
  exit 1
fi
```

在PR中审查 `cargo tree -e features`，确认没有通过其他依赖意外开启HTTP、额外TLS后端或公共serde派生。针对未启用模块，设置独立的compile-fail consumer证明相应模块确实不可访问；开启后再用正向consumer编译。它们应实际调用公开API，不能只搜索 `#[cfg]`字符串。

## 4. 黄金语料与离线差分

逐接口定义来自 [HQ 规格](hq-api-spec.md)、[扩展规格](extension-api-spec.md)；具体跨语言输入、响应、预期及fixture ID来自 [COMP案例](compatibility-cases.md)。[Python静态基线](python-baseline.md) 及其JSON固定签名、默认表达式、字段与导出。三类证据分开：静态快照检查接口形状，行为fixture检查语义，loopback检查实际执行和清理。

### 4.1 语料格式和来源

建议复用现有类型化canonical概念，所有新增SDK语料统一放在 `tests/fixtures/sdk/{hq,ex,mac,f10,offline,policy}/`，分别记录协议、文件与本地业务策略场景，由Rust测试通过稳定路径读取。既有 `tests/fixtures/7709/` 保持原位并继续复用，不为目录统一而复制或移动；新HQ补充语料放在`sdk/hq/`，fixture清单同时索引两处并检查case id唯一。每个case必须包含：

1. 唯一case id、协议配置、命令id/entry、市场、调用参数、feature要求。
2. 精确请求bytes；HTTP则保存URL路由类别、方法、有效headers和请求body。动态字段如序号、日期或凭据要单列匹配规则，不能无差别忽略。
3. 原始响应：包括帧头和压缩体；HTTP保留状态、响应headers和原始bytes。拆包时间线另存，不改变原始内容。
4. 预期原始字段、规范模型、单位、日期时区、顺序、重复/缺失、错误阶段及稳定错误类别。
5. 来源 `synthetic` / `captured` / `legacy-frozen`，创建工具版本、原始文件SHA-256、采样日期和必要的服务器配置。合成数据不能标成真实采样。
6. 预期兼容策略：严格拒绝、保留raw并返回部分结果、业务状态失败等。未知字段必须有可观察的保存位置。

当前canonical区分整数、浮点位、bytes、日期、list/tuple、有序字典及缺失值；Rust runner应保留这些区别。Python类的module/qualname不是Rust业务契约，应由显式映射转成相同中性类型标识，而不是假装Rust有Python dataclass。

序号/时间相关用例通过可注入clock和sequence确定性生成。不要要求公网服务器返回固定序号，也不要只匹配payload而漏掉请求头错误。fixture构造器与被测解码器应独立；不能用待测编码器生成唯一请求期望后再检查自己。

### 4.2 比较规则

| 数据 | 比较方式 |
| --- | --- |
| 请求帧、压缩体、raw字段、未知字节 | byte-for-byte；确定性的压缩输出才比较压缩字节，否则比较固定录包解码和协议容许压缩输出 |
| wire整数/f32、bitmask、原始时间 | 精确整数和浮点bit；保留signed zero/NaN信息时按raw位比较 |
| 派生f64价格、金额、复权值 | 先精确对齐raw和公式；只有文档列出的计算允许绝对/相对误差，不对所有浮点统一放宽 |
| 批量、分页、F10列/行 | 保持业务顺序和重复；`missing`与`null`、空串与空bytes分开 |
| 错误 | 对齐阶段、稳定ErrorKind、上下文与可重试性；Python异常类名通过adapter映射，文本不用成为全部兼容契约 |
| 超时/关闭 | 按内部执行profile记录单调期限、起点和阶段；Rust总期限与Python逐请求期限分别断言，不靠固定sleep或精确毫秒相等断言 |

对lossless raw和规范模型分别断言。显示价格相同不代表原始值相同，JSON排序后相等不代表F10重复列顺序保留。

### 4.3 HQ必须增加的业务验证

复用现有25命令语料，并以实际注册表自动检查无缺项。现有部分normal/empty样本只证明空响应可解析；每个有记录命令应至少有非空、最小合法、分页边界、截断与上限case。

- 三种行情 `0x054c/0x0547/0x053e` 使用不同模型、请求和parser，不因统一quotes服务而互换。
- 批量返回按请求顺序关联，覆盖重复输入、乱序、缺失、全空及on_missing策略；返回 `BatchOutcome`时验证每项状态和原始失败信息。
- 代码表、K线“取全”遇短页继续，遇空页停止；无进展/重复页和总页数上限有明确结果。分页数据变化的去重策略要按业务约定，不统一按code去重。
- 历史/当日成交的混合记录、正式撮合、盘后真实成交和集合竞价series分别验证，不能把响应记录数宣传成实际逐笔成交数。
- 原始K线单位、价格精度注册、债券/ETF与A股差异、固定时区、复权仿射系数和锚定日期全部有非空样本。
- `resources.read`单块与`download_file/read_stats`组合分别测试；下载上限下推到读取循环，ZIP/DAT限制在解压前生效。默认HQ统计资源应在无`offline` feature下跑真实ZIP非空样本，避免错误地把其必需依赖放到可选功能后面。

### 4.4 主数据、底层错误和可选补充

每个组合服务建立来源清单，标明主数据、必需依赖和明确允许降级的optional enrichment。底层传输/帧/解析器始终报告真实错误；主数据或必需来源失败不能转换为空集合、零价格或成功结果。只有组合层明确标为可选的补充来源，在策略允许且操作仍有效时，才能返回已取得主数据并附来源状态和诊断。测试同时验证诊断含失败来源、错误类别及缺失字段，不能通过默认零值掩盖未取得数据。

至少覆盖主数据成功/失败与补充成功/失败的四种组合；未声明可选的来源必须严格失败。对于`RustPublicExecutionProfile`，调用者取消、共享session关闭或整次操作deadline到期时，整次操作终止，不能以“可选补充失败”为理由继续分页、重试或返回成功。用受控时钟/回环barrier在补充发起前、响应中和返回前注入取消或超时，断言没有后续请求、只交付一个终态、已启动工作退出。对于`PythonLegacyExecutionProfile`，逐请求超时、startup/预取计时和可选补充降级按旧契约断言，不施加新Rust总窗；显式中断/关闭仍按旧异常及清理语义处理。

Python兼容adapter保留既有降级条件和返回模型；新增来源诊断通过兼容诊断通道提供。同一语料分别断言Rust显式状态与Python既有投影，防止迁移导致静默放宽主数据错误。

## 5. EX、MAC 与 MAC-EX 的新增fixture清单

下列清单是移植最低要求，不是现有Rust覆盖声明。

### 5.1 会话与帧

| 场景 | 必须验证的结果 | 对照资产 |
| --- | --- | --- |
| EX 7727固定92字节setup | 精确发送；16字节头及整个压缩体读尽，后续帧不受残留污染 | `ex/transport/sync.py`、`test_ex.py` |
| MAC-EX | 不发EX setup；每条新连接执行`0x2454`登录，失败不发布可用session | `ex/mac_client.py`、`test_ex_mac.py` |
| MAC7709三步setup | 顺序/字节正确；哪些无响应可容忍需显式兼容规则，不无限等待 | `mac/transport.py`、`test_mac.py` |
| 16字节`<IIIHH>`响应 | 精确长度、压缩上限、trailing data、截断、非zlib、声明长度不符 | EX/MAC codec及现有对应测试 |
| 请求首字节 | MAC-EX仅改符合条件的`0x1c`；`0x1218`的head_flag=1/2语义保留 | EX transport、MAC commands |
| 序号循环 | EX范围K线1字节序号1..255回绕，至少600请求；并发请求不串响应 | EX range command及测试 |
| 响应关联 | 错seq、错method、迟到旧代、合法push分别处理 | 新增录包与注入；现有Python未完整关联校验 |

响应magic/seq/method在现有Python中部分仅解析、不验证。Rust增加校验前，先确认每个profile合法回包规则；不能直接把HQ全部校验搬过去。兼容例外应限定到命令/profile并保留原始头，不能全局关闭校验。

### 5.2 业务值、未知字段和路由

- EX全部命令至少一条非空记录、空响应、截断记录；GBK中文名称、损坏GBK、UTF-8请求代码、9字节边界、未知市场id。
- EX多数模型原有 `_raw`；quote-list的10字节身份、290字节槽位及未解释尾部单独保存。Rust新增raw不能只覆盖当前解析的前140字节。
- EX常规bar同一4字节分别读u32 position和f32 amount、范围bar的amount/settlement规则要独立fixture；不能通过统一Bar投影丢语义。
- MAC `0x122B`20字节bitmap、`0x122C`16字节bitmap+4字节控制各建样本；合法未知bit仍占4字节，151..159保留bit不占槽。包含多个未知bit夹在已知bit之间的记录，验证后续字段对齐及raw保存。
- MAC字段别名按股票/板块上下文显示；相同bit不能通过单一全局label误标。覆盖f32/u32/i32和NaN raw。
- MAC `0x122E`四flag字节、36字节bar结构、当日秒数和秒级周期；MAC `0x122F`保留秒但保持聚合成交语义。
- HK股票市场27/31/48/49/71/98的逐笔路由EX当前/历史命令，衍生品23..26不走该路径；价格/1000、trade_count/nature映射及1800分页有fixture。
- MAC-EX goods目录首选`0x2562`，空结果再用EX count/区间二分；覆盖不存在市场、区间边界、目录变化及缓存失效。宏观市场38覆盖完整code、唯一裸code、歧义、缺失、大小写、空批量及明确刷新。
- 板块金额/净流入/成交量topN必须在全量聚合后排序；相等值稳定顺序、缺失值和异常值要有约定。不能只用第一页已排序样本。
- 700页K线、80页行情、1800成交等真实限制由命令规定；覆盖count恰好等于页限、页限+1、短页、重复满页和最后空页。
- 本地前复权覆盖上市初期、ST/板块、非交易日事件、无效因子、QFQ服务器异常再请求NONE的fallback；没有有效事件时的结果来源可追踪。

### 5.3 探测、切换与缓存

真回环服务器至少提供：接受TCP但不回setup、setup成功但业务探测失败、第一台慢第二台健康、读体途中断开、恢复后重连等组合。EX探测使用markets业务，MAC探测使用含交易日的server info；TCP建立不等于主机健康。

验证主机配置优先级、无效配置错误、失败排除/重置和重试次数；所有探测子任务在结束/关闭后退出。重试是否换下一台还是重试当前台按各协议profile写清。Rust公开执行的自动重试受一个operation总deadline约束，不能每次切换重置完整预算；Python兼容执行保持旧逐请求期限及独立重试计数，不能把它们合并或改成Rust全局attempts。

当前Python的目录/宏观缓存没有TTL和统一失效；Rust若增加cache策略应提供显式refresh和版本化行为测试。缓存命中、过期、刷新失败、并发刷新去重、session切换后适用性分别验证。不要把F10服务端HitCache算入这个本地cache。

## 6. F10 HTTP、ResultSets 与财报资源验证

### 6.1 HTTP层

F10单独使用本机HTTP/HTTPS假服务器，核对普通static、hot、page2三类路由，以及mg/hk/sb/bond/hy/qhf9前缀。入口正确而路由错误也应失败。测试compact UTF-8请求、BOM响应、错误JSON、非JSON错误页、HTTP状态错误、超长/截断响应、慢headers/慢body和取消。

连接策略应覆盖IPv4优先、IPv6 fallback、DNS失败、HTTP代理、HTTPS CONNECT、Host与SNI、证书验证。采用临时测试CA让HTTPS正常路径成功，再用证书不匹配/不受信证书断言失败；不能为方便测试关闭TLS验证。当前 `_http.py`不全局patch DNS，Rust实现同样不修改进程全局解析状态。

Rust公开客户端的operation deadline应包括DNS、建连、代理、TLS、响应与解析阶段；若底层HTTP库无法取消同步DNS，应明确该限制及选择可满足deadline的实现，不把单个socket timeout写成端到端保障。Python兼容路径按原HTTP请求期限起点和范围验收，不借共享实现强加Rust整次组合调用期限。重定向、响应大小、重试次数按相应执行profile明确验证，不能从Python“目前无自动重试”推导无限重试。

网络采样仅在独立、显式启用job运行；单测、doctest和默认consumer不联系公网。不把因无外网跳过的采样标记为“线上兼容通过”。

### 6.2 动态表与字段语义

| 必补语料 | 断言 |
| --- | --- |
| 多ResultSets，包含空表与缺表 | 表顺序、key、metadata和raw完整；新闻提取第二表 |
| ColName/ColDes、缺少列名、同名列三次 | 原始列序、位置、重复名；字典投影`__2/__3`不覆盖 |
| 行短于/长于列、标量行、null与缺失 | lossless cells及投影策略可区分，不悄悄丢尾列 |
| 嵌套array/object、bool、整数、大整数、小数 | `serde_json::Value`能力边界明确；避免全部转String或f64 |
| ErrorCode非零，HTTP仍200 | 保留业务状态，按公开结果约定返回；区别传输失败 |
| 相同T字段跨entry、scope与nhytype | 标签按上下文；银行现金流T088保留动态说明，不伪造全局含义 |
| mg/hk scope在不同Params位置 | 标签及路由依据真实参数位置，未知scope不猜标签 |
| 先查报告期/主题，再查内容 | 两步依赖、空元信息、第二步失败；Rust总deadline与Python原逐请求期限分别验证 |
| 板块分红同code多行、同日排序不稳定 | 按记录身份去重，不能按股票code抹掉分红记录 |
| stock_stage_gain的zq_num | 精确请求值和非空时间序列，避免只有t0统计仍误判成功 |

兼容测试应明确保留目前宽松行为，如`BRK.B`取末段、六位HK数字、部分无效日期原样透传、非正days转字符串；如果新公开API选择严格校验，Python兼容adapter保持旧契约，并把不同点加入迁移说明。严谨输入类型不意味着可以未经说明改变所有旧入口。

### 6.3 资源与offline

有界下载的公开返回为`DownloadResult { data, completion, next_offset }`。`completion`区分已由协议证实EOF的`Complete`与达到用户上限、尚未证实EOF的`LimitReached`；不能把“恰好收到max_bytes字节”当作完整下载。`next_offset`必须对应下次实际读取起点，在部分块被截取时也不得跳过剩余数据。严格完整下载入口`download_complete`在限额内不能证明完整时返回`LimitExceeded`，不返回截断内容冒充完整文件。

用下表分别验证Rust明确状态和Python兼容投影：

| 场景 | Rust有界下载 | 严格完整下载 | Python既有`max_bytes`投影 |
| --- | --- | --- | --- |
| EOF先于限额 | `Complete`及准确data/next_offset | 完整bytes | 保留原bytes行为 |
| 限额处有可信EOF证据 | `Complete`，记录该证据 | 完整bytes | 保留原bytes行为 |
| 恰达限额但无EOF证据 | `LimitReached`、准确next_offset | `LimitExceeded` | 仍按原语义返回截取bytes |
| 单个响应块跨过限额 | 返回上限内data、`LimitReached`，续读不跳字节 | `LimitExceeded` | 原截取语义不变 |
| 空EOF、offset非零、限额0或非法限额 | 按公开输入/EOF约定逐项断言 | 对应明确结果或参数错误 | 与既有参数契约逐项对照 |
| 传输中断、坏帧/坏块、主数据解析错误 | 原错误，不伪装成`LimitReached`或`Complete` | 原错误 | 保留既有异常映射 |
| 调用取消或Rust总deadline到期 | 取消/超时终态，停止续块 | 同左 | 按旧中断语义及每次wire期限判断，不能把Rust总窗耗尽转成新增Python超时 |

财报ZIP解压前核对压缩上限、条目数、DAT大小、总解压量和重名；下载过程按max_bytes流式终止并检查完整性。需要完整ZIP的财报解析不能把`LimitReached`的数据继续按正常完整归档处理。新增损坏central directory、错误CRC、多个DAT、非DAT、索引越界/重叠、report_size非4倍数及恶意压缩比fixture。header日期与文件名日期冲突要选择并记录规则。

offline需验证大小端、record长度、截断尾部、非法日期、价格/成交量单位、GBK文件内容及Windows路径；纯bytes解码和文件IO分别测试。不能只用“写入函数生成，再读回”验证读写，因为相同偏差可能互相抵消。至少使用独立手工bytes和已知客户端产生的脱敏文件。

## 7. 真正的Rust回环、关闭与取消

在 `crates/eltdx/tests/`实现独立集成测试，假服务器绑定`127.0.0.1:0`并返回已知fixtures；必要时单测IPv6 `::1`。测试内保存实际请求以便核验，使用barrier/channel协调阶段，不用长sleep制造“看似并发”。下面列出的测试名是待实现建议名，不是目前已有的target。先在相应开发任务创建并登记target，再执行命令；不存在target时应报告“待创建”，不能称测试通过或以零测试替代。

| 建议target | 负责创建/补全的任务 | 适用范围 |
| --- | --- | --- |
| `lifecycle` | T05–T08，T15补订阅生命周期 | pin/pending交接、owner、deadline、取消和关闭 |
| `hq_loopback` | T11–T18；T05–T10提供运行时基础 | 公开HQ服务的非空业务与故障回环 |
| `f10_http` | T19–T20；T22补财报资源边界 | HTTP/TLS路由、动态表与请求预算 |
| `extended_loopback` | T24–T27 | EX/MAC/MAC-EX握手、命令、路由和切换 |
| `stress` | 各实现任务提供场景，T30汇总创建独立runner | 发布候选资源、吞吐、并发与关闭压力 |

T28–T30负责将这些target接入独立consumer、feature/平台与发布证据门禁。测试target命名可在实施时调整，但需要同步本文件和CI命令，不能遗留不可运行的文档命令。

```sh
cargo +1.89.0 test -p eltdx --test hq_loopback --locked
cargo +1.89.0 test -p eltdx --test lifecycle --locked
cargo +1.89.0 test -p eltdx --no-default-features --features ex,mac --test extended_loopback --locked
cargo +1.89.0 test -p eltdx --no-default-features --features f10 --test f10_http --locked
```

预期应包含一次公开服务句柄发起的非空正常业务响应，不只是直接调用内部Engine或创建builder。至少注入：

- 每字节拆包、多个帧粘包、只回header/部分body、慢读慢写、对端半关闭/复位、坏长度、坏zlib和超过buffer预算。
- submit与close竞态、connect与close竞态、多个close、取消排队请求、取消已写请求、响应与取消同时到达、deadline在排队/握手/重试阶段到期。
- 已超时请求的迟到响应、新连接的相同request id、host切换后的旧push，均不污染新generation。
- `TdxClient` clone与owned service handle共享session；任何clone调用`close`后其他入口返回稳定关闭错误。builder纯校验不产生DNS、socket、线程。
- 关闭预算按入口验证：正常Rust多个繁忙池共用一个绝对shutdown deadline；Python兼容路径保留主transport、资金流、竞价的旧顺序、原阶段期限及异常投影，不能因总1秒预算新增超时。两者共用清理状态机，各自保留可复验行为。
- `close(&self)`幂等并终结共享session，不能隐式重开。Python兼容层需要重开时创建新session generation；旧PinnedSession/Subscription仍失效。
- `PinnedSession`/`Subscription`不Clone：compile-fail验证；Rust第一版订阅采用pull接口，验证pin/订阅drop、显式close、并发poll与close、慢consumer和队列满时不死锁、不泄漏。
- Rust公开操作的总deadline不因分页、host切换、F10两步调用重复增长；剩余时间传入每个子操作。Python兼容执行独立验证每请求期限、startup/预取范围及retry/page/missing计数，包含`max_pages=None`，不能套用此Rust不增长断言。

对每个测试记录请求状态、session generation、槽位/连接数、存活worker、原始/解码缓存峰值。收到预期错误后还要断言线程结束、服务器连接关闭、队列清空；仅断言异常抛出不足以证明清理完成。

Python adapter另跑已有native/Python契约回归，包括异常、partial/missing、序列化、close/reopen、push以及2.1节的执行profile用例。现有Python回调、以及将来可选callback adapter的“回调内关闭”和“回调抛错”用例属于适配器验证，不能算作Rust第一版pull订阅API的承诺。此job可以安装maturin；它通过隐藏绑定桥选择`PythonLegacyExecutionProfile`，复用同一个 `eltdx`业务算法，不能复制另一套Python业务算法，也不能把策略区别误判成算法不一致。

## 8. 公开API文档必须可执行

每个公开服务至少提供一个能编译的普通使用例和一个本机回环行为例。无网络示例可直接doctest；演示真实行情请求的文档使用`no_run`保证编译，并提供同API的回环测试真正执行。不要把所有示例标`ignore`，也不要用字符串搜索“包含函数名”代替调用编译。

全源码门槛后待执行的验收命令（当前均未运行）：

```sh
cargo +1.89.0 test -p eltdx --doc --no-default-features --locked
cargo +1.89.0 test -p eltdx --doc --locked
cargo +1.89.0 test -p eltdx --doc --no-default-features --features full --locked
cargo +1.89.0 doc -p eltdx --no-deps --no-default-features --locked
cargo +1.89.0 doc -p eltdx --no-deps --locked
cargo +1.89.0 doc -p eltdx --no-deps --no-default-features --features full --locked
```

`rust_sdk_matrix.ps1`的platform分支为上述最小/default/full命令设置`RUSTDOCFLAGS=-D warnings`、记录实际flag并在失败或成功后恢复原环境；`rust-sdk.yml`另有独立手动documentation job复用metadata/mirror、MkDocs strict及生成HTML本地链接/fragment检查。源码入口已写，不表示CI已执行。文档中的feature标签、单位、错误、顺序、missing语义、分页停止条件、缓存、同步/异步、线程安全、取消与close语义必须与行为测试关联。compile-fail示例验证私有 `OperationContext`不可访问、受控advanced边界、不Clone的资源句柄及关闭后使用的运行期结果；不把内部Engine所有方法自动重导出为长期公共API。

`serde`测试应覆盖模型往返、enum未知值、bytes格式、日期、finite/non-finite浮点的公开策略。lossless wire数据和JSON可表达能力不同；不能承诺普通JSON完整保存任意NaN payload。F10大整数策略和serde feature结果格式要单独记录。

## 9. workspace外consumer门禁

### 9.1 Path consumer：最小、默认和full

在临时目录创建consumer，目录不能位于本仓库workspace树内，设独立target目录。consumer只依赖 `eltdx`，不能显式把 `eltdx-protocol`、`eltdx-runtime`、`eltdx-python`补进Cargo.toml才能编译。以下Linux CI Bash脚本是**待实现后运行的建议**：

```bash
set -euo pipefail
sdk_root=$(pwd -P)
consumer_root=$(mktemp -d)
export CARGO_TARGET_DIR="$consumer_root/target"
mkdir -p "$consumer_root/src"
cat > "$consumer_root/Cargo.toml" <<EOF
[package]
name = "eltdx-independent-consumer"
version = "0.0.0"
edition = "2021"
rust-version = "1.89"

[workspace]

[features]
default = ["sdk-default"]
sdk-default = ["eltdx/default"]
sdk-full = ["eltdx/full"]

[dependencies]
eltdx = { path = "$sdk_root/crates/eltdx", default-features = false }
EOF
cat > "$consumer_root/src/main.rs" <<'EOF'
use eltdx::TdxClient;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = TdxClient::builder().build()?;
    let quotes = client.quotes();
    let klines = client.klines();
    let trades = client.trades();
    let calendar = client.calendar();
    let resources = client.resources();
    drop((quotes, klines, trades, calendar, resources));
    client.close()?;
    client.close()?;
    Ok(())
}
EOF
cargo +1.89.0 generate-lockfile --manifest-path "$consumer_root/Cargo.toml"
cargo +1.89.0 run --manifest-path "$consumer_root/Cargo.toml" --no-default-features --locked
cargo +1.89.0 run --manifest-path "$consumer_root/Cargo.toml" --locked
cargo +1.89.0 run --manifest-path "$consumer_root/Cargo.toml" --no-default-features --features sdk-full --locked
```

这里的`eltdx/default`对应已定义但为空的default feature。第一条run是最小，第二条显式跟随SDK default，第三条是full。第一次`generate-lockfile`用于consumer自行解析依赖，不复用库workspace的lock；随后`--locked`才有意义。依赖解析可能访问Cargo注册表，与行情测试禁止公网采样是两件事。

以上程序按拟定API实际调用builder、五个服务句柄及close，实施后应能编译并运行；本轮没有编译它。约定这些调用不做DNS、建socket或创建worker，CI还应在无外网环境运行它验证这一点。该smoke不测试行情业务；另建consumer回环case，通过公开接口发出至少一个真正请求并核验结果和关闭。

Windows/macOS运行相同Rust程序，使用各平台原生临时目录/路径生成manifest，不把反斜线直接写入未转义的TOML字符串。保存consumer manifest、生成lock及正常/构建依赖图，使用前述规则拒绝Python相关生产依赖。CI不安装Python、pip或maturin，不用`PYO3_NO_PYTHON`掩盖误引入绑定的问题。

### 9.2 打包后的consumer

T29 源码已提供 `scripts/verification/rust_sdk_packages.py` 的 archives/registry-observation/registry-dry-run 三种入口；独立 SDK 候选版本为 0.1.0，下层及 Python 仍为 3.3.5。版本碰撞、精确依赖、原许可证与打包/注册表证据的区别见 [发布政策](release-policy.md)。全部入口尚未运行，下面模板仍为最终验收要求。

path consumer仍可能借到workspace文件；因此还需独立验证`.crate`中存在实际发布所需源码、许可证、readme及被`include_bytes!`引用的内容。测试专用大fixture可以不发布，但公开doctest/example依赖的文件必须存在。

先对拟发布链条列包内容；这些命令不发布：

```sh
cargo +1.89.0 package -p eltdx-protocol --list --locked
cargo +1.89.0 package -p eltdx-runtime --list --locked
cargo +1.89.0 package -p eltdx --list --locked
```

检查manifest中每条同仓库生产依赖均同时带`path`和可发布的`version`。当前path-only依赖在实施时需要修正。SDK首版按独立`0.x`版本发布；`eltdx-protocol`、`eltdx-runtime`沿用各自当前`3.x`或经批准调整的发布版本，不能默认三包同版。先从候选manifest读取并确认三个版本：

```bash
sdk_version="${SDK_VERSION:?set the eltdx candidate version from its manifest}"
protocol_version="${PROTOCOL_VERSION:?set the eltdx-protocol candidate version from its manifest}"
runtime_version="${RUNTIME_VERSION:?set the eltdx-runtime candidate version from its manifest}"
```

SDK manifest的protocol/runtime依赖要求应分别匹配实际准备消费的版本，runtime的protocol依赖亦同；按版本策略决定精确固定还是兼容区间，并验证目标版本满足该要求。版本变化只更新需要发布的包和受影响依赖，不强制三包一致升级。Cargo发布后的依赖解析使用注册表版本，不会携带开发机path。workspace锁定成功不能证明注册表有正确下层版本。

两种发布前验证路径应分开记录：

1. **下层候选已在目标注册表可解析**：执行正常`cargo package`验证及`cargo publish --dry-run`；允许验证下载已发布依赖。它不上传包。
2. **所需候选依赖尚未发布**：在本地生成各自版本的归档、解包到仓库外；用临时local registry或consumer的`[patch.crates-io]`把各候选指向对应解包目录，验证完整内容和代码。补丁仅用于预发布模拟；仍需发布后无patch的注册表consumer，不能把模拟称为已完成注册表验证。

路径2为取得`.crate`可使用`cargo package --no-verify`，但**该命令本身不是通过证据**。如果Cargo仍因下层版本不可解析而失败，先建立临时local registry/依赖来源；不要删掉version、把原workspace路径伪装成发布内容或跳过最终验证。测试报告需明确`--no-verify`和patch的存在。

在候选依赖可解析后，建议命令：

```sh
cargo +1.89.0 package -p eltdx-protocol --locked
cargo +1.89.0 package -p eltdx-runtime --locked
cargo +1.89.0 package -p eltdx --locked
cargo +1.89.0 publish -p eltdx-protocol --dry-run --locked
cargo +1.89.0 publish -p eltdx-runtime --dry-run --locked
cargo +1.89.0 publish -p eltdx --dry-run --locked
```

预期每个包的构建验证成功；consumer使用解包后的 `eltdx`manifest，重新生成lock，并运行最小/默认/full。不能复用开发目录的`target`来代替解包目录的构建结果。对下层使用patch时，在报告中列出三个解包路径、版本和归档SHA-256。

以下给出解包consumer的具体命令。前提是三个候选归档已经生成，分别设置`SDK_VERSION`、`PROTOCOL_VERSION`、`RUNTIME_VERSION`为各归档内真实版本，且`CARGO_TARGET_DIR`指向生成归档的target目录。此命令中的patch验证解包候选，故不算注册表消费：

```bash
set -euo pipefail
sdk_version="${SDK_VERSION:?set the eltdx candidate version}"
protocol_version="${PROTOCOL_VERSION:?set the eltdx-protocol candidate version}"
runtime_version="${RUNTIME_VERSION:?set the eltdx-runtime candidate version}"
package_target="${CARGO_TARGET_DIR:-$PWD/target}"
dist_root=$(mktemp -d)
for package in "eltdx-protocol:$protocol_version" "eltdx-runtime:$runtime_version" "eltdx:$sdk_version"; do
  crate="${package%%:*}"
  version="${package#*:}"
  archive="$package_target/package/$crate-$version.crate"
  sha256sum "$archive"
  tar -xzf "$archive" -C "$dist_root"
done
mkdir -p "$dist_root/consumer/src"
cat > "$dist_root/consumer/Cargo.toml" <<EOF
[package]
name = "eltdx-packaged-consumer"
version = "0.0.0"
edition = "2021"
rust-version = "1.89"

[workspace]

[features]
default = ["sdk-default"]
sdk-default = ["eltdx/default"]
sdk-full = ["eltdx/full"]

[dependencies]
eltdx = { path = "$dist_root/eltdx-$sdk_version", default-features = false }

[patch.crates-io]
eltdx-protocol = { path = "$dist_root/eltdx-protocol-$protocol_version" }
eltdx-runtime = { path = "$dist_root/eltdx-runtime-$runtime_version" }
EOF
cat > "$dist_root/consumer/src/main.rs" <<'EOF'
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = eltdx::TdxClient::builder().build()?;
    drop(client.quotes());
    client.close()?;
    Ok(())
}
EOF
export CARGO_TARGET_DIR="$dist_root/target"
cargo +1.89.0 generate-lockfile --manifest-path "$dist_root/consumer/Cargo.toml"
cargo +1.89.0 run --manifest-path "$dist_root/consumer/Cargo.toml" --no-default-features --locked
cargo +1.89.0 run --manifest-path "$dist_root/consumer/Cargo.toml" --locked
cargo +1.89.0 run --manifest-path "$dist_root/consumer/Cargo.toml" --no-default-features --features sdk-full --locked
```

预期三个归档校验值可记录，consumer仅从解包目录取得同仓库crate并成功构建运行。验证生成的lock与`cargo tree`确实引用三个对应版本且满足各自manifest依赖；如果补丁没有被选中而仍用了已有注册表版本，这次验证无效。最终注册表consumer必须移除这一整段patch。

### 9.3 注册表发布顺序

正式发布属于独立发布动作，下列命令只是手册，不由本文自动执行。优先使用临时registry演练；正式目标存在时，按依赖拓扑发布各自manifest声明的候选版本，并在每步等待版本可解析。若某个下层版本已发布且本次无需变更，则验证其版本及依赖可解析并跳过重发：

```sh
cargo +1.89.0 publish -p eltdx-protocol --locked
# 确认目标注册表可解析此候选版本后再继续。
cargo +1.89.0 publish -p eltdx-runtime --locked
# 确认runtime及其protocol依赖可解析后再继续。
cargo +1.89.0 publish -p eltdx --locked
```

若采用合并内部crate、私有registry或其他版本策略，必须调整拓扑与consumer依赖来源。不得无依据重发已经发布的同名同版本。`eltdx-python`继续走既有wheel/sdist流程，不是Rust consumer依赖，也不应作为Rust三包发布前置。

发布后创建第二个全新consumer，移除所有path、patch、replace，依赖固定的SDK候选`0.x`版本。下面Bash命令生成实际依赖行，`SDK_VERSION`必须来自发布后的SDK manifest，不能取protocol/runtime版本：

```bash
sdk_version="${SDK_VERSION:?set the published eltdx version}"
printf 'eltdx = { version = "=%s", default-features = false }\n' "$sdk_version"
```

在该目录执行 `cargo +1.89.0 generate-lockfile`、`cargo +1.89.0 run --no-default-features --locked`、`cargo +1.89.0 run --no-default-features --features eltdx/full --locked`，并核对lock中的registry来源。默认配置另用`default-features=true`生成独立lock并运行。所有结果绑定实际发布的版本；不能用发布前path consumer结果冒充。

## 10. 独立Rust CI与Python兼容CI

新增独立SDK工作流或独立jobs，默认不安装Python/maturin。建议分工：

| job | 执行内容 | 失败含义 |
| --- | --- | --- |
| `sdk-msrv` | 显式三个Rust包；1.89.0最小/默认/full及关键单feature | 最低支持版本或feature契约破坏 |
| `sdk-feature-matrix` | 32子集、full别名、生产依赖图及feature禁用consumer | 可选依赖泄漏、cfg交集错误 |
| `sdk-unit-replay` | Rust golden、业务模型、属性测试 | 字节/业务语义偏差 |
| `sdk-loopback` | Linux/Windows真实Rust调用；HQ/EX/MAC/F10、关闭取消 | 会话或端到端边界失败 |
| `sdk-doc-consumer` | doctest、rustdoc警告、workspace外path消费、包内容 | 公开API或交付物不可消费 |
| `sdk-stress` | Linux/Windows独立Rust压力和证据聚合 | 资源界限/生命周期回归或证据缺失 |
| `python-compatibility` | 安装maturin，调用同业务crate，跑既有契约/差分/服务适配器 | Python兼容面变化；不代替独立SDK验收 |
| `python-wheel-consumer` | 干净Python运行环境，无Rust/maturin；只装构建wheel并执行真实回环查询及close | 预编译wheel仍暗依赖编译工具、源码目录或无法实际工作 |
| `sdk-registry-consumer` | 发布演练/发布后不带patch消费 | 实际发布链条错误 |

Rust主jobs用显式`-p`而非`--workspace`，feature矩阵不因Python构建成功而跳过。现有CI含Python绑定、21命令旧标题和Python压力runner，实施时应保留兼容job并增加原生job，不能只改job名称就宣称去除了Python依赖。

### 10.1 无编译工具的Python wheel消费硬门禁

`python-wheel-consumer`与“无Python的Rust consumer”对称，必须和构建wheel的job隔离。运行镜像/机器只提供受支持Python、pip和操作系统运行库；没有`rustc`、`cargo`、`maturin`。仅创建venv不足以隔离PATH中的Rust，不能在同一个已安装Rust的runner上建venv后宣称满足条件。Linux使用干净运行镜像；Windows/macOS使用等价的运行环境并核对PATH/安装包清单。

T18首次引入独立的`installed_wheel_loopback.py`检查脚本，后续T23/T27补迁移域，T28/T29接入consumer矩阵和发行平台。该脚本及必要fixture作为CI检查附件复制到源码树外，仅依赖Python标准库和已安装wheel；不导入仓库tests/helpers、`src`或已构建的工作区`.so/.pyd`。脚本需要启动本机假服务器，使用已安装的公开`TdxClient`执行至少一次正常非空行情查询，核对字段、原始值/单位和请求，再验证`close`释放连接、重复close及兼容重开语义。不能只做`import eltdx`或mock掉客户端/传输。

以下是Linux干净运行镜像内的建议命令。`WHEELHOUSE`只含候选版本对应的构建wheel及确需的运行时wheel，`CHECKS_DIR`含待创建的独立脚本与fixture；均以只读附件提供。先创建这些交付物再运行，当前文档不表示脚本已存在或已通过：

```bash
set -euo pipefail
wheelhouse="${WHEELHOUSE:?set the built-wheel artifact directory}"
checks_dir="${CHECKS_DIR:?set the isolated wheel-check artifact directory}"
python_version="${PYTHON_PACKAGE_VERSION:?set the wheel distribution version}"
python -I - <<'PY'
import importlib.metadata
import shutil
for tool in ("rustc", "cargo", "maturin"):
    assert shutil.which(tool) is None, f"unexpected build tool: {tool}"
installed = {dist.metadata["Name"].lower() for dist in importlib.metadata.distributions()}
assert "maturin" not in installed
PY
run_root=$(mktemp -d)
cd "$run_root"
python -I -m pip install --force-reinstall --no-index --find-links "$wheelhouse" --only-binary=:all: "eltdx==$python_version"
python -I "$checks_dir/installed_wheel_loopback.py" --fixtures "$checks_dir/fixtures"
python -I - <<'PY'
import importlib.metadata
import shutil
for tool in ("rustc", "cargo", "maturin"):
    assert shutil.which(tool) is None, f"unexpected build tool after install: {tool}"
assert "maturin" not in {dist.metadata["Name"].lower() for dist in importlib.metadata.distributions()}
PY
```

`--only-binary=:all:`与`--no-index`共同禁止无对应wheel时回退源码构建或意外从其他版本下载。安装失败应让job失败，不安装Rust重试。`python -I`屏蔽PYTHONPATH和用户site；再从源码树外执行，检查导入路径确实位于运行环境site-packages。该版本是Python发行包版本，不必等于SDK的`0.x`或两个底层Rust crate版本。

保存wheel文件SHA-256、wheel标签、Python/OS/架构、安装日志、实际import路径、回环请求/结果、关闭后连接/线程计数和工具缺席检查。对正式支持的发行平台执行，不能用Linux通过代替Windows wheel消费。构建job可使用maturin；本job不允许，二者报告分别保存。

## 11. 压力、资源与性能证据

基础正确性通过后再运行压力。各任务先实现对应场景，T30创建并汇总独立Rust测试/runner，例如 `crates/eltdx/tests/stress.rs`中的ignored用例；确认`stress` target已登记存在后才执行以下建议命令，不能直接对当前未实现target宣称运行通过：

```sh
cargo +1.89.0 test -p eltdx --test stress --no-default-features --features full --release --locked -- --ignored --test-threads=1
```

每个case记录实际次数、并发度、队列/连接配置、随机seed、payload大小和运行耗时。最低覆盖：

- 至少10万次请求的唯一完成/取消身份、每请求最多一个终态，响应不跨session generation串线。
- 有界排队、慢服务器、满推送队列、消费者暂停、持续错误帧、断线后重试；raw/decoded缓存峰值不超过配置预算。
- 创建、连接、关闭和释放循环；Windows与Linux分别设可完成且有意义的次数，记录TIME_WAIT影响，不把端口耗尽当内存泄漏，也不因减少次数隐去覆盖变化。
- 长驻服务句柄、pin、subscription、宏观/目录cache及F10 HTTP池反复创建/释放；线程、FD/句柄、socket、RSS达到稳定范围。
- 分页大结果和财报/ZIP限额：分配前限制、输出大小预算、取消后及时释放；不能仅监控最后RSS。

使用同配置预热后采集基线、中途、结束和冷却后的资源值。RSS不是唯一指标：记录Rust内部budget峰值、OS句柄和worker数；分配器可能不立即归还内存，应区分稳定保留与逐循环增长。

性能只作为测量结果，不预先承诺“更快”。对同fixture、同并发度、同服务器脚本比较迁移前后，分别报告解析吞吐、端到端吞吐、p50/p95/p99、CPU、峰值内存、取消/关闭延迟；有Python包装和纯Rust两条曲线。波动区间、机器与运行模式完整列出，不能拿debug对release或公网随机延迟做结论。

每次Linux/Windows证据包含候选完整SHA、dirty状态、Cargo.lock/语料/脚本SHA-256、OS/CPU、Rust版本、feature集、命令、实际计数、退出码、开始结束UTC、资源数据与日志校验值。聚合器拒绝不同提交、缺平台、缺日志、failed/未运行被标passed。已有 `build_stress_evidence.py`可参考这种提交绑定格式，但不能直接把旧Python压力日志更名为Rust压力结果。

PR一般执行有界的确定性子集；发布候选执行完整压力并归档。调整阈值或缩减Windows循环需在报告解释依据、实际次数与覆盖影响，不能为了绿灯放宽且不记录。

## 12. 发布候选检查表

- [ ] 公开模块、feature含义、MSRV与文档一致；默认核心无Python生产依赖。
- [ ] 最小、默认、五单feature、32子集及full在规定矩阵通过。
- [ ] 每个已声明支持的命令/entry具备非空正常、边界和错误语料；unknown/raw没有被统一投影丢失。
- [ ] EX/MAC/MAC-EX握手、路由、序号、位图、GBK与价格日期规则有独立测试。
- [ ] F10路由、HTTP/TLS、动态ResultSets、上下文标签及资源限制通过。
- [ ] 真Rust回环验证正常业务、故障、deadline、取消、generation、共享句柄及close。
- [ ] doctest实际编译；私有/不可Clone/feature禁用的负面API有compile-fail验证。
- [ ] workspace外path、打包解包及最终注册表consumer按阶段通过，依赖图和归档内容已保存。
- [ ] Python绑定复用业务crate，旧API兼容/有意变化有明细；Pythonjob与Rustjob分别报告。
- [ ] Rust公开有限operation预算与Python旧逐请求策略分别通过；两页各小于8秒而合计大于8秒、startup/预取、独立retry/page/missing计数、`max_pages=None`及可选补充用例已对照旧/新Python。
- [ ] 无Python的Rust consumer与无Rust/maturin的Python wheel consumer均通过；后者使用`--only-binary=:all:`安装候选wheel，并完成真实回环查询和close。
- [ ] 下载完整/达限状态及next_offset正确，严格下载上限失败和Python原截取语义有对照；主数据严格失败、可选补充诊断与取消/超时按各自执行profile验证。
- [ ] Linux和Windows压力证据绑定同一发布候选，资源和性能限制/变化如实记录。
- [ ] 真实服务器兼容性只写已采样的profile/市场/日期；未运行项仍标未运行。
- [ ] 发布顺序、版本、变更说明、许可证、源码包和可复现验收命令已审查。

检查表只有在存在相应运行记录时才能勾选。本文件的初始版本所有运行门禁均待实施和执行。
