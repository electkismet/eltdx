# 调用方法与解析字段手册

想快速知道某个方法怎么传参、底层走哪个接口、返回对象里有哪些常用字段，就看这里。

更底层的命令 payload、offset 和原始字段说明见 [COMMANDS_7709.md](COMMANDS_7709.md)。只想查模型字段总表时看 [FIELD_REFERENCE.md](FIELD_REFERENCE.md)。

## 通用约定

| 约定            | 说明                                                              |
| ------------- | --------------------------------------------------------------- |
| `code`        | 支持 `sz000001`、`sh600000`、`bj920001` 这类完整代码；部分场景也支持只传六位代码并自动推断市场 |
| `market`      | 市场，常用 `sz`、`sh`、`bj`，也可用 `0`、`1`、`2`                            |
| `include_raw` | 是否保留原始 payload / record hex，用于抓包对照和协议字段排查                       |
| `refresh`     | 部分 Helper 支持；用于跳过对应的内存缓存重新请求服务端，是否可用以具体方法签名为准                                                 |
| `full_code`   | 返回模型属性，等于 `exchange + code`                                     |
| `*_raw`       | 协议原始值或原始 bytes，主要用于排查解析                                         |
| `*_milli`     | 毫厘价格，通常 `price = price_milli / 1000`                            |
| `volume`      | 成交明细、分时、K 线里大多按“手”理解，具体字段以对应模型说明为准                              |

## 客户端入口

### `TdxClient(...)`

真实 `7709` 行情客户端。默认使用包内主站列表；进入 `with`、手动 `connect()` 或首次请求时会建立 socket 连接，并按 `heartbeat_interval` 自动保活。

```python
from eltdx import TdxClient

with TdxClient(timeout=3) as client:
    quote = client.helpers.full_quotes("sz000001")
```

| 参数                   | 含义                               |
| -------------------- | -------------------------------- |
| `host`               | 单个主站，例如 `"116.205.183.150:7709"` |
| `hosts`              | 多个主站，客户端按顺序尝试                    |
| `timeout`            | 单次 socket 请求等待秒数                 |
| `pool_size`          | 连接池连接数                           |
| `probe_hosts`        | 是否启动时测速主站                        |
| `heartbeat_interval` | 自动心跳间隔秒数；传 `None` 关闭             |

### `TdxClient.from_hosts(...)`

显式使用连接池创建客户端，适合长时间运行或多请求场景。

```python
with TdxClient.from_hosts(pool_size=2, probe_hosts=True, timeout=3) as client:
    quotes = client.helpers.full_quotes(["sz000001", "sh600000"])
```

### `TdxClient.in_memory()`

内存客户端，主要给单元测试和示例使用，不连接真实主站。

```python
client = TdxClient.in_memory()
```

## 连接和会话

### `client.connect()` / `client.close()`

手动打开和关闭底层连接。多数情况下直接用 `with TdxClient(...) as client:` 即可。

### `client.ping()`

检查客户端是否可用。

| 项目  | 内容                        |
| --- | ------------------------- |
| 返回  | 字符串，内存客户端返回 `"pong"`      |
| 底层  | transport 层健康检查，不对应具体行情字段 |

<a id="method-session-handshake"></a>

### `client.session.handshake()`

连接握手，对应 `0x000d`。

```python
info = client.session.handshake()
```

| 参数  | 含义      |
| --- | ------- |
| 无   | 不需要业务参数 |

| 返回字段                                        | 含义             |
| ------------------------------------------- | -------------- |
| `server_datetime`                           | 服务端日期时间        |
| `session_minutes_1` / `session_minutes_2`   | 服务端返回的交易时段分钟信息 |
| `server_date_1` / `server_date_2`           | 服务端日期          |
| `server_name`                               | 主站名称           |
| `product_tag`                               | 产品标识           |
| `unknown_time_1_raw` / `unknown_time_2_raw` | 原始时间相关字段       |
| `flags_raw` / `tail_control_raw`            | 原始控制字段         |
| `raw_payload`                               | 原始 payload     |

<a id="method-session-heartbeat"></a>

### `client.session.heartbeat()`

心跳保活，对应 `0x0004`。真实 socket 默认后台自动发，业务代码一般不需要手动调。

```python
ack = client.session.heartbeat()
```

| 返回字段              | 含义         |
| ----------------- | ---------- |
| `reserved`        | 保留字节       |
| `server_date_raw` | 原始日期       |
| `server_date`     | 解析后的日期     |
| `raw_payload`     | 原始 payload |

## 代码表

<a id="method-codes-count"></a>

### `client.codes.count(market)`

查询某个市场的证券数量，对应 `0x044e`。

```python
count = client.codes.count("sz")
```

| 参数        | 含义                                |
| --------- | --------------------------------- |
| `market`  | `sz`、`sh`、`bj`                    |

| 返回    | 含义      |
| ----- | ------- |
| `int` | 该市场代码数量 |

<a id="method-codes-list"></a>

### `client.codes.list(market, start=0, limit=1600)`

分页查询代码表，对应 `0x044d`。

```python
items = client.codes.list("sz", start=0, limit=1600)
```

| 参数       | 含义       |
| -------- | -------- |
| `market` | 市场       |
| `start`  | 从第几条开始   |
| `limit`  | 本页最多取多少条 |

<a id="method-codes-all"></a>

### `client.codes.all(market)`

自动分页拉取某市场全量代码表。

```python
items = client.codes.all("sz")
```

| 返回模型                 | 说明      |
| -------------------- | ------- |
| `list[SecurityCode]` | 代码表记录列表 |

