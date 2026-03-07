# API 用法

这份文档就是“每个函数怎么用”的总说明。

你可以把它当作查表页：想找初始化方式、参数、返回值、示例，就来这里看。

先记住这几件事：

- 推荐尽量使用完整代码，例如 `sz000001`、`sh600000`、`bj920001`
- 多数接口也接受裸六码，内部会自动补前缀
- 时间字段会直接转换成 Python 原生对象：交易日用 `date`，带时分秒的字段用 `datetime`
- 价格字段会同时保留浮点值和 `*_milli` 整数值
- 很多接口支持 `include_raw=True`，需要排查时可以直接看 `raw_frame_hex` 和 `raw_payload_hex`

配套阅读：

- 字段说明：[`FIELD_REFERENCE.md`](./FIELD_REFERENCE.md)
- 调试指南：[`DEBUG_GUIDE.md`](./DEBUG_GUIDE.md)
- 使用示例：[`EXAMPLES.md`](./EXAMPLES.md)

> 本页示例只用于说明调用方式和字段含义，实时行情会变化，示例数值不代表固定结果。

## 1. `TdxClient` 初始化

```python
from eltdx import TdxClient

client = TdxClient()
custom = TdxClient(host="124.71.187.122:7709")
cluster = TdxClient(
    hosts=["124.71.187.122:7709", "122.51.120.217:7709"],
    timeout=8.0,
    pool_size=2,
    batch_size=80,
)
```

参数说明：

| 参数 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `host` | `str | None` | `None` | 单个服务器地址，格式如 `124.71.187.122:7709` |
| `hosts` | `list[str] | tuple[str, ...] | None` | `None` | 服务器地址列表；传入后优先于 `host` |
| `timeout` | `float` | `8.0` | 单个 socket 请求超时秒数 |
| `pool_size` | `int` | `2` | 连接池大小；默认两条长连接 |
| `batch_size` | `int` | `80` | `get_quote()` 自动分批上限，内部会限制在 `1~80` |

连接模式：

- **一次性脚本**：`with TdxClient() as client:`
- **长连接常驻**：手动 `connect()`，在需要时再 `close()`

## 2. 生命周期

### `connect() -> None`

建立连接池并准备线程池。

说明：

- 可重复调用，已经连接时会直接返回。
- 大多数业务方法内部也会自动触发连接；你可以手动先连上，也可以直接调用业务方法。

### `close() -> None`

关闭线程池与全部底层 socket 连接。

说明：

- 建议脚本结束前调用，避免留下僵尸连接。
- 对于实时长连场景，不需要每次请求后都关闭。

### `with TdxClient() as client:`

上下文管理器模式，进入时自动 `connect()`，离开时自动 `close()`。

```python
from eltdx import TdxClient

with TdxClient() as client:
    quotes = client.get_quote(["sz000001", "sh600000"])
    print(quotes[0].last_price)
```

## 3. 市场 / 代码表

### `get_count(exchange: str) -> int`

返回单个市场的代码表条目数。

参数说明：

- `exchange`：`"sh"`、`"sz"`、`"bj"`

返回说明：

- 返回的是**底层代码表条目数**。
- 不能把它直接理解成“这个市场有多少只股票”。

### `get_codes(exchange: str, start: int = 0, limit: int | None = 1000) -> CodePage`

分页读取单个市场的代码表。

参数说明：

- `exchange`：市场代码
- `start`：起始偏移，必须大于等于 `0`
- `limit`：读取条数；传 `None` 表示从 `start` 读到结尾

返回结构：

- `CodePage.exchange`：市场
- `CodePage.start`：起始偏移
- `CodePage.count`：本页返回数量
- `CodePage.total`：该市场代码表总条数
- `CodePage.items`：`list[SecurityCode]`

`SecurityCode` 关键字段：

- `exchange`：市场前缀
- `code`：六码
- `name`：名称
- `full_code`：完整代码属性，例如 `sz000001`
- `multiple`：价格倍率相关原始参数
- `decimal`：小数位相关原始参数
- `last_price`：最近价浮点值
- `last_price_raw`：最近价原始整数值

