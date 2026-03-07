# 使用示例

这份文档放的是可以直接复制、改一改就能用的代码。

如果你不想先通读完整 API 文档，先从这页开始也完全没问题。

配套阅读：

- API 用法：[`API_REFERENCE.md`](./API_REFERENCE.md)
- 字段说明：[`FIELD_REFERENCE.md`](./FIELD_REFERENCE.md)
- 调试指南：[`DEBUG_GUIDE.md`](./DEBUG_GUIDE.md)

## 1. 一次性抓取：推荐用 `with`

适合脚本、导出任务、临时核对。

```python
from eltdx import TdxClient

with TdxClient() as client:
    quote = client.get_quote("sz000001")[0]
    print(quote.code, quote.last_price, quote.server_time)
```

适合场景：

- 拉一次数据就结束
- 不想忘记关连接
- 希望代码更简洁

## 2. 长连接场景：手动 `connect()` / `close()`

适合你希望把连接保持一段时间的场景。

```python
from eltdx import TdxClient

client = TdxClient(pool_size=2, timeout=8.0)
client.connect()
try:
    quotes = client.get_quote(["sz000001", "sh600000"])
    print([(item.code, item.last_price) for item in quotes])
finally:
    client.close()
```

说明：

- `with` 不是强制的
- 长连接场景可以手动持有 client
- 默认 `pool_size=2`，也就是两条长连接

## 3. 行情快照 `get_quote()`

### 3.1 单个代码

```python
from eltdx import TdxClient

with TdxClient() as client:
    quote = client.get_quote("sz000001")[0]
    print(quote.code)
    print(quote.last_price)
    print(quote.last_close_price)
    print(quote.current_hand)
    print(quote.call_auction_amount, quote.call_auction_rate)
    print(quote.open_price, quote.high_price, quote.low_price)
    print(quote.total_hand, quote.amount)
```

常看字段：

- `code`：代码
- `last_price`：最新价
- `open_price`：今开
- `high_price`：最高
- `low_price`：最低
- `last_close_price`：昨收 / 前收价
- `current_hand`：现手数（现量）
- `call_auction_amount`：竞价额
- `call_auction_rate`：竞价涨幅
- `total_hand`：总手
- `amount`：成交额
- `buy_levels` / `sell_levels`：五档盘口
- `server_time`：服务端时间，已转成 Python `datetime`

### 3.2 多个代码

```python
from eltdx import TdxClient

codes = ["sz000001", "sh600000", "sh601398", "sz159915"]

with TdxClient() as client:
    quotes = client.get_quote(codes)
    for item in quotes:
        print(item.code, item.last_price, item.total_hand)
```

### 3.3 大批量代码

```python
from eltdx import TdxClient

with TdxClient() as client:
    codes = client.get_a_share_codes_all()[:5000]
    quotes = client.get_quote(codes)
    print(len(quotes))
```

说明：

- `get_quote()` 已内置自动分批
- 默认每批 `80` 个代码
- 默认会分发到两条连接上执行
- 一般不需要你自己手动切片

### 3.4 字段对照示例

真实样例常见对照：

- `server_time_raw = 15330719`
- `server_time = 2026-03-07T15:33:07.190000+08:00`
- `last_price_milli = 10820 -> last_price = 10.82`
- `open_price_milli = 10780 -> open_price = 10.78`
- `last_close_price_milli = 10810 -> last_close_price = 10.81`
- `total_hand = 476576`
- `inside_dish = 206012`
- `outer_disc = 270565`

如果你想知道这些字段更详细的中文含义，看：[`FIELD_REFERENCE.md`](./FIELD_REFERENCE.md)

## 4. 市场与代码表

### 4.1 分页读取代码表

```python
from eltdx import TdxClient

with TdxClient() as client:
    page = client.get_codes("sh", start=0, limit=50)
    print(page.exchange, page.total, page.count)
    print(page.items[0].full_code, page.items[0].name)
```

`CodePage` 常看字段：

- `exchange`：市场，如 `sh` / `sz` / `bj`
- `start`：起始偏移
- `count`：本页条数
- `total`：总条数
- `items`：`SecurityCode` 列表

### 4.2 获取完整代码表

