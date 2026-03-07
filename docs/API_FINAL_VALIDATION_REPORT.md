# eltdx API 最终归纳（基于最新实测）

## 1. 验证基线

- 验证时间：`2026-03-07 07:51:32 +08:00`
- 产物目录：`C:\Users\ax\Desktop\通达信接口\eltdx\artifacts\live_validation_20260307_075132`
- 验证步骤：`43`
- 结果：`43 通过 / 0 失败`
- 别名一致性：`5 / 5 全通过`
- 样本主代码：`sz000001`

## 2. 先看总结

当前 `eltdx` 的客户端主链路已经能整体跑通，但要把“能跑通”和“业务语义完全正确”分开看。

这次最终结论分成三层：

1. **协议链路层面**：已经可用。连接、分页、自动分批、别名、历史/实时链路都跑通。
2. **模型解析层面**：已经比较可靠。时间字段已转成 Python `datetime` / `date`，价格保留了 `float + milli-int` 双形态。
3. **业务语义层面**：大多数接口可信；但 `get_count()` / `get_codes*()` 必须按“底层代码表”理解，不能再误读成“股票总数 / 全股票表”。

## 3. 这次最重要的纠偏

### `get_count(exchange)`

- 现在应理解为：**市场代码表条目数**
- 本次实测：
  - `sh = 26779`
  - `sz = 22736`
  - `bj = 297`
- 不应理解为：三个市场各自有多少只股票

### `get_codes()` / `get_codes_all()`

- 现在应理解为：**通达信底层混合代码表**
- 真实样本里已经看到：指数、分类项、ETF、LOF、REITs、债券回购等混合在一起
- 所以它更像“底层主数据抓取接口”，不是“纯股票清单接口”

### `get_stock_codes_all()` / `get_etf_codes_all()` / `get_index_codes_all()`

- 现在已改成更严格的“实用 helper”
- 它们比之前可信很多，但仍不等于“交易所全品类官方分类口径”
- 适合拿来直接做行情拉取，不适合当成最终产品分类百科

## 4. API 逐项归纳

下面按“做什么 / 关键字段 / 现在是否可信”来写。

### 4.1 生命周期

#### `connect() -> None`

- 做什么：建立底层长连接池
- 关键点：是所有在线接口的基础入口
- 现在是否可信：**可信**
- 核对文件：`summary.json`

#### `close() -> None`

- 做什么：关闭连接池，释放 socket 资源
- 关键点：长连模式下手动释放资源
- 现在是否可信：**可信**
- 核对文件：`summary.json`

#### `with TdxClient() as client:`

- 做什么：自动调用 `connect()` 和 `close()`
- 关键点：适合一次性抓取任务，避免忘记关连接
- 现在是否可信：**可信**
- 说明：这是“可选关闭”，不是“强制关闭”；需要长连时完全可以手动持有 client
- 核对文件：`with_context_count_sz.json`

### 4.2 市场与代码表

#### `get_count(exchange) -> int`

- 做什么：返回单市场代码表条目数
- 关键字段：整数
- 现在是否可信：**可信，但语义限定为代码表条目数**
- 本次结果：`sh=26779`、`sz=22736`、`bj=297`
- 核对文件：`counts.json`

#### `get_codes(exchange, start=0, limit=1000) -> CodePage`

- 做什么：分页读取市场代码表
- 关键字段：
  - `exchange`：市场
  - `start`：起始偏移
  - `count`：本页条数
  - `total`：总条数
  - `items`：`SecurityCode` 列表
- `SecurityCode` 关键字段：
  - `code`
  - `name`
  - `exchange`
  - `full_code`
  - `last_price`
  - `last_price_raw`
  - `multiple`
  - `decimal`
- 现在是否可信：**可信，定位是底层代码表接口**
- 核对文件：`codes_page_sz_start0_limit20.json`

#### `get_codes_all(exchange) -> list[SecurityCode]`