| `SecurityCode` 字段        | 含义                                           |
| ------------------------ | -------------------------------------------- |
| `exchange` / `market_id` | 市场前缀和市场编号                                    |
| `code` / `full_code`     | 六位代码 / 完整代码                                  |
| `name`                   | 证券名称                                         |
| `multiple`               | 协议价格换算相关倍数                                   |
| `decimal`                | 小数位                                          |
| `previous_close_price`   | 昨收参考价                                        |
| `volume_ratio_base`      | 量比相关基础值                                      |
| `category`               | 本地派生品种分类，如 `a_share`、`b_share`、`etf`、`index` |
| `category_reason`        | 分类规则说明                                       |
| `board`                  | 本地派生板块，如主板、创业板、科创板、北交所                       |
| `board_reason`           | 板块规则说明                                       |

### 代码过滤便捷方法

这些方法都基于 `0x044d` 代码表的 `category` 派生字段过滤。

```python
client.codes.all_stocks()
client.codes.all_a_shares()
client.codes.all_etfs()
client.codes.all_indices()
client.codes.stock_count("sz")
client.codes.a_share_count("sz")
```

| 方法                          | 返回              |
| --------------------------- | --------------- |
| `all_stocks()`              | A 股 + B 股完整代码列表 |
| `all_a_shares()`            | A 股完整代码列表       |
| `all_etfs()`                | ETF 完整代码列表       |
| `all_indices()`             | 指数完整代码列表       |
| `stock_count(market)`       | 某市场股票数量         |
| `a_share_count(market)`     | 某市场 A 股数量         |

## 行情快照和列表

<a id="method-quotes-snapshots"></a>

### `client.quotes.get_snapshots(codes)` / `client.helpers.full_quotes(codes)`

按显式代码列表查询行情。`client.quotes.get_snapshots()` 直接对应 `0x054c`，当前实盘只稳定确认买一 / 卖一；`client.helpers.full_quotes()` 组合 `0x054c + 0x0547` 返回实时完整五档。

```python
quotes = client.quotes.get_snapshots(["sz000001", "sh600000"])
quotes = client.helpers.full_quotes(["sz000001", "sh600000"])
quote = client.helpers.full_quotes("sz000001")[0]
```

| 参数      | 含义        |
| ------- | --------- |
| `codes` | 单个代码或代码列表 |

| 返回模型                  | 说明         |
| --------------------- | ---------- |
| `list[QuoteSnapshot]` | 每个代码一条行情快照 |

| `QuoteSnapshot` 字段                        | 含义            |
| ----------------------------------------- | ------------- |
| `exchange` / `market_id`                  | 市场            |
| `code` / `full_code`                      | 代码            |
| `last_price`                              | 最新价           |
| `pre_close_price`                         | 昨收            |
| `open_price` / `high_price` / `low_price` | 今开 / 最高 / 最低  |
| `total_hand`                              | 总成交量，单位手      |
| `current_hand`                            | 现手            |
| `amount`                                  | 成交额           |
| `inside_dish` / `outer_disc`              | 内盘 / 外盘       |
| `open_amount_yuan`                        | 开盘金额，单位元      |
| `buy_levels` / `sell_levels`              | `get_snapshots()` 为已确认一档；`client.helpers.full_quotes()` 通过 `0x0547` 补齐买一到买五 / 卖一到卖五 |
| `tail_raw`                                | 尾部扩展原始字段      |

| 派生字段           | 计算方式                             |
| -------------- | -------------------------------- |
| `change`       | `last_price - pre_close_price`   |
| `change_pct`   | `change / pre_close_price * 100` |
| `sum_buy_vol`  | 五档买量合计                           |
| `sum_sell_vol` | 五档卖量合计                           |

`buy_levels` 和 `sell_levels` 的单档模型是 `QuoteLevel`：

| `QuoteLevel` 字段   | 含义        |
| ----------------- | --------- |
| `price`           | 档位价格      |
| `volume`          | 档位委托量     |
| `price_delta_raw` | 协议价格差分原始值 |

### `client.quotes.legacy(codes)`

查询 `0x053e` 旧版批量行情。该入口直接发送一次原生请求，调用方需要自行按服务端限制分批。接口返回五档盘口和交易状态原始字段，不做股票筛选或状态分类。

```python
quotes = client.quotes.legacy(["sz000001", "sh600000"])
```

| 返回模型 | 说明 |
| --- | --- |
| `list[LegacyQuote]` | 每个代码一条旧版行情记录 |

`LegacyQuote` 包含行情价、成交量额、内外盘、五档盘口、`trading_status_raw`、四个尾部原始指标以及可选的旧版尾部字段。

### `client.quotes.get_depth(codes)`

按代码列表直接发起一次原生 `0x0547` 刷新，首次刷新建立实时五档，后续由推送队列增量更新。

```python
depth = client.quotes.get_depth(["sz000001", "sh600000"])
depth = client.quotes.get_depth("sz000001")
```

| 返回模型               | 说明             |
| ------------------ | -------------- |
| `QuoteRefreshPage` | 本次刷新返回的五档行情记录 |

<a id="method-quotes-category"></a>

### `client.quotes.list_by_category(category, sort_by=None, start=0, count=80, ascending=False)`

查询分类行情列表，对应 `0x054b`。

```python
page = client.quotes.list_by_category("沪深A股", sort_by="涨幅", count=100)
```

| 参数          | 含义                                                                                                |
| ----------- | ------------------------------------------------------------------------------------------------- |
| `category`  | 分类编号或别名；常用 `"沪深A股"`                                                                               |
| `sort_by`   | 排序字段，可传 `"代码"`、`"现价"`、`"成交额"`、`"涨幅"`、`"封单额"`、`"开盘金额"`、`"涨速"`、`"短换手"`、`"量涨速"`、`"开盘抢筹"`、`"2分钟金额"` 等 |
| `start`     | 起始行                                                                                               |
| `count`     | 本页条数                                                                                              |
| `ascending` | 是否升序；默认降序                                                                                         |

| 返回模型                | 说明     |
| ------------------- | ------ |
| `CategoryQuotePage` | 一页分类行情 |