### `get_codes_all(exchange: str) -> list[SecurityCode]`

读取单个市场完整代码表。

说明：

- `sh` / `sz` 走通达信代码表。
- `bj` 当前走北交所官方兜底源，口径与 `sh` / `sz` 不是完全同源，但对使用层足够实用。

### `get_stock_count(exchange: str) -> int`

对 `get_codes_all(exchange)` 做过滤后返回“股票类代码条目数”。

说明：

- 这是**本库 helper 统计**，不是服务端原生接口。
- 当前股票 helper 包含：上海 `900xxx`、深圳 `200xxx` 这类 B 股。

### `get_a_share_count(exchange: str) -> int`

对 `get_codes_all(exchange)` 做过滤后返回“A 股代码条目数”。

说明：

- 这是**更严格的 A 股 helper 统计**。
- 会排除上海 `900xxx`、深圳 `200xxx` 这类 B 股。
- 北交所当前按 `920xxx` 识别为股票范围。

### `get_stock_codes_all() -> list[str]`
### `get_a_share_codes_all() -> list[str]`
### `get_etf_codes_all() -> list[str]`
### `get_index_codes_all() -> list[str]`

这些方法都是在完整代码表基础上做的“实用过滤 helper”。

说明：

- 适合直接拿去做 `quote`、`minute`、`trade`、`kline` 拉取。
- 不应把它们理解成交易所官方分类主表。
- 如果你只关心 A 股，优先使用 `get_a_share_codes_all()`。
- 如果你希望 A 股 + B 股一起抓，可以使用 `get_stock_codes_all()`。

## 4. 行情快照

### `get_quote(codes: str | list[str] | tuple[str, ...]) -> list[Quote]`

获取一个或多个证券的实时行情快照。

```python
with TdxClient() as client:
    one = client.get_quote("sz000001")
    many = client.get_quote(["sz000001", "sh600000", "sh000001"])
```

行为说明：

- 单个代码会被自动包装成列表返回。
- 返回类型始终是 `list[Quote]`。
- 当代码数量超过 `batch_size` 时会自动分批。
- 默认 `batch_size=80`，默认 `pool_size=2`，会在两条长连接间分发请求。

`Quote` 关键字段：

- `exchange`、`code`
- `server_time_raw`：服务端原始时间整数
- `server_time`：解析后的 Python `datetime | None`
- `last_price`、`last_price_milli`：最新价
- `open_price`、`open_price_milli`
- `high_price`、`high_price_milli`
- `low_price`、`low_price_milli`
- `last_close_price`、`last_close_price_milli`：昨收 / 前收价
- `total_hand`：总手数
- `amount`：成交额
- `inside_dish`、`outer_disc`
- `buy_levels`、`sell_levels`：五档盘口，元素类型是 `QuoteLevel`
- `rate`：涨跌幅相关字段

说明：

