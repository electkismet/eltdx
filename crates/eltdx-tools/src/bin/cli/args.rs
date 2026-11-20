use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "eltdx",
    version,
    disable_help_subcommand = true,
    about = "eltdx 命令行 —— 通达信 7709 行情 / 7615 F10 / 7727 扩展行情 / 本地行情文件。",
    after_help = "场外基金（016947 等）不在 7709 行情里，请用 `eltdx fund`。"
)]
pub(super) struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub(super) enum Commands {
    /// 显示版本
    Version(Output),
    /// 探测 7709 服务器
    Ping(Hq),
    /// 交易日判断
    Workday(Workday),
    /// 实时快照 / 五档盘口
    Quote(Quote),
    /// K 线（7709）
    Kline(Kline),
    /// 分时数据（7709）
    Minute(Minute),
    /// 逐笔成交（7709）
    Trades(Trades),
    /// 证券代码表
    Codes(Directory),
    /// 财务数据
    Finance(Finance),
    /// 资金流向
    MoneyFlow(MoneyFlow),
    /// 7615 F10 资料接口
    F10(F10),
    /// 指数成份股 / 跟踪产品
    Index(Index),
    /// 近期新股列表
    Ipo(F10Connection),
    /// 每日涨跌停价
    Limits(Limits),
    /// 扩展行情（7727，EX 协议）
    Ex(Ex),
    /// MAC 协议（7709 MAC 服务器）
    Mac(Mac),
    /// MAC 扩展行情（7727，MAC_EX 主站）
    Macex(MacEx),
    /// 场外基金净值
    Fund(Fund),
    /// 本地行情文件（vipdoc）
    Offline(Offline),
}

#[derive(Clone, Copy, PartialEq, ValueEnum)]
pub(super) enum Format {
    Json,
    Jsonl,
    Table,
    Csv,
}

#[derive(Args, Clone)]
pub(super) struct Output {
    /// 输出格式
    #[arg(short, long, value_enum, default_value = "json")]
    pub output: Format,
    /// JSON 缩进空格数
    #[arg(long, allow_negative_numbers = true)]
    pub indent: Option<i32>,
}

#[derive(Args)]
pub(super) struct Hq {
    #[command(flatten)]
    pub output: Output,
    /// 指定服务器地址
    #[arg(long)]
    pub host: Option<String>,
    /// 连接/读取超时秒数
    #[arg(long, default_value_t = 15.0, allow_negative_numbers = true)]
    pub timeout: f64,
}

#[derive(Args)]
pub(super) struct Workday {
    #[command(flatten)]
    pub common: Hq,
    #[arg(long)]
    pub date: Option<String>,
}

#[derive(Args)]
pub(super) struct Quote {
    #[command(flatten)]
    pub common: Hq,
    #[arg(required = true)]
    pub code: Vec<String>,
    #[arg(long)]
    pub depth: bool,
}

#[derive(Args)]
pub(super) struct Kline {
    #[command(flatten)]
    pub common: Hq,
    pub code: Vec<String>,
    #[arg(long, default_value = "day")]
    pub period: String,
    #[arg(long)]
    pub adjust: Option<String>,
    #[arg(long, default_value_t = 0, allow_negative_numbers = true)]
    pub start: i64,
    #[arg(long, default_value_t = 100, allow_negative_numbers = true)]
    pub count: i64,
    #[arg(long)]
    pub raw: bool,
}

#[derive(Args)]
pub(super) struct Minute {
    #[command(flatten)]
    pub common: Hq,
    #[arg(required = true)]
    pub code: Vec<String>,
    #[arg(long)]
    pub date: Option<String>,
}

#[derive(Args)]
pub(super) struct Trades {
    #[command(flatten)]
    pub common: Hq,
    #[arg(required = true)]
    pub code: Vec<String>,
    #[arg(long)]
    pub date: Option<String>,
    #[arg(long, default_value_t = 0, allow_negative_numbers = true)]
    pub start: i64,
    #[arg(long, default_value_t = 100, allow_negative_numbers = true)]
    pub count: i64,
    #[arg(long)]
    pub all: bool,
    #[arg(long)]
    pub raw: bool,
}

