# 字段说明

这份文档专门用来解释返回结果里的字段是什么意思。

如果你拿到数据后，不确定某个字段该怎么理解、原始值和解析值是什么关系，就先看这里。

适合这些场景：

- 想知道每个字段是什么意思
- 想把接口字段对应到中文指标名
- 想核对原始值和解析值之间的关系

配套阅读：

- API 用法：[`API_REFERENCE.md`](./API_REFERENCE.md)
- 使用示例：[`EXAMPLES.md`](./EXAMPLES.md)
- 调试指南：[`DEBUG_GUIDE.md`](./DEBUG_GUIDE.md)

样例里的数值来自真实联网返回，只用于帮助理解字段，不代表固定行情结果。

## 1. 通用约定

### 1.1 价格双形态：`*_milli -> *`

大多数价格字段都会同时保留两种形式：

- `xxx_milli`：整数毫厘价格，适合精确计算与比对
- `xxx`：浮点价格，适合展示与直接使用

换算关系：

- `price = price_milli / 1000`
- `open_price = open_price_milli / 1000`
- `close_price = close_price_milli / 1000`

真实例子：

- `price_milli = 10820 -> price = 10.82`
- `last_price_milli = 10820 -> last_price = 10.82`
- `buy1.price_milli = 10820 -> buy1.price = 10.82`

### 1.2 成交额双形态：`amount_milli -> amount`

K 线里会保留成交额双形态：

- `amount_milli`：整数毫厘成交额
- `amount`：浮点成交额

真实例子：

- `amount_milli = 607501376000 -> amount = 607501376.0`

### 1.3 时间字段全部转成 Python 原生对象

本库已经统一把时间字段转成 Python 原生类型：

- 交易日：`date`
- 带时分秒的时间：`datetime`

真实例子：

- `trading_date = 2026-03-07`
- `time = 2026-03-07T09:31:00+08:00`
- `server_time = 2026-03-07T15:33:07.190000+08:00`

### 1.4 原始调试字段

很多协议接口支持 `include_raw=True`，会带上以下字段：

- `raw_frame_hex`：整帧十六进制
- `raw_payload_hex`：payload 十六进制
- `raw_hex`：单条记录十六进制（集合竞价单条明细）

它们主要用于：

- 协议对拍
- 出错时定位解析问题
- 与抓包结果或原始 hex 交叉核验

### 1.5 完整代码与裸六码

有些模型会拆成：

- `exchange`：市场，例如 `sh`、`sz`、`bj`
- `code`：六码，例如 `000001`

如果你需要完整代码：

- `full_code = exchange + code`
- `SecurityCode` 模型里还提供 `full_code` 属性

### 1.6 “协议原始字段”说明

有少数字段虽然已经稳定解析出来，但其**业务口径尚未最终冻结**。这类字段在本文里会明确标注为：

- **协议原始字段**：已能稳定返回，但中文业务语义还不建议过度解读

典型例子：

- `Quote.active1` / `Quote.active2`：当前实现按活跃度字段处理，但业务算法口径仍未冻结
- `Quote.current_hand`：现手数（现量）
- `Quote.rate`：当前实现按涨速口径保守解释，样本中常见 `0.0`
- `SecurityCode.multiple` / `SecurityCode.decimal`：当前实现按“倍数 / 小数位”使用，但业务层通常不直接使用

## 2. `SecurityCode` / `CodePage`

### 2.1 `SecurityCode`

| 字段 | 类型 | 中文含义 | 说明 |
| --- | --- | --- | --- |
| `exchange` | `str` | 市场前缀 | `sh` / `sz` / `bj` |
| `code` | `str` | 六码证券代码 | 例如 `000001` |
| `name` | `str` | 证券名称 / 代码表条目名称 | 可能不是股票，也可能是分类条目 |
| `multiple` | `int` | 倍数 | 用于协议价格换算的基础参数 |
| `decimal` | `int` | 小数位 | 用于协议价格换算的基础参数 |
| `last_price` | `float` | 最近价 | 已转换成浮点价格 |
| `last_price_raw` | `int` | 最近价原始整数 | 与 `last_price` 对应 |
| `full_code` | `property[str]` | 完整代码 | Python 属性，值为 `exchange + code` |

真实样例：

来源：`codes_page_sz_start0_limit20.json`

