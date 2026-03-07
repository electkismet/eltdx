# eltdx API 在线验证归纳

这份归纳基于最新一次全量在线验证结果：

- 验证时间：`2026-03-07 07:51:32 +08:00`
- 数据目录：`C:\Users\ax\Desktop\通达信接口\eltdx\artifacts\live_validation_20260307_075132`
- 验证步骤总数：`43`
- 通过：`43`
- 失败：`0`
- 兼容别名一致性检查：`5/5 全通过`

## 范围说明

这份报告覆盖的是 `TdxClient` 本次真实联网跑过的公开方法：

- 生命周期方法
- 市场数量与代码表方法
- 行情快照方法
- 分时 / 逐笔 / K线方法
- 集合竞价方法
- gbbq / xdxr / 股本 / 换手率 / 复权因子方法
- 兼容别名方法
- 组合链路方法，例如：
  - `逐笔 -> 分钟K线`
  - `gbbq -> xdxr -> factors -> adjusted_kline`

不包含内部的 `_` 私有方法。

## 总结结论

当前 `eltdx` 的客户端层已经处于**可整体跑通**状态。

本次结论：

- 所有已验证公开 API 全部跑通
- 实时链路和历史链路都跑通
- 实时分时 `TYPE_MINUTE` 与历史分时 `TYPE_HISTORY_MINUTE` 都已单独验证通过
- `get_quote()` 自动分批跑通
- 两条长连接生命周期跑通
- 兼容别名和主方法结果一致
- 本次验证里涉及到的时间字段都已经转成 Python 原生 `datetime` / `date`

## 一、生命周期 API

### `connect() -> None`

- 功能：建立底层长连接池
- 状态：正常
- 返回：`None`
- 作用：所有在线行情接口的基础入口
- 核对依据：`summary.json`

### `close() -> None`

- 功能：关闭连接池，释放 socket 资源
- 状态：正常
- 返回：`None`
- 作用：长连接模式的标准退出方法
- 核对依据：`summary.json`

### `with TdxClient() as client:`

- 功能：自动托管 `connect()` / `close()`
- 状态：正常
- 返回：`with` 代码块内可直接使用的 `client`
- 作用：短任务最推荐的调用方式
- 核对依据：`with_context_count_sz.json`

## 二、市场数量与代码表 API

### `get_count(exchange) -> int`

- 功能：返回单个市场的代码表条目数
- 状态：正常
- 返回：整数数量
- 本次实际返回：
  - `sh = 26779`
  - `sz = 22736`
  - `bj = 297`
- 说明：
  - `sh` / `sz` 来自通达信原始代码表，里面混有指数、板块分类、基金、回购、债券等多类条目
  - `bj` 来自北交所官方兜底代码源，不是和 `sh` / `sz` 完全同口径的“股票总数”
- 结论：这三个数字不能直接理解成“三个市场各有多少只股票”
- 核对文件：`counts.json`

### `get_codes(exchange, start=0, limit=1000) -> CodePage`

- 功能：分页获取某个市场的底层代码表
- 状态：正常
- 返回模型：`CodePage`
- 关键字段：
  - `exchange`：市场，例如 `sh` / `sz` / `bj`
  - `start`：本次请求的起始偏移
  - `count`：本次实际返回条数
  - `total`：该市场总条数
  - `items`：`SecurityCode` 列表
- `SecurityCode` 关键字段：
  - `code`：6 位证券代码
  - `name`：代码表名称字段
  - `exchange`：所属市场
  - `full_code`：完整代码，例如 `sz000001`
  - `last_price`：参考价格 / 解码后的价格浮点值
  - `last_price_raw`：原始整数价格
  - `multiple`：倍数信息
  - `decimal`：小数位提示
- 说明：返回的是通达信原始代码表条目，不是“纯股票列表”；真实样本里能看到指数、分类项、ETF、LOF、REITs、债券回购等混合内容
- 核对文件：`codes_page_sz_start0_limit20.json`

### `get_codes_all(exchange) -> list[Code]`

- 功能：获取某个市场的完整底层代码表
- 状态：正常
- 返回：`SecurityCode` 全量列表
- 本次实际返回：
  - `sh = 26779 行`
  - `sz = 22736 行`
  - `bj = 297 行`