```python
from eltdx import TdxClient

with TdxClient() as client:
    all_codes = client.get_codes_all("sz")
    print(len(all_codes))
    print(all_codes[0].full_code, all_codes[0].name)
```

### 4.3 获取过滤后的清单 helper

```python
from eltdx import TdxClient

with TdxClient() as client:
    a_shares = client.get_a_share_codes_all()
    etfs = client.get_etf_codes_all()
    indexes = client.get_index_codes_all()
    print(len(a_shares), len(etfs), len(indexes))
```

注意：

- 如果你想拿 A 股清单，优先用 `get_a_share_codes_all()`
- 不要把 `get_count()` 当成 A 股总数接口

## 5. 分时 `get_minute()` / `get_history_minute()`

### 5.1 实时分时

```python
from eltdx import TdxClient

with TdxClient() as client:
    minute = client.get_minute("sz000001")
    print(minute.trading_date)
    print(minute.count)
    print(minute.items[0].time, minute.items[0].price, minute.items[0].volume)
```

### 5.2 历史分时

```python
from eltdx import TdxClient

with TdxClient() as client:
    minute = client.get_history_minute("sz000001", "2026-03-06")
    print(minute.trading_date)
    print(minute.items[-1].time, minute.items[-1].price)
```

说明：

- `get_minute(code)`：走实时分时路径
- `get_minute(code, date)`：走历史分时路径
- `get_history_minute(code, date)`：历史分时兼容别名

### 5.3 分时字段样例

- `time = 2026-03-07T09:31:00+08:00`
- `price_milli = 10820 -> price = 10.82`
- `volume = 16557`
- `trading_date = 2026-03-07`

## 6. 逐笔 `get_trades()` / `get_trades_all()`

### 6.1 实时逐笔分页

```python
from eltdx import TdxClient

with TdxClient() as client:
    page = client.get_trades("sz000001", start=0, count=100)
    first = page.items[0]
    print(first.time, first.price, first.volume, first.status, first.side)
```

### 6.2 历史逐笔分页

```python
from eltdx import TdxClient

with TdxClient() as client:
    page = client.get_trades("sz000001", "2026-03-06", start=0, count=200)
    print(page.trading_date)
    print(page.items[0].time, page.items[0].price, page.items[0].side)
```

### 6.3 全量逐笔

```python
from eltdx import TdxClient

with TdxClient() as client:
    trades = client.get_trades_all("sz000001", "2026-03-06")
    print(trades.count)
    print(trades.items[0].time, trades.items[-1].time)
```

### 6.4 逐笔字段样例

- `time = 2026-03-07T14:54:00+08:00`
- `price_milli = 10830 -> price = 10.83`
- `volume = 63`
- `status = 0 -> side = buy`
- `order_count = 7`

## 7. K 线 `get_kline()` / `get_kline_all()` / 复权 K 线

### 7.1 普通股票 K 线

```python
from eltdx import TdxClient

with TdxClient() as client:
    kline = client.get_kline("sz000001", "day", count=10)
    print(kline.items[0].time, kline.items[0].close_price)
```

### 7.2 指数 K 线：建议显式传 `kind="index"`

```python
from eltdx import TdxClient

with TdxClient() as client:
    index_kline = client.get_kline("sh000001", "day", kind="index", count=10)
    item = index_kline.items[0]
    print(item.time, item.close_price, item.up_count, item.down_count)
```

### 7.3 全量 K 线

```python
from eltdx import TdxClient

with TdxClient() as client:
    all_kline = client.get_kline_all("sz000001", "day")
    print(all_kline.count)
```

### 7.4 复权 K 线

```python
from eltdx import TdxClient

with TdxClient() as client:
    qfq = client.get_adjusted_kline("day", "sz000001", adjust="qfq", count=10)
    hfq = client.get_adjusted_kline("day", "sz000001", adjust="hfq", count=10)
    print(qfq.items[-1].close_price)
    print(hfq.items[-1].close_price)
```

### 7.5 K 线字段样例

股票日线样例：

- `time = 2026-02-13T15:00:00+08:00`
- `open_price_milli = 10960 -> open_price = 10.96`
- `amount_milli = 607501376000 -> amount = 607501376.0`

指数日线样例：

- `time = 2026-02-13T15:00:00+08:00`
- `open_price = 4115.92`
- `close_price = 4082.07`
- `volume = 500799500`
- `up_count = 619`
- `down_count = 1677`