- `exchange = sz`
- `code = 395001`
- `name = 主板Ａ股`
- `multiple = 100`
- `decimal = 2`
- `last_price_raw = 1153064960`
- `last_price = 1491.0`
- `full_code = sz395001`

说明：

- 这也再次说明：`get_codes*()` 返回的是**底层代码表条目**，不一定是股票。

### 2.2 `CodePage`

| 字段 | 类型 | 中文含义 | 说明 |
| --- | --- | --- | --- |
| `exchange` | `str` | 当前市场 | `sh` / `sz` / `bj` |
| `start` | `int` | 起始偏移 | 分页起点 |
| `count` | `int` | 本页条数 | 实际返回数量 |
| `total` | `int` | 总条数 | 该市场代码表总量 |
| `items` | `list[SecurityCode]` | 当前页数据 | 代码表条目列表 |

## 3. `MinuteItem` / `MinuteResponse`

### 3.1 `MinuteItem`

| 字段 | 类型 | 中文含义 | 说明 |
| --- | --- | --- | --- |
| `time` | `datetime` | 分钟时间 | 例如 `09:31` 对应的完整时间 |
| `price` | `float` | 该分钟价格 | 已解析浮点值 |
| `price_milli` | `int` | 该分钟原始整数价格 | 精确对拍用 |
| `volume` | `int` | 该分钟成交量 | 保留协议数值 |

真实样例：

来源：`minute_live_sz000001.json`

- `time = 2026-03-07T09:31:00+08:00`
- `price_milli = 10820`
- `price = 10.82`
- `volume = 16557`

### 3.2 `MinuteResponse`

| 字段 | 类型 | 中文含义 | 说明 |
| --- | --- | --- | --- |
| `count` | `int` | 分时条数 | 通常 A 股日内为 `240` |
| `trading_date` | `date` | 交易日 | Python `date` |
| `items` | `list[MinuteItem]` | 分时序列 | 每条是一分钟记录 |
| `raw_frame_hex` | `str | None` | 原始 frame 十六进制 | 仅 `include_raw=True` 时返回 |
| `raw_payload_hex` | `str | None` | 原始 payload 十六进制 | 仅 `include_raw=True` 时返回 |

说明：

- `get_minute(code)`：实时分时路径
- `get_minute(code, date)`：历史分时路径
- 两条路径统一返回 `MinuteResponse`

## 4. `TradeItem` / `TradeResponse`

### 4.1 `TradeItem`

| 字段 | 类型 | 中文含义 | 说明 |
| --- | --- | --- | --- |
| `time` | `datetime` | 逐笔成交时间 | 精确到分钟展示，底层由协议时间恢复 |
| `price` | `float` | 成交价 | 浮点值 |
| `price_milli` | `int` | 成交价整数毫厘值 | 精确对拍用 |
| `volume` | `int` | 该笔成交量 | 当前按“手”口径解释 |
| `status` | `int` | 成交方向状态码 | 当前实现按 `0=买`、`1=卖`、`2=中性/汇总` 解释 |
| `side` | `str | None` | 买卖方向 | 由 `status` 解析得到 |
| `order_count` | `int | None` | 订单笔数 / 聚合笔数 | 实时逐笔常有值，历史逐笔常为 `None` |

`status -> side` 当前映射：

- `0 -> buy`
- `1 -> sell`
- `2 -> neutral`
- 其他值 -> `None`

真实样例：

来源：`trades_live_page_sz000001.json`

- `time = 2026-03-07T14:54:00+08:00`
- `price_milli = 10830 -> price = 10.83`
- `volume = 63`
- `status = 0 -> side = buy`
- `order_count = 7`

### 4.2 `TradeResponse`

| 字段 | 类型 | 中文含义 | 说明 |
| --- | --- | --- | --- |
| `count` | `int` | 本页 / 本次返回条数 | 与 `items` 数量一致 |
| `trading_date` | `date` | 交易日 | Python `date` |
| `items` | `list[TradeItem]` | 逐笔列表 | 每条是一笔成交 |
| `raw_frame_hex` | `str | None` | 原始 frame 十六进制 | 仅 `include_raw=True` 时返回 |
| `raw_payload_hex` | `str | None` | 原始 payload 十六进制 | 仅 `include_raw=True` 时返回 |

说明：

- `get_trades(code)`：实时逐笔分页
- `get_trades(code, date)`：历史逐笔分页
- `get_trades_all(...)`：自动翻页拿完整逐笔

### 4.3 `Auction0925Result`