| `CategoryQuotePage` 字段    | 含义          |
| ------------------------- | ----------- |
| `category`                | 分类编号        |
| `sort_type`               | 排序编号        |
| `start` / `request_count` | 请求起点 / 请求条数 |
| `sort_reverse`            | 排序方向原始值     |
| `records`                 | 行情记录列表      |
| `count`                   | 实际返回条数      |

| `CategoryQuoteRecord` 字段                  | 含义              |
| ----------------------------------------- | --------------- |
| `last_price` / `pre_close_price`          | 最新价 / 昨收        |
| `open_price` / `high_price` / `low_price` | 开高低             |
| `total_hand` / `current_hand`             | 总量 / 现量         |
| `amount`                                  | 成交额             |
| `inside_dish` / `outer_disc`              | 内外盘             |
| `open_amount`                             | 开盘金额            |
| `bid1` / `ask1`                           | 买一 / 卖一价格       |
| `bid_vol1` / `ask_vol1`                   | 买一 / 卖一量        |
| `rise_speed`                              | 涨速              |
| `short_turnover`                          | 短周期换手口径字段       |
| `min2_amount`                             | 近 2 分钟金额口径字段    |
| `opening_rush`                            | 开盘抢筹 / 开盘冲击口径字段 |
| `vol_rise_speed`                          | 量增速             |
| `depth`                                   | 深度口径字段          |
| `extra_meta_raw` / `tail_raw`             | 扩展原始字段          |

| 派生字段            | 计算方式                             |
| --------------- | -------------------------------- |
| `change`        | `last_price - pre_close_price`   |
| `change_pct`    | `change / pre_close_price * 100` |
| `locked_amount` | `bid1 * bid_vol1 * 100`          |

<a id="method-quotes-refresh"></a>

### `client.quotes.refresh(codes=None, cursors=None)`

行情增量刷新，对应 `0x0547`，单次最多 100 个代码。

```python
page = client.quotes.refresh(["sz000001"], cursors={"sz000001": 0})
```

| 参数        | 含义                  |
| --------- | ------------------- |
| `codes`   | 关注代码列表，单次最多 100 个代码 |
| `cursors` | 每个代码的增量游标，通常首次传 `0` |

| 返回模型               | 说明     |
| ------------------ | ------ |
| `QuoteRefreshPage` | 增量行情结果 |

| 字段                | 含义           |
| ----------------- | ------------ |
| `requested_codes` | 请求代码         |
| `records`         | 增量行情记录       |
| `decoded_payload` | 解码后的 payload |
| `raw_payload`     | 原始 payload   |
| `count`           | 记录数          |

`QuoteRefreshRecord` 的主要字段和 `QuoteSnapshot` 接近；它属于增量刷新协议，档位内容以主站实际返回为准。

<a id="method-quotes-push"></a>

### `client.quotes.poll_push(timeout=0.0, parse=False)` / `client.quotes.drain_pushes(parse=False)`

读取 transport 收到但没有匹配到主动请求的推送帧。

| 方法               | 返回                 |
| ---------------- | ------------------ |
| `poll_push()`    | 一条推送帧；没有则返回 `None` |
| `drain_pushes()` | 当前队列里的全部推送帧        |
| `parse=True`     | 尝试解析成业务模型          |

## K 线 / 周期线

<a id="method-bars-get"></a>

### `client.bars.get(code, period="day", ...)`

查询一页 K 线，对应 `0x052d`。

```python
series = client.bars.get("sz000001", period="day", count=800)
```

| 参数            | 含义                                    |
| ------------- | ------------------------------------- |
| `code`        | 证券代码                                  |
| `period`      | 周期                                    |
| `start`       | 起始位置                                  |
| `count`       | 本页条数                                  |
| `adjust`      | 复权模式                                  |
| `anchor_date` | 定点复权日期                                |
| `kind`        | `stock` 或 `index`；指数 K 线可能解析上涨 / 下跌家数 |
| `include_raw` | 是否保留原始 payload                        |

| `period`                                  | 含义                              |
| ----------------------------------------- | ------------------------------- |
| `1m`, `5m`, `15m`, `30m`, `60m`           | 分钟 K 线                          |
| `day`, `week`, `month`, `quarter`, `year` | 日、周、月、季、年                       |
| `10m`, `2d`, `5s`                         | 自定义 N 分钟、N 日、N 秒形式；实际覆盖以服务端支持为准 |

| `adjust`                    | 含义                     |
| --------------------------- | ---------------------- |
| `None` / `none`             | 不复权                    |
| `qfq` / `front`             | 前复权                    |
| `hfq` / `back`              | 后复权                    |
| `fixed_qfq` / `fixed_front` | 定点前复权，需要 `anchor_date` |
| `fixed_hfq` / `fixed_back`  | 定点后复权，需要 `anchor_date` |

| 返回模型          | 说明     |
| ------------- | ------ |
| `KlineSeries` | 一组 K 线 |

| `KlineSeries` 字段                                | 含义          |
| ----------------------------------------------- | ----------- |
| `exchange` / `market_id` / `code` / `full_code` | 市场和代码       |
| `period_name`                                   | 周期名         |
| `start` / `request_count`                       | 请求起点 / 请求条数 |
| `adjust_mode`                                   | 复权模式        |
| `anchor_date`                                   | 定点复权日期      |
| `bars`                                          | K 线列表       |
| `count`                                         | 实际 K 线数量    |

| `KlineBar` 字段                     | 含义                  |
| --------------------------------- | ------------------- |
| `time`                            | K 线时间               |
| `open` / `high` / `low` / `close` | 开高低收                |
| `last_close_price_milli`          | 上一根收盘毫厘价            |
| `volume_lots`                     | 成交量，单位手             |
| `amount`                          | 成交额                 |
| `up_count` / `down_count`         | 指数类上涨 / 下跌家数，股票通常为空 |
| `record_hex`                      | 单条 K 线原始十六进制        |

<a id="method-bars-all"></a>

### `client.bars.all(code, period="day", ...)`

