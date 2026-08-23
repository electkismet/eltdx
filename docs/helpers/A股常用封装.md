---
hide:
  - navigation
---

[← 返回接口目录](../index.md){ .interface-detail-back }

# A 股常用封装

这些方法都是 Python Helpers 组合，不是新的协议号。底层原始数据仍可通过对应的 `client.*` 原生入口取得。

| 功能 | 调用 | 数据来源 |
| --- | --- | --- |
| 最新股票列表 | `client.helpers.latest_stock_list()` | `0x044d` 代码表 |
| 最新 ST 列表 | `client.helpers.latest_st()` | `0x044d` 名称规则 |
| 最新停牌列表 | `client.helpers.latest_suspended()` | `0x044d` + `0x053e` 状态位 `0x20` |
| 每日股本（盘前） | `client.helpers.daily_share_capital(codes=None)` | `0x0010` 财务快照 + `0x06b9` 统计资源 |
| 每日涨跌停价 | `client.helpers.daily_price_limits(codes=None)` | `0x054c` 昨收 + 板块/ST规则 |
| 实时榜单 | `client.helpers.realtime_rank(sort_by="涨幅")` | `0x054b` 分类行情 |
| 买卖力道 | `client.helpers.buy_sell_strength(code)` | `0x051b` 分时副图 |
| 成交对比 | `client.helpers.volume_comparison(code)` | `0x051b` 分时副图 |
| 连板天梯 | `client.helpers.limit_ladder()` | 实时快照 + `0x06b9` |
| 题材强度排行 | `client.helpers.theme_strength_rank()` | 连板天梯 + F10 题材 |

## 接口说明

<a id="latest-stock-list"></a>
### 最新股票列表

调用 `client.helpers.latest_stock_list(market=None)` 获取当前代码表中的沪深北 A 股。返回 `list[SecurityCode]`，每项包含 `full_code`、`name`、`market` 和证券分类等代码表字段。

<a id="latest-st"></a>
### 最新 ST 列表

调用 `client.helpers.latest_st(market=None)` 按证券名称筛选 `ST`、`*ST`、`SST` 和 `S*ST` 股票。返回 `list[SecurityCode]`，数据来自当前代码表，不额外请求历史标记。

<a id="latest-suspended"></a>
### 最新停牌列表

调用 `client.helpers.latest_suspended(market=None)` 先读取代码表，再通过 `0x053e` 的交易状态位 `0x20` 筛选停牌证券。返回 `list[SecurityCode]`，表示当前状态，不表示历史停牌日期。

<a id="daily-share-capital"></a>
### 每日股本（盘前）

调用 `client.helpers.daily_share_capital(codes=None)` 合并 `0x0010` 财务快照和 `0x06b9` 统计资源，返回 `DailyShareCapitalTable`。每行包含证券代码、总股本、流通股本和自由流通股本等字段；不传 `codes` 时按全市场扫描。

<a id="daily-price-limits"></a>
### 每日涨跌停价

调用 `client.helpers.daily_price_limits(codes=None)` 使用实时昨收、市场板块、ST 和新股规则计算标准化涨跌停价，返回 `DailyPriceLimitTable`。每行包含 `full_code`、`pre_close`、`limit_up_price`、`limit_down_price` 和 `limit_rule`。

<a id="realtime-rank"></a>
### 实时榜单

调用 `client.helpers.realtime_rank(category="沪深A股", sort_by="涨幅", count=80, ascending=False)` 分页读取 `0x054b` 分类行情并整理成 `RealtimeRankTable`。支持按涨幅、成交额、换手率等字段排序，返回排名、代码、名称、涨幅、成交量和成交额。

<a id="buy-sell-strength"></a>
### 买卖力道

调用 `client.helpers.buy_sell_strength(code)` 获取 `0x051b` 买卖力道分时副图，返回 `MinuteAuxSeries`。序列按分钟提供买方、卖方或对应副图数值，具体字段以返回对象的 `rows` 为准。

<a id="volume-comparison"></a>
### 成交对比

调用 `client.helpers.volume_comparison(code)` 获取 `0x051b` 成交对比分时副图，返回 `MinuteAuxSeries`。序列用于比较当前交易日与前一交易日的累计成交量，不是逐笔成交明细。

<a id="limit-ladder"></a>
### 连板天梯

调用 `client.helpers.limit_ladder(codes=None)` 从短线指标中筛选封板或触板证券，返回 `LimitLadderTable`，按连板级别和封单金额整理。默认扫描全部 A 股，建议批量场景传入候选代码。

<a id="theme-strength-rank"></a>
### 题材强度排行

调用 `client.helpers.theme_strength_rank(codes=None)` 读取候选股票的 F10 题材并聚合为 `ThemeStrengthTable`，返回题材名称、涨停数量、最高板、连板数和封单金额等字段。适合盘后或小范围候选，不建议高频刷新。

```python
with TdxClient(timeout=3) as client:
    st_rows = client.helpers.latest_st()
    suspended = client.helpers.latest_suspended()
    limits = client.helpers.daily_price_limits(["sz000001", "sh600000"])
    rank = client.helpers.realtime_rank(sort_by="涨幅", count=20)
```

### 说明

- `daily_price_limits()` 是按昨收、市场板块和 ST/新股规则计算的业务结果，不冒充服务端独立的“每日价位”协议。
- `latest_suspended()` 使用旧版批量行情的公开交易状态位，只返回当前状态，不提供历史停牌日期表。
- `limit_ladder()` 默认扫描全部 A 股，数据量大时建议先传入候选代码列表。
- `theme_strength_rank()` 会读取候选股票的 F10 题材，适合盘后或小范围候选，不建议在每秒刷新循环中调用。

??? return-sample "真实返回 JSON · A 股常用封装（真实采样节选）"

    ```json
    {
      "latest_stock_list": [
        {"full_code": "sz000001", "name": "平安银行", "category": "a_share"}
      ],
      "latest_st": [],
      "latest_suspended": [],
      "daily_price_limits": {
        "full_code": "sz000001",
        "pre_close": 11.2,
        "limit_up_price": 12.32,
        "limit_down_price": 10.08,
        "limit_rule": "main_10pct"
      },
      "realtime_rank": {
        "rank": 1,
        "full_code": "sz000001",
        "change_pct": 3.21,
        "amount": 1280000000.0
      }
    }
    ```