- 说明：`bj` 当前走的是北交所官方兜底代码源，因为通达信代码表协议对北交所代码列表不稳定
- 结论：这个接口适合做“底层代码表抓取 / 交叉核对”，不应直接等价理解成“该市场全部股票”
- 核对文件：
  - `codes_all_sh.json`
  - `codes_all_sz.json`
  - `codes_all_bj.json`

### `get_stock_codes_all() -> list[str]`

- 功能：按更严格的股票规则返回可直接用于行情接口的股票完整代码
- 状态：正常
- 返回：字符串列表，例如 `sh600000`
- 当前规则：
  - `sh`：`600/601/603/605/688/689/900`
  - `sz`：`000/001/002/003/300/301/200`
  - `bj`：`920`
- 说明：
  - 现在不再把“代码表里剩下的所有东西”都当成股票
  - 这个结果更接近“可交易股票代码集合”，但不是对全市场所有证券品种做穷举分类
- 本次实际返回：`5566` 条
- 核对文件：`stock_codes_all.txt`

### `get_etf_codes_all() -> list[str]`

- 功能：按更严格的 ETF 规则返回完整代码
- 状态：正常
- 返回：字符串列表，例如 `sz159001`
- 当前规则：
  - `sh`：限定在常见 ETF 代码段内，且名称里显式包含 `ETF`
  - `sz`：限定 `159xxx`，且名称里显式包含 `ETF`
- 说明：
  - 这样可以排除 `sh152xxx` 这类之前被误归进去的债券 / 类债条目
  - 该方法定位是“实用 ETF 清单”，不是基金全品类清单
- 本次实际返回：`267` 条
- 核对文件：`etf_codes_all.txt`

### `get_index_codes_all() -> list[str]`

- 功能：按扩展后的指数规则返回完整代码
- 状态：正常
- 返回：字符串列表，例如 `sh000001` / `sh880001` / `sz399001` / `bj899050`
- 当前规则：
  - `sh`：`000xxx`、`880xxx`、`881xxx`、`999xxx`
  - `sz`：`399xxx`
  - `bj`：`899xxx`
- 说明：
  - 相比旧版本，已经补上 `sh999998`、`sh999997`、`sh880001` 这类明显的指数 / 指标代码
  - `sz395xxx` 这类更像分类标签的条目目前仍保留在原始代码表接口里，不混进这里的实用指数清单
- 本次实际返回：`1675` 条
- 核对文件：`index_codes_all.txt`

## 三、行情快照 API

### `get_quote(codes) -> list[Quote]`

- 功能：获取一个或多个标的的实时行情快照
- 状态：正常
- 返回模型：`list[Quote]`
- 本次覆盖：
  - 单代码行情
  - 120 只批量行情
  - 自动分批链路
- `Quote` 关键字段：
  - `exchange`, `code`：标的身份
  - `server_time`：解析后的服务器时间
  - `server_time_raw`：服务器原始时间整数
  - `last_price`：最新价
  - `open_price`, `high_price`, `low_price`, `close_price`：核心 OHLC
  - `total_hand`：成交手数
  - `amount`：成交额
  - `inside_dish`, `outer_disc`：内外盘指标
  - `buy_levels`, `sell_levels`：买五 / 卖五
  - `rate`：涨跌幅
- `QuoteLevel` 关键字段：
  - `buy`：是否买盘档位
  - `price`：档位价格
  - `price_milli`：整数价格形式
  - `number`：档位数量
- 核对文件：
  - `quote_single_sz000001.json`
  - `quotes_batch_120.json`
  - `quote_request_codes.txt`

## 四、分时 API
### `get_minute(code, date=None) -> MinuteSeries`

- 功能：返回分时序列
- 当前状态：正常
- 返回模型：`MinuteSeries`
- 协议路径：
  - 不传 `date` 时走**实时分时协议 `TYPE_MINUTE`**
  - 传入 `date` 时走**历史分时协议 `TYPE_HISTORY_MINUTE`**
- `MinuteSeries` 关键字段：
  - `count`：分时条数
  - `trading_date`：交易日，Python `date`
  - `items`：`MinuteItem` 列表