| 字段 | 类型 | 中文含义 | 说明 |
| --- | --- | --- | --- |
| `code` | `str` | 完整证券代码 | 统一规范成 `sh` / `sz` 前缀加六码 |
| `trading_date` | `date` | 交易日 | Python `date` |
| `has_auction_0925` | `bool` | 是否找到 `09:25` 那一笔 | `True` 表示命中，`False` 表示这天没有 `09:25` 成交 |
| `price` | `float \| None` | `09:25` 成交价 | 未命中时为 `None` |
| `price_milli` | `int \| None` | `09:25` 成交价整数毫厘值 | 精确对拍用 |
| `volume` | `int \| None` | `09:25` 成交量 | 当前按“手”口径解释 |
| `amount` | `float \| None` | `09:25` 成交额 | 当前按 `price * volume * 100` 计算 |
| `status` | `int \| None` | 方向状态码 | 与 `TradeItem.status` 口径一致 |
| `side` | `str \| None` | 买卖方向 | 由 `status` 解析得到 |
| `pages_used` | `int` | 本次定位探测页数 | 用于性能诊断 |
| `source_mode` | `str` | 命中路径标记 | 用于调试，不建议当成业务字段 |

说明：

- `get_auction_0925(code, date)` 只用于历史逐笔场景。
- 没找到 `09:25` 时，只需要看 `has_auction_0925=False` 即可，其余数值字段会统一留空。
- 当前实现会优先做快速探测，不会默认把整天逐笔都拉下来。

## 5. `KlineItem` / `KlineResponse`

### 5.1 `KlineItem`

| 字段 | 类型 | 中文含义 | 说明 |
| --- | --- | --- | --- |
| `time` | `datetime` | K 线时间 | 分钟线 / 日线 / 周线等对应时间点 |
| `open_price` | `float` | 开盘价 | 浮点值 |
| `open_price_milli` | `int` | 开盘价整数毫厘值 | 精确对拍用 |
| `high_price` | `float` | 最高价 | 浮点值 |
| `high_price_milli` | `int` | 最高价整数毫厘值 | 精确对拍用 |
| `low_price` | `float` | 最低价 | 浮点值 |
| `low_price_milli` | `int` | 最低价整数毫厘值 | 精确对拍用 |
| `close_price` | `float` | 收盘价 / 当前周期收价 | 浮点值 |
| `close_price_milli` | `int` | 收盘价整数毫厘值 | 精确对拍用 |
| `last_close_price` | `float | None` | 前收价 | 某些页首记录可能为 `None` |
| `last_close_price_milli` | `int | None` | 前收价整数毫厘值 | 与 `last_close_price` 对应 |
| `volume` | `int` | 成交量 | 周期累计量 |
| `amount` | `float` | 成交额 | 周期累计额 |
| `amount_milli` | `int` | 成交额整数毫厘值 | 精确对拍用 |
| `order_count` | `int | None` | 订单数 | 有则返回 |
| `up_count` | `int | None` | 上涨家数 | 多见于指数型 K 线 |
| `down_count` | `int | None` | 下跌家数 | 多见于指数型 K 线 |

真实样例：

来源：`kline_day_page_sz000001.json`

- `time = 2026-02-13T15:00:00+08:00`
- `open_price_milli = 10960 -> open_price = 10.96`
- `high_price_milli = 10990 -> high_price = 10.99`
- `low_price_milli = 10900 -> low_price = 10.90`
- `close_price_milli = 10910 -> close_price = 10.91`
- `last_close_price = null`
- `volume = 555047`
- `amount_milli = 607501376000 -> amount = 607501376.0`

### 5.2 `KlineResponse`

| 字段 | 类型 | 中文含义 | 说明 |
| --- | --- | --- | --- |
| `count` | `int` | 本页 / 本次返回条数 | 与 `items` 数量一致 |
| `items` | `list[KlineItem]` | K 线列表 | 周期记录 |
| `raw_frame_hex` | `str | None` | 原始 frame 十六进制 | 仅协议接口支持 raw 时返回 |
| `raw_payload_hex` | `str | None` | 原始 payload 十六进制 | 仅协议接口支持 raw 时返回 |

说明：

- `get_kline()`：单页
- `get_kline_all()`：自动翻页全量
- `get_adjusted_kline*()`：在 K 线基础上应用复权因子后的结果

## 6. `Quote` / `QuoteLevel`

### 6.1 `QuoteLevel`

