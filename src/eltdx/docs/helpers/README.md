# 常用问题

这一页按使用问题组织。每个问题链接到单独文档，里面包含调用方法、参数、返回字段和示例。

## 股票和行情

- [想拿一个或一批证券的完整行情和五档盘口怎么办？](完整行情.md)
- [A 股常用封装总览](A股常用封装.md)
- [最新股票列表](最新股票列表.md) · [最新 ST 列表](最新ST列表.md) · [最新停牌列表](最新停牌列表.md)
- [每日股本（盘前）](每日股本.md) · [每日涨跌停价](每日涨跌停价.md) · [实时榜单](实时榜单.md)
- [买卖力道](买卖力道.md) · [成交对比](成交对比.md) · [连板天梯](连板天梯.md) · [题材强度排行](题材强度排行.md)
- [想拿某个或某些股票的表头信息怎么办？](股票信息汇总.md)
- [想拿流通市值Z、开盘量比、普通流通股本、封单和几天几板怎么办？](短线指标.md)

## 题材和概念板块

- [想查询某个股票都有哪些概念板块怎么办？](个股概念板块.md)
- [想查询某个概念板块都有哪些股票怎么办？](概念板块成分股.md)

## 竞价

- [想拿集合竞价数据怎么办？](竞价数据.md)

## 快速示例

```python
from eltdx import TdxClient

with TdxClient(timeout=3) as client:
    table = client.helpers.stock_profile_table(["sz000001", "sh600000"])
    shortline = client.helpers.shortline_indicators(["sz000001", "sh600000"])
    topics = client.helpers.stock_topics("000034")
    stocks = client.helpers.topic_stocks("000034", topic_name="存储芯片")
    auction = client.helpers.auction_data("sz000001", "2026-05-20")

print(table.rows[0])
print(shortline.rows[0])
print(topics.topics[:3])
print(stocks.rows[:10])
print(auction.open_price, auction.open_change_pct, auction.open_amount)
```

## 返回模型

返回模型是 dataclass，可以直接访问字段，也可以用 `to_jsonable()` / `to_json()` 转成字典或 JSON。

```python
from eltdx import to_jsonable

data = to_jsonable(table)
```