- 做什么：读取单市场完整代码表
- 关键字段：同 `SecurityCode`
- 现在是否可信：**可信，定位是完整底层代码表**
- 特别说明：`bj` 目前走北交所官方兜底源，口径和 `sh/sz` 的通达信代码表不是完全同源
- 核对文件：`codes_all_sh.json`、`codes_all_sz.json`、`codes_all_bj.json`

#### `get_stock_codes_all() -> list[str]`

- 做什么：返回更严格的股票代码清单
- 当前规则：
  - `sh`：`600/601/603/605/688/689/900`
  - `sz`：`000/001/002/003/300/301/200`
  - `bj`：`920`
- 本次结果：`5566` 条
- 现在是否可信：**实用可信**
- 说明：适合直接拿去 `quote / minute / trade / kline`，但不宣称覆盖所有证券品种
- 核对文件：`stock_codes_all.txt`

#### `get_etf_codes_all() -> list[str]`

- 做什么：返回更严格的 ETF 清单
- 当前规则：代码段命中，且名称里显式包含 `ETF`
- 本次结果：`267` 条
- 现在是否可信：**实用可信**
- 说明：已经排除了之前混进来的 `sh152xxx` 这类明显非 ETF 条目
- 核对文件：`etf_codes_all.txt`

#### `get_index_codes_all() -> list[str]`

- 做什么：返回更严格的指数清单
- 当前规则：
  - `sh`：`000xxx`、`880xxx`、`881xxx`、`999xxx`
  - `sz`：`399xxx`
  - `bj`：`899xxx`
- 本次结果：`1675` 条
- 现在是否可信：**实用可信**
- 说明：已经补上 `sh999998`、`sh999997`、`sh880001` 这类过去漏掉的明显指数
- 核对文件：`index_codes_all.txt`

### 4.3 实时行情快照

#### `get_quote(codes) -> list[Quote]`

- 做什么：获取一个或多个标的的实时快照
- 关键字段：
  - `exchange`、`code`
  - `server_time_raw`、`server_time`
  - `last_price_milli`、`last_price`
  - `open_price_milli`、`open_price`
  - `high_price_milli`、`high_price`
  - `low_price_milli`、`low_price`
  - `close_price_milli`、`close_price`
  - `total_hand`
  - `amount`
  - `inside_dish`、`outer_disc`
  - `buy_levels`、`sell_levels`
  - `rate`
- 现在是否可信：**可信**
- 特别说明：
  - 自动分批已跑通
  - `server_time` 是 best-effort 解析；如果原始整数不合法，会返回 `None`
- 核对文件：`quote_single_sz000001.json`、`quotes_batch_120.json`、`quote_five_stocks_sample.json`

### 4.4 分时

#### `get_minute(code, date=None) -> MinuteResponse`

- 作用：
  - `date is None` 时走**实时分时协议路径**
  - 传入 `date` 时走**历史分时协议路径**
- 关键字段：
  - `count`
  - `trading_date`
  - `items[].time`
  - `items[].price_milli`、`items[].price`
  - `items[].volume`
  - `raw_frame_hex`、`raw_payload_hex`
- 现在是否可信：**可信**
- 最新联网验证说明：
  - 实时分时路径与历史分时路径现在已经**拆开验证**
  - 不传日期时验证的是**实时分时协议**
  - 传入日期时验证的是**历史分时协议**
- 本次样本：
  - 实时分时：`2026-03-07`，`240` 条
  - 历史分时：`2026-03-06`，`240` 条
- 核对文件：`minute_live_sz000001.json`、`minute_history_2026-03-06_sz000001.json`

#### `get_history_minute(code, date) -> MinuteResponse`

- 作用：历史分时的兼容别名接口
- 现在是否可信：**可信**
- 与主方法关系：等价于 `get_minute(code, date)`
- 注意：它只对应“传入 `date` 的历史分时路径”，不代表 `get_minute()` 的实时分时路径
- 核对文件：`history_minute_alias_2026-03-06_sz000001.json`