| 字段 | 类型 | 中文含义 | 说明 |
| --- | --- | --- | --- |
| `buy` | `bool` | 买盘还是卖盘 | `True` 表示买档，`False` 表示卖档 |
| `price` | `float` | 档位价格 | 浮点值 |
| `price_milli` | `int` | 档位整数毫厘价格 | 精确对拍用 |
| `number` | `int` | 档位挂单量 | 协议数值 |

说明：

- `buy_levels[0]` 对应买一
- `sell_levels[0]` 对应卖一

### 6.2 `Quote`

| 字段 | 类型 | 中文含义 | 说明 |
| --- | --- | --- | --- |
| `exchange` | `str` | 市场 | `sh` / `sz` / `bj` |
| `code` | `str` | 六码证券代码 | 不带市场前缀 |
| `active1` | `int` | 活跃度字段 1 | 已稳定返回，但具体算法口径未冻结 |
| `active2` | `int` | 活跃度字段 2 | 已稳定返回，但具体算法口径未冻结 |
| `server_time_raw` | `int` | 服务端原始时间整数 | 例如 `15330719` |
| `server_time` | `datetime | None` | 解析后的服务端时间 | best-effort，非法时允许为 `None` |
| `last_price` | `float` | 最新价 | 浮点值 |
| `last_price_milli` | `int` | 最新价整数毫厘值 | 精确对拍用 |
| `open_price` | `float` | 今开 | 浮点值 |
| `open_price_milli` | `int` | 今开整数毫厘值 | 精确对拍用 |
| `high_price` | `float` | 最高 | 浮点值 |
| `high_price_milli` | `int` | 最高整数毫厘值 | 精确对拍用 |
| `low_price` | `float` | 最低 | 浮点值 |
| `low_price_milli` | `int` | 最低整数毫厘值 | 精确对拍用 |
| `last_close_price` | `float` | 昨收 / 前收价 | 浮点值 |
| `last_close_price_milli` | `int` | 昨收整数毫厘值 | 精确对拍用 |
| `total_hand` | `int` | 总手数 | 快照累计成交量字段 |
| `current_hand` | `int` | 现手数（现量） | 表示当前一笔 / 当前撮合显示的手数，当前按现手口径解释 |
| `amount` | `float` | 成交额 | 快照累计成交额 |
| `call_auction_amount` | `float | None` | 竞价额 | 当前实现按 `unknown_after_outer_disc_2 * 100` 解释；集合竞价阶段更有参考意义 |
| `call_auction_rate` | `float | None` | 竞价涨幅 | 当前实现按 `(open_price - last_close_price) / last_close_price * 100` 计算 |
| `inside_dish` | `int` | 内盘 | 当前实现按内盘口径保守解释，不承诺与第三方软件逐项一致 |
| `outer_disc` | `int` | 外盘 | 当前实现按外盘口径保守解释，不承诺与第三方软件逐项一致 |
| `buy_levels` | `list[QuoteLevel]` | 买盘五档 | `buy1 ~ buy5` |
| `sell_levels` | `list[QuoteLevel]` | 卖盘五档 | `sell1 ~ sell5` |
| `rate` | `float` | 涨速 | 当前实现按涨速口径保守解释，样本常见 `0.0` |

真实样例：

来源：`quote_single_sz000001.json`

- `exchange = sz`
- `code = 000001`
- `server_time_raw = 15330719`
- `server_time = 2026-03-07T15:33:07.190000+08:00`
- `last_price_milli = 10820 -> last_price = 10.82`
- `open_price_milli = 10780 -> open_price = 10.78`
- `high_price_milli = 10840 -> high_price = 10.84`
- `low_price_milli = 10770 -> low_price = 10.77`
- `last_close_price_milli = 10810 -> last_close_price = 10.81`
- `call_auction_amount = 123400.0`
- `call_auction_rate = -0.2775208140610546`
- `total_hand = 476576`
- `amount = 514733536.0`
- `inside_dish = 206012`
- `outer_disc = 270565`
- `buy1.price_milli = 10820 -> buy1.price = 10.82`
- `buy1.number = 6320`
- `sell1.price_milli = 10830 -> sell1.price = 10.83`
- `sell1.number = 6878`

补充说明：