自动分页拉取 K 线，直到服务端返回空页。

```python
series = client.bars.all("sz000001", period="day")
```

| 参数          | 含义                |
| ----------- | ----------------- |
| `page_size` | 每页条数              |
| `max_pages` | 最多拉几页；防止异常情况下无限循环 |

返回仍然是合并后的 `KlineSeries`。

<a id="method-bars-adjusted"></a>

### `client.bars.get(..., adjust=...)` / `client.bars.all(..., adjust=...)`

直接使用 `0x052d` 的服务端复权参数。

```python
client.bars.get("sz000001", period="day", adjust="qfq")
client.bars.all("sz000001", period="day", adjust="hfq")
client.bars.get("sz000001", period="day", adjust="fixed_qfq", anchor_date="2024-06-03")
```

`anchor_date` 会透传给 `0x052d`，可用于定点前复权 / 定点后复权。

## 分时

<a id="method-minutes-today"></a>

### `client.minutes.today(code)`

查询主站当前保存的分时，对应 `0x0537`。凌晨、周末或节假日可能返回最近交易日数据。

```python
series = client.minutes.today("sz000001")
```

| 返回模型           | 说明   |
| -------------- | ---- |
| `MinuteSeries` | 分时序列 |

| `MinuteSeries` 字段                               | 含义              |
| ----------------------------------------------- | --------------- |
| `exchange` / `market_id` / `code` / `full_code` | 市场和代码           |
| `trading_date`                                  | 原始当日分时响应不带日期，通常为空    |
| `prev_close`                                    | 昨收；历史 / 近期接口通常有 |
| `open_price`                                    | 今开；近期接口通常有      |
| `points`                                        | 分时点列表           |
| `count`                                         | 分时点数量           |
| `volume_sum`                                    | 分时成交量合计         |

| `MinutePoint` 字段 | 含义              |
| ---------------- | --------------- |
| `index`          | 分时序号            |
| `time_label`     | 时间文本            |
| `time`           | 带日期的时间；当日分时通常为空 |
| `price`          | 当前价格            |
| `avg_price`      | 均价              |
| `volume`         | 该分钟成交量，单位手      |
| `record_hex`     | 单条原始十六进制        |

<a id="method-minutes-history"></a>

### `client.minutes.history(code, trading_date)`

查询指定日期历史分时，对应 `0x0fb4`。

```python
series = client.minutes.history("sz000001", "2026-05-20")
series = client.minutes.history("sz000001", "2026-05-20")
```

| 参数             | 含义                                    |
| -------------- | ------------------------------------- |
| `trading_date` | 交易日，支持 `YYYY-MM-DD`、`YYYYMMDD`、`date` |

返回字段同 `MinuteSeries` / `MinutePoint`。

<a id="method-minutes-recent"></a>

### `client.minutes.recent(code, trading_date=None)`

查询近期历史分时，对应 `0x0feb`。

```python
series = client.minutes.recent("sz000001", "2026-05-20")
```

| 参数             | 含义                  |
| -------------- | ------------------- |
| `trading_date` | 近期窗口内的交易日；不传时使用当前日期 |

返回字段同 `MinuteSeries`，通常额外有 `prev_close`、`open_price`、`date_selector_raw`。

<a id="method-minutes-aux"></a>

### `client.minutes.aux(code, kind="buy_sell_strength")`

查询分时副图序列，对应 `0x051b`。

```python
series = client.minutes.aux("sz000001", kind="buy_sell_strength")
series = client.minutes.aux("sz000001", kind="volume_comparison")
```

| `kind`                                          | 含义            |
| ----------------------------------------------- | ------------- |
| `buy_sell_strength` / `buy_sell` / `commission` | 买卖力道 / 委买委卖口径 |
| `volume_comparison` / `volume_compare`          | 成交对比口径        |

| 返回模型              | 说明     |
| ----------------- | ------ |
| `MinuteAuxSeries` | 分时副图序列 |

| 字段                                                                 | 含义       |
| ------------------------------------------------------------------ | -------- |
| `kind`                                                             | 副图类型     |
| `selector_raw`                                                     | 副图选择器原始值 |
| `points`                                                           | 副图点列表    |
| `points[].time_label`                                              | 时间       |
| `points[].series_a` / `series_b`                                   | 两条序列的通用值 |
| `buy_commission` / `sell_commission`                               | 买卖力道口径字段 |
| `previous_day_cumulative_volume` / `current_day_cumulative_volume` | 成交对比口径字段 |

<a id="method-minutes-sparkline"></a>

### `client.minutes.sparkline(code, selector=1, window=20)`

查询单标的小走势图，对应 `0x0fd1`。

```python
series = client.minutes.sparkline("sz000001", selector=1, window=20)
```

| 返回模型              | 说明       |
| ----------------- | -------- |
| `SparklineSeries` | 小走势图价格序列 |

| 字段                               | 含义            |
| -------------------------------- | ------------- |
| `base_price`                     | 基准价           |
| `prices`                         | 价格序列          |
| `selector_raw` / `selector_echo` | 请求选择器 / 服务端回显 |
| `window_or_count_raw`            | 请求窗口参数        |
| `max_count_raw`                  | 服务端返回最大数量口径   |
| `count`                          | 实际价格点数量       |

## 成交明细

<a id="method-trades-today"></a>

### `client.trades.today(code, start=0, count=1800)`

查询主站当前保存的混合明细，对应 `0x0fc5`。凌晨、周末或节假日可能返回最近交易日数据。返回普通成交、`status=8` 集合竞价快照和 09:25 正式开盘撮合。

```python
page = client.trades.today("sz000001", start=0, count=1800)
page = client.trades.today("sz000001")
```

| 参数            | 含义             |
| ------------- | -------------- |
| `start`       | 起始位置           |
| `count`       | 本页条数           |
| `include_raw` | 是否保留原始 payload |

<a id="method-trades-history"></a>

