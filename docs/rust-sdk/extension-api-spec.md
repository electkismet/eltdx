# F10、EX、MAC、MAC-EX、offline 与 financial 分域 API 规格

状态：拟议规格，尚未实现。本文补全 [public-api.md](public-api.md)、[architecture.md](architecture.md)、[capability-matrix.md](capability-matrix.md) 与 [verification.md](verification.md) 的扩展域契约。所有 Rust 签名均为待实现 API；现有 Python 行为、拟议 Rust 行为与待采集证据分别写明。本文不表示已构建 crate、通过测试或连通线上服务。

源码基线固定为 `af471f1adf38757f6aa0188968db8110eb23f6ad`。下文源码链接均指向这一提交；fixture 是实施阶段必须新增的验收设计，不是本次已经生成的文件。

## 1. 共享契约与覆盖口径

`eltdx::f10::F10Client`、`eltdx::ex::ExClient`、`eltdx::mac::MacClient`、`eltdx::mac::MacExClient` 是独立惰性 owner。它们的 builder、`build()`、服务 getter 不联网、不扫描安装目录、不读取用户配置；`connect()` 或首个业务调用才解析、连接、握手。owned service、Clone、关闭后不可 reopen、显式 close 等待确认，遵循公共 API 的同一所有权规则。不同协议的 endpoint、缓存、会话 generation 与握手状态隔离，不能因为端口或数值市场码相同而共享已认证 socket。

正常 Rust 公共网络方法最后一个参数为 `&OperationOptions`。默认 8 秒覆盖一次完整 operation：启动、DNS（仅具备有界契约的 resolver）、探测、连接、握手、锁等待、全部分页、重试、辅助元数据和解析；各阶段只取剩余时间。总尝试预算及 `PageOptions` 在开始时校验，子调用共享预算，取消/期限错误始终终止，不降级成空数据。网络入口返回根级 `Result<T>`。本地纯函数和文件方法没有网络 options，采用第 6 节的本地选项。

Python 绑定经隐藏的 `__python_compat` 桥接同一算法，保留 `PythonLegacyExecutionProfile`：原来每次 wire 请求独立超时、启动和预取原计时范围、重试计数、缺失恢复计数、分页计数及 `max_pages=None` 含义不变；不得给旧 helper 追加总 8 秒或新的共享尝试上限。Python 原来的 `ValueError`、`TransportError`、`ProtocolError`、`ResourceFormatError`、空列表/`None`、降级与返回类型保持。以下 Rust 更强校验、完整性状态、严格 ZIP 选择均不偷偷施加到旧 Python 路径。

| 域 | 现有公共业务入口覆盖 | 管理、别名与辅助入口 | feature / 依赖 |
| --- | --- | --- | --- |
| F10 | 157 个 `F10Client` public 方法全部列于第 2 节；其中 156 个可能联网，`news_channels` 纯本地 | 4 个模块辅助函数、13 个模型及 news 枚举/常量 | `f10`；内部 JSON 库不强制公共 `serde` |
| EX | 11 个业务方法全部列于第 3 节 | 8 个管理/属性、3 个导出配置函数；底层命令只进入受控 advanced 面 | `ex`，与 HQ 独立 |
| MAC | 23 个业务方法全部列于第 4 节 | 8 个管理/属性、3 个赋值别名、11 个模块辅助函数 | `mac`，隐含 `ex` |
| MAC-EX | 13 个业务方法全部列于第 5 节 | 6 个管理方法；MAC 命令和模型单一实现 | `mac`，隐含 `ex` |
| offline | `offline.__all__` 共 46 项：38 个函数全部列于第 6 节，8 个模型 | ZIP 选取 helper 纳入新受控 archive API | `offline`；包括 EX 本地日线，不要求网络 `ex` |
| financial | 4 个网络/构造函数全部列于第 7 节 | 复用 offline 的列表、DAT、ZIP 解析 | `offline`，网络依赖默认 HQ 资源服务 |

默认 `[]` 的 HQ 必须仍能读取统计资源所需的内部受限 ZIP；这不开放本地文件 API。`offline` 开放本地读写、财务 DAT/ZIP 和财务下载组合；不能把整个 ZIP 解码依赖从默认 HQ 移走。第一版无空 `async` feature，`full` 精确等于 `serde + offline + f10 + ex + mac`。

### 1.1 身份、数值、时间与结果

```rust,ignore
// 各域定义，非 HqMarket 的别名；构造器验证 wire 范围与编码长度。
pub struct ExMarketId(u8);
pub struct MacMarketId(u16);
pub struct ExInstrumentId { pub market: ExMarketId, pub code: ExCode }
pub struct MacInstrumentId { pub market: MacMarketId, pub code: MacCode }
pub struct ExCode(/* GBK/UTF-8 wire 合法、非空、最多 9 字节 */);
pub struct MacCode(/* GBK wire 合法、非空、最多 22 字节 */);
pub struct WireFloat { pub bits: u32 } // as_f32()；不丢 NaN/负零原位
pub struct NativeNumber<T> { pub value: T, pub unit: NativeUnit }
pub enum NativeUnit { Currency(String), Shares, Lots, Contracts, Percent,
    Ratio, IndexPoints, Count, Unknown }
pub enum Evidence { WireDefined, SourceDocumented, Inferred, Unknown }
pub struct FieldMeaning { pub unit: NativeUnit, pub evidence: Evidence }
pub type ExtensionMeta = eltdx::types::ResultMeta;
pub struct PageResult<T> {
    pub rows: Vec<T>, pub server_total: Option<u64>,
    pub next_offset: Option<u32>, pub completion: PageCompletion,
    pub meta: ExtensionMeta,
}
pub enum PageCompletion { EndOfData, RequestedCount, MoreAvailable, Unknown, Incomplete }
```