- 如果 `server_time_raw` 不合法，`server_time` 会是 `None`；这时应优先保留和核对 `server_time_raw`。
- 快照协议里的盘口价差是围绕“当前价 / 最新价”展开的，不是围绕昨收展开。
- 当前快照字段约定是：`last_price` 表示最新价，`last_close_price` 表示昨收 / 前收价。
- `call_auction_amount` 和 `call_auction_rate` 是集合竞价场景字段；离开集合竞价阶段后不一定仍然具备同样解释力。

## 7. `CallAuctionItem` / `CallAuctionResponse`

### 7.1 `CallAuctionItem`

| 字段 | 类型 | 中文含义 | 说明 |
| --- | --- | --- | --- |
| `time` | `datetime` | 集合竞价记录时间 | Python 原生时间 |
| `price` | `float` | 竞价价格 | 浮点值 |
| `price_milli` | `int` | 竞价整数毫厘价格 | 精确对拍用 |
| `match` | `int` | 撮合量 | 已撮合数量 |
| `unmatched` | `int` | 未撮合量 | 剩余未撮合数量 |
| `flag` | `int` | 未撮合方向 | `1=买未撮合`，`-1=卖未撮合` |
| `raw_hex` | `str | None` | 单条记录原始十六进制 | 仅 `include_raw=True` 时返回 |

真实样例：

来源：`call_auction_sz000001.json`

- `time = 2026-03-07T09:15:00+08:00`
- `price_milli = 10810 -> price = 10.81`
- `match = 19`
- `unmatched = 24`
- `flag = -1 -> 卖未撮合`
- `raw_hex = 2b02c3f52c4113000000e8ffffff0000`

### 7.2 `CallAuctionResponse`

| 字段 | 类型 | 中文含义 | 说明 |
| --- | --- | --- | --- |
| `count` | `int` | 记录条数 | 与 `items` 数量一致 |
| `items` | `list[CallAuctionItem]` | 集合竞价明细 | 单条竞价记录列表 |
| `raw_frame_hex` | `str | None` | 原始 frame 十六进制 | 仅 `include_raw=True` 时返回 |
| `raw_payload_hex` | `str | None` | 原始 payload 十六进制 | 仅 `include_raw=True` 时返回 |

## 8. `GbbqItem` / `XdxrItem` / `EquityItem` / `FactorItem`

### 8.1 `GbbqItem`

| 字段 | 类型 | 中文含义 | 说明 |
| --- | --- | --- | --- |
| `code` | `str` | 完整代码 | 例如 `sz000001` |
| `time` | `datetime` | 生效时间 | Python 原生时间 |
| `category` | `int` | 事件类别编号 | 原始类别码 |
| `category_name` | `str | None` | 事件类别名称 | 例如 `除权除息`、`股本变化` |
| `c1` | `float` | 类别相关数值 1 | 含义依 `category` 而变 |
| `c2` | `float` | 类别相关数值 2 | 含义依 `category` 而变 |
| `c3` | `float` | 类别相关数值 3 | 含义依 `category` 而变 |
| `c4` | `float` | 类别相关数值 4 | 含义依 `category` 而变 |

真实样例：

来源：`gbbq_sz000001.json`

- `code = sz000001`
- `time = 1990-03-01T15:00:00+08:00`
- `category = 1`
- `category_name = 除权除息`
- `c1 = 0.0`
- `c2 = 3.559999942779541`
- `c3 = 0.0`
- `c4 = 1.0`

说明：

- `GbbqItem` 是最原始的公司行为 / 股本变迁记录。
- `c1 ~ c4` 的业务含义要结合 `category` 理解。

### 8.2 `XdxrItem`

`XdxrItem` 是把 `GbbqItem(category=除权除息)` 做了业务化解释后的结果。

| 字段 | 类型 | 中文含义 | 说明 |
| --- | --- | --- | --- |
| `code` | `str` | 完整代码 | 例如 `sz000001` |
| `time` | `datetime` | 除权除息时间 | Python 原生时间 |
| `fenhong` | `float` | 分红 | 现金分红相关值 |
| `peigujia` | `float` | 配股价 | 配股价格 |
| `songzhuangu` | `float` | 送转股 | 送股/转增相关值 |
| `peigu` | `float` | 配股比例 | 配股数量 / 比例 |

真实样例：

来源：`xdxr_sz000001.json`

- `time = 1990-03-01T15:00:00+08:00`
- `fenhong = 0.0`
- `peigujia = 3.56`
- `songzhuangu = 0.0`
- `peigu = 1.0`

对应关系（当前样例 `category=1`）：