### `client.trades.history(code, trading_date, start=0, count=1800)`

查询历史混合明细增强接口，对应 `0x0fc6`。返回普通成交、`status=8` 集合竞价快照和 09:25 正式开盘撮合。

```python
page = client.trades.history("sz000001", "2026-05-20")
```

<a id="method-trades-all"></a>

### `client.trades.all_today(...)` / `client.trades.all_history(...)`

自动分页拉取成交明细，直到服务端返回空页。

```python
page = client.trades.all_today("sz000001")
page = client.trades.all_history("sz000001", "2026-05-20")
```

| 返回模型        | 说明          |
| ----------- | ----------- |
| `TradePage` | 一页或合并后的成交明细 |

| `TradePage` 字段                                  | 含义                                           |
| ----------------------------------------------- | -------------------------------------------- |
| `exchange` / `market_id` / `code` / `full_code` | 市场和代码                                        |
| `trading_date`                                  | 历史成交日期；当前 `0x0fc5` 原始响应不带日期，因此保持为空 |
| `start` / `request_count`                       | 请求起点 / 请求条数                                  |
| `ticks`                                         | 原始混合记录                                       |
| `auction_snapshots`                             | `status=8` 集合竞价快照                           |
| `opening_matches`                               | 09:25 正式开盘撮合                                |
| `count`                                         | 混合记录条数                                       |
| `has_more`                                      | 单页结果中 `count >= request_count` 时为 `True`，表示可能还有下一页 |

### `client.trades.auction_snapshots(code, trading_date=None, ...)`

从当前或历史成交明细分页结果中筛出 `status=8` 的集合竞价过程快照。`trading_date=None` 使用 `0x0fc5`，传入日期使用 `0x0fc6`。

```python
current_snapshots = client.trades.auction_snapshots("sz000001")
history_snapshots = client.trades.auction_snapshots("sz000001", "2026-05-20")
```

| `TradeTick` 字段                | 含义                             |
| ----------------------------- | ------------------------------ |
| `index` / `absolute_index`    | 页内序号 / 全局序号                    |
| `time_minutes` / `time_label` | 分钟数 / 时间文本                     |
| `trade_datetime`              | 成交时间                           |
| `price` / `price_milli`       | 成交价 / 毫厘价                      |
| `volume`                      | 原始数量字段；普通成交/正式撮合时是成交量，竞价快照时是虚拟匹配量                        |
| `order_count`                 | 原始笔数字段；竞价快照时是带符号未匹配量             |
| `event_kind`                  | `trade`、`auction_snapshot` 或 `opening_match` |
| `is_auction_snapshot`         | 是否为集合竞价快照                               |
| `is_opening_match`            | 是否为 09:25 正式开盘撮合                         |
| `is_trade`                     | 是否为普通成交                                   |
| `auction_matched_volume`      | 竞价快照虚拟匹配量                               |
| `auction_unmatched_signed_volume` | 竞价快照带符号未匹配量                         |
| `auction_unmatched_volume`    | 竞价快照未匹配量绝对值                           |
| `side`                        | 方向，`buy`、`sell`、`neutral` 或状态名 |
| `status_raw`                  | 方向 / 状态原始值                     |
| `trade_amount_yuan`           | `price * volume * 100`；竞价快照仅为估算，不代表实际成交金额    |

分类规则：`status_raw == 8` 为 `auction_snapshot`；时间为 `09:25` 且不是 `status=8` 为 `opening_match`；其余为 `trade`。竞价快照的 `volume` / `order_count` 仍保留原始字段，不应按实际成交解释。

成交明细完整分页入口：

```python
client.trades.all_today("sz000001")
client.trades.all_history("sz000001", "2026-05-20")
```

`client.trades.today()` 和 `client.trades.history()` 每次只返回一页，用于手动分页、抽样或控制单次请求量。

## 集合竞价

<a id="method-auctions-series"></a>

### `client.auctions.series(code)`

查询主站当前保存的集合竞价快照明细，对应 `0x056a`。该接口专门返回竞价过程，不是逐笔成交；记录通常从 `09:15:00` 到 `09:24:57`，也可能到 `09:24:59` 或 `09:25:00`，尾盘还可能有 `14:57` 到 `15:00`。

```python
series = client.auctions.series("sz000001")
```

| 返回模型            | 说明     |
| --------------- | ------ |
| `AuctionSeries` | 集合竞价明细 |

| `AuctionPoint` 字段          | 含义                                   |
| -------------------------- | ------------------------------------ |
| `time_label`               | 时间                                   |
| `time_seconds`             | 当日秒数                                 |
| `price` / `price_milli`    | 竞价价格 / 毫厘价                           |
| `matched_volume`           | 虚拟成交量，单位手                            |
| `unmatched_volume`         | 未匹配量，单位手                             |
| `unmatched_direction_raw`  | 未匹配方向原始值                             |
| `matched_amount_estimated` | 估算成交额，`price * matched_volume * 100` |

<a id="method-auction-0925"></a>

### `client.helpers.auction_0925(code, date)`

扫描成交明细中的 09:25 正式开盘撮合：目标日期等于主站当前交易日时使用 `0x0fc5`，其他日期使用 `0x0fc6`；`status=8` 的竞价快照不会被选中。

```python
result = client.helpers.auction_0925("sz000001", "2026-05-20")
```

| 返回模型                | 说明           |
| ------------------- | ------------ |
| `Auction0925Result` | 09:25 正式撮合结果 |

| 字段                      | 含义            |
| ----------------------- | ------------- |
| `has_auction_0925`      | 是否找到 09:25 成交 |
| `price` / `price_milli` | 竞价成交价         |
| `volume`                | 成交量，单位手       |
| `amount`                | 成交额           |
| `status` / `side`       | 原始状态 / 方向     |
| `pages_used`            | 扫描了几页成交明细       |
| `source_mode`           | 数据来源说明        |

