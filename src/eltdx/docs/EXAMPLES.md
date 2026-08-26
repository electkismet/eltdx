# 使用示例

这些示例都基于真实 `7709` 行情主站。代码建议带市场前缀，例如 `sz000001`、`sh600000`、`bj920001`。

## 快照

```python
from eltdx import TdxClient

with TdxClient(timeout=3) as client:
    quotes = client.helpers.full_quotes(["sz000001", "sh600000"])

for item in quotes:
    print(item.full_code, item.last_price, item.change_pct, item.total_hand)
```

## 代码表

```python
from eltdx import TdxClient

with TdxClient(timeout=3) as client:
    print(client.codes.count("sz"))
    print(client.codes.list("sz", start=0, limit=5))
    print(client.codes.all_a_shares()[:10])
```

## K 线和复权 K 线

```python
from eltdx import TdxClient

with TdxClient(timeout=3) as client:
    day = client.bars.get("sz000001", period="day", count=5)
    history = client.bars.get("sz000001", period="day", all_pages=True)
    qfq = client.bars.get("sz000001", period="day", adjust="qfq", count=5)
    hfq = client.bars.get("sz000001", period="day", adjust="hfq", count=5)

print(day.bars[-1].time, day.bars[-1].close)
print(qfq.adjust_mode, qfq.bars[-1].close)
print(hfq.adjust_mode, hfq.bars[-1].close)
```

## 分时

```python
from eltdx import TdxClient

with TdxClient(timeout=3) as client:
    today = client.minutes.today("sz000001")
    history = client.minutes.history("sz000001", "2026-05-20")

print(today.trading_date, today.count)
print(history.trading_date, history.points[-1].price)
```

## 成交明细和 09:25 竞价

```python
from eltdx import TdxClient

with TdxClient(timeout=3) as client:
    ticks = client.trades.today("sz000001", count=20)
    auction = client.trades.opening_match_history("sz000001", "2026-05-20")

print(ticks.count, ticks.ticks[0].time_label, ticks.ticks[0].price)
print(auction is not None, auction.price if auction else None, auction.volume if auction else None)
```

## 股本变迁和本地复权系数

```python
from eltdx import TdxClient

with TdxClient(timeout=3) as client:
    gbbq = client.corporate.capital_changes("sz000001")
    factors = client.corporate.adjustment_factors("sz000001")

print(gbbq.count)
print(gbbq.records[0].category_name)
print(factors.count, factors.items[0].qfq_scale, factors.items[0].qfq_offset)
```

`adjustment_factors()` 返回每个除权事件日期的 `scale + offset`，可按 K 线日期应用到本地不复权 OHLC。直接获取复权 K 线时使用 `client.bars.get(..., adjust="qfq" / "hfq")`；完整的本地应用示例见 [本地复权系数](methods/7709-本地复权系数.md)。

## 主站测速和连接池

```python
from eltdx import TdxClient

with TdxClient.from_hosts(pool_size=2, probe_hosts=True, timeout=3) as client:
    print(client.transport.hosts[:3])
    print(client.helpers.full_quotes("sz000001")[0].last_price)
```

## JSON 输出

```python
from eltdx import TdxClient, to_json

with TdxClient(timeout=3) as client:
    quotes = client.helpers.full_quotes(["sz000001", "sh600000"])

print(to_json(quotes, indent=2))
```

## 常用问题

```python
from eltdx import TdxClient

with TdxClient(timeout=3) as client:
    profiles = client.helpers.stock_profile_table(["sz000001", "sh600000"])
    topics = client.helpers.stock_topics("000034")
    stocks = client.helpers.topic_stocks("000034", topic_name="存储芯片")
    auction = client.helpers.auction_data("sz000001", "2026-05-20")

print(profiles.rows[0].name, profiles.rows[0].last_price)
print(topics.topics[:3])
print(stocks.rows[:10])
print(auction.open_price, auction.open_change_pct, auction.open_amount)
```

## 协议排查

```python
from eltdx import TdxClient

with TdxClient(timeout=3) as client:
    minute = client.minutes.today("sz000001", include_raw=True)

print(minute.raw_payload.hex())
print(minute.points[0].record_hex)
```