- `MinuteItem` 关键字段：
  - `time`：Python 原生 `datetime`
  - `price`：浮点价格
  - `price_milli`：毫厘整数价格
  - `volume`：成交量
- 本次实测：
  - 实时分时：`2026-03-07`，`240` 条
  - 历史分时：`2026-03-06`，`240` 条
- 核对文件：
  - `minute_live_sz000001.json`
  - `minute_history_2026-03-06_sz000001.json`

### `get_history_minute(code, date) -> MinuteSeries`

- 功能：历史分时兼容别名
- 当前状态：正常
- 等价关系：等同于 `get_minute(code, date=...)`
- 语义边界：只对应“传入日期后的历史分时路径”
- 核对文件：`history_minute_alias_2026-03-06_sz000001.json`


## 五、逐笔 API

### `get_trades(code, date=None, start=0, count=1800) -> TradePage`

- 功能：获取一页逐笔成交明细
- 状态：正常
- 返回模型：`TradePage`
- `TradePage` 关键字段：
  - `count`：本页返回条数
  - `trading_date`：交易日期，Python `date`
  - `items`：`TradeItem` 列表
- `TradeItem` 关键字段：
  - `time`：成交时间
  - `price`：成交价
  - `price_milli`：整数价格
  - `volume`：成交量
  - `status`：原始状态码
  - `side`：解析后的方向，例如 `buy` / `sell` / `neutral`
  - `order_count`：合并订单数（有则返回）
- 核对文件：
  - `trades_live_page_sz000001.json`
  - `trades_history_page_2026-03-06_sz000001.json`

### `get_trades_all(code, date=None) -> TradePage`

- 功能：自动翻页并合并整日逐笔
- 状态：正常
- 返回：完整 `TradePage`
- 本次历史整日逐笔返回：`3867` 条
- 核对文件：
  - `trades_live_all_sz000001.json`
  - `trades_history_all_2026-03-06_sz000001.json`

### 逐笔兼容别名

#### `get_trade(code, ...) -> TradePage`

- 功能：`get_trades(code, ...)` 的兼容别名
- 状态：正常
- 核对文件：`trade_alias_live_page_sz000001.json`

#### `get_history_trade(code, date, ...) -> TradePage`

- 功能：`get_trades(code, date, ...)` 的兼容别名
- 状态：正常
- 核对文件：`history_trade_alias_page_2026-03-06_sz000001.json`

#### `get_trade_all(code) -> TradePage`

- 功能：`get_trades_all(code)` 的兼容别名
- 状态：正常
- 核对文件：`trade_all_alias_live_sz000001.json`

#### `get_history_trade_day(code, date) -> TradePage`

- 功能：`get_trades_all(code, date)` 的兼容别名
- 状态：正常
- 核对文件：`history_trade_day_alias_2026-03-06_sz000001.json`

## 六、逐笔聚合链路 API

### `get_trade_minute_kline(code) -> KlinePage`

- 功能：把实时逐笔聚合成分钟 K 线
- 状态：正常
- 返回模型：`KlinePage`
- 链路含义：
  - `逐笔明细 -> 分钟K线`
- 核对文件：`trade_minute_kline_live_sz000001.json`

### `get_history_trade_minute_kline(code, date) -> KlinePage`

- 功能：把历史逐笔聚合成分钟 K 线
- 状态：正常
- 返回模型：`KlinePage`
- 链路含义：
  - `历史逐笔 -> 分钟K线`
- 核对文件：`trade_minute_kline_history_2026-03-06_sz000001.json`

## 七、K 线 API

### `get_kline(code, freq, ...) -> KlinePage`

- 功能：获取一页 K 线数据
- 状态：正常
- 返回模型：`KlinePage`
- 本次覆盖：
  - `day`
  - `1m`
- `KlinePage` 关键字段：
  - `count`：K线条数
  - `items`：`KlineItem` 列表
- `KlineItem` 关键字段：
  - `time`：K线时间
  - `open_price`, `high_price`, `low_price`, `close_price`：OHLC
  - `*_milli`：整数价格形式
  - `last_close_price`：上一收盘连续值
  - `volume`：成交量
  - `amount`：成交额
  - `order_count`：订单数（有则返回）
  - `up_count`, `down_count`：上涨/下跌家数（有则返回）