## 股本变迁、除权除息和复权因子

<a id="method-corporate-capital-changes"></a>

### `client.corporate.capital_changes(code)`

查询股本变迁 / 除权相关事件，对应 `0x000f`。

```python
block = client.corporate.capital_changes("sz000001")
```

| 参数            | 含义                     |
| ------------- | ---------------------- |
| `code`        | 证券代码                   |
| `include_raw` | 是否保留原始 payload         |

| 返回模型                 | 说明     |
| -------------------- | ------ |
| `CapitalChangeBlock` | 股本事件列表 |

| `CapitalChangeRecord` 字段                          | 含义                  |
| ------------------------------------------------- | ------------------- |
| `date`                                            | 事件日期                |
| `category_raw` / `category`                       | 事件类别编号              |
| `category_name`                                   | 类别名称                |
| `c1_value` / `c2_value` / `c3_value` / `c4_value` | 按类别解码后的四个业务值        |
| `c1_raw` / `c2_raw` / `c3_raw` / `c4_raw`         | 四个原始字段              |
| `time`                                            | 事件日期派生出的 `15:00` 时间 |

<a id="method-corporate-xdxr"></a>

### `client.helpers.xdxr(code)`

从 `0x000f` 股本变迁里筛出除权除息记录。

```python
records = client.helpers.xdxr("sz000001")
```

| 返回模型               | 字段                                                                           |
| ------------------ | ---------------------------------------------------------------------------- |
| `list[XdxrRecord]` | `date`、`category`、`category_name`、`fenhong`、`peigujia`、`songzhuangu`、`peigu` |

| 字段            | 含义  |
| ------------- | --- |
| `fenhong`     | 分红  |
| `peigujia`    | 配股价 |
| `songzhuangu` | 送转股 |
| `peigu`       | 配股  |

<a id="method-corporate-equity"></a>

### `client.helpers.equity_changes(code)` / `client.helpers.equity(code, on=None)`

从股本变迁里整理股本变化记录，并取某日之前最近一条。

```python
changes = client.helpers.equity_changes("sz000001")
equity = client.helpers.equity("sz000001", on="2026-05-20")
```

| 返回模型             | 字段                                                              |
| ---------------- | --------------------------------------------------------------- |
| `EquityResponse` | `count`、`items`                                                 |
| `EquityRecord`   | `date`、`category`、`category_name`、`float_shares`、`total_shares` |

<a id="method-corporate-turnover"></a>

### `client.helpers.turnover(code, volume, on=None, unit="hand")`

用成交量和流通股本计算换手率。

```python
turnover = client.helpers.turnover("sz000001", 123456, on="2026-05-20", unit="hand")
```

| 参数       | 含义                     |
| -------- | ---------------------- |
| `volume` | 成交量                    |
| `unit`   | `hand` 表示手，`share` 表示股 |

| 返回      | 计算                  |
| ------- | ------------------- |
| `float` | `成交股数 / 流通股本 * 100` |

<a id="method-corporate-factors"></a>

### `client.helpers.factors(code)` / `client.helpers.local_adjusted_kline(...)`

用不复权日 K 和除权除息记录计算本地复权因子；普通取复权 K 线优先用 `0x052d` 服务端复权参数。

```python
factors = client.helpers.factors("sz000001")
local_qfq = client.helpers.local_adjusted_kline("sz000001", period="day", adjust="qfq")
```

| 返回模型             | 字段                                                                         |
| ---------------- | -------------------------------------------------------------------------- |
| `FactorResponse` | `count`、`items`                                                            |
| `FactorRecord`   | `time`、`last_close_price`、`pre_last_close_price`、`qfq_factor`、`hfq_factor` |

## 财务基础信息

<a id="method-corporate-finance-batch"></a>

### `client.corporate.finance_batch(codes, fields=None)`

批量查询财务基础信息，对应 `0x0010`。

```python
batch = client.corporate.finance_batch(["sz000001", "sh600000"])
selected = client.corporate.finance_batch(["sz000001"], fields=["流通股本", "total_shares"])
```

| 参数            | 含义                              |
| ------------- | ------------------------------- |
| `codes`       | 单个代码或代码列表                       |
| `fields`      | 本地字段过滤；不改变服务端请求                 |
| `include_raw` | 是否保留原始 payload                  |

| 返回模型           | 说明       |
| -------------- | -------- |
| `FinanceBatch` | 财务记录批量结果 |

| `FinanceRecord` 字段              | 含义          |
| ------------------------------- | ----------- |
| `updated_date`                  | 财务数据更新日期    |
| `ipo_date`                      | 上市日期        |
| `eps_raw`                       | 每股收益原始值     |
| `province_raw` / `industry_raw` | 地区 / 行业原始编号 |
| `liu_tong_gu_ben_raw_float`     | 流通股本原始万股口径  |
| `zong_gu_ben_raw_float`         | 总股本原始万股口径   |
| `zong_zi_chan_raw_float`        | 总资产原始千元口径   |
| `jing_li_run_raw_float`         | 净利润原始千元口径   |
| `record_hex`                    | 单条原始十六进制    |

| 派生字段                 | 计算方式                                |
| -------------------- | ----------------------------------- |
| `circulating_shares` | `liu_tong_gu_ben_raw_float * 10000` |
| `total_shares`       | `zong_gu_ben_raw_float * 10000`     |
| `total_assets_yuan`  | `zong_zi_chan_raw_float * 1000`     |
| `net_profit_yuan`    | `jing_li_run_raw_float * 1000`      |

## 特殊品种涨跌停限制

<a id="method-limits-special"></a>

### `client.limits.special(start_index=0)` / `client.limits.scan_special(...)`

查询特殊品种涨跌停限制表，对应 `0x0452`。这个接口按表内位置分页。

```python
page = client.limits.special(start_index=0)
records = client.limits.scan_special()
```

