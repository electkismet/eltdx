# HQ Rust 分域 API 规格

状态：拟议规格，未实现、未编译。范围是默认 feature 的 HQ 7709、HQ-only helpers、股本复权与交易日；对应 P2 的 T11–T18。本次只新增文档，不改变 Python、runtime 或 protocol。版本基线固定为 `af471f1adf38757f6aa0188968db8110eb23f6ad`。

## 1. 适用边界与计数规则

本文件细化 [public-api.md](public-api.md)，执行策略与绑定边界详见 [execution-and-compatibility.md](execution-and-compatibility.md)。核心名称保持 `TdxClient`、`CodeService`、`QuoteService`、`KlineService`、`MinuteService`、`TradeService`、`AuctionService`、`CorporateService`、`CalendarService`、`ResourceService`、`PinnedSession`、`Subscription`。本文件其余 service、query、DTO 和方法均为**本分域新增拟议**，尚不属于已发布 API。新增 query/DTO 放 `eltdx::types`，服务类型的公共路径为 `eltdx::services`，不得一律重导出至根；该命名空间不限制源码物理目录。

`TdxClient` 已约定的 getter 不改名。以下是默认 HQ 域完整的15个 getter；均为 `pub fn <name>(&self) -> <Service>`、owned、可 Clone、不启动线程或网络。它们持同一 owner，任一 client close 封闭全部网络入口。`money_flow` 专门承载 HQ 0FFC；F10 历史资金流仍由独立 feature 服务提供。

| Getter | 返回服务 | 本文方法 |
| --- | --- | --- |
| `codes()` | `CodeService` | M01–M17 |
| `quotes()` | `QuoteService` | M18–M22、M24 |
| `klines()` | `KlineService` | M25–M30、M98–M99 |
| `minutes()` | `MinuteService` | M31–M40 |
| `trades()` | `TradeService` | M41–M50 |
| `auctions()` | `AuctionService` | M51 |
| `money_flow()` | `MoneyFlowService` | M52、M100 |
| `corporate()` | `CorporateService` | M53–M56 |
| `limits()` | `LimitService` | M58–M59 |
| `calendar()` | `CalendarService` | M60–M66 |
| `resources()` | `ResourceService` | M67–M71 |
| `company_info()` | `CompanyInfoService` | M72–M73 |
| `session()` | `SessionService` | M74–M76 |
| `health()` | `HealthService` | M77–M78 |
| `helpers()` | `HelperService` | M79–M94、M96–M97 |

`PinnedSession::quotes(&self) -> PinnedQuoteService<'_>` 单独承载 M23，不是 `TdxClient` getter；M57/M95 是纯函数。本文的 `Mxx` 只计业务方法，不计 getter、query builder、DTO getter、别名和已有生命周期方法；`Fxx` 为离线测试语料建议，尚未运行。

正常 Rust 入口固定 `RustPublicExecutionProfile`：默认整次操作8秒、每个 wire 请求最多2次尝试（含首次）、整次额外尝试最多32；元数据、分页、恢复、可选补充共享预算。Python 通过 `#[doc(hidden)] pub mod __python_compat` 使用 `PythonLegacyExecutionProfile`：每次 wire 请求默认8秒，保留旧 startup 计时边界、各层 retry/page 计数、`max_pages=None`、fallback 条件和已有 DTO。桥接不引入 PyO3、不新增 feature，是未稳定 API 边界，不是访问安全隔离。Python 不新增65535输入上限或整次32次预算。已知旧行为问题须单独批准修复，不利用迁移暗改。

完整 `stock_profile_table`、`shortline_indicators`、板块行情与成分股行情都属于默认 HQ P2。统计 ZIP、板块文本/DBF 的**内部解码器**不能错误地挂到 `offline` 或 `f10` feature。HTTP F10 的 `index_members/index_etfs/ipo_calendar/board_members/stock_topics/topic_stocks/theme_strength_rank` 不在本域；其中旧 `board_members` 是分红成分记录表，不等同于这里的板块当前成员。

## 2. 固定源码索引

下列所有链接固定提交，不使用 main/HEAD。编号用于各节引用。