## 8. 集合竞价 `get_call_auction()`

```python
from eltdx import TdxClient

with TdxClient() as client:
    resp = client.get_call_auction("sz000001", include_raw=True)
    print(resp.count)
    print(resp.raw_frame_hex)
    print(resp.raw_payload_hex)
    first = resp.items[0]
    print(first.time, first.price, first.match, first.unmatched, first.flag)
    print(first.raw_hex)
```

字段样例：

- `raw_hex = 2b02c3f52c4113000000e8ffffff0000`
- `price_milli = 10810 -> price = 10.81`
- `match = 19`
- `unmatched = 24`
- `flag = -1 -> 卖未撮合`

适合场景：

- 看集合竞价序列
- 做原始记录比对
- 验证 `include_raw=True` 的调试输出

## 9. 公司行为 / 股本 / 复权因子

### 9.1 GBBQ 原始记录

```python
from eltdx import TdxClient

with TdxClient() as client:
    gbbq = client.get_gbbq("sz000001")
    print(gbbq.count)
    print(gbbq.items[0].time, gbbq.items[0].category_name)
```

### 9.2 XDXR 除权除息记录

```python
from eltdx import TdxClient

with TdxClient() as client:
    items = client.get_xdxr("sz000001")
    first = items[0]
    print(first.time, first.fenhong, first.peigujia, first.peigu)
```

### 9.3 股本记录

```python
from eltdx import TdxClient

with TdxClient() as client:
    equity = client.get_equity("sz000001")
    if equity is not None:
        print(equity.time, equity.float_shares, equity.total_shares)
```

### 9.4 复权因子

```python
from eltdx import TdxClient

with TdxClient() as client:
    factors = client.get_factors("sz000001")
    first = factors.items[0]
    print(first.time, first.qfq_factor, first.hfq_factor)
```

字段样例：

- `GbbqItem.category_name = 除权除息`
- `XdxrItem.peigujia = 3.56`
- `EquityItem.float_shares = 26500000`
- `EquityItem.total_shares = 48500170`
- `FactorItem.qfq_factor = 0.0031807693670508697`
- `FactorItem.hfq_factor = 1.0`

## 10. 自定义服务器地址

### 10.1 单个地址

```python
from eltdx import TdxClient

with TdxClient(host="124.71.187.122:7709") as client:
    print(client.get_quote("sz000001")[0].last_price)
```

### 10.2 多个地址

```python
from eltdx import TdxClient

hosts = [
    "124.71.187.122:7709",
    "122.51.120.217:7709",
    "59.173.18.140:7709",
]

with TdxClient(hosts=hosts, timeout=8.0) as client:
    print(client.get_minute("sz000001").count)
```

说明：

- 你可以自己传 `host=` 或 `hosts=`
- 不传时会回退到库内默认地址列表

## 11. 原始值调试与比对

如果你要做协议核对，最实用的方式是打开 `include_raw=True`。

```python
from eltdx import TdxClient

with TdxClient() as client:
    minute = client.get_minute("sz000001", include_raw=True)
    print(minute.raw_frame_hex)
    print(minute.raw_payload_hex)
    print(minute.items[0].time, minute.items[0].price_milli, minute.items[0].price)
```

推荐核对顺序：

1. 先看 `raw_payload_hex`
2. 再看 `price_milli` / `amount_milli` / `server_time_raw` 这类原始整数值
3. 最后看 `price` / `amount` / `server_time` 这类解析后的业务值

适合核对的字段：

- 价格字段
- 时间字段
- 成交量字段
- 买卖方向字段
- 公司行为字段

## 12. 推荐 smoke 命令

```bash
python scripts/smoke/smoke_codes.py
python scripts/smoke/smoke_minute.py
python scripts/smoke/smoke_trade.py
python scripts/smoke/smoke_kline.py
python scripts/smoke/smoke_call_auction.py
python scripts/smoke/smoke_live_all.py
python scripts/validation/export_live_validation.py
```

用途：

- 前几个 smoke：快速看单个模块是否正常
- `scripts/smoke/smoke_live_all.py`：总链路快速检查
- `scripts/validation/export_live_validation.py`：导出一整套联网验证样本