#[derive(Clone, ValueEnum)]
pub(super) enum DirectoryKind {
    Stocks,
    Etfs,
    Indices,
    All,
    List,
    Count,
    Markets,
}

#[derive(Args)]
pub(super) struct Directory {
    #[command(flatten)]
    pub common: Hq,
    #[arg(long, default_value = "sz")]
    pub market: String,
    #[arg(long, value_enum, default_value = "stocks")]
    pub kind: DirectoryKind,
    #[arg(long, default_value_t = 0, allow_negative_numbers = true)]
    pub start: i64,
    #[arg(long, default_value_t = 100, allow_negative_numbers = true)]
    pub limit: i64,
}

#[derive(Args)]
pub(super) struct Finance {
    #[command(flatten)]
    pub common: Hq,
    #[arg(required = true)]
    pub code: Vec<String>,
    #[arg(long)]
    pub raw: bool,
}

#[derive(Args)]
pub(super) struct MoneyFlow {
    #[command(flatten)]
    pub common: Hq,
    #[arg(required = true)]
    pub code: Vec<String>,
    #[arg(long, value_parser = ["3", "5", "10", "20", "60"])]
    pub days: Option<String>,
    #[arg(long)]
    pub raw: bool,
}

#[derive(Args)]
pub(super) struct Limits {
    #[command(flatten)]
    pub common: Hq,
    pub code: Vec<String>,
    #[arg(long)]
    pub date: Option<String>,
}