| 源码组 | 固定来源 |
| --- | --- |
| S01 代码与报价 | [codes.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/codes.py)、[quotes.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/quotes.py)、[_batch.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/_batch.py)、[subscription.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/subscription.py) |
| S02 K线/分时 | [bars.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/bars.py)、[delisted_bars.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/delisted_bars.py)、[minutes.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/minutes.py)、[_bar_time.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/_bar_time.py) |
| S03 逐笔/竞价/金额流 | [trades.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/trades.py)、[auctions.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/auctions.py)、[money_flow.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/money_flow.py) |
| S04 企业/文件/会话 | [corporate.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/corporate.py)、[limits.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/limits.py)、[company_info.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/company_info.py)、[resources.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/resources.py)、[session.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/session.py)、[health.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/health.py) |
| S05 helpers | [core.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/helpers/core.py)、[shortline.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/helpers/shortline.py)、[market.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/helpers/market.py)、[boards.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/helpers/boards.py) |
| S06 equity/calendar/stats | [equity.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/equity.py)、[workday.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/workday.py)、[stats.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/stats.py)、[ApiBase](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/api/base.py) |
| S07 数据模型 | [security.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/models/security.py)、[quote.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/models/quote.py)、[kline.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/models/kline.py)、[minute.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/models/minute.py)、[trade.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/models/trade.py)、[trade_batch.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/models/trade_batch.py) |
| S08 更多模型 | [corporate.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/models/corporate.py)、[auction.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/models/auction.py)、[resource.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/models/resource.py)、[session.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/models/session.py)、[limit.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/models/limit.py)、[money_flow.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/models/money_flow.py) |
| S09 wire规格 | [constants.py](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/src/eltdx/protocol/constants.py)、[unit.rs](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/crates/eltdx-protocol/src/unit.rs)、[quotes.rs](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/crates/eltdx-protocol/src/commands/quotes.rs)、[resources.rs](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/crates/eltdx-protocol/src/commands/resources.rs)、[trades.rs](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/crates/eltdx-protocol/src/commands/trades.rs)、[corporate.rs](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/crates/eltdx-protocol/src/commands/corporate.rs)、[auctions.rs](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/crates/eltdx-protocol/src/commands/auctions.rs)、[security.rs](https://github.com/electkismet/eltdx/blob/af471f1adf38757f6aa0188968db8110eb23f6ad/crates/eltdx-protocol/src/commands/security.rs) |

## 3. 所有领域共用的返回、单位与限制

本节字段是逻辑 schema；除已在主规格明确公开字段的 `DownloadResult` 外，DTO 实际字段私有，用同名只读 getter，Vec 对应 getter 返回 slice，String 返回 str，复制标量按值返回。查询也为私有字段。`Result<T>` 为主规格的 `Result<T, Error>`；`Vec/Option/String/u8/u16/u32/u64/usize/i64/f32/f64/bool`、`std::path::PathBuf`、`std::time::Duration`、`std::net::SocketAddr` 和 `bytes::Bytes` 是标准库/已有依赖类型。

表内省略类型的 `id` 均为 `SecurityId`，日期字段为 `TradingDate`，`meta` 为 `ResultMeta`；同一行以斜杠连列的字段各自具有冒号右侧类型。服务类型只持主规格owner及私有配置：`HelperService`（组合算法/缓存）、`LimitService`（0452）、`CompanyInfoService`（02CF/02D0）、`SessionService`（显式会话查询）、`HealthService`（本地检查/远端探测）、`MoneyFlowService`（0FFC）。它们无新增公有字段。`ErrorKind`、`Error`与生命周期类型全部复用主规格。

复用主规格类型：`SecurityId`（规范化六位代码与HQ市场）、`HqMarket`（Shenzhen/Shanghai/Beijing）、`TradingDate`（合法交易日期值）、`EventTime`（日期/交易所本地时刻/仅日内时刻明确区分）、`Price`（原数值与尺度来源）、`Quantity`（值与股/手/张/未知单位）、`Amount`（元或来源未知单位）、`Ratio`（比例或百分数）、`OperationOptions`、`RawPolicy`、`PageOptions`、`MissingPolicy`、`BatchOutcome<T>`、`BatchItem<T>`、`SnapshotBatch = BatchOutcome<Snapshot>`、`SubscriptionOptions`、`Subscription`、`DownloadResult`、`DownloadCompletion`。这些不是本文件重新定义的平行类型。

定义位置索引如下；同一名称只有一个逻辑定义。主规格共享类型尚待实现，但本文件不能创建同名替代品。

| 名称组 | 公共路径与定义位置 |
| --- | --- |
| client、基础服务、PinnedSession | client/pin沿用根路径，服务为 `eltdx::services`；[主规格](public-api.md) 第3节“最小公共入口”与第4节“所有权与关闭”；新增6服务的状态和getter见本文第1/3节。 |
| Error、ErrorKind、Result | 根路径；主规格第10节“语言中立错误与 Python 映射”。 |
| OperationOptions、CancellationToken | 根路径及主规格允许的types重导出；主规格第6节“操作期限、取消与重试预算”。 |
| SecurityId、HqMarket、TradingDate、EventTime、Price、Quantity、Amount、Ratio、RawPolicy | 主规格第8节“类型、单位、日期和 raw 策略”；根路径及同一个 `eltdx::types` 定义。 |
| BatchOutcome、BatchItem、MissingPolicy、SnapshotBatch、PageOptions | 主规格第9节“批量结果、缺失和分页”与本文第3/4节；根路径及同一个 `eltdx::types` 定义。 |
| SubscriptionOptions、Subscription与其事件 | 主规格第5节；资源Subscription沿用根路径，其DTO走主规格的根/types重导出。 |
| DownloadResult、DownloadCompletion | 主规格第9.3节“有界资源读取与完整下载”，本文第10节逐字复用；根/types同一类型。 |
| 本文111个表项 | query集中在第4节，其余DTO/枚举按第3节、第5–14节的“类型”表定义，均 `eltdx::types`；唯一服务表项 `PinnedQuoteService<'a>` 位于第5节，路径 `eltdx::services`。 |

| 类型 | 本分域集中定义 |
| --- | --- |
| `ResultMeta` | `source: SourceStamp, diagnostics: Vec<Diagnostic>, raw: Option<RawData>`；每个独立响应、聚合/表格结果含 `meta`，下表不再重复。 |
| `SourceStamp` | `command: Option<u16>, endpoint: Option<SocketAddr>, session_id: String, metadata_version: Option<u64>, trading_date: Option<TradingDate>`；聚合另在diagnostics/页meta记录多个来源，不能谎报唯一command。 |
| `Diagnostic` | `source: String, stage: String, status: EnrichmentStatus, error_kind: Option<ErrorKind>, message: Option<String>`；不含PyErr。 |
| `EnrichmentStatus` | `Succeeded / NotRequested / Unavailable / Failed`。Unavailable为有效但无可用补充数据；Failed带原因；业务取消/总deadline仍Err。 |
| `RawData` | `payloads: Vec<Bytes>, wire_frames: Option<Vec<Bytes>>, row_ranges: Vec<(usize, usize, usize)>`，range为页号/offset/len；只有Retain策略有。 |
| `DataPage<T>` | `items: Vec<T>, start: u64, requested: usize, next_cursor: Option<PageCursor>, end: EndEvidence, meta`；None cursor必须有EOF或RequestedWindow证据，后者仅表示用户窗口完成，不能把防失控cap当EOF。 |
| `PageCursor` | 不透明版本化query fingerprint、下一offset和必要累加基准；不可手造、跨代码/日期/来源复用。 |
| `EndEvidence` | `Unknown / EmptyPage / VerifiedShortPage / ExplicitTotal(u64) / RequestedWindow`；RequestedWindow只表示满足用户选定窗口。 |
| `Collected<T>` | `items: Vec<T>, end: EndEvidence, pages: usize, meta`；完整扫描不能返回Unknown，cap触发返回Err。 |
| `Table<T>` | `rows: Vec<T>, meta`；列表顺序由领域定义，没有隐含排序；一行可含不可用的可选字段。 |
| `SecuritySelection` | `Explicit(Vec<SecurityId>) / AllAShares`。Explicit空是空集合，不等于全市场；AllAShares才触发目录读取。 |
| `SecurityCategory` | `AShare / BShare / Etf / Index / PrivateConvertibleBond / Bond / Unknown / Other(String)`；前缀分类是推断，不等于交易所权威存在性。 |
| `SecurityBoard` | `SseMain / SzseMain / Star / ChiNext / Beijing / None / Other(String)`，伴随原因字符串，未知不猜。 |
| `QuoteCategory` | `HqAShares / Wire(u16)`，HqAShares=6；Wire只承诺可编码，不承诺未取样类别业务含义。 |
| `QuoteSort` | `Code=0, Last=6, Amount=10, ChangePct=14, SealAmount=28, OpenAmount=29, RiseSpeed=46, ShortTurnover=204, VolumeRiseSpeed=208, OpeningRush=266, TwoMinuteAmount=268, OpenChange=281, HighChange=282, LowChange=283, Drawdown=286, Attack=287, Wire(u16)`。 |
| `KlinePeriod` | `Min1/Min5/Min15/Min30/Min60/Day/Week/Month/Quarter/Year/Minutes(u16)/Days(u16)/Seconds(u16)`；参数1..65535，固定周期规范化，禁止category22当OHLC。 |
| `AdjustMode` | `None / Qfq / Hfq / FixedQfq / FixedHfq`，wire0..4。 |
| `BarTime` | `Start / End`；End只对分钟级加周期，日/周/月/季/年和秒级不做分钟偏移。 |
| `KlineKind` | `Auto / Stock / Index`，Auto使用已冻结代码分类，响应保留实际kind。 |
| `AuxKind` | `BuySellStrength / VolumeComparison`，wire0与11；未知selector走advanced，不伪装已命名业务量。 |
| `TradeSide` | `Buy / Sell / Neutral / Status(i64)`，保留未识别status。 |
| `TradeEventKind` | `Trade / OpeningMatch / AuctionSnapshot`；status8优先为AuctionSnapshot，其他09:25记录为OpeningMatch。 |
| `MarketDateChoice` | `Current / Date(TradingDate)`；Current按服务端会话日期规则，不在各parser读取电脑今天。 |
| `BoardCategory` | `Concept / Style / Region / Industry1 / Industry2 / Industry3 / Industry880 / All`，对应旧中文八种类别。 |
| `BoardId` | 经校验六位板块号及可选显式HQ市场；与SecurityId分开，lookup接受代码或完整代码，找不到报InvalidArgument。 |
| `LimitStatus` | `Normal / NoPriceLimit / MissingReference / UnknownRule / Sealed / Touched / NotAtLimit`；按DTO场景使用。 |
| `LimitRule` | `Approved { id: String, effective_from: TradingDate, effective_to: Option<TradingDate>, ratio: Option<Ratio> } / Unknown { reason: String }`；None ratio只在已确认无限制规则中使用。 |

所有正常Rust批量输入最多65535个 **occurrence**，重复也计数。2 MiB（2097152字节）限制精确定义为SDK持有的**规范化身份缓冲的逻辑字节数**：HQ每个身份为1字节market加6字节ASCII代码，共7字节；按输入occurrence累加，不是调用者原字符串的UTF-8字节数，也不是 `size_of::<SecurityId>()` 或总堆占用。65535个HQ身份共458745字节，因此HQ先触及条数限制；跨协议身份按各域规范化编码长度累计并与条数限制取较早者。复制/分配前checked计数并 `try_reserve`；额外索引、Vec容量和分配器开销仍需各自有界，不能由这2 MiB推断总内存。文本/path在各自wire约束下另验字节数。超过工作量cap为 `LimitExceeded`；单个wire参数无法编码、非法date/枚举组合为 `InvalidArgument`；响应损坏为 `Protocol`。这不是旧Python的新限制。

普通请求默认RawPolicy::Omit；保留模式共享页级Bytes，聚合保留页界，不能拼成一个伪wire。必需字段的损坏/主来源失败返回Err；Option仅表示协议未提供、合法空值、零分母或明示可选源不可用，必须能从状态/diagnostics辨认。非有限数不得在必需字段默默转0。单位未取证用Quantity/Amount未知单位，不能因为Python名带volume就声称股。

所有 `*_many` 返回输入occurrence顺序的 `BatchOutcome`，重复不删，空输入不联网。其他接收 `ids: &[SecurityId]` 的普通批量入口也适用空输入返回空结果的规则，包括snapshots/legacy/depth/capital/finance/full_quotes/profile；`SecuritySelection::Explicit(vec![])` 同理，不加载全市场目录。显式要求非空的ShortlineQuery与pin绑定的refresh空激活在各自定义中例外说明。没有某日行情的合法空series不是证券缺失；只有按域验证后才使用ConfirmedMissing。Python原来去重并返回dict的入口仍如此投影。

## 4. Query 与 builder 完整字段表

所有以下query均有 `pub fn new() -> Self`（必须参数除外），以及 `pub fn with_<field>(self, value: <字段类型>) -> Result<Self>`；表内每个字段都产生这一完整签名，包括有默认值、必需构造参数与嵌套query的字段。例如 `TradeQuery::with_count(self, value: usize) -> Result<Self>`、`RefreshRequest::with_id(self, value: SecurityId) -> Result<Self>`。字段私有；复制标量/Copy枚举的同名getter返回值，String返回 `&str`，Vec返回slice，其余非Copy值返回共享引用。`Option<T>`字段接收 `Option<T>`，`None`即清除，不能以0代替；getter对非Copy值返回 `Option<&T>`。可构造无参数默认的query实现 `Default`，等同 `new()`；有必需构造参数的query不实现 `Default`。

每个setter先构造候选值，再验证该字段与当前其余字段构成的完整query，成功才返回新对象；例如FactorQuery设置start/anchor后的顺序必须合法，CompanySectionQuery修改offset/length必须checked且保持完整范围可编码。构造和setter不做I/O；`build`不是必需步骤。网络入口重验整个query及所选方法的专有限制，不能把验证推迟到wire cast。`with_pages`验证该领域的page_size上限，但不要求max_records是page_size的整数倍；最终页面只允许交付剩余记录数，未证明EOF仍按cap失败。

`PageOptions` 复用主规格，并明确以下完整签名：`pub fn new(page_size: usize, max_pages: usize, max_records: usize) -> Result<Self>`、`pub fn with_page_size(self, value: usize) -> Result<Self>`、`pub fn with_max_pages(self, value: usize) -> Result<Self>`、`pub fn with_max_records(self, value: usize) -> Result<Self>`；三个同名只读getter均为 `pub fn <field>(&self) -> usize`。值须>0，不提供 `Default`，各domain query明确构造下表自己的默认值。下表的 `{page_size,max_pages,max_records}` 只是三元默认值记法，不是允许公开字段构造的Rust语法。任何usize值都表示有限数值，`usize::MAX`不能充当“无限”哨兵，相关算术必须checked。原Python `None`只在legacy桥接内部表示。`PageOptions`自身不限制某一wire上限，设置到具体query时按域验证，0452无page-size字段的例外见SpecialLimitQuery。

`CodeQuery/CodeScanQuery/CategoryQuery/KlineQuery/DelistedQuery/TradeQuery/SpecialLimitQuery` 另有 `pub fn with_cursor(self, cursor: &PageCursor) -> Result<Self>`，continuation初始None、字段私有。起点setter清除continuation，其他语义字段改变后提交前校验失配，细则按[公共分页契约第9.2节](public-api.md)。所有many入口拒绝continuation（包括len=1）；单证券page/collect可续读，collect仅收剩余范围。metadata变版、跨owner/endpoint/日期的token不能直接沿用；token不保有pin或承诺跨调用快照。

| Query类型 | 字段类型、默认及范围（均为新Rust拟议默认，除非注明旧事实） |
| --- | --- |
| `CodeQuery` | `start: u32=0`（wire可表示范围）、`limit: usize=1600`，0..1600；0直接空窗口不联网。 |
| `CodeScanQuery` | `pages: PageOptions={1600,200,320000}`；`category: Option<SecurityCategory>=None`；过滤在保留原顺序的完整目录上应用。 |
| `CategoryQuery` | `category: QuoteCategory=HqAShares, sort: QuoteSort=Code, start:u16=0, count:usize=80`（1..80拟议服务批次上限）、`ascending:bool=false`。无sort/code排序的wire reverse=0；其他ascending为2/descending为1。 |
| `RefreshRequest` | 必须 `new(id: SecurityId) -> Self`；`cursor:u32=0`。用于有pin身份的刷新，不跨会话承诺cursor可恢复。 |
| `KlineQuery` | `period:KlinePeriod=Day, start:u16=0, count:usize=800`（1..800）、`adjust:AdjustMode=None, anchor_date:Option<TradingDate>=None, kind:KlineKind=Auto, bar_time:BarTime=Start, pages:PageOptions={800,200,160000}`；page取count，collect取pages。offset递增超过wire u16范围时LimitExceeded，不能绕回。 |
| `DelistedQuery` | `start:u16=0, count:usize=800`（1..800）、`pages:PageOptions={800,200,160000}`；只日线/无复权，没有假装支持的period/adjust字段。 |
| `MinuteQuery` | `date:MarketDateChoice=Current`；最近分时允许Current或Date；today/history便利方法将date固定；完整曲线一次wire，响应仍受frame/record cap。 |
| `SparklineQuery` | `selector:u8=1, window:u16=20`（拟议1..65535；旧wire本身u16）、未知selector保留echo与状态，若无语义支持返回Unsupported；不是自动采样器。 |
| `TradeQuery` | `date:MarketDateChoice=Current, start:u16=0, count:usize=1800`（1..1800）、`pages:PageOptions={1800,100,180000}`；page/columns用count，collect/opening_match用pages。今日/历史分派0FC5/0FC6，不用日期字符串猜测。 |
| `AuctionQuery` | `date:MarketDateChoice=Current`；当前/历史wire056A selector按协议，禁止私加固定时间窗截断。 |
| `CapitalQuery` | `batch_size:usize=75`（1..200）、用于分块提交；原Python默认75/max200事实相同，但Rust仍共享总预算。 |
| `FactorQuery` | `anchor_date:Option<TradingDate>=None, start_date:Option<TradingDate>=None, capital:CapitalQuery=default`；两者都在时anchor>=start。 |
| `FinanceQuery` | `missing:MissingPolicy=Error`；完整DTO默认，不以字符串fields改变返回类型；字段投影另用枚举方法。 |
| `SpecialLimitQuery` | `start_index:u16=0, pages:PageOptions={800,200,10000}`；0452无客户端page-size字段，800只作为分配/工作量提示，不能写入wire；max_records按真实返回记录计数。 |
| `StatsQuery` | `path:String="zhb.zip", chunk_size:usize=30000`（1..60000）；ZIP32MiB、单entry32MiB、累计展开64MiB、entries128；不能用自定义query调大到无界。 |
| `CompanySectionQuery` | 必须 `new(filename:String, offset:u32, length:u32)->Result<Self>`；filename trim后GBK编码为1..80字节，不可编码时InvalidArgument，Rust新路径拒绝NUL；length>0且checked offset+length可表示；正文响应受总frame上限；详见第10节。 |
| `CalendarQuery` | `benchmark:SecurityId=sh000001, pages:PageOptions={800,200,160000}, refresh:bool=false`；只指数日线、无复权。 |
| `RankQuery` | `category:QuoteCategory=HqAShares, sort:QuoteSort=ChangePct, ascending:bool=false, count:Option<usize>=Some(80)`（Some>0；None收集至EOF，仍受pages）、`pages:PageOptions={80,200,16000}`。 |
| `DailySharesQuery` | `selection:SecuritySelection=AllAShares, stats:StatsQuery=default, refresh_stats:bool=false`。 |
| `DailyLimitsQuery` | 必须 `new(trade_date:TradingDate)->Self`；`selection:SecuritySelection=AllAShares`；规则来自版本化已批准policy，无调用方偷偷改百分比的字段。 |
| `ShortlineQuery` | 必须 `new(ids:Vec<SecurityId>)->Result<Self>`（非空）；`stats:StatsQuery=default, refresh_stats:bool=false`。 |
| `LadderQuery` | `selection:SecuritySelection=AllAShares, include_touched:bool=false, count:Option<usize>=None`（Some>0）、`stats:StatsQuery=default, refresh_stats:bool=false`。 |
| `ProfileQuery` | `include_security:bool=true, include_finance:bool=true`；quote是主来源，security/finance明示可选。 |
| `AuctionDataQuery` | `date:MarketDateChoice=Current, include_series:bool=true, include_snapshot:bool=true, include_quote:bool=true, pre_close:Option<Price>=None`；显式pre_close优先，非有限/负值InvalidArgument。 |
| `BoardQuery` | `category:BoardCategory=Concept, refresh:bool=false, data_dir:Option<PathBuf>=None, definitions_dir:Option<PathBuf>=None`；Rust默认内存缓存，不扫描C盘/cwd/环境目录。指定目录后可读取/更新该缓存；缺必需定义/DBF返回ResourceFormat，不猜类别。 |

Rust新增默认不是对旧Python边界值合法性的追认：例如旧category count走u16编码，Rust便利入口拟限制单页80；旧resource read(size=0)的行为也不因Rust空窗口策略改变。Python基线详见 [python-baseline.md](python-baseline.md)。

## 5. 代码表与行情（T11、T12、T15）

旧Python事实（S01）：`count(market)`；`list(market,*,start=0,limit=1600)`；`all(market,*,page_size=1600)`；stocks/a_shares/etfs/indices及各all版本；`latest_stock_list/latest_st/latest_suspended(market=None)`。报价为 `get_snapshots(codes,*,on_missing="raise")`、`legacy(codes)`、`get_depth(codes)`、`list_by_category(category,*,sort_by=None,start=0,count=80,ascending=False)`、`refresh(codes=None,cursors=None)`；`poll_push(timeout=0.0,parse=False)`、`drain_pushes(parse=False)`；callback订阅默认timeout0.5秒、filter_codes=True、reactivate_interval7秒。Python别名st/latest_stocks/suspended/subscribe_hq保持。

以下各行都是 `pub` 方法签名；表中 `Type::method(...) -> T` 精确定义 `impl Type { pub fn method(...) -> T; }`，纯函数路径定义同名 `pub fn`。这一记法适用于全篇方法表，`options`总在最后。

| ID | 拟议签名 |
| --- | --- |
| M01 | `CodeService::count(&self, market: HqMarket, options: &OperationOptions) -> Result<u32>` |
| M02 | `CodeService::list(&self, market: HqMarket, query: &CodeQuery, options: &OperationOptions) -> Result<DataPage<Security>>` |
| M03 | `CodeService::all(&self, market: HqMarket, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<Security>>` |
| M04 | `CodeService::all_markets(&self, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<Security>>` |
| M05 | `CodeService::stock_count(&self, market: HqMarket, query: &CodeScanQuery, options: &OperationOptions) -> Result<usize>` |
| M06 | `CodeService::a_share_count(&self, market: HqMarket, query: &CodeScanQuery, options: &OperationOptions) -> Result<usize>` |
| M07 | `CodeService::stocks(&self, market: HqMarket, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<Security>>` |
| M08 | `CodeService::a_shares(&self, market: HqMarket, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<Security>>` |
| M09 | `CodeService::etfs(&self, market: HqMarket, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<Security>>` |
| M10 | `CodeService::indices(&self, market: HqMarket, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<Security>>` |
| M11 | `CodeService::all_stocks(&self, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<SecurityId>>` |
| M12 | `CodeService::all_a_shares(&self, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<SecurityId>>` |
| M13 | `CodeService::all_etfs(&self, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<SecurityId>>` |
| M14 | `CodeService::all_indices(&self, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<SecurityId>>` |
| M15 | `CodeService::latest_stock_list(&self, market: Option<HqMarket>, options: &OperationOptions) -> Result<Collected<Security>>` |
| M16 | `CodeService::latest_st(&self, market: Option<HqMarket>, options: &OperationOptions) -> Result<Collected<Security>>` |
| M17 | `CodeService::latest_suspended(&self, market: Option<HqMarket>, options: &OperationOptions) -> Result<Collected<SecurityId>>` |
| M18 | `QuoteService::snapshots(&self, ids: &[SecurityId], options: &OperationOptions) -> Result<SnapshotBatch>` |
| M19 | `QuoteService::snapshots_with_policy(&self, ids: &[SecurityId], missing: MissingPolicy, options: &OperationOptions) -> Result<SnapshotBatch>` |
| M20 | `QuoteService::legacy(&self, ids: &[SecurityId], options: &OperationOptions) -> Result<BatchOutcome<LegacyQuote>>` |
| M21 | `QuoteService::category(&self, query: &CategoryQuery, options: &OperationOptions) -> Result<DataPage<CategoryQuote>>` |
| M22 | `QuoteService::depth(&self, ids: &[SecurityId], options: &OperationOptions) -> Result<QuoteUpdates>` |
| M23 | `PinnedQuoteService::refresh(&self, requests: &[RefreshRequest], options: &OperationOptions) -> Result<QuoteUpdates>` |
| M24 | `QuoteService::subscribe(&self, ids: &[SecurityId], subscription: &SubscriptionOptions, options: &OperationOptions) -> Result<Subscription>` |

`PinnedQuoteService<'a>` 是 `PinnedSession::quotes(&self) -> PinnedQuoteService<'_>` 所有权受限借用服务，不可逃出guard；M23的receiver隐含同一寿命。depth在一次操作内取得pin、cursor0刷新并释放；超过100可分块但不能复用不属该pin的cursor。refresh单wire最多100、空列表是该pin合法的空激活/轮询命令，故不同于一般空批量。普通quotes getter不提供任意pool slot的stateful刷新。Python refresh/poll/drain旧入口由桥接维持，原始push只能从唯一demultiplexer取得，不与Subscription抢底层queue。

| 类型 | 必需字段与含义 |
| --- | --- |
| `Security` | `id:SecurityId, name:String, multiple:u16, decimal:u8, previous_close:Price, volume_ratio_base:f64, category:SecurityCategory, category_reason:String, board:SecurityBoard, board_reason:String`；name可空但非None；精度来自目录，不注册进程global。 |
| `QuoteLevel` | `price:Price, volume:Quantity`（行情手数）；raw delta在RawData记录，不构造伪第五档。 |
| `Snapshot` | `id, active:u16, last/pre_close/open/high/low:Price, event_time:EventTime, server_time_raw:i64, total/current/inside/outside:Quantity, amount/open_amount:Amount, bids/asks:Vec<QuoteLevel>, meta`；change是Price差，change_pct为Option<Ratio>，pre_close0→None。 |
| `LegacyQuote` | `snapshot:Snapshot, trading_status:u16, rise_speed:Option<Ratio>, active2:Option<u16>, tail_metrics:[i64;4]`；0x20停牌位按原值，不能用volume0猜停牌。 |
| `CategoryQuote` | `snapshot:Snapshot, bid1/ask1:QuoteLevel, status_sort_raw:u16, rise_speed/short_turnover/opening_rush/volume_rise_speed:Ratio, two_minute_amount:Amount, depth:f64, meta`；只有已取证字段有命名单位，派生seal=bid1.price×bid1.volume×100元。 |
| `QuoteUpdate` | `id, cursor:u32, active:u16, snapshot:Snapshot, status_raw:i64`；cursor按协议意义保留，不能当严格递增交易序号。 |
| `QuoteUpdates` | `requested:Vec<SecurityId>, updates:Vec<QuoteUpdate>, meta`；增量未出现某证券表示无更新，不是ConfirmedMissing。 |
| `PinnedQuoteService<'a>` | 持 `&'a PinnedSession`，不可Clone为owned service；只暴露受控业务调用。 |

证券all必须空页停止，短页继续；固定市场合并顺序Shanghai→Shenzhen→Beijing，与旧all_markets顺序一致。重复页/无进展为Protocol；按wire限制不能再增长offset为LimitExceeded。ST筛选去空白/转大写后前缀ST、*ST、SST、S*ST。snapshots/finance的二分恢复最多32个额外请求并受Rust整次预算，按identity重排；重复响应冲突、外来id、不可恢复解析错为Protocol，不能当missing。订阅详见主规格；分组100，建立失败回滚全部pin，续订7秒，队列1024/64MiB，Gap必达控制槽。

## 6. K线、退市日线与分时（T13）

旧Python事实（S02）：bars.get(code,*,period="day",start=0,count=800,adjust=None,anchor_date=None,kind=None,include_raw=False,all_pages=False,page_size=800,max_pages=200,batch_size=None,bar_time="start")；delisted.get相似但没有anchor/kind/bar_time，只支持day且无复权；minutes.today/history/recent支持单个或多code，raw=False、batch_size=None，history必需date、recent日期None；aux默认buy_sell_strength，sparkline selector1/window20。

| ID | 拟议签名 |
| --- | --- |
| M25 | `KlineService::page(&self, id: &SecurityId, query: &KlineQuery, options: &OperationOptions) -> Result<KlineSeries>` |
| M26 | `KlineService::collect(&self, id: &SecurityId, query: &KlineQuery, options: &OperationOptions) -> Result<KlineSeries>` |
| M27 | `KlineService::page_many(&self, ids: &[SecurityId], query: &KlineQuery, options: &OperationOptions) -> Result<BatchOutcome<KlineSeries>>` |
| M28 | `KlineService::collect_many(&self, ids: &[SecurityId], query: &KlineQuery, options: &OperationOptions) -> Result<BatchOutcome<KlineSeries>>` |
| M29 | `KlineService::delisted_page(&self, id: &SecurityId, query: &DelistedQuery, options: &OperationOptions) -> Result<KlineSeries>` |
| M30 | `KlineService::delisted_collect(&self, id: &SecurityId, query: &DelistedQuery, options: &OperationOptions) -> Result<KlineSeries>` |
| M98 | `KlineService::delisted_page_many(&self, ids: &[SecurityId], query: &DelistedQuery, options: &OperationOptions) -> Result<BatchOutcome<KlineSeries>>` |
| M99 | `KlineService::delisted_collect_many(&self, ids: &[SecurityId], query: &DelistedQuery, options: &OperationOptions) -> Result<BatchOutcome<KlineSeries>>` |
| M31 | `MinuteService::today(&self, id: &SecurityId, options: &OperationOptions) -> Result<MinuteSeries>` |
| M32 | `MinuteService::history(&self, id: &SecurityId, date: TradingDate, options: &OperationOptions) -> Result<MinuteSeries>` |
| M33 | `MinuteService::recent(&self, id: &SecurityId, query: &MinuteQuery, options: &OperationOptions) -> Result<MinuteSeries>` |
| M34 | `MinuteService::today_many(&self, ids: &[SecurityId], options: &OperationOptions) -> Result<BatchOutcome<MinuteSeries>>` |
| M35 | `MinuteService::history_many(&self, ids: &[SecurityId], date: TradingDate, options: &OperationOptions) -> Result<BatchOutcome<MinuteSeries>>` |
| M36 | `MinuteService::recent_many(&self, ids: &[SecurityId], query: &MinuteQuery, options: &OperationOptions) -> Result<BatchOutcome<MinuteSeries>>` |
| M37 | `MinuteService::aux(&self, id: &SecurityId, kind: AuxKind, options: &OperationOptions) -> Result<MinuteAuxSeries>` |
| M38 | `MinuteService::buy_sell_strength(&self, id: &SecurityId, options: &OperationOptions) -> Result<MinuteAuxSeries>` |
| M39 | `MinuteService::volume_comparison(&self, id: &SecurityId, options: &OperationOptions) -> Result<MinuteAuxSeries>` |
| M40 | `MinuteService::sparkline(&self, id: &SecurityId, query: &SparklineQuery, options: &OperationOptions) -> Result<SparklineSeries>` |

| 类型 | 必需字段与含义 |
| --- | --- |
| `KlineBar` | `time:EventTime, source_time:EventTime, open/close/high/low:Price, last_close:Option<Price>, volume:Quantity, amount:Amount, index_up/index_down:Option<u32>`；source_time保留结束标签平移前的原始交易所时刻，供Python旧同日回绕投影使用；保留原integer/f32位及尺度来源。index counts仅指数有，None不是0。 |
| `KlineSeries` | `id, period:KlinePeriod, kind:KlineKind, adjust:AdjustMode, anchor_date:Option<TradingDate>, time_label:BarTime, bars:DataPage<KlineBar>, pages_read:usize`；page与collect同类型，但后者end必须完整。 |
| `MinutePoint` | `index:u32, time:EventTime, price:Price, volume:Quantity, average:Option<Price>`；今天/历史/最近格式未提供平均价时None，time不把未有日期的点伪装成带UTC日期。 |
| `MinuteSeries` | `id, trading_date:Option<TradingDate>, previous_close:Option<Price>, open:Option<Price>, points:Vec<MinutePoint>, meta`；完整单响应；旧volume_sum保留为同单位Quantity求和。 |
| `MinuteAuxPoint` | `index:u32, time:EventTime, values:AuxValues`。 |
| `AuxValues` | `BuySell { buy_commission:i64, sell_commission:i64 } / Volumes { previous:f32, current:f32, cumulative:f32 }`；字段为累计量，保留wire浮点；未取证单位不标股。 |
| `MinuteAuxSeries` | `id, kind:AuxKind, points:Vec<MinuteAuxPoint>, meta`。 |
| `SparklineSeries` | `id, selector:u8, selector_echo:u16, requested_window:u16, max_count:u16, base:Price, prices:Vec<Price>, reserved:u32, meta`；echo不符不能悄悄解释为请求selector。 |

普通K线collect按请求offset递增，读取到空页，反转**页序**保留页内顺序；不是按时间去重。退市collect按旧行为做稳定time升序，重复time仍保留。Python普通collect聚合raw_payload清空，而退市保留首页raw；桥接保持此差异，Rust统一页级RawData不代表统一旧返回。秒级period13、参数化period8/9与category22须分开；22只属后续资金流接口，不返回KlineBar。

分时三个命令0537/0FB4/0FEB各自解析，不能混用价基准/平均价字段。混合资产volume单位依协议证据，转债逐笔价格另见第7节。任何分页cap、无进展、返回越界都终止，超时/取消不返回前几页冒充完整结果。

## 7. 逐笔、竞价与HQ金额流（T14）

旧Python签名族（S03）：today(code,*,start=0,count=1800,include_raw=False,batch_size=None)、history(code,trading_date,同参数)；all_today/all_history将start/count换成page_size1800/max_pages100；today_batch/history_batch及all_*_batch为同参数列批量视图；opening_match_today/history用page_size1800/max_pages100。auctions.series(code,date=None,*,include_raw=False)。money_flow.daily(code,*,include_raw=False,batch_size=75)是0FFC；其history为F10，排除本域。

Rust以TradeQuery.date显式分派今日/历史，避免十余个仅日期位置不同的方法；下表覆盖全部旧族，不改变Python名。

| ID | 拟议签名 |
| --- | --- |
| M41 | `TradeService::page(&self, id: &SecurityId, query: &TradeQuery, options: &OperationOptions) -> Result<TradePage>` |
| M42 | `TradeService::collect(&self, id: &SecurityId, query: &TradeQuery, options: &OperationOptions) -> Result<TradePage>` |
| M43 | `TradeService::columns(&self, id: &SecurityId, query: &TradeQuery, options: &OperationOptions) -> Result<TradeColumns>` |
| M44 | `TradeService::collect_columns(&self, id: &SecurityId, query: &TradeQuery, options: &OperationOptions) -> Result<TradeColumns>` |
| M45 | `TradeService::opening_match(&self, id: &SecurityId, query: &TradeQuery, options: &OperationOptions) -> Result<Option<TradeTick>>` |
| M46 | `TradeService::page_many(&self, ids: &[SecurityId], query: &TradeQuery, options: &OperationOptions) -> Result<BatchOutcome<TradePage>>` |
| M47 | `TradeService::collect_many(&self, ids: &[SecurityId], query: &TradeQuery, options: &OperationOptions) -> Result<BatchOutcome<TradePage>>` |
| M48 | `TradeService::columns_many(&self, ids: &[SecurityId], query: &TradeQuery, options: &OperationOptions) -> Result<BatchOutcome<TradeColumns>>` |
| M49 | `TradeService::collect_columns_many(&self, ids: &[SecurityId], query: &TradeQuery, options: &OperationOptions) -> Result<BatchOutcome<TradeColumns>>` |
| M50 | `TradeService::opening_match_many(&self, ids: &[SecurityId], query: &TradeQuery, options: &OperationOptions) -> Result<BatchOutcome<Option<TradeTick>>>` |
| M51 | `AuctionService::series(&self, id: &SecurityId, query: &AuctionQuery, options: &OperationOptions) -> Result<AuctionSeries>` |
| M52 | `MoneyFlowService::daily_many(&self, ids: &[SecurityId], options: &OperationOptions) -> Result<BatchOutcome<DailyMoneyFlow>>` |
| M100 | `MoneyFlowService::daily(&self, id: &SecurityId, options: &OperationOptions) -> Result<DailyMoneyFlow>` |

| 类型 | 必需字段与含义 |
| --- | --- |
| `TradeTick` | `index:u32, absolute_index:u32, time:EventTime, price:Price, volume:Quantity, order_count:i64, status_raw:i64, side:TradeSide, kind:TradeEventKind, auction_matched:Option<Quantity>, auction_unmatched_signed:Option<i64>, unknown_tail:Option<i64>`；另保留price_acc_raw/price_delta_raw/price_denominator/reserved_zero/source getters，即使Omit仍可追溯；当前mixed parser两个auction专属量可None，不能从status盲算。 |
| `TradePage` | `id, trading_date:Option<TradingDate>, price_base:Option<Price>, ticks:DataPage<TradeTick>, pages_read:usize`。 |
| `TradeColumns` | `id, trading_date:Option<TradingDate>, start:u32, end:EndEvidence, next_cursor:Option<PageCursor>, len:usize, columns:TradeColumnData, meta`；列长度一致，cursor与对应TradePage同义；`tick(index)->Result<TradeTick>`、`to_page()->TradePage`为纯转换，不联网。 |
| `TradeColumnData` | 与TradeTick一一对应的列：`indices/absolute_indices:Vec<u32>, times:Vec<EventTime>, prices:Vec<Price>, volumes:Vec<Quantity>, order_counts/statuses:Vec<i64>, sides:Vec<TradeSide>, kinds:Vec<TradeEventKind>, auction_matched:Vec<Option<Quantity>>, auction_unmatched_signed/unknown_tails:Vec<Option<i64>>`，以及原价/差分/分母/source列；无Any。 |
| `AuctionPoint` | `index:u32, time:EventTime`（保留秒）、`price:Price, matched:Quantity, unmatched_signed:i64, unmatched:Quantity, direction_raw:i64`，另保留minute_of_day_raw/second_raw/reserved；matched amount仅估算，Price×已证实单位换算，不伪装实际成交额。 |
| `AuctionSeries` | `id, trading_date:Option<TradingDate>, mode_selector:u32, start:u32, requested:u32, points:Vec<AuctionPoint>, meta`；底层默认selector3/start0/limit500，当前/历史由日期字段决定，不将selector缩到u16。 |
| `DailyMoneyFlow` | `id, records:Vec<MoneyFlowDay>, meta`；零记录是合法空业务数据，不自动Missing。 |
| `MoneyFlowDay` | `date:Option<TradingDate>, date_raw:u32, total_amount:Amount, buckets:[u16;16], raw_values:[u32;21], main_net/main_buy_net:Amount, main_ratio/main_buy_ratio:Ratio, main_size_net/main_buy_size_net:[Amount;4]`；数组顺序超大/大/中/小，ratio为百分数；raw_values为未完全命名的业务字段，即使Omit仍保留。 |

逐笔完整collect反转页序但不排序页内，同分钟多笔必须稳定，保留absolute_index与价格累加基准。转债逐笔沿用milli-yuan，不能套quote decimal4。opening_match逐页找首个非status8的09:25正式撮合；只有已EOF而未找到才Ok(None)，到cap仍Err。side原值0/1/2分别Buy/Sell/Neutral，其他保留Status；status5且15:05–15:30是盘后定价成交，仍属于实际成交，但没有Buy/Sell方向时不硬计入资金流。0x056A使用专属资源池/受owner控制，秒不截分钟。

0FFC的 `MoneyFlowService::daily` 对应单个身份，`daily_many` 保留批量occurrence顺序，每个wire仍只发一个身份；调度工作块最多75项，实际并发不超过owner池/连接预算，不创建75个隐含连接。单值与批量共用同一算法，单值不是先造含一项的公开BatchOutcome再暴露给用户。空结果可按代码有限探测：每代码至多3个备用主机、整批至多min(n,8)×3，且受Rust额外32/总8秒更紧约束。A全空不能断言B空；确认源无数据返回空records，连接/协议错误仍按该域既定规则处理，不填零数据。Python保留旧次数作用域和fallback，不套Rust全局预算。当前Python today_batch在native socket/pool会抛 `ValueError("trade batch only supports historical ticks")`，仅execute的custom transport可回落。新Rust可新增合法今日列转换，但必须独立测试，桥接不能顺带消除旧报错。

## 8. Corporate、equity 与价格限制（T16）

旧事实（S04/S06）：capital_changes(code,*,include_raw=False,batch_size=75)；adjustment_factors(code,anchor_date=None,*,start_date=None,batch_size=75)；finance_batch(codes,fields=None,*,include_raw=False,on_missing="raise")。纯函数build_adjustment_factor_response(changes,*,anchor_date=None,start_date=None)。limits.special(start_index=0)；scan_special(start_index=0,max_rows=10000)。

| ID | 拟议签名 |
| --- | --- |
| M53 | `CorporateService::capital_changes(&self, ids: &[SecurityId], query: &CapitalQuery, options: &OperationOptions) -> Result<BatchOutcome<CapitalChangeBlock>>` |
| M54 | `CorporateService::adjustment_factors(&self, ids: &[SecurityId], query: &FactorQuery, options: &OperationOptions) -> Result<BatchOutcome<AdjustmentFactors>>` |
| M55 | `CorporateService::finance(&self, ids: &[SecurityId], query: &FinanceQuery, options: &OperationOptions) -> Result<BatchOutcome<FinanceRecord>>` |
| M56 | `CorporateService::finance_fields(&self, ids: &[SecurityId], fields: &[FinanceField], query: &FinanceQuery, options: &OperationOptions) -> Result<BatchOutcome<FinanceProjection>>` |
| M57 | `eltdx::equity::build_adjustment_factors(changes: &CapitalChangeBlock, query: &FactorQuery) -> Result<AdjustmentFactors>` |
| M58 | `LimitService::special(&self, query: &SpecialLimitQuery, options: &OperationOptions) -> Result<DataPage<SpecialLimit>>` |
| M59 | `LimitService::scan_special(&self, query: &SpecialLimitQuery, options: &OperationOptions) -> Result<Collected<SpecialLimit>>` |

| 类型 | 必需字段与含义 |
| --- | --- |
| `CapitalChangeBlock` | `id, records:Vec<CapitalEvent>, declared_blocks:u16, meta`；空events不等于缺证券。 |
| `CapitalEvent` | `date:Option<TradingDate>, date_raw:u32, category:u8, category_name:Option<String>, coefficients:[f64;4], coefficient_bits:[[u8;4];4], reserved:u8`；tag1 c1现金分红元/10股，c2配股价元，c3送转股数/10股，c4配股数/10股；其他tag保留原值，不冒用tag1单位。 |
| `AdjustmentFactors` | `id, anchor_date/start_date:Option<TradingDate>, rows:Vec<AdjustmentFactor>, meta`。 |
| `AdjustmentFactor` | `date:TradingDate, qfq_scale/qfq_offset/hfq_scale/hfq_offset:f64`；scale无量纲、offset为价格单位；不是单一乘数。 |
| `FinanceRecord` | `id, province:u16, industry:u16, updated/ipo:Option<TradingDate>, values:Vec<(FinanceField, FinanceValue)>, meta`；完整固定wire字段集合，不因选fields漏解后续offset。 |
| `FinanceField` | `CirculatingShares, TotalShares, StateShares, FounderLegalShares, LegalShares, BShares, HShares, Eps, TotalAssets, CurrentAssets, FixedAssets, IntangibleAssets, ShareholderCount, CurrentLiabilities, LongTermLiabilities, CapitalReserve, NetAssets, MainRevenue, MainProfit, Receivables, OperatingProfit, InvestmentIncome, OperatingCashFlow, TotalCashFlow, Inventory, TotalProfit, AfterTaxProfit, NetProfit, UndistributedProfit, NetAssetsPerShare, Reserved2`；province/industry/date单独字段。 |
| `FinanceValue` | `Shares(Quantity) / Money(Amount) / PerShare(Price) / Count(f64) / Unclassified { value:f32, bits:[u8;4] }`；至少circulating/total原万股×10000股，total_assets/net_profit原千元×1000元；其余字段须逐一验单位，不能从上述两项推全体。 |
| `FinanceProjection` | `id, fields:Vec<(FinanceField,FinanceValue)>, updated/ipo:Option<TradingDate>, meta`；按fields请求顺序，可空fields返回仅identity/date，重复field保留；不接受任意属性字符串。 |
| `SpecialLimit` | `id:Option<SecurityId>, market_raw:u8, code_raw:u32, upper/lower:Price, meta`；未知market可保留记录，不伪造SecurityId；Price保留原f32位。 |

capital批量验证返回为请求剩余序列的严格前缀；返回2个就只移除这2个后继续，0个/外来id/错位前缀Protocol。调整只取合法日期tag1；同日事件保持源顺序。`m=(10+c3+c4)/10, o=(c1-c4*c2)/10`，m<=0为InvalidArgument；qfq更新scale/=m、offset=(offset-o)/m，hfq逆时间序scale*=m、offset=m*offset+o。返回日期升序，不额外按2位小数舍入。anchor<start拒绝。

special扫描空页结束，真实记录数到cap仍未EOF为LimitExceeded，不能按页估计。旧scan_special会按while边界停且可能越过max_rows；Python保留旧结果，新Rust完整扫描不假称完整。价格政策见第12节，低层special报价不等同于全国通用交易规则。

## 9. Calendar（T16）

旧WorkdayService(client=None,benchmark_code="sh000001")的today/normalize/text/same_day为纯日期工具；refresh从指数all_pages日线建sorted set，is_workday、range(start,end,descending=False)、next/previous(value=None,include_self=False)惰性载入；无client是工作日近似，已绑定client被回收则ReferenceError，不降级。新Rust采用持强owner的CalendarService，不需要弱引用回收行为。

| ID | 拟议签名 |
| --- | --- |
| M60 | `CalendarService::today(&self) -> Result<TradingDate>` |
| M61 | `CalendarService::refresh(&self, query: &CalendarQuery, options: &OperationOptions) -> Result<CalendarSnapshot>` |
| M62 | `CalendarService::is_workday(&self, date: TradingDate, query: &CalendarQuery, options: &OperationOptions) -> Result<bool>` |
| M63 | `CalendarService::range(&self, start: TradingDate, end: TradingDate, descending: bool, query: &CalendarQuery, options: &OperationOptions) -> Result<Vec<TradingDate>>` |
| M64 | `CalendarService::next(&self, date: TradingDate, include_self: bool, query: &CalendarQuery, options: &OperationOptions) -> Result<Option<TradingDate>>` |
| M65 | `CalendarService::previous(&self, date: TradingDate, include_self: bool, query: &CalendarQuery, options: &OperationOptions) -> Result<Option<TradingDate>>` |
| M66 | `CalendarService::clear_cache(&self) -> ()` |

| 类型 | 必需字段与含义 |
| --- | --- |
| `CalendarSnapshot` | `benchmark:SecurityId, days:Vec<TradingDate>, coverage_start/coverage_end:Option<TradingDate>, complete:bool, meta`；空calendar不能声称所有日期休市，调用is_workday需有效覆盖。 |

today为注入时钟的Shanghai日历日，不表示一定开市。T16实施修正其返回为Result以传播既有Clock trait的错误和日期溢出，不panic或伪造日期；这不改变旧Python签名。TradingDate构造/Display/相等比较代替Python normalize/text/same_day，不接受任意Python对象。range含两端，start>end交换再按descending排序。查询覆盖外日期不能简单false；返回MetadataUnavailable并携带覆盖范围。next/previous在有效快照内无后继/前驱返回None并保留快照边界语义，不预测未来开市。无client的weekday近似如后续需要应另立纯函数，不能作为网络calendar自动fallback。

## 10. 资源、公司资料、session 与health（T17）

旧resources.read(path,*,offset=0,size=30000)、block_meta(filename)、download_file(path,*,chunk_size=30000,max_bytes=None)及read_stats(path="zhb.zip",*,chunk_size=30000)。公司category(code,market)、content(code,market,filename,offset,length)。session.ping/handshake/heartbeat无参数；health.ping(client=None)默认in_memory；**旧ping只返回本地pong，不是远端网络活性测量**。

| ID | 拟议签名 |
| --- | --- |
| M67 | `ResourceService::read(&self, path: &str, offset: u64, size: usize, options: &OperationOptions) -> Result<DownloadResult>` |
| M68 | `ResourceService::download_file(&self, path: &str, max_bytes: usize, options: &OperationOptions) -> Result<DownloadResult>` |
| M69 | `ResourceService::download_complete(&self, path: &str, max_bytes: usize, options: &OperationOptions) -> Result<Bytes>` |
| M70 | `ResourceService::block_meta(&self, filename: &str, options: &OperationOptions) -> Result<BlockMeta>` |
| M71 | `ResourceService::read_stats(&self, query: &StatsQuery, options: &OperationOptions) -> Result<StatsResource>` |
| M72 | `CompanyInfoService::category(&self, id: &SecurityId, options: &OperationOptions) -> Result<Table<CompanySection>>` |
| M73 | `CompanyInfoService::content(&self, id: &SecurityId, query: &CompanySectionQuery, options: &OperationOptions) -> Result<CompanyText>` |
| M74 | `SessionService::ping(&self) -> Result<PingReply>` |
| M75 | `SessionService::handshake(&self, options: &OperationOptions) -> Result<HandshakeInfo>` |
| M76 | `SessionService::heartbeat(&self, options: &OperationOptions) -> Result<HeartbeatAck>` |
| M77 | `HealthService::ping(&self) -> Result<PingReply>` |
| M78 | `HealthService::probe(&self, options: &OperationOptions) -> Result<HealthReport>` |

M67–69与主规格逐字同契约：`DownloadResult { pub data: Bytes, pub completion: DownloadCompletion, pub next_offset: Option<u64> }`，`DownloadCompletion::{Complete,LimitReached}`。这两个是复用类型，不另定义ResourceResult。到size/max_bytes不是EOF；0字节上限不联网、空data/LimitReached/Some(start)。Complete须空/合法短块等证据；strict达到cap未证明EOF为LimitExceeded。单块size0..60000，offset和offset+size必须可用wire u32；下载max_bytes显式，非零最多u32::MAX且内存分配受独立工作量限制。普通download_file用30000分块；无更改chunk_size的隐含全局配置。多块固定pin/host，长度不符/换主机/取消/总期限为Err。next_offset不是跨会话恢复保证。

| 类型 | 必需字段与含义 |
| --- | --- |
| `BlockMeta` | `size:u32, md5:String, meta`；MD5为源完整性字段，不宣传安全签名。filename非空ASCII、无NUL、<=40bytes。 |
| `CompanySection` | `name:String, filename:String, offset:u32, length:u32`；GBK目录，offset/length是字节范围，不是字符号。 |
| `CompanyText` | `id, filename:String, offset:u32, requested_length:u32, text:String, decoding_had_errors:bool, meta`；损坏GBK的旧丢字策略由bridge保持，Rust记录有损诊断；协议截断仍Protocol。 |
| `StatsResource` | `stat:Vec<StatRow>, stat2:Vec<Stat2Row>, source_path:String, meta`；按 `(market,code)` 唯一，row lookup不联网。 |
| `StatRow` | `id, stats_date:Option<TradingDate>, beta_60d/pe_ttm:Option<f64>, free_float_shares:Option<Quantity>, year_limit_up_days/limit_stat_days/limit_up_count/limit_streak:Option<u32>`。 |
| `Stat2Row` | `id, stats_date:Option<TradingDate>, amount/seal/previous_amount/previous_seal/previous2_amount/previous2_seal/open_amount/previous_open_amount:Option<Amount>, open_volume/previous_open_volume:Option<Quantity>`；旧万字段转元×10000，volume为手。 |
| `PingReply` | `message:String="pong", remote_verified:bool=false`；局部可用性，已closed可读诊断不隐式connect。 |
| `HandshakeInfo` | `server_datetime:Option<EventTime>, server_date1/server_date2:Option<TradingDate>, session_minutes1/session_minutes2:Vec<String>, server_name/product_tag:String, unknown_time1/unknown_time2:Option<u32>, flags/control_tail:Bytes, meta`；flags为控制语义不受Omit移除。 |
| `HeartbeatAck` | `server_date:Option<TradingDate>, server_date_raw:u32, reserved:Bytes, meta`。 |
| `HealthReport` | `heartbeat:HeartbeatAck, elapsed:Duration, endpoint:SocketAddr, meta`；probe明确发一次heartbeat，测量一次操作耗时，不承诺全pool都健康。 |

path为trim后非空ASCII、无NUL、<=300bytes，反斜杠规范为正斜杠；只是服务端path，不等于允许在本机写同名文件。Company filename trim后GBK编码1..80字节，目录name64字节/filename80字节/offset与length各u32；长度按字节而非Unicode字符计。旧编码遇不可编码字符的处置保留在Python桥接，新Rust拒绝有损文件名；不从传入路径扫描磁盘。stats采用32MiB+1探测，在展开前拒绝超限；必须存在唯一tdxstat.cfg与tdxstat2.cfg，拒绝重复、加密、展开超限。解析两表本身不要求同日期；供shortline/daily shares使用时才验主日期与覆盖门槛。旧Python download_file的max_bytes截取bytes/None语义不变。

T17实施保护：保留逐响应来源的Rust资源下载最多65536个响应页，所保留payload及wire frame总计最多128MiB；即使content很短，合法未知尾部也计入原始缓冲区上限。达到这些独立工作量限制返回LimitExceeded并清理pin，不误标EOF。这些限制不应用于Python legacy路径。StatRow/Stat2Row的id getter为Option<&SecurityId>，未知market/code继续保留source_market/source_code；日期保留原字符串，计数和单位转换不可表示时为None并记录诊断，原始有限数值/十进制计数字符串仍可读。受限纯整数统计和原金额字段不被替换成零。

## 11. 完整行情、目录、排行与profile helpers（T18）

旧签名（S05）：full_quotes(codes)；latest_stock_list/latest_st/latest_suspended(market=None)；buy_sell_strength/volume_comparison(code,include_raw=False)；realtime_rank(category="沪深A股",sort_by="涨幅",count=80,ascending=False)；stock_profile_table(codes,include_security=True,include_finance=True)。原stock_realtime_rank为alias。

| ID | 拟议签名 |
| --- | --- |
| M79 | `HelperService::full_quotes(&self, ids: &[SecurityId], options: &OperationOptions) -> Result<BatchOutcome<FullQuote>>` |
| M80 | `HelperService::latest_stock_list(&self, market: Option<HqMarket>, options: &OperationOptions) -> Result<Collected<Security>>` |
| M81 | `HelperService::latest_st(&self, market: Option<HqMarket>, options: &OperationOptions) -> Result<Collected<Security>>` |
| M82 | `HelperService::latest_suspended(&self, market: Option<HqMarket>, options: &OperationOptions) -> Result<Collected<Security>>` |
| M83 | `HelperService::buy_sell_strength(&self, id: &SecurityId, options: &OperationOptions) -> Result<MinuteAuxSeries>` |
| M84 | `HelperService::volume_comparison(&self, id: &SecurityId, options: &OperationOptions) -> Result<MinuteAuxSeries>` |
| M85 | `HelperService::realtime_rank(&self, query: &RankQuery, options: &OperationOptions) -> Result<Table<RealtimeRankRow>>` |
| M86 | `HelperService::stock_profile_table(&self, ids: &[SecurityId], query: &ProfileQuery, options: &OperationOptions) -> Result<Table<StockProfile>>` |
| M87 | `HelperService::clear_cache(&self) -> ()` |

| 类型 | 必需字段与含义 |
| --- | --- |
| `FullQuote` | `snapshot:Snapshot, depth_status:EnrichmentStatus, depth_source:Option<SourceStamp>, diagnostics:Vec<Diagnostic>`；snapshot主结果，更新两侧各至少5档才以refresh替换bids/asks及open_amount，未足5档保留原值并明示Unavailable。 |
| `RealtimeRankRow` | `rank:usize, id, name:Option<String>, last/pre_close:Price, change_pct:Option<Ratio>, amount:Amount, volume:Quantity, opening_rush:Ratio, seal_amount:Amount, quote:CategoryQuote`；rank全结果从1累加。 |
| `StockProfile` | `id, name:Option<String>, category:Option<SecurityCategory>, board:Option<SecurityBoard>, last/pre_close/open/high/low/change:Option<Price>, change_pct:Option<Ratio>, volume:Option<Quantity>, amount/open_amount:Option<Amount>, circulating/total_shares:Option<Quantity>, turnover:Option<Ratio>, circulating/total_market_value:Option<Amount>, eps:Option<Price>, ipo/updated:Option<TradingDate>, security:Option<Security>, quote:FullQuote, finance:Option<FinanceRecord>, meta`。 |

full_quotes每80个snapshots后可选depth，按identity合并；主snapshots缺失/损坏严格失败，depth有效范围内失败可降级，Rust总超时/取消仍Err。profile所有字段都来自HQ quote+目录+finance，无F10依赖；quote主来源严格，security/finance为显式可选源，失败时对应None+diagnostic。turnover%=volume手×100/流通股×100，market_value元=股×价；零股本分母None。Python _finance_map按有序code tuple缓存，80一批、捕获Exception可缓存空/部分；保留旧行为，不借新的成功缓存策略修bug。Rust缓存需来源状态/版本，clear只清内存不删文件、不关闭session。

rank按服务端返回顺序，不客户端重新排名；Some(count)达目标标RequestedWindow，None到合法空/经该command确认短页EOF；同页/页间重复不能无证据去重。name目录是可选补充。helper.latest_suspended返回Security行，而codes.latest_suspended返回SecurityId，两个现存Python外观不同；Rust同样明确类型。

T18源码接口补充：`Table<T>::end() -> Option<EndEvidence>`暴露组合收集器的结束证据；rank达到Some(count)为Some(RequestedWindow)，真正空页为Some(EmptyPage)。普通非分页table（如公司资料目录）为None。当前category的短页尚无独立EOF证明，Rust继续到空页或显式有限上限，不把短页当完整数据。此为尚未发布Rust入口的新增只读方法，不更改Python结果外观；实现与用例尚未执行验证。

T18 profile/cache源码补充：`ProfileQuery::new()`默认启用security/finance，`with_security/with_finance`可关闭；StockProfile增加security_status/finance_status只读状态。两补充来源按输入occurrence关联且保留原DTO，零/非法股本及非有限EPS不产出错误派生字段。Helper缓存归属共享owner，目录按market、财务按完整有序occurrence tuple，再匹配本地加载日、metadata版本和RawPolicy；完整或确认缺失缓存900秒，失败/部分失败30秒，每源至多16项/8192行或occurrence权重，超大返回但不缓存。local load date不证明服务端交易日；不把旧stamp重写成当前来源。clear只清helper内存，通过generation拒绝清理前在途发布，calendar/precision/session/用户文件保持各自职责。目录扫描使用实际发布版本作缓存键；financial collector不得重设已发请求的外层limits。这是源码约定，未执行验证；Python旧缓存尚未切换。

## 12. 每日股本、涨跌停、短线与天梯（T16、T18）

旧daily_share_capital(codes=None,stats_path="zhb.zip",refresh_stats=False)，daily_price_limits(codes=None,*,trade_date)；shortline_indicators(codes,stats_path="zhb.zip",refresh_stats=False)；limit_ladder(codes=None,include_touched=False,count=None)。daily_shares/stock_daily_price_limits/stock_limit_ladder只是Python别名。

| ID | 拟议签名 |
| --- | --- |
| M88 | `HelperService::daily_share_capital(&self, query: &DailySharesQuery, options: &OperationOptions) -> Result<Table<DailyShareCapital>>` |
| M89 | `HelperService::daily_price_limits(&self, query: &DailyLimitsQuery, options: &OperationOptions) -> Result<Table<DailyPriceLimit>>` |
| M90 | `HelperService::shortline_indicators(&self, query: &ShortlineQuery, options: &OperationOptions) -> Result<ShortlineTable>` |
| M91 | `HelperService::limit_ladder(&self, query: &LadderQuery, options: &OperationOptions) -> Result<Table<ShortlineIndicator>>` |

| 类型 | 必需字段与含义 |
| --- | --- |
| `DailyShareCapital` | `id, trade_date:TradingDate, total/circulating/free_float:Option<Quantity>, finance_updated:Option<TradingDate>, sources:Vec<String>, meta`；股，free_float来自stat万股×10000，缺省None，不与circulating互相替补。 |
| `DailyPriceLimit` | `id, trade_date:TradingDate, name:Option<String>, reference_date:Option<TradingDate>, reference:Option<Price>, upper/lower:Option<Price>, rule:LimitRule, status:LimitStatus, reference_source:Option<String>, meta`。 |
| `ShortlineTable` | `ids:Vec<SecurityId>, target/previous/stats_date:TradingDate, stats_path:String, stats_refreshed:bool, rows:Vec<ShortlineIndicator>, meta`。 |
| `ShortlineIndicator` | `id, target/previous:TradingDate, stats_date:Option<TradingDate>, alignment:StatsAlignment, limit_status:LimitStatus, beta_60d/pe_ttm:Option<f64>, free_float_shares/float_shares:Option<Quantity>, previous_amount/previous_seal/previous2_seal/previous_open_amount:Option<Amount>, previous_open_volume:Option<Quantity>, limit_stat_days/limit_up_count/limit_streak/year_limit_up_days:Option<u32>, free_float_market_value/float_market_value:Option<Amount>, open_turnover/open_previous_amount/open_previous_seal/seal_to_float/open_change:Option<Ratio>, auction_previous_volume/seal_previous/open_volume_ratio/seal_to_amount:Option<Ratio>, board_text:Option<String>, ladder_level:Option<u32>, open/pre_close:Option<Price>, open_amount/seal_amount:Option<Amount>, open_volume:Option<Quantity>, opening_rush:Option<Ratio>, meta`。 |
| `StatsAlignment` | `SameDay / PreviousTradingDay / RowMissing / DateUnaligned`；None金额要配此状态。 |

短线必需前提是可用handshake服务端datetime、双日期最大值作为target、calendar给出previous；server_datetime同target且<09:25则DataNotReady。两统计文件各自主日期覆盖>=95%、同主日期、日期仅target或previous；不满足ResourceFormat/日期细分，不能拿本机今日或另一个文件补齐。source已成功但单证券行缺失是字段None+RowMissing，非整档案失败。前一交易日的stat2采用amount→previous_amount、seal→previous_seal、previous_seal→previous2_seal、open→previous_open；当天则用原previous列。

量价计算：open_volume手=open_amount元/open价/100；free_float_market_value=股×last；open_turnover%=open_volume手/(free_float股/100)×100；open_previous_amount和open_previous_seal为百分数；auction_previous_volume、seal_previous、seal_to_amount为普通比例；seal_to_float为百分数。5个先前日bars的volume手总和/(240×5)为平均每分钟手数，open_volume除之为open_volume_ratio，少于5个None。所有零分母None；Python保留round6。opening_rush只对>=80证券进行code排序category补扫，最多200页×80；小请求不触发全市场扫。bars/finance/opening-rush是可选补充，状态与缺失按来源记录，Rust取消/总期限仍终止。

天梯只取Sealed或显式include_touched的Touched，按ladder降序、seal_amount降序、id升序排序；count只截最终榜。上一日stats且当日封板，ladder为max(1,previous_streak+1)，当天stats为max(1,streak)；board_text相应累计天数/板数。不要将旧源字段与派生字段混写。

daily_price_limits用选定trade_date之前最近**不复权**日bar作reference，再按当日tag1资本事件顺序作 `(reference-o)/m`；要求交易日，缺价为MissingReference。新Rust规则必须是按生效日期冻结的Approved，证据不足UnknownRule，不能默认猜10%。现存Python冲突必须保留到独立修复决策：core.py普通主板ST仍10%、任意sz30前缀20%、IPO统一前5交易日；shortline.py ST优先5%、N/C名字不设限制。这些不能直接合成一套“已正确”的policy。新Rust精确涨跌停取整方法须与规则版本一同批准，未批准不能宣称可交易价。

T18 M88/M89源码补充（未执行）：SecuritySelection::Explicit保留重复及顺序，Explicit(empty)在准入后不读clock/日历/目录/会话/资源。M88 trade_date为服务端target；接受previous日stats时保留其源日诊断，不重标当日。双表主日期各覆盖>=95%（缺失/非法日计入分母），且同日、属于target或previous；普通格式/双表不同日为ResourceFormat，共同日期超出接受范围附code=stats_date_mismatch。只缓存已对齐成功的stats，key含完整StatsQuery/target/previous/本地加载日/RawPolicy，900秒TTL、最多2项/65536权重（行数+保留raw字节/4KiB），超大返回但不存；refresh失败仍报错、clear隔离旧在途发布。M89需要有覆盖证据的交易日，完整不复权日线按最大前序源日选参考，同日冲突收盘报Protocol，按原序使用tag1系数；缺参考status=MissingReference同时保留Unknown rule，有参考仍UnknownRule且upper/lower=None。新内部resource/calendar/Kline/capital collector不重设外层limits、不finish父操作。Python尚未切换，不能据此声称兼容验证通过。

T18 M90/M91源码补充（未执行）：公开模型在types::shortline及根重导出；ShortlineQuery非空且保留occurrence，LadderQuery Explicit(empty)准入后直接返回。读取日历/finance前一次捕获全部输入及category需要的三个市场，避免首次decode冻结不完整metadata。前六日bar只选择五个不同的严格前序源日，未知/冲突单位不算零。分类补扫只在>=80个输入occurrence启动，固定首endpoint、最多200x80、重复身份页检测，不以短页判断EOF；中途失败保留已匹配补充，未找到者Failed带error_kind，不能声称确证缺失。通用Snapshot.amount仍是Unknown单位的来源，不强制重标CNY，此类seal_to_amount为None且有诊断；其余元/股/手及百分数/比例按本节公式。计数使用各stat自己的对齐日期作派生，源计数不覆写。approved policy仍空，实际limit_status为UnknownRule且ladder_level=None，天梯排除未知行并明确诊断未分类数；空输出不是无涨停的证明。封板/触板算术仅在人工approved价用例中覆盖，不注册真实交易所规则。Python round6、旧规则/金额投影由后续adapter保留。全部实现/用例未编译或运行。

## 13. 竞价组合、市场统计与逐笔资金流（T14、T18）

旧auction_data(code,date=None,include_series=True,include_snapshot=True,include_quote=True,pre_close_price=None)；market_stat()；today_fund_flow(code)；纯classify_fund_flow(ticks)。

| ID | 拟议签名 |
| --- | --- |
| M92 | `HelperService::auction_data(&self, id: &SecurityId, query: &AuctionDataQuery, options: &OperationOptions) -> Result<AuctionData>` |
| M93 | `HelperService::market_stat(&self, options: &OperationOptions) -> Result<MarketStat>` |
| M94 | `HelperService::today_fund_flow(&self, id: &SecurityId, options: &OperationOptions) -> Result<FundFlow>` |
| M95 | `eltdx::helpers::classify_fund_flow(ticks: &[TradeTick]) -> Result<FundFlow>` |

| 类型 | 必需字段与含义 |
| --- | --- |
| `AuctionData` | `id, date:TradingDate, series:Option<AuctionSeries>, opening_match:Option<TradeTick>, pre_close/open:Option<Price>, open_volume:Option<Quantity>, open_amount:Option<Amount>, open_change:Option<Ratio>, meta`；未请求与请求但未找到由diagnostics区分。 |
| `MarketStat` | `up/down/neutral/suspended_or_unclassified/total/limit_up/limit_down:u64, total_amount:Amount, total_volume:Quantity, market_cap:Amount, meta`；剩余项只是近似停牌/未参与，不作确证停牌数。 |
| `FundFlow` | `super_in/large_in/medium_in/small_in/super_out/large_out/medium_out/small_out:Amount, main_net/total_net:Amount, classified_count/skipped_count:usize, meta`；元，分类是L1成交大小，不是真实机构订单流。 |

auction current日期取服务端，历史显式date。显式pre_close优先；历史include_quote时从0FC6 price_base取得，不取当前quote污染历史；当天用snapshot.pre_close。历史opening_match可从全日trade结果获取；无正式撮合则None，不能用09:24最近点补充。用户关闭全部来源也可返回含date且所有选项None的结果，不产生假行情。

market_stat发legacy报价请求sh880005、sh880001、sh880006；Rust按id关联，必需伪指数缺失返回MissingData。880005 last/open/low/high各×10并按已冻结round为上涨/下跌/平盘/总数；剩余max(0,total-up-down-neutral)，amount为元、volume保留手；880001 last×1e10元；880006 last/open×10为涨停/跌停。Python旧按位置、缺少后两项补0行为保留，乱序错位修复单列，不伪称旧对照全等。

fund_flow忽略AuctionSnapshot，包含OpeningMatch与盘后真实成交；side只Buy/Sell参与，其余skip。金额元=price×volume手×100；严格阈值 `>1,000,000` 超大、`>200,000` 大、`>40,000` 中，其余小。空ticks八桶和净额皆0；未知volume单位/非有限或负金额不能强制归小桶，Rust返回InvalidArgument。today_fund_flow必须完整取得当日trades，主分页失败不返回半桶结果；classify是纯函数无options，无I/O。

T18 M92–M94源码补充（未执行）：market_stat按id识别必需三项，合法缺项MissingData，foreign/多余occurrence为Protocol；counter采用decimal nearest-even round(value×10)，仅880005 amount按本节字段合同解释CNY，不改通用legacy amount的Unknown。today_fund_flow复用受控pin分页，只在EmptyPage后分类，并保留空EOF页raw及source。实际A股逐笔的手单位依既有code分类及明确的ordinary-equity手×100合同，provenance为PrefixFallback；债券等未取证资产与status8量保持Unknown。auction_data当前日取握手双日期最大值（双日期均无效才用服务端datetime的日期，仍无效则MetadataUnavailable），显式历史不加载当前行情；显式pre_close优先，0保留但open_change=None。开启的主来源失败终止，关闭/合法空/未找到正式match以diagnostics区分；全部关闭且显式历史不联网，Current仍须服务端日期。聚合保留原source日期，不将握手旧日期重写为target。open_change为PercentagePoints；未知opening单位仅省略派生金额，不伪造股数。这些不是已测试或生产单位已验证声明，Python尚未切换。

## 14. 板块文件与成员行情（T17、T18）

旧HelperApi可配置board_data_dir/board_definitions_dir；board_quotes(category="概念",refresh=False)，board_member_quotes(board_code,refresh=False)。BoardService先按服务端市场日缓存，数据默认取环境ELTDX_BOARD_DATA_DIR或cwd/downloads，定义还尝试cwd/downloads与固定C盘路径；Python保持这些搜索与磁盘行为。Rust改为BoardQuery显式目录，构造client/query不扫描磁盘。

| ID | 拟议签名 |
| --- | --- |
| M96 | `HelperService::board_quotes(&self, query: &BoardQuery, options: &OperationOptions) -> Result<Table<BoardQuote>>` |
| M97 | `HelperService::board_member_quotes(&self, board: &BoardId, query: &BoardQuery, options: &OperationOptions) -> Result<BoardMemberQuotes>` |

| 类型 | 必需字段与含义 |
| --- | --- |
| `BoardDefinition` | `id:BoardId, name:String, market:HqMarket, category:BoardCategory, category_raw:u16, membership_key:String`；只支持有证据category映射，fallback默认市场必须记录推断。 |
| `BoardQuote` | `board:BoardDefinition, security_id:SecurityId, last/pre_close/open/high/low/change:Option<Price>, change_pct:Option<Ratio>, amount:Option<Amount>, volume:Option<Quantity>, quote:Option<Snapshot>, meta`；无行情是可选字段None+diagnostic，目录解析失败不是空榜。 |
| `BoardMember` | `id:SecurityId, excluded_reason:Option<String>`；None表示当期证券目录有效成员，排除原因至少absent_from_current_security_list。 |
| `BoardMemberQuotes` | `board:BoardDefinition, prepared_date:TradingDate, raw_members/display_members/excluded_members:Vec<BoardMember>, rows:Vec<BoardQuote>, meta`。 |

定义来源tdxzs.cfg/tdxzs3.cfg；只有infoharbor也可构建concept/style。HQ下载infoharbor_block.dat与tdxhy.cfg，Region还需明确取得base.dbf；当前Python下载列表没有base.dbf，不得声称服务端自动补齐。concept/style按header及实际去重成员数核对，行业按tdxhy列2或5的membership_key前缀，region按DBF字段/记录长度验证。一级/二级/三级行业key分别X+2/4/6digits，880行业category2、concept4、style5、region3。成员按第一次出现稳定去重，行情80一批按id合并；display以当前证券目录过滤，raw/excluded保留，不能删除排除事实。

DBF header/record length、deleted标记、field offset/width和编码均需有界；旧DBF set顺序不保证，Rust首次出现顺序为新增确定性选择，Python桥接原有顺序不偷偷修改。默认只有concept可用且资料不足时返回明确错误，不编造全类别。可选行情/更新下载失败允许使用**仍符合所选缓存策略**的已知来源并标状态，不能把上次日期标成今天；Rust未确认日期/哈希的磁盘缓存不宣称fresh。内存clear不删除用户文件。

## 15. 每域具体输入→输出语料建议

所有F例同时记录原请求、非空wire回包、原typed输出及Rust期望；伪造symbol用固定id即可，不需要在线查询。总deadline用虚拟时钟，不能用真实sleep拖测试。下面数字是可直接写入fixture的期望，不是行情预测。

| ID | 域/任务 | 输入与源响应 | 必须输出/错误 |
| --- | --- | --- | --- |
| F01 | codes T11 | SH page size2依次[A,B]、[C]、[] | 三行A/B/C、3页、EmptyPage；短页未提前停。 |
| F02 | codes T11 | 两页都[A,B]且offset增加 | Protocol无进展/重复页，不无限请求。 |
| F03 | quotes T11/T12 | 输入[A,B,A]、wire[B=20,A=10] | slots0=A10、1=B20、2=A10。 |
| F04 | missing T11 | batch[A,B]缺B、单B明确缺失 | strict MissingData(B)，AllowPartial两slot且B ConfirmedMissing。 |
| F05 | depth T12 | A snapshot一档/open_amount1，refresh两侧各5档/open_amount9；B只有一侧4档 | A用5档及9；B保留snapshot+Unavailable；不覆盖所有价格。 |
| F06 | subscribe T15 | 201证券需3pin，第二pin失败 | 已得pin全部释放；无活动subscription；成功路径201路由不跨订阅偷帧。 |
| F07 | gap T15 | 1025个事件填满默认1024队列 | Gap控制仍可见，ReportOnly标失同步；close释放pin。 |
| F08 | kline T13 | 新页[D3,D4]、旧页[D1,D2a,D2b]、空，其中D2两条同time不同值 | 反页序[D1,D2a,D2b,D3,D4]，不按值/time去重；重复合法记录保留。 |
| F09 | bar_time T13 | 5min 11:25/13:00、Day D1；End | 11:30/13:05，Day日期不变；只偏移一次。 |
| F10 | delisted T13 | 页内D4/D2，下一页D3/D1，空 | 稳定time升序D1..D4；raw页保留与Python首页规则分别断言。 |
| F11 | minutes T13 | 今日price100→110、volume2/3；历史无avg | 价按各命令尺度，volume同单位和5；average=None非0；未知date不套今日。 |
| F12 | aux T13 | VolumeComparison prev100.5/current120.5 | f32原值保留，累计量字段正确；非commission。 |
| F13 | trades T14 | 同分钟两trade、status8@09:25、status1@09:25、status5@15:10 | 两trade相对顺序不变，auction与opening区分，盘后真实成交保留。 |
| F14 | columns T14 | 同一非空历史page经rows/columns | 每列长度一致，tick(i)全字段相等；越界InvalidArgument。另录Python native今日列ValueError原样。 |
| F15 | opening T14 | 首page无撮合、第二page有09:25正式记录 | 返回该记录；只有扫到空页才None，max_pages1返回LimitExceeded。 |
| F16 | auction T14 | 09:24:59价10、09:25:00价10.01 | 两点秒级时刻都保留，不把09:24:59视为正式撮合。 |
| F17 | 0FFC T14 | A主/备均空，B主空备1非空 | A空records、B非空，B仍得到探测机会；各code3/批24/全局32均不超。 |
| F18 | capital T16 | request[A,B,C]回[A,B]再回[C] | 三block次序正确；第一回[B]为Protocol。 |
| F19 | equity T16 | tag1 D=1,P=0,S=1,R=0，一日期；anchor当日 | m1.1/o0.1，qfq scale10/11、offset-1/11；hfq1.1/0.1；anchor<start InvalidArgument。 |
| F20 | finance T16 | raw流通1.5万股、总2万股、资产3千元、净利4千元 | 15000股/20000股/3000元/4000元；未知其他项仍Unclassified。 |
| F21 | special T16 | 0452两条，用户max_records1，未EOF | Rust LimitExceeded，不能Complete；Python旧overshoot输出另存。 |
| F22 | calendar T16 | 指数days=[D1,D3]覆盖D1..D3，D2无bar | is_workday(D2)=false；查询D4 MetadataUnavailable；range(D3,D1)=D1/D3。 |
| F23 | read T17 | size3响应abc，无其他EOF证据 | dataabc/LimitReached/Some(3)；不是Complete。size0无wire。 |
| F24 | download T17 | max6，chunk3时abcdef；next read空但cap不许发 | bounded LimitReached；strict LimitExceeded；max7允许空探测则Complete；Pythonmax6仍b"abcdef"。 |
| F25 | stats T17 | ZIP缺stat2/重复stat/加密/33MiB/展开65MiB分别语料 | ResourceFormat或LimitExceeded；解压前拒绝可判上限；pin均回收。 |
| F26 | company T17 | filename为40个GBK“公”共80字节，正文“公司”，offset以字节2且length2；另41个“公”共82字节 | 80字节文件名可编码，正文返回“司”；82字节文件名在I/O前InvalidArgument；损坏GBK记录loss；长度声明越界Protocol。目录name固定64字节、filename固定80字节另断言。 |
| F27 | session T17 | 本地ping与延迟1ms heartbeat回server_date | ping remote_verified=false且无wire；probe有heartbeat/elapsed且日期不来自电脑。 |
| F28 | profile T18 | 价10、volume100手、流通10000股、总20000股；finance可选失败第二行 | 第一行turnover100%、市值100000/200000元；第二行finance相关None+Failed、quote保留；不发F10。 |
| F29 | rank T18 | category页B/A、下一页C，count3 | rank1B/2A/3C，不按id重排；终态RequestedWindow。 |
| F30 | daily_shares T18 | stat free_float1.5万股、finance流通2万股 | free_float15000股、circulating20000股、sources都记录；不把缺free_float等同流通。 |
| F31 | limits T16/T18 | trade_dateD2、D1close10、tag1D1元/10股、已批准10% | reference9.9；按批准round得upper10.89/lower8.91；未批准规则UnknownRule，旧主板ST10%另fixture。 |
| F32 | shortline T18 | server09:24:59；另两个文件主日期95/100与94/100 | 前者DataNotReady；后者ResourceFormat；95/100双方才达门槛。 |
| F33 | shortline T18 | previous stat2 amount2万、seal3万、prev_seal4万；开盘金额10000、开价10 | prev_amount20000、prev_seal30000、prev2_seal40000、open_volume10手；无当日列污染。 |
| F34 | ladder T18 | levels A2/seal100、B2/seal200、C1/seal500、TouchedD3 | 默认B,A,C；include_touched时D在首；count2只截最终排序。 |
| F35 | auction_data T18 | 历史price_base10、当前quote20、opening11 | pre_close10、open_change10%；不请求当前quote替历史。 |
| F36 | market_stat T18 | 伪指数乱序；880005=up10/down20/neutral5/total40，cap2、limit3/4 | 按id得100/200/50/400、剩余50、cap2e10、limit30/40；旧按位置另差分。 |
| F37 | fund_flow T14/T18 | 四笔Buy金额40000/200000/1000000/1000001；auction2e6、Neutral10 | small40000、medium200000、large1000000、super1000001；后两skip，不归流出；净额2440001。 |
| F38 | boards T17/T18 | header声明2成员[A,A,B]去重2，目录仅A，A quote价10 | rawA/B、displayA、excludedB；行A10；声明3而实际unique2为ResourceFormat。 |
| F39 | boards T17/T18 | DBF记录长度越header界、成员码正常 | 解析失败，不能跨记录读出假成员；缺base.dbf的Region明确不可用。 |
| F40 | 时间策略 T12/T18 | 两次wire各5秒，另预取5秒+主查5秒 | Rust总8秒Timeout；Python两次请求均成功；各自单wire9秒均按策略超时。 |
| F41 | retry策略 T11/T18 | 二分恢复需第3次额外请求且旧32内；Rust配置额外2 | Rust LimitExceeded/预算根因；Python继续；第33旧恢复仍失败，不能新cap削短。 |
| F42 | close/raw T12/T18 | 主请求成功、可选补充仍等；close或取消先赢 | 整操作Err/retirement，已成功raw页无孤儿引用；不是Optional Failed后继续成功。 |

## 16. 完整调用示例（拟议、未编译）

下面只使用本文与主规格定义的名称；代码明确处理查询和关闭错误。`with_count`校验失败发生在创建client前；查询失败后仍显式close。close本身失败时记录清理未确认，而不是声称资源已全部回收。该示例尚未编译，因为SDK尚未实现；落地时必须作为外部consumer示例编译并使用loopback验证。

```rust,ignore
use eltdx::{OperationOptions, Result, SecurityId, TdxClient};
use eltdx::types::{KlinePeriod, KlineQuery};
use std::time::Duration;

fn main() -> Result<()> {
    let id: SecurityId = "sh600000".parse()?;
    let query = KlineQuery::new()
        .with_period(KlinePeriod::Day)?
        .with_count(20)?;
    let options = OperationOptions::default()
        .with_timeout(Duration::from_secs(8));
    let client = TdxClient::builder().pool_size(1).build()?;

    let query_result = client.klines().page(&id, &query, &options);
    let close_result = client.close();
    match (query_result, close_result) {
        (Ok(series), Ok(())) => {
            println!("{}: {} bars", id, series.bars().items().len());
            Ok(())
        }
        (Err(error), Ok(())) => {
            eprintln!("query failed: {:?}: {}", error.kind(), error);
            Err(error)
        }
        (Ok(_), Err(error)) => {
            eprintln!("close not confirmed: {}", error);
            Err(error)
        }
        (Err(query_error), Err(close_error)) => {
            eprintln!("query failed: {}; close not confirmed: {}",
                      query_error, close_error);
            Err(query_error)
        }
    }
}
```

## 17. 完成证据与仍须冻结的决策

本文件列M01–M100共100个业务方法、F01–F42共42组具体语料、111个类型规格条目（包括25个query/request规格），另有6个新增service类型定义。类型在第3/4节与各域“类型”表各定义一次，主规格复用名称在第3节集中列出；不把方法参数中首次出现的未定义名字交给实现者猜。111是规格条目计数，不意味着111个都要新增根级导出；生命周期/getter/query setter/DTO getter不计业务方法。

| 项目 | 当前决定/还须取得的证据 | 阻断范围 |
| --- | --- | --- |
| 分页/预算 | 本文件明确的有限Rust默认、extra32/wire2已作拟议值；每命令offset上限实测与页游标基准须T11–14证明。Python无新增总cap。 | 各域发布前 |
| 价格/量单位 | quote/逐笔转债不可混精度；分钟、auction、finance其余字段的逐字段单位仍须独立wire/golden样本；未知保留原值+Unknown。 | 相关字段的强单位承诺 |
| 涨跌停policy | 规则有效日、ST/创业板前缀/BJ首日/IPO天数、round方式须固定来源并批准，旧三处冲突不能复制成事实。 | 宣称准确涨跌停的Rust helper |
| GBK与正文参数 | filename已核实GBK80字节、目录name64字节；损坏GBK、不可编码和内嵌NUL的旧处置与Rust拒绝/diagnostic需要三类差分语料。 | M72–73 |
| ZIP/DBF | 选择内部实现、MSRV、license及压缩/展开/DBF记录语料通过；默认HQ依赖可用。不得等到P3才补。 | T17–18 |
| 板块来源 | 默认Concept基于infoharbor；多级行业/Region必需定义/DBF的供给路径、日期/hash缓存有效性须样本验证；不承诺自动下载不存在的base.dbf。 | M96–97对应类别 |
| 旧问题迁移 | today_batch原异常、市场统计位置关联、helper涨跌停冲突、finance失败缓存、普通/退市raw差异均保留旧对照；另批修复。 | Python兼容验收 |
| 内存所有权 | 整批65535+2MiB输入检查、page raw共享、retire回收、三pin失败回滚、单demux推送分别量化验证。 | T11–18 |

T11验收代码/快照/批量；T12双语言quote桥接与执行profile；T13所有bar/minute；T14trades/auction/0FFC/fund-flow；T15订阅；T16corporate/equity/calendar/limits；T17文件/公司/session/health/ZIP/DBF基础；T18完整HQ helpers、缓存与Python投影。每域至少一条**非空**SDK公开入口loopback及其故障、预算、raw对照，fixture期望不能从同一个待测parser即时生成。文档完整不代表这些实现或验证已完成。