| 方法                                            | 含义        |
| --------------------------------------------- | --------- |
| `special(start_index=0)`                      | 从指定行号取一页  |
| `scan_special(start_index=0, max_rows=10000)` | 连续扫描并合并记录 |

| 返回模型                 | 字段                                                                            |
| -------------------- | ----------------------------------------------------------------------------- |
| `SpecialLimitPage`   | `start_index`、`records`、`count`                                               |
| `SpecialLimitRecord` | `exchange`、`market_id`、`code`、`full_code`、`limit_up_price`、`limit_down_price` |

## 服务器文件

### `client.resources.read(path, offset=0, size=30000)`

通过 `0x06b9` 读取一个服务器文件块。`path` 必须为最长 300 字节的 ASCII 路径。

```python
chunk = client.resources.read("zhb.zip", offset=0, size=30000)
```

| 返回模型 | 说明 |
| --- | --- |
| `FileContentChunk` | 包含 `path`、`offset`、`request_size`、`chunk_len`、`content`、`raw_payload` 和 `is_last` |

### `client.resources.download_file(path, chunk_size=30000, max_bytes=None)`

循环读取并拼接完整服务器文件，返回 `bytes`。不猜测文件格式。

### `client.resources.read_stats(path="zhb.zip", chunk_size=30000)`

下载 `zhb.zip`，解压并以 GBK 解析其中的 `tdxstat.cfg` 和 `tdxstat2.cfg`。

| 返回模型 | 说明 |
| --- | --- |
| `TdxStatsResource` | `stat`、`stat2` 分别按 `(market_id, code)` 建立索引，并提供 `row()`、`stats_date`、`stat_count` 和 `stat2_count` |
| `TdxStatRow` | 60 日 Beta、PE TTM、自由流通股本、年内涨停数和连板统计字段 |
| `TdxStat2Row` | 当日/前一日/前两日成交额、封单额，以及当日/前一日开盘量额 |

完整列号、单位和校验边界见[服务器文件读取](methods/7709-服务器文件读取.md)。

## F10 / 资料接口

`client.f10` 走 `7615/TQLEX` HTTP 网关，独立于 `7709` socket 握手。

所有 F10 方法统一返回 `F10Response`：

| 字段 / 属性                  | 含义                             |
| ------------------------ | ------------------------------ |
| `entry`                  | 实际调用的 TQLEX Entry              |
| `request_body`           | 实际发送的 JSON body                |
| `error_code`             | 服务端错误码                         |
| `ok`                     | `error_code` 为 `0` 或空时为 `True` |
| `tables` / `result_sets` | 返回表集合                          |
| `rows`                   | 第一张表的行                         |
| `first_table`            | 第一张表                           |
| `raw`                    | 原始 JSON                        |

每张表是 `F10ResultSet`：

| 字段 / 属性     | 含义                |
| ----------- | ----------------- |
| `key`       | 表名或自动生成的 `table0` |
| `columns`   | 原生列名              |
| `rows`      | 字典行               |
| `row_cells` | 带列位置的单元格，适合处理重复列名 |
| `count`     | 行数                |

### 通用调用

```python
response = client.f10.call("CWServ.tdxf10_gg_gsgk", params=["8", "000034", ""])
response = client.f10.params("CWServ.tdxf10_gg_gsgk", "8", "000034", "")
```

| 方法                                    | 说明                                          |
| ------------------------------------- | ------------------------------------------- |
| `call(entry, body=None, params=None)` | 调任意 Entry；`params` 会包装成 `{"Params": [...]}` |
| `params(entry, *params)`              | CWServ 常用 Params 数组写法                       |

### 常用 F10 方法

| 调用方法                                                   | 底层 Entry                                                 | 返回内容 / 常见字段                               |
| ------------------------------------------------------ | -------------------------------------------------------- | ----------------------------------------- |
| `stock_info(code)`                                     | `CWServ.tdxf10_gg_comreq`                                | 股票基础信息；也用于报告期、题材 ID 等辅助查询                 |
| `business_periods(code)`                               | `CWServ.tdxf10_gg_comreq`                                | 主营构成可选报告期                                 |
| `topic_ids(code)`                                      | `CWServ.tdxf10_gg_comreq`                                | 股票关联题材 ID                                 |
| `company_profile(code, section="8")`                   | `CWServ.tdxf10_gg_gsgk`                                  | 公司概况，默认发行上市信息                             |
| `business_composition(code, report_date=None)`         | `CWServ.tdxf10_gg_jyfx`                                  | 主营收入、成本、毛利、收入占比、毛利率                       |
| `shareholder_change_plans(code, page=1, page_size=20)` | `CWServ.tdxf10_gg_gdyj`                                  | 股东增减持计划                                   |
| `dividend_financing(code, section="fh")`               | `CWServ.tdxf10_gg_fhrz`                                  | 分红方案、股权登记日、除权派息日、股息率                      |
| `allotment_dates(code)`                                | `CWServ.tdxf10_gg_fhrz_zfhpmx`                           | 增发获配可选日期                                  |
| `allotment_details(code, date)`                        | `CWServ.tdxf10_gg_fhrz_zfhpmx`                           | 获配机构、获配数量、获配金额、锁定期                        |
| `finance_report(code, report_type="zcfzb")`            | `CWServ.tdxf10_gg_cwfx`                                  | 财务报表，默认资产负债表                              |
| `finance_diagnosis(code, section="yynl")`              | `CWServ.tdxf10_gg_cwzd`                                  | 营运、盈利、成长、现金流、资产质量等诊断                      |
| `stock_score(code, section="pf")`                      | `CWServ.tdxf10_gg_ggzp`                                  | 综合评分、排名、资金面 / 基本面 / 主题面评分                 |
| `profit_forecast(code)`                                | `CWServ.tdxf10_gg_ybpj`                                  | EPS、归母净利润、营业收入预测                          |
| `ranking_detail(code, section="scpmdela")`             | `CWServ.tdxf10_gg_zxts_rqpm`                             | 市场 / 行业排名明细                               |
| `governance(code, section="wgcl")`                     | `CWServ.tdxf10_gg_zbyz`                                  | 违规处理、担保明细等治理数据                            |
| `hot_topics(code, section="zttzbkz")`                  | `CWServ.tdxf10_gg_rdtc`                                  | 题材名称、关联度、入选日期、入选原因、详情 ID                  |
| `topic_compare(code, topic_id, section="gndbzfsj")`    | `CWServ.tdxf10_gg_rdtc_gndb`                             | 题材内个股对比和排名                                |
| `topic_compare_first(code)`                            | `CWServ.tdxf10_gg_comreq` + `CWServ.tdxf10_gg_rdtc_gndb` | 先取第一个题材 ID，再查题材内对比                        |
| `company_news(code, section="gsyj")`                   | `CWServ.tdxf10_gg_gszx`                                  | 研报、监管措施等公司资讯                              |
| `northbound_holding(code, section="bszj")`             | `CWServ.tdxf10_gg_zlcc`                                  | 沪深股通持股比例、数量和变动                            |
| `detail(detail_type, record_id)`                       | `CWServ.tdxf10_gg_idreq`                                 | 按记录 ID 查正文                                |
| `cache_list(code, kind="gg")`                          | `CWSearch.tzx_rcache`                                    | 新闻 / 公告 / 路演缓存列表；`kind` 可传 `xw`、`gg`、`ly` |
| `announcements(code)`                                  | `CWSearch.tzx_rcache`                                    | 公告列表                                      |
| `news(code)`                                           | `CWSearch.tzx_rcache`                                    | 新闻列表                                      |
| `roadshows(code)`                                      | `CWSearch.tzx_rcache`                                    | 路演列表                                      |
| `theme_market(code, req_id="200743")`                  | `HQServ.hq_nlp_tcihq`                                    | 题材概念行情、相关板块、成分股等                          |
| `valuation(code, req_id="200191")`                     | `HQServ.hq_nlp_gpsj`                                     | PE、PB、市销率、市现率、估值百分位、市值等                   |