- `server_time` 是 best-effort 解析结果；若原始值非法，`server_time` 可能是 `None`，这时应以 `server_time_raw` 为准。
- 当前快照字段约定是：`last_price` 表示最新价，`last_close_price` 表示昨收 / 前收价。
- 更完整的字段中文映射见 [`FIELD_REFERENCE.md`](./FIELD_REFERENCE.md#6-quote--quotelevel-行情快照)

## 5. 分时

### `get_minute(code: str, date: str | None = None, include_raw: bool = False) -> MinuteResponse`

读取分时序列；这是统一入口。

```python
with TdxClient() as client:
    today_minute = client.get_minute("sz000001")
    history_minute = client.get_minute("sz000001", "2026-03-06", include_raw=True)

print(today_minute.trading_date)
print(today_minute.items[0].time, today_minute.items[0].price)
print(history_minute.raw_payload_hex)
```

参数说明：

- `code`：证券代码，支持 `sz000001` / `000001` 形式
- `date`：`None`、`YYYY-MM-DD`、`YYYYMMDD`、`date`、`datetime`
- `include_raw`：是否附带原始调试字段

行为说明：

- `date is None` 时走**实时分时协议路径**。
- 传入 `date` 时走**历史分时协议路径**。
- 两条路径统一返回 `MinuteResponse` 模型。

返回结构：

- `MinuteResponse.count`
- `MinuteResponse.trading_date`
- `MinuteResponse.items`
- `MinuteResponse.raw_frame_hex`
- `MinuteResponse.raw_payload_hex`

`MinuteItem` 关键字段：

- `time`：Python `datetime`
- `price`：浮点价格
- `price_milli`：整数毫厘价格
- `volume`：成交量

### `get_history_minute(code: str, date, include_raw: bool = False) -> MinuteResponse`

`get_minute(code, date=...)` 的兼容别名，仅对应历史分时路径。

## 6. 逐笔

### `get_trades(code: str, date: str | None = None, start: int = 0, count: int = 1800, include_raw: bool = False) -> TradeResponse`

读取一页逐笔成交。

```python
with TdxClient() as client:
    live_page = client.get_trades("sz000001", start=0, count=100)
    history_page = client.get_trades("sz000001", "2026-03-06", start=0, count=200, include_raw=True)
```

说明：

- 不传 `date` 时读取实时逐笔。
- 传入 `date` 时读取指定交易日的历史逐笔。
- 实时单页上限是 `1800`。
- 历史单页上限是 `2000`。

`TradeResponse` 关键字段：

- `count`
- `trading_date`
- `items`
- `raw_frame_hex`
- `raw_payload_hex`

`TradeItem` 关键字段：

- `time`：Python `datetime`
- `price`、`price_milli`
- `volume`
- `status`
- `side`
- `order_count`

### `get_trades_all(code: str, date: str | None = None) -> TradeResponse`

自动翻页拉取完整逐笔。

说明：

- 不传 `date` 时返回实时全量逐笔。
- 传入 `date` 时返回指定交易日全量逐笔。

### 逐笔别名

- `get_trade()`：`get_trades()` 的实时别名
- `get_history_trade()`：`get_trades(code, date, ...)` 的历史页别名
- `get_trade_all()`：`get_trades_all(code)` 的实时别名
- `get_history_trade_day()`：`get_trades_all(code, date)` 的历史全量别名

## 7. K 线

### `get_kline(code: str, freq: str, start: int = 0, count: int = 800, kind: str = "stock", include_raw: bool = False) -> KlineResponse`

读取一页 K 线。

```python
with TdxClient() as client:
    day_page = client.get_kline("sz000001", "day", count=10)
    minute_page = client.get_kline("sz000001", "1m", count=30)
    index_page = client.get_kline("sh000001", "day", kind="index", count=10)
```

频率说明：

- 冻结推荐值：`1m`、`5m`、`15m`、`30m`、`60m`、`day`、`week`、`month`、`quarter`、`year`
- 兼容别名也支持，例如 `1min`、`daily`、`1w`、`1mo`、`1y`

参数说明：

- `start`：起始偏移，必须大于等于 `0`
- `count`：单页条数，必须大于 `0`，内部单页上限 `800`
- `kind`：`"stock"` 或 `"index"`
- `include_raw`：是否附带原始调试字段

说明：

- 推荐调用形式是 `get_kline(code, freq, ...)`。
- 当前实现也兼容 `get_kline(freq, code, ...)`，用于兼容旧调用习惯。
- 指数场景建议显式传 `kind="index"`，这样 `up_count` / `down_count` 等字段语义才更完整。

`KlineResponse` 关键字段：

- `count`
- `items`
- `raw_frame_hex`
- `raw_payload_hex`

`KlineItem` 关键字段：

- `time`
- `open_price`、`open_price_milli`
- `high_price`、`high_price_milli`
- `low_price`、`low_price_milli`
- `close_price`、`close_price_milli`
- `last_close_price`、`last_close_price_milli`
- `volume`
- `amount`、`amount_milli`
- `order_count`
- `up_count`、`down_count`

### `get_kline_all(code: str, freq: str, kind: str = "stock") -> KlineResponse`

自动翻页拉取完整 K 线。

说明：

- 推荐调用形式是 `get_kline_all(code, freq)`。
- 当前实现也兼容 `get_kline_all(freq, code)`。

### `get_adjusted_kline(period, code, adjust="qfq", ...) -> KlineResponse`
### `get_adjusted_kline_all(period, code, adjust="qfq") -> KlineResponse`

返回复权 K 线。

```python
with TdxClient() as client:
    qfq_page = client.get_adjusted_kline("day", "sz000001", adjust="qfq", count=10)
    hfq_all = client.get_adjusted_kline_all("day", "sz000001", adjust="hfq")
```

说明：

- `adjust` 支持 `qfq` / `hfq`
- 这两个方法是计算型 helper，底层链路是：`gbbq -> xdxr -> factors -> adjusted_kline`

## 8. 集合竞价

### `get_call_auction(code: str, include_raw: bool = False) -> CallAuctionResponse`

读取集合竞价序列。

```python
with TdxClient() as client:
    auction = client.get_call_auction("sz000001", include_raw=True)

print(auction.count)
print(auction.items[0].time, auction.items[0].price, auction.items[0].flag)
print(auction.items[0].raw_hex)
```

`CallAuctionResponse` 关键字段：

- `count`
- `items`
- `raw_frame_hex`
- `raw_payload_hex`

`CallAuctionItem` 关键字段：

- `time`：Python `datetime`
- `price`、`price_milli`
- `match`：撮合量
- `unmatched`：未撮合量
- `flag`：`1` 表示买未撮合，`-1` 表示卖未撮合
- `raw_hex`：单条记录原始十六进制，仅在 `include_raw=True` 时返回

## 9. 股本变动 / 复权 / 因子

### `get_gbbq(code: str, include_raw: bool = False) -> GbbqResponse`

读取原始股本变迁表。

```python
with TdxClient() as client:
    gbbq = client.get_gbbq("sz000001", include_raw=True)
    print(gbbq.items[0].time, gbbq.items[0].category_name)
```

`GbbqItem` 关键字段：

- `code`
- `time`
- `category`、`category_name`
- `c1`、`c2`、`c3`、`c4`

### `get_xdxr(code: str) -> list[XdxrItem]`

从 `gbbq` 数据中过滤出除权除息相关记录。

```python
with TdxClient() as client:
    xdxr = client.get_xdxr("sz000001")
    print(xdxr[0].time, xdxr[0].fenhong, xdxr[0].peigujia)
```

### `get_equity_changes(code: str) -> EquityResponse`

从股本变动记录中提取流通股本、总股本变化序列。

```python
with TdxClient() as client:
    equity_changes = client.get_equity_changes("sz000001")
    print(equity_changes.count)
```

### `get_equity(code: str, on=None) -> EquityItem | None`

取某一日期对应的股本记录；默认取最新有效记录。

```python
with TdxClient() as client:
    latest_equity = client.get_equity("sz000001")
    equity_on_day = client.get_equity("sz000001", on="2026-03-06")
```

### `get_turnover(code: str, volume: int | float, on=None, unit: str = "hand") -> float`

根据股本记录计算换手率百分比。

```python
with TdxClient() as client:
    turnover = client.get_turnover("sz000001", 1000, on="2026-03-06", unit="hand")
    print(turnover)
```

说明：

- `volume`：成交量数值
- `unit`：支持 `share` / `shares` / `stock` / `hand` / `hands` / `lot` / `lots`
- 默认 `hand`，即按“手”解释，内部会乘以 `100`

### `get_factors(code: str) -> FactorResponse`

根据日 K 与除权除息记录构造复权因子序列。

```python
with TdxClient() as client:
    factors = client.get_factors("sz000001")
    print(factors.items[0].qfq_factor, factors.items[0].hfq_factor)
```

`FactorItem` 关键字段：

- `time`
- `last_close_price`、`last_close_price_milli`
- `pre_last_close_price`、`pre_last_close_price_milli`
- `qfq_factor`
- `hfq_factor`

## 10. 衍生 helper

### `get_trade_minute_kline(code: str) -> KlineResponse`
### `get_history_trade_minute_kline(code: str, date) -> KlineResponse`

把逐笔数据聚合成分钟 K 线。

```python
with TdxClient() as client:
    live_trade_kline = client.get_trade_minute_kline("sz000001")
    history_trade_kline = client.get_history_trade_minute_kline("sz000001", "2026-03-06")
```

说明：

- 这是计算型 helper，不是独立协议接口。
- 它基于逐笔成交聚合得到 1 分钟 K 线序列。

## 11. 服务层对象

服务层不是必须使用；它们是围绕公开 API 做的便捷封装。

### `CodesService`

```python
from eltdx import TdxClient
from eltdx.services import CodesService

with TdxClient() as client:
    service = CodesService(client)
    service.refresh()
    print(service.get_name("sz000001"))
    print(service.get_stocks()[:5])
```

主要方法：

- `get_page(exchange, start=0, limit=1000)`
- `get_all(exchange)`
- `refresh()`
- `all()`
- `by_exchange(exchange)`
- `get(code)` / `get_name(code)`
- `stocks()` / `etfs()` / `indexes()`
- `get_stocks()` / `get_etfs()` / `get_indexes()`

说明：

- 内部是**内存缓存**，不是数据库缓存。
- 适合做代码表查名、分市场遍历、快速过滤。

### `WorkdayService`

```python
from eltdx import TdxClient
from eltdx.services import WorkdayService

with TdxClient() as client:
    workday = WorkdayService(client)
    print(workday.is_workday("2026-03-06"))
    print(workday.next_workday("2026-03-06"))
```

主要方法：

- `today()`
- `normalize(value)`
- `text(value)`
- `same_day(left, right)`
- `refresh()`
- `is_workday(value)` / `today_is_workday()`
- `range(start, end)` / `iter_days(start, end)`
- `next_workday(value)` / `previous_workday(value)`

说明：

- 传入 `client` 时，会用 `sh000001` 的日线历史构建真实交易日集合。
- 不传 `client` 时，会退化为“周一到周五”的简单规则。

### `GbbqService`

```python
from eltdx import TdxClient
from eltdx.services import GbbqService

with TdxClient() as client:
    service = GbbqService(client)
    print(service.get_equity("sz000001"))
    print(service.get_factors("sz000001").count)
```

主要方法：

- `refresh(code, include_raw=False)`
- `clear(code=None)`
- `get_gbbq(code, refresh=False, include_raw=False)`
- `items(code)`
- `get_xdxr(code)`
- `get_equity_changes(code)`
- `get_equity(code, on=None)`
- `get_turnover(code, volume, on=None, unit="hand")`
- `get_factors(code)`
- `get_adjusted_kline(code, freq, adjust="qfq")`
- `get_adjusted_kline_all(code, freq, adjust="qfq")`

说明：

- 内部维护 `gbbq` 与 `factors` 的**内存缓存**。
- 适合在同一代码上重复做复权和股本查询。

## 12. 模型别名与延伸阅读

当前还保留了几组兼容别名：

- `Code = SecurityCode`
- `MinuteSeries = MinuteResponse`
- `Trade = TradeItem`
- `TradePage = TradeResponse`
- `Kline = KlineItem`
- `KlinePage = KlineResponse`

继续阅读：

1. API 用法总览：[`README.md`](../README.md)
2. 字段中文说明：[`FIELD_REFERENCE.md`](./FIELD_REFERENCE.md)
3. 调试与 raw 比对：[`DEBUG_GUIDE.md`](./DEBUG_GUIDE.md)
4. 使用示例：[`EXAMPLES.md`](./EXAMPLES.md)