### 4.5 逐笔

#### `get_trades(code, date=None, start=0, count=1800) -> TradeResponse`

- 做什么：读取一页逐笔成交
- 关键字段：
  - `count`
  - `trading_date`
  - `items[].time`
  - `items[].price_milli`、`items[].price`
  - `items[].volume`
  - `items[].status`
  - `items[].side`
  - `items[].order_count`
  - `raw_frame_hex`、`raw_payload_hex`
- 现在是否可信：**可信**
- 核对文件：`trades_live_page_sz000001.json`、`trades_history_page_2026-03-06_sz000001.json`

#### `get_trades_all(code, date=None) -> TradeResponse`

- 做什么：自动翻页拿完整逐笔
- 现在是否可信：**可信**
- 本次历史样本：`3867` 条
- 核对文件：`trades_live_all_sz000001.json`、`trades_history_all_2026-03-06_sz000001.json`

#### 逐笔别名

- `get_trade()`：`get_trades()` 别名，**可信**
- `get_history_trade()`：历史页别名，**可信**
- `get_trade_all()`：`get_trades_all()` 别名，**可信**
- `get_history_trade_day()`：历史全量别名，**可信**
- 核对文件：`trade_alias_live_page_sz000001.json`、`history_trade_alias_page_2026-03-06_sz000001.json`、`trade_all_alias_live_sz000001.json`、`history_trade_day_alias_2026-03-06_sz000001.json`

### 4.6 K 线

#### `get_kline(code, freq, start=0, count=800, kind="stock") -> KlineResponse`

- 做什么：读取一页 K 线
- 关键字段：
  - `items[].time`
  - `items[].open_price_milli`、`items[].open_price`
  - `items[].high_price_milli`、`items[].high_price`
  - `items[].low_price_milli`、`items[].low_price`
  - `items[].close_price_milli`、`items[].close_price`
  - `items[].last_close_price_milli`、`items[].last_close_price`
  - `items[].volume`
  - `items[].amount_milli`、`items[].amount`
  - `raw_frame_hex`、`raw_payload_hex`
- 现在是否可信：**可信**
- 本轮结果：`43 通过 / 0 失败`
- 最新验证时间：`2026-03-07 07:51:32 +08:00`

#### `get_kline_all(code, freq, kind="stock") -> KlineResponse`

- 做什么：自动翻页拿完整 K 线
- 现在是否可信：**可信**
- 最新验证时间：`2026-03-07 07:51:32 +08:00`
- 核对文件：`kline_day_all_sz000001.json`

#### 复权 K 线

- `get_adjusted_kline()`：单页复权 K，**可信**
- `get_adjusted_kline_all()`：全量复权 K，**可信**
- 说明：依赖 `gbbq -> xdxr -> factors -> adjusted_kline` 整条链路，这次已实测跑通
- 核对文件：`adjusted_kline_qfq_day_page_sz000001.json`、`adjusted_kline_hfq_day_page_sz000001.json`、`adjusted_kline_qfq_day_all_sz000001.json`

### 4.7 集合竞价

#### `get_call_auction(code, include_raw=False) -> CallAuctionResponse`

- 做什么：读取集合竞价逐条记录
- 关键字段：
  - `count`
  - `items[].time`
  - `items[].price_milli`、`items[].price`
  - `items[].match`
  - `items[].unmatched`
  - `items[].flag`
  - `items[].raw_hex`
  - `raw_frame_hex`、`raw_payload_hex`
- 现在是否可信：**可信**
- 说明：是当前最适合做“原始记录核对”的接口之一，因为每条记录都保留了 `raw_hex`
- 核对文件：`call_auction_sz000001.json`

### 4.8 服务链 / 公司行为 /股本

#### `get_gbbq(code)`

- 做什么：读取股本变迁 / 除权除息等底层事件
- 关键字段：`time`、`category`、`category_name`、`c1/c2/c3/c4`
- 现在是否可信：**可信**
- 核对文件：`gbbq_sz000001.json`