F10 的字段名来自服务端返回的 `ColName` / `ColDes`，不同 Entry 的列名可能是 `T001` 这类原生列名，也可能是中文或拼音字段。`response.rows` 会把每行转成字典；如果同一张表出现重复列名，重复列会保存成 `字段名__2`、`字段名__3`。

按具体方法查看常用字段含义时，优先看 [methods/README.md](methods/README.md) 里的单页文档。F10 返回多张表时，可用 `response.tables` 查看每张表的 `columns` 和 `rows`。

## 交易日工具

### `client.workdays`

交易日工具默认绑定当前客户端。绑定真实客户端时，会用基准指数日 K 加载真实交易日；不绑定客户端时退回工作日逻辑。

```python
client.workdays.refresh()
client.workdays.is_workday("2026-05-20")
client.workdays.previous_workday("2026-05-20")
client.workdays.next_workday("2026-05-20")
client.workdays.range("2026-05-01", "2026-05-31")
```

| 方法                                            | 返回 / 含义         |
| --------------------------------------------- | --------------- |
| `today()`                                     | 今天日期            |
| `normalize(value)`                            | 转成 `date`       |
| `text(value)`                                 | 转成 `YYYY-MM-DD` |
| `same_day(left, right)`                       | 判断两个日期是否同一天     |
| `refresh()`                                   | 加载交易日，返回交易日数量   |
| `clear()`                                     | 清空已加载交易日        |
| `is_workday(value)`                           | 是否交易日           |
| `today_is_workday()`                          | 今天是否交易日         |
| `range(start, end, descending=False)`         | 交易日列表           |
| `iter_days(start, end, descending=False)`     | 交易日迭代器          |
| `next_workday(value, include_self=False)`     | 下一个交易日          |
| `previous_workday(value, include_self=False)` | 上一个交易日          |

## JSON 输出

### `to_jsonable(value)` / `to_json(value)`

把 dataclass 模型、日期、bytes、列表、字典转成适合 JSON 的结构或字符串。

```python
from eltdx import to_json, to_jsonable

data = to_jsonable(client.helpers.full_quotes("sz000001"))
text = to_json(data, indent=2)
```

| 方法                                                | 返回                                                 |
| ------------------------------------------------- | -------------------------------------------------- |
| `to_jsonable(value)`                              | Python dict / list / str / int / float 等 JSON 友好对象 |
| `to_json(value, ensure_ascii=False, indent=None)` | JSON 字符串                                           |

## 缓存方法

### `client.clear_cache()`

清空低频数据缓存。

```python
client.clear_cache()
```

当前缓存仅用于部分 Helper 组合查询：股本变迁结果、`stock_profile_table()` 内部使用的财务批次和已验证的短线统计资源。`client.codes.count()`、`client.codes.all()`、`client.corporate.finance_batch()`、实时行情、分时、成交明细和 K 线均不缓存。

股本变迁 Helper 使用 `refresh=True` 强制刷新，短线统计资源使用 `refresh_stats=True`；`client.clear_cache()` 清空上述全部 Helper 缓存。

## 常用问题

常用问题入口见 [helpers/README.md](helpers/README.md)。

- [想拿某个或某些股票的表头信息怎么办？](helpers/股票信息汇总.md)
- [想查询某个股票都有哪些概念板块怎么办？](helpers/个股概念板块.md)
- [想查询某个概念板块都有哪些股票怎么办？](helpers/概念板块成分股.md)
- [想拿集合竞价数据怎么办？](helpers/竞价数据.md)
- [想给一批股票整理行情表怎么办？](helpers/批量行情表.md)
- [想拿流通市值Z、开盘换手Z、竞价昨比、开盘昨封比、昨封比、封流比和几天几板怎么办？](helpers/短线指标.md)
- [想拿复权或不复权 K 线怎么办？](helpers/复权K线.md)