这里和下文的 `pub` 字段只表达逻辑 schema；除主规格已公开字段的 `DownloadResult` 外，query/DTO实际字段私有、同名只读getter，Vec返回slice，String返回str。构造与 `with_<field>` 在纯本地校验。`ExtensionMeta` 是 [HQ规格第3节](hq-api-spec.md#3-所有领域共用的返回单位与限制) 定义的 `eltdx::types::ResultMeta` 类型别名，不是第二份meta；它复用 `SourceStamp{command,endpoint,session_id,metadata_version,trading_date}`、`Diagnostic{source,stage,status,error_kind,message}`、`EnrichmentStatus` 和 `RawData{payloads,wire_frames,row_ranges}`。HTTP来源没有SocketAddr时endpoint=None并在source诊断保存route名，聚合每页来源分别保留。`TimeOfDay` 为本域值类型 `{hour:u8,minute:u8,second:u8}`（0..23/0..59/0..59）；`EventTime`、`TradingDate`复用主规格。`RecordKey` 为 `Date(TradingDate) | Minute(MinuteKey)`，`MinuteKey{date:TradingDate,time:TimeOfDay}` 且second必须0。

`PageResult` 为本域命名空间类型，不新增根级重导出。`Incomplete` 必须有诊断，只由明确允许部分结果的分页 API 返回；“收集全部”的入口不能借它掩盖上限、坏帧或取消。达到调用方请求的最近 N 条是 `RequestedCount`，不是声称已遍历数据集。`next_offset` 按实际消耗的 wire 行数推进，不能按去重后行数推进。空但格式完整的响应是成功空集；声明行数超出完整记录数、半条行、非法压缩体等在普通 Rust 路径报 `Protocol`，Python 保留各旧 parser 的 break/截断行为。

`Unknown` 表示当前单次响应已完整解码，但协议证据不足以确认还有数据或已经到末尾。EX 普通单页短响应（包括合法空页）没有已核实终止标记时使用此值，不能凭长度改成 `EndOfData` 或 `MoreAvailable`；它也不是损坏或遗漏已声明记录的 `Incomplete`。`next_offset` 若存在，仅为按实际 wire 行数计算的后续查询位置，不证明该位置存在数据。明确达到请求条数时可使用 `RequestedCount`，只有经该命令证据支持的终止/剩余标记才使用另外两种状态。完整收集入口仍必须取得终止证据，不能将 `Unknown` 当完成。

自动分页默认上限（新Rust设计）固定为：F10 index/tracking分别100页×100条=10000记录，board分红50页×100条=5000记录；MAC报价/板块成员2000页×80条=160000记录、bar200页×700条=140000记录、transactions100页×1000条=100000记录；MAC-EX HK all为50页/90000条；EX原11入口为单页或单次range，另需collect时显式 `PageOptions{page_size,max_pages,max_records}`，不得默认为无限。商品fallback扫描默认2000页、每页600、最多1200000目录项，仍受总期限和32次额外尝试约束。调用目标count小于max_records时以目标为准，PageOptions更小值优先。单页方法只收一页不会把保护页数当EOF。Python不新增这些保护值，保持原循环和上限。

`RawPolicy::Omit` 不返回整帧/整 JSON，也不构造逐行 hex；`RetainPayload` 保存响应与行范围，`RetainWire` 额外保存相应wire帧，受同一字节预算约束。未知 MAC 槽位、F10 原列名/位置/值、EX 双义位模式、本地 reserved 位均是业务必需信息，不因 Omit 丢失。价格、量、额不一律套用 A 股 `/100`；每个字段附域内已核实单位，未知标记 `Unknown`。本地 float32 和 wire float32 保留 bits；对外 f64 视图只是无损扩宽有限 f32，非有限数不能默认变零。日期用 `TradingDate`，盘中时间用 `TimeOfDay`，组合时间用 `EventTime`，不擅自加入 UTC 时区；交易日历与夜盘归属未获证据时保留源日期。

### 1.1.1 补充配置与值类型

`Endpoint=SocketAddr`（数字地址）；HTTP的URL独立于Endpoint。`DiscoveryOptions{max_workers:NonZeroUsize=8,probe:ProbeOptions}`，`ProbeOptions{timeout:Duration=5s,mode:ProbeMode=DataReady}`，ProbeMode=TcpOnly/DataReady；`HostProbe{endpoint:Endpoint,latency:Option<Duration>,status:ProbeStatus,error_kind:Option<ErrorKind>}`，`ProbeStatus=Ready|Failed`。`DiagnosticSnapshot{active_endpoint:Option<Endpoint>,candidates:Vec<Endpoint>,probes:Vec<HostProbe>}`。EX使用setup后markets作DataReady探测，MAC/MAC-EX按profile业务探测；仅TCP通不能标数据健康。普通operation仍限制总预算，probe timeout不是每个host额外扩张总期限。EX/MAC builders均有endpoints、timeout=8s、raw=Omit、auto_failover=true；不自动读ELTDX_EX_HOSTS或用户文件，显式discovery才扫描候选。Python EX/MAC-EX历史默认15s、MAC来自MAC_TIMEOUT=15s，各自桥接保留，不受新Rust统一8s改变。

本地 `LocalCode` 为无路径分隔符的六位代码；`Exchange=Shenzhen|Shanghai`；`SecurityType`含SH_A/SH_B/SH_INDEX/SH_FUND/SH_BOND/SZ_A/SZ_B/SZ_INDEX/SZ_FUND/SZ_BOND/Unknown；`CoefficientDecision{coefficients:Coefficients,security_type:SecurityType,evidence:Evidence}`；`IndustryMap`为保持首次key插入顺序、后值覆盖的code→IndustryCodes列表，`IndustryCodes{tdx:String,sw:String}`。`ExPeriod`含0/1/2/3/4/5/6/7/8/9/10/11与显式Wire(u16)，时间转换仅对有明确单位的周期；`QuoteCategory=HongKong|Futures`。`FinancialArchiveName`/`RemoteFileName`不是本地Path，不能直接以路径拼接保存。

### 1.2 管理和错误的统一签名

每种 owner 均提供 `builder() -> Builder`、`connect(&OperationOptions) -> Result<()>`、`close() -> Result<()>`、只读 `config()`、`diagnostics()`。`Builder::build() -> Result<Client>` 纯本地验证。Python `disconnect` 是 close 别名；`ensure_connected` 在 Rust 对应 `connect` 的幂等网络语义。`from_best_host` 对应显式 `Builder::discover(&DiscoveryOptions, &OperationOptions) -> Result<Client>`，`ping_all` 对应 `probe_endpoints(&[Endpoint], &ProbeOptions, &OperationOptions) -> Result<Vec<HostProbe>>`；不是 build 的隐含动作。`host` / `hosts` / `healthy_hosts` / `unhealthy_hosts` 对应 `diagnostics().active_endpoint/candidates/probes` 快照，不自动重探测。EX `refresh_hosts` 对应 `refresh_endpoints(&DiscoveryOptions, &OperationOptions)`。

输入溢出、负数转换、非法 enum、code 编码/长度、日期区间反转为 `InvalidArgument`；网络连接为 `Connection`，本地I/O为 `Io`，坏帧/坏 JSON/坏行布局为 `Protocol`，本地坏格式为 `ResourceFormat`，必要元数据缺失为 `MetadataUnavailable`，确认对象缺失为 `MissingData`，总数/字节/页保护为 `LimitExceeded`。错误带 domain、operation、endpoint、阶段及相关 code/offset，日志不输出任意 body。调用方 `Cancelled`、`Timeout`、`Closed` 不转换成“无数据”。F10 服务端非零业务码按第 2 节作为正常响应状态保留。

## 2. F10：HTTP、多表及完整入口

事实来源：[客户端与路由](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L95)、[HTTP 处理](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/_http.py#L10)、[响应解析](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L2197)、[模型](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/models.py#L12)。

### 2.1 构造、HTTP 与动态数据

`F10ClientBuilder` 设置 `base_url`、`ladder_url`、`international_url`、`headers`、`prefer_ipv4`、响应大小限制和 resolver；无 cookie、重试或本地缓存默认副作用。路由按 Entry：`CWServ.cfg_fx_lbtt` → `http://hot.icfqs.com:7615/TQLEX`；`CWServ.skef10_mg_` / `_hk_` / `_sb_` / `_bond_` / `_hy_` 与 `CWServ.qhf9_` → `http://page2.tdx.com.cn:7615/TQLEX`；其余 → `http://static.tdx.com.cn:7615/TQLEX`。显式覆盖全部保存；Python 自定义 base_url 对 ladder 的历史回退和 international 默认保持旧实现。URL query 仅 `Entry=<urlencoded>`；POST UTF-8 JSON 不 ASCII 转义，默认 Content-Type/Accept application/json；`QSID=tdx` 不能凭常量存在推定每个请求都会发送。

第一版 `F10ClientBuilder` 的所有配置私有，提供同名 `with_*`：三个URL为无凭证的http/https URL（禁止fragment），headers为合法HTTP名值有序列表（禁止CR/LF），`prefer_ipv4:bool=true`，`timeout:Duration=8s`，`max_response_bytes:NonZeroU64=16MiB`（解码后），`max_encoded_bytes:NonZeroU64=16MiB`（压缩输入），`max_header_bytes:NonZeroUsize=64KiB`，`max_redirects:u8=0`，`proxy:Option<ProxyUrl>=None`（不隐读环境代理），`tls:TlsPolicy=SystemRoots`，`resolver:ResolverPolicy=Bounded`。可显式自定义信任根，不提供静默跳过TLS验证；SystemRoots无法加载时报Connection。生产HTTP库的MSRV/TLS/SNI/代理/IPv4/IPv6/取消/三种上限需E03实际验证；限制值为新Rust设计，不是当前Python事实，legacy不新增16MiB或header cap。HTTP解压途中累积计数，超限报LimitExceeded，不截JSON返回成功。

IPv4 优先仅在本请求连接阶段生效，不修改全局 DNS；失败后地址族回退仍保留代理、HTTP CONNECT、Host、SNI 和 TLS 校验。TLS/代理不会因 IP 直连而关掉。HTTP 状态、网络、UTF-8/BOM JSON、顶层不是 object 分别有明确错误。新 Rust 默认不做隐含 HTTP 重试；若显式策略允许，仍共享整个 operation 预算。Python 一次 `_post` 无应用层重试。服务器 `HitCache`/`hitCache` 作为响应字段保留；`cache_list` 是远端 CWSearch 查询，不代表 SDK 本地缓存；body精确为action=get、key=`kind:market_code`（例如gg:1_600000）、bin="1"、qsid="tdx"。

```rust,ignore
pub enum RequestPayload { Body(JsonValue), Params(Vec<JsonValue>) }
pub struct CallQuery { pub entry: EntryName, pub payload: RequestPayload }
pub struct ParamsQuery { pub entry: EntryName, pub params: Vec<JsonValue> }
pub struct Cell { pub name: String, pub index: usize, pub value: JsonValue }
pub struct Row { pub cells: Vec<Cell> }
pub struct ResultSet { pub key: Option<String>, pub columns: Vec<String>,
    pub rows: Vec<Row>, pub raw: Option<JsonValue> }
pub struct Response { pub entry: String, pub request: RequestPayload,
    pub error_code: Option<i64>, pub result_sets: Vec<ResultSet>,
    pub meta: ExtensionMeta }
impl Response { pub fn ok(&self) -> bool; pub fn first_table(&self) -> Option<&ResultSet>; }
impl Row { pub fn get_all(&self, name: &str) -> impl Iterator<Item=&Cell>;
    pub fn python_projection(&self) -> Vec<(String, &JsonValue)>; }
```

`JsonValue` 为 f10 自有公开值类型（Null/Bool/Number/String/Array/Object），不以公共 `serde` feature 决定是否可用；对象也保留输入顺序。F10 权威结构是按表、行、列位置排列的 `Vec`，不能只给 `HashMap<String, Value>`。列名优先 `ColName`，否则 `ColDes`；行比列名长时补 `col_i`；标量行是一个 cell。重复列不覆盖，`python_projection` 才给第 2/3 次同名列后缀 `__2`/`__3`，并复刻旧碰撞行为；保真访问用 `cells[index]` 或 `get_all`。表 key 可重复或缺失，仍以响应位置排序；缺失原key存None，Python兼容视图补table0/table1。`rows()` 兼容视图仅第一表，不代表整响应；新闻提取需要第二表。零表、空表、Null、空字符串、缺 cell 各有区别；缺 cell 不补造 0。

具体值schema为 `JsonValue::{Null,Bool(bool),Number(JsonNumber),String(String),Array(Vec<JsonValue>),Object(Vec<(String,JsonValue)>)}`；`JsonNumber`保留原合法JSON数字lexeme并提供checked i64/u64/f64访问，避免财务大整数先转float丢精度。`OrderedJsonObject=Vec<(String,JsonValue)>`，typed extra拒绝重复键；原raw仍可保真重复键。`EntryName`为非空字符串，`RequestId`为非空十进制字符串，`SectionCode`是带method registry的非空字符串；`F10MarketCode(u16)`不是HqMarket别名。`DateOrWindow=Date(TradingDate)|Window(FundWindow)`，`FundWindow`可显式3M（ETF/margin默认）或非空已声明wire窗口值。`NewsChannelInfo{id:i32,label:String,group:Option<String>}`；Tab/Channel保留wire数值与已知标签。`PageSelector=All|Number(u32)`，只在原page=-1方法暴露All；对应N=-1的page_size是 `PageSizeSelector::All|Count(NonZeroU32)`，不是将-1强转无符号数。

`error_code` 缺失或 0 时 `ok=true`；非 0 仍 `Ok(Response { ok=false, ... })`，不把服务业务码升级成传输异常。typed 组合若缺必要报告期/topic，报 `MetadataUnavailable` 并附原响应状态；旧 Python 原 `ProtocolError` 保留。字段字典必须按 Entry + scope + nhytype/reporttype 解码，不全局将同名 `N001` 解释为同一业务字段；未知字段和银行 `T088` 不猜测。

`Response::attributes() -> &[(String, JsonValue)]` 保留非 `ResultSets` 顶层附加字段及其顺序和重复键，包括 `HitCache`/`hitCache`；`ErrorCode` 仍有checked专用访问。只有原始payload保留模式才另存输入中更早的重复 `ResultSets`，Omit不为这些重复表构造额外副本。请求与表raw的保留复制也检查父操作取消/期限；Params外包装及所有元素共用整文档的复杂度限额。

多请求的累计保护由 `max_operation_bytes:NonZeroU64=64MiB` 配置，并提供 `with_max_operation_bytes` 和只读getter。它计入一次公开操作所有响应的解码字节、按raw策略另存的压缩payload/HTTP明文，以及分页去重键的逻辑字节；不是Rust堆内存测量值。传输在分配相应保留缓冲前扣同一OperationContext预算；不在每页/前置查询重置，也不在元数据合并时对共享Bytes句柄再扣一遍。较小配置允许提前终止整个操作，报LimitExceeded而非成功返回截断列表。该上限仅适用新Rust入口，Python既有策略不新增此保护。

### 2.2 query 字段类型、默认和分页

下表每行定义一个 public query struct；名称为方法名 PascalCase + `Query`。例如 `ShareholdersQuery` 包含表列所有字段，签名是 `F10Client::shareholders(&self, query: &ShareholdersQuery, options: &OperationOptions) -> Result<Response>`。下表的 `method(Q) -> R` 是此完整签名的缩写；无参网络方法省略 query，仍有 `options`；只有 `news_channels()` 无网络 options。`call` 的互斥 Body/Params 用 enum 编译期表达，`params` 是 ParamsQuery 便捷包装。每个 query constructor 提供下面默认值，所有字段有 read-only getter 与 checked builder，禁止通过 public mutable 字段绕过校验。

| 字段类型缩写 | Rust 类型/边界与行为 |
| --- | --- |
| `A` / `US` / `HK` / `SB` / `B` / `HY` / `QH` | `AshareCode` / `UsTicker` / `HongKongCode` / `ThirdBoardCode` / `BondCode` / `IndustryCode` / `FuturesCode`；非空及域内编码/长度验证。A/SB/板块六位数字，保留显式市场；B 六位带可选 SH/SZ，bare 继承旧 SH 默认并给 inferred 诊断；HY/QH 为非空域内 code，不套 HQ 号段。US 保留 ticker 含点，HK 构造标准五位，六位旧兼容见下文 |
| `S` | `String`，UTF-8，缺省空；传 Entry 参数时不自动数值化，显式编码 JSON 字符串 |
| `Section` | method-scoped `SectionCode`，由对应下节 registry 验证；开放式页面子标签保留非空 wire string，未知值可用 `call`，不猜新 enum |
| `D?` | `Option<TradingDate>`，Gregorian 有效日期；None 按现有该方法约定编码空字符串或触发前置请求，不自动变今天 |
| `P` / `N` | `u32` 页号（通常 >=1）/ `NonZeroU32` 页大小；该 wire 字段 <=i32::MAX。`-1` 用 `PageSelector::All`，普通 0 页只有显式支持的 valuation；不把所有页都钳到 100 |
| `Count` | `NonZeroU32`，<=i32::MAX；天数必须 >0，`zq_num` 同此；Python 字符串数和非正数透传行为单独保留 |
| `Limit` | `NonZeroU32` max_pages，合并 `PageOptions` 的更小上限；all_pages=false 时只发一页；不能把 max_pages 作为真实总页数 |
| `J` / `Extra?` | `JsonValue` / `Option<OrderedJsonObject>`；Extra 不能覆盖 typed 请求的 identity、ReqId、分页字段，若要覆盖用 `call`；Python 旧 `payload.update(extra)` 不变 |
| `Tab` / `Channel` | `NewsHomeTab` 八个已知值 / `NewsChannel` 84 个已知值；允许显式 `Unknown(i32)` 透传并记录未知，不能把缺新闻当未知频道成功 |

`allotment_details.date` 是必填TradingDate，表中D?标记仅表示日期家族，不能据此放宽为None；其余源默认None/empty的日期用Option，source必填日期必填。`TradingDate`不校验是否交易所开市日，只校验Gregorian年月日；DateOrWindow只在fund_report出现，其他日期不容许3M字符串。

Python 的 `_us_code` 会去掉点之前部分，`BRK.B` 会变 `B`；新 Rust `UsTicker` 保留 `BRK.B` 并按 endpoint 参数传递，不能继承这个有损标准化。Python HK 补足五位但接受六位；新 Rust 以显式 `HongKongCode::from_wire` 保留六位及 provenance，标准构造五位。Python 一些日期/天数仅转字符串不验证，保持原样；新 typed query 在联网前拒绝无效值。`trading_report.date/page/page_size` 全部ignored，`market_report.date` ignored，`digest.page` ignored且page_size仅interaction使用；下表仍保留其query字段，不赋予虚假服务能力。`AshareCode`也可表示六位基金/指数/板块代码，另存用途，不能靠该名字推定它是A股普通股票。

Wire notation: code/us/hk/bond = normalized query code; bond_market = explicit/inferred bond market; page_text/size_text = decimal strings. Literal strings retain quotes; unconverted numeric variables remain JSON numbers.

#### Generic / A-share / fund (16)

| Python entry / Rust method(Q) -> R | Query fields / defaults | Wire / composition |
| --- | --- | --- |
| [call](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L123)<br>`call(&CallQuery) -> Response` | `entry:EntryName; payload:RequestPayload=Body({})` | `Entry + RequestPayload; Body/Params exclusive` |
| [params](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L135)<br>`params(&ParamsQuery) -> Response` | `entry:EntryName; params:Vec<J>` | `Entry + Params[]` |
| [limit_up_down_list](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L140)<br>`limit_up_down_list(&LimitUpDownListQuery) -> LimitBoardLadder` | `start_date:D?=None; end_date:D?=None; include_summary:bool=false` | `cfg_fx_lbtt [1,start,end]; summary [2,start,end]` |
| [limit_board_ladder](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L179)<br>`limit_board_ladder(&LimitBoardLadderQuery) -> LimitBoardLadder` | `start_date:D?=None; end_date:D?=None; include_summary:bool=false` | `alias limit_up_down_list` |
| [stock_info](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L194)<br>`stock_info(&StockInfoQuery) -> Response` | `code:A` | `tdxf10_gg_comreq: ['gpquery', code]` |
| [business_periods](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L199)<br>`business_periods(&BusinessPeriodsQuery) -> Response` | `code:A` | `tdxf10_gg_comreq: ['zygcfx', code]` |
| [topic_ids](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L204)<br>`topic_ids(&TopicIdsQuery) -> Response` | `code:A` | `tdxf10_gg_comreq: ['rdtcgn', code]` |
| [company_profile](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L209)<br>`company_profile(&CompanyProfileQuery) -> Response` | `code:A; section:Section='8'` | `tdxf10_gg_gsgk: [section, code, '']` |
| [business_composition](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L214)<br>`business_composition(&BusinessCompositionQuery) -> Response` | `code:A; report_date:D?=None` | `business_periods -> business_report(composition)` |
| [shareholders](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L223)<br>`shareholders(&ShareholdersQuery) -> Response` | `code:A; section:Section='count'; report_date:D?=None; kind:S='000'; filter1:S=empty; filter2:S=empty; page:P=1; page_size:N=20` | `section registry` |
| [shareholder_change_plans](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L274)<br>`shareholder_change_plans(&ShareholderChangePlansQuery) -> Response` | `code:A; page:P=1; page_size:N=20; filter1:S=empty; filter2:S=empty` | `shareholders(plan)` |
| [capital_structure](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L294)<br>`capital_structure(&CapitalStructureQuery) -> Response` | `code:A; section:Section='structure'; date:D?=None` | `section registry` |
| [business_report](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L331)<br>`business_report(&BusinessReportQuery) -> Response` | `code:A; section:Section='business'; report_date:D?=None` | `section registry` |
| [industry_analysis](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L366)<br>`industry_analysis(&IndustryAnalysisQuery) -> Response` | `code:A; section:Section='peers'; arg:S=empty` | `section registry` |
| [trading_report](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L409)<br>`trading_report(&TradingReportQuery) -> Response` | `code:A; section:Section='margin'; arg:S=empty; date:D?=None; page:P=1; page_size:N=20` | `section registry` |
| [fund_report](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L459)<br>`fund_report(&FundReportQuery) -> Response` | `code:A; section:Section='basic'; arg:S=empty; date:DateOrWindow?=None; page:P=1; page_size:N=20` | `section registry + report period lookup` |

#### US (17)

| Python entry / Rust method(Q) -> R | Query fields / defaults | Wire / composition |
| --- | --- | --- |
| [mg_name](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L548)<br>`mg_name(&MgNameQuery) -> Response` | `code:US` | `skef10_mg_idreq: ['name', us]` |
| [mg_tips](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L553)<br>`mg_tips(&MgTipsQuery) -> Response` | `code:US; section:Section='zxdt'` | `skef10_mg_zxdt: [section, us, '']` |
| [mg_profile](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L563)<br>`mg_profile(&MgProfileQuery) -> Response` | `code:US` | `skef10_mg_gsgk: ['001', us]` |
| [mg_finance](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L568)<br>`mg_finance(&MgFinanceQuery) -> Response` | `code:US; section:Section='zcfzb'` | `skef10_mg_cwfx: [section, us]` |
| [mg_finance_indicator](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L577)<br>`mg_finance_indicator(&MgFinanceIndicatorQuery) -> Response` | `code:US; section:Section='jbzb'` | `skef10_mg_cwzb: [_MG_CWZB_SECTIONS[section], us]` |
| [mg_business](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L587)<br>`mg_business(&MgBusinessQuery) -> Response` | `code:US` | `skef10_mg_jyfx: [us]` |
| [mg_industry](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L592)<br>`mg_industry(&MgIndustryQuery) -> Response` | `code:US` | `skef10_mg_hyfx: ['001', us, '4', '1']` |
| [mg_industry_compare](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L597)<br>`mg_industry_compare(&MgIndustryCompareQuery) -> Response` | `code:US` | `skef10_mg_hydb: ['001', us, '', '0', '0']` |
| [mg_dividend](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L602)<br>`mg_dividend(&MgDividendQuery) -> Response` | `code:US; section:Section='fh'` | `skef10_mg_fhrz: [us, section]` |
| [mg_capital](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L611)<br>`mg_capital(&MgCapitalQuery) -> Response` | `code:US; section:Section='001'; page:P=1` | `skef10_mg_gbjg: ['002', us, page] / skef10_mg_gbjg: ['001', us, 1]` |
| [mg_shareholders](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L622)<br>`mg_shareholders(&MgShareholdersQuery) -> Response` | `code:US; section:Section='006'; page:P=1; page_size:N=10` | `skef10_mg_gbgd: [section, us, page, page_size] / skef10_mg_gbgd: ['006', us, 1, 10]` |
| [mg_directors](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L640)<br>`mg_directors(&MgDirectorsQuery) -> Response` | `code:US; section:Section='ds'` | `skef10_mg_dsgg: [us, section]` |
| [mg_market](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L649)<br>`mg_market(&MgMarketQuery) -> Response` | `code:US; section:Section='001'; page:P=1; page_size:N=20` | `skef10_mg_schq: ['003', us, 0, 0] / skef10_mg_schq: ['001', us, page, page_size]` |
| [mg_announcements](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L667)<br>`mg_announcements(&MgAnnouncementsQuery) -> Response` | `code:US` | `skef10_mg_gsbd: ['gsgg', us]` |
| [mg_rating](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L672)<br>`mg_rating(&MgRatingQuery) -> Response` | `code:US; section:Section='001'` | `skef10_mg_gspj: [section, us]` |
| [mg_topic](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L677)<br>`mg_topic(&MgTopicQuery) -> Response` | `code:US` | `skef10_mg_rdtc: ['001', us, '', '4', '1']` |
| [mg_deep_analysis](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L682)<br>`mg_deep_analysis(&MgDeepAnalysisQuery) -> Response` | `code:US; section:Section='001'` | `skef10_mg_sdfx: [section, us]` |

#### Hong Kong (20)

| Python entry / Rust method(Q) -> R | Query fields / defaults | Wire / composition |
| --- | --- | --- |
| [hk_name](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L691)<br>`hk_name(&HkNameQuery) -> Response` | `code:HK` | `skef10_hk_idreq: ['name', hk]` |
| [hk_tips](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L696)<br>`hk_tips(&HkTipsQuery) -> Response` | `code:HK; section:Section='1'` | `skef10_hk_zxts: [section, hk]` |
| [hk_events](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L701)<br>`hk_events(&HkEventsQuery) -> Response` | `code:HK; section:Section='1'` | `skef10_hk_gsds: [section, hk]` |
| [hk_profile](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L706)<br>`hk_profile(&HkProfileQuery) -> Response` | `code:HK; section:Section='1'` | `skef10_hk_gsgk: [section, hk]` |
| [hk_related](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L711)<br>`hk_related(&HkRelatedQuery) -> Response` | `code:HK; section:Section='6'` | `skef10_hk_glzq: [section, hk]` |
| [hk_executives](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L716)<br>`hk_executives(&HkExecutivesQuery) -> Response` | `code:HK; section:Section='1'` | `skef10_hk_ggzl: [section, hk]` |
| [hk_capital](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L721)<br>`hk_capital(&HkCapitalQuery) -> Response` | `code:HK; section:Section='1'` | `skef10_hk_gbjg: [section, hk]` |
| [hk_shareholders](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L726)<br>`hk_shareholders(&HkShareholdersQuery) -> Response` | `code:HK; section:Section='1'` | `skef10_hk_gdcg: [section, hk, '']` |
| [hk_finance](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L731)<br>`hk_finance(&HkFinanceQuery) -> Response` | `code:HK; section:Section='zcfzb'` | `skef10_hk_cwfx: [_HK_CWFX_SECTIONS[section], hk]` |
| [hk_finance_indicator](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L741)<br>`hk_finance_indicator(&HkFinanceIndicatorQuery) -> Response` | `code:HK; section:Section='jbzb'` | `skef10_hk_cwzb: [_HK_CWZB_SECTIONS[section], hk]` |
| [hk_business](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L751)<br>`hk_business(&HkBusinessQuery) -> Response` | `code:HK; section:Section='1'` | `skef10_hk_jyfx: [section, hk]` |
| [hk_industry_compare](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L756)<br>`hk_industry_compare(&HkIndustryCompareQuery) -> Response` | `code:HK; section:Section='1'` | `skef10_hk_hybj: [section, hk, '0', '0']` |
| [hk_market](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L761)<br>`hk_market(&HkMarketQuery) -> Response` | `code:HK; section:Section='3'` | `skef10_hk_schq: [section, hk]` |
| [hk_capital_flow](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L771)<br>`hk_capital_flow(&HkCapitalFlowQuery) -> Response` | `code:HK; section:Section='001'` | `skef10_hk_schq_zjlx: [section, hk, '0']` |
| [hk_news](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L779)<br>`hk_news(&HkNewsQuery) -> Response` | `code:HK; section:Section='3'` | `skef10_hk_gszx: [section, hk]` |
| [hk_topic](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L784)<br>`hk_topic(&HkTopicQuery) -> Response` | `code:HK` | `skef10_hk_rdtc: ['001', hk, '', '0', '0']` |
| [hk_pepb_band](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L789)<br>`hk_pepb_band(&HkPepbBandQuery) -> Response` | `code:HK; sjfw:S=empty; tab_value:S=empty; hqzq:S=empty` | `skef10_hk_sdfx_pepb_band: ['001', hk, sjfw, tab_value, hqzq]` |
| [hk_valuation](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L803)<br>`hk_valuation(&HkValuationQuery) -> Response` | `code:HK; item_id:S=empty` | `skef10_hk_sdfx_gzfx: [hk, item_id]` |
| [hk_price_chart](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L808)<br>`hk_price_chart(&HkPriceChartQuery) -> Response` | `code:HK` | `skef10_hk_sdfx_hqzft: [hk]` |
| [hk_analyst_forecast](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L813)<br>`hk_analyst_forecast(&HkAnalystForecastQuery) -> Response` | `code:HK; start:D?=None -> wire empty; end:D?=None -> wire empty` | `skef10_hk_sdfx_fxysyfx: [hk, start.strip().replace('-', ''), end.strip().replace('-', '')]` |

#### Third board (13)

| Python entry / Rust method(Q) -> R | Query fields / defaults | Wire / composition |
| --- | --- | --- |
| [sb_name](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L840)<br>`sb_name(&SbNameQuery) -> Response` | `code:SB` | `skef10_sb_idreq: ['name', code]` |
| [sb_profile](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L845)<br>`sb_profile(&SbProfileQuery) -> Response` | `code:SB; section:Section='001'` | `skef10_sb_gsgk: [code, section]` |
| [sb_tips](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L850)<br>`sb_tips(&SbTipsQuery) -> Response` | `code:SB; section:Section='001'` | `skef10_sb_zxts: [code, section]` |
| [sb_events](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L855)<br>`sb_events(&SbEventsQuery) -> Response` | `code:SB; section:Section='ydjy'` | `skef10_sb_jyds: [code, section]` |
| [sb_capital](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L860)<br>`sb_capital(&SbCapitalQuery) -> Response` | `code:SB; section:Section='gbjg'` | `skef10_sb_gbjg: [code, section]` |
| [sb_shareholders](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L865)<br>`sb_shareholders(&SbShareholdersQuery) -> Response` | `code:SB; section:Section='sdgd'` | `skef10_sb_gdyj: [code, section]` |
| [sb_compliance](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L870)<br>`sb_compliance(&SbComplianceQuery) -> Response` | `code:SB; section:Section='001'` | `skef10_sb_hgyy: [code, section]` |
| [sb_dividend](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L875)<br>`sb_dividend(&SbDividendQuery) -> Response` | `code:SB; section:Section='fh'` | `skef10_sb_fhrz: [code, section]` |
| [sb_finance](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L880)<br>`sb_finance(&SbFinanceQuery) -> Response` | `code:SB; section:Section='zcfzb'` | `skef10_sb_cwfx: [code, section]` |
| [sb_business](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L888)<br>`sb_business(&SbBusinessQuery) -> Response` | `code:SB; section:Section='001'` | `skef10_sb_jyfx: [code, section]` |
| [sb_major_events](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L893)<br>`sb_major_events(&SbMajorEventsQuery) -> Response` | `code:SB; section:Section='001'` | `skef10_sb_zdsx: [code, section]` |
| [sb_news](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L898)<br>`sb_news(&SbNewsQuery) -> Response` | `code:SB; section:Section='001'` | `skef10_sb_gszx: [code, section]` |
| [sb_industry_compare](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L903)<br>`sb_industry_compare(&SbIndustryCompareQuery) -> Response` | `code:SB; section:Section='hyxx'` | `skef10_sb_hybj: [code, section]` |

#### Bonds / repos (31)

| Python entry / Rust method(Q) -> R | Query fields / defaults | Wire / composition |
| --- | --- | --- |
| [bond_profile](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L918)<br>`bond_profile(&BondProfileQuery) -> Response` | `code:B` | `skef10_bond_kzz_zqgk: ['sdcyrbgq', code, '']` |
| [bond_finance](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L923)<br>`bond_finance(&BondFinanceQuery) -> Response` | `code:B; section:Section='zcfzb'` | `skef10_bond_kzz_cwfx: [section, code]` |
| [bond_rating](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L928)<br>`bond_rating(&BondRatingQuery) -> Response` | `code:B; section:Section='pjzst'` | `skef10_bond_kzz_zqpj: [section, code]` |
| [bond_conversion](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L936)<br>`bond_conversion(&BondConversionQuery) -> Response` | `code:B; section:Section='zgjg'` | `skef10_bond_kzz_zg: [section, code]` |
| [bond_rate](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L944)<br>`bond_rate(&BondRateQuery) -> Response` | `code:B; section:Section='zqll'` | `skef10_bond_kzz_zqll: [section, code]` |
| [bond_announcements](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L952)<br>`bond_announcements(&BondAnnouncementsQuery) -> Response` | `code:B; section:Section='1'` | `skef10_bond_kzz_zjzx: [code, section]` |
| [gz_profile](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L969)<br>`gz_profile(&GzProfileQuery) -> Response` | `code:B; section:Section='zqgk'` | `skef10_bond_gz_zjgk: [section, bond, bond_market]` |
| [gz_interest](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L979)<br>`gz_interest(&GzInterestQuery) -> Response` | `code:B; section:Section='fxqk'` | `skef10_bond_gz_fxqk: [section, bond, bond_market]` |
| [gz_market_data](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L989)<br>`gz_market_data(&GzMarketDataQuery) -> Response` | `code:B; section:Section='dzjy'` | `skef10_bond_gz_zjsj: [section, bond, bond_market]` |
| [gz_rating](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L999)<br>`gz_rating(&GzRatingQuery) -> Response` | `code:B; section:Section='dfzf'` | `skef10_bond_gz_zjpj: [section, bond, bond_market]` |
| [gz_finance](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1009)<br>`gz_finance(&GzFinanceQuery) -> Response` | `code:B` | `skef10_bond_gz_cwfx: ['t', bond, bond_market]` |
| [qyz_profile](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1014)<br>`qyz_profile(&QyzProfileQuery) -> Response` | `code:B; section:Section='zqzl'` | `skef10_bond_qyz_zqgk: [section, bond, bond_market]` |
| [qyz_interest](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1024)<br>`qyz_interest(&QyzInterestQuery) -> Response` | `code:B; section:Section='fxqk'` | `skef10_bond_qyz_xjlfx: [section, bond, bond_market]` |
| [qyz_guarantee](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1034)<br>`qyz_guarantee(&QyzGuaranteeQuery) -> Response` | `code:B` | `skef10_bond_qyz_zqdb: ['dbsm', bond, bond_market]` |
| [qyz_rating](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1041)<br>`qyz_rating(&QyzRatingQuery) -> Response` | `code:B; section:Section='zjpjzst'` | `skef10_bond_qyz_zjpj: [section, bond, bond_market]` |
| [qyz_rate](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1052)<br>`qyz_rate(&QyzRateQuery) -> Response` | `code:B` | `skef10_bond_qyz_zjlv: ['zqlv', bond, bond_market]` |
| [qyz_review](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1059)<br>`qyz_review(&QyzReviewQuery) -> Response` | `code:B` | `skef10_bond_qyz_gsps: [bond, bond_market]` |
| [gzhg_info](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1066)<br>`gzhg_info(&GzhgInfoQuery) -> Response` | `code:B` | `skef10_bond_gzhg_zqxx: ['zqgk', code]` |
| [gzhg_quotes](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1071)<br>`gzhg_quotes(&GzhgQuotesQuery) -> Response` | `code:B; section:Section='bpzs'` | `skef10_bond_gzhg_zqxx: [section, code]` |
| [zczq_profile](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1080)<br>`zczq_profile(&ZczqProfileQuery) -> Response` | `code:B` | `skef10_bond_zczqh_zqgk: [bond, bond_market]` |
| [zczq_finance](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1087)<br>`zczq_finance(&ZczqFinanceQuery) -> Response` | `code:B` | `skef10_bond_zczqh_cwfx: ['t', bond, bond_market]` |
| [zczq_rating](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1094)<br>`zczq_rating(&ZczqRatingQuery) -> Response` | `code:B; section:Section='pjzst'` | `skef10_bond_zczqh_zqpj: [section, bond, bond_market]` |
| [kjhz_profile](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1104)<br>`kjhz_profile(&KjhzProfileQuery) -> Response` | `code:B` | `skef10_bond_kjhz_zqgk: [bond, bond_market]` |
| [kjhz_finance](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1111)<br>`kjhz_finance(&KjhzFinanceQuery) -> Response` | `code:B` | `skef10_bond_kjhz_cwfx: ['t', bond]` |
| [kjhz_interest](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1116)<br>`kjhz_interest(&KjhzInterestQuery) -> Response` | `code:B; section:Section='fxqk'` | `skef10_bond_kjhz_fxqk: [section, bond]` |
| [kjhz_guarantee](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1124)<br>`kjhz_guarantee(&KjhzGuaranteeQuery) -> Response` | `code:B` | `skef10_bond_kjhz_zqdb: ['dbsm', bond]` |
| [kjhz_rating](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1129)<br>`kjhz_rating(&KjhzRatingQuery) -> Response` | `code:B; section:Section='pjzst'` | `skef10_bond_kjhz_zqpj: [section, bond]` |
| [kjhz_exchange](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1138)<br>`kjhz_exchange(&KjhzExchangeQuery) -> Response` | `code:B; section:Section='jgtzzst'` | `skef10_bond_kjhz_jhqk: [section, bond]` |
| [kjhz_rate](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1147)<br>`kjhz_rate(&KjhzRateQuery) -> Response` | `code:B; section:Section='zqll'` | `skef10_bond_kjhz_zqll: [section, bond]` |
| [bond_terms](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1155)<br>`bond_terms(&BondTermsQuery) -> Response` | `code:B; section:Section='hstk'` | `skef10_bond_kzz_lltk: [section, code]` |
| [bond_bulletin](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1164)<br>`bond_bulletin(&BondBulletinQuery) -> Response` | `code:B` | `skef10_bond_zjzx: [bond, bond_market]` |

#### Industry (13)

| Python entry / Rust method(Q) -> R | Query fields / defaults | Wire / composition |
| --- | --- | --- |
| [hy_summary](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1179)<br>`hy_summary(&HySummaryQuery) -> Response` | `hy:HY; section:Section='hqtj'` | `section registry` |
| [hy_stage_rise](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1194)<br>`hy_stage_rise(&HyStageRiseQuery) -> Response` | `hy:HY; flag:S='e'` | `skef10_hy_jdzf: [hy, flag]` |
| [hy_profile](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1199)<br>`hy_profile(&HyProfileQuery) -> Response` | `hy:HY` | `skef10_hy_hyzl_hyjs: [hy]` |
| [hy_commentary](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1204)<br>`hy_commentary(&HyCommentaryQuery) -> Response` | `hy:HY; section:Section='001'` | `skef10_hy_hyzl_hyplk: [section, hy]` |
| [hy_news](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1209)<br>`hy_news(&HyNewsQuery) -> Response` | `hy:HY; section:Section='hydt'` | `skef10_hy_zxdt: [section, hy]` |
| [hy_members_rank](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1217)<br>`hy_members_rank(&HyMembersRankQuery) -> Response` | `hy:HY; section:Section='cwzkpm'` | `skef10_hy_hydw_cwzkpm: ['002', hy, '', 'e', ''] / skef10_hy_hydw_gsgmpm: [hy] / skef10_hy_hydw_gzsppm: ['01', hy, 'o'] / skef10_hy_hydw_scbxpm: ['scpm', '', hy] / skef10_hy_hydw_zysrpm: ['001', hy, '']` |
| [hy_capital_stats](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1238)<br>`hy_capital_stats(&HyCapitalStatsQuery) -> Response` | `hy:HY; section:Section='001'` | `skef10_hy_hysj_bxzjtj: [section, hy, 'n' if section == '001' else '']` |
| [hy_fund_holdings](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1246)<br>`hy_fund_holdings(&HyFundHoldingsQuery) -> Response` | `hy:HY; section:Section='1'` | `skef10_hy_hysj_jjzb: [section, hy, '']` |
| [hy_capital_flow](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1251)<br>`hy_capital_flow(&HyCapitalFlowQuery) -> Response` | `hy:HY; section:Section='001'` | `skef10_hy_hysj_zjlx: [section, hy]` |
| [hy_analyst_index](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1259)<br>`hy_analyst_index(&HyAnalystIndexQuery) -> Response` | `hy:HY; section:Section='001'` | `skef10_hy_hyyj_fxszs: [section, hy, '', '']` |
| [hy_major_holders](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1264)<br>`hy_major_holders(&HyMajorHoldersQuery) -> Response` | `hy:HY` | `skef10_hy_hyyj_hymzgd: [hy]` |
| [hy_rating_summary](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1269)<br>`hy_rating_summary(&HyRatingSummaryQuery) -> Response` | `hy:HY; section:Section='001'` | `skef10_hy_hyyj_hypjhz: [section, hy, '']` |
| [hy_index](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1274)<br>`hy_index(&HyIndexQuery) -> Response` | `hy:HY` | `skef10_hy_idsy: ['002', hy]` |

#### Futures / indices (8)

| Python entry / Rust method(Q) -> R | Query fields / defaults | Wire / composition |
| --- | --- | --- |
| [qh_position_report](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1287)<br>`qh_position_report(&QhPositionReportQuery) -> Response` | `code:QH; section:Section='cjcc'; date:D?=None -> wire empty; end_date:D?=None -> wire empty; index:bool=false` | `prefix + suffix: ['1', code, d1, d2, ''] / prefix + suffix: ['1', code, d1] / prefix + suffix: ['1', code, d1, '']` |
| [qh_basis_spread](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1315)<br>`qh_basis_spread(&QhBasisSpreadQuery) -> Response` | `code:QH; section:Section='001'; period:S='1Y'` | `qhf9_sp_jcjcfx: [section, code, period, '', '']` |
| [qh_warehouse_receipt](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1325)<br>`qh_warehouse_receipt(&QhWarehouseReceiptQuery) -> Response` | `code:QH; date:D?=None -> wire empty` | `qhf9_sp_ckrb_ckrb: ['1', code, date.strip().replace('-', '')]` |
| [qh_inventory](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1333)<br>`qh_inventory(&QhInventoryQuery) -> Response` | `code:QH` | `qhf9_sp_kcbg_kcbg: ['1', code]` |
| [qh_variety_info](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1342)<br>`qh_variety_info(&QhVarietyInfoQuery) -> Response` | `code:QH; section:Section='pzjj'` | `section registry` |
| [qh_index_info](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1357)<br>`qh_index_info(&QhIndexInfoQuery) -> Response` | `code:QH; section:Section='zsgk'` | `qhf9_gz_zszl_' + section: ['1' if section == 'zsgk' else '2', code]` |
| [qh_index_valuation](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1366)<br>`qh_index_valuation(&QhIndexValuationQuery) -> Response` | `code:QH='000300'; section:Section='dtpe'; start:D?=None -> wire empty; end:D?=None -> wire empty` | `qhf9_gz_zsgz_' + section: ['1', code, start.strip().replace('-', ''), end.strip().replace('-', '')]` |
| [qh_weights](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1384)<br>`qh_weights(&QhWeightsQuery) -> Response` | `code:QH='399300'; section:Section='zxcfg'; date:D?=None -> wire empty` | `qhf9_gz_cfg_' + section: ['1', code, d] / qhf9_gz_cfg_cfghyfb: ['1', code]` |

#### Market / composite / news / valuation (39)

| Python entry / Rust method(Q) -> R | Query fields / defaults | Wire / composition |
| --- | --- | --- |
| [market_report](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1398)<br>`market_report(&MarketReportQuery) -> Response` | `code:Option<A>=empty; section:Section='board_info'; arg:S=empty; date:D?=None; page:P=1; page_size:N=20` | `section registry` |
| [index_members](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1486)<br>`index_members(&IndexMembersQuery) -> IndexMemberTable` | `code:A; all_pages:bool=true; page_size:N=100; max_pages:Limit=100` | `market_report(index_members), pagination` |
| [index_tracking_products](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1539)<br>`index_tracking_products(&IndexTrackingProductsQuery) -> IndexTrackingTable` | `code:A; all_pages:bool=true; page_size:N=100; max_pages:Limit=100` | `market_report(index_etfs), pagination` |
| [board_members](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1595)<br>`board_members(&BoardMembersQuery) -> BoardMemberTable` | `code:A; all_pages:bool=true; page_size:N=100; max_pages:Limit=50` | `market_report(board_members), dividend record pagination` |
| [ipo_calendar](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1663)<br>`ipo_calendar() -> IpoCalendar` | `-` | `market_report(ipo_calendar)` |
| [dividend_financing](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1695)<br>`dividend_financing(&DividendFinancingQuery) -> Response` | `code:A; section:Section='fh'` | `tdxf10_gg_fhrz: [code, section]` |
| [allotment_dates](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1700)<br>`allotment_dates(&AllotmentDatesQuery) -> Response` | `code:A` | `CWServ.tdxf10_gg_fhrz_zfhpmx: ['zfpg_bgq', code, '']` |
| [allotment_details](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1705)<br>`allotment_details(&AllotmentDetailsQuery) -> Response` | `code:A; date:D` | `CWServ.tdxf10_gg_fhrz_zfhpmx: ['zfpg', code, date]` |
| [finance_report](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1710)<br>`finance_report(&FinanceReportQuery) -> Response` | `code:A; report_type:S='zcfzb'` | `tdxf10_gg_cwfx: [code, report_type, '']` |
| [finance_diagnosis](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1715)<br>`finance_diagnosis(&FinanceDiagnosisQuery) -> Response` | `code:A; section:Section='zzyj'; scope:S=empty` | `tdxf10_gg_cwzd: [section, code, scope]` |
| [stock_score](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1725)<br>`stock_score(&StockScoreQuery) -> Response` | `code:A; section:Section='pf'; arg:S=empty` | `tdxf10_gg_ggzp: [section, code, arg, '']` |
| [profit_forecast](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1730)<br>`profit_forecast(&ProfitForecastQuery) -> Response` | `code:A; section:Section='ylyctj'` | `tdxf10_gg_ybpj: [code, section]` |
| [ranking_detail](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1737)<br>`ranking_detail(&RankingDetailQuery) -> Response` | `code:A; section:Section='scpmdela'` | `tdxf10_gg_zxts_rqpm: [code, section]` |
| [governance](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1742)<br>`governance(&GovernanceQuery) -> Response` | `code:A; section:Section='gljy'; arg:S=empty` | `tdxf10_gg_zbyz: [section, code, arg]` |
| [hot_topics](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1753)<br>`hot_topics(&HotTopicsQuery) -> Response` | `code:A; section:Section='zttzbkz'` | `tdxf10_gg_rdtc: [code, section]` |
| [topic_compare](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1758)<br>`topic_compare(&TopicCompareQuery) -> Response` | `code:A; topic_id:S; section:Section='gndbzfsj'; sort_by:S='zdf'` | `tdxf10_gg_rdtc_gndb: [section, code, topic_id, sort_by]` |
| [topic_compare_first](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1763)<br>`topic_compare_first(&TopicCompareFirstQuery) -> Response` | `code:A; section:Section='gndbzfsj'; sort_by:S='zdf'` | `topic_ids -> topic_compare` |
| [company_news](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1771)<br>`company_news(&CompanyNewsQuery) -> Response` | `code:A; section:Section='gsyj'; keyword:S=empty; rating:S='0'; page:P=1; page_size:N=20` | `tdxf10_gg_gszx: [code, section, keyword, str(rating), page_text, size_text] / tdxf10_gg_cwfx_cbdp: [code, page_text]` |
| [digest](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1792)<br>`digest(&DigestQuery) -> Response` | `code:A; section:Section='tips'; page:P=1; page_size:N=20` | `section registry` |
| [northbound_holding](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1829)<br>`northbound_holding(&NorthboundHoldingQuery) -> Response` | `code:A; section:Section='bszj'; filter_value:S=empty; page:P=1; page_size:N=20` | `tdxf10_gg_zlcc: [code, section, filter_value, page_text, page_text, size_text]` |
| [detail](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1850)<br>`detail(&DetailQuery) -> Response` | `detail_type:S; record_id:S` | `tdxf10_gg_idreq: [detail_type, record_id]` |
| [cache_list](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1855)<br>`cache_list(&CacheListQuery) -> Response` | `code:A; kind:S='gg'` | `CWSearch.tzx_rcache, kind=xw/gg/ly` |
| [announcements](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1864)<br>`announcements(&AnnouncementsQuery) -> Response` | `code:A` | `cache_list(code, 'gg')` |
| [news](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1869)<br>`news(&NewsQuery) -> Response` | `code:A` | `cache_list(code, 'xw')` |
| [roadshows](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1874)<br>`roadshows(&RoadshowsQuery) -> Response` | `code:A` | `cache_list(code, 'ly')` |
| [news_channels](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1882)<br>`news_channels() -> Vec<NewsChannelInfo>` | `-` | `local NEWS_CHANNEL_LABELS, sorted by id` |
| [market_news](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1889)<br>`market_news(&MarketNewsQuery) -> Response` | `tab:Tab=Headlines(3); page:P=1; page_size:N=50` | `call('tdxi_zxxwsy', {'Params': [str(int(tab)), page_text, size_text]})` |
| [channel_news](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1907)<br>`channel_news(&ChannelNewsQuery) -> Response` | `channel:Channel; page:P=1; page_size:N=50; query1:S='1'; keyword:S=empty` | `call('tdxi_zxxwnews', {'Params': [str(query1), str(int(channel)), page_text, size_text, keyword]})` |
| [theme_market](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1925)<br>`theme_market(&ThemeMarketQuery) -> Response` | `code:A; req_id:RequestId='200743'; page:P=-1; page_size:N=10; zq_num:Count=30; extra:Extra?=None` | `HQServ.hq_nlp_tcihq (2.3)` |
| [stock_stage_gain](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1960)<br>`stock_stage_gain(&StockStageGainQuery) -> Response` | `code:A; days:Count=60; page_size:N=10` | `theme_market(code, 200740, page_size=page_size, zq_num=days)` |
| [stock_main_funds](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1974)<br>`stock_main_funds(&StockMainFundsQuery) -> Response` | `code:A; days:Count=30; page_size:N=10` | `theme_market(code, 200742, page_size=page_size, zq_num=days)` |
| [stock_industry_rank](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L1987)<br>`stock_industry_rank(&StockIndustryRankQuery) -> Response` | `code:A; days:Count=30; page_size:N=10` | `theme_market(code, 200745, page_size=page_size, zq_num=days)` |
| [stock_industry_pct_rank](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L2001)<br>`stock_industry_pct_rank(&StockIndustryPctRankQuery) -> Response` | `code:A; days:Count=30; page_size:N=10` | `theme_market(code, 200747, page_size=page_size, zq_num=days)` |
| [stock_board_member_quotes](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L2014)<br>`stock_board_member_quotes(&StockBoardMemberQuotesQuery) -> Response` | `board_code:A; board_market:F10MarketCode(u16)=1; page:P=-1; page_size:N=-1` | `call('HQServ.hq_nlp_tcihq', [payload])` |
| [stock_valuation_percentile](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L2040)<br>`stock_valuation_percentile(&StockValuationPercentileQuery) -> Response` | `code:A; kind:S='0'; page:P=-1; page_size:N=5` | `valuation(code, 200192, page=page, page_size=page_size, extra={'Type': kind})` |
| [stock_pepb_percentile](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L2058)<br>`stock_pepb_percentile(&StockPepbPercentileQuery) -> Response` | `code:A; indicator:S='0'; window:S='1Y'; period:S='0'; page:P=1; page_size:N=300` | `valuation(code, 200124, page=page, page_size=page_size, extra={'sjfw': window, 'zb': indicator, 'hqzq': period})` |
| [stock_history_trend](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L2085)<br>`stock_history_trend(&StockHistoryTrendQuery) -> Response` | `code:A` | `valuation(code, 200135)` |
| [stock_finance_weights](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L2093)<br>`stock_finance_weights(&StockFinanceWeightsQuery) -> Response` | `code:A; section:Section='zcfzb'` | `tdxf10_gg_zxtscwbq: [code, section]` |
| [valuation](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/f10/client.py#L2102)<br>`valuation(&ValuationQuery) -> Response` | `code:A; req_id:RequestId='200191'; page:P=0; page_size:N=20; extra:Extra?=None` | `HQServ.hq_nlp_gpsj (2.3)` |

### 2.3 section registry、关键请求与组合语义

以下 registry 是 typed query 的明确受理集合；没有列出的开放式 section 使用 `SectionCode` 字符串并在 endpoint fixture 中冻结。相同 `section` 文本不跨方法复用语义。

| Method | Closed section registry |
| --- | --- |
| `shareholders` | `count`, `ranking`, `controller`, `top_circulating`, `top`, `bonds`, `changes`, `institutions`, `institution_types`, `institution_holdings`, `plan` |
| `capital_structure` | `structure`, `changes`, `buyback`, `restricted`, `restricted_detail` |
| `business_report` | `business`, `composition`, `customers`, `suppliers`, `price`, `changes` |
| `industry_analysis` | `peers`, `dividend_rank`, `scale`, `valuation`, `news`, `reports`, `market_share`, `summary`, `comparable`, `comparable_profile` |
| `trading_report` | `margin`, `margin_history`, `block_trades`, `dragon_board`, `northbound`, `limit_down`, `limit_up`, `bonds`, `main_force`, `reports`, `kcb` |
| `fund_report` | `name`, `managers`, `company`, `basic`, `seats`, `scale`, `scale_history`, `nav`, `nav_period`, `nav_rank`, `dividends`, `asset_allocation`, `industry_allocation`, `holdings_compare`, `holdings_rank`, `holding_summary`, `bond_allocation`, `futures_allocation`, `holdings`, `holdings_detail`, `holdings_changes`, `fund_of_funds`, `finance`, `sales_info`, `sales_announcements`, `sales_news`, `etf`, `margin`, `ratings`, `categories` |
| `mg_tips` | `zxdt`, `sypl`, `gsgy`, `zysj` |
| `mg_finance` | `zcfzb`, `lyb`, `xjllb` |
| `mg_finance_indicator` | `jbzb`, `ylnl`, `cznl`, `yynl`, `ccznl` |
| `mg_dividend` | `fh`, `cf` |
| `mg_capital` | `001`, `002` |
| `mg_directors` | `ds`, `gg`, `lrdjg` |
| `mg_market` | `001`, `003` |
| `hk_finance` | `zhsyb`, `zcfzb`, `xjllb` |
| `hk_finance_indicator` | `jbzb`, `ylnl`, `cznl`, `yynl`, `ccznl` |
| `hy_summary` | `hqtj`, `hyzq`, `hygy`, `zszfb` |
| `hy_members_rank` | `cwzkpm`, `gsgmpm`, `gzsppm`, `scbxpm`, `zysrpm` |
| `qh_position_report` | `cjcc`, `cjjg`, `jcgc`, `ccjj`, `ykfx` |
| `qh_variety_info` | `pzjj`, `hyjj`, `xgzq` |
| `qh_index_info` | `zsgk`, `lsjg` |
| `qh_index_valuation` | `dtpe`, `lspe`, `xjfh` |
| `qh_weights` | `zxcfg`, `lscfg`, `cfgqz`, `cfghyfb`, `xwpl` |
| `market_report` | `board_info`, `board_basics`, `board_description`, `board_trading`, `board_flow`, `board_members`, `board_analysis`, `board_finance_compare`, `board_candidates`, `board_trend`, `index_info`, `index_profile`, `index_valuation`, `index_tracking`, `index_flow`, `index_flow_members`, `index_notices`, `market_basic`, `market_margin`, `market_margin_summary`, `market_margin_investors`, `market_new_shares`, `market_capital` |
| `profit_forecast` | `ylyctj`, `ylycmx`, `yzyq`, `tzpjtj`, `ycpjyjbg` |
| `digest` | `tips`, `profile`, `events`, `news`, `announcements`, `reports`, `roadshows`, `interaction` |


#### 关闭式分支的具体Entry与Params

记法同2.2；下表将语义section落到具体wire参数，日期和前置请求仍按2.3处理。字符串与数字不互换。

`shareholders` aliases: `gdyj='CWServ.tdxf10_gg_gdyj'`

| section | (Entry, Params) |
| --- | --- |
| `count` | `(gdyj, (code6, 'gdrs', filter1, filter2, page_text, page_text, size_text))` |
| `ranking` | `(gdyj, (code6, 'thygdrs', filter1, filter2, page_text, page_text, size_text))` |
| `controller` | `(gdyj, (code6, 'kggd', filter1, filter2, page_text, page_text, size_text))` |
| `top_circulating` | `(gdyj, (code6, 'ltgd', filter1, filter2, page_text, page_text, size_text))` |
| `top` | `(gdyj, (code6, 'sdgdbgq', filter1, filter2, page_text, page_text, size_text))` |
| `bonds` | `(gdyj, (code6, 'sdzqcyr', filter1, filter2, page_text, page_text, size_text))` |
| `changes` | `(gdyj, (code6, 'cgbd', filter1, filter2, page_text, page_text, size_text))` |
| `institutions` | `(gdyj, (code6, 'jgcg', filter1, filter2, page_text, page_text, size_text))` |
| `institution_types` | `(gdyj, (code6, 'jgcgz', filter1, date_text, page_text, page_text, size_text))` |
| `institution_holdings` | `('CWServ.tdxf10_gg_gdyj_jgcgmx', (code6, kind, date_text, '99', page_text, page_text, size_text))` |
| `plan` | `(gdyj, (code6, 'gdzjcjh', filter1, filter2, page_text, page_text, size_text))` |

`capital_structure` aliases: `gbjg='CWServ.tdxf10_gg_gbjg'`

| section | (Entry, Params) |
| --- | --- |
| `structure` | `(gbjg, (code6, 'gbjg'))` |
| `changes` | `(gbjg, (code6, 'gbbd'))` |
| `buyback` | `(gbjg, (code6, 'gphg'))` |
| `restricted` | `(gbjg, (code6, 'xslt'))` |

`business_report` aliases: `jyfx='CWServ.tdxf10_gg_jyfx'`

| section | (Entry, Params) |
| --- | --- |
| `business` | `(jyfx, (code6, 'zyyw', ''))` |
| `composition` | `(jyfx, (code6, 'zygc', date_text))` |
| `customers` | `(jyfx, (code6, 'qwm', date_text))` |
| `suppliers` | `(jyfx, (code6, 'qwmgys', date_text))` |
| `price` | `(jyfx, (code6, 'cpjg', ''))` |
| `changes` | `(jyfx, (code6, '0', date_text))` |

`industry_analysis` aliases: `hyfx='CWServ.tdxf10_gg_hyfx'; kbgs='CWServ.tdxf10_gg_kbgs'`

| section | (Entry, Params) |
| --- | --- |
| `peers` | `(hyfx, ('cwgz', code6, arg or '000'))` |
| `dividend_rank` | `(hyfx, ('fhrzb', code6, arg or '100'))` |
| `scale` | `(hyfx, ('gsgm', code6, arg or '000'))` |
| `valuation` | `(hyfx, ('gzsp', code6, arg or '011'))` |
| `news` | `(hyfx, ('hyxw', code6, arg or ''))` |
| `reports` | `(hyfx, ('hyyb', code6, arg or ''))` |
| `market_share` | `(hyfx, ('scbx', code6, arg or '000'))` |
| `summary` | `(hyfx, ('tot', code6, arg or ''))` |
| `comparable` | `(kbgs, ('1', code6))` |
| `comparable_profile` | `(kbgs, ('0', code6))` |

`trading_report` aliases: `jyds='CWServ.tdxf10_gg_jyds'`

| section | (Entry, Params) |
| --- | --- |
| `margin` | `(jyds, (code6, 'rzrq', ''))` |
| `margin_history` | `('CWServ.tdxf10_gg_sj', ('rzrq_sj', code6, '0', code6))` |
| `block_trades` | `(jyds, (code6, 'dzjy', ''))` |
| `dragon_board` | `(jyds, (code6, 'lgt', arg or '2'))` |
| `northbound` | `(jyds, (code6, 'islgt', ''))` |
| `limit_down` | `(jyds, (code6, 'dtfx', ''))` |
| `limit_up` | `(jyds, (code6, 'ztfx', ''))` |
| `bonds` | `(jyds, (code6, 'zrq', ''))` |
| `main_force` | `(jyds, (code6, 'zlpskcj', ''))` |
| `reports` | `(jyds, (code6, 'yxsbxx', ''))` |
| `kcb` | `(jyds, (code6, 'checkkcb', ''))` |

`fund_report` aliases: `jj='CWServ.skef10_jj_'; jjzl=f'{jj}jjzl'; gmfe=f'{jj}gmfe'; jzhb=f'{jj}jzhb'; tzzh=f'{jj}tzzh'; cgqk=f'{jj}cgqk'; xsxx=f'{jj}xsxx'; etfzj=f'{jj}etfzj'`

| section | (Entry, Params) |
| --- | --- |
| `name` | `(f'{jj}idreq', ('name', code6))` |
| `managers` | `(jjzl, ('1', code6, '', page_text, page_text, size_text))` |
| `company` | `(jjzl, ('2', code6, '', page_text, page_text, size_text))` |
| `basic` | `(jjzl, ('4', code6, '', page_text, page_text, size_text))` |
| `seats` | `(jjzl, ('5', code6, arg, page_text, page_text, size_text))` |
| `scale` | `(gmfe, ('0', code6, '', ''))` |
| `scale_history` | `(gmfe, (arg or '0', code6, '', date_text))` |
| `nav` | `(jzhb, ('0', code6, '0', ''))` |
| `nav_period` | `(jzhb, ('1', code6, '0', ''))` |
| `nav_rank` | `(jzhb, ('3', code6, '0', ''))` |
| `dividends` | `(jzhb, ('5', code6, '0', ''))` |
| `asset_allocation` | `(tzzh, ('zcpz', code6, ''))` |
| `industry_allocation` | `(tzzh, ('hyfb', code6, date_text))` |
| `holdings_compare` | `(tzzh, ('cgjzd', code6, ''))` |
| `holdings_rank` | `(tzzh, ('zjpz', code6, ''))` |
| `holding_summary` | `(tzzh, ('jjxtzzh', code6, ''))` |
| `bond_allocation` | `(tzzh, ('zczq', code6, date_text))` |
| `futures_allocation` | `(tzzh, ('ccqhmx', code6, date_text))` |
| `holdings` | `(cgqk, ('cgqkbgq', code6, '', '', '1', '1', '1'))` |
| `holdings_detail` | `(cgqk, ('cgmxbgq', code6, '', '', '1', '1', '1'))` |
| `holdings_changes` | `(cgqk, ('cyjjmxbgq', code6, '', '', '1', '1', '1'))` |
| `fund_of_funds` | `(cgqk, ('sxjjzcbgq', code6, '', '', '1', '1', '1'))` |
| `finance` | `(f'{jj}cwzk', (arg or 'zcfzb', code6))` |
| `sales_info` | `(xsxx, ('0', code6))` |
| `sales_announcements` | `(xsxx, ('4', code6))` |
| `sales_news` | `(xsxx, ('5', code6))` |
| `etf` | `(etfzj, ('etfzjlx', code6, date_text or '3M'))` |
| `margin` | `(etfzj, ('etfrzrq', code6, date_text or '3M'))` |
| `ratings` | `(f'{jj}jjpj', (arg or '001', code6))` |
| `categories` | `(f'{jj}fxsy', (arg or 'bgqzf', code6, ''))` |

`market_report` aliases: `bk='CWServ.skef10_bk_'; zs='CWServ.skef10_zs_'; dpzx='CWServ.skef10_dpzx_'`

| section | (Entry, Params) |
| --- | --- |
| `board_info` | `(f'{bk}getinfo', (code6,))` |
| `board_basics` | `(f'{bk}cpbd_jczl', ('001', code6, ''))` |
| `board_description` | `(f'{bk}cpbd_jczl', ('002', code6, ''))` |
| `board_trading` | `(f'{bk}cpbd_jczl', ('004', code6, ''))` |
| `board_flow` | `(f'{bk}zjlx', (arg or '001_1', code6, ''))` |
| `board_members` | `(f'{bk}scgl', (arg or '001', code6, page_text, size_text))` |
| `board_analysis` | `(f'{bk}sdfx', ('001', code6, ''))` |
| `board_finance_compare` | `(f'{bk}ggdb_cwdb', (code6,))` |
| `board_candidates` | `(f'{bk}cfgsj', (arg or '001', code6))` |
| `board_trend` | `(f'{bk}ydfx', (code6,))` |
| `index_info` | `(f'{zs}getinfo', (code6,))` |
| `index_profile` | `(f'{zs}jbxx_zsgk', (code6,))` |
| `index_valuation` | `(f'{zs}jbxx_gzxx', ('001', code6, '', ''))` |
| `index_tracking` | `(f'{zs}jbxx_gzbd', (code6, page_text, size_text))` |
| `index_flow` | `(f'{zs}zjdx', (arg or '004_1', code6, '', '', page_text, size_text))` |
| `index_flow_members` | `(f'{zs}cfgsj_cfgjcsj', (arg or '001', code6, page_text, size_text))` |
| `index_notices` | `(f'{zs}jrts', (arg or '001', code6, page_text, size_text))` |
| `market_basic` | `(f'{dpzx}jbxx', ('002', code6))` |
| `market_margin` | `(f'{dpzx}rzrq', ('001', code6))` |
| `market_margin_summary` | `(f'{dpzx}rzrq', ('002', code6))` |
| `market_margin_investors` | `(f'{dpzx}rzrq', ('003', arg or '1m'))` |
| `market_new_shares` | `(f'{dpzx}xgfx', ('002',))` |
| `market_capital` | `(f'{dpzx}scgl', (arg or '004',))` |

`digest` aliases: `zxts='CWServ.tdxf10_gg_zxts'`

| section | (Entry, Params) |
| --- | --- |
| `tips` | `(zxts, (code6, 'zxts', ''))` |
| `profile` | `(zxts, (code6, 'gsgy', ''))` |
| `events` | `(zxts, (code6, 'gsds', ''))` |
| `news` | `(zxts, (code6, 'gsxw', ''))` |
| `announcements` | `(zxts, (code6, 'gsgg', ''))` |
| `reports` | `(zxts, (code6, 'gsyj', ''))` |
| `roadshows` | `(zxts, (code6, 'lyhd', ''))` |
| `interaction` | `('CWServ.tdxf10_gg_zxtstzzhd', (code6, '1', '1', str(page_size)))` |

`stock_info/business_periods/topic_ids` 都走 `CWServ.tdxf10_gg_comreq`，分别为 Params `["gpquery", code]` / `["zygcfx", code]` / `["rdtcgn", code]`；`business_composition` 的 None 报告期先取 business_periods 的首个 T002，再走 business_report composition。fund 的 industry_allocation/bond_allocation/futures_allocation 未给日期时先查相应 `hyfbbgq/zczqbgq/ccqhmxbgq`，不能直接用系统今天。`topic_compare_first` 先取 topic_ids 第一个可用主题；缺元数据不发第二个伪造请求。

`TopicCompareQuery::new(code, topic_id) -> Result<Self>`要求构造时给非空、非全空白且无控制字符的topic_id；更新ID同样校验。Rust自动主题查询跳过null/bool/无效字符串，业务失败的前置响应也不发第二请求；可用数字保留原lexeme，不丢前导零字符串。选中ID复制/累计字节计费及所有请求共用父操作。Python原`_first_value`只读首行且其字符串转换更宽松，由T23兼容投影保持，不把新Rust“首个可用”搜索施加到旧入口。`FundReportQuery::with_section`和`with_date`都立即检查已有date/window组合，禁止先设置3M再切换到必须报告日期的分区绕过构造器。

`market_report` 别名：index_members → index_flow_members；index_etfs → index_tracking；ipo_calendar → market_new_shares。只有 market_new_shares/market_capital 可省 code。个股 `trading_report.margin/history` 与指数两融不互换：指数日序列必须 `CWServ.skef10_dpzx_rzrq Params=["002", code]`；投资者统计为 `["003", window]`，window=1m/3m/6m/1y；`["001", code]` 是活跃个股列表。不得用 `sh000001` 去个股 margin_history 后把深市同号股当上证指数。

MG条件分支明确如下：capital section002发送[002,us,page]，其他分支固定[001,us,1]；shareholders section006固定[006,us,1,10]，其他分支按给定section/page/page_size；market section003固定[003,us,0,0]，其他分支[001,us,page,page_size]。不使全部分支看起来都支持分页。hy_summary registry=hqtj/hyzq/hygy/zszfb，各发同名 `skef10_hy_<section>` Entry及[hy]；hy_members_rank五类Entry参数见逐入口表。qh_position_report registry=cjcc/cjjg/jcgc/ccjj/ykfx，index选择gz或sp前缀，cjcc=[1,code,date]、cjjg=[1,code,date,empty]、其他=[1,code,date,end_date,empty]。qh_variety_info registry=pzjj/hyjj/xgzq，pzjj多一个字符串1；qh_index_info已知zsgk/lsjg，qh_index_valuation已知dtpe，qh_weights已知zxcfg/lscfg/cfgqz/cfghyfb/xwpl（cfghyfb不发date）；未证实子标签仅通过显式wire escape。开放section保持可透传不等于已验证业务含义。

`theme_market` 为 `HQServ.hq_nlp_tcihq`，body 是单元素数组，ReqId 默认 200743，setcode 来自显式身份，Page 为数字，PageSize 为字符串，modname=mod_tcihq.dll；200740/200742/200745/200747 才发 zq_num。四个 stock_* helper 对应这四个 ReqId。`stock_board_member_quotes` 固定 200744。

`valuation` 为 `HQServ.hq_nlp_gpsj`：200191/200192 使用 `Code="code|market"`、BeginDate/EndDate="0"、字符串 Page；200192 旧默认 Page=0 自动改 -1，PageSize=20 自动改5，Type 默认0。200124 使用 code/setcode、sjfw=1Y、zb=0、hqzq=0、cwppgz=1、jglx=0、默认 Page1/PageSize300；200135 为 code/setcode 年度趋势，不发上述分页组。typed wrapper 直接构造最终明确值；通用 `valuation` 保留这些 ReqId 条件默认。

`index_members` 与 tracking：页大小1..100（Rust越界报错；Python max/min 钳制），页码从1起；短页、已达 server total 或 all_pages=false 终止，保留服务排序与所有重复。`board_members` 实际是分红记录：key=(code, dividend_plan, announce_date, record_date, ex_date, report_period)，按此去重保留首次出现顺序；同 code 的中报与年报不能合并。推进 page 而非唯一行数；空页、短页、达到总记录数或无新增记录终止。Rust 的无新增但未达总数返回显式 `Incomplete` 诊断；收集完整要求的入口返回 `LimitExceeded`/完整性错误而非声称全部。Python 按原代码提前返回列表。达到 max_pages 且不能证明完整：Rust 严格 collect 报 `LimitExceeded`，Python 原停止规则保持。

Rust分页总数只读取第一表之后明确名为`zs`的非负整数列，不把其他附加表数字当总数；同一响应多个有效`zs`冲突则该页总数未知并给诊断。Python原`_result_total`的宽松首个整数扫描由兼容层保留。三类typed集合提供`page_responses()`：各页第一表行已移动到`rows()`，列定义、附加表、业务码和顶层属性仍完整保留；即使RawPolicy::Omit也不会丢掉这些结构。首次业务码非零立即停止分页，保留该页及此前行，`ok=false`、`error_code=该码`、`completion=BusinessError`，不以短页或total将其标为完整。网络/协议错误和资源限额仍返回Err；正常停止为EndOfData或RequestedPage。

普通 F10 endpoint 只返回指定页，不能全域自动分页。排序为服务端顺序；SDK 不通过本地重排使分页看似稳定。Ladder 日期全省略取调用时本地交易日期语义的 today（记录 clock 来源），只给一端复制到另一端，start>end 拒绝；include_summary 才第二次请求 `Params=["2", start, end]`，detail 为 `"1"`。两次调用共享 Rust 总期限；Python 每次独立期限及错误路径不变。

Rust本地日期使用 `LocalDateClock::today() -> Result<TradingDate>`；默认 `SystemLocalDateClock` 读取调用时操作系统本机civil date，`F10ClientBuilder::with_local_date_clock(Arc<dyn LocalDateClock>)` 可注入确定来源。build与显式日期查询均不调用clock；全省日期每次公开操作仅调用一次。`LadderDateSource` 区分显式区间、复制start、复制end与本机clock，不能把HQ固定Shanghai日期无说明地替代本机日期。晋级率超1时保留原counts并给诊断，率省略，不静默裁剪。

### 2.4 typed 结果字段、单位和无值

| 结果 | 字段（均保留原列位置/原值） | 单位、缺失、证据 |
| --- | --- | --- |
| `LimitBoardLadder` | entry/request/error_code、start/end、rows、summary、meta；row 的 trading_date/value、board_level、code/market、reason/extra、seal_amount、name、limit_time、broken_count、industry、limit_type、success_rate、highest_board_level | 数字/日期无法解析为 `None` 并诊断；status 1 up、3 broken、0 down、其他 Unknown；仅 up 且正 board_level 参与阶梯计数；范围统计按天分组，晋级率只分母>0 时产生 0..1 ratio；seal_amount/summary amount 未确认单位保留 native，不误标元 |
| `IndexMemberTable` | index_code、server total Option、rows；row code/name/market/join_date | 无权重字段；market源值+可选推断身份分开。join_date无法证实时原JsonValue |
| `IndexTrackingTable` | index_code、total、rows；code/name/market、scale_yuan、change_pct、amount_yuan、inception_date、manager、company、nav、fee_rate_pct | scale/amount 为元，pct 是百分点，nav 是每份净值；N004/N007 Unknown；ETF 前缀筛选只是 heuristic，不是证券分类事实 |
| `BoardMemberTable` | board_code、total、rows；code/name/market、dividend_plan/progress、announce_date/record_date/ex_date/report_period | total是记录数；三个日期解释为 Inferred，保留源值；stocks/full_codes 另行按首次顺序去重 |
| `IpoCalendar` | rows；apply_date、code/name/market、issue_pe、extra_pct、issue_price、apply_limit、raise_amount | N006 issue_price元、N008 raise_amount元；N004 PE、N007申购上限为推断（上限疑似万股），N005意义未知不能强制Percent；兼容字段名 extra_pct 仅保留历史命名并标Unknown |
| 通用 `Response` | 所有表、列、任意JSON数组/对象/字符串/数值 | 字段字典命中才给单位和别名；scope不同独立。空值不视为0；非零业务码保留数据和 `ok=false` |

四类市场行继续提供原始日期getter，另有`parsed_join_date/parsed_inception_date/parsed_announce_date/parsed_record_date/parsed_ex_date/parsed_report_period/parsed_apply_date`，接受8位ASCII日期或`YYYY-MM-DD`/`YYYY/MM/DD`且检查Gregorian日期；空值为None，非法非空值为None并诊断。数字getter只给有限f64视图，不替换raw里的原始数值和lexeme。`field_info(原始字段名)`返回`MarketFieldInfo`，分别以`MarketFieldEvidence::{SourceMapping,Inferred,Unknown}`和`MarketFieldUnit::{Yuan,PercentagePoints,NetAssetValuePerUnit,Unknown}`表达证据和单位；SourceMapping仅指本仓库既定映射，不宣称本次已联网核验。board三个推断日期、IPO的N004/N007明确为Inferred；IPO N005、tracking N004/N007及未知列均为Unknown。

集合的`count()`为实际保留行数；IndexMemberTable的codes/full_codes保留行顺序和重复，IndexTrackingTable的etfs仅前缀粗筛；BoardMemberTable的stocks为首次出现的行引用，codes为首次出现的代码，full_codes为首次出现的已解析完整代码，记录行本身仍按完整分红身份去重。`full_code()`仅在原市场或可解释的代码推断得到身份时产生，原始market单独保留，不假称推断身份是服务端事实。IpoCalendar提供response/ok/error_code，保留附加表及属性；第一表行移动到rows。raise_amount_yi显式按已知元值除1e8，不改变原值。

F10 模块辅助映射：`parse_tqlex_response` → `f10::parse_response(entry: &EntryName, request: &RequestPayload, json: JsonValue, raw: RawPolicy) -> Result<Response>`；`extract_news_rows` → `f10::news::extract_rows(&Response) -> Vec<&Row>`；`channel_name(NewsChannel)->Option<&'static str>`；`home_tab_name(NewsHomeTab)->Option<&'static str>`。保留8个首页tab与84个频道 registry、频道分组和标签，不在本次规格声称服务端仍覆盖每个频道。news第二结果集优先的事实由 fixture固定。

### 2.5 历史资金流独立服务

`F10Client::money_flow_history() -> MoneyFlowHistoryService` 为不联网的owned getter，service持有同一F10 owner。`history(&MoneyFlowHistoryQuery, &OperationOptions)` 和同语义别名 `range(...)` 返回 `FundFlowRangeSeries`。这些类型从 `eltdx::f10` 和 `eltdx::services` 导出，均只在 `f10` feature 下可用；默认HQ `TdxClient::money_flow()`只提供既有0FFC日资金流，不添加HTTP依赖。

Query由明确的`AshareCode`构造；`with_days(u32)`只接受3/5/10/20/60（默认60，getter返回`HistoryDays`），`with_start(usize)`默认0，`with_count(Option<NonZeroU32>)`默认None且最大800。一次合法调用仅POST `CWServ.tdxf10_gg_sj`，Params全部字符串：`["zjlx_sj",裸代码,str(days),裸代码]`。不发HQ请求、不因空结果切换来源。输入错误在构造query时返回InvalidArgument，零请求。

结果先反转第一表原始记录序列，再截取start/count，严禁额外排序；源乱序时同样仅reverse（COMP-050）。`source_row_count`保存截取前行数，源响应第一表的行移动到`rows()`，列定义和附加表仍在`response()`。保留顶层业务码及属性；HTTP成功而业务码非零仍是可检查的结果。`meta()`保留真实HTTP来源/raw，不用重新序列化JSON冒充原字节。

行保留全部`Row` cells；N001/N009分别给`trade_date/listing_date`，接受8位ASCII日期或`YYYY-MM-DD`/`YYYY/MM/DD`并验证Gregorian日期；非法原值保留并诊断，投影为None。N002/N003给原字符串code/name，N004–N007分别为主力、超大单、大单、主买净额的显式有限f64视图，未知/溢出值投影None并保留原JSON数字lexeme。金额单位为Unknown，不从字段中文标签推断元。Python的弱工厂/批量executor、宽松参数及重序列化`raw_payload`语义由T23兼容桥保留，不把新Rust校验加到旧入口。

## 3. EX：7727 扩展行情

来源：[高层入口](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/ex/client.py#L362)、[连接与握手](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/ex/transport/sync.py#L25)、[帧](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/ex/frame.py#L16)、[时间](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/ex/datetime_.py#L8)、[配置](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/ex/hosts.py#L77)。

### 3.1 请求、握手和错误

EX 默认端口7727；普通 EX 先发送固定92B setup，完整消费16B响应头及声明body才发布 Ready。MAC-EX 模式不用该 setup，见第5节。每个 EX socket 完整 request/response 在同一串行门内；重试前退休坏 socket，不能让旧响应被下个请求读到。响应头 `<IIIHH>` 16B：magic、sequence、method、compressed_len、uncompressed_len；压缩长度/解压长度均受65535 wire上限及更小本地限制，zlib必须完整结束且解压精确匹配声明，禁止跨帧吃字节。当前 Python 不完整校验响应关联，Rust须校验 profile允许的 magic/method/sequence；异步乱序不得靠仅消费下一帧解决。

EX `get_history_instrument_bars_range` 序号是1..255回绕，Rust每会话原子分配并与在途隔离；不能复制 Python 类级计数器引入跨 client 竞争。只对传输失败实施配置的重试；业务解析错误不换服务器碰运气。当前Python优先显式host、失败排除后重探测的选择顺序继续保持；新Rust discovery明确执行。`load_ex_hosts` 旧优先级 explicit > ELTDX_EX_HOSTS > data/ex_servers.json > defaults，坏显式/用户文件报错不静默回默认；Rust builder不自动读取这些，显式 `ex::config::load_hosts(&HostConfigSource)->Result<Vec<Endpoint>>` 才执行同优先级。`ex_server_file_path`→`config::server_file_path`；`save_ex_hosts`→`config::save_hosts(path, endpoints)` 显式写；`load_ex_hosts`覆盖上述源，默认/known/probe_verified主机常量是候选快照，不是当前健康保证。

### 3.2 所有业务签名

下表 `Q{...}` 定义对应 PascalCaseQuery，`ExClient::method(&Q, &OperationOptions) -> Result<R>`；无 Q 时仅 options。offset用u32；wire count均 `NonZeroU16`，总目标可u32并显式分页。任何usize到wire转换必须checked；不能依赖pack截断。代码非空且实际wire编码<=9B，不把长代码静默截断。

| Python入口 | Rust入口/返回 | query字段默认、wire边界 |
| --- | --- | --- |
| get_markets | `markets() -> Vec<MarketInfo>` | 无参数；返回服务顺序 |
| get_instrument_count | `instrument_count() -> u32` | 无参数；是全目录计数，不推定某市场数量 |
| get_instrument_info | `instruments(&InstrumentsQuery) -> PageResult<InstrumentInfo>` | start:u32 必填，count:NonZeroU16=100；64B记录；单页 |
| get_instrument_quote | `quote(&QuoteQuery) -> Option<Quote>` | id:ExInstrumentId；确认缺失为None，坏帧不None |
| get_instrument_quote_list | `quote_list(&QuoteListQuery) -> PageResult<QuoteListRow>` | market:ExMarketId，category:QuoteCategory={HongKong=2,Futures=3}，start:u16=0，count:NonZeroU16=80 |
| get_instrument_bars | `bars(&BarsQuery) -> PageResult<Bar>` | id、period:ExPeriod(u16)、start:u32=0、count:NonZeroU16=700、bar_time:BarTime=Start；period已知0..11/支持码registry，未知显式WirePeriod |
| get_history_instrument_bars_range | `bars_range(&BarsRangeQuery) -> Vec<Bar>` | id、start_date/end_date:TradingDate必填且升序、bar_time=Start；无可靠period元数据，End输入InvalidArgument |
| get_minute_time_data | `minutes(&MinutesQuery) -> Vec<Minute>` | id；当前交易日不加假日期 |
| get_history_minute_time_data | `history_minutes(&HistoryMinutesQuery) -> Vec<Minute>` | id、date:TradingDate |
| get_transaction_data | `transactions(&TransactionsQuery) -> PageResult<Transaction>` | id、start:u32=0、count:NonZeroU16=1800，start<=i32::MAX（wire i32） |
| get_history_transaction_data | `history_transactions(&HistoryTransactionsQuery) -> PageResult<Transaction>` | id、date:TradingDate、start:u32=0、count:NonZeroU16=1800，start<=i32::MAX；与当日不同wire布局单独fixture |

普通 EX 页不隐含拉全、不排序、不按code去重。目录固定64B、行情slot固定10B identity+290B payload：即使目前只知道前140B，也保留剩余150B及identity，不丢尾部。Rust声明N条但payload不足报Protocol；Python各方法原来返回已完整解析前缀的行为保留。请求count不等于真正返回数，不用“短页必EOF”给所有EX命令统一下结论。

### 3.3 返回字段

| 模型 | 字段 | 数值/时间语义 |
| --- | --- | --- |
| MarketInfo | market:u8、category:u8、name、short_name、record_raw | 所有值含未知市场码均保留；不转HqMarket |
| InstrumentInfo | category:u8、market:u8、code、name、desc、record_raw | GBK replace解码记录诊断；原字节可查，当前64B行后24B仍保留 |
| Quote | id、pre_close/open/high/low/price:f32，kaicang/zongliang/xianliang/neipan/waipan/chicang，bid/ask[5]各price+volume | 价格为native报价单位，数量整型不擅自乘100；字段命名保留中文wire语义；盘口缺档不补假0，wire0依旧0 |
| QuoteListRow | id、category、`Vec<NamedNativeField>`、known HK/Futures view、unused_tail | Futures含BiShu/ZuoJie/JinKai/ZuiGao/ZuiDi/MaiChu/KaiCang/ZongLiang/XianLiang/ZongJinE/NeiPan/WaiPan/ChiCangLiang/MaiRuJia/MaiRuLiang/MaiChuJia/MaiChuLiang；HK按独立字段表，不能共用Futures含义 |
| Bar | time、OHLC:f32、position_bits:u32、position:u32、amount_view:f32、trade:u32、settlement:Option<f32>、record_raw | 普通bars position与amount共享同一4B槽，不当两个独立经济指标；range 的末f32解释settlement并标来源。保留f32 bits、单位Unknown至市场证据齐备 |
| Minute | time(HH:MM)、price/avg_price:f32、volume/open_interest:u32、record_raw | time无日期则不填系统日期；扩展盘中交易时段不强套A股240分钟 |
| Transaction | time(HH:MM:SS)、price_raw:u32、volume、zengcang、nature、record_raw | 未知nature保留数字；原整数价格scale按市场证据，通用EX不默认/1000。MacEx HK映射另见5节 |

压缩日期 `(year-2004)<<11 | month*100+day`、盘中分钟数按源规则；日线常见15:00是协议构造时间并标 `SyntheticClose`，不是服务端逐秒事件。End只对已知分钟周期把Start加时长；不改变日/周/月的日期。非法日历/分钟字段Rust报Protocol并带原值；Python已有解析边界不改。

## 4. MAC：报价位图、行情、板块与资源

来源：[业务](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/mac/client.py#L291)、[命令](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/mac/commands.py#L73)、[位图](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/mac/bitmap.py#L427)、[枚举](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/mac/enums.py#L1)、[模型](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/mac/models.py#L1)。

### 4.1 协议和字段语义

MAC请求头10B `<BIBHH>`，响应16B；独立profile完整读取响应，长度/zlib限制与关联校验同EX要求。标准MAC为三阶段setup，当前Python容许部分setup缺响应；Rust将这定义为显式profile的 `OptionalReply`，只在已验证边界消费，不能把超时之后到达的setup响应当业务响应。每个socket串行门是必须项，当前Python无锁不能成为Rust数据竞争先例。连接换主机仅新连接Ready后替换旧连接；普通Python重试一次下一host的规则保留。

```rust,ignore
pub struct FieldId(u8); // 0..150；未知合法位仍可表示
pub struct FieldSelection { /* 去重bitset，输出按bit升序 */ }
pub enum SlotValue { F32(WireFloat), U32(u32), I32(i32), Unknown([u8;4]) }
pub struct QuoteField { pub bit: FieldId, pub name: Option<&'static str>,
    pub value: SlotValue, pub derived_text: Option<String>, pub meaning: FieldMeaning }
pub struct Quote { pub id: MacInstrumentId, pub name: String,
    pub fields: Vec<QuoteField>, pub reserved_bitmap: [u8;20], pub meta: ExtensionMeta }
```

0x122B的20B位图有合法field bit0..150；**任何置位的未知合法bit都占4B slot**。按bit升序计算 `slot_index`，行宽=68+4*合法置位总数，不能只数认识的FieldBit。151..159是reserved，不占slot；记录保留reserved位的观测值但不增加行宽。已知类型按字段表解f32/u32/i32与明确postprocess；缺请求字段与值0分别表达。`fields`不能仅按name做map：同义别名及未知位保真由bit定位，legacy dict投影单独生成。

0x122C只16B位图，末4B是exclude_flags；bit128..150必须在构造时拒绝，不截断也不挤入控制区。FilterType是mask(1,2,4,8,16,32,64,128)，OR合成；重复字段只发一次，未知合法位可用FieldId显式选择。FieldSelection::Common/BASIC/VOLUME/HANDICAP/BOARD_STATS/ALL等preset成员逐字节固定，`DEBUG`全1只进advanced，不作为默认。

Period: 5m=0、15m=1、30m=2、60m=3、Daily=4、Weekly=5、Monthly=6、1m=7、Mins=8、Days=9、Quarterly=10、Yearly=11、Seconds=13；Adjust None=0/Qfq=1/Hfq=2。Mins/Days/Seconds times>0，其他times默认1；秒周期保留秒，不截成分钟。SymbolBar 0x122E必须四个标志byte，record36B `<II7f>`，time为午夜以来秒数。BarTime End只改已知持续时间的盘中bar。

### 4.2 所有业务入口

统一为 `MacClient::method(&PascalCaseQuery, &OperationOptions)->Result<R>`（query名取新Rust方法名，如QuoteListQuery、BarsQuery、FileMetaQuery）；下表无Q则仅options。count总目标u32>0、start/offset u32，wire分块checked。默认保留当前值但不把总目标误限成u16单页大小。23项加3个兼容别名全部覆盖。

| Python入口 | Rust入口/返回R | query字段与默认 |
| --- | --- | --- |
| get_stock_quotes | `quotes -> Vec<Quote>` | ids:Vec<MacInstrumentId> 1..80；fields:FieldSelection=Common；空输入InvalidArgument |
| get_stock_quotes_list | `quote_list -> PageResult<Quote>` | category:Category必填；start=0,count=80,sort=ChangePct,order=Desc,exclude:FilterMask=0,fields=Basic+Volume |
| get_stock_kline | `bars -> Vec<Bar>` | id；period=Daily,start=0,count=800,times=1,adjust=None,bar_time=Start |
| get_tick_chart | `tick_chart -> TickChart` | id、date:Option<TradingDate>=None(当前) |
| get_tick_charts | `tick_charts -> MultiTickChart` | id、date=None、days:u8=5，必须1..5 |
| get_chart_sampling | `chart_sampling -> Vec<WireFloat>` | id；服务端采样顺序，不能保证固定240点 |
| get_transactions | `transactions -> PageResult<Transaction>` | id、count=2000,start=0,date=None；内部每页<=1000 |
| get_symbol_info | `symbol_info -> SymbolInfo` | id |
| get_board_list | `boards -> Vec<BoardInfo>` | board_type:BoardType=All(255)，count=10000 |
| get_board_members | `board_members -> PageResult<Quote>` | board:BoardSymbol,count=100000,sort=ChangePct,order=Desc,fields=Common,exclude=0；每页80 |
| get_belong_board | `belonging_boards -> Vec<BelongBoard>` | id |
| get_board_summary | `board_summary -> BoardSummary` | board、sort=ChangePct、order=Desc |
| get_board_ranking | `board_ranking -> Vec<BoardRank>` | board_type=Hy(0),top_n:NonZeroU32=50,sort:BoardRankKey=ChangePct,ascending=false |
| get_board_change_ranking | `board_change_ranking -> Vec<BoardChangeRank>` | board_type=Hy,target_date=None,days:NonZeroU32=20,top_n:Option<NonZeroU32>=None,ascending=false |
| get_capital_flow | `capital_flow -> Option<CapitalFlow>` | id；合法无对象None，JSON破损报Protocol |
| get_auction | `auction -> Vec<Auction>` | id；原协议请求500，返回服务顺序 |
| get_unusual | `unusual -> PageResult<Unusual>` | market:MacMarketId,start=0,count=600；Python count=0映射600保留在桥接层 |
| get_server_info | `server_info() -> ServerSession` | 无Q |
| get_kline_offset | `kline_offsets -> KlineOffsetInfo` | offset:u32=0,count:u32=128000，wire范围checked |
| get_file_meta | `file_meta -> FileMeta` | filename:RemoteFileName；非空GBK<=70B，超长拒绝而非旧Python截断 |
| download_file_chunk | `download_chunk -> Vec<u8>` | filename、index:u32>=1、offset:u32、size:NonZeroU32<=30000；精确响应长度<=请求size |
| download_file | `download_file -> DownloadResult` | filename、expected_size:Option<u64>=None、max_bytes=显式DownloadOptions默认上限；另`download_complete`仅Complete交bytes |
| get_goods_list | `goods -> PageResult<GoodsItem>` | market:MacMarketId,start=0,count=600，count<=1000；不推定category就是market |
| get_stock_minute_time | `tick_chart`（无第二实现） | 参数与get_tick_chart一致 |
| get_stock_minute_time_hist | `tick_charts`（无第二实现） | 参数与get_tick_charts一致 |
| get_stock_transaction | `transactions`（无第二实现） | 参数与get_transactions一致 |

Category定义独立：SH=0、SZ=2、A=6、B=7、KCB=8、BJ=12、CYB=14；板块10000..10010及HGT/SGT/ETF/LOF/ZS等110xx按源码enum值；它不是MacMarketId。BoardType Hy0/Hy2=1/Gn3/Fg4/Dq5/Other6/YjLevel1=7/2=8/3=9/All255。SortOrder None0/Desc1/Asc2；SortType是明确wire code enum，不能与FieldId强转。`BoardSymbol`保留原字符串及编码值，US0401→30401、HK0283→20283、000686→31686、399372→30372、899050→32050、880686→20686，checked运算；字母前缀大小写按旧规则并提供明确invalid错误。

### 4.3 分页、复权、排序与完整性

quote_list/board_members每页最多80、transactions1000、bars700，末页按剩余目标数请求，按实际条数推进；MAC这些入口空页或短页结束，维持服务端顺序。bar单页为时间正序、分页从最新偏移向旧取，合并时前插，所以结果oldest→newest；行情列表不前插。重复输入/返回保留，不能按证券代码去重行情发生记录。

boards的配对计数是本页board/leader记录数，total_raw没有全局总量证据；原实现首个短页后仍可继续，因此完整扫描只接受空页结束证据。unusual原实现只有一次请求，没有可复用的空/短页EOF规则；Rust同样只取一页，大小为min(目标、PageOptions.page_size/max_records、600)，按实际行数给next_offset。目标满足为RequestedCount，否则Unknown；即使空页也不宣称完整异动日志。其PageOptions只限制这一页，不授权隐式全量扫描。

QFQ出现非正或非有限OHLC时，原算法重取None bars，再可选XDXR本地重算；category1事件以每股换算，排序事件并跳过不合法因子。没有provider/没有事件/重算仍坏时，Python返回原服务器QFQ。Rust保留原结果但附 `AdjustmentFallbackUnavailable` 或 `AdjustmentStillInvalid`，可选strict_adjustment令其报错；重取失败按原Python异常传播，Rust更不能吞取消/期限。XDXR provider通过明确依赖注入，不把HQ cache视为MAC身份元数据。已证实数据可提供本地 `compute_forward_factor`、`apply_forward_adjust`、`has_bad_prices`，不发隐含网络。

显式Rust注入接口为`Arc<dyn XdxrProvider>`，MAC/MAC-EX的config与builder提供`with_xdxr_provider`/`without_xdxr_provider`；纯build不调用provider。`events(&MacInstrumentId,&XdxrContext)->Result<XdxrBatch>`仅在非空异常QFQ重取出非空None bars后调用，正常QFQ、None/Hfq及合法空集不触发。`XdxrBatch`保留事件及调用者可选来源meta，默认stamp仅标注injected来源；不能将其宣传为已验证HQ/服务器来源。提供者错误原样传播，不降级为无事件。

`XdxrContext`暴露父deadline、remaining、check_boundary、raw_policy、剩余事件容量提示和record_bytes，不包含新的owner或独立期限。阻塞回调在原owner注册worker内运行；提供者须在循环检查边界并把自身I/O限制在remaining内。访问外部源时以`source_request()`建立逻辑请求，每次connect/send前调用`XdxrSourceRequest::start_attempt()`；每次实际开始消耗父extra额度，并受原wire_attempt_limit约束。内存事件不消耗物理请求次数。提供者不得同步重入同一MAC owner；不合作阻塞回调无法强杀，清理未确认返回CancelUnconfirmed且保留串行门至worker实际退出。事件Vec的capacity和meta保留内存也计入同一字节预算，返回raw范围须合法且服从操作RawPolicy。

异常后的None首个物理请求按Fallback扣一次额外尝试，后续正常页不重复扣，重发仍扣；它不是对QFQ的transport retry。无provider、无事件、None为空或所有事件无可用因子为AdjustmentFallbackUnavailable；运算后OHLC仍坏为AdjustmentStillInvalid。strict模式分别返回MetadataUnavailable或Protocol并带adjustment上下文；取消、期限、坏重取帧、连接和provider错误继续传播。至少一个有效事件实际影响前缀后才可认定本地修复；旧Python对非空但全跳过事件可能采用未复权序列的行为由T27旧策略保留，不能直接套用新Rust严格判定。

修复或降级时，聚合meta保留原QFQ、None及provider全部payload/frame/source顺序，主row_ranges只选择实际返回序列；修复按`AdjustmentResult::source_indices()`匹配稳定排序后的原记录，降级选原QFQ。所有返回行共享这一聚合meta，逐行record_raw仍指向该行实际采用的源36B记录；逐事件证据共享，原QFQ作为辅助证据不会伪装成修复后的源值。diagnostics.latest_meta也保存最终判定，包括strict失败。正常无需修复的路径继续使用原逐响应meta。

board_summary从全部members聚合amount、vol、main_net_amount、main_net_3d、main_net_5d、up_count、down_count、member_count并保留members；不是直接服务端JSON的别名。0x1218 flag1为板块归属JSON，flag2为资金流JSON，跳过27B协议prefix后GBK解码，不当完整body都是JSON。ranking的money/amount/volume类必须先取全部候选聚合再topN，不能先按涨幅截前N；change_pct可用服务排序预截。days回报需要每块各取历史bar并说明缺历史的数据点，不能空历史当0%。相同sort值保持输入顺序作为稳定tie规则；top_n=None是全候选，受PageOptions保护。

download采用30000B chunk、index从1、offset按实际字节递增；未知expected_size先取meta。短块/空块是该协议的EOF证据；到配置max_bytes但没有EOF为LimitReached并给next_offset。已知size读满仍需已验证EOF证据才能Complete；size未满提前空块不能声称完整，带ExpectedSizeMismatch错误。整次下载固定同一来源并校验meta/响应，途中换host要失败或从0重启显式新操作，不能拼接不同文件。Python原filesize<=0取meta、短块终止且可能返回部分bytearray保持；Rust DownloadResult和strict下载不得反向改旧返回类型。

### 4.4 返回字段与单位

| 模型 | 全部业务字段 | 单位/缺失规则 |
| --- | --- | --- |
| SymbolInfo | id/name、time:Option<EventTime>、activity、pre_close/open/high/low/close、momentum、vol、amount、inside/outside_volume、turnover、avg | time缺失None；native price/volume/amount单位按市场证据；momentum/turnover不能仅因名称强制Ratio |
| Bar | datetime/source_time、OHLC、vol、amount、float_shares、wire_values、adjustment_events | 7个源f32保留bits；OHLC为NativeNumber<BarPrice>，有效本地价格独立f64且不覆盖源bits；datetime秒级；float_shares线数缺失标Absent与wire0分开 |
| Tick / TickChart / MultiTickChart | tick time/price/avg/vol/momentum；图id/name、preclose/OHLC、vol/amount/turnover/avg；多日date/preclose/ticks | 固定最多5日期槽，根据实际tick数分配，短首日不可跨天错位；empty charts成功，坏日期槽Protocol |
| Transaction | time、price、vol、trade_count、bs_flag | aggregate逐笔不是Level2订单；flag0买/1卖/2中性/5盘后，其他Unknown(u32)；交易时刻保留秒 |
| BoardInfo | id/name/price/rise_speed/pre_close、leader id/name/price/rise_speed/pre_close | leader另一个身份；BoardSummary/Rank以这些字段、涨跌幅、汇总amount/volume、成员数及动态extra组成 |
| Auction | time、price、matched、unmatched | 数量有符号与原wire保持；未知单位不按手转股 |
| Unusual | index、id/name、time、desc、value、unusual_type | value是原字符串，不能随意parse成价格；未知type数字保留 |
| CapitalFlow | date、main/small/mid/large各in/out/net | native金额，原JSON缺键默认0是Python行为；Rust Option+Presence区分缺键与0，公开不自造金额 |
| BelongBoard | board_type、market、board_code/name、close、pre_close | 市场与板块type分别存，不强制A股交易所 |
| ServerSession | today、last_trading_day、sessions_1/sessions_2、market_param_1/2 | 两组session保留动态有序字段，未知值不推交易日历；明确日期解析失败保留源字符串及diagnostic |
| KlineOffsetInfo | total、returned | u32计数；returned不自动代表EOF |
| GoodsItem | name、category、u、index、switch、code:Vec<f32>、c1、c2 | code是float占位数组，不能当可靠证券代码；name有时实际代码；保留原bits |
| FileMeta | offset、size、flag、hash | 字节；hash算法未核实，不声称校验完整性 |

模块辅助11项全部有归属：`apply_forward_adjust(&[Bar], &[XdxrEvent])->Result<Vec<Bar>>`、`compute_forward_factor(cum_close:f64, fenhong:f64, peigujia:f64, songzhuangu:f64, peigu:f64)->Result<f64>`、`has_bad_prices(&[Bar])->bool`、`convert_board_code(&BoardSymbol)->Result<u32>`为 `mac::util`；`build_bitmap(&FieldSelection, BitmapLayout)->Result<Vec<u8>>` / `get_active_fields(&[u8])->Result<Vec<KnownField>>` 为 `mac::fields`；`build_mac_request` / `parse_mac_header` / `decompress_mac_body` 为 `advanced::mac` 有界codec；`ping_mac_host` / `ping_mac_all`合并为第1.2节probe API。`MacConnection`不直接稳定公开裸socket，advanced受控会话不得绕过owner/预算。

`BarPrice::wire()/bits()`始终返回源f32及其原始位；`as_f64()`返回本地计算值（若存在）或源值，`adjusted_value()`与`source()`明确区分Wire和LocalForwardAdjustment。未受任何事件影响的行继续Wire，不能因其他行重算就改变其价格来源。`wire_values()`保留全部七个源数；amount/volume/float_shares和raw不随价格修复变化。详细接口`mac::util::forward_adjustment`返回`AdjustmentResult`，每个`AdjustmentEvent`提供原事件、可选因子、实际影响行数及跳过原因；行共享同一事件证据表，每个原响应仅共享一份增加摘要诊断的meta，避免按行复制原始报文或事件表。

纯复权要求输入属于同一MAC身份且有有效source_time日期/时刻；未知period的无日期序列返回InvalidArgument。按源日期/时刻稳定排序，事件按日期稳定排序，同日事件沿用输入次序并以前一事件累计后的收盘价求下一个因子。End显示跨日仍以未移动的源日期决定除权前缀；无效事件保留跳过证据，导致非有限价格的整次事件不生效，有限非正值保留以供has_bad_prices明确识别。此处不调用HQ或网络；其内部parent边界供T26传播取消/期限。

`mac::fields`接受16B零扩展或精确20B位图，具名结果按bit升序，并以`active_fields_in`选择Instrument/Board别名；合法未知bit仍由唯一wire解码器占槽，reserved151..159不占槽，不能用具名列表长度推导响应行宽。高级头解析严格只接受16B；解压重用EX/MAC共同有界frame实现，要求完整单一zlib流、实际/声明长度一致、无尾随流。DEBUG全ff仅高级位图入口可用，Board16只产生16B，不覆盖后续exclude控制字。

本段补足辅助/组合类型：`XdxrEvent{date:TradingDate,category:u8=1,fenhong:f64=0,peigujia:f64=0,songzhuangu:f64=0,peigu:f64=0}`以每股单位，单因子函数无效分母/非有限值在Rust报InvalidArgument，Python仍返回NaN；`BoardSymbol{source:String,wire_code:u32}`；`BitmapLayout::{Quote20,Board16}`；`KnownField{bit:FieldId,name:String,kind:SlotKind}`，SlotKind=F32/U32/I32。`NamedNativeField{name:String,value:NativeValue,meaning:FieldMeaning}`，NativeValue=I64/U64/F32/Text/Bytes；`BoardSummary{vol:NativeNumber<u64>,amount,main_net_amount,main_net_3d,main_net_5d:NativeNumber<f64>,up_count,down_count,member_count:u32,members:Vec<Quote>,extra:Vec<NamedNativeField>,meta}`；`BoardRank{code,name:String,change_pct:Option<f64>,summary:BoardSummary}`；`BoardChangeRank{code,name:String,close_start,close_end:WireFloat,change_pct:f64,meta}`。前收为0时Rust change_pct=None并诊断，Python旧以0参与排序保持。`RemoteFileName`为非空GBK<=70B、不含NUL的源服务器名称，不自行写入本地；`MacroCode`为原目录中非空精确代码或唯一名称匹配结果。任何公开返回类型都不能留成未定义的“任意对象”。

既有 `FieldBit` 的成员、wire值、slot型、别名及全部Preset作为schema数据迁移；[固定字段表](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/mac/bitmap.py#L19) 是版本基线。已知明确单位如AMOUNT元、TOTAL_SHARES/FLOAT_SHARES万股、SERVER_UPDATE_DATE YYYYMMDD、SERVER_UPDATE_TIME HHMMSS、百分比字段百分点保留；LOT_SIZE_INFO/LOT_SIZE/EX_PRICE_PLACEHOLDER等带“未确定/占位”标记的字段继续Unknown。AH_CODE后处理在market0/1补5位、其他补6位，原u32和派生字符串都保留；不能因代码补位就改变该quote的market。完整字段schema生成清单必须由T25 fixture逐member比对，不按字段英文名猜单位。


#### 既有MAC字段schema全集

`FieldId`仍允许此表未出现的0..150位；下表是已命名字段的全部成员，不是完整slot计数集合。单位以Meaning元数据附加，表中原注释仅为源码证据，不自动升级为交易所确认。

| 位 | 名称 | wire类型 | 基线说明 |
| --- | --- | --- | --- |
| 0 | `PRE_CLOSE` | `<f` | 昨收 |
| 1 | `OPEN` | `<f` | 开盘价 |
| 2 | `HIGH` | `<f` | 最高价 |
| 3 | `LOW` | `<f` | 最低价 |
| 4 | `CLOSE` | `<f` | 收盘价 |
| 5 | `VOL` | `<I` | 成交量 |
| 6 | `VOL_RATIO` | `<f` | 量比 |
| 7 | `AMOUNT` | `<f` | 总金额(元) |
| 8 | `INSIDE_VOLUME` | `<I` | 内盘 |
| 9 | `OUTSIDE_VOLUME` | `<I` | 外盘 |
| 10 | `TOTAL_SHARES` | `<f` | 总股数(单位万) |
| 11 | `FLOAT_SHARES` | `<f` | 流通股(单位万) |
| 12 | `EPS` | `<f` | 每股收益 |
| 13 | `NET_ASSETS` | `<f` | 净资产 |
| 14 | `SECURITY_TYPE_PRICE` | `<f` | 证券类型价 |
| 15 | `TOTAL_MARKET_CAP_AB` | `<f` | AB股总市值 |
| 16 | `PE_DYNAMIC` | `<f` | 市盈率(动) |
| 17 | `BID_PRICE` | `<f` | 买一价 |
| 18 | `ASK_PRICE` | `<f` | 卖一价 |
| 19 | `SERVER_UPDATE_DATE` | `<I` | 服务器更新日期 YYYYMMDD |
| 20 | `SERVER_UPDATE_TIME` | `<I` | 服务器更新时间 HHMMSS |
| 21 | `LOT_SIZE_INFO` | `<I` | 未确定 |
| 22 | `BOARD_STRENGTH` | `<f` | 板块强度(涨跌家数差) |
| 23 | `DIVIDEND_YIELD` | `<f` | 每股股息(元) |
| 24 | `BID_VOLUME` | `<I` | 买量 |
| 25 | `ASK_VOLUME` | `<I` | 卖量 |
| 26 | `LAST_VOLUME` | `<I` | 现量 |
| 27 | `TURNOVER` | `<f` | 换手 |
| 28 | `INDUSTRY` | `<I` | 行业分类代码 |
| 29 | `INDUSTRY_CHANGE_UP` | `<f` | 行业涨跌幅 |
| 30 | `STOCK_TAG_FLAGS` | `<I` | 股票标签位图 |
| 31 | `DECIMAL_POINT` | `<I` | 数据精度 |
| 32 | `BUY_PRICE_LIMIT` | `<f` | 涨停价 |
| 33 | `SELL_PRICE_LIMIT` | `<f` | 跌停价 |
| 34 | `PRICE_DECIMAL_INFO` | `<I` | 价格精度标志 |
| 35 | `LOT_SIZE` | `<I` | 所属地区板块/每手股数 |
| 36 | `PRE_IOPV` | `<f` | 昨IOPV |
| 37 | `SPEED_PCT` | `<f` | 涨速 |
| 38 | `AVG_PRICE` | `<f` | 均价 |
| 39 | `IOPV` | `<f` | IOPV |
| 40 | `PE_TTM_VOL_RELATED` | `<f` | 前参考价(美股适用) |
| 41 | `EX_PRICE_PLACEHOLDER` | `<f` | 前金额参考 |
| 42 | `OPERATING_REVENUE` | `<f` | 营业收入(万) |
| 43 | `FLAG_KCB` | `<I` | 科创板标志 |
| 44 | `FLAG_BJ` | `<I` | 北交所标志 |
| 45 | `CIRCULATING_CAPITAL_Z` | `<f` | 流通股本Z（单位：万股） |
| 46 | `AFTER_HOURS_VOLUME` | `<i` | 盘后量 |
| 48 | `PE_TTM` | `<f` | 市盈率TTM |
| 49 | `PE_STATIC` | `<f` | 市盈率静 |
| 55 | `INDEX_METRIC` | `<f` | 指数指标 |
| 56 | `MAIN_NET_AMOUNT` | `<f` | 今日主力净流入 |
| 57 | `BID_ASK_RATIO` | `<f` | 委比 |
| 58 | `NON_INDEX_FLAG` | `<I` | 非指数标志 |
| 59 | `CHANGE_20D_PCT` | `<f` | 20日涨幅% |
| 60 | `YTD_PCT` | `<f` | 年初至今% |
| 62 | `STOCK_CLASS_CODE` | `<I` | 证券子分类码 |
| 63 | `PERCENT_BASE` | `<I` | 百分比基底 |
| 64 | `MTD_PCT` | `<f` | 月初至今% |
| 65 | `CHANGE_1Y_PCT` | `<f` | 一年涨幅% |
| 66 | `PREV_CHANGE_PCT` | `<f` | 昨涨幅% |
| 67 | `CHANGE_3D_PCT` | `<f` | 3日涨幅% |
| 68 | `CHANGE_60D_PCT` | `<f` | 60日涨幅% |
| 69 | `CHANGE_5D_PCT` | `<f` | 5日涨幅% |
| 70 | `CHANGE_10D_PCT` | `<f` | 10日涨幅% |
| 71 | `PREV2_CHANGE_PCT` | `<f` | 前日涨幅% |
| 72 | `BID2_PRICE` | `<f` | 买二价 |
| 73 | `ASK2_PRICE` | `<f` | 卖二价 |
| 74 | `AH_CODE` | `<I` | 对应A/H股code |
| 75 | `UNKNOWN_CODE` | `<I` | 少部分有数据 |
| 87 | `OPEN_AMOUNT` | `<f` | 开盘金额(元) |
| 88 | `ANNUAL_LIMIT_UP_DAYS` | `<i` | 年涨停天数 |
| 89 | `ACTIVITY` | `<I` | 活跃度 |
| 91 | `DIVIDEND_YIELD_RATE` | `<f` | 股息率% |
| 92 | `CONSECUTIVE_UP_DAYS` | `<i` | 连涨天 |
| 93 | `BID2_VOLUME` | `<I` | 买二量(个股) / 涨停数(板块) |
| 93 | `LIMIT_UP_COUNT` | `<I` | 涨停数(板块) |
| 94 | `ASK2_VOLUME` | `<I` | 卖二量(个股) / 跌停数(板块) |
| 94 | `LIMIT_DOWN_COUNT` | `<I` | 跌停数(板块) |
| 95 | `INDUSTRY_SUB` | `<I` | 行业二级分类 |
| 102 | `AUCTION_BUY_LIMIT` | `<f` | 连续竞价买入上限 |
| 103 | `AUCTION_SELL_LIMIT` | `<f` | 连续竞价卖出下限 |
| 104 | `VOL_SPEED_PCT` | `<f` | 量涨速% |
| 105 | `SHORT_TURNOVER_PCT` | `<f` | 短换手% |
| 106 | `AMOUNT_2M` | `<f` | 2分钟金额(元) |
| 107 | `MAIN_NET_AMOUNT_COPY` | `<f` | 今日主力净流入(副本) |
| 108 | `MAIN_NET_RATIO` | `<f` | 主力净比% |
| 109 | `RETAIL_NET_AMOUNT` | `<f` | 散户单增比 |
| 110 | `MAIN_NET_5M_AMOUNT` | `<f` | 5分钟主力净额 |
| 111 | `MAIN_NET_3D_AMOUNT` | `<f` | 近三日主力净额 |
| 112 | `MAIN_NET_5D_AMOUNT` | `<f` | 近五日主力净额 |
| 113 | `MAIN_NET_10D_AMOUNT` | `<f` | 近十日主买金额(待确定) |
| 114 | `MAIN_BUY_NET_AMOUNT` | `<f` | 今日主买净额 |
| 115 | `DDX` | `<f` | DDX |
| 116 | `DDY` | `<f` | DDY |
| 117 | `DDZ` | `<f` | DDZ |
| 118 | `DDF` | `<f` | DDF |
| 119 | `STOCK_FLAG_A` | `<f` | 个股标志位A |
| 120 | `STOCK_FLAG_B` | `<f` | 个股标志位B(副本) |
| 122 | `AUCTION_VOL_RATIO` | `<f` | 竞价昨比 |
| 123 | `PREV_AMOUNT` | `<f` | 昨成交额(元) |
| 125 | `RECENT_INDICATOR` | `<f` | 近日指标提示 |
| 128 | `BID3_PRICE` | `<f` | 买三价 |
| 129 | `BID4_PRICE` | `<f` | 买四价 |
| 130 | `BID5_PRICE` | `<f` | 买五价 |
| 131 | `ASK3_PRICE` | `<f` | 卖三价 |
| 132 | `ASK4_PRICE` | `<f` | 卖四价 |
| 133 | `ASK5_PRICE` | `<f` | 卖五价 |
| 134 | `BID3_VOLUME` | `<I` | 买三量 |
| 135 | `BID4_VOLUME` | `<I` | 买四量 |
| 136 | `BID5_VOLUME` | `<I` | 买五量(个股) / 上涨家数(板块) |
| 136 | `UP_COUNT` | `<I` | 上涨家数(板块) |
| 137 | `ASK3_VOLUME` | `<I` | 卖三量 |
| 138 | `ASK4_VOLUME` | `<I` | 卖四量 |
| 139 | `ASK5_VOLUME` | `<I` | 卖五量(个股) / 下跌家数(板块) |
| 139 | `DOWN_COUNT` | `<I` | 下跌家数(板块) |
| 140 | `BID_ASK_DIFF` | `<i` | 委差 |
| 141 | `CHANGE_UP_TYPE` | `<i` | 封板状态 |
| 142 | `SAFETY_SCORE` | `<f` | 安全分 |
| 143 | `HIGHLIGHT_COUNT` | `<f` | 亮点数 |
| 144 | `CHANGE_AT_1000` | `<f` | 日内涨幅% 10:00 |
| 145 | `CHANGE_AT_1030` | `<f` | 日内涨幅% 10:30 |
| 146 | `CHANGE_AT_1100` | `<f` | 日内涨幅% 11:00 |
| 147 | `CHANGE_AT_1130` | `<f` | 日内涨幅% 11:30 |
| 148 | `CHANGE_AT_1330` | `<f` | 日内涨幅% 13:30 |
| 149 | `CHANGE_AT_1400` | `<f` | 日内涨幅% 14:00 |
| 150 | `CHANGE_AT_1430` | `<f` | 日内涨幅% 14:30 |

## 5. MAC-EX：商品、宏观与港股路由

来源：[MAC-EX客户端](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/ex/mac_client.py#L129)、[命令单一来源](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/ex/mac_commands.py#L1)、[港股路由](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/ex/_hk_transaction.py#L17)。

本节Query名取Rust方法PascalCase+Query；所需id使用ExInstrumentId，进入共用MAC command时仅显式转换可表示的market/code，不改身份。

MAC-EX是独立profile/owner：每个新连接做MAC-EX login，成功才Ready；当前Python未严格检查login布尔结果需在fixture说明，不能据此声称已认证成功。复用EX连接的包改写仅 `packet[0]==0x1c` 时改首byte为0x01，不扫描或替换body；普通EX包保持。新Rust login否定/非法响应是Protocol/Handshake错误，旧Python行为不变。MAC命令、模型、四flag K线布局仅一份共享，不复制 `mac_commands` 引入布局漂移。市场使用ExMarketId/独立wire转换，宏观38、港股31等不会转成HQ0/1/2。

`Handshake`是错误阶段，ErrorKind仍为Connection或Protocol。所有批量id遵守最多65535个occurrence及SDK自有规范化输入副本的2MiB逻辑字节上限：HQ按其定长身份实际布局计数，EX/MAC按固定市场字段字节数+规范化code字节长度+长度字段checked累加；该上限只计SDK复制的身份数据，不是Rust堆内存测量，也不含调用者拥有的字符串。wire单页1..80限制与全操作65535限制分开，超过80的明确collect方法可分批；单页quotes表述1..80不隐含拒绝collect。Python不增加这些新上限。

| Python入口 | `MacExClient::method(&Query, &OperationOptions)->Result<R>` | query默认与边界 |
| --- | --- | --- |
| macro_indicators | `macro_indicators() -> Vec<GoodsItem>` | market固定38，缓存策略显式；无query |
| macro_quotes | `macro_quotes -> Vec<mac::Quote>` | codes:Vec<MacroCode>非空；fields默认MAC-EX七字段；支持单个字符串的Python形态由绑定转Vec |
| macro_kline | `macro_bars -> Vec<mac::Bar>` | code:MacroCode,period=Monthly,start=0,count=800,adjust=None |
| goods_count | `goods_count -> u32` | market:Option<ExMarketId>=None；None全目录 |
| goods_list | `goods -> PageResult<GoodsItem>` | market,start=0,count=600 |
| instruments | `instruments -> PageResult<ex::InstrumentInfo>` | market,start=0,count=600；EX目录页按市场切片 |
| goods_quotes | `quotes -> Vec<mac::Quote>` | ids:Vec<ExInstrumentId>1..80、fields七字段 |
| goods_quotes_list | `quote_list -> PageResult<mac::Quote>` | market,start=0,count=100；SourceOrder为默认，实际最多80；另显式sort只能支持已实现的本地完整排序，不能承诺远端排序 |
| goods_kline | `bars -> Vec<mac::Bar>` | id、period=Daily,start=0,count=800,adjust=None；当前一次wire与MAC自动700分页不是同义，Rust需明确collect分块策略 |
| goods_tick_chart | `tick_chart -> mac::TickChart` | id、date:Option<TradingDate>=None |
| goods_chart_sampling | `chart_sampling -> Vec<WireFloat>` | id |
| goods_transaction | `transactions -> PageResult<mac::Transaction>` | id、date=None,start=0,count=2000 |
| goods_transaction_all | `all_transactions -> Vec<mac::Transaction>` | id、date=None、pages:PageOptions；仅27/31/48/49/71/98接受；默认安全页上限50且未完报LimitExceeded |

默认七字段严格为PRE_CLOSE/OPEN/HIGH/LOW/CLOSE/VOL/AMOUNT，不能换MAC的34项Common。宏观名称保留完整 `1_GDP`；bare `GDP`只在缓存内唯一匹配时允许，歧义报InvalidArgument，未知报MissingData；大小写敏感。目录market38、名称映射cache键包含endpoint/profile/session generation、market和schema版本；Python缓存无TTL的现状不改。Rust可显式刷新/失效，换源不得把同名记录假定相同数据。

GoodsList0x2562不可用时可明确诊断后回退EX count+全局目录二分查market边界并缓存。二分依赖目录按market排序；Rust必须验证边界/代表行，假设不成立时在预算内改扫描或报Protocol，不能漏行。回退GoodsItem.name存实际code、code浮点占位只在可解析时填，category/u/index/switch/c1/c2按旧转换并标derived。新cache按源隔离，Python旧无TTL路径保留。`instruments`保留原EXInfo，不经GoodsItem丢desc。

`goods_quotes_list`的Python sort_type=Code/sort_order=None参数目前被忽略，count100仅取min(count,80)商品再报价；Rust默认SourceOrder+响应有效数量明确表示，不宣称按请求sort排序或全100条。兼容桥接继续忽略旧sort参数；新严格排序需求必须另显式 `collect_sorted_quotes` 拉足目标后按已知字段排序，并共享预算。

HK股票类market={27,31,48,49,71,98}逐笔走EX当日0x23FC/历史0x2406，价格原u32除1000为港币；time保留秒、vol原值、trade_count=0且标 `NotProvided`、bs_flag=nature。23..26港股期货/期权不是这条路，走MAC0x122F。HK每页1800，普通count目标按剩余数分页；all最多50页共90000，不等于永远拿全一天，Rust到上限仍满页报LimitExceeded。Python到50页返回已有列表的行为保留。顺序是协议最新偏移顺序，不本地反转成时间正序。

## 6. offline：本地读取、编码与追加

来源：[导出面](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/offline/__init__.py#L14)、[日线](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/offline/daily_bar.py#L1)、[分钟](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/offline/min_bar.py#L1)、[写日线](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/offline/write_daily.py#L1)、[写分钟](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/offline/write_min_bar.py#L1)、[行业](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/offline/industry.py#L1)、[板块](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/offline/block.py#L1)、[GBBQ](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/offline/gbbq.py#L288)。

### 6.1 本地选项与完整函数表

```rust,ignore
pub struct ReadOptions { pub tail: TailPolicy, pub raw: RawPolicy,
    pub max_bytes: u64, pub cancel: Option<CancellationToken> }
pub enum TailPolicy { IgnoreWithDiagnostic, Reject }
pub struct AppendOptions { pub durability: Durability,
    pub create_parents: bool, pub cancel: Option<CancellationToken> }
pub enum Durability { FormatDefault, Flush, SyncAll }
pub struct Coefficients { pub price: f64, pub volume: f64 }
pub struct ReadResult<T> { pub rows: Vec<T>, pub ignored_tail_bytes: usize,
    pub diagnostics: Vec<Diagnostic> }
pub struct WriteReport { pub accepted: usize, pub skipped: usize,
    pub bytes_written: u64, pub final_key: Option<RecordKey>,
    pub durability: Durability }
```

默认ReadOptions max_bytes=256MiB（新Rust可配置正有限值，非Python新增限额）、raw=Omit、cancel=None、tail=IgnoreWithDiagnostic，保留现有bar读残尾行为。局部取消在记录边界检查；本地调用不创建网络runtime、不用8秒网络operation。编码先校验整批并检查溢出；文件写取消/故障不承诺回滚，WriteReport或I/O错误携带已写字节，文件可能含完整已写前缀，不能称原子事务。AppendOptions默认durability=FormatDefault、create_parents=false、cancel=None。现有全部write/append直接open("ab")，不创建父目录；父目录不存在抛FileNotFoundError，Rust对应Io。create_parents=true为显式新能力。

| Python导出 | 拟议Rust路径和完整参数→返回（共同省略 `&` 的类型表示） |
| --- | --- |
| detect_tdx_home | `offline::paths::detect_tdx_home() -> Result<Option<PathBuf>>`，调用才读TDX_HOME及已知安装位置 |
| resolve_vipdoc | `paths::resolve_vipdoc(explicit: Option<&Path>) -> Result<PathBuf>`；显式目录或detect结果/vipdoc |
| market_to_exchange | `paths::market_to_exchange(market: u8) -> Result<Exchange>`；仅0=SZ/1=SH，2不冒充BJ支持 |
| read_daily_bar | `daily::read(path: &Path, coefficients: Option<Coefficients>, options: &ReadOptions) -> Result<ReadResult<DailyBar>>` |
| encode_daily_bar | `daily::encode(bar: &DailyBar, coefficients: Coefficients) -> Result<[u8;32]>` |
| find_daily_bar_file | `paths::find_daily(market: u8, code: &LocalCode, vipdoc: Option<&Path>) -> Result<PathBuf>` |
| detect_security_type | `daily::detect_security_type(filename: &OsStr) -> SecurityType` |
| security_coefficients | `daily::security_coefficients(filename: &OsStr) -> CoefficientDecision`（系数+推断依据/unknown） |
| write_daily_bar | `daily::write(path: &Path, bars: &[DailyBar], coefficients: Coefficients, options: &AppendOptions) -> Result<WriteReport>`，默认系数.01/.01 |
| append_daily_bar | `daily::append(path: &Path, bars: &[DailyBar], coefficients: Coefficients, options: &AppendOptions) -> Result<WriteReport>` |
| get_last_bar_date | `daily::last_date(path: &Path) -> Result<Option<TradingDate>>` |
| read_min_bar | `minute::read_lc(path: &Path, options: &ReadOptions) -> Result<ReadResult<MinBar>>` |
| read_5min_bar | `minute::read_5(path: &Path, options: &ReadOptions) -> Result<ReadResult<MinBar>>` |
| encode_min_bar | `minute::encode_lc(bar: &MinBar) -> Result<[u8;32]>` |
| encode_5min_bar | `minute::encode_5(bar: &MinBar) -> Result<[u8;32]>` |
| find_lc1_bar_file | `paths::find_lc1(market: u8, code: &LocalCode, vipdoc: Option<&Path>) -> Result<PathBuf>` |
| find_lc5_bar_file | `paths::find_lc5(market: u8, code: &LocalCode, vipdoc: Option<&Path>) -> Result<PathBuf>` |
| find_5min_bar_file | `paths::find_5(market: u8, code: &LocalCode, vipdoc: Option<&Path>) -> Result<PathBuf>` |
| write_min_bar | `minute::write_lc(path: &Path, bars: &[MinBar], options: &AppendOptions) -> Result<WriteReport>` |
| append_min_bar | `minute::append_lc(path: &Path, bars: &[MinBar], options: &AppendOptions) -> Result<WriteReport>` |
| write_5min_bar | `minute::write_5(path: &Path, bars: &[MinBar], options: &AppendOptions) -> Result<WriteReport>` |
| append_5min_bar | `minute::append_5(path: &Path, bars: &[MinBar], options: &AppendOptions) -> Result<WriteReport>` |
| get_last_min_bar_datetime | `minute::last_lc_time(path: &Path) -> Result<Option<MinuteKey>>` |
| get_last_5min_bar_datetime | `minute::last_5_time(path: &Path) -> Result<Option<MinuteKey>>` |
| read_ex_daily_bar | `ex_daily::read(path: &Path, options: &ReadOptions) -> Result<ReadResult<ExDailyBar>>` |
| encode_ex_daily_bar | `ex_daily::encode(bar: &ExDailyBar) -> Result<[u8;32]>` |
| write_ex_daily_bar | `ex_daily::write(path: &Path, bars: &[ExDailyBar], options: &AppendOptions) -> Result<WriteReport>` |
| append_ex_daily_bar | `ex_daily::append(path: &Path, bars: &[ExDailyBar], options: &AppendOptions) -> Result<WriteReport>` |
| get_last_ex_bar_date | `ex_daily::last_date(path: &Path) -> Result<Option<TradingDate>>` |
| read_block | `block::read(path: &Path, options: &ReadOptions) -> Result<ReadResult<BlockInfo>>` |
| read_customer_block | `block::read_custom(dir: &Path, options: &ReadOptions) -> Result<ReadResult<CustomerBlock>>` |
| read_industry_cfg | `industry::read(path: &Path, options: &ReadOptions) -> Result<IndustryMap>` |
| parse_tdxhy_cfg | `industry::parse(content: &[u8]) -> Result<IndustryMap>` |
| read_gbbq | `gbbq::read(path: &Path, options: &ReadOptions) -> Result<ReadResult<GbbqRecord>>` |
| read_history_financial | `financial::read(path: &Path, options: &FinancialReadOptions) -> Result<ReadResult<FinancialRecord>>` |
| parse_financial_dat | `financial::parse_dat(data: &[u8], report_date: Option<TradingDate>, options: &FinancialParseOptions) -> Result<ReadResult<FinancialRecord>>` |
| parse_financial_file_list | `financial::parse_file_list(data: &[u8]) -> Result<Vec<FinancialFileInfo>>` |
| ensure_aligned_for_append | `alignment::ensure_aligned(path: &Path, record_size: NonZeroUsize) -> Result<()>`；32B格式调用前统一检查 |

38个函数与8个模型完整覆盖；`write` 名称保留当前入口语义为追加，若要覆盖文件须新增显式 `replace` API，不把历史write改成truncate。所有路径由调用方明确提供才写，无builder/background自动落盘。ReadResult是Rust增强，Python仍原list/dict/int数量等返回形状。

### 6.2 格式、单位和写入规则

| 格式/模型 | 原字节布局与字段 | 语义和缺失 |
| --- | --- | --- |
| `.day` / DailyBar | 32B `<IIIIIfII>`：date、OHLCu32、amountf32、volumeu32、adjust_flagu32；模型date+OHLC/volume/amount、adjust_flag、raw | OHLC乘price系数，volume乘volume系数；amount不乘此系数；reserved/flag不猜用途 |
| `.lc1/.lc5` / MinBar | `<HHfffffII>`：packed_date/minutes、OHLCf32、amountf32、volu32、reservedu32 | packed_date=(year-2004)*2048+month*100+day；minutes午夜起；volume保持现有换算，raw/reserved保真 |
| `.5` / MinBar | `<HHIIIIfII>`：OHLCu32/100、amountf32、volumeu32、reserved | 与.lc5不同布局，不按扩展名近似解析成同一种 |
| EX `.day` / ExDailyBar | `<IffffIIf>`：date、OHLCf32、amount_bits:u32、volumeu32、settlementf32 | amount整数和hk_stock_amount=f32同4B位模式双视图；encode使用原u32，不能把HK浮点数再int转换；不要求开启网络ex |
| BlockInfo | name、category:u8、declared_count:u16、codes:Vec<String> | 384B头+u16计数，2813B每行，最多400个7B code；count保留声明，即使>400；category按文件名gn/fg/dy否则行业 |
| CustomerBlock | blockname、block_type、codes | cfg120B行(50B名字+70B叶文件名)，GBK替换；blk UTF-8 replace逐行去首位market；空/仅1char行跳过，旧行为不保留market |
| IndustryMap | 有序(code:String → {tdx:String,sw:String}) | GBK replace、`&#124;`分列、>=3列且仅len(code)==6，SW第6列缺省空。`abcdef`合法保留；不验证数字，不trim每个字段，不映射HqMarket；重复code后值覆盖 |
| GbbqRecord | market:u8、code、date_raw:u32、category:u8、四f32：hongli_panqianliutong/peigujia_qianzongguben/songgu_qianzongguben/peigu_houzongguben | 4B计数+29B/条，前三8B块解密、末5B附加；u32 wrapping算法原样；字段含义随category，不一律当分红；原bytes/日期和值保留 |
| FinancialFileInfo / Record | filename/hash/filesize；code/market_id/report_date/fields:Vec<WireFloat> | 列表file size字节；财报字段按索引保存，不把索引直接映射成统一金额，见7节 |

日线系数表精确保留：SH_A(.01,.01)、SH_B(.001,.01)、SH_INDEX(.01,1)、SH_FUND(.001,1)、SH_BOND(.001,1)、SZ_A(.01,.01)、SZ_B(.01,.01)、SZ_INDEX(.01,1)、SZ_FUND(.001,.01)、SZ_BOND(.001,.01)。Unknown旧默认(.01,.01)；Rust `CoefficientDecision`标fallback，允许调用方指定系数，不能把它说成证实精度。系数必须有限且>0，编码用Python等价ties-to-even rounding，溢出返回InvalidArgument。price缩放和float bits roundtrip分别测，不用字符串decimal偶然相等替代字节断言。

bar reader只解完整32B记录并保持文件顺序，不保证文件已排序；残尾默认忽略并报告字节数，Reject模式报ResourceFormat。last_key读取向下对齐最后完整32B，不因残尾读错key；四种last_key入口（daily、lc、5、ex_daily）遇缺文件、目录路径或长度<32均返回None；读取已存在文件的I/O错误传播。普通read遇缺文件则FileNotFoundError，Rust对应Io。追加前即使输入为空也先做文件对齐检查，残尾报ResourceFormat且不自动truncate。过滤条件是 `record.key > 原始磁盘last_key`；**批内不排序、不去重，不更新过滤阈值**。例磁盘last=20240101，输入[20240103,20240102,20240103]全部接受且原顺序落盘；最终磁盘last=20240103。若输入最后一条较早，final_key是实际最后条，不是max。

当前所有write_*使用 `ab` 追加；daily原有flush+fsync，minute与ex_daily依赖close、不具有同样fsync保证。Rust FormatDefault保持此差别；Flush/SyncAll由调用方显式加强，报告实际完成等级，SyncAll失败不能报告已持久化。不得因为统一helper而把Python所有格式同步策略或性能偷偷改掉。没有跨进程文件锁和原子追加批保证；共享写由调用者串行，或未来显式locked writer，不能以单次open宣称并发安全。

行业规则按上表保持；另加 `parse_strict_numeric_codes` 才可拒绝abcdef，不能在现有parse路径含糊写“跳过非法code”。block声明记录截断必须错误，不返回看似完整前缀；客户板块文件名拒绝空/. /..、斜杠、反斜杠、冒号，并验证resolve路径仍在目录内，符号链接越界也拒绝；缺blk或空name按旧规则跳过并在Rust诊断。GBBQ头<4B旧[]；声明记录不足旧struct错误，RustResourceFormat并给记录索引；不能用bar残尾规则套GBBQ。

## 7. financial：HQ下载 + 有界ZIP/DAT

来源：[组合下载](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/financial.py#L68)、[ZIP与DAT](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/offline/history_financial.py#L30)。

```rust,ignore
pub struct FinancialService { /* owned HQ resource service */ }
impl FinancialService {
    pub fn list_archive_files(&self, options: &OperationOptions)
        -> Result<Vec<offline::FinancialFileInfo>>;
    pub fn download_archive(&self, filename: &FinancialArchiveName,
        download: &DownloadOptions, options: &OperationOptions) -> Result<DownloadResult>;
    pub fn download_records(&self, query: &FinancialRecordsQuery,
        options: &OperationOptions) -> Result<offline::ReadResult<offline::FinancialRecord>>;
}
pub fn open_calc_client(config: &CalcClientConfig) -> Result<TdxClient>; // 惰性
pub struct FinancialRecordsQuery { pub filename: FinancialArchiveName,
    pub selector: DatSelector, pub report_date: Option<TradingDate> }
pub enum DatSelector { RequireSingle, Named(String) }
```

`mac::DownloadOptions` 与 `financial::DownloadOptions` 采用同一已定义逻辑schema：`max_bytes:u64=64MiB`（允许0，返回空LimitReached且不联网）、`chunk_size:NonZeroU32=30000`（MAC<=30000；HQ<=60000）、`expected_size:Option<u64>=None`（仅来自已验证metadata/调用方，不能当EOF的唯一证据）、`start_offset:u64=0`（MAC/HQ wire offset必须<=u32::MAX）、`max_chunks:NonZeroU32=4096`。MAC自动取meta也消耗operation预算；结果`DownloadResult`复用根级定义。普通下载在触限时给LimitReached；strict下载在无法证明EOF时报LimitExceeded。完整文件的非零start_offset只能由read_range入口使用，不能伪称完整档案。MAC即使恰好读满metadata.size，仍须该协议认可的短/空块或已验证总长响应证据，metadata陈旧不能制造Complete。

`FinancialParseOptions`：`max_dat_bytes:u64=256MiB`、`max_fields_per_record:u32=4096`、`max_total_fields:u64=4*1024*1024`、`strict_index:bool=false`、`raw:RawPolicy=Omit`、`cancel:Option<CancellationToken>=None`。各上限须正、有限并checked乘加，可配置更小值。`FinancialReadOptions`内含 `parse:FinancialParseOptions`、`max_archive_bytes:u64=64MiB`、`max_entries:u32=64`、`selector:DatSelector=RequireSingle`。`CalcClientConfig`为HQ ClientConfig的惰性构造参数加`endpoint=120.76.152.87:7709`、`probe_hosts=false`；`FinancialArchiveName`为无路径分隔符、无NUL、符合HQ远端路径长度限制的叶名。

T22源码补充：`DownloadOptions`是根级与financial重导出的共享类型，max_chunks允许正数且不超过4096。`CalcClientConfig`持有可替换的ClientBuilder和单个显式endpoint，构造不启动runtime；probe_hosts固定false。`FinancialRecordsQuery`/read/parse options字段私有，checked builder和getter为实际接口。新增`list_archive_files_detailed`、`download_records_detailed`、`download_records_with_options`、本地`read_detailed/parse_zip_detailed`返回来源完整的结果；普通方法维持本节签名。较小read archive cap同时下传到下载，query date/selector及OperationOptions raw为组合的最终配置。实现/用例索引见[financial-implementation-index.md](financial-implementation-index.md)，所有执行验证仍延期。

这四项对应Python同名4函数；Service由现有HQclient显式取得，或open_calc_client独立client。calc默认数字地址120.76.152.87、端口按现有HQ配置、8秒、probe_hosts=false，build不connect。列表路径固定 `tdxfin/gpcw.txt`，下载用HQ资源命令0x06B9，非F10 HTTP，也不借EX会话。download_records在同一operation内list/下载/ZIP/DAT全流程，必须使用Complete，LimitReached不能送ZIP当完整输入；Python旧max_bytes截断再parse造成的原异常保持。

列表UTF-8 replace，逐行strip后逗号split，少于3列/size非整数跳过，filename/hash按字段文本保留，多余列忽略；旧允许负size的事实在Python保留，Rust `FinancialFileInfo.filesize`用 `Option<u64>`+invalid诊断或strict拒绝，不能unsigned wrap。hash字符串和filesize是服务器元数据；目前没有MD5/实际大小认证，不能把“下载成功”写成“hash已验证”。FinancialArchiveName限制远端合法叶名/编码长度，不能用服务器filename作本地路径写入。

### 7.1 ZIP选择与限制

输入ZIP最大64MiB，总entries<=64，选中DAT解压<=256MiB；解压同时累计实际输出，不只信central directory。拒绝选中加密DAT、坏CRC/损坏ZIP、重复同名DAT；检查与读取同一个entry标识，不能验证第一同名条再按name取最后条。后缀匹配保持大小写敏感 `.dat`，`.DAT`不是同类，目录不当文件。仅在内存读取指定entry、不解压目录树。

**多个不同名DAT明确分流**：新Rust默认RequireSingle，两个不同名DAT报ResourceFormat并列出候选；Named精确选择一个entry，仍检查全归档DAT重名和全entries上限；无匹配报MissingData。Python legacy保持按archive顺序选第一个不同名`.dat`，已有helper虽然docstring说“唯一”，实际没有拒绝多个不同名DAT。不能用新RequireSingle改变旧Python。选择结果 `ArchiveProvenance{entry_name,entry_index,declared_size,actual_size,selector}`保留。

### 7.2 DAT和报告期

20B头 `<hIHIII>`，max_count为u16；每索引11B `<6s1cI>`，data按绝对offset读取 `report_size/4` 个f32。新Rust直接DAT输入也受256MiB限制；旧Python直接parse_dat和read_history_financial(.dat)没有此总长度检查，256MiB仅ZIP选中DAT解压路径已有，不能迁移时反套。report_size=0返回空；非4倍数、每行>4096floats、累计>4*1024*1024floats报ResourceFormat。code ASCII replace+去尾NUL，market raw u8不强制0/1，空code或offset0跳过；偏移加长度用checked算术。重复有效offset报ResourceFormat避免数据放大；truncated索引/越界offset旧break/skip，Rust默认带diagnostic的ReadResult，strict mode可拒绝；不得把未解析记录报告成已完整。

报告期优先级：新Rust显式query.report_date > archive filename第一个Unicode 15.0.0 Nd八位有效日期 > DAT header；本地read默认DAT header，除非显式override。Python parse_dat非0override优先，否则header；Python download_records从运行时 `re` 的第一个 `\d{8}` 字符串override（即使业务日期未核实）保持，T23桥接不得用固定Unicode版本的Rust扫描替代。无有效日期Rust保留raw header并诊断，不用今天补齐。字段向量按位置保持，包括NaN/Inf/负零bits；含义和单位未有字段字典证据，不公开虚假的named financial指标。

## 8. fixture、case ID 与 T19–T27 交付映射

所有新fixture置于 `tests/fixtures/sdk/{f10,ex,mac,offline,policy}`；不修改既有7709 fixture。每个manifest注明源基线、synthetic/recorded、请求hex/JSON、响应、预期typed值、expected error/diagnostic、RawPolicy、PythonLegacy/RustPublic差异、解析器版本及脱敏。下列输入是可直接构造的验收设计，不需要访问公网；录制样本另设显式 opt-in 流程。

| Case ID / 任务 | 具体输入 | 必须输出/断言 |
| --- | --- | --- |
| `F10-HTTP-01` T19 | local HTTP记录 stock_info(code=600000) | Entry=tdxf10_gg_comreq；body={Params:[gpquery,600000]}；UTF8 POST，正确headers；无隐含重试 |
| `F10-HTTP-02` T19 | localhost代理/CONNECT、IPv6失败→IPv4、TLS测试证书与正确SNI | 保留Host/代理/TLS验证；地址族回退不改全局resolver；取消停止后续connect |
| `F10-HTTP-03` T19 | ladder / skef10_hk / qhf9 /普通Entry各一个 | 精确路由hot/page2/page2/static；自定义Python路由覆盖另golden |
| `F10-TABLE-01` T20 | `{"ResultSets":[{"ColName":["N010","N010"],"Content":[[1,2,3]]}]}` | cells=[(N010,0,1),(N010,1,2),(col_2,2,3)]；Python投影N010=1,N010__2=2；三值都存在 |
| `F10-TABLE-02` T20 | 第一表空、第二表含新闻；重复key；一行标量7；ColDes fallback | 保留两表/重复key，news提第二表，标量转1cell，顺序不丢 |
| `F10-TABLE-03` T20 | ErrorCode=17且带表；顶层[]；非法UTF8/JSON | 前者Ok且ok=false；后两Protocol；空合法object区别于坏JSON |
| `F10-ENDPOINT-<method>` T20 | 第2.2节157个方法各默认/非默认query；复合前置响应固定 | 157项逐项request snapshot与返回shape；news_channels纯本地；每个非默认section至少一case；不会仅验签名不验wire |
| `F10-COMPOSE-01` T20/T23 | business_periods=[T002:20231231]→composition；缺T002另case | 第二请求报告期20231231；缺metadata Rust错误/旧Python ProtocolError；无无效第二请求 |
| `F10-COMPOSE-02` T20 | fund三类period列表 + topic列表空/多项 | 正确选首期/首topic，None/空的分支与旧错误一致 |
| `F10-PAGE-01` T20 | index页size2，返回2,2,1，总5；另达到max2仍满 | 正序5行；strict limit报LimitExceeded；Python max停止结果4行 |
| `F10-PAGE-02` T20 | 分红页1[A年报,B]，页2[A年报,A中报]，页3全重复 | 去重得到[A年报,B,A中报]、count3不是2；显式incomplete；推进页而非去重数量 |
| `F10-IDENTITY-01` T20/T27 | BRK.B、HK700/00700/六位code、sh000001 margin | Rust保留BRK.B；Python变B golden；HK兼容与严格区分；指数margin使用002，个股通道不冒充指数 |
| `EX-FRAME-01` T24 | 分片92Bsetup→分片16B头/body→业务帧；合并两帧 | Ready在setup完整后；帧不串包，method/sequence错误报Protocol |
| `EX-FRAME-02` T24 | zip_len=合法，unzip_len差1/超限/尾垃圾 | 精确长度/EOF验证，失败无重试当业务成功；不分配声明超限 |
| `EX-CODEC-<method>` T24 | 11个业务入口各最小正常+空+声明N少半行 | query字节与模型字段全断言，Rust坏行严格，Python保留prefix行为 |
| `EX-RAW-01` T24 | QuoteList 10B身份+290B槽，后150B全0xAA | 已知140B字段正确且150B保留；RawOmit仍有业务未知数据 |
| `EX-TIME-01` T24 | packed2004-01-02/minute=570；daily20240102；seq255→1 | 09:30正确、daily15:00标synthetic；range End拒绝；seq回绕合法 |
| `MAC-FIELD-01` T25 | 位图bit0、未知合法bit47、已知price bit72；slots=[1.0bits,DEADBEEF,12.5bits] | 3槽行宽80；bit72=12.5，不得读DEADBEEF；未知slot原4B保留，RawOmit相同 |
| `MAC-FIELD-02` T25 | bit128/150/151/159组合；0x122C请求bit128 | 128/150计slot、151/159不计；122C highbit InvalidArgument，控制区仍4B |
| `MAC-BAR-01` T25 | 0x122E四flag请求+36B记录，time=09:30:05，Seconds13 | 请求长度golden、秒不丢、7f32字段位正确；三flag旧坏布局不能通过 |
| `MAC-CODEC-<method>` T25 | 23个业务入口最小正常/空/损坏，3别名输入同canonical | 返回字段presence/unknown一致；所有aliases同wire；坏JSON不None |
| `MAC-PAGE-01` T25/T26 | count100行情页80+20；bars700+100；transaction1000+1000 | 最后一页剩余值正确；行情正序拼接，bar前插oldest→newest；短页终止 |
| `MAC-ADJUST-01` T26 | QFQ[-1,2]、None[10,20]、合法category1事件；另无events/provider | 有效重算数据和因子golden；无事件原QFQ+Rust诊断，Python原列表；取消不降级 |
| `MAC-RANK-01` T26 | 涨幅前2资金小，涨幅第3资金最大；top_n=2 sortMoney | 先全候选聚合，第三块进入top2；ties稳定；缺历史不当0 |
| `MAC-DL-01` T26 | metadata45000，chunk30000+15000；上限30000另case | Complete45000；上限恰满且未知EOF→LimitReached,next30000；strict LimitExceeded；源变更失败 |
| `MACEX-LOGIN-01` T25 | 新socketlogin成功/否定；MAC0x1c包与普通EX0x01包 | 每次连接login；否定Rust错误；只改首0x1c，body保持；Python原忽略bool单列golden |
| `MACEX-MACRO-01` T26 | names=[1_GDP,2_GDP,1_CPI] | bareGDP歧义、1_GDP精确、cpi大小写错误；换hostcache隔离 |
| `MACEX-FALLBACK-01` T26 | 0x2562失败，EX目录按market含28/31/38；另乱序目录 | 有界定位目标市场正确；乱序显式扫描/错误；返回name为code，desc不误丢 |
| `MACEX-HK-01` T25/T26 | market31 price_raw12345,time10:01:02,nature1；market23相同query | 31走23FC输出HKD12.345、trade_count0+NotProvided；23走122F；历史31走2406 |
| `MACEX-PAGE-01` T26 | all连续50满页1800 | Rust LimitExceeded，Python90000列表；普通count2000请求1800+200；排序不反转 |
| `OFF-DAY-01` T21 | pack(date20240102,100,120,90,110,amount1000.5,volume200,flag7),coeff(.01,.01) | OHLC1/1.2/.9/1.1、vol2、amount1000.5、flag7；encode相同32B |
| `OFF-MIN-01` T21 | 同日09:31的.lc1 float与.5整数两记录、reserved0xABC | 时间、价格各按独立layout，reserved原位roundtrip；误格式不默默通过 |
| `OFF-APPEND-01` T21 | disk.last=20240101，batch=[03,02,03]；另batch=[03,02] | 接受3条原顺序含重复；第二case final_key=02而非max03；write也append |
| `OFF-TAIL-01` T21 | 32B有效+3B尾；read/last_key/append(empty) | read1+tail3、last完整key、append错误且文件不变；Reject读报错 |
| `OFF-SYNC-01` T21 | mock writer记录flush/sync_all/close调用，三格式FormatDefault | daily有flush+sync；minute/EX无同等fsync；显式SyncAll三者执行并传播失败 |
| `OFF-EX-01` T21 | amount slot是f32 1234567.5的bits，其他字段固定 | amount_bits整数与HK视图1234567.5共享；encode完全一致 |
| `OFF-IND-01` T21 | GBK行 `0&#124;abcdef&#124;行业甲&#124;&#124;&#124;申万甲` 再同code行业乙、短code、坏GBK | abcdef保留且值乙覆盖；len!=6跳过；替换符保留；旧/新默认均不擅查isdigit |
| `OFF-BLOCK-01` T21 | 声明2条只1条；stock_count401；cfg叶名../x或越界symlink | 截断错误；declared401且最多400codes；越界错误，不读取目录外 |
| `OFF-GBBQ-01` T21 | 公开固定加密29B已知明文，0头/3B头/声明1但短记录 | u32 wrapping解密字段一致；短头空；短记录格式错误，非静默尾忽略 |
| `FIN-ZIP-01` T22 | archive=[a.dat,b.dat]、[a.dat,a.dat]、[A.DAT] | Rust默认多DAT拒绝、Named(b.dat)取b；Python第一a；同名全部拒绝；A.DAT无匹配 |
| `FIN-ZIP-02` T22 | 65entry、加密目标、声明小实际>限、坏CRC、64MiB+1 | 各限额和格式错误，检查/读取同一entry；无文件树解压 |
| `FIN-DAT-01` T22 | header日期20231231，1索引offset31，fields[1.5,-0bits]；override20240630 | 日期按优先级、两个floatbits保真、市场raw保留 |
| `FIN-DAT-02` T22 | report_size3/0/4097*4、重复offset、截索引、越界offset | 3和超字段错误，0空；重复有效offset拒绝；legacy break/skip与Rustdiagnostic各断言 |
| `FIN-DOWNLOAD-01` T23 | loopbackHQ list→filename gpcw20240630.zip→ZIP header20231231 | 06B9、列表路径正确，record日期20240630；Complete才解析；不触F10/EX |
| `POLICY-EXT-01` T19/T23/T26 | 时钟分阶段推进：探测2s+setup2s+page13s+page23s，默认8s | Rust在page2余额截止，总<=8s+容差；Python每请求8s可完成10s，不错误套总期限 |
| `PY-EXT-01` T27 | 上述全部legacy golden：错误类/返回形态/default/重试/页数/raw/降级 | 迁移前后Python可观察结果一致；no-rustc wheel仍跑已迁移扩展loopback |

T19交付HTTP transport/错误/预算；T20交付157入口与多表/分页/新闻；T21交付38offline函数和8模型；T22交付ZIP/DAT上限与selector；T23交付HQ财务组合/混合helpers与Python迁移边界；T24交付EX11命令及setup/frame/profile；T25交付MAC23/MAC-EX13布局与未知位；T26交付复权/排序/回退/cache/分页/下载组合；T27交付扩展Python双路径兼容。每任务验收要求对应case manifest、fixture实际存在、测试日志与消费者例子齐备，不能用本文覆盖计数替代通过率。

## 9. 待补证据与冻结条件

1. F10完整section/page endpoint wire快照、84频道有效性与动态字段的scope字典仍需脱敏录制；本文157方法覆盖为入口规格覆盖，不是157项线上可用证明。所有Inferred字段（IPO、分红日期、金额/比例）不提升为已证实单位。
2. EX各市场volume/amount/price scale、未知290B槽尾、日线/夜盘时间含义、服务端header关联例外、各命令短页EOF需真实样本。按命令允许的例外必须有fixture，不关闭全局严格校验。
3. MAC三阶段可选setup响应、MAC-EX login成功/否定payload、各市场postprocess单位、资金JSON缺键与file meta hash算法需证据；默认MAC总目标与MacEx一次wire差别须在T25/T26固定。
4. 跨平台flush/sync错误与GBK替换差异需基线fixture运行冻结；ReadOptions=256MiB、缺文件last_key=None、write不创建父目录和append/批内保序/残尾/行业规则已在本文明确。
5. 新Rust的严格类型、RequireSingle、RawPolicy传递、owner总期限与cache命名空间尚无实现证据；T28 feature消费者与T29 wheel验证需在实现后完成。本次不构建、不跑测试、不联网、不新增源代码或fixture。