#### `get_xdxr(code)`

- 做什么：把 `gbbq` 里的相关事件提炼成更直接的分红配股模型
- 关键字段：`time`、`fenhong`、`peigujia`、`songzhuangu`、`peigu`
- 现在是否可信：**可信**
- 核对文件：`xdxr_sz000001.json`

#### `get_equity_changes(code)`

- 做什么：读取股本变化序列
- 关键字段：`time`、`category`、`category_name`、`float_shares`、`total_shares`
- 现在是否可信：**可信**
- 核对文件：`equity_changes_sz000001.json`

#### `get_equity(code)`

- 做什么：读取最新一期股本
- 关键字段：`time`、`float_shares`、`total_shares`
- 现在是否可信：**可信**
- 核对文件：`equity_latest_sz000001.json`

#### `get_turnover(code, volume, on, unit)`

- 做什么：按股本口径计算换手率
- 关键字段：`code`、`volume`、`unit`、`on`、`turnover`
- 现在是否可信：**可信**
- 核对文件：`turnover_sample_sz000001.json`

#### `get_factors(code)`

- 做什么：生成前复权 / 后复权因子序列
- 关键字段：`time`、`last_close_price`、`pre_last_close_price`、`qfq_factor`、`hfq_factor`
- 现在是否可信：**可信**
- 核对文件：`factors_sz000001.json`

### 4.9 组合链路

#### `get_trade_minute_kline(code)` / `get_history_trade_minute_kline(code, date)`

- 做什么：把逐笔聚合成分钟 K 线
- 现在是否可信：**可信**
- 说明：说明“逐笔 -> 分钟线”的二次加工链路是通的
- 核对文件：`trade_minute_kline_live_sz000001.json`、`trade_minute_kline_history_2026-03-06_sz000001.json`

## 5. 原始字段 vs 解析字段：你重点看这部分

这部分专门给你做“原始值”和“解析后值”的对照，便于你人工核对。

### 5.1 五只股票 Quote 对照

- 对照文件：`quote_five_stocks_sample.json`
- 五只样本：
  - `sz000001`
  - `sh600000`
  - `sh601398`
  - `sz300750`
  - `bj920571`
- 每只股票都做了这 5 组对照：
  - `server_time_raw -> server_time`
  - `last_price_milli -> last_price`
  - `open_price_milli -> open_price`
  - `close_price_milli -> close_price`
  - `buy_levels[0].price_milli -> buy_levels[0].price`

#### 示例 A：`sz000001`

- `server_time_raw = 15330719`
- `server_time = 2026-03-07T15:33:07.190000+08:00`
- `last_price_milli = 10810`
- `last_price = 10.81`
- `open_price_milli = 10780`
- `open_price = 10.78`
- `close_price_milli = 10820`
- `close_price = 10.82`
- `buy1.price_milli = 10820`
- `buy1.price = 10.82`

#### 示例 B：`sh600000`

- `server_time_raw = 14999512`
- `server_time = null`
- 解释：这说明服务端返回的该时间整数本次不满足有效时分秒格式，所以库按 best-effort 策略保留原始值，同时把解析字段设为 `None`
- `last_price_milli = 9780`
- `last_price = 9.78`

### 5.2 代码表对照

- 对照文件：`api_sample_field_comparisons.json`
- `get_codes()` 样本：
  - 原始字段：`exchange=sz`、`code=395001`、`name=主板Ａ股`、`last_price_raw=1153064960`
  - 解析字段：`full_code=sz395001`、`last_price=1491.0`
- 这能直接说明：这个接口拿到的是**代码表条目**，不一定是股票

### 5.3 分时对照

- 核对文件：`api_sample_field_comparisons.json`
- `get_minute()` 实时分时样本：
  - 交易日：`2026-03-07`
  - 原始值：`price_milli=10820`
  - 解析值：`price=10.82`
  - 解析时间：`time=2026-03-07T09:31:00+08:00`
  - 成交量：`volume=16557`