#[derive(Args)]
pub(super) struct F10Connection {
    #[command(flatten)]
    pub output: Output,
    #[arg(long, default_value_t = 15.0, allow_negative_numbers = true)]
    pub timeout: f64,
    #[arg(long)]
    pub base_url: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(super) enum F10Action {
    Sections,
    Call,
    StockInfo,
    Profile,
    Digest,
    Shareholders,
    Capital,
    Business,
    Industry,
    Trading,
    Dividend,
    Finance,
    Governance,
    Forecast,
    Score,
    Topics,
    TopicCompare,
    News,
    Northbound,
    Ranking,
    Valuation,
    BoardMembers,
    LimitBoard,
    Fund,
    Market,
    MarketNews,
    ChannelNews,
    NewsChannels,
    Detail,
}

#[derive(Args)]
pub(super) struct F10 {
    #[command(flatten)]
    pub common: F10Connection,
    #[arg(value_enum)]
    pub action: F10Action,
    pub code: Vec<String>,
    #[arg(long)]
    pub section: Option<String>,
    #[arg(long)]
    pub arg: Option<String>,
    #[arg(long, allow_negative_numbers = true)]
    pub tab: Option<i64>,
    #[arg(long, allow_negative_numbers = true)]
    pub channel: Option<i64>,
    #[arg(long)]
    pub date: Option<String>,
    #[arg(long)]
    pub report_date: Option<String>,
    #[arg(long)]
    pub topic: Option<String>,
    #[arg(long)]
    pub entry: Option<String>,
    #[arg(long)]
    pub params: Option<String>,
    #[arg(long, default_value_t = 1, allow_negative_numbers = true)]
    pub page: i64,
    #[arg(long, default_value_t = 20, allow_negative_numbers = true)]
    pub page_size: i64,
}

#[derive(Clone, ValueEnum)]
pub(super) enum IndexAction {
    Members,
    Etfs,
}

#[derive(Args)]
pub(super) struct Index {
    #[command(flatten)]
    pub common: F10Connection,
    #[arg(value_enum)]
    pub action: IndexAction,
    #[arg(required = true)]
    pub code: Vec<String>,
    #[arg(long, default_value_t = 100, allow_negative_numbers = true)]
    pub page_size: i64,
    #[arg(long)]
    pub first_page: bool,
    #[arg(long)]
    pub etf_only: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(super) enum ExAction {
    Markets,
    Count,
    Info,
    Quote,
    QuoteList,
    Bars,
    Minute,
    Txn,
}

#[derive(Args)]
pub(super) struct Ex {
    #[command(flatten)]
    pub common: Hq,
    #[arg(value_enum)]
    pub action: ExAction,
    pub code: Vec<String>,
    #[arg(long, default_value_t = 7727, allow_negative_numbers = true)]
    pub port: i64,
    #[arg(long, allow_negative_numbers = true)]
    pub market: Option<i64>,
    #[arg(long, allow_negative_numbers = true)]
    pub category: Option<i64>,
    #[arg(long, default_value_t = 0, allow_negative_numbers = true)]
    pub start: i64,
    #[arg(long, default_value_t = 20, allow_negative_numbers = true)]
    pub count: i64,
    #[arg(long)]
    pub all_hosts: bool,
    #[arg(long)]
    pub probe_server: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(super) enum MacAction {
    Quote,
    Kline,
    Ranking,
    BoardList,
    ServerInfo,
    SymbolInfo,
    Minute,
    Txn,
    Auction,
    Unusual,
    GoodsList,
}

#[derive(Args)]
pub(super) struct Mac {
    #[command(flatten)]
    pub common: Hq,
    #[arg(value_enum)]
    pub action: MacAction,
    pub code: Vec<String>,
    #[arg(long, default_value_t = 7709, allow_negative_numbers = true)]
    pub port: i64,
    #[arg(long, allow_negative_numbers = true)]
    pub market: Option<i64>,
    #[arg(long, allow_negative_numbers = true)]
    pub category: Option<i64>,
    #[command(flatten)]
    pub bars: Bars,
    #[arg(long, default_value_t = 100, allow_negative_numbers = true)]
    pub count: i64,
}

#[derive(Args)]
pub(super) struct Bars {
    #[arg(long, default_value = "day")]
    pub period: String,
    #[arg(long, default_value = "none")]
    pub adjust: String,
    #[arg(long)]
    pub date: Option<String>,
    #[arg(long, default_value_t = 0, allow_negative_numbers = true)]
    pub start: i64,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(super) enum MacExAction {
    Count,
    Instruments,
    List,
    Quote,
    QuoteList,
    Kline,
    Minute,
    Txn,
}

#[derive(Args)]
pub(super) struct MacEx {
    #[command(flatten)]
    pub common: Hq,
    #[arg(value_enum)]
    pub action: MacExAction,
    pub code: Vec<String>,
    #[arg(long, default_value_t = 7727, allow_negative_numbers = true)]
    pub port: i64,
    #[arg(long, allow_negative_numbers = true)]
    pub market: Option<i64>,
    #[command(flatten)]
    pub bars: Bars,
    #[arg(long, default_value_t = 20, allow_negative_numbers = true)]
    pub count: i64,
    #[arg(long)]
    pub all_hosts: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(super) enum FundAction {
    Markets,
    List,
    Nav,
    Kline,
    Info,
}

#[derive(Args)]
pub(super) struct Fund {
    #[command(flatten)]
    pub common: Hq,
    #[arg(value_enum)]
    pub action: FundAction,
    pub code: Vec<String>,
    #[arg(long, default_value_t = 7727, allow_negative_numbers = true)]
    pub port: i64,
    #[arg(long, default_value_t = 33, allow_negative_numbers = true)]
    pub market: i64,
    #[arg(long, default_value = "day")]
    pub period: String,
    #[arg(long, default_value_t = 0, allow_negative_numbers = true)]
    pub start: i64,
    #[arg(long, default_value_t = 20, allow_negative_numbers = true)]
    pub count: i64,
}

#[derive(Clone, Copy, ValueEnum)]
pub(super) enum OfflineAction {
    Day,
    Min,
    #[value(name = "5min")]
    FiveMin,
    ExDay,
    Block,
    Industry,
    Gbbq,
    Financial,
}

#[derive(Args)]
pub(super) struct Offline {
    #[command(flatten)]
    pub output: Output,
    #[arg(value_enum)]
    pub action: OfflineAction,
    #[arg(long)]
    pub file: Option<PathBuf>,
    #[arg(long)]
    pub vipdoc: Option<PathBuf>,
    #[arg(long, allow_negative_numbers = true)]
    pub market: Option<i64>,
    #[arg(long)]
    pub code: Option<String>,
    #[arg(long, default_value_t = 0, allow_negative_numbers = true)]
    pub count: i64,
    #[arg(long)]
    pub last: bool,
}