- 最新验证时间：`2026-03-07 07:51:32 +08:00`
- 核对文件：
  - `kline_day_page_sz000001.json`
  - `kline_1m_page_sz000001.json`
  - `kline_day_page_sh000001_index.json`

### `get_kline_all(code, freq) -> KlinePage`

- 功能：自动翻页并合并完整 K 线序列
- 状态：正常
- 返回：完整 `KlinePage`
- 本次日 K 全量返回：`8322` 条
- 核对文件：`kline_day_all_sz000001.json`

### K 线参数顺序兼容

- 旧写法 `get_kline(freq, code)` 也已验证
- 状态：正常
- 结果：与 `get_kline(code, freq)` 一致
- 核对文件：`alias_checks.json`

## 八、复权相关链路 API

### `get_gbbq(code) -> GbbqResponse`

- 功能：获取原始除权除息 / 股本变动事件流
- 状态：正常
- 返回模型：`GbbqResponse`
- `GbbqResponse` 关键字段：
  - `count`：事件数量
  - `items`：`GbbqItem` 列表
- `GbbqItem` 关键字段：
  - `code`：完整代码
  - `time`：事件时间
  - `category`：原始类别编号
  - `category_name`：类别名称
  - `c1`, `c2`, `c3`, `c4`：原始数值字段
- 核对文件：`gbbq_sz000001.json`

### `get_xdxr(code) -> list[XdxrItem]`

- 功能：从 `gbbq` 里筛出分红配股送转相关事件
- 状态：正常
- 返回：`XdxrItem` 列表
- `XdxrItem` 关键字段：
  - `time`：事件时间
  - `fenhong`：分红
  - `peigujia`：配股价
  - `songzhuangu`：送转股比例
  - `peigu`：配股比例
- 核对文件：`xdxr_sz000001.json`

### `get_equity_changes(code) -> EquityResponse`

- 功能：从 `gbbq` 派生股本变化序列
- 状态：正常
- 返回模型：`EquityResponse`
- `EquityItem` 关键字段：
  - `time`：生效时间
  - `category`, `category_name`：变化类别
  - `float_shares`：流通股本
  - `total_shares`：总股本
- 核对文件：`equity_changes_sz000001.json`

### `get_equity(code, on=None) -> EquityItem | None`

- 功能：返回某个日期可用的最新股本快照
- 状态：正常
- 返回：单个 `EquityItem` 或 `None`
- 关键字段：
  - `float_shares`
  - `total_shares`
  - `time`
- 核对文件：`equity_latest_sz000001.json`

### `get_turnover(code, volume, on=None, unit="hand") -> float`

- 功能：根据成交量和股本快照计算换手率
- 状态：正常
- 返回：浮点百分比
- 字段说明：
  - 返回的是换手率，不是原始成交量
- 核对文件：`turnover_sample_sz000001.json`

### `get_factors(code) -> FactorResponse`

- 功能：从 `day kline + xdxr` 推导前复权 / 后复权因子
- 状态：正常
- 返回模型：`FactorResponse`
- `FactorItem` 关键字段：
  - `time`：日期
  - `last_close_price`：原始前收
  - `pre_last_close_price`：调整后的前收参考值
  - `qfq_factor`：前复权因子
  - `hfq_factor`：后复权因子
- 链路含义：
  - `gbbq / xdxr + day kline -> factors`
- 核对文件：`factors_sz000001.json`

### `get_adjusted_kline(period, code, adjust=...) -> KlinePage`

- 功能：返回单页复权 K 线
- 状态：正常
- 返回：和普通 `KlinePage` 相同结构，但价格字段已复权
- 本次覆盖：
  - `qfq`
  - `hfq`
- 核对文件：
  - `adjusted_kline_qfq_day_page_sz000001.json`
  - `adjusted_kline_hfq_day_page_sz000001.json`

### `get_adjusted_kline_all(period, code, adjust=...) -> KlinePage`