- `c1 -> fenhong`
- `c2 -> peigujia`
- `c3 -> songzhuangu`
- `c4 -> peigu`

### 8.3 `EquityItem`

| 字段 | 类型 | 中文含义 | 说明 |
| --- | --- | --- | --- |
| `code` | `str` | 完整代码 | 例如 `sz000001` |
| `time` | `datetime` | 生效时间 | Python 原生时间 |
| `category` | `int` | 股本事件类别码 | 原始类别码 |
| `category_name` | `str | None` | 股本事件名称 | 例如 `股本变化` |
| `float_shares` | `int` | 流通股本 | 过滤后得到的业务字段 |
| `total_shares` | `int` | 总股本 | 过滤后得到的业务字段 |

真实样例：

来源：`equity_changes_sz000001.json`

- `time = 1991-04-03T15:00:00+08:00`
- `category = 5`
- `category_name = 股本变化`
- `float_shares = 26500000`
- `total_shares = 48500170`

### 8.4 `EquityResponse`

| 字段 | 类型 | 中文含义 | 说明 |
| --- | --- | --- | --- |
| `count` | `int` | 股本记录数 | 与 `items` 数量一致 |
| `items` | `list[EquityItem]` | 股本变化列表 | 已过滤后的业务记录 |

### 8.5 `FactorItem`

| 字段 | 类型 | 中文含义 | 说明 |
| --- | --- | --- | --- |
| `time` | `datetime` | 因子时间点 | Python 原生时间 |
| `last_close_price` | `float | None` | 当日参考前收价 | 浮点值 |
| `last_close_price_milli` | `int | None` | 当日参考前收原始整数价 | 精确对拍用 |
| `pre_last_close_price` | `float | None` | 上一参考前收价 | 用于因子衔接 |
| `pre_last_close_price_milli` | `int | None` | 上一参考前收原始整数价 | 精确对拍用 |
| `qfq_factor` | `float` | 前复权因子 | 用于前复权价格还原 |
| `hfq_factor` | `float` | 后复权因子 | 用于后复权价格还原 |

真实样例：

来源：`factors_sz000001.json`

- `time = 1991-04-03T15:00:00+08:00`
- `last_close_price = null`
- `pre_last_close_price = null`
- `qfq_factor = 0.0031807693670508697`
- `hfq_factor = 1.0`

### 8.6 `FactorResponse`

| 字段 | 类型 | 中文含义 | 说明 |
| --- | --- | --- | --- |
| `count` | `int` | 因子条数 | 与 `items` 数量一致 |
| `items` | `list[FactorItem]` | 因子列表 | 时间序列 |

## 9. 一眼能看懂的“原始值 -> 业务值”对照

下面这些是目前最适合人工核对的几组字段：

### 9.1 价格类

- `price_milli = 10820 -> price = 10.82`
- `last_price_milli = 10820 -> last_price = 10.82`
- `open_price_milli = 10780 -> open_price = 10.78`
- `buy1.price_milli = 10820 -> buy1.price = 10.82`

### 9.2 时间类

- `server_time_raw = 15330719 -> server_time = 2026-03-07T15:33:07.190000+08:00`
- `time = 2026-03-07T09:31:00+08:00` 已是最终 Python `datetime`
- `trading_date = 2026-03-07` 已是最终 Python `date`

### 9.3 方向 / 状态类

- `status = 0 -> side = buy`
- `status = 1 -> side = sell`
- `status = 2 -> side = neutral`
- `flag = -1 -> 卖未撮合`
- `flag = 1 -> 买未撮合`

### 9.4 公司行为类

- `GbbqItem(category=1, c2=3.56, c4=1.0)`
- 业务化后对应：`XdxrItem(peigujia=3.56, peigu=1.0)`

## 10. 当前最值得注意的边界

- `get_count()` / `get_codes*()` 返回的是代码表语义，不是“股票官方分类总数”。
- `SecurityCode.multiple`、`SecurityCode.decimal` 是协议参数，不建议业务层拿来当核心指标。
- `Quote.active1`、`active2`、`rate` 已稳定返回，但业务中文口径尚未最终冻结。
- `Quote.server_time` 是 best-effort 解析值；若原始整数不合法，它允许为 `None`。
- `TradeItem.order_count` 在实时逐笔里更常见，历史逐笔通常为 `None`。
- `KlineItem.up_count` / `down_count` 更适合在指数型 K 线里理解；普通股票 K 线常为 `None`。