- `get_history_minute()` 历史分时样本：
  - 交易日：`2026-03-06`
  - 原始值：`price_milli=10820`
  - 解析值：`price=10.82`
  - 解析时间：`time=2026-03-06T09:31:00+08:00`
  - 成交量：`volume=16557`
- 两条路径都保留可选原始调试字段：`raw_frame_hex`、`raw_payload_hex`

### 5.4 逐笔对照

- 对照文件：`api_sample_field_comparisons.json`
- `get_trades()` 首条样本：
  - 原始：`price_milli=10830`
  - 原始：`status=0`
  - 解析：`price=10.83`
  - 解析：`side=buy`
  - 解析：`time=2026-03-06T14:54:00+08:00`

### 5.5 K 线对照

- 对照文件：`api_sample_field_comparisons.json`
- `get_kline()` 首条样本：
  - 原始：`open_price_milli=10960`
  - 原始：`high_price_milli=10990`
  - 原始：`low_price_milli=10900`
  - 原始：`close_price_milli=10910`
  - 原始：`amount_milli=607501376000`
  - 解析：`open_price=10.96`
  - 解析：`high_price=10.99`
  - 解析：`low_price=10.90`
  - 解析：`close_price=10.91`
  - 解析：`amount=607501376.0`

### 5.6 集合竞价对照

- 对照文件：`api_sample_field_comparisons.json`
- `get_call_auction()` 首条样本：
  - 原始记录：`raw_hex = 2b02c3f52c4113000000e8ffffff0000`
  - 原始字段：`price_milli=10810`
  - 原始字段：`flag=-1`
  - 解析字段：`time=2026-03-07T09:15:00+08:00`
  - 解析字段：`price=10.81`
  - 解析字段：`match=19`
  - 解析字段：`unmatched=24`
  - 解析字段：`flag_meaning=卖未撮合`

## 6. 你现在核对数据时，建议按这个顺序看

1. 先看 `counts.json`，但把它当“代码表总量”看。
2. 再看 `codes_all_sh.json`、`codes_all_sz.json`、`codes_all_bj.json`，确认代码表内容是否完整。
3. 看 `quote_five_stocks_sample.json`，这是最适合理解“原始字段 -> 解析字段”的样本。
4. 再看 `api_sample_field_comparisons.json`，它把 `codes / minute / trades / kline / call_auction / gbbq / xdxr / equity / factors / turnover` 的样本都集中到一个文件里了。
5. 最后才看全量文件，比如 `trades_history_all_*`、`kline_day_all_*`、`adjusted_kline_*`。

## 7. 当前还剩下的真实边界

- `get_count()` 不是股票总数接口，这一点已经确认。
- `get_codes*()` 不是纯股票主表接口，这一点已经确认。
- `get_stock_codes_all()` / `get_etf_codes_all()` / `get_index_codes_all()` 现在已经比之前准确很多，但它们仍是“面向实用调用的 helper”，不是“官方证券分类总表”。
- `get_kline()` / `get_kline_all()` 在 `kind` 的证券类型语义上仍建议按场景显式传参；指数场景优先传 `kind="index"`。
- `Quote.server_time` 仍然要保留 best-effort 语义，因为服务端原始整数偶尔会不合法；这时应该以 `server_time_raw` 为准，`server_time` 允许为 `None`。

## 8. 这版最终判断

如果目标是：

- 让用户用 Python 方便地连通达信行情接口
- 拉实时行情、分时、逐笔、K 线、集合竞价
- 做基础复权、股本、换手率链路

那么当前项目已经进入：**可以对外给人试用和实盘核对的阶段**。

如果目标是：

- 建一套“官方语义级别”的证券分类体系
- 把所有市场条目做成严格的产品百科主数据

那么这一部分目前还不应该宣称“完全完成”；它已经从“明显错分”修到“实用可用”，但还不是最终分类工程。