- 功能：返回完整复权 K 线序列
- 状态：正常
- 返回：完整复权 `KlinePage`
- 链路含义：
  - `factors + raw kline -> adjusted kline`
- 核对文件：`adjusted_kline_qfq_day_all_sz000001.json`

## 九、集合竞价 API

### `get_call_auction(code, include_raw=False) -> CallAuctionResponse`

- 功能：获取集合竞价明细
- 状态：正常
- 返回模型：`CallAuctionResponse`
- 本次实际返回：`70` 条
- `CallAuctionResponse` 关键字段：
  - `count`：记录数量
  - `items`：`CallAuctionItem` 列表
  - `raw_frame_hex`：整帧原始 hex
  - `raw_payload_hex`：payload 原始 hex
- `CallAuctionItem` 关键字段：
  - `time`：记录时间
  - `price`：竞价价格
  - `price_milli`：整数价格
  - `match`：撮合量
  - `unmatched`：未撮合量
  - `flag`：方向标记，`1` 买未撮合，`-1` 卖未撮合
  - `raw_hex`：原始记录 hex
- 核对文件：`call_auction_sz000001.json`

## 十、兼容别名总结

以下兼容关系都已真实验证，并且结果一致：

- `get_kline(code, freq) == get_kline(freq, code)`
- `get_kline_all(code, freq) == get_kline_all(freq, code)`
- `get_minute(code, date) == get_history_minute(code, date)`：仅针对传入日期的历史分时路径
- `get_trades(code, date) == get_history_trade(code, date)`
- `get_trades_all(code, date) == get_history_trade_day(code, date)`

核对文件：

- `alias_checks.json`

## 十一、通用字段规则

本次验证里，以下规则全部成立：

- 所有交易日字段使用 Python 原生 `date`
- 所有时间戳字段使用 Python 原生 `datetime`
- 已验证的 `datetime` 都带 `Asia/Shanghai` 时区
- 价格字段通常同时提供浮点值和整数 milli 值
- 原始协议 hex 字段只在 `include_raw=True` 时返回

## 十二、建议你优先人工核对的文件

如果你想拿去和真实数据做人工比对，建议优先从这些文件开始：

- `C:\Users\ax\Desktop\通达信接口\eltdx\artifacts\live_validation_20260307_042853\summary.json`
- `C:\Users\ax\Desktop\通达信接口\eltdx\artifacts\live_validation_20260307_042853\counts.json`
- `C:\Users\ax\Desktop\通达信接口\eltdx\artifacts\live_validation_20260307_042853\codes_all_sh.json`
- `C:\Users\ax\Desktop\通达信接口\eltdx\artifacts\live_validation_20260307_042853\codes_all_sz.json`
- `C:\Users\ax\Desktop\通达信接口\eltdx\artifacts\live_validation_20260307_042853\codes_all_bj.json`
- `C:\Users\ax\Desktop\通达信接口\eltdx\artifacts\live_validation_20260307_042853\quotes_batch_120.json`
- `C:\Users\ax\Desktop\通达信接口\eltdx\artifacts\live_validation_20260307_042853\minute_history_2026-03-06_sz000001.json`
- `C:\Users\ax\Desktop\通达信接口\eltdx\artifacts\live_validation_20260307_042853\trades_history_all_2026-03-06_sz000001.json`
- `C:\Users\ax\Desktop\通达信接口\eltdx\artifacts\live_validation_20260307_042853\kline_day_all_sz000001.json`
- `C:\Users\ax\Desktop\通达信接口\eltdx\artifacts\live_validation_20260307_042853\call_auction_sz000001.json`
- `C:\Users\ax\Desktop\通达信接口\eltdx\artifacts\live_validation_20260307_042853\gbbq_sz000001.json`
- `C:\Users\ax\Desktop\通达信接口\eltdx\artifacts\live_validation_20260307_042853\xdxr_sz000001.json`
- `C:\Users\ax\Desktop\通达信接口\eltdx\artifacts\live_validation_20260307_042853\factors_sz000001.json`

## 最终判断

对于当前已经验证过的 `TdxClient` 公开 API：

- 功能职责：清晰
- 在线链路：正常
- 返回结构：稳定、类型化
- 兼容别名：正常
- 客户端层首发可用性：可以
