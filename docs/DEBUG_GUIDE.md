# 调试指南

如果你遇到下面这些情况，可以先看这页：

- 接口没有按预期返回数据
- 你想看原始十六进制数据
- 你怀疑是服务器地址的问题
- 你不确定某个返回值是不是解析错了

## 建议先这样排查

### 第一步：先跑最简单的调用

```python
from eltdx import TdxClient

with TdxClient() as client:
    quote = client.get_quote("sz000001")[0]
    print(quote.code, quote.last_price, quote.server_time)
```

如果这一步都不通，先别急着看复杂字段，先确认连接和地址是不是正常。

### 第二步：自己指定服务器地址试一下

```python
from eltdx import TdxClient

with TdxClient(host="124.71.187.122:7709") as client:
    quote = client.get_quote("sz000001")[0]
    print(quote.last_price)
```

如果你手头有多个地址，也可以这样传：

```python
from eltdx import TdxClient

hosts = [
    "124.71.187.122:7709",
    "122.51.120.217:7709",
    "111.229.247.189:7709",
]

with TdxClient(hosts=hosts, pool_size=2) as client:
    quote = client.get_quote("sz000001")[0]
    print(quote.last_price)
```

### 第三步：打开 `include_raw=True`

很多接口都支持 `include_raw=True`。如果你想知道“原始返回是什么、我们又解析成了什么”，这是最直接的办法。

支持查看 raw 数据的常用方法：

| 方法 | 可查看字段 |
| --- | --- |
| `get_minute()` | `raw_frame_hex`, `raw_payload_hex` |
| `get_history_minute()` | `raw_frame_hex`, `raw_payload_hex` |
| `get_trades()` | `raw_frame_hex`, `raw_payload_hex` |
| `get_history_trade()` | `raw_frame_hex`, `raw_payload_hex` |
| `get_kline()` | `raw_frame_hex`, `raw_payload_hex` |
| `get_adjusted_kline()` | `raw_frame_hex`, `raw_payload_hex` |
| `get_call_auction()` | `raw_frame_hex`, `raw_payload_hex`, `items[].raw_hex` |
| `get_gbbq()` | `raw_frame_hex`, `raw_payload_hex` |

## raw 数据怎么查看

### 看分时

```python
from eltdx import TdxClient

with TdxClient() as client:
    minute = client.get_minute("sz000001", include_raw=True)
    print(minute.raw_frame_hex)
    print(minute.raw_payload_hex)
    print(minute.items[0].time, minute.items[0].price_milli, minute.items[0].price)
```

### 看 K 线

```python
from eltdx import TdxClient

with TdxClient() as client:
    response = client.get_kline("sz000001", "day", count=10, include_raw=True)
    print(response.raw_frame_hex)
    print(response.raw_payload_hex)
    print(response.items[0].open_price_milli, response.items[0].open_price)
```

### 看集合竞价

```python
from eltdx import TdxClient

with TdxClient() as client:
    response = client.get_call_auction("sz000001", include_raw=True)
    first = response.items[0]
    print(first.raw_hex)
    print(first.price_milli, first.price)
    print(first.match, first.unmatched, first.flag)
```

## 常见问题

### 为什么 `get_count("sh")` 看起来特别大？

因为它表示的是代码表条目数，不是股票总数。

如果你想看更接近股票口径的数量，优先用：

- `get_stock_count("sh")`
- `get_a_share_count("sh")`

### 为什么 `get_codes()` / `get_codes_all()` 看起来不全是股票？

因为这里面会混有股票、指数、板块分类项、ETF、基金、债券回购等条目。

如果你想直接拿一组更常用的代码去拉行情，优先用：

- `get_stock_codes_all()`
- `get_a_share_codes_all()`
- `get_etf_codes_all()`
- `get_index_codes_all()`

### `sh900xxx` 和 `sz200xxx` 是什么？

它们通常对应 B 股代码，不是 A 股代码。

如果你只想拿 A 股，优先用：

- `get_a_share_codes_all()`
- `get_a_share_count()`

### 为什么 `Quote.server_time` 有时是 `None`？

因为 `server_time` 是尽量恢复出来的时间字段：

- 原始值在 `server_time_raw`
- 如果原始值没法转成合法时间，`server_time` 允许是 `None`
- 排查时可以同时看 `server_time_raw` 和 `server_time`

### `with TdxClient()` 是不是必须用？

不是。

- 只是临时拉一次数据：推荐 `with`
- 需要长期保持连接：手动 `connect()` / `close()` 也可以
